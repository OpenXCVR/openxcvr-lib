//! Source qualification is CMIS 5.3-only, not permission to consume flags passively.
use openxcvr_cmis::{generated::union::CmisLowerMemory, read_planning::CmisReadPlanner};
use openxcvr_core::{types::management::*, Rev};

fn range(offset: u8) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len: 1,
    }
}
fn input(offset: u8) -> PlanObservationInput {
    PlanObservationInput {
        revision: Rev::CMIS_5_3,
        requester: ReadRequester::PassiveInspection,
        ranges: vec![range(offset)],
        limits: ReadLimits {
            max_ranges: 1,
            max_bytes: 1,
            max_transfer_len: 1,
        },
        selector: None,
        flat_memory: Some(false),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
    }
}

#[test]
fn byte8_full_physical_cor_is_qualified_but_passive_acquisition_still_refuses() {
    // Table 8-9 (p.146): flags 7,6,2,1,0 and Reserved 5:3;
    // Table 8-3 (p.138): *all bits* of a RO/COR byte are cleared on read.
    let byte = &CmisLowerMemory::read_classifications()[8];
    assert_eq!(byte.readable_mask, 0xff);
    assert_eq!(byte.unknown_mask, 0);
    assert_eq!(byte.uncovered_mask, 0);
    assert_eq!(byte.write_only_mask, 0);
    assert_eq!(byte.known_effects, vec![ReadEffect::ClearOnRead]);
    assert_eq!(byte.change_source, ChangeSource::Dynamic);
    assert!(!byte.is_qualified_none());
    let planner = CmisReadPlanner::cmis_5_3();
    assert_eq!(
        planner.classify_range(Rev::CMIS_5_3, range(8)),
        Ok(RangeClassified::NonPassive {
            byte: 8,
            reason: NonPassiveReason::ClearOnRead,
        })
    );
    assert_eq!(
        planner.plan_observation(&input(8)),
        Err(PlanRefusal::NonPassive {
            byte: 8,
            reason: NonPassiveReason::ClearOnRead,
        })
    );
}

#[test]
fn byte37_plain_ro_status_is_passive_but_never_cache_static() {
    // Tables 8-13/14 (p.151) define every bit of module-updated CdbStatus1.
    let byte = &CmisLowerMemory::read_classifications()[37];
    assert!(byte.is_qualified_none());
    assert_eq!(byte.known_effects, vec![ReadEffect::None]);
    assert_eq!(byte.change_source, ChangeSource::Dynamic);
    let planner = CmisReadPlanner::cmis_5_3();
    assert_eq!(
        planner.classify_range(Rev::CMIS_5_3, range(37)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic,
        })
    );
    let plan = planner.plan_observation(&input(37)).unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].physical_range, range(37));
    for revision in [Rev::CMIS_3_0, Rev::CMIS_5_0, Rev::CMIS_5_1, Rev::CMIS_5_2] {
        assert!(
            matches!(CmisReadPlanner::for_revision(revision), Err(InputError::UnsupportedRevision(r)) if r == revision)
        );
    }
    let earlier = openxcvr_cmis::generated::v4_0::CmisLowerMemory::read_classifications();
    assert_eq!(
        earlier[8],
        ByteClassification::from_masks(
            0xc7,
            0,
            0x38,
            0,
            &[ReadEffect::ClearOnRead],
            ChangeSource::Dynamic
        )
    );
    assert_eq!(
        earlier[37],
        ByteClassification::from_masks(0, 0xff, 0, 0, &[], ChangeSource::Dynamic)
    );
}
