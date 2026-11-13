//! Literal CMIS 5.3 Tables 8-46/8-49 oracles. Software-only, no device.
use openxcvr::capability_meanings::cmis53_capability_meaning;
use openxcvr::catalog::{resolve, FieldEntry, FieldQuery, LaneSelection, Scaling, Unit};
use openxcvr::core::rev::{Family, Rev};
use openxcvr::core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp,
    RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};
use openxcvr::observation::{
    observe_field, FieldObservation, FieldValue, ObservationState, ObserveOptions,
};

fn entry(field: &str, rev: Rev) -> FieldEntry<'static> {
    resolve(
        rev,
        &FieldQuery {
            pattern: field,
            lane: LaneSelection::Unspecified,
            page: None,
            bank: 0,
        },
    )
    .unwrap()
    .into_iter()
    .next()
    .unwrap()
}

fn observation(field: &'static str, code: u64) -> FieldObservation {
    FieldObservation {
        field,
        lane: None,
        state: ObservationState::Valid,
        value: Some(FieldValue::Raw(code)),
        note: Some("source caveat retained"),
    }
}

#[test]
fn eight_literal_code_meanings_preserve_the_source() {
    for (field, code, expected) in [
        ("banks_supported", 0, "Bank0With8Lanes"),
        ("banks_supported", 1, "Banks0Through1With16Lanes"),
        ("banks_supported", 2, "Banks0Through3With32Lanes"),
        ("banks_supported", 3, "Reserved"),
        (
            "tx_input_clocking_capabilities",
            0,
            "FrequencySynchronousLanes1To8",
        ),
        (
            "tx_input_clocking_capabilities",
            1,
            "FrequencySynchronousGroups1To4And5To8",
        ),
        (
            "tx_input_clocking_capabilities",
            2,
            "FrequencySynchronousLanePairs",
        ),
        (
            "tx_input_clocking_capabilities",
            3,
            "LanesMayBeAsynchronousInFrequency",
        ),
    ] {
        let entry = entry(field, Rev::CMIS_5_3);
        let source = observation(field, code);
        let before = source.clone();
        // No module, ByteView, transport, context or I/O callback is supplied.
        let meaning = cmis53_capability_meaning(&entry, &source, Some(code)).unwrap();
        assert_eq!(meaning.name, expected);
        assert_eq!(meaning.revision, "5.3");
        assert!(!meaning.name.is_empty() && meaning.name.len() <= 64);
        assert!(meaning.revision.len() <= 16);
        assert_eq!(meaning.raw, code as u8);
        assert!(std::ptr::eq(meaning.source, &source));
        assert_eq!(meaning.entry.range(), entry.range());
        assert_eq!(meaning.entry.rev, Rev::CMIS_5_3);
        assert_eq!(source, before);
        if field == "banks_supported" {
            assert!(meaning.note.contains("10h-2Fh"));
            assert!(meaning.note.contains("CDB"));
        } else {
            for phrase in [
                "8-lane bank",
                "across apps: default",
                "if meaningful",
                "highest capabilities",
                "strongest restrictions",
                "App-standard tolerance",
            ] {
                assert!(meaning.note.contains(phrase), "{}", meaning.note);
            }
        }
    }
}

#[test]
fn exact_family_revision_and_field_only() {
    for field in ["banks_supported", "tx_input_clocking_capabilities"] {
        let valid = entry(field, Rev::CMIS_5_3);
        let source = observation(field, 0);
        for rev in [
            Rev::CMIS_3_0,
            Rev::CMIS_4_0,
            Rev::CMIS_5_0,
            Rev::CMIS_5_1,
            Rev::CMIS_5_2,
            Rev {
                family: Family::Cmis,
                major: 5,
                minor: 4,
            },
            Rev {
                family: Family::Cmis,
                major: 5,
                minor: u8::MAX,
            },
        ] {
            let mut wrong = valid;
            wrong.rev = rev;
            assert!(cmis53_capability_meaning(&wrong, &source, Some(0)).is_none());
        }
        for family in [
            Family::Sff8472,
            Family::Sff8636,
            Family::CCmis,
            Family::Elsfp,
        ] {
            let mut wrong = valid;
            wrong.rev.family = family;
            assert!(cmis53_capability_meaning(&wrong, &source, Some(0)).is_none());
        }
        let wrong_source = observation("module_state", 0);
        assert!(cmis53_capability_meaning(&valid, &wrong_source, Some(0)).is_none());
        let mut lane_source = source.clone();
        lane_source.lane = Some(1);
        assert!(cmis53_capability_meaning(&valid, &lane_source, Some(0)).is_none());
    }
    let wrong = entry("module_state", Rev::CMIS_5_3);
    assert!(cmis53_capability_meaning(&wrong, &observation("module_state", 0), Some(0)).is_none());
    assert_eq!(Rev::parse_cmis("5.x"), None);
    assert_eq!(Rev::parse_cmis("5"), None);
}

