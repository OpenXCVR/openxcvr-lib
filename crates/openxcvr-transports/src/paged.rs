//! Memory-window selection, independent of USB or a particular bus controller.
use crate::{
    bus::{Address7, I2cBus},
    remaining, Error, ErrorKind, Result,
};
use openxcvr_core::{
    error::{IoError, ModuleError},
    transport::Transport,
};
use std::time::{Duration, Instant};

/// Which spec's memory map a [`Memory`] selects pages and banks for, plus
/// the facts the layout itself owns. Selection bytes are written only where
/// the layout defines them.
#[derive(Debug, Clone, Copy)]
pub enum Layout {
    /// CMIS: lower memory at 0x50, bank selector at byte 126. `flat` marks a
    /// module without paging/banking (only page 0 / bank 0 reachable).
    Cmis {
        /// True when the module has no page/bank selectors.
        flat: bool,
    },
    /// SFF-8636: lower memory at 0x50, page selector only; byte 126 (the
    /// CMIS bank selector) is reserved and never written.
    Sff8636 {
        /// True when the module has no page selector.
        flat: bool,
    },
    /// Optional A2 paging must be supplied from the module's capabilities.
    Sff8472 {
        /// True when the module supports the optional A2h paged diagnostics
        /// (from the module's capabilities, never guessed).
        paged_diagnostics: bool,
    },
}

/// Timing parameters for one module's accesses, as established by the caller
/// for this module and operation. The library performs no timing measurement
/// of its own; these are trusted caller values.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Physical write boundary, as established for this module/operation.
    pub write_block: usize,
    /// Settle time after a data write before the next access.
    pub write_settle: Duration,
    /// Settle time after a selector write before the dependent access.
    pub select_settle: Duration,
    /// Per-operation deadline; every transfer of one operation derives from it.
    pub timeout: Duration,
}

/// Page- and bank-aware access to one module's 256-byte memory window.
///
/// # Contract, shared by every [`I2cBus`] backend
///
/// - `read_memory`/`write_memory` reach one 128-byte window and require the
///   canonical page and bank for lower memory; `read_window`/`write_window`
///   reach all 256 bytes, selecting first and then crossing the window
///   boundary without re-selecting between chunks. Address, page, bank and
///   range are checked before anything reaches the bus.
/// - Selector bytes are written only where the layout defines them: the CMIS
///   bank selector (126) is never written for SFF-8636 or SFF-8472, whose byte
///   126 is reserved.
/// - The selection is published before any access that depends on it, and a
///   successful selection is remembered: a repeated access to the same
///   address/page/bank does not rewrite the selector bytes.
/// - Every write boundary is marked: a failed select write, a data write issued
///   after earlier data already applied, and a failure after a settle all carry
///   `write_may_have_applied`, and nothing is ever retried.
/// - A failed select, a disconnected data operation, and any write whose range
///   covers the selector bytes drop the remembered selection so the next
///   access re-selects rather than trusting an unknown page.
/// - A read publishes nothing on failure: `out` is unchanged unless the whole
///   transfer succeeded.
pub struct Memory<B> {
    bus: B,
    layout: Layout,
    timing: Timing,
    /// The selection last confirmed on the device, or `None` when the current
    /// page and bank are unknown and must be re-published.
    selection: Option<(Address7, u8, u8)>,
}

impl<B: I2cBus> Memory<B> {
    /// Wrap a bus with one module's layout and timing. Validates the supplied
    /// timing and the bus's limits up front (fail closed with
    /// [`ErrorKind::Invalid`] before anything reaches the device) and starts
    /// with no remembered selection.
    pub fn new(bus: B, layout: Layout, timing: Timing) -> Result<Self> {
        let limits = bus.limits();
        if timing.write_block == 0
            || timing.write_block > 128
            || timing.timeout.is_zero()
            || Instant::now().checked_add(timing.timeout).is_none()
            || limits.write < 2
            || limits.read == 0
            || limits.prefix < 1
        {
            return Err(Error::new(
                ErrorKind::Invalid,
                "invalid memory timing or bus limits",
            ));
        }
        Ok(Self {
            bus,
            layout,
            timing,
            selection: None,
        })
    }

