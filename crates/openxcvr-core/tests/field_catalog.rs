//! Field-catalog resolution over a hand-built synthetic family table.
//! Synthetic rows only; no schema files, captures or generator output.
use openxcvr_core::field_catalog::{
    resolve, AccessKind, CatalogRow, FamilyCatalog, FieldKind, FieldPresence, FieldQuery,
    InstanceDimension, LaneSelection, RenameRecord, ScaledLsb, Scaling, Unit,
};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{ChangeSource, ReadEffect};

const fn scaled_lsb(signed: bool, num: u32, den: u32, unit: Unit) -> ScaledLsb {
    ScaledLsb {
        signed,
        num,
        den,
        unit,
        note: None,
    }
}

const fn row(name: &'static str) -> CatalogRow {
    CatalogRow {
        name,
        display_name: name,
        page_id: "SyntheticLower",
        page_label: "lower",
        address: 0x50,
        page: 0,
        banked: false,
        offset: 0,
        len: 1,
        bit_offset: 0,
        bit_width: 8,
        kind: FieldKind::U8,
        enum_ref: None,
        scaling: Scaling::None,
        access: AccessKind::Ro,
        read_effects: &[ReadEffect::None],
        change_source: None,
        instances: None,
        since: None,
        until: None,
        renamed_from: None,
        aliased_to: &[],
        tags: &[],
        category: None,
        unit: None,
        discriminator: None,
        presence: FieldPresence::Unconditional,
    }
}

static ROWS: &[CatalogRow] = &[
    CatalogRow {
        name: "temp_mon_value",
        display_name: "TempMonValue",
        offset: 14,
        len: 2,
        bit_width: 16,
        kind: FieldKind::I16Be,
        scaling: Scaling::Scaled(scaled_lsb(true, 1, 256, Unit::Celsius)),
        change_source: Some(ChangeSource::Dynamic),
        category: Some("Telemetry"),
        tags: &["Monitor", "Monitor::Temperature"],
        ..row("temp_mon_value")
    },
    // Renamed across revisions: base name until 4.0, alias at 4.0, base again
    // from 5.0. Three spans, one field.
    CatalogRow {
        name: "mci_max_speed",
        offset: 2,
        bit_offset: 2,
        bit_width: 4,
        kind: FieldKind::U4,
        until: Some(Rev::CMIS_4_0),
        aliased_to: &[
            RenameRecord {
                rev: "4.0",
                name: "twi_max_speed_cmis4",
            },
            RenameRecord {
                rev: "5.0",
                name: "mci_max_speed",
            },
        ],
        ..row("mci_max_speed")
    },
    CatalogRow {
        name: "twi_max_speed_cmis4",
        offset: 2,
        bit_offset: 2,
        bit_width: 4,
        kind: FieldKind::U4,
        since: Some(Rev::CMIS_4_0),
        until: Some(Rev::CMIS_5_0),
        renamed_from: Some(RenameRecord {
            rev: "4.0",
            name: "mci_max_speed",
        }),
        ..row("twi_max_speed_cmis4")
    },
    CatalogRow {
        name: "mci_max_speed",
        offset: 2,
        bit_offset: 2,
        bit_width: 4,
        kind: FieldKind::U4,
        since: Some(Rev::CMIS_5_0),
        ..row("mci_max_speed")
    },
    // Lane-instanced monitor: 8 lanes, 2 bytes each from 154.
    CatalogRow {
        name: "optical_power_tx",
        offset: 154,
        len: 2,
        bit_width: 16,
        kind: FieldKind::U16Be,
        scaling: Scaling::Scaled(scaled_lsb(false, 1, 10_000_000, Unit::Watt)),
        instances: Some(InstanceDimension {
            index: "lane",
            first: 1,
            last: 8,
            offsets: &[154, 156, 158, 160, 162, 164, 166, 168],
            bit_offsets: &[0, 0, 0, 0, 0, 0, 0, 0],
        }),
        tags: &["Monitor", "Monitor::OpticalPower"],
        ..row("optical_power_tx")
    },
    // Clear-on-read flag: resolvable as metadata, refused for decode later.
    CatalogRow {
        name: "temp_mon_high_alarm_flag",
        offset: 8,
        bit_offset: 7,
        bit_width: 1,
        kind: FieldKind::Bool,
        read_effects: &[ReadEffect::ClearOnRead],
        ..row("temp_mon_high_alarm_flag")
    },
    // Introduced at 5.0.
    CatalogRow {
        name: "aux_mon_value",
        offset: 18,
        len: 2,
        bit_width: 16,
        kind: FieldKind::I16Be,
        since: Some(Rev::CMIS_5_0),
        ..row("aux_mon_value")
    },
    // Same name in two pages: exact resolution is ambiguous.
    CatalogRow {
        name: "host_interface_id_app",
        page_id: "SyntheticPage01h",
        page_label: "01h",
        page: 0x01,
        offset: 200,
        len: 1,
        banked: true,
        instances: Some(InstanceDimension {
            index: "app",
            first: 1,
            last: 8,
            offsets: &[200, 201, 202, 203, 204, 205, 206, 207],
            bit_offsets: &[0; 8],
        }),
        ..row("host_interface_id_app")
    },
    CatalogRow {
        name: "host_interface_id_app",
        page_id: "SyntheticPage1Ch",
        page_label: "1Ch",
        page: 0x1c,
        offset: 128,
        len: 1,
        ..row("host_interface_id_app")
    },
];

