//! Checksum-recompute tests for SFF-8636 Page 00h Upper Administrative.
//!
//! For each fixture, clone the raw page buffer, call `recompute_checksums`,
//! and assert the recomputed bytes match those already stored in the fixture.
//! CC_BASE lives at buf[63] and covers buf[0..=62];
//! CC_EXT lives at buf[95] and covers buf[64..=94].

use openxcvr_sff8636::generated::union::Sff8636Page00hUpperAdministrative;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_page_00h(stem: &str) -> [u8; 128] {
    let path = repo_root().join(format!("tests/fixtures/sff-8636/{stem}.page-00h.bin"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        bytes.len(),
        128,
        "{}: expected 128 bytes, got {}",
        path.display(),
        bytes.len()
    );
    let mut arr = [0u8; 128];
    arr.copy_from_slice(&bytes);
    arr
}

macro_rules! test_page_00h {
    ($name:ident, $stem:expr) => {
        #[test]
        fn $name() {
            let mut buf = read_page_00h($stem);
            let cc_base_stored = buf[63];
            let cc_ext_stored = buf[95];
            Sff8636Page00hUpperAdministrative::recompute_checksums(&mut buf);
            assert_eq!(
                buf[63], cc_base_stored,
                "{} CC_BASE: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[63], cc_base_stored
            );
            assert_eq!(
                buf[95], cc_ext_stored,
                "{} CC_EXT: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[95], cc_ext_stored
            );
        }
    };
}

test_page_00h!(
    page_00h_finisar_ftl410qe3c_40g_sr4,
    "finisar-ftl410qe3c-40g-sr4"
);
test_page_00h!(
    page_00h_finisar_ftlc9551repm_100g_sr4,
    "finisar-ftlc9551repm-100g-sr4"
);
test_page_00h!(
    page_00h_synthetic_qsfp_dac_40g_passive_cu,
    "synthetic-qsfp-dac-40g-passive-cu"
);
test_page_00h!(
    page_00h_synthetic_qsfp28_aoc_100g_sr4,
    "synthetic-qsfp28-aoc-100g-sr4"
);
test_page_00h!(
    page_00h_synthetic_qsfp28_bidi_100g_cwdm4,
    "synthetic-qsfp28-bidi-100g-cwdm4"
);
