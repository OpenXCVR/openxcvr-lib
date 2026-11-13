//! End-to-end catalog resolution and basic-observation decode against the
//! real generated family tables. Synthetic snapshots only; no captures,
//! no I/O. The C parity test mirrors the same snapshots and expectations.
use openxcvr::catalog::{family_catalog, resolve, FieldQuery, LaneSelection};
use openxcvr::observation::{
    observe_basic, observe_field, ObservationError, ObservationState, ObserveOptions,
    ReportedCompliance,
};
use openxcvr_core::field_catalog::CatalogError;
use openxcvr_core::field_catalog::Unit;
use openxcvr_core::observation::FieldValue;
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp, Range,
    RangeObservation, RangeOutcome, ReadEffect, ReadRequester, Snapshot,
};

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(2);

fn stamp() -> ObservationStamp {
    ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    }
}

fn snapshot(parts: &[(u8, u8, u8, u8, Vec<u8>)]) -> Snapshot {
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: parts
            .iter()
            .map(
                |&(address, bank, page, offset, ref bytes)| RangeObservation {
                    range: Range {
                        address,
                        bank,
                        page,
                        offset,
                        len: bytes.len() as u16,
                    },
                    outcome: RangeOutcome::Bytes(bytes.clone()),
                    stamp: stamp(),
                    requester: ReadRequester::PassiveInspection,
                },
            )
            .collect(),
    }
}

fn query(pattern: &str) -> FieldQuery<'_> {
    FieldQuery {
        pattern,
        lane: LaneSelection::Unspecified,
        page: None,
        bank: 0,
    }
}

fn query_all_lanes(pattern: &str) -> FieldQuery<'_> {
    FieldQuery {
        pattern,
        lane: LaneSelection::All,
        page: None,
        bank: 0,
    }
}

fn scaled(observation: &openxcvr_core::observation::FieldObservation) -> (f64, Unit) {
    match observation.value.as_ref() {
        Some(FieldValue::Scaled { value, unit }) => (*value, *unit),
        other => panic!("expected scaled value, got {other:?}"),
    }
}

// ─── Catalog resolution against the real generated tables ──────────────────

#[test]
fn cmis_exact_name_resolves_schema_location() {
    let entries = resolve(Rev::CMIS_5_3, &query("temp_mon_value")).unwrap();
    assert_eq!(entries.len(), 1);
    let range = entries[0].range();
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
    assert_eq!(entries[0].width_bits(), 16);
}

#[test]
fn cmis_schema_alias_resolves_per_revision() {
    // The 4.0 alias of the lower-memory speed field.
    let at_40 = resolve(Rev::CMIS_4_0, &query("twi_max_speed_cmis4")).unwrap();
    assert_eq!(at_40.len(), 1);
    // …which is not the effective name at 5.3.
    let miss = resolve(Rev::CMIS_5_3, &query("twi_max_speed_cmis4"));
    assert!(matches!(miss, Err(CatalogError::NoMatch { .. })));
    let at_53 = resolve(Rev::CMIS_5_3, &query("mci_max_speed")).unwrap();
    assert_eq!(at_53.len(), 1);
    assert_eq!(at_53[0].offset, 2);
    assert_eq!(at_53[0].bit_offset, 2);
}

#[test]
fn cmis_glob_resolution_and_ambiguity() {
    let entries = resolve(Rev::CMIS_5_3, &query("optical_power_*")).unwrap();
    assert!(entries.len() >= 2);
    let names: Vec<_> = entries.iter().map(|e| e.name()).collect();
    assert!(names.contains(&"optical_power_tx"));
    assert!(names.contains(&"optical_power_rx"));

    let unmatched = resolve(Rev::CMIS_5_3, &query("no_such_*"));
    assert!(matches!(unmatched, Err(CatalogError::NoMatch { .. })));

    let ambiguous = resolve(Rev::CMIS_5_3, &query("host_interface_id_app"));
    assert!(matches!(ambiguous, Err(CatalogError::Ambiguous { .. })));
}

#[test]
fn cmis_lane_resolution_bounds() {
    let all = resolve(
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::All,
            ..query("optical_power_tx")
        },
    )
    .unwrap();
    assert_eq!(all.len(), 8);
    assert_eq!(all[7].offset, 168);

    let out_of_bounds = resolve(
        Rev::CMIS_5_3,
        &FieldQuery {
            lane: LaneSelection::One(9),
            ..query("optical_power_tx")
        },
    );
    assert!(matches!(
        out_of_bounds,
        Err(CatalogError::LaneOutOfRange { .. })
    ));
}

