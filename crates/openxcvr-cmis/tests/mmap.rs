use openxcvr_cmis::{CmisMmapTransport, CmisModule};
use openxcvr_core::{module::Module, transport::Transport};
use std::path::{Path, PathBuf};

fn fixture(suffix: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/cmis")
        .join(format!("fboss-cmis-4-0-qsfpdd-400g-dr4.{suffix}.bin"))
}

fn transport() -> CmisMmapTransport {
    // This capture reports CMIS 4.0. Page 02h is a full 128-byte stand-in
    // for unused Pages 03h..05h, as in the existing golden tests.
    assert_eq!(std::fs::read(fixture("lower")).unwrap()[1], 0x40);
    CmisMmapTransport::from_files(
        fixture("lower"),
        fixture("page-00h"),
        fixture("page-01h"),
        fixture("page-02h"),
        fixture("page-02h"),
        fixture("page-02h"),
        fixture("page-02h"),
    )
    .unwrap()
}

#[test]
fn mmap_reads_page_11h_and_decodes_lanes_and_flags() {
    let mut t = transport().with_page_11h(fixture("page-11h")).unwrap();
    let bytes = std::fs::read(fixture("page-11h")).unwrap();
    let mut slice = [0; 8];
    t.read(0x50, 0x11, 0, 192, &mut slice).unwrap();
    assert_eq!(slice, bytes[64..72]);
    assert!(t.read(0x50, 0x11, 1, 128, &mut slice).is_err());
    assert!(t.read(0x50, 0x11, 0, 255, &mut slice).is_err());
    let module = CmisModule::new(t).unwrap();
    assert_eq!(module.lanes().unwrap().len(), 8);
    assert_eq!(module.flags().unwrap().lanes.len(), 8);
}

#[test]
fn missing_page_11h_is_not_fabricated() {
    let mut t = transport();
    assert!(t.read(0x50, 0x11, 0, 128, &mut [0; 128]).is_err());
    assert!(t.with_page_11h(fixture("missing-page")).is_err());
}
