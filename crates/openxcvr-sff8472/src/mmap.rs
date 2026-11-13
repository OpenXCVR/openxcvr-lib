//! Mmap-capture transport for SFF-8472: A0h and A2h pages loaded from
//! files, serving reads from the captured bytes.

use openxcvr_core::{
    error::{IoError, ModuleError},
    transport::Transport,
};

/// In-memory transport over captured SFF-8472 memory images: the 256-byte
/// A0h base page and the 256-byte A2h diagnostics page, loaded once from
/// files. Reads are served from the captured bytes; writes fail — a capture
/// is immutable evidence, not a device.
pub struct Sff8472MmapTransport {
    a0h: [u8; 256],
    a2h: [u8; 256],
}

impl Sff8472MmapTransport {
    /// Load the A0h and A2h captures from files. A short A0h image is padded
    /// with zeros; an A2h image must be exactly 256 bytes — a truncated or
    /// over-long A2h capture fails closed rather than decoding partial
    /// diagnostics.
    pub fn from_files(
        a0h_path: impl AsRef<std::path::Path>,
        a2h_path: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        Ok(Self {
            a0h: read_padded(a0h_path)?,
            a2h: read_exact(a2h_path)?,
        })
    }
}

fn read_exact<const N: usize>(path: impl AsRef<std::path::Path>) -> Result<[u8; N], ModuleError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| {
        ModuleError::Io(IoError::with_source(format!("{}: {e}", path.display()), e))
    })?;
    if bytes.len() != N {
        return Err(ModuleError::Io(
            format!(
                "{}: expected {N} bytes, got {}",
                path.display(),
                bytes.len()
            )
            .into(),
        ));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn read_padded<const N: usize>(path: impl AsRef<std::path::Path>) -> Result<[u8; N], ModuleError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| {
        ModuleError::Io(IoError::with_source(format!("{}: {e}", path.display()), e))
    })?;
    if bytes.len() > N {
        return Err(ModuleError::Io(
            format!(
                "{}: expected at most {N} bytes, got {}",
                path.display(),
                bytes.len()
            )
            .into(),
        ));
    }
    let mut arr = [0u8; N];
    arr[..bytes.len()].copy_from_slice(&bytes);
    Ok(arr)
}

impl Transport for Sff8472MmapTransport {
    fn read(
        &mut self,
        slave_addr: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match slave_addr {
            0xA0 => &self.a0h[offset as usize..],
            0xA2 => &self.a2h[offset as usize..],
            _ => {
                return Err(ModuleError::Io(
                    format!("unknown SFF-8472 slave address: 0x{slave_addr:02x}").into(),
                ))
            }
        };
        if buf.len() > src.len() {
            return Err(ModuleError::Io(
                format!(
                    "read overflow: slave_addr=0x{slave_addr:02x} offset={offset} len={}",
                    buf.len()
                )
                .into(),
            ));
        }
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }

    fn write(
        &mut self,
        slave_addr: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), ModuleError> {
        // In-memory read-modify-write over the capture: an offline capture
        // file is the transport for offline control testing, so a write
        // mutates the capture image rather than hardware. Only the A2h
        // control bytes this crate's control surface exposes (byte 110,
        // Table 9-16, and byte 118, Table 10-1) are writable — mirroring
        // the SFF-8636 mmap transport's writable control region.
        if slave_addr != 0xA2 || data.len() != 1 || !matches!(offset, 110 | 118) {
            return Err(ModuleError::Io(
                format!(
                    "Sff8472MmapTransport write outside the A2h control bytes: \
                 slave 0x{slave_addr:02x} offset {offset} len {}",
                    data.len()
                )
                .into(),
            ));
        }
        self.a2h[offset as usize] = data[0];
        Ok(())
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