#[test]
fn sff8636_and_sff8472_catalogs_resolve() {
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let temperature = resolve(rev, &query("temperature")).unwrap();
    assert_eq!(temperature.len(), 1);
    let range = temperature[0].range();
    assert_eq!(
        (range.address, range.page, range.offset, range.len),
        (0x50, 0, 22, 2)
    );

    let lanes = resolve(
        rev,
        &FieldQuery {
            lane: LaneSelection::All,
            ..query("rx_power_lane")
        },
    )
    .unwrap();
    assert_eq!(lanes.len(), 4);
    assert_eq!(lanes[3].offset, 40);

    let rev8472 = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let rx = resolve(rev8472, &query("rx_power")).unwrap();
    assert_eq!(rx.len(), 1);
    let range = rx[0].range();
    assert_eq!(
        (range.address, range.page, range.offset, range.len),
        (0x51, 0, 104, 2)
    );
}

#[test]
fn sff8636_legacy_windows_resolve_per_revision() {
    // The tier-2 legacy SFF-8079 application-select rows (lower bytes
    // 89-97) resolve before Rev 2.11 and are gone from 2.11 on, where the
    // reserved placeholders take over; the family domain spans 1.3-2.12.
    let at = |major, minor| Rev {
        family: Family::Sff8636,
        major,
        minor,
    };
    for rev in [at(1, 3), at(1, 5), at(2, 0), at(2, 8)] {
        let rows = resolve(rev, &query_all_lanes("sff8079_rx_application_select")).unwrap();
        assert_eq!(rows.len(), 4, "lane rows at {}", rev.as_str());
        let rows = resolve(rev, &query_all_lanes("sff8079_tx_application_select")).unwrap();
        assert_eq!(rows.len(), 4, "tx lane rows at {}", rev.as_str());
        // The reserved placeholders must not double-cover those bytes: a
        // resolve names NoMatch when the revision window excludes the only
        // candidate rows.
        assert!(matches!(
            resolve(rev, &query("reserved_lower_89_92")),
            Err(CatalogError::NoMatch { .. })
        ));
    }
    for rev in [at(2, 11), at(2, 12)] {
        assert!(matches!(
            resolve(rev, &query_all_lanes("sff8079_rx_application_select")),
            Err(CatalogError::NoMatch { .. })
        ));
        let rows = resolve(rev, &query("reserved_lower_89_92")).unwrap();
        assert_eq!(rows.len(), 1);
    }
}

#[test]
fn family_catalog_table_lookup() {
    assert_eq!(
        family_catalog(Family::Cmis).map(|c| c.rows.len()),
        Some(openxcvr_cmis::generated::catalog::CATALOG.rows.len())
    );
    assert!(family_catalog(Family::Cmis).unwrap().rows.len() > 500);
    // Census from the regenerated per-rev span catalogs (see
    // crates/openxcvr-sff8636/src/generated/catalog.rs: one row per field
    // revision span, including the tier-2 legacy SFF-8079 rows).
    assert_eq!(family_catalog(Family::Sff8636).unwrap().rows.len(), 252);
    assert_eq!(family_catalog(Family::Sff8472).unwrap().rows.len(), 155);
    assert!(family_catalog(Family::Elsfp).is_none());
}

// ─── Basic observations: CMIS ───────────────────────────────────────────────

fn cmis_snapshot(
    revision_byte: u8,
    with_page_11h: bool,
    page_01h_byte_160: Option<u8>,
) -> Snapshot {
    let mut lower = vec![0u8; 18];
    lower[1] = revision_byte;
    // temp_mon_value at 14: 16.0 degC; vcc at 16: 3.3 V.
    lower[14..16].copy_from_slice(&0x1000u16.to_be_bytes());
    lower[16..18].copy_from_slice(&33000u16.to_be_bytes());
    let mut parts = vec![(0x50u8, 0u8, 0u8, 0u8, lower)];
    if with_page_11h {
        let mut page11 = vec![0u8; 128];
        for lane in 0..8u16 {
            // OpticalPowerTx / LaserBiasTx / OpticalPowerRx per lane.
            page11[154 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(1000 + lane).to_be_bytes());
            page11[170 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(2500 + 10 * lane).to_be_bytes());
            page11[186 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(3000 + lane).to_be_bytes());
        }
        parts.push((0x50, 0, 0x11, 128, page11));
    }
    if let Some(byte) = page_01h_byte_160 {
        parts.push((0x50, 0, 0x01, 160, vec![byte]));
    }
    snapshot(&parts)
}

