//! Checksum-recompute tests for SFF-8472 pages.
//!
//! For each fixture, clone the raw page buffer, call `recompute_checksums`,
//! and assert the recomputed byte equals the byte already stored in the
//! fixture. This proves the generated recompute logic is correct for all
//! real and synthetic SFF-8472 samples in the corpus.

use openxcvr_sff8472::generated::union::{PageA0hBaseMemory, PageA2hDiagnosticsBase};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Read a fixture file and return it as a fixed-size array, zero-padding if
/// the file is shorter than N (matches the read_padded behaviour in mmap.rs).
fn read_padded<const N: usize>(rel: &str) -> [u8; N] {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(
        bytes.len() <= N,
        "{}: file is {} bytes, expected at most {N}",
        path.display(),
        bytes.len()
    );
    let mut arr = [0u8; N];
    arr[..bytes.len()].copy_from_slice(&bytes);
    arr
}

/// Read a fixture file and return it as a fixed-size array (exact size required).
fn read_exact<const N: usize>(rel: &str) -> [u8; N] {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        bytes.len(),
        N,
        "{}: expected {N} bytes, got {}",
        path.display(),
        bytes.len()
    );
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    arr
}

// ─── A0h fixtures: CC_BASE (buf[63]) + CC_EXT (buf[95]) ──────────────────────

macro_rules! test_a0h {
    ($name:ident, $stem:expr) => {
        #[test]
        fn $name() {
            let mut buf: [u8; 256] =
                read_padded(&format!("tests/fixtures/sff-8472/{}.a0h.bin", $stem));
            let cc_base_stored = buf[63];
            let cc_ext_stored = buf[95];
            PageA0hBaseMemory::recompute_checksums(&mut buf);
            assert_eq!(
                buf[63], cc_base_stored,
                "{} a0h CC_BASE: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[63], cc_base_stored
            );
            assert_eq!(
                buf[95], cc_ext_stored,
                "{} a0h CC_EXT: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[95], cc_ext_stored
            );
        }
    };
}

test_a0h!(
    a0h_finisar_ftlx8571d3bcl_mup0wb0,
    "finisar-ftlx8571d3bcl-mup0wb0"
);
test_a0h!(
    a0h_finisar_ftlx8571d3bcl_muq1bzb,
    "finisar-ftlx8571d3bcl-muq1bzb"
);
test_a0h!(
    a0h_synthetic_dac_sfp10g_passive_cu,
    "synthetic-dac-sfp10g-passive-cu"
);
test_a0h!(a0h_synthetic_extcal_sfp10g_sr, "synthetic-extcal-sfp10g-sr");
test_a0h!(a0h_synthetic_sfp28_25g_sr, "synthetic-sfp28-25g-sr");

// ─── A2h fixtures: CHKSUM (buf[95]) ──────────────────────────────────────────

macro_rules! test_a2h {
    ($name:ident, $stem:expr) => {
        #[test]
        fn $name() {
            let mut buf: [u8; 256] =
                read_exact(&format!("tests/fixtures/sff-8472/{}.a2h.bin", $stem));
            let chksum_stored = buf[95];
            PageA2hDiagnosticsBase::recompute_checksums(&mut buf);
            assert_eq!(
                buf[95], chksum_stored,
                "{} a2h CHKSUM: recomputed 0x{:02x} != stored 0x{:02x}",
                $stem, buf[95], chksum_stored
            );
        }
    };
}

test_a2h!(
    a2h_finisar_ftlx8571d3bcl_mup0wb0,
    "finisar-ftlx8571d3bcl-mup0wb0"
);
test_a2h!(
    a2h_finisar_ftlx8571d3bcl_muq1bzb,
    "finisar-ftlx8571d3bcl-muq1bzb"
);
test_a2h!(
    a2h_synthetic_dac_sfp10g_passive_cu,
    "synthetic-dac-sfp10g-passive-cu"
);
test_a2h!(a2h_synthetic_extcal_sfp10g_sr, "synthetic-extcal-sfp10g-sr");
test_a2h!(a2h_synthetic_sfp28_25g_sr, "synthetic-sfp28-25g-sr");