#[test]
fn absent_invalid_missing_mismatched_and_out_of_width_are_not_reserved() {
    for field in ["banks_supported", "tx_input_clocking_capabilities"] {
        let entry = entry(field, Rev::CMIS_5_3);
        let valid = observation(field, 3);
        assert!(cmis53_capability_meaning(&entry, &valid, Some(3)).is_some());
        for state in [
            ObservationState::NotSupplied,
            ObservationState::Invalid,
            ObservationState::Uncalibrated,
            ObservationState::UnsupportedRevision(Rev::CMIS_4_0),
            ObservationState::ClearOnReadRefused,
            ObservationState::WriteOnlyRefused,
            ObservationState::UnsupportedField,
        ] {
            let mut source = valid.clone();
            source.state = state;
            assert!(cmis53_capability_meaning(&entry, &source, Some(3)).is_none());
        }
        for value in [
            None,
            Some(FieldValue::Raw(2)),
            Some(FieldValue::Scaled {
                value: 3.0,
                unit: Unit::Watt,
            }),
        ] {
            let mut source = valid.clone();
            source.value = value;
            assert!(cmis53_capability_meaning(&entry, &source, Some(3)).is_none());
        }
        assert!(cmis53_capability_meaning(&entry, &valid, None).is_none());
        assert!(cmis53_capability_meaning(&entry, &valid, Some(2)).is_none());
        for code in [4, 255, 256, u64::MAX] {
            assert!(
                cmis53_capability_meaning(&entry, &observation(field, code), Some(code)).is_none()
            );
        }
    }
}