static CATALOG: FamilyCatalog = FamilyCatalog {
    family: Family::Cmis,
    supported_revs: &[
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ],
    revision_probe: None,
    rows: ROWS,
};

fn query(pattern: &str) -> FieldQuery<'_> {
    FieldQuery {
        pattern,
        lane: LaneSelection::Unspecified,
        page: None,
        bank: 0,
    }
}

#[test]
fn exact_name_hit_returns_location_and_metadata() {
    let entries = resolve(&CATALOG, Rev::CMIS_5_3, &query("temp_mon_value")).unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.name(), "temp_mon_value");
    assert_eq!(entry.row.display_name, "TempMonValue");
    let range = entry.range();
    assert_eq!(
        (
            range.address,
            range.bank,
            range.page,
            range.offset,
            range.len
        ),
        (0x50, 0, 0, 14, 2)
    );
    assert_eq!(entry.offset, 14);
    assert_eq!(entry.bit_offset, 0);
    assert_eq!(entry.width_bits(), 16);
    assert_eq!(entry.row.kind, FieldKind::I16Be);
    assert_eq!(entry.row.access, AccessKind::Ro);
    assert_eq!(entry.row.read_effects, &[ReadEffect::None]);
    assert_eq!(entry.row.change_source, Some(ChangeSource::Dynamic));
    assert_eq!(entry.row.category, Some("Telemetry"));
    assert!(entry.row.tags.contains(&"Monitor::Temperature"));
    match &entry.row.scaling {
        Scaling::Scaled(lsb) => {
            assert!(lsb.signed);
            assert_eq!((lsb.num, lsb.den), (1, 256));
            assert_eq!(lsb.unit, Unit::Celsius);
        }
        other => panic!("expected scaled, got {other:?}"),
    }
    assert_eq!(entry.lane, None);
}

#[test]
fn alias_resolves_only_in_its_revision_span() {
    // Base name at 3.0.
    let at_30 = resolve(&CATALOG, Rev::CMIS_3_0, &query("mci_max_speed")).unwrap();
    assert_eq!(at_30.len(), 1);
    assert_eq!(at_30[0].name(), "mci_max_speed");
    assert_eq!(at_30[0].row.until, Some(Rev::CMIS_4_0));
    // The 4.0 alias.
    let at_40 = resolve(&CATALOG, Rev::CMIS_4_0, &query("twi_max_speed_cmis4")).unwrap();
    assert_eq!(at_40.len(), 1);
    assert_eq!(
        at_40[0].row.renamed_from.map(|r| r.name),
        Some("mci_max_speed")
    );
    // The base name is absent at 4.0 …
    let missing = resolve(&CATALOG, Rev::CMIS_4_0, &query("mci_max_speed"));
    assert!(matches!(
        missing,
        Err(openxcvr_core::field_catalog::CatalogError::NoMatch { .. })
    ));
    // … and the alias is absent at 5.3.
    let missing = resolve(&CATALOG, Rev::CMIS_5_3, &query("twi_max_speed_cmis4"));
    assert!(matches!(
        missing,
        Err(openxcvr_core::field_catalog::CatalogError::NoMatch { .. })
    ));
    // Base name again from 5.0, on a different span row.
    let at_53 = resolve(&CATALOG, Rev::CMIS_5_3, &query("mci_max_speed")).unwrap();
    assert_eq!(at_53.len(), 1);
    assert_eq!(at_53[0].row.since, Some(Rev::CMIS_5_0));
}

#[test]
fn revision_window_hides_fields_before_their_since() {
    let hit = resolve(&CATALOG, Rev::CMIS_5_0, &query("aux_mon_value")).unwrap();
    assert_eq!(hit.len(), 1);
    let miss = resolve(&CATALOG, Rev::CMIS_4_0, &query("aux_mon_value"));
    assert!(matches!(
        miss,
        Err(openxcvr_core::field_catalog::CatalogError::NoMatch { .. })
    ));
}

#[test]
fn unsupported_revision_refuses_before_matching() {
    let future = resolve(&CATALOG, Rev::CMIS_5_3, &query("temp_mon_value"));
    assert_eq!(future.unwrap().len(), 1);
    let unsupported = resolve(
        &CATALOG,
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 4,
        },
        &query("temp_mon_value"),
    );
    assert!(matches!(
        unsupported,
        Err(openxcvr_core::field_catalog::CatalogError::UnsupportedRevision { .. })
    ));
    let wrong_family = resolve(
        &CATALOG,
        Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        },
        &query("temp_mon_value"),
    );
    assert!(matches!(
        wrong_family,
        Err(openxcvr_core::field_catalog::CatalogError::UnsupportedRevision { .. })
    ));
}

