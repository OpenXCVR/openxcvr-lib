//! Qualification is from real generated metadata, not synthetic masks or fixture values.
use openxcvr_cmis::{
    generated::v5_3::CmisPage01hAdvertising,
    read_planning::{CmisPage01ReadFootprint, CmisReadPlanner},
};
use openxcvr_core::{rev::Rev, types::management::*};

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 1,
        offset,
        len,
    }
}

#[test]
fn byte142_is_completely_qualified_only_at_53() {
    let metadata = CmisPage01hAdvertising::read_classifications();
    assert_eq!(
        metadata,
        openxcvr_cmis::generated::union::CmisPage01hAdvertising::read_classifications()
    );
    assert!(
        openxcvr_cmis::generated::v4_0::CmisPage01hAdvertising::read_classifications()
            .iter()
            .all(|byte| !byte.is_qualified_none())
    );
    let byte = &metadata[142 - 128];
    assert!(byte.is_qualified_none());
    assert_eq!(byte.readable_mask, 0xff);
    assert_eq!(
        (byte.unknown_mask, byte.uncovered_mask, byte.write_only_mask),
        (0, 0, 0)
    );
    assert_eq!(byte.known_effects, vec![ReadEffect::None]);
    assert_eq!(byte.change_source, ChangeSource::Static);
    let footprint = CmisPage01ReadFootprint::for_revision(Rev::CMIS_5_3).unwrap();
    assert_eq!(
        footprint.classify_range(range(142, 1)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Static
        })
    );
    for revision in [
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::parse_cmis("5.4").unwrap(),
    ] {
        assert_eq!(
            CmisPage01ReadFootprint::for_revision(revision),
            Err(InputError::UnsupportedRevision(revision))
        );
    }
    let legacy = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    assert_eq!(
        legacy.classify_range(Rev::CMIS_4_0, range(142, 1)),
        Ok(RangeClassified::Uncovered)
    );
}

