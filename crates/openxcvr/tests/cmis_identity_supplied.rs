//! Synthetic supplied spans only; no captures, transport or padded pages.
use openxcvr::core::{rev::Family, types::management::*, Rev};
use openxcvr::live::{cmis_identity_layout, decode_cmis_identity_supplied};
use openxcvr_cmis::generated::union::CmisPage00hAdministrative as Page;

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(2);

const ALL_SIX_CMIS_REVISIONS: [Rev; 6] = [
    Rev::CMIS_3_0,
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
];

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

fn identity() -> Snapshot {
    let stamp = ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    };
    // CMIS 4.0 Table 8-15 / §§8.3.2–8.3.7, not obtained from the API under test.
    let fields = [
        (129, b"EXAMPLE         ".to_vec()),
        (145, vec![0x48, 0xb0, 0x2d]),
        (148, b"PART            ".to_vec()),
        (164, b"A1".to_vec()),
        (166, b"SERIAL          ".to_vec()),
        (182, b"261006L1".to_vec()),
    ];
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: fields
            .into_iter()
            .map(|(offset, bytes)| RangeObservation {
                range: range(offset, bytes.len() as u16),
                outcome: RangeOutcome::Bytes(bytes),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    }
}

fn independent_witness() -> Snapshot {
    let stamp = ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    };
    // Nonzero independent ASCII/OUI/date witness from independent-test-vectors.json
    let fields = [
        (129, b"QUALIFY-LAB     ".to_vec()),
        (145, vec![0x12, 0xa5, 0x7c]),
        (148, b"PART-42         ".to_vec()),
        (164, b"B7".to_vec()),
        (166, b"SERIAL-09       ".to_vec()),
        (182, b"240229L7".to_vec()),
    ];
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: fields
            .into_iter()
            .map(|(offset, bytes)| RangeObservation {
                range: range(offset, bytes.len() as u16),
                outcome: RangeOutcome::Bytes(bytes),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    }
}

#[test]
fn cmis_six_revisions_layouts_match_the_schema_and_actual_identity_tables() {
    // Exact primary tables for all six revisions establish these identical ranges.
    let schema = [
        Page::VENDOR_NAME_RANGE,
        Page::VENDOR_OUI_RANGE,
        Page::VENDOR_PN_RANGE,
        Page::VENDOR_REV_RANGE,
        Page::VENDOR_SN_RANGE,
    ];
    assert_eq!(
        schema,
        [(129, 16), (145, 3), (148, 16), (164, 2), (166, 16)]
    );
    assert_eq!(
        [
            Page::DATE_CODE_YEAR_RANGE,
            Page::DATE_CODE_MONTH_RANGE,
            Page::DATE_CODE_DAY_OF_MONTH_RANGE,
            Page::DATE_CODE_LOT_CODE_RANGE
        ],
        [(182, 2), (184, 2), (186, 2), (188, 2)]
    );
    for revision in ALL_SIX_CMIS_REVISIONS {
        let layout = cmis_identity_layout(revision).unwrap();
        assert_eq!(layout.revision, revision);
        assert_eq!(
            [
                layout.vendor_name,
                layout.oui,
                layout.vendor_pn,
                layout.vendor_rev,
                layout.vendor_sn,
                layout.date_code
            ],
            [
                range(129, 16),
                range(145, 3),
                range(148, 16),
                range(164, 2),
                range(166, 16),
                range(182, 8)
            ]
        );
    }
}

#[test]
fn cmis_six_revisions_decode_only_the_supplied_identity_spans() {
    for revision in ALL_SIX_CMIS_REVISIONS {
        let view = identity();
        let info = decode_cmis_identity_supplied(&view, revision, GENERATION, EPOCH).unwrap();
        assert_eq!(info.name, "EXAMPLE");
        assert_eq!(info.part_number, "PART");
        assert_eq!(info.rev, "A1");
        assert_eq!(info.serial_number, "SERIAL");
        assert_eq!(info.date_code, "261006L1");
        assert_eq!(info.oui, [0x48, 0xb0, 0x2d]);
        // Split every field into single-byte observations, in reverse order.
        let mut split = view.clone();
        split.ranges = view
            .ranges
            .iter()
            .flat_map(|o| {
                let RangeOutcome::Bytes(bytes) = &o.outcome else {
                    unreachable!()
                };
                bytes
                    .iter()
                    .enumerate()
                    .map(|(i, b)| RangeObservation {
                        range: range(o.range.offset + i as u8, 1),
                        outcome: RangeOutcome::Bytes(vec![*b]),
                        ..o.clone()
                    })
                    .collect::<Vec<_>>()
            })
            .rev()
            .collect();
        let split_info =
            decode_cmis_identity_supplied(&split, revision, GENERATION, EPOCH).unwrap();
        assert_eq!(
            (
                split_info.name,
                split_info.part_number,
                split_info.rev,
                split_info.serial_number,
                split_info.date_code,
                split_info.oui
            ),
            (
                info.name,
                info.part_number,
                info.rev,
                info.serial_number,
                info.date_code,
                info.oui
            )
        );
        // All-zero unspecified PN/OUI and blank lot are allowed by 4.0 §§8.3.3–7.
        let mut unspecified = view.clone();
        unspecified.ranges[1].outcome = RangeOutcome::Bytes(vec![0; 3]);
        unspecified.ranges[2].outcome = RangeOutcome::Bytes(vec![0; 16]);
        unspecified.ranges[5].outcome = RangeOutcome::Bytes(b"261006  ".to_vec());
        let info =
            decode_cmis_identity_supplied(&unspecified, revision, GENERATION, EPOCH).unwrap();
        assert_eq!(info.oui, [0; 3]);
        assert_eq!(info.part_number, "");
        assert_eq!(info.date_code, "261006");
    }
}

