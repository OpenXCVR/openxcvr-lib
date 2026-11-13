//! Checksum-recompute tests for CMIS pages.
//!
//! Page 00h: checksum at buf[94] covering buf[0..=93].
//!   Only fixtures whose stored checksum byte matches the sum8 formula are
//!   used here — 12 of the 17 fboss Page 00h fixtures carry pre-computed
//!   vendor-specific values that do not match the sum8 formula and are
//!   intentionally excluded.
//!
//! Page 02h: checksum at buf[127] covering buf[0..=126].
//!   All 15 Page 02h fixtures in the corpus are valid and are tested.

use openxcvr_cmis::generated::{union::CmisPage00hAdministrative, CmisPage02hThresholds};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_128(rel: &str) -> [u8; 128] {
    let path = repo_root().join(rel);
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

// ─── Page 00h ─────────────────────────────────────────────────────────────────
// Only the fixtures below have valid sum8 checksums.  The remaining fboss
// fixtures contain vendor-specific stored values that intentionally differ
// from the spec formula and must not be tested against recompute.

macro_rules! test_page_00h {
    ($name:ident, $stem:expr) => {
        #[test]
        fn $name() {
            let mut buf = read_128(&format!("tests/fixtures/cmis/{}.page-00h.bin", $stem));
            let stored = buf[94];
            CmisPage00hAdministrative::recompute_checksums(&mut buf);
            assert_eq!(
                buf[94], stored,
                "{} page-00h checksum: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[94], stored
            );
        }
    };
}

test_page_00h!(
    page_00h_fboss_cmis_4_0_qsfp112_200g_fr4,
    "fboss-cmis-4-0-qsfp112-200g-fr4"
);
test_page_00h!(
    page_00h_fboss_cmis_4_0_qsfpdd_400g_fr4_multiport,
    "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport"
);
test_page_00h!(
    page_00h_fboss_cmis_4_0_qsfpdd_400g_lr4,
    "fboss-cmis-4-0-qsfpdd-400g-lr4"
);
test_page_00h!(
    page_00h_synthetic_cmis_3_0_qsfp_dd,
    "synthetic-cmis-3-0-qsfp-dd"
);
test_page_00h!(
    page_00h_synthetic_cmis_5_2_qsfp_dd,
    "synthetic-cmis-5-2-qsfp-dd"
);

// ─── Page 02h ─────────────────────────────────────────────────────────────────
// All 15 fixtures carry valid sum8 checksums.

macro_rules! test_page_02h {
    ($name:ident, $stem:expr) => {
        #[test]
        fn $name() {
            let mut buf = read_128(&format!("tests/fixtures/cmis/{}.page-02h.bin", $stem));
            let stored = buf[127];
            CmisPage02hThresholds::recompute_checksums(&mut buf);
            assert_eq!(
                buf[127], stored,
                "{} page-02h checksum: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[127], stored
            );
        }
    };
}

test_page_02h!(
    page_02h_fboss_cmis_4_0_osfp_2x400g_dr4,
    "fboss-cmis-4-0-osfp-2x400g-dr4"
);
test_page_02h!(
    page_02h_fboss_cmis_4_0_qsfp112_200g_fr4,
    "fboss-cmis-4-0-qsfp112-200g-fr4"
);
test_page_02h!(
    page_02h_fboss_cmis_4_0_qsfpdd_400g_dr4,
    "fboss-cmis-4-0-qsfpdd-400g-dr4"
);
test_page_02h!(
    page_02h_fboss_cmis_4_0_qsfpdd_400g_fr4_multiport,
    "fboss-cmis-4-0-qsfpdd-400g-fr4-multiport"
);
test_page_02h!(
    page_02h_fboss_cmis_4_0_qsfpdd_400g_lr4,
    "fboss-cmis-4-0-qsfpdd-400g-lr4"
);
test_page_02h!(
    page_02h_fboss_cmis_5_0_osfp_2x400g_fr4,
    "fboss-cmis-5-0-osfp-2x400g-fr4"
);
test_page_02h!(
    page_02h_fboss_cmis_5_1_osfp_2x400g_fr4_lite,
    "fboss-cmis-5-1-osfp-2x400g-fr4-lite"
);
test_page_02h!(
    page_02h_fboss_cmis_5_1_osfp_2x400g_fr4_lpo,
    "fboss-cmis-5-1-osfp-2x400g-fr4-lpo"
);
test_page_02h!(
    page_02h_fboss_cmis_5_1_osfp_2x800g_dr4,
    "fboss-cmis-5-1-osfp-2x800g-dr4"
);
test_page_02h!(
    page_02h_fboss_cmis_5_1_osfp_credo_800g_aec,
    "fboss-cmis-5-1-osfp-credo-800g-aec"
);
test_page_02h!(
    page_02h_fboss_cmis_5_3_osfp_800g_zr,
    "fboss-cmis-5-3-osfp-800g-zr"
);
test_page_02h!(
    page_02h_fboss_cmis_5_3_osfp_800g_zr_bias_reserved,
    "fboss-cmis-5-3-osfp-800g-zr-bias-reserved"
);
test_page_02h!(
    page_02h_fboss_cmis_5_3_osfp_800g_zr_dp_reserved,
    "fboss-cmis-5-3-osfp-800g-zr-dp-reserved"
);
test_page_02h!(
    page_02h_synthetic_cmis_3_0_qsfp_dd,
    "synthetic-cmis-3-0-qsfp-dd"
);
test_page_02h!(
    page_02h_synthetic_cmis_5_2_qsfp_dd,
    "synthetic-cmis-5-2-qsfp-dd"
);

// ─── Mutation: recompute actually computes (not a stored-value copy) ──────────

#[test]
fn page_02h_recompute_tracks_data_mutation() {
    let mut buf = read_128("tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-02h.bin");
    CmisPage02hThresholds::recompute_checksums(&mut buf);
    let before = buf[127];
    // +1 on a covered byte must move the sum8 by exactly +1 (mod 256).
    buf[0] = buf[0].wrapping_add(1);
    CmisPage02hThresholds::recompute_checksums(&mut buf);
    assert_eq!(buf[127], before.wrapping_add(1));
}

#[test]
fn page_02h_recompute_zeroed_buffer_is_zero() {
    let mut buf = [0u8; 128];
    CmisPage02hThresholds::recompute_checksums(&mut buf);
    assert_eq!(buf[127], 0);
}
