//! The shared synthetic adapter profiles have to satisfy the properties the whole
//! hardware-free suite rests on, so they are tested rather than assumed.
//!
//! `tests/support/mod.rs` declares the MCP2221A profile and a synthetic
//! non-MCP2221A profile; every transport and host-state test runs against both.
//! These tests hold that pair to plan task 2: the profiles differ in exactly their
//! identity fields, neither carries board control, and neither the data nor the
//! shared support module carries adapter-specific logic.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use support::{
    Field, Profile, DIFFERING_FIELD_NAMES, FIELD_NAMES, MCP2221A, PROFILES, SYNTHETIC_NON_MCP2221A,
};

/// Field names that would mean a profile carries board control. Extend the
/// vocabulary here if a capability name is missing; never add the capability to a
/// profile.
const BOARD_CONTROL_NAMES: [&str; 6] = [
    "gpio",
    "gpio_line",
    "power",
    "power_rail",
    "reset",
    "reset_line",
];

/// The hazards the vocabulary must keep naming, so the disjointness check cannot
/// pass merely because the list stopped mentioning them.
const BOARD_CONTROL_HAZARDS: [&str; 3] = ["gpio", "power", "reset"];

/// The identity fields the suite expects to be the only ones that differ, written
/// out here so widening the manifest in the support module fails these tests.
const EXPECTED_DIFFERING_FIELDS: [&str; 3] = ["vendor_id", "product_id", "kernel_driver"];

fn fields(profile: &Profile) -> BTreeMap<&'static str, Field> {
    profile.fields().into_iter().collect()
}

/// The names of the fields in which two profiles differ, sorted.
fn differing_fields(a: &Profile, b: &Profile) -> Vec<&'static str> {
    let left = fields(a);
    let right = fields(b);
    let mut differing: Vec<&'static str> = left
        .iter()
        .filter(|(name, value)| right.get(*name) != Some(*value))
        .map(|(name, _)| *name)
        .collect();
    differing.sort_unstable();
    differing
}

#[test]
fn profiles_differ_in_exactly_their_identity_fields() {
    for (index, first) in PROFILES.iter().enumerate() {
        for second in &PROFILES[index + 1..] {
            let differing: BTreeSet<&str> = differing_fields(first, second).into_iter().collect();
            assert_eq!(
                differing,
                BTreeSet::from(DIFFERING_FIELD_NAMES),
                "{} and {} must differ only in the identity fields",
                first.kernel_driver,
                second.kernel_driver
            );
        }
    }
    // The manifest itself is the identity triple, not whatever the support module
    // happens to declare: a widened manifest fails here.
    assert_eq!(
        BTreeSet::from(DIFFERING_FIELD_NAMES),
        BTreeSet::from(EXPECTED_DIFFERING_FIELDS)
    );
    assert_ne!(MCP2221A, SYNTHETIC_NON_MCP2221A);
}

#[test]
fn field_manifest_describes_every_profile_field() {
    let manifest: BTreeSet<&str> = FIELD_NAMES.iter().copied().collect();
    assert_eq!(
        manifest.len(),
        FIELD_NAMES.len(),
        "the field manifest repeats a name"
    );
    let enumerated: BTreeSet<&str> = MCP2221A.fields().iter().map(|(name, _)| *name).collect();
    assert_eq!(
        enumerated, manifest,
        "every Profile field must appear in FIELD_NAMES"
    );
}

#[test]
fn mcp2221a_profile_names_the_documented_device() {
    assert_eq!(MCP2221A.vendor_id, 0x04d8);
    assert_eq!(MCP2221A.product_id, 0x00dd);
    assert_eq!(MCP2221A.kernel_driver, "hid_mcp2221");
}

#[test]
fn both_profiles_share_one_transport_shape() {
    let mcp = MCP2221A.shape();
    let synthetic = SYNTHETIC_NON_MCP2221A.shape();
    assert_eq!(
        mcp, synthetic,
        "a shape difference is adapter-specific behaviour"
    );
    // The equality above must not pass on an empty placeholder shape.
    assert_eq!(mcp.hidraw_class, "hidraw");
    assert_eq!(mcp.i2c_adapter_class, "i2c-dev");
}

