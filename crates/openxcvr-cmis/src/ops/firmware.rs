use std::{error::Error, fmt};

use openxcvr_core::error::ModuleError;

use crate::{
    adapter::cdb::{
        Cdb, CdbError, CmisCdbFirmwareDownloadComplete, CmisCdbFirmwareDownloadCompleteRequest,
        CmisCdbFirmwareDownloadStart, CmisCdbFirmwareDownloadStartRequest,
        CmisCdbFirmwareWriteBlockLpl, CmisCdbFirmwareWriteBlockLplRequest,
    },
    module::CmisTransport,
};

/// Why a firmware download failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum FirmwareError {
    /// The CDB firmware command failed; the original error is the source.
    Cdb(CdbError),
    /// The image to download is empty.
    ImageEmpty,
    /// A download chunk exceeds the CDB LPL window.
    ChunkTooLarge,
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
}

impl fmt::Display for FirmwareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cdb(err) => write!(f, "firmware CDB error: {err}"),
            Self::ImageEmpty => write!(f, "firmware image is empty"),
            Self::ChunkTooLarge => write!(f, "firmware chunk size is too large"),
            Self::Transport(err) => write!(f, "firmware transport error: {err}"),
        }
    }
}

impl Error for FirmwareError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<CdbError> for FirmwareError {
    fn from(err: CdbError) -> Self {
        Self::Cdb(err)
    }
}

/// What a firmware download accomplished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// Number of chunks written.
    pub chunks_written: u32,
    /// Total image bytes written.
    pub total_bytes: u32,
}

const DEFAULT_CHUNK_SIZE: usize = 110;

/// Download firmware image to module via CDB commands.
/// Steps: CDB 0x0101 (Start) -> CDB 0x0103 per chunk -> CDB 0x0107 (Complete)
pub fn download<T: CmisTransport>(
    transport: &mut T,
    image: &[u8],
    chunk_size: Option<usize>,
    max_polls: u32,
) -> Result<DownloadOutcome, FirmwareError> {
    if image.is_empty() {
        return Err(FirmwareError::ImageEmpty);
    }

    let chunk_sz = chunk_size
        .unwrap_or(DEFAULT_CHUNK_SIZE)
        .min(DEFAULT_CHUNK_SIZE);
    if chunk_sz == 0 {
        return Err(FirmwareError::ChunkTooLarge);
    }

    Cdb::send::<CmisCdbFirmwareDownloadStart, _>(
        transport,
        &CmisCdbFirmwareDownloadStartRequest {
            image_size: image.len() as u32,
            ..Default::default()
        },
        max_polls,
    )?;

    let mut chunks_written = 0;
    for (index, chunk) in image.chunks(chunk_sz).enumerate() {
        Cdb::send::<CmisCdbFirmwareWriteBlockLpl, _>(
            transport,
            &CmisCdbFirmwareWriteBlockLplRequest {
                block_address: (index * chunk_sz) as u32,
                firmware_block: chunk.to_vec(),
            },
            max_polls,
        )?;
        chunks_written += 1;
    }

    Cdb::send::<CmisCdbFirmwareDownloadComplete, _>(
        transport,
        &CmisCdbFirmwareDownloadCompleteRequest,
        max_polls,
    )?;

    Ok(DownloadOutcome {
        chunks_written,
        total_bytes: image.len() as u32,
    })
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
    const CDB_REPLY_LENGTH_OFFSET: u8 = 134;

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

        fn written_commands(&self) -> Vec<u16> {
            self.cdb_writes
                .iter()
                .map(|write| u16::from_be_bytes([write[0], write[1]]))
                .collect()
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
                    buf.copy_from_slice(
                        &self.page_9f[CDB_REPLY_LENGTH_OFFSET as usize
                            ..CDB_REPLY_LENGTH_OFFSET as usize + 2],
                    );
                    Ok(())
                }
                (CDB_PAGE, _, _) => {
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
            assert_eq!((page, bank, offset), (CDB_PAGE, 0, 128));

            let start = offset as usize;
            self.page_9f[start..start + data.len()].copy_from_slice(data);
            self.cdb_writes.push(data.to_vec());
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_download_empty_image() {
        let mut transport = MockTransport::new();

        assert!(matches!(
            download(&mut transport, &[], Some(128), 1),
            Err(FirmwareError::ImageEmpty)
        ));
    }

    #[test]
    fn test_download_sequence() {
        let mut transport = MockTransport::new();
        let image = vec![0u8; 256];

        let outcome = download(&mut transport, &image, Some(128), 1).unwrap();

        assert_eq!(
            outcome,
            DownloadOutcome {
                chunks_written: 3,
                total_bytes: 256,
            }
        );
        assert_eq!(
            transport.written_commands(),
            vec![0x0101, 0x0103, 0x0103, 0x0103, 0x0107]
        );
    }
}
