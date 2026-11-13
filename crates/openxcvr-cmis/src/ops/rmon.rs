use std::{error::Error, fmt};

use crate::{
    adapter::cdb::{
        Cdb, CdbError, CmisCdbRmonStatistics, CmisCdbRmonStatisticsRequest,
        CmisCdbRmonStatisticsResponse,
    },
    module::CmisTransport,
};

const DEFAULT_MAX_POLLS: u32 = 100;

/// Why an RMON statistics read failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum RmonError {
    /// The CDB command failed; the original `CdbError` (with its transport
    /// cause chain) is preserved as the error source.
    Cdb(CdbError),
}

impl fmt::Display for RmonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cdb(err) => write!(f, "RMON CDB error: {err}"),
        }
    }
}

impl Error for RmonError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cdb(err) => Some(err),
        }
    }
}

impl From<CdbError> for RmonError {
    fn from(err: CdbError) -> Self {
        Self::Cdb(err)
    }
}

/// Which side of the data path RMON counters watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmonMonitorLocation {
    /// Count frames on the media-side receive path.
    MediaSideRx,
    /// Count frames on the host-side transmit path.
    HostSideTx,
}

impl RmonMonitorLocation {
    fn as_cdb_bit(self) -> bool {
        match self {
            Self::MediaSideRx => false,
            Self::HostSideTx => true,
        }
    }
}

/// Options one RMON read carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmonOptions {
    /// The data path to read statistics for.
    pub dp_id: u8,
    /// Clear the counters after this read.
    pub clear_on_read: bool,
    /// Which side to count frames on.
    pub monitor_location: RmonMonitorLocation,
    /// The CDB busy-poll budget.
    pub max_polls: u32,
}

impl RmonOptions {
    /// Default options for one data path: media-side RX, no clear-on-read,
    /// the default poll budget.
    pub fn new(dp_id: u8) -> Self {
        Self {
            dp_id,
            clear_on_read: false,
            monitor_location: RmonMonitorLocation::MediaSideRx,
            max_polls: DEFAULT_MAX_POLLS,
        }
    }
}

/// Typed RMON statistics for one data path, decoded from the 0220h reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RmonStats {
    /// Total good frames.
    pub frame_count: u64,
    /// Total good octets.
    pub octet_count: u64,
    /// Total bad (FCS/CRC error) frames.
    pub bad_frame_count: u64,
    /// Total bad octets.
    pub bad_octet_count: u64,
    /// Multicast frames received.
    pub multicast_count: u64,
    /// Broadcast frames received.
    pub broadcast_count: u64,
    /// 64-byte packets.
    pub packets_64b: u64,
    /// 65-127 byte packets.
    pub packets_64_to_127b: u64,
    /// 128-255 byte packets.
    pub packets_128_to_255b: u64,
    /// 256-511 byte packets.
    pub packets_256_to_511b: u64,
    /// 512-1023 byte packets.
    pub packets_512_to_1023b: u64,
    /// 1024-1518 byte packets.
    pub packets_1024_to_1518b: u64,
    /// Packets larger than 1518 bytes but not jumbo.
    pub packets_large_non_jumbo: u32,
    /// Jumbo packets.
    pub packets_jumbo: u32,
    /// Bad multicast frames.
    pub bad_multicast_count: u32,
    /// Bad broadcast frames.
    pub bad_broadcast_count: u32,
    /// Bad 64-byte packets.
    pub bad_packets_64b: u32,
    /// Bad 65-127 byte packets.
    pub bad_packets_64_to_127b: u32,
    /// Bad 128-255 byte packets.
    pub bad_packets_128_to_255b: u32,
    /// Bad 256-511 byte packets.
    pub bad_packets_256_to_511b: u32,
    /// Bad 512-1023 byte packets.
    pub bad_packets_512_to_1023b: u32,
    /// Bad 1024-1518 byte packets.
    pub bad_packets_1024_to_1518b: u32,
    /// Bad packets larger than 1518 bytes but not jumbo.
    pub bad_packets_large_non_jumbo: u32,
    /// Bad jumbo packets.
    pub bad_packets_jumbo: u32,
}

/// Read RMON statistics for one data path with default options.
pub fn statistics<T: CmisTransport>(transport: &mut T, dp_id: u8) -> Result<RmonStats, RmonError> {
    statistics_with_options(transport, RmonOptions::new(dp_id))
}

/// Read RMON statistics for one data path with caller options.
pub fn statistics_with_options<T: CmisTransport>(
    transport: &mut T,
    options: RmonOptions,
) -> Result<RmonStats, RmonError> {
    let response = Cdb::send::<CmisCdbRmonStatistics, _>(
        transport,
        &CmisCdbRmonStatisticsRequest {
            clear_on_read: options.clear_on_read,
            monitor_location: options.monitor_location.as_cdb_bit(),
            data_path_id: options.dp_id,
        },
        options.max_polls,
    )?;

    Ok(RmonStats::from_response(&response))
}

