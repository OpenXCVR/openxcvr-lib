//! T36: Revision-dispatch tests across the CMIS fixture corpus.
//!
//! Verifies that lower memory byte 1 (CMIS revision compliance) carries the
//! expected packed-BCD value for each revision tier present in the corpus:
//! 3.0 (synthetic), 4.0 (fboss), 5.0 (fboss), 5.1 (fboss), 5.2 (synthetic),
//! 5.3 (fboss).  One representative fixture per tier is sufficient — the
//! identifier byte and higher-level decode are exercised separately in golden.rs.

use openxcvr_cmis::{module::CmisTransport, CmisModule};
use openxcvr_core::{error::ModuleError, transport::Transport};

struct BytesTransport {
    lower: [u8; 128],
    upper: [u8; 128],
}

impl Transport for BytesTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = if offset < 128 {
            &self.lower[offset as usize..]
        } else {
            &self.upper[(offset as usize - 128)..]
        };
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }
    fn write(&mut self, _: u8, _: u8, _: u8, _: u8, _: &[u8]) -> Result<(), ModuleError> {
        Err(ModuleError::Io("read-only dispatch transport".into()))
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
impl CmisTransport for BytesTransport {}

fn load(stem: &str) -> CmisModule<BytesTransport> {
    let lower_path = format!("../../tests/fixtures/cmis/{stem}.lower.bin");
    let lower_bytes =
        std::fs::read(&lower_path).unwrap_or_else(|e| panic!("could not read {lower_path}: {e}"));
    assert_eq!(lower_bytes.len(), 128, "{lower_path}: expected 128 bytes");
    CmisModule::new(BytesTransport {
        lower: lower_bytes.try_into().unwrap(),
        upper: [0u8; 128],
    })
    .expect("CMIS detect")
}

#[test]
fn synthetic_cmis_3_0_revision_byte() {
    let m = load("synthetic-cmis-3-0-qsfp-dd");
    assert_eq!(m.cmis_revision().unwrap(), 0x30, "CMIS 3.0 → 0x30");
}

#[test]
fn fboss_cmis_4_0_revision_byte() {
    let m = load("fboss-cmis-4-0-qsfpdd-400g-dr4");
    assert_eq!(m.cmis_revision().unwrap(), 0x40, "CMIS 4.0 → 0x40");
}

#[test]
fn fboss_cmis_5_0_revision_byte() {
    let m = load("fboss-cmis-5-0-osfp-2x400g-fr4");
    assert_eq!(m.cmis_revision().unwrap(), 0x50, "CMIS 5.0 → 0x50");
}

#[test]
fn fboss_cmis_5_1_revision_byte() {
    let m = load("fboss-cmis-5-1-osfp-2x800g-dr4");
    assert_eq!(m.cmis_revision().unwrap(), 0x51, "CMIS 5.1 → 0x51");
}

#[test]
fn synthetic_cmis_5_2_revision_byte() {
    let m = load("synthetic-cmis-5-2-qsfp-dd");
    assert_eq!(m.cmis_revision().unwrap(), 0x52, "CMIS 5.2 → 0x52");
}

#[test]
fn fboss_cmis_5_3_revision_byte() {
    let m = load("fboss-cmis-5-3-osfp-800g-zr");
    assert_eq!(m.cmis_revision().unwrap(), 0x53, "CMIS 5.3 → 0x53");
}