#[test]
fn neither_profile_carries_board_control() {
    for hazard in BOARD_CONTROL_HAZARDS {
        assert!(
            BOARD_CONTROL_NAMES.contains(&hazard),
            "the board-control vocabulary must keep naming {hazard}"
        );
    }
    for profile in PROFILES {
        for (name, _) in profile.fields() {
            let name = name.to_ascii_lowercase();
            for term in BOARD_CONTROL_NAMES {
                assert!(
                    !name.contains(term),
                    "profile field {name} carries {term}: board control comes from the caller, never from a profile"
                );
            }
        }
    }
}

#[test]
fn shared_profile_support_is_data_without_adapter_logic() {
    // Doc comments are stripped, so the module may explain what it excludes; the
    // remainder is lowered so a capability cannot hide behind its case.
    let source = include_str!("support/mod.rs");
    let code = source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    // Substring, so a compound member name is still caught.
    for term in BOARD_CONTROL_NAMES {
        assert!(
            !code.contains(term),
            "the shared profiles name `{term}`: board control comes from the caller, never from a profile"
        );
    }
    let tokens: BTreeSet<&str> = code
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .collect();
    for keyword in ["if", "match", "while", "loop", "for"] {
        assert!(
            !tokens.contains(keyword),
            "the shared profiles contain `{keyword}`: adapter-dependent behaviour belongs in the test that needs it"
        );
    }
}

#[test]
fn profiles_are_compile_time_synthetic_fixtures() {
    // A profile probed from the host could not be named in a const item; the
    // suite's fixtures stay synthetic and hardware-independent (goal G4).
    const COMPILE_TIME: [Profile; 2] = PROFILES;
    assert_eq!(COMPILE_TIME[0], MCP2221A);
    assert_eq!(COMPILE_TIME[1], SYNTHETIC_NON_MCP2221A);
    assert_eq!(PROFILES, [MCP2221A, SYNTHETIC_NON_MCP2221A]);
    // The second profile is not the first one under another name.
    assert_ne!(SYNTHETIC_NON_MCP2221A.vendor_id, MCP2221A.vendor_id);
    assert_ne!(SYNTHETIC_NON_MCP2221A.product_id, MCP2221A.product_id);
    assert_ne!(SYNTHETIC_NON_MCP2221A.kernel_driver, MCP2221A.kernel_driver);
    assert_ne!(SYNTHETIC_NON_MCP2221A.kernel_driver, "hid_mcp2221");
}

#[cfg(feature = "hid")]
#[test]
fn the_hid_enumeration_filter_takes_identity_from_each_profile() {
    use openxcvr_transports::hid::Filter;

    fn filter_for(profile: &Profile) -> Filter {
        Filter {
            vendor_id: Some(profile.vendor_id),
            product_id: Some(profile.product_id),
            usage_page: None,
            usage: None,
        }
    }

    let mcp = filter_for(&MCP2221A);
    let synthetic = filter_for(&SYNTHETIC_NON_MCP2221A);
    assert_eq!(mcp.vendor_id, Some(0x04d8));
    assert_eq!(mcp.product_id, Some(0x00dd));
    assert_eq!(synthetic.vendor_id, Some(SYNTHETIC_NON_MCP2221A.vendor_id));
    assert_eq!(
        synthetic.product_id,
        Some(SYNTHETIC_NON_MCP2221A.product_id)
    );
    assert_ne!(mcp.vendor_id, synthetic.vendor_id);
    assert_ne!(mcp.product_id, synthetic.product_id);
    // Only identity differs: the filter carries no adapter-specific selection.
    assert_eq!(mcp.usage_page, synthetic.usage_page);
    assert_eq!(mcp.usage, synthetic.usage);
}

#[test]
fn profile_fields_are_pinned_at_compile_time() {
    // No `..` in these patterns: adding any field to `Profile`, board control
    // included, fails to compile here until this test and the manifest are
    // reviewed together. The keyword and vocabulary scans above are best-effort;
    // this is the guarantee.
    for profile in PROFILES {
        let Profile {
            vendor_id: _,
            product_id: _,
            kernel_driver: _,
            hidraw_class: _,
            i2c_adapter_class: _,
        } = profile;
    }
    assert_eq!(FIELD_NAMES.len(), 5);
}