#[test]
fn unmatched_glob_is_an_explicit_no_match() {
    let miss = resolve(&CATALOG, Rev::CMIS_5_3, &query("no_such_*_field"));
    assert!(
        matches!(miss, Err(openxcvr_core::field_catalog::CatalogError::NoMatch { ref pattern, .. }) if pattern == "no_such_*_field")
    );
}

#[test]
fn glob_returns_every_match() {
    let entries = resolve(&CATALOG, Rev::CMIS_5_3, &query("*_mon_value")).unwrap();
    let mut names: Vec<_> = entries.iter().map(|e| e.name()).collect();
    names.sort();
    assert_eq!(names, ["aux_mon_value", "temp_mon_value"]);
}

#[test]
fn exact_name_matching_two_rows_is_ambiguous() {
    let err = resolve(&CATALOG, Rev::CMIS_5_3, &query("host_interface_id_app")).unwrap_err();
    match err {
        openxcvr_core::field_catalog::CatalogError::Ambiguous { candidates, .. } => {
            assert_eq!(candidates.len(), 2);
            assert!(candidates.contains(&"SyntheticPage01h::host_interface_id_app".to_string()));
            assert!(candidates.contains(&"SyntheticPage1Ch::host_interface_id_app".to_string()));
        }
        other => panic!("expected ambiguity, got {other:?}"),
    }
}

#[test]
fn page_filter_disambiguates_same_name_rows() {
    let entries = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            pattern: "host_interface_id_app",
            lane: LaneSelection::Unspecified,
            page: Some("SyntheticPage1Ch"),
            bank: 0,
        },
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].row.page, 0x1c);
    // The page label filters too.
    let by_label = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            pattern: "host_interface_id_app",
            lane: LaneSelection::Unspecified,
            page: Some("01h"),
            bank: 0,
        },
    )
    .unwrap();
    assert_eq!(by_label.len(), 1);
    assert_eq!(by_label[0].row.page, 0x01);
}

#[test]
fn lane_all_expands_every_instance_offset() {
    let entries = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::All,
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert_eq!(entries.len(), 8);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry.lane, Some(index as u16 + 1));
        assert_eq!(entry.offset, 154 + 2 * index as u16);
        let range = entry.range();
        assert_eq!(
            (range.page, range.offset, range.len),
            (0u8, (154 + 2 * index as u16) as u8, 2u16)
        );
    }
}

#[test]
fn single_lane_resolves_one_instance() {
    let entries = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::One(3),
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].lane, Some(3));
    assert_eq!(entries[0].offset, 158);
}

#[test]
fn lane_out_of_bounds_is_refused() {
    for bad in [0u16, 9, 99] {
        let err = resolve(
            &CATALOG,
            Rev::CMIS_5_3,
            &FieldQuery {
                lane: LaneSelection::One(bad),
                ..query("optical_power_tx")
            },
        );
        assert!(matches!(
            err,
            Err(openxcvr_core::field_catalog::CatalogError::LaneOutOfRange { lane, first: 1, last: 8, field: "optical_power_tx" }) if lane == bad
        ));
    }
}

#[test]
fn lane_trim_limits_returned_instances() {
    let trimmed = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::UpTo(4),
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert_eq!(trimmed.len(), 4);
    assert_eq!(trimmed[3].lane, Some(4));
    // Trimming beyond the schema dimension clamps, never invents lanes.
    let clamped = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::UpTo(99),
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert_eq!(clamped.len(), 8);
    // Trim to zero yields an empty, non-error result.
    let none = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::UpTo(0),
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert!(none.is_empty());
}

#[test]
fn unspecified_lane_returns_base_instance() {
    let entries = resolve(&CATALOG, Rev::CMIS_5_3, &query("optical_power_tx")).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].lane, None);
    assert_eq!(entries[0].offset, 154);
    assert!(entries[0].row.instances.is_some());
}

#[test]
fn lane_selection_on_uninstanced_field_is_refused() {
    let err = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::One(1),
            ..query("temp_mon_value")
        },
    );
    assert!(matches!(
        err,
        Err(openxcvr_core::field_catalog::CatalogError::NotInstanced {
            field: "temp_mon_value"
        })
    ));
}

#[test]
fn clear_on_read_field_resolves_as_metadata() {
    let entries = resolve(&CATALOG, Rev::CMIS_5_3, &query("temp_mon_high_alarm_flag")).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0]
        .row
        .read_effects
        .contains(&ReadEffect::ClearOnRead));
    assert_eq!(entries[0].bit_offset, 7);
    assert_eq!(entries[0].width_bits(), 1);
}

#[test]
fn banked_page_entries_carry_the_requested_bank() {
    let entries = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &FieldQuery {
            bank: 2,
            page: Some("SyntheticPage01h"),
            ..query("host_interface_id_app")
        },
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].range().bank, 2);
    assert_eq!(entries[0].bank, 2);
}