fn snapshot(entry: &FieldEntry<'_>, byte: u8, revision: u8) -> Snapshot {
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    let stamp = ObservationStamp {
        generation,
        epoch,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    };
    let mut revision_range = entry.range();
    revision_range.page = 0;
    revision_range.offset = 1;
    Snapshot {
        generation,
        epoch,
        validity_interval: (MonoTime(3), MonoTime(4)),
        ranges: vec![(revision_range, revision), (entry.range(), byte)]
            .into_iter()
            .map(|(range, byte)| RangeObservation {
                range,
                outcome: RangeOutcome::Bytes(vec![byte]),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    }
}

#[test]
fn actual_catalog_notes_are_complete_within_the_fixed_c_budget() {
    // The published C note[128] includes its terminating NUL: no truncation.
    const MAX_DECODED_NOTE_BYTES: usize = 127;
    for (field, bit, expected_note) in [
        (
            "banks_supported",
            0,
            "Only lane-banked pages 10h-2Fh in 8-lane groups; not general/CDB banking. Reserved defines no bank count.",
        ),
        (
            "tx_input_clocking_capabilities",
            5,
            "Per 8-lane bank; across apps: default or, if meaningful, highest capabilities/strongest restrictions. App-standard tolerance.",
        ),
    ] {
        let entry = entry(field, Rev::CMIS_5_3);
        assert_eq!(entry.row.scaling, Scaling::None);
        for code in 0..4 {
            let view = snapshot(&entry, (code << bit) | !(3 << bit), 0x53);
            let source = observe_field(
                &entry,
                &view,
                ModuleGeneration(1),
                CacheEpoch(2),
                &ObserveOptions::default(),
            );
            assert_eq!(source.state, ObservationState::Valid);
            assert_eq!(source.value, Some(FieldValue::Raw(code.into())));
            assert_eq!(source.note, None);
            let before = source.clone();
            let meaning = cmis53_capability_meaning(&entry, &source, Some(code.into())).unwrap();
            assert!(
                meaning.note.len() <= MAX_DECODED_NOTE_BYTES,
                "{field} code {code}: {} decoded bytes exceed the 127-byte C note budget",
                meaning.note.len()
            );
            assert_eq!(meaning.note, expected_note);
            // The daemon's (None, Some(meaning)) branch emits meaning.note
            // alone, without a source-note prefix or separating space.
            assert_eq!(meaning.source.note, None);
            assert!(std::ptr::eq(meaning.source, &source));
            assert_eq!(meaning.raw, code);
            assert_eq!(meaning.entry.range(), entry.range());
            assert_eq!(meaning.entry.rev, Rev::CMIS_5_3);
            assert_eq!(source, before);
        }
    }
}

#[test]
fn numeric_catalog_and_raw_decode_are_unchanged_at_every_revision() {
    for (field, offset, bit) in [
        ("banks_supported", 142, 0),
        ("tx_input_clocking_capabilities", 145, 5),
    ] {
        for (rev, byte) in [
            (Rev::CMIS_3_0, 0x30),
            (Rev::CMIS_4_0, 0x40),
            (Rev::CMIS_5_0, 0x50),
            (Rev::CMIS_5_1, 0x51),
            (Rev::CMIS_5_2, 0x52),
            (Rev::CMIS_5_3, 0x53),
        ] {
            let entry = entry(field, rev);
            assert_eq!(entry.row.enum_ref, None);
            assert_eq!(
                (
                    entry.range().page,
                    entry.range().offset,
                    entry.bit_offset,
                    entry.width_bits()
                ),
                (1, offset, bit, 2)
            );
            for code in 0..4 {
                let view = snapshot(&entry, (code << bit) | !(3 << bit), byte);
                let source = observe_field(
                    &entry,
                    &view,
                    ModuleGeneration(1),
                    CacheEpoch(2),
                    &ObserveOptions::default(),
                );
                assert_eq!(source.state, ObservationState::Valid);
                assert_eq!(source.value, Some(FieldValue::Raw(code.into())));
                assert_eq!(
                    cmis53_capability_meaning(&entry, &source, Some(code.into())).is_some(),
                    rev == Rev::CMIS_5_3
                );
            }
        }
    }
}

#[test]
fn earlier_generated_public_field_types_and_module_state_are_preserved() {
    macro_rules! numeric_fields {
        ($module:ident) => {{
            fn numeric(page: &openxcvr_cmis::generated::$module::CmisPage01hAdvertising) {
                let _: u8 = page.banks_supported;
                let _: u8 = page.tx_input_clocking_capabilities;
            }
            let _ = numeric;
        }};
    }
    numeric_fields!(v3_0);
    numeric_fields!(v4_0);
    numeric_fields!(v5_0);
    numeric_fields!(v5_1);
    numeric_fields!(v5_2);
    numeric_fields!(v5_3);
    use openxcvr_cmis::generated::v4_0::CmisLowerMemory;
    for (byte, name) in [
        (2, "ModuleLowPwr"),
        (4, "ModulePwrUp"),
        (6, "ModuleReady"),
        (8, "ModulePwrDn"),
        (10, "ModuleFault"),
    ] {
        for flags in [0, 1, 0xf0, 0xf1] {
            assert_eq!(
                CmisLowerMemory::decode_module_state_supplied(&[byte | flags], 3)
                    .unwrap()
                    .name(),
                name
            );
        }
    }
}

#[test]
fn generated_enum_projections_refuse_all_earlier_meanings() {
    use openxcvr_cmis::generated::{CmisBanksSupported, CmisTxInputClockingCapabilities};
    for revision in [0x30, 0x40, 0x50, 0x51, 0x52] {
        for raw in 0..=255 {
            assert_eq!(
                CmisBanksSupported::for_revision(revision).unwrap()(raw),
                None
            );
            assert_eq!(
                CmisTxInputClockingCapabilities::for_revision(revision).unwrap()(raw),
                None
            );
        }
    }
    for revision in [0, 0x55, 0x5f, 0xff] {
        assert!(CmisBanksSupported::for_revision(revision).is_none());
        assert!(CmisTxInputClockingCapabilities::for_revision(revision).is_none());
    }
    for raw in 4..=255 {
        assert_eq!(CmisBanksSupported::for_revision(0x53).unwrap()(raw), None);
        assert_eq!(
            CmisTxInputClockingCapabilities::for_revision(0x53).unwrap()(raw),
            None
        );
    }
}