impl RmonStats {
    fn from_response(response: &CmisCdbRmonStatisticsResponse) -> Self {
        let u48 = response
            .rmon_u48_counters
            .chunks_exact(6)
            .map(u48be)
            .collect::<Vec<_>>();
        let u32 = response
            .rmon_u32_counters
            .chunks_exact(4)
            .map(u32be)
            .collect::<Vec<_>>();

        Self {
            frame_count: u48[0],
            octet_count: u48[1],
            bad_frame_count: u48[2],
            bad_octet_count: u48[3],
            multicast_count: u48[4],
            broadcast_count: u48[5],
            packets_64b: u48[6],
            packets_64_to_127b: u48[7],
            packets_128_to_255b: u48[8],
            packets_256_to_511b: u48[9],
            packets_512_to_1023b: u48[10],
            packets_1024_to_1518b: u48[11],
            packets_large_non_jumbo: u32[0],
            packets_jumbo: u32[1],
            bad_multicast_count: u32[2],
            bad_broadcast_count: u32[3],
            bad_packets_64b: u32[4],
            bad_packets_64_to_127b: u32[5],
            bad_packets_128_to_255b: u32[6],
            bad_packets_256_to_511b: u32[7],
            bad_packets_512_to_1023b: u32[8],
            bad_packets_1024_to_1518b: u32[9],
            bad_packets_large_non_jumbo: u32[10],
            bad_packets_jumbo: u32[11],
        }
    }
}

/// Read `samples` consecutive RMON snapshots for one data path.
pub fn poll_statistics<T: CmisTransport>(
    transport: &mut T,
    dp_id: u8,
    samples: usize,
) -> Result<Vec<RmonStats>, RmonError> {
    (0..samples).map(|_| statistics(transport, dp_id)).collect()
}

fn u48be(bytes: &[u8]) -> u64 {
    let mut out = [0u8; 8];
    out[2..].copy_from_slice(bytes);
    u64::from_be_bytes(out)
}

fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
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
    const CDB_CMD_OFFSET: u8 = 128;
    const CDB_REPLY_LENGTH_OFFSET: u8 = 134;
    const CDB_LPL_OFFSET: u8 = 136;
    const CDB_STATUS_OFFSET: u8 = 37;

    #[derive(Default)]
    struct MockTransport {
        cdb_writes: Vec<Vec<u8>>,
        reply_lpl: Vec<u8>,
    }

    impl MockTransport {
        fn with_reply_lpl(reply_lpl: Vec<u8>) -> Self {
            Self {
                cdb_writes: Vec::new(),
                reply_lpl,
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
                (LOWER_PAGE, CDB_STATUS_OFFSET, 1) => buf[0] = 0x01,
                (CDB_PAGE, CDB_REPLY_LENGTH_OFFSET, 2) => {
                    buf[0] = self.reply_lpl.len() as u8;
                    // RPLChkCode: one's complement of the reply payload sum
                    // (CMIS 5.3 Table 8-179); 0 when there is no reply body.
                    buf[1] = if self.reply_lpl.is_empty() {
                        0
                    } else {
                        !self
                            .reply_lpl
                            .iter()
                            .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                    };
                }
                (CDB_PAGE, CDB_LPL_OFFSET, _) => buf.copy_from_slice(&self.reply_lpl),
                _ => panic!(
                    "unexpected read page=0x{page:02x} bank={bank} offset={offset} len={}",
                    buf.len()
                ),
            }
            Ok(())
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
            assert_eq!((page, bank, offset), (CDB_PAGE, 0, CDB_CMD_OFFSET));
            self.cdb_writes.push(data.to_vec());
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    fn rmon_reply_lpl() -> Vec<u8> {
        let mut lpl = Vec::new();
        for value in 1u64..=12 {
            lpl.extend_from_slice(&value.to_be_bytes()[2..]);
        }
        for value in 101u32..=112 {
            lpl.extend_from_slice(&value.to_be_bytes());
        }
        lpl
    }

    #[test]
    fn test_statistics_sends_rmon_cdb_request() {
        let mut transport = MockTransport::with_reply_lpl(rmon_reply_lpl());

        statistics(&mut transport, 3).unwrap();

        let write = &transport.cdb_writes[0];
        assert_eq!(write[0..2], [0x02, 0x20]);
        assert_eq!(write[4], 2, "LPL length at 9Fh:132");
        assert_eq!(&write[8..10], &[0x00, 3]);
    }

    #[test]
    fn test_statistics_decodes_counters() {
        let mut transport = MockTransport::with_reply_lpl(rmon_reply_lpl());

        let stats = statistics(&mut transport, 7).unwrap();

        assert_eq!(
            stats,
            RmonStats {
                frame_count: 1,
                octet_count: 2,
                bad_frame_count: 3,
                bad_octet_count: 4,
                multicast_count: 5,
                broadcast_count: 6,
                packets_64b: 7,
                packets_64_to_127b: 8,
                packets_128_to_255b: 9,
                packets_256_to_511b: 10,
                packets_512_to_1023b: 11,
                packets_1024_to_1518b: 12,
                packets_large_non_jumbo: 101,
                packets_jumbo: 102,
                bad_multicast_count: 103,
                bad_broadcast_count: 104,
                bad_packets_64b: 105,
                bad_packets_64_to_127b: 106,
                bad_packets_128_to_255b: 107,
                bad_packets_256_to_511b: 108,
                bad_packets_512_to_1023b: 109,
                bad_packets_1024_to_1518b: 110,
                bad_packets_large_non_jumbo: 111,
                bad_packets_jumbo: 112,
            }
        );
    }
}