#[test]
fn cmis_six_revisions_decode_independent_test_vector() {
    for revision in ALL_SIX_CMIS_REVISIONS {
        let view = independent_witness();
        let info = decode_cmis_identity_supplied(&view, revision, GENERATION, EPOCH).unwrap();
        assert_eq!(info.name, "QUALIFY-LAB");
        assert_eq!(info.oui, [0x12, 0xa5, 0x7c]);
        assert_eq!(info.part_number, "PART-42");
        assert_eq!(info.rev, "B7");
        assert_eq!(info.serial_number, "SERIAL-09");
        assert_eq!(info.date_code, "240229L7");
    }
}

#[test]
fn all_six_revisions_reject_incomplete_conflicting_or_stale_supplied_ranges() {
    for revision in ALL_SIX_CMIS_REVISIONS {
        let decode =
            |view: &Snapshot| decode_cmis_identity_supplied(view, revision, GENERATION, EPOCH);
        for index in 0..6 {
            let mut missing = identity();
            missing.ranges.remove(index);
            assert!(matches!(
                decode(&missing),
                Err(InputError::InsufficientInput { .. })
            ));
            let mut short = identity();
            let RangeOutcome::Bytes(bytes) = &mut short.ranges[index].outcome else {
                unreachable!()
            };
            bytes.pop();
            assert!(matches!(decode(&short), Err(InputError::InvalidLength(_))));
            for outcome in [
                RangeOutcome::Absent,
                RangeOutcome::Failed {
                    reason: "synthetic failure".into(),
                },
                RangeOutcome::Uncertain {
                    reason: "synthetic uncertainty".into(),
                },
                RangeOutcome::Bytes(vec![]),
            ] {
                let mut unavailable = identity();
                unavailable.ranges[index].outcome = outcome;
                assert!(decode(&unavailable).is_err());
            }
            let mut conflict = identity();
            let mut duplicate = conflict.ranges[index].clone();
            duplicate.outcome = RangeOutcome::Bytes(vec![0xff; duplicate.range.len as usize]);
            conflict.ranges.push(duplicate);
            assert_eq!(
                decode(&conflict).unwrap_err(),
                InputError::OverlappingRanges
            );
        }
        let mut stale = identity();
        stale.ranges[0].stamp.generation = ModuleGeneration(9);
        assert_eq!(decode(&stale).unwrap_err(), InputError::StaleObservation);
        assert_eq!(
            decode_cmis_identity_supplied(&identity(), revision, GENERATION, CacheEpoch(9))
                .unwrap_err(),
            InputError::StaleObservation
        );
        let mut historical = identity();
        historical.ranges[0].stamp.freshness = Freshness::Historical;
        assert_eq!(
            decode(&historical).unwrap_err(),
            InputError::StaleObservation
        );
        let mut wrong_page = identity();
        wrong_page.ranges[0].range.page = 1;
        assert!(matches!(
            decode(&wrong_page),
            Err(InputError::InsufficientInput { .. })
        ));
    }
}

#[test]
fn identity_does_not_guess_other_revisions_or_families() {
    for revision in [
        Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 4,
        },
        Rev {
            family: Family::Cmis,
            major: 0,
            minor: 0,
        },
        Rev {
            family: Family::Sff8636,
            major: 4,
            minor: 0,
        },
    ] {
        assert_eq!(
            cmis_identity_layout(revision),
            Err(InputError::UnsupportedRevision(revision))
        );
        assert_eq!(
            decode_cmis_identity_supplied(&identity(), revision, GENERATION, EPOCH).unwrap_err(),
            InputError::UnsupportedRevision(revision)
        );
    }
}

struct PanickingByteView;
impl ByteView for PanickingByteView {
    fn get(&self, _range: Range) -> Option<&RangeObservation> {
        panic!("ByteView must not be accessed for unsupported revisions");
    }
}

#[test]
fn unsupported_revisions_fail_early_without_byte_view_access() {
    for revision in [
        Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 4,
        },
        Rev {
            family: Family::Cmis,
            major: 0,
            minor: 0,
        },
        Rev {
            family: Family::Sff8636,
            major: 4,
            minor: 0,
        },
    ] {
        assert_eq!(
            cmis_identity_layout(revision),
            Err(InputError::UnsupportedRevision(revision))
        );
        assert_eq!(
            decode_cmis_identity_supplied(&PanickingByteView, revision, GENERATION, EPOCH)
                .unwrap_err(),
            InputError::UnsupportedRevision(revision)
        );
    }
}