#[test]
fn no_other_byte_or_addressing_becomes_passive() {
    let footprint = CmisPage01ReadFootprint::for_revision(Rev::CMIS_5_3).unwrap();
    for offset in 128..=255 {
        if offset != 142 && !CAPABILITY_BYTES.contains(&offset) {
            assert!(
                matches!(
                    footprint.classify_range(range(offset, 1)),
                    Ok(RangeClassified::NonPassive { .. })
                ),
                "offset {offset}"
            );
            assert!(footprint.plan_range(range(offset, 1)).is_err());
        }
    }
    assert!(matches!(
        footprint.plan_range(range(142, 2)),
        Err(PlanRefusal::NonPassive { byte: 143, .. })
    ));
    for requested in [
        Range {
            page: 0x11,
            ..range(142, 1)
        },
        Range {
            bank: 1,
            ..range(142, 1)
        },
        Range {
            page: 2,
            ..range(142, 1)
        },
        Range {
            address: 0x51,
            ..range(142, 1)
        },
        range(127, 2),
    ] {
        assert_eq!(
            footprint.classify_range(requested),
            Ok(RangeClassified::Uncovered)
        );
        assert_eq!(
            footprint.plan_range(requested),
            Err(PlanRefusal::Uncovered { range: requested })
        );
    }
    for offset in [141, 143] {
        assert_eq!(
            footprint.classify_range(range(offset, 1)),
            Ok(RangeClassified::NonPassive {
                byte: offset,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
    }
    assert!(footprint.classify_range(range(142, 0)).is_err());
    assert!(footprint.plan_range(range(255, 2)).is_err());
}

#[test]
fn plan_names_exact_selection_and_data_without_executing_selection() {
    let footprint = CmisPage01ReadFootprint::for_revision(Rev::CMIS_5_3).unwrap();
    let plan = footprint.plan_range(range(142, 1)).unwrap();
    assert_eq!(
        plan.required_selection,
        Selection {
            address: 0x50,
            bank: 0,
            page: 1
        }
    );
    assert_eq!(plan.read.physical_range, range(142, 1));
    assert_eq!(plan.read.requester, ReadRequester::PassiveInspection);
    assert_eq!(plan.read.declared_effects, vec![ReadEffect::None]);
    assert_eq!(
        plan.read.classifications,
        CmisPage01hAdvertising::read_classifications()[14..15]
    );
    // The existing observation planner cannot change selection or acquire this page.
    let input = PlanObservationInput {
        revision: Rev::CMIS_5_3,
        requester: ReadRequester::PassiveInspection,
        ranges: vec![range(142, 1)],
        limits: ReadLimits {
            max_ranges: 1,
            max_bytes: 1,
            max_transfer_len: 1,
        },
        selector: Some(Selection {
            address: 0x50,
            bank: 0,
            page: 0,
        }),
        flat_memory: Some(true),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(1),
    };
    assert_eq!(
        CmisReadPlanner::cmis_5_3().plan_observation(&input),
        Err(PlanRefusal::Uncovered {
            range: range(142, 1)
        })
    );
}

#[test]
fn legacy_planner_still_refuses_to_change_the_selector() {
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let planner = CmisReadPlanner::for_revision(revision).unwrap();
        let input = PlanObservationInput {
            revision,
            requester: ReadRequester::PassiveInspection,
            ranges: vec![Range {
                page: 0,
                ..range(129, 1)
            }],
            limits: ReadLimits {
                max_ranges: 1,
                max_bytes: 1,
                max_transfer_len: 1,
            },
            selector: Some(Selection {
                address: 0x50,
                bank: 0,
                page: 1,
            }),
            flat_memory: Some(false),
            generation: ModuleGeneration(1),
            epoch: CacheEpoch(1),
        };
        assert_eq!(
            planner.plan_observation(&input),
            Err(PlanRefusal::SelectorChangeRequired { bank: 0, page: 0 })
        );
    }
}

// Exact requested physical bytes; gaps are deliberately not implied.
const CAPABILITY_BYTES: &[u8] = &[
    128, 129, 145, 146, 147, 150, 151, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164,
    165, 166,
];

#[test]
fn capabilities_are_complete_passive_physical_bytes_only_at_53() {
    let metadata = CmisPage01hAdvertising::read_classifications();
    let old = openxcvr_cmis::generated::v4_0::CmisPage01hAdvertising::read_classifications();
    let footprint = CmisPage01ReadFootprint::for_revision(Rev::CMIS_5_3).unwrap();
    let unqualified: Vec<_> = CAPABILITY_BYTES
        .iter()
        .copied()
        .filter(|offset| !metadata[usize::from(*offset - 128)].is_qualified_none())
        .collect();
    assert!(unqualified.is_empty(), "unqualified bytes: {unqualified:?}");
    for &offset in CAPABILITY_BYTES {
        assert!(
            metadata[usize::from(offset - 128)].is_qualified_none(),
            "byte {offset}"
        );
        assert!(
            !old[usize::from(offset - 128)].is_qualified_none(),
            "4.0 byte {offset}"
        );
        assert!(matches!(
            footprint.classify_range(range(offset, 1)),
            Ok(RangeClassified::Passive { .. })
        ));
        let plan = footprint.plan_range(range(offset, 1)).unwrap();
        assert_eq!(
            plan.required_selection,
            Selection {
                address: 0x50,
                bank: 0,
                page: 1
            }
        );
        assert_eq!(plan.read.physical_range, range(offset, 1));
        assert_eq!(
            plan.read.classifications,
            metadata[usize::from(offset - 128)..usize::from(offset - 127)]
        );
        assert!(footprint
            .plan_range(Range {
                bank: 1,
                ..range(offset, 1)
            })
            .is_err());
    }
}

#[test]
fn capability_gaps_are_not_passive_and_cannot_be_bridged() {
    let footprint = CmisPage01ReadFootprint::for_revision(Rev::CMIS_5_3).unwrap();
    for offset in [143, 144, 148, 149, 152, 167] {
        assert!(matches!(
            footprint.classify_range(range(offset, 1)),
            Ok(RangeClassified::NonPassive { .. })
        ));
        assert!(footprint.plan_range(range(offset, 1)).is_err());
    }
    for (offset, len, refused) in [(142, 4, 143), (147, 4, 148), (151, 3, 152), (166, 2, 167)] {
        assert!(
            matches!(footprint.plan_range(range(offset, len)), Err(PlanRefusal::NonPassive { byte, .. }) if byte == refused)
        );
    }
}

#[test]
fn active_firmware_bytes_are_passive_only_at_53() {
    for offset in [39, 40] {
        let requested = Range {
            page: 0,
            ..range(offset, 1)
        };
        assert!(
            openxcvr_cmis::generated::union::CmisLowerMemory::read_classifications()
                [usize::from(offset)]
            .is_qualified_none()
        );
        assert!(
            !openxcvr_cmis::generated::v4_0::CmisLowerMemory::read_classifications()
                [usize::from(offset)]
            .is_qualified_none()
        );
        assert!(matches!(
            CmisReadPlanner::cmis_5_3().classify_range(Rev::CMIS_5_3, requested),
            Ok(RangeClassified::Passive { .. })
        ));
    }
}