    /// Release the bus, dropping the remembered selection.
    pub fn into_inner(self) -> B {
        self.bus
    }

    fn forget_on_continuity_error(&mut self, error: Error) -> Error {
        if error.kind == ErrorKind::Disconnected {
            self.selection = None;
        }
        error
    }

    fn address(&self, address: u8, page: u8, bank: u8) -> Result<Address7> {
        let address = Address7::from_legacy(address)?;
        let valid = match self.layout {
            Layout::Cmis { flat } => {
                address.get() == 0x50
                    && (!flat || (page == 0 && bank == 0))
                    && (page >= 0x10 || bank == 0)
            }
            Layout::Sff8636 { flat } => address.get() == 0x50 && bank == 0 && (!flat || page == 0),
            Layout::Sff8472 { paged_diagnostics } => {
                bank == 0
                    && match address.get() {
                        0x50 => page == 0,
                        0x51 => page == 0 || paged_diagnostics,
                        _ => false,
                    }
            }
        };
        if !valid {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "address/page/bank unsupported by supplied layout",
            ));
        }
        Ok(address)
    }

    fn validate(
        &self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        length: usize,
        window: bool,
    ) -> Result<Address7> {
        if length == 0 || length > 256 - usize::from(offset) {
            return Err(Error::new(
                ErrorKind::Invalid,
                "empty or overflowing memory range",
            ));
        }
        let address = self.address(address, page, bank)?;
        let has_lower = !matches!(self.layout, Layout::Sff8472 { .. }) || address.get() == 0x51;
        if !window
            && has_lower
            && offset < 128
            && (page != 0 || bank != 0 || length > 128 - usize::from(offset))
        {
            return Err(Error::new(
                ErrorKind::Invalid,
                "lower memory requires canonical page/bank and a single window",
            ));
        }
        Ok(address)
    }

    fn settle(duration: Duration, deadline: Instant) -> Result<()> {
        let left = remaining(deadline)?;
        if duration >= left {
            return Err(Error::new(
                ErrorKind::Timeout,
                "insufficient time to settle",
            ));
        }
        std::thread::sleep(duration);
        remaining(deadline).map(|_| ())
    }

    /// Publishes `page`/`bank` on the device, or confirms they are already set.
    ///
    /// A select write is a write boundary in its own right: a failure is
    /// returned marked `write_may_have_applied` and drops the confirmed
    /// selection, so the next access re-selects instead of trusting a page the
    /// failed write may have changed.
    fn select(&mut self, address: Address7, page: u8, bank: u8, deadline: Instant) -> Result<()> {
        if self.selection == Some((address, page, bank)) {
            return Ok(());
        }
        match self.write_selection(address, page, bank, deadline) {
            Ok(()) => {
                self.selection = Some((address, page, bank));
                Ok(())
            }
            Err(error) => {
                self.selection = None;
                Err(error.uncertain())
            }
        }
    }

    fn write_selection(
        &mut self,
        address: Address7,
        page: u8,
        bank: u8,
        deadline: Instant,
    ) -> Result<()> {
        // These are protocol selectors, not decoded module fields. Never write
        // the CMIS bank selector into SFF-8636's reserved byte 126.
        let banked = matches!(self.layout, Layout::Cmis { flat: false });
        let paged = banked
            || matches!(self.layout, Layout::Sff8636 { flat: false })
            || (matches!(
                self.layout,
                Layout::Sff8472 {
                    paged_diagnostics: true
                }
            ) && address.get() == 0x51);
        if banked {
            self.bus.write(address, &[126, bank], deadline)?;
        }
        if paged {
            self.bus.write(address, &[127, page], deadline)?;
            Self::settle(self.timing.select_settle, deadline)?;
        }
        Ok(())
    }

    /// Compatibility with the C API: select first, then access the 256-byte
    /// window without reselecting between chunks, including selector bytes.
    pub fn read_window(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> Result<()> {
        self.read_impl(address, page, bank, offset, out, true)
    }

    /// Read a 128-byte window: the canonical page and bank must be supplied
    /// for lower memory, and the selection is remembered so a repeated read
    /// of the same window does not rewrite the selector bytes. On failure
    /// `out` is unchanged.
    pub fn read_memory(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> Result<()> {
        self.read_impl(address, page, bank, offset, out, false)
    }

    fn read_impl(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        out: &mut [u8],
        window: bool,
    ) -> Result<()> {
        let address = self.validate(address, page, bank, offset, out.len(), window)?;
        let deadline = Instant::now() + self.timing.timeout;
        if window || offset >= 128 {
            self.select(address, page, bank, deadline)?;
        }
        // Publish no partial output, including after a late failure.
        let mut data = vec![0; out.len()];
        let mut done = 0;
        while done < data.len() {
            let at = usize::from(offset) + done;
            // A controller's larger transfer limit does not remove the
            // module's lower/upper boundary. Retain selection across fragments.
            let length = (data.len() - done)
                .min(self.bus.limits().read)
                .min(128 - at % 128);
            self.bus
                .write_read(
                    address,
                    &[at as u8],
                    &mut data[done..done + length],
                    deadline,
                )
                .map_err(|error| self.forget_on_continuity_error(error))?;
            done += length;
        }
        out.copy_from_slice(&data);
        Ok(())
    }

    /// Compatibility with the C API on writes: select first, then write the
    /// 256-byte window without reselecting between chunks, including across
    /// the selector bytes (which drops the remembered selection). Every
    /// write boundary is marked `write_may_have_applied` on failure.
    pub fn write_window(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<()> {
        self.write_impl(address, page, bank, offset, data, true)
    }

    /// Write inside one 128-byte window: the canonical page and bank must be
    /// supplied for lower memory, a write covering the selector bytes drops
    /// the remembered selection, and every write boundary is marked
    /// `write_may_have_applied` on failure — never retried.
    pub fn write_memory(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<()> {
        self.write_impl(address, page, bank, offset, data, false)
    }

    fn write_impl(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        data: &[u8],
        window: bool,
    ) -> Result<()> {
        let address = self.validate(address, page, bank, offset, data.len(), window)?;
        let deadline = Instant::now() + self.timing.timeout;
        if window || offset >= 128 {
            self.select(address, page, bank, deadline)?;
        }
        // Bytes 126 and 127 of the lower window are the module's own selector
        // registers, so a write whose range covers them changes the selection
        // itself - possibly only in part, if the write fails. The confirmed
        // selection stops being trustworthy before the first packet, not after
        // the last one.
        if usize::from(offset) < 128 && usize::from(offset) + data.len() > 126 {
            self.selection = None;
        }
        let mut done = 0;
        while done < data.len() {
            let at = usize::from(offset) + done;
            let length = (data.len() - done)
                .min(self.bus.limits().write - 1)
                .min(self.timing.write_block - at % self.timing.write_block)
                .min(128 - at % 128);
            let mut packet = Vec::with_capacity(length + 1);
            packet.push(at as u8);
            packet.extend_from_slice(&data[done..done + length]);
            self.bus.write(address, &packet, deadline).map_err(|e| {
                let e = self.forget_on_continuity_error(e);
                if done > 0 {
                    e.uncertain()
                } else {
                    e
                }
            })?;
            done += length;
            Self::settle(self.timing.write_settle, deadline).map_err(Error::uncertain)?;
        }
        Ok(())
    }
}

impl<B: I2cBus> Transport for Memory<B> {
    fn read(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> std::result::Result<(), ModuleError> {
        self.read_memory(address, page, bank, offset, out)
            .map_err(|e| ModuleError::Io(IoError::with_source(e.to_string(), e)))
    }
    fn write(
        &mut self,
        address: u8,
        page: u8,
        bank: u8,
        offset: u8,
        data: &[u8],
    ) -> std::result::Result<(), ModuleError> {
        self.write_memory(address, page, bank, offset, data)
            .map_err(|e| ModuleError::Io(IoError::with_source(e.to_string(), e)))
    }
    fn probe(&mut self) -> std::result::Result<(), ModuleError> {
        self.read(0x50, 0, 0, 0, &mut [0])
    }
}
