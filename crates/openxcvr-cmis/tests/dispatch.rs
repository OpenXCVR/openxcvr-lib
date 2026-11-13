//! B8: detect-once construction reads the MajorRevision byte (Lower:1) exactly
//! once and freezes the detected rev on the module handle.

use openxcvr_cmis::CmisModule;
use openxcvr_core::Rev;

#[test]
fn detect_once_uses_major_revision_byte() {
    let mut buf = vec![0u8; 256];
    buf[0] = 0x18; // QSFP-DD identifier (SFF-8024)
    buf[1] = 0x40; // MajorRevision = CMIS 4.0
    let m = CmisModule::from_bytes(&buf).expect("from_bytes");
    assert_eq!(m.detected_rev(), Rev::CMIS_4_0);
}
