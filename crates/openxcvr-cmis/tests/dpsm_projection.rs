//! Native tests for the generated revision-bound CMIS DPSM name projection
//! (`CmisDataPathState::for_revision`, schema-owned since Task 2).
//!
//! The expected tables below are an **independent literal oracle** transcribed
//! from the spec tables (CMIS 3.0 § 1.7.7.1 Table 66; CMIS 4.0 Table 8-58;
//! CMIS 5.3 § 8.10.1 / Table 8-84), not values derived from a decoder, the
//! generated table, a raw adapter or runtime case conversion. `None` means
//! "no name for this raw at this revision" — the caller renders its own
//! `Unknown<n>` fallback; it is never the string "Unknown<n>".
//!
//! The 3.0 column of Table 66 names only raws 0–4; raws 5–7 are reserved
//! there and gained names in CMIS 4.0 (`since: "4.0"` in the schema).

use openxcvr_cmis::generated::{CmisDataPathState, CmisDataPathStateProjection};
use openxcvr_cmis::Rev;

/// Expected name per raw nibble at CMIS 3.0 (Table 66).
const V3_0: [Option<&str>; 16] = [
    Some("Reserved0"),
    Some("DpDeactivated"),
    Some("DpInit"),
    Some("DpDeinit"),
    Some("DpActivated"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

/// Expected name per raw nibble at every catalog revision from 4.0 on
/// (Table 8-58 / Table 8-84: 5h–7h named since 4.0).
const V4_0_PLUS: [Option<&str>; 16] = [
    Some("Reserved0"),
    Some("DpDeactivated"),
    Some("DpInit"),
    Some("DpDeinit"),
    Some("DpActivated"),
    Some("DpTxTurnOn"),
    Some("DpTxTurnOff"),
    Some("DpInitialized"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

/// The seven known CMIS context bytes (packed-BCD major/minor).
const CATALOG_BYTES: [u8; 7] = [0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54];

fn oracle(rev: u8) -> [Option<&'static str>; 16] {
    if rev == 0x30 {
        V3_0
    } else {
        V4_0_PLUS
    }
}

/// The factory's support domain must equal `openxcvr_core`'s revision decode
/// for every possible byte — no second whitelist, no wider/narrower set.
#[test]
fn factory_domain_matches_core_revision_decode_for_every_byte() {
    for byte in 0..=255u8 {
        assert_eq!(
            CmisDataPathState::for_revision(byte).is_some(),
            Rev::from_major_byte_cmis(byte).is_some(),
            "factory support at byte {byte:#04x} must match the core revision decode"
        );
    }
}

/// Separately from the core-derived sweep, the bound set is pinned to the
/// literal seven bytes.
#[test]
fn factory_pins_the_literal_seven_byte_catalog() {
    let mut bound = Vec::new();
    for byte in 0..=255u8 {
        if CmisDataPathState::for_revision(byte).is_some() {
            bound.push(byte);
        }
    }
    assert_eq!(bound, CATALOG_BYTES);
}

/// 96 exact-string cases: every catalog revision × every raw nibble 0–15.
#[test]
fn projections_match_the_literal_oracle_for_every_catalog_revision() {
    let mut cases = 0usize;
    for &rev in &CATALOG_BYTES {
        let project = CmisDataPathState::for_revision(rev)
            .unwrap_or_else(|| panic!("catalog byte {rev:#04x} must be bound"));
        for raw in 0..=15u8 {
            assert_eq!(
                project(raw),
                oracle(rev)[raw as usize],
                "name at rev {rev:#04x} raw {raw:#04x}"
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 112, "seven revisions × sixteen raws");
}

/// Unsupported revision contexts refuse — including the explicitly named
/// 3.1/4.1/5.5 neighbors, 0x00 and 0xFF.
#[test]
fn unsupported_contexts_refuse() {
    for byte in [0x31u8, 0x41, 0x55, 0x00, 0xFF] {
        assert!(
            CmisDataPathState::for_revision(byte).is_none(),
            "byte {byte:#04x} is not a known context"
        );
    }
}

/// Unnamed raws: the reserved range 8–15 at every revision, and out-of-enum
/// raws 16 and 255 — the projection never renders `Unknown<n>` itself.
#[test]
fn unnamed_raws_have_no_name() {
    for &rev in &CATALOG_BYTES {
        let project = CmisDataPathState::for_revision(rev).unwrap();
        for raw in [8u8, 9, 10, 11, 12, 13, 14, 15, 16, 255] {
            assert_eq!(
                project(raw),
                None,
                "raw {raw:#04x} must be unnamed at rev {rev:#04x}"
            );
        }
    }
}

/// Two handles for different revisions stay alive and interleave correctly —
/// binding is per-call immutable data, with no shared mutable selection.
#[test]
fn interleaved_handles_keep_independent_selections() {
    let v3 = CmisDataPathState::for_revision(0x30).unwrap();
    let v5 = CmisDataPathState::for_revision(0x53).unwrap();
    for raw in 0..=15u8 {
        assert_eq!(
            v3(raw),
            oracle(0x30)[raw as usize],
            "3.0 handle at {raw:#04x}"
        );
        assert_eq!(
            v5(raw),
            oracle(0x53)[raw as usize],
            "5.3 handle at {raw:#04x}"
        );
        // Interleave again in the same loop body to prove no cross-handle state.
        assert_eq!(v3(raw), oracle(0x30)[raw as usize]);
    }
    // A fresh binding of the same revision agrees with the held one.
    let v3_again = CmisDataPathState::for_revision(0x30).unwrap();
    assert_eq!(v3_again(0x05), None);
    assert_eq!(v5(0x05), Some("DpTxTurnOn"));
}

/// The exact public signature exists: the factory returns the alias type.
#[test]
fn factory_returns_the_projection_alias_type() {
    let bound: Option<CmisDataPathStateProjection> = CmisDataPathState::for_revision(0x51);
    let project = bound.expect("0x51 is a catalog byte");
    assert_eq!(project(0x07), Some("DpInitialized"));
}

/// Existing revision-blind flat semantics are unchanged for all 16 raw
/// nibbles: `From`, `repr`, `name` and the variants keep their pre-projection
/// behavior regardless of revision metadata.
#[test]
fn flat_conversion_and_names_remain_revision_blind() {
    let expected = [
        "Reserved0",
        "DpDeactivated",
        "DpInit",
        "DpDeinit",
        "DpActivated",
        "DpTxTurnOn",
        "DpTxTurnOff",
        "DpInitialized",
        "Unknown8",
        "Unknown9",
        "Unknown10",
        "Unknown11",
        "Unknown12",
        "Unknown13",
        "Unknown14",
        "Unknown15",
    ];
    for raw in 0..=15u8 {
        let value = CmisDataPathState::from(raw);
        assert_eq!(value.repr(), raw, "repr round-trip at {raw:#04x}");
        assert_eq!(
            value.name(),
            expected[raw as usize],
            "flat name at {raw:#04x}"
        );
    }
    // Variants unchanged: named raws map to named variants, reserved raws to
    // the payload catch-all.
    assert_eq!(CmisDataPathState::from(0x00), CmisDataPathState::Reserved0);
    assert_eq!(CmisDataPathState::from(0x05), CmisDataPathState::DpTxTurnOn);
    assert!(matches!(
        CmisDataPathState::from(0x0C),
        CmisDataPathState::Unknown(12)
    ));
    // Out-of-nibble raws stay total through the catch-all.
    assert_eq!(CmisDataPathState::from(16).name(), "Unknown16");
    assert_eq!(CmisDataPathState::from(255).name(), "Unknown255");
}
