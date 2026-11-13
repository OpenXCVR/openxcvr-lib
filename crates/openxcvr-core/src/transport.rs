use crate::error::ModuleError;

/// Standard i2c slave address for CMIS / SFF-8636 modules (single-address layout).
pub const I2C_ADDR_MODULE: u8 = 0x50;

/// SFF-8472 base ID memory (`A0h` in the SFF-8472 spec).
pub const I2C_ADDR_SFF8472_BASE: u8 = 0xA0;

/// SFF-8472 diagnostics + DOM memory (`A2h` in the SFF-8472 spec).
pub const I2C_ADDR_SFF8472_DIAG: u8 = 0xA2;

/// The byte-oriented paged/banked transport contract family adapters are
/// generic over: one read/write primitive addressing (`slave_addr`, `page`,
/// `bank`, `offset`). The transport stays the caller's — implementations are
/// mechanisms and never autonomous state owners.
pub trait Transport {
    /// Read `buf.len()` bytes at (`slave_addr`, `page`, `bank`, `offset`).
    ///
    /// `slave_addr` is the i2c slave byte (SFF-8472 8-bit form): use
    /// [`I2C_ADDR_MODULE`] (`0x50`) for CMIS / SFF-8636 modules,
    /// [`I2C_ADDR_SFF8472_BASE`] (`0xA0`) for SFF-8472 base ID, or
    /// [`I2C_ADDR_SFF8472_DIAG`] (`0xA2`) for SFF-8472 DOM.
    fn read(
        &mut self,
        slave_addr: u8,
        page: u8,
        bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError>;

    /// Write `data` at (`slave_addr`, `page`, `bank`, `offset`).
    fn write(
        &mut self,
        slave_addr: u8,
        page: u8,
        bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), ModuleError>;

    /// Check that the device answers (for backends that can prove it);
    /// implementations without a cheaper check may read a known byte.
    fn probe(&mut self) -> Result<(), ModuleError>;
}

/// In-memory buffer-backed transport for unit tests and offline
/// fixture decode. Flat memory layout: bytes 0..128 are Lower
/// memory; bytes 128.. are concatenated Upper pages indexed
/// by `page`, each 128 bytes (Page 00h = buf[128..256], Page 01h
/// = buf[256..384], …). Banked pages are not supported in this
/// trivial impl — sysfs/HIL transports cover that path.
pub struct BufTransport {
    buf: Vec<u8>,
}

impl BufTransport {
    /// Build the buffer-backed transport from a byte slice (copied).
    pub fn from_slice(b: &[u8]) -> Self {
        Self { buf: b.to_vec() }
    }
}

impl Transport for BufTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        // Lower memory is bytes 0..128; Upper pages start at byte 128.
        // For Lower memory the caller passes page=0x00, offset∈[0,128);
        // for an Upper page the caller passes page=NN, offset∈[128,256).
        // Subtract the Upper-memory address-space base before indexing the
        // selected 128-byte page: page 0x00 maps to buf[128..256], page 0x01
        // to buf[256..384], and so on.
        let (start, remaining) = match (page, offset) {
            (0x00, 0..=127) => (offset as usize, 128 - offset as usize),
            (_, 128..=255) => (
                128 + (page as usize) * 128 + (offset as usize - 128),
                256 - offset as usize,
            ),
            _ => {
                return Err(ModuleError::Io(
                    format!("BufTransport unsupported page/offset: 0x{page:02x}/{offset}").into(),
                ))
            }
        };
        if buf.len() > remaining {
            return Err(ModuleError::Io(
                format!(
                    "BufTransport read overflow: page=0x{page:02x} offset={offset} len={}",
                    buf.len()
                )
                .into(),
            ));
        }
        let end = start + buf.len();
        if end > self.buf.len() {
            return Err(ModuleError::Io(
                format!(
                    "BufTransport short read: requested {}..{}, buffer len {}",
                    start,
                    end,
                    self.buf.len()
                )
                .into(),
            ));
        }
        buf.copy_from_slice(&self.buf[start..end]);
        Ok(())
    }

    fn write(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        _data: &[u8],
    ) -> Result<(), ModuleError> {
        Err(ModuleError::Io("BufTransport is read-only".into()))
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{BufTransport, Transport, I2C_ADDR_MODULE};

    #[test]
    fn upper_page_offsets_are_relative_to_the_selected_page() {
        let mut bytes = vec![0u8; 128 * 3];
        bytes[128] = 0xa0;
        bytes[129] = 0xa1;
        bytes[256] = 0xb0;
        bytes[257] = 0xb1;
        let mut transport = BufTransport::from_slice(&bytes);

        let mut page_00h = [0u8; 2];
        transport
            .read(I2C_ADDR_MODULE, 0x00, 0, 128, &mut page_00h)
            .unwrap();
        assert_eq!(page_00h, [0xa0, 0xa1]);

        let mut page_01h = [0u8; 2];
        transport
            .read(I2C_ADDR_MODULE, 0x01, 0, 128, &mut page_01h)
            .unwrap();
        assert_eq!(page_01h, [0xb0, 0xb1]);
    }

    #[test]
    fn rejects_upper_page_with_lower_memory_offset() {
        let mut transport = BufTransport::from_slice(&vec![0u8; 128 * 3]);
        let error = transport
            .read(I2C_ADDR_MODULE, 0x01, 0, 127, &mut [0u8; 1])
            .unwrap_err();
        assert!(error.to_string().contains("unsupported page/offset"));
    }

    #[test]
    fn rejects_read_crossing_lower_memory_boundary() {
        let mut transport = BufTransport::from_slice(&vec![0u8; 128 * 2]);
        let error = transport
            .read(I2C_ADDR_MODULE, 0x00, 0, 127, &mut [0u8; 2])
            .unwrap_err();
        assert!(error.to_string().contains("read overflow"));
    }

    #[test]
    fn rejects_read_crossing_upper_page_boundary() {
        let mut transport = BufTransport::from_slice(&vec![0u8; 128 * 3]);
        let error = transport
            .read(I2C_ADDR_MODULE, 0x01, 0, 255, &mut [0u8; 2])
            .unwrap_err();
        assert!(error.to_string().contains("read overflow"));
    }
}