#[test]
fn cmis_basic_observations_two_revisions() {
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let byte = match revision {
            Rev::CMIS_4_0 => 0x40,
            Rev::CMIS_5_3 => 0x53,
            _ => unreachable!(),
        };
        let view = cmis_snapshot(byte, true, Some(0x00));
        let observed = observe_basic(
            Family::Cmis,
            revision,
            &view,
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap();
        assert_eq!(observed.revision, revision, "revision used");

        assert_eq!(observed.temperature.state, ObservationState::Valid);
        let (value, unit) = scaled(&observed.temperature);
        assert!((value - 16.0).abs() < 1e-9, "value {value}");
        assert_eq!(unit, Unit::Celsius);
        assert_eq!(observed.temperature.field, "temp_mon_value");

        assert_eq!(observed.supply_voltage.state, ObservationState::Valid);
        let (value, unit) = scaled(&observed.supply_voltage);
        assert!((value - 3.3).abs() < 1e-9, "value {value}");
        assert_eq!(unit, Unit::Volt);
        assert_eq!(observed.supply_voltage.field, "vcc_mon_voltage");

        assert_eq!(observed.lanes.len(), 8);
        for (lane, observed_lane) in observed.lanes.iter().enumerate() {
            assert_eq!(observed_lane.lane, lane as u16 + 1);
            let (bias, unit) = scaled(&observed_lane.tx_bias);
            // Base 2 uA LSB; multiplier advertisement reads x1 here.
            let expected = (2500 + 10 * lane as u32) as f64 * 2.0e-6;
            assert!((bias - expected).abs() < 1e-12, "bias {bias}");
            assert_eq!(unit, Unit::Ampere);
            let (tx, unit) = scaled(&observed_lane.tx_power);
            let expected = (1000 + lane as u32) as f64 * 1.0e-7;
            assert!((tx - expected).abs() < 1e-12, "tx {tx}");
            assert_eq!(unit, Unit::Watt);
            let (rx, unit) = scaled(&observed_lane.rx_power);
            let expected = (3000 + lane as u32) as f64 * 1.0e-7;
            assert!((rx - expected).abs() < 1e-12, "rx {rx}");
            assert_eq!(unit, Unit::Watt);
        }
        assert_eq!(observed.lanes[0].tx_bias.field, "laser_bias_tx");
        assert_eq!(observed.lanes[0].tx_power.field, "optical_power_tx");
        assert_eq!(observed.lanes[0].rx_power.field, "optical_power_rx");
    }
}

#[test]
fn cmis_bias_multiplier_advertisement_is_applied() {
    // Page 01h:160 bits 4:3 = 01b -> multiply x2.
    let view = cmis_snapshot(0x53, true, Some(0x08));
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    let (bias, _) = scaled(&observed.lanes[0].tx_bias);
    let expected = 2500.0 * 2.0e-6 * 2.0;
    assert!((bias - expected).abs() < 1e-12, "bias {bias}");
    assert!(observed.lanes[0].tx_bias.note.is_none());

    // Advertisement absent: no scaled value, the insufficient-input /
    // not-supplied representation. The factor is x1/x2/x4, so an assumed
    // x1 could be off by 4x.
    let view = cmis_snapshot(0x53, true, None);
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        observed.lanes[0].tx_bias.state,
        ObservationState::NotSupplied
    );
    assert!(observed.lanes[0].tx_bias.value.is_none());
    assert!(observed.lanes[0].tx_bias.note.is_some());

    // Advertisement x4 (page 01h:160 bits 4:3 = 10b).
    let view = cmis_snapshot(0x53, true, Some(0x10));
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    let (bias, _) = scaled(&observed.lanes[0].tx_bias);
    let expected = 2500.0 * 2.0e-6 * 4.0;
    assert!((bias - expected).abs() < 1e-12, "bias {bias}");
    assert!(observed.lanes[0].tx_bias.note.is_none());

    // Reserved multiplier code 11b: no value, invalid state.
    let view = cmis_snapshot(0x53, true, Some(0x18));
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.lanes[0].tx_bias.state, ObservationState::Invalid);
    assert!(observed.lanes[0].tx_bias.value.is_none());
}

#[test]
fn cmis_absent_optional_page_leaves_lane_monitors_not_supplied() {
    let view = cmis_snapshot(0x53, false, None);
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.temperature.state, ObservationState::Valid);
    assert_eq!(observed.supply_voltage.state, ObservationState::Valid);
    for lane in &observed.lanes {
        assert_eq!(lane.tx_bias.state, ObservationState::NotSupplied);
        assert!(lane.tx_bias.value.is_none());
        assert_eq!(lane.tx_power.state, ObservationState::NotSupplied);
        assert_eq!(lane.rx_power.state, ObservationState::NotSupplied);
    }
}

#[test]
fn cmis_revision_byte_governs_and_refuses() {
    // Unsupported future revision byte.
    let view = cmis_snapshot(0x55, true, None);
    let err = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(err, ObservationError::UnsupportedRevision { .. }));

    // Requested revision disagreeing with the byte.
    let view = cmis_snapshot(0x40, true, None);
    let err = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(err, ObservationError::RevisionMismatch { .. }));

    // Missing revision byte: bytes 2.. supplied, byte 1 absent.
    let view = snapshot(&[(0x50, 0, 0, 2, vec![0u8; 16])]);
    let err = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(err, ObservationError::MissingRevisionByte));
}

// ─── Basic observations: SFF-8636 ───────────────────────────────────────────

