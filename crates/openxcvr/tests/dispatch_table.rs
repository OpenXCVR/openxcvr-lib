//! Golden table for `dispatch_identifier`, shared with the C binding.
//!
//! The C binding does not link this crate. Both sides read
//! `bindings/c/tests/dispatch_table.txt`. This test rebuilds the table from
//! `openxcvr::unified::dispatch_identifier` and asserts the checked-in file
//! still matches. Regenerate by running the same format and replacing the file
//! only when the dispatcher itself changes.

use openxcvr::unified::{dispatch_identifier, SpecKind};

const REVISIONS: [u8; 7] = [0x00, 0x10, 0x30, 0x3F, 0x40, 0x50, 0xFF];

fn kind_name(kind: SpecKind) -> &'static str {
    match kind {
        SpecKind::Sff8472 => "SFF8472",
        SpecKind::Sff8636 => "SFF8636",
        SpecKind::Cmis => "CMIS",
    }
}

fn render_table() -> String {
    let mut out = String::new();
    for identifier in 0u16..=255 {
        for &revision in &REVISIONS {
            let kind = match dispatch_identifier(identifier as u8, revision) {
                Ok(kind) => kind_name(kind),
                Err(_) => "UNKNOWN",
            };
            out.push_str(&format!("{identifier:02X} {revision:02X} {kind}\n"));
        }
    }
    out
}

#[test]
fn dispatch_table_matches_checked_in_golden() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bindings/c/tests/dispatch_table.txt"
    );
    let expected = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
    let actual = render_table();
    assert_eq!(
        actual, expected,
        "dispatch_table.txt drifted from dispatch_identifier; regenerate from this test's renderer"
    );
}
