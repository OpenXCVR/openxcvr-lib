use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE};

use crate::{adapter::cdb::CdbError, module::CmisTransport};

const LOOPBACK_PAGE: u8 = 0x13;
const LOOPBACK_FIRST_OFFSET: u8 = 180;

/// Loopback type to engage on a lane. CMIS 5.3 § 8.12.12 (Table 8-121, Page
/// 13h bytes 180-183) defines four loopback registers by side and signal
/// direction — MediaSideOutput, MediaSideInput, HostSideOutput,
/// HostSideInput — and does not use near/far terminology. This API maps
/// `*Near` onto the side's output loopback and `*Far` onto the side's input
/// loopback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackMode {
    /// Loopbacks disabled.
    None,
    /// Host-side electrical output loopback.
    HostNear,
    /// Host-side electrical input loopback.
    HostFar,
    /// Media-side (optical) output loopback.
    MediaNear,
    /// Media-side (optical) input loopback.
    MediaFar,
}

/// Why a loopback operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoopbackError {
    /// The CDB Loopbacks command failed; the original error is the source.
    Cdb(CdbError),
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
}

impl From<CdbError> for LoopbackError {
    fn from(err: CdbError) -> Self {
        Self::Cdb(err)
    }
}

impl fmt::Display for LoopbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cdb(err) => write!(f, "loopback CDB error: {err}"),
            Self::Transport(err) => write!(f, "loopback transport error: {err}"),
        }
    }
}

impl Error for LoopbackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

/// Engage one loopback mode on the lanes in `lane_mask`.
pub fn engage<T: CmisTransport>(
    transport: &mut T,
    mode: LoopbackMode,
    lane_mask: u8,
    max_polls: u32,
) -> Result<(), LoopbackError> {
    let _ = max_polls;
    let mut controls = [0u8; 4];
    transport
        .read(
            I2C_ADDR_MODULE,
            LOOPBACK_PAGE,
            0,
            LOOPBACK_FIRST_OFFSET,
            &mut controls,
        )
        .map_err(LoopbackError::Transport)?;

    if mode == LoopbackMode::None {
        for control in &mut controls {
            *control &= !lane_mask;
        }
    } else {
        // Byte 180 = MediaSideOutput, 181 = MediaSideInput,
        // 182 = HostSideOutput, 183 = HostSideInput loopback enable lanes.
        let index = match mode {
            LoopbackMode::MediaNear => 0,
            LoopbackMode::MediaFar => 1,
            LoopbackMode::HostNear => 2,
            LoopbackMode::HostFar => 3,
            LoopbackMode::None => unreachable!(),
        };
        controls[index] |= lane_mask;
    }
    transport
        .write(
            I2C_ADDR_MODULE,
            LOOPBACK_PAGE,
            0,
            LOOPBACK_FIRST_OFFSET,
            &controls,
        )
        .map_err(LoopbackError::Transport)
}

/// Release all loopbacks on the lanes in `lane_mask`.
pub fn release<T: CmisTransport>(
    transport: &mut T,
    lane_mask: u8,
    max_polls: u32,
) -> Result<(), LoopbackError> {
    engage(transport, LoopbackMode::None, lane_mask, max_polls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{
        error::ModuleError,
        transport::{Transport, I2C_ADDR_MODULE},
    };

    const LOWER_PAGE: u8 = 0x00;
    const CDB_PAGE: u8 = 0x9F;
    const CDB_STATUS_OFFSET: u8 = 37;
    const CDB_REPLY_LENGTH_OFFSET: u8 = 130;

    struct MockTransport {
        lower: [u8; 128],
        page_9f: [u8; 256],
        cdb_writes: Vec<Vec<u8>>,
    }

    impl MockTransport {
        fn new() -> Self {
            let mut lower = [0u8; 128];
            lower[CDB_STATUS_OFFSET as usize] = 0x01;

            Self {
                lower,
                page_9f: [0u8; 256],
                cdb_writes: Vec::new(),
            }
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(bank, 0);

            match (page, offset, buf.len()) {
                (LOWER_PAGE, CDB_STATUS_OFFSET, 1) => {
                    buf[0] = self.lower[CDB_STATUS_OFFSET as usize];
                    Ok(())
                }
                (CDB_PAGE, CDB_REPLY_LENGTH_OFFSET, 2) => {
                    let start = CDB_REPLY_LENGTH_OFFSET as usize;
                    buf.copy_from_slice(&self.page_9f[start..start + 2]);
                    Ok(())
                }
                (CDB_PAGE, _, _) | (LOOPBACK_PAGE, _, _) => {
                    let start = offset as usize;
                    buf.copy_from_slice(&self.page_9f[start..start + buf.len()]);
                    Ok(())
                }
                _ => Err(ModuleError::Io(
                    format!(
                        "unexpected read page=0x{page:02x} offset={offset} len={}",
                        buf.len()
                    )
                    .into(),
                )),
            }
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(bank, 0);
            let start = offset as usize;
            self.page_9f[start..start + data.len()].copy_from_slice(data);
            if page == CDB_PAGE {
                self.cdb_writes.push(data.to_vec());
            } else {
                assert_eq!((page, offset), (LOOPBACK_PAGE, LOOPBACK_FIRST_OFFSET));
            }
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_engage_host_near_programs_page_13h() {
        let mut mock = MockTransport::new();
        mock.page_9f[182] = 0x80;

        assert!(engage(&mut mock, LoopbackMode::HostNear, 0x0F, 10).is_ok());
        assert_eq!(mock.page_9f[182], 0x8f);
        assert!(mock.cdb_writes.is_empty());
    }

    #[test]
    fn test_release_clears_requested_lanes_only() {
        let mut mock = MockTransport::new();
        mock.page_9f[180..184].fill(0xff);

        assert!(release(&mut mock, 0x0f, 10).is_ok());
        assert_eq!(&mock.page_9f[180..184], &[0xf0; 4]);
        assert!(mock.cdb_writes.is_empty());
    }
}