const SFF8636_REV_2_12: Rev = Rev {
    family: Family::Sff8636,
    major: 2,
    minor: 12,
};

fn sff8636_snapshot(compliance: u8) -> Snapshot {
    let mut lower = vec![0u8; 128];
    lower[1] = compliance;
    lower[22..24].copy_from_slice(&(-768i16).to_be_bytes()); // -3.0 degC
    lower[26..28].copy_from_slice(&33000u16.to_be_bytes()); // 3.3 V
    for lane in 0..4u16 {
        let lane = usize::from(lane);
        lower[34 + 2 * lane..][..2].copy_from_slice(&(3000 + lane as u16).to_be_bytes());
        lower[42 + 2 * lane..][..2].copy_from_slice(&(2500 + 10 * lane as u16).to_be_bytes());
        lower[50 + 2 * lane..][..2].copy_from_slice(&(1000 + lane as u16).to_be_bytes());
    }
    snapshot(&[(0x50, 0, 0, 0, lower)])
}

#[test]
fn sff8636_basic_observations_four_lanes() {
    let view = sff8636_snapshot(0x0a); // Rev 2.12: the schema's own code
    let rev = SFF8636_REV_2_12;
    let observed = observe_basic(
        Family::Sff8636,
        rev,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.revision, rev);
    let (value, unit) = scaled(&observed.temperature);
    assert!((value - (-3.0)).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
    let (value, unit) = scaled(&observed.supply_voltage);
    assert!((value - 3.3).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Volt);
    assert_eq!(observed.lanes.len(), 4);
    let (rx, unit) = scaled(&observed.lanes[0].rx_power);
    assert!((rx - 3000.0 * 1.0e-7).abs() < 1e-12, "rx {rx}");
    assert_eq!(unit, Unit::Watt);
    let (bias, _) = scaled(&observed.lanes[0].tx_bias);
    assert!((bias - 2500.0 * 2.0e-6).abs() < 1e-12, "bias {bias}");
    let (tx, _) = scaled(&observed.lanes[0].tx_power);
    assert!((tx - 1000.0 * 1.0e-7).abs() < 1e-12, "tx {tx}");

    // Unsupported revision for the family.
    let bad = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 10,
    };
    let err = observe_basic(
        Family::Sff8636,
        bad,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    );
    assert!(matches!(
        err,
        Err(ObservationError::UnsupportedRevision { .. })
    ));
}

