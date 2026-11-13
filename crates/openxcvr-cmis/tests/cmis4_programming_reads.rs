//! CMIS 4.0 source qualification A (page 00h) and B (bootstrap) are independent.
//! Authority is the CMIS 4.0 page-00h identity window (bytes 128–222) and bootstrap byte 2.
use openxcvr_cmis::{
    prerequisite_receipt::qualification_for_ranges_for_revision, read_planning::CmisReadPlanner,
};
use openxcvr_core::{types::management::*, Rev};

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}
fn input(ranges: Vec<Range>, max_transfer_len: u16) -> PlanObservationInput {
    PlanObservationInput {
        revision: Rev::CMIS_4_0,
        requester: ReadRequester::PassiveInspection,
        ranges,
        limits: ReadLimits {
            max_ranges: 4,
            max_bytes: 256,
            max_transfer_len,
        },
        selector: Some(Selection {
            address: 0x50,
            bank: 0,
            page: 0,
        }),
        flat_memory: Some(false),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
    }
}
#[test]
fn a_exact_95_byte_php_verification_and_chunked_94_plus_checksum() {
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    for ranges in [vec![range(128, 95)], vec![range(128, 94), range(222, 1)]] {
        for limit in [16, 94, 95] {
            let plan = planner
                .plan_observation(&input(ranges.clone(), limit))
                .unwrap();
            let bytes: Vec<_> = plan
                .steps
                .iter()
                .flat_map(|step| {
                    assert!(step.physical_range.len <= limit);
                    assert_eq!(step.declared_effects, [ReadEffect::None]);
                    assert!(step
                        .classifications
                        .iter()
                        .all(ByteClassification::is_qualified_none));
                    u32::from(step.physical_range.offset)..step.physical_range.end()
                })
                .collect();
            assert_eq!(bytes, (128..=222).collect::<Vec<_>>());
        }
    }
    for byte in 128..=222 {
        assert_eq!(
            planner.classify_range(Rev::CMIS_4_0, range(byte, 1)),
            Ok(RangeClassified::Passive {
                change_source: ChangeSource::Static
            })
        );
    }
    assert_eq!(
        qualification_for_ranges_for_revision(Rev::CMIS_4_0, &[range(128, 95)]).unwrap(),
        "qualified"
    );
}
#[test]
fn a_unrelated_unknown_and_boundary_bytes_remain_refused_at_both_revisions() {
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let planner = CmisReadPlanner::for_revision(rev).unwrap();
        // Lower 85–117 is independently qualified by application_advertising_reads.
        for byte in (223..=255).chain([14, 25, 41]) {
            assert!(
                matches!(
                    planner.classify_range(rev, range(byte, 1)),
                    Ok(RangeClassified::NonPassive { .. })
                ),
                "{rev:?} byte {byte}"
            );
        }
    }
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    for ranges in [vec![range(128, 96)], vec![range(128, 94), range(222, 2)]] {
        assert_eq!(
            planner.plan_observation(&input(ranges, 95)),
            Err(PlanRefusal::NonPassive {
                byte: 223,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
    }
    let v5 = CmisReadPlanner::cmis_5_3();
    for byte in [128, 190, 200, 208, 213, 221, 222] {
        assert_eq!(
            v5.classify_range(Rev::CMIS_5_3, range(byte, 1)),
            Ok(RangeClassified::NonPassive {
                byte,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
    }
}
#[test]
fn b_whole_byte2_and_d4_passive_detection_are_source_qualified() {
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range(2, 1)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Static
        })
    );
    let plan = planner
        .plan_observation(&input(vec![range(0, 4)], 4))
        .unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].physical_range, range(0, 4));
    assert_eq!(
        plan.steps[0].classifications[3].change_source,
        ChangeSource::Dynamic
    );
    assert_eq!(
        qualification_for_ranges_for_revision(Rev::CMIS_4_0, &[range(0, 4)]).unwrap(),
        "qualified"
    );
}

#[test]
fn only_the_qualified_bytes_are_passive_across_the_complete_256_byte_footprint() {
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    for byte in 0..=255 {
        let expected = byte <= 3
            || (85..=117).contains(&byte)
            || (126..=127).contains(&byte)
            || (128..=222).contains(&byte);
        assert_eq!(
            matches!(
                planner
                    .classify_range(Rev::CMIS_4_0, range(byte, 1))
                    .unwrap(),
                RangeClassified::Passive { .. }
            ),
            expected,
            "byte {byte}"
        );
    }
    for requested in [
        Range {
            address: 0x51,
            ..range(128, 95)
        },
        Range {
            bank: 1,
            ..range(128, 95)
        },
        Range {
            page: 1,
            ..range(128, 95)
        },
    ] {
        assert_eq!(
            planner.classify_range(Rev::CMIS_4_0, requested),
            Ok(RangeClassified::Uncovered)
        );
    }
    for revision in [Rev::CMIS_3_0, Rev::CMIS_5_3] {
        assert_eq!(
            planner.classify_range(revision, range(128, 95)),
            Err(InputError::UnsupportedRevision(revision))
        );
    }
}
