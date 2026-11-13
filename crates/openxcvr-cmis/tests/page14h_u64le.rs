//! CMIS Page 14h diagnostic counters are little-endian and stay that way
//! when C-CMIS counters move to `u64be`.

use openxcvr_cmis::generated::union::CmisPage14hDiagnosticsResults;

/// LSB at the lowest address: the Page 14h rule ("U64 Little-endian
/// (LSB first)"), not a decoder.
fn lsb_first(bytes: &[u8; 8]) -> u64 {
    bytes.iter().enumerate().fold(0u64, |acc, (index, byte)| {
        acc | (u64::from(*byte) << (8 * index))
    })
}

#[test]
fn page14h_host_error_count_decodes_little_endian() {
    // HostSideErrorCountLane1 is bytes 192-199 of the 256-byte window,
    // index 64 of the 128-byte upper-page buffer.
    let bytes = [0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let expected = lsb_first(&bytes);
    assert_eq!(expected, 0x0102);
    let mut buf = [0u8; 128];
    buf[64..72].copy_from_slice(&bytes);
    let page = CmisPage14hDiagnosticsResults::decode(&buf);
    assert_eq!(page.host_side_error_count_lane_1_4[0], expected);
    let big_endian = bytes
        .iter()
        .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
    assert_ne!(page.host_side_error_count_lane_1_4[0], big_endian);
}