#[test]
fn sff8636_compliance_gate_admits_only_the_schema_revision() {
    // SFF-8636 Rev 2.12 § 6.2.2 Table 6-4: only 0Ah names Rev 2.12, the
    // revision the schema decodes. 09h names Rev 2.11 (a named code, not
    // this schema's revision); 08h/07h name earlier revisions; 00h is
    // unspecified; 0Bh..FFh are reserved.
    let admitted = observe_basic(
        Family::Sff8636,
        SFF8636_REV_2_12,
        &sff8636_snapshot(0x0a),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(admitted.temperature.state, ObservationState::Valid);
    assert_eq!(admitted.lanes.len(), 4);
    // The result carries the reported compliance (code and meaning) and
    // the decoder's schema revision.
    assert_eq!(admitted.revision, SFF8636_REV_2_12);
    assert_eq!(
        admitted.reported_compliance,
        Some(ReportedCompliance {
            code: 0x0a,
            meaning: "SFF-8636 Rev 2.12",
        })
    );

    for code in [0x08, 0x07, 0x00, 0x09, 0x0b, 0xff] {
        let observed = observe_basic(
            Family::Sff8636,
            SFF8636_REV_2_12,
            &sff8636_snapshot(code),
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap_or_else(|e| panic!("code {code:#04x}: {e:?}"));
        // Refusals still report what the device said.
        assert_eq!(observed.revision, SFF8636_REV_2_12, "code {code:#04x}");
        assert!(observed.reported_compliance.is_some(), "code {code:#04x}");
        assert_eq!(
            observed.reported_compliance.map(|c| c.code),
            Some(code),
            "code {code:#04x}"
        );
        assert_eq!(
            observed.temperature.state,
            ObservationState::UnsupportedRevision(SFF8636_REV_2_12),
            "code {code:#04x}"
        );
        assert!(observed.temperature.value.is_none(), "code {code:#04x}");
        assert_eq!(
            observed.supply_voltage.state,
            ObservationState::UnsupportedRevision(SFF8636_REV_2_12),
            "code {code:#04x}"
        );
        assert!(observed.supply_voltage.value.is_none(), "code {code:#04x}");
        assert_eq!(observed.lanes.len(), 4, "code {code:#04x}");
        for lane in &observed.lanes {
            assert_eq!(
                lane.tx_bias.state,
                ObservationState::UnsupportedRevision(SFF8636_REV_2_12),
                "code {code:#04x}"
            );
            assert!(lane.tx_bias.value.is_none(), "code {code:#04x}");
            assert_eq!(
                lane.tx_power.state,
                ObservationState::UnsupportedRevision(SFF8636_REV_2_12),
                "code {code:#04x}"
            );
            assert!(lane.tx_power.value.is_none(), "code {code:#04x}");
            assert_eq!(
                lane.rx_power.state,
                ObservationState::UnsupportedRevision(SFF8636_REV_2_12),
                "code {code:#04x}"
            );
            assert!(lane.rx_power.value.is_none(), "code {code:#04x}");
        }
    }
}

#[test]
fn sff8636_missing_compliance_byte_leaves_revision_undetermined() {
    // Lower page supplied from byte 2: the compliance byte is absent, so
    // applicability to the schema revision is undetermined — refused with
    // no value, never a decode on an assumed revision.
    let mut lower = vec![0u8; 127];
    lower[21..23].copy_from_slice(&(-768i16).to_be_bytes());
    let view = snapshot(&[(0x50, 0, 0, 2, lower)]);
    let observed = observe_basic(
        Family::Sff8636,
        SFF8636_REV_2_12,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        observed.temperature.state,
        ObservationState::UnsupportedRevision(SFF8636_REV_2_12)
    );
    assert!(observed.temperature.value.is_none());
    assert!(observed.temperature.note.is_some());
    assert_eq!(
        observed.lanes[0].tx_bias.state,
        ObservationState::UnsupportedRevision(SFF8636_REV_2_12)
    );
    assert!(observed.lanes[0].tx_bias.value.is_none());
    // Nothing is reported when the compliance byte is not supplied.
    assert_eq!(observed.reported_compliance, None);
}

#[test]
fn observation_result_carries_reported_compliance_and_schema_revision() {
    // SFF-8636: refused 08h still reports the code and its meaning.
    let observed = observe_basic(
        Family::Sff8636,
        SFF8636_REV_2_12,
        &sff8636_snapshot(0x08),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.revision, SFF8636_REV_2_12);
    assert_eq!(
        observed.reported_compliance,
        Some(ReportedCompliance {
            code: 0x08,
            meaning: "SFF-8636 Rev 2.8, 2.9, and 2.10",
        })
    );

    // Reserved codes report the reserved meaning.
    let observed = observe_basic(
        Family::Sff8636,
        SFF8636_REV_2_12,
        &sff8636_snapshot(0x0b),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        observed.reported_compliance,
        Some(ReportedCompliance {
            code: 0x0b,
            meaning: "Reserved",
        })
    );

    // SFF-8472: admitted 0Ah reports Rev 12.5, the revision the 12.5a
    // schema implements.
    let observed = observe_basic(
        Family::Sff8472,
        SFF8472_REV_12_5,
        &sff8472_snapshot(0x40, false),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.revision, SFF8472_REV_12_5);
    assert_eq!(
        observed.reported_compliance,
        Some(ReportedCompliance {
            code: 0x0a,
            meaning: "Complies with SFF-8472 Rev 12.5.",
        })
    );

    // CMIS has no compliance code: its revision is byte-proven in
    // `revision` and reported_compliance stays None.
    let observed = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &cmis_snapshot(0x53, true, Some(0x00)),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.revision, Rev::CMIS_5_3);
    assert_eq!(observed.reported_compliance, None);
}

#[test]
fn sff_revision_gate_applies_on_the_field_path() {
    // resolve + observe_field cannot bypass the compliance gate.
    let temperature = resolve(SFF8636_REV_2_12, &query("temperature"))
        .unwrap()
        .remove(0);
    let refused = observe_field(
        &temperature,
        &sff8636_snapshot(0x08),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(
        refused.state,
        ObservationState::UnsupportedRevision(SFF8636_REV_2_12)
    );
    assert!(refused.value.is_none());
    assert_eq!(refused.note, Some("SFF-8636 Rev 2.8, 2.9, and 2.10"));

    let admitted = observe_field(
        &temperature,
        &sff8636_snapshot(0x0a),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(admitted.state, ObservationState::Valid);
    let (value, unit) = scaled(&admitted);
    assert!((value - (-3.0)).abs() < 1e-9);
    assert_eq!(unit, Unit::Celsius);
}

#[test]
fn cmis_bias_rule_applies_on_the_field_path() {
    // The generic field path applies the same TxBiasCurrentScalingFactor
    // rule as observe_basic: x2/x4 scale, a missing advertisement yields
    // no scaled value, a reserved code is invalid.
    let bias = resolve(
        Rev::CMIS_5_3,
        &FieldQuery {
            pattern: "laser_bias_tx",
            lane: LaneSelection::One(1),
            page: Some("CmisPage11hLaneStatus"),
            bank: 0,
        },
    )
    .unwrap()
    .remove(0);

    let observed = observe_field(
        &bias,
        &cmis_snapshot(0x53, true, Some(0x08)),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(observed.state, ObservationState::Valid);
    let (value, unit) = scaled(&observed);
    assert!((value - 2500.0 * 2.0e-6 * 2.0).abs() < 1e-12, "x2 {value}");
    assert_eq!(unit, Unit::Ampere);
    assert!(observed.note.is_none());

    let observed = observe_field(
        &bias,
        &cmis_snapshot(0x53, true, Some(0x10)),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    let (value, _) = scaled(&observed);
    assert!((value - 2500.0 * 2.0e-6 * 4.0).abs() < 1e-12, "x4 {value}");

    let observed = observe_field(
        &bias,
        &cmis_snapshot(0x53, true, None),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(observed.state, ObservationState::NotSupplied);
    assert!(observed.value.is_none());
    assert!(observed.note.is_some());

    let observed = observe_field(
        &bias,
        &cmis_snapshot(0x53, true, Some(0x18)),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(observed.state, ObservationState::Invalid);
    assert!(observed.value.is_none());
}

// ─── Basic observations: SFF-8472 ───────────────────────────────────────────

fn sff8472_snapshot(dmt_byte: u8, with_constants: bool) -> Snapshot {
    let mut a0 = vec![0u8; 96];
    a0[92] = dmt_byte;
    // A0h byte 94 = 0Ah: compliance with SFF-8472 Rev 12.5, which the
    // 12.5a schema implements (SFF-8472 Rev 12.5a § 8.11 Table 8-8).
    a0[94] = 0x0a;
    // Without constants the A2h supply starts at byte 92, so bytes 56-91
    // (the calibration constants) are genuinely absent from the snapshot.
    let a2_offset = if with_constants { 0u8 } else { 92u8 };
    let mut a2 = vec![0u8; 120 - usize::from(a2_offset)];
    let page = |offset: usize| offset - usize::from(a2_offset);
    // Internal-calibration raw values at 96-105 (page-relative indices).
    a2[page(96)..page(98)].copy_from_slice(&(-768i16).to_be_bytes()); // -3.0 degC
    a2[page(98)..page(100)].copy_from_slice(&33000u16.to_be_bytes()); // 3.3 V
    a2[page(100)..page(102)].copy_from_slice(&2500u16.to_be_bytes()); // 5 mA
    a2[page(102)..page(104)].copy_from_slice(&1000u16.to_be_bytes()); // 0.1 mW
    a2[page(104)..page(106)].copy_from_slice(&3000u16.to_be_bytes()); // 0.3 mW
    if with_constants {
        // External constants: slope 2.0 / offset -256 for temperature
        // (parameter LSB units), identity for the rest, degree-1
        // coefficient 1.0 for Rx.
        a2[76..78].copy_from_slice(&0x0100u16.to_be_bytes()); // tx_bias slope 1.0
        a2[78..80].copy_from_slice(&0i16.to_be_bytes());
        a2[80..82].copy_from_slice(&0x0100u16.to_be_bytes()); // tx_power slope 1.0
        a2[82..84].copy_from_slice(&0i16.to_be_bytes());
        a2[84..86].copy_from_slice(&0x0200u16.to_be_bytes()); // temp slope 2.0
        a2[86..88].copy_from_slice(&(-256i16).to_be_bytes()); // temp offset
        a2[88..90].copy_from_slice(&0x0100u16.to_be_bytes()); // vcc slope 1.0
        a2[90..92].copy_from_slice(&0i16.to_be_bytes());
        a2[68..72].copy_from_slice(&1.0f32.to_be_bytes()); // Rx degree 1
    }
    snapshot(&[(0x50, 0, 0, 0, a0), (0x51, 0, 0, a2_offset, a2)])
}

#[test]
fn sff8472_internal_calibration_single_lane() {
    let view = sff8472_snapshot(0x40, false); // bit 6 DDM + bit 5 internal
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let observed = observe_basic(
        Family::Sff8472,
        rev,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.revision, rev);
    let (value, unit) = scaled(&observed.temperature);
    assert!((value - (-3.0)).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
    let (value, unit) = scaled(&observed.supply_voltage);
    assert!((value - 3.3).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Volt);
    assert_eq!(observed.lanes.len(), 1);
    assert_eq!(observed.lanes[0].lane, 1);
    let (bias, unit) = scaled(&observed.lanes[0].tx_bias);
    assert!((bias - 5.0e-3).abs() < 1e-12, "bias {bias}");
    assert_eq!(unit, Unit::Ampere);
    let (tx, unit) = scaled(&observed.lanes[0].tx_power);
    assert!((tx - 1.0e-4).abs() < 1e-12, "tx {tx}");
    assert_eq!(unit, Unit::Watt);
    let (rx, unit) = scaled(&observed.lanes[0].rx_power);
    assert!((rx - 3.0e-4).abs() < 1e-12, "rx {rx}");
    assert_eq!(unit, Unit::Watt);
}

#[test]
fn sff8472_external_calibration_with_constants() {
    let view = sff8472_snapshot(0x10, true); // bit 4 external
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let observed = observe_basic(
        Family::Sff8472,
        rev,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    // Temperature: (2.0 * -768 + -256) = -1792 LSBs of 1/256 degC -> -7.0.
    let (value, unit) = scaled(&observed.temperature);
    assert!((value - (-7.0)).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
    // Voltage: identity slope -> 3.3 V.
    let (value, _) = scaled(&observed.supply_voltage);
    assert!((value - 3.3).abs() < 1e-9, "value {value}");
    // Bias: identity -> 5 mA. Power: identity -> 0.1 mW. Rx poly: 1.0*raw -> 0.3 mW.
    let (bias, _) = scaled(&observed.lanes[0].tx_bias);
    assert!((bias - 5.0e-3).abs() < 1e-12, "bias {bias}");
    let (tx, _) = scaled(&observed.lanes[0].tx_power);
    assert!((tx - 1.0e-4).abs() < 1e-12, "tx {tx}");
    let (rx, _) = scaled(&observed.lanes[0].rx_power);
    assert!((rx - 3.0e-4).abs() < 1e-12, "rx {rx}");
}

#[test]
fn sff8472_external_calibration_without_constants_is_uncalibrated() {
    let view = sff8472_snapshot(0x10, false); // external bit set, no constants
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let observed = observe_basic(
        Family::Sff8472,
        rev,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.temperature.state, ObservationState::Uncalibrated);
    assert!(observed.temperature.value.is_none());
    assert_eq!(
        observed.supply_voltage.state,
        ObservationState::Uncalibrated
    );
    assert_eq!(
        observed.lanes[0].tx_bias.state,
        ObservationState::Uncalibrated
    );
    assert_eq!(
        observed.lanes[0].tx_power.state,
        ObservationState::Uncalibrated
    );
    assert_eq!(
        observed.lanes[0].rx_power.state,
        ObservationState::Uncalibrated
    );
}

#[test]
fn sff8472_missing_a2h_page_is_not_supplied() {
    let mut a0 = vec![0u8; 96];
    a0[92] = 0x40; // internal calibration
    a0[94] = 0x0a; // Rev 12.5 compliance
    let view = snapshot(&[(0x50, 0, 0, 0, a0)]);
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let observed = observe_basic(
        Family::Sff8472,
        rev,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.temperature.state, ObservationState::NotSupplied);
    assert!(observed.temperature.value.is_none());
    assert_eq!(
        observed.lanes[0].rx_power.state,
        ObservationState::NotSupplied
    );
}

const SFF8472_REV_12_5: Rev = Rev {
    family: Family::Sff8472,
    major: 12,
    minor: 5,
};

#[test]
fn sff8472_compliance_gate_admits_only_the_schema_revision() {
    // SFF-8472 Rev 12.5a § 8.11 Table 8-8: 0Ah names Rev 12.5 (which the
    // 12.5a schema implements); 09h/08h name earlier revisions; 00h is
    // unspecified; 0Bh..FFh are reserved.
    let admitted = observe_basic(
        Family::Sff8472,
        SFF8472_REV_12_5,
        &sff8472_snapshot(0x40, false),
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(admitted.temperature.state, ObservationState::Valid);

    for code in [0x09, 0x08, 0x00, 0x0b, 0xff] {
        let mut snapshot = sff8472_snapshot(0x40, false);
        // Rewrite byte 94 (the last byte of the first supplied range).
        snapshot.ranges[0].outcome = RangeOutcome::Bytes({
            let mut a0 = vec![0u8; 96];
            a0[92] = 0x40;
            a0[94] = code;
            a0
        });
        let observed = observe_basic(
            Family::Sff8472,
            SFF8472_REV_12_5,
            &snapshot,
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap_or_else(|e| panic!("code {code:#04x}: {e:?}"));
        assert_eq!(
            observed.temperature.state,
            ObservationState::UnsupportedRevision(SFF8472_REV_12_5),
            "code {code:#04x}"
        );
        assert!(observed.temperature.value.is_none(), "code {code:#04x}");
        assert_eq!(
            observed.supply_voltage.state,
            ObservationState::UnsupportedRevision(SFF8472_REV_12_5),
            "code {code:#04x}"
        );
        assert_eq!(
            observed.lanes[0].tx_bias.state,
            ObservationState::UnsupportedRevision(SFF8472_REV_12_5),
            "code {code:#04x}"
        );
        assert!(
            observed.lanes[0].tx_bias.value.is_none(),
            "code {code:#04x}"
        );
        assert_eq!(
            observed.lanes[0].tx_power.state,
            ObservationState::UnsupportedRevision(SFF8472_REV_12_5),
            "code {code:#04x}"
        );
        assert_eq!(
            observed.lanes[0].rx_power.state,
            ObservationState::UnsupportedRevision(SFF8472_REV_12_5),
            "code {code:#04x}"
        );
    }
}

#[test]
fn sff8472_missing_compliance_byte_leaves_revision_undetermined() {
    // A0h supplied only through byte 93: byte 94 is absent, so the module's
    // compliance — and with it the decode revision — is undetermined.
    let mut a0 = vec![0u8; 94];
    a0[92] = 0x40;
    let view = snapshot(&[(0x50, 0, 0, 0, a0), (0x51, 0, 0, 92, vec![0u8; 28])]);
    let observed = observe_basic(
        Family::Sff8472,
        SFF8472_REV_12_5,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        observed.temperature.state,
        ObservationState::UnsupportedRevision(SFF8472_REV_12_5)
    );
    assert!(observed.temperature.value.is_none());
    assert!(observed.temperature.note.is_some());
    assert_eq!(
        observed.lanes[0].rx_power.state,
        ObservationState::UnsupportedRevision(SFF8472_REV_12_5)
    );
    assert!(observed.lanes[0].rx_power.value.is_none());
}

#[test]
fn sff8472_revision_gate_applies_on_the_field_path() {
    let rx = resolve(SFF8472_REV_12_5, &query("rx_power"))
        .unwrap()
        .remove(0);

    let mut snapshot = sff8472_snapshot(0x40, false);
    snapshot.ranges[0].outcome = RangeOutcome::Bytes({
        let mut a0 = vec![0u8; 96];
        a0[92] = 0x40;
        a0[94] = 0x09;
        a0
    });
    let refused = observe_field(
        &rx,
        &snapshot,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(
        refused.state,
        ObservationState::UnsupportedRevision(SFF8472_REV_12_5)
    );
    assert!(refused.value.is_none());
    assert_eq!(refused.note, Some("Complies with SFF-8472 Rev 12.4."));

    let admitted = observe_field(
        &rx,
        &sff8472_snapshot(0x40, false),
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(admitted.state, ObservationState::Valid);
    let (value, unit) = scaled(&admitted);
    assert!((value - 3.0e-4).abs() < 1e-12);
    assert_eq!(unit, Unit::Watt);
}

#[test]
fn cmis_clear_on_read_field_refused_then_authorized() {
    use openxcvr::observation::{observe_field, FieldValue as ObsValue, ObservationState as State};
    let mut lower = vec![0u8; 18];
    lower[1] = 0x53;
    lower[9] = 0x01; // TempMonHighAlarmFlag (byte 9, bit 0)
    let view = snapshot(&[(0x50, 0, 0, 0, lower)]);
    let entries = resolve(Rev::CMIS_5_3, &query("temp_mon_high_alarm_flag")).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].offset, 9);
    assert!(entries[0]
        .row
        .read_effects
        .contains(&ReadEffect::ClearOnRead));

    let refused = observe_field(
        &entries[0],
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(refused.state, State::ClearOnReadRefused);
    assert!(refused.value.is_none());

    let authorized = observe_field(
        &entries[0],
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions {
            authorize_clear_on_read: true,
        },
    );
    assert_eq!(authorized.state, State::Valid);
    assert_eq!(authorized.value, Some(ObsValue::Raw(1)));
}

#[test]
fn snapshot_length_bounds_are_refused_not_wrapped() {
    // A supplied range past the 256-byte window is invalid, not decoded.
    let view = snapshot(&[(0x50, 0, 0, 254, vec![0x53, 0x53, 0x00])]);
    let err = observe_basic(
        Family::Cmis,
        Rev::CMIS_5_3,
        &view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    );
    assert!(matches!(err, Err(ObservationError::Input(_))));
}

/// Conversion consumes the already-calibrated W observation, not raw codes.
#[test]
fn optical_dbm_after_external_calibration() {
    use openxcvr::observation::{optical_power_dbm, OpticalDbmValue};
    for with_constants in [false, true] {
        let view = sff8472_snapshot(0x10, with_constants);
        let observed = observe_basic(
            Family::Sff8472,
            SFF8472_REV_12_5,
            &view,
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap();
        for (source, expected) in [
            (&observed.lanes[0].tx_power, -10.0),
            (&observed.lanes[0].rx_power, -5.228787452803376),
        ] {
            let result = optical_power_dbm(source);
            assert!(std::ptr::eq(result.source, source));
            if with_constants {
                assert_eq!(result.state, ObservationState::Valid);
                let OpticalDbmValue::Value(dbm) = result.value else {
                    panic!("expected dBm")
                };
                assert!((dbm - expected).abs() < 1e-10);
                assert_eq!(scaled(result.source).1, Unit::Watt);
            } else {
                assert_eq!(result.state, ObservationState::Uncalibrated);
                assert_eq!(result.value, OpticalDbmValue::Unavailable);
            }
        }
    }
}
