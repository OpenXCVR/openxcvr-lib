//! CMIS 5.3 software-fixture qualification, not evidence of a physical read.
use openxcvr_cmis::read_planning::CmisReadPlanner;
use openxcvr_core::{rev::Rev, types::management::*};

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

#[test]
fn cmis53_module_status_is_passive_dynamic() {
    // Exercise the actual planner's flat generated/union table, not an unused
    // per-revision projection or a manually assembled classifier.
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_5_3).unwrap();
    let actual = planner.classify_range(Rev::CMIS_5_3, range(3, 1));
    eprintln!("CMIS 5.3 actual planner byte 3: {actual:?}");
    assert_eq!(
        actual,
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        })
    );
}

fn request(revision: Rev, ranges: Vec<Range>) -> PlanObservationInput {
    PlanObservationInput {
        revision,
        requester: ReadRequester::PassiveInspection,
        ranges,
        limits: ReadLimits {
            max_ranges: 4,
            max_bytes: 256,
            max_transfer_len: 16,
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
fn whole_byte_has_declared_none_full_coverage_and_no_extra_read() {
    use openxcvr_cmis::generated::union::CmisLowerMemory;
    let class = &CmisLowerMemory::read_classifications()[3];
    assert_eq!(
        class,
        &ByteClassification::from_masks(0xff, 0, 0, 0, &[ReadEffect::None], ChangeSource::Dynamic)
    );
    let plan = CmisReadPlanner::for_revision(Rev::CMIS_5_3)
        .unwrap()
        .plan_observation(&request(Rev::CMIS_5_3, vec![range(3, 1)]))
        .unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].physical_range, range(3, 1));
    assert_eq!(plan.steps[0].classifications, vec![class.clone()]);
    assert_eq!(plan.steps[0].declared_effects, vec![ReadEffect::None]);
    // CMIS4 keeps its independently qualified whole-byte result.
    assert_eq!(
        openxcvr_cmis::generated::v4_0::CmisLowerMemory::read_classifications()[3],
        *class
    );
}

#[test]
fn unsupported_revisions_families_and_addressing_stay_closed() {
    use openxcvr_core::rev::Family;
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_5_3).unwrap();
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 4,
        },
        Rev {
            family: Family::Sff8636,
            major: 5,
            minor: 3,
        },
    ] {
        assert!(matches!(CmisReadPlanner::for_revision(revision),
            Err(InputError::UnsupportedRevision(r)) if r == revision));
        assert_eq!(
            planner.classify_range(revision, range(3, 1)),
            Err(InputError::UnsupportedRevision(revision))
        );
        assert_eq!(
            planner.plan_observation(&request(revision, vec![range(3, 1)])),
            Err(PlanRefusal::UnsupportedRevision(revision))
        );
    }
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range(3, 1)),
        Err(InputError::UnsupportedRevision(Rev::CMIS_4_0))
    );
    for wrong in [
        Range {
            address: 0x51,
            ..range(3, 1)
        },
        Range {
            bank: 1,
            ..range(3, 1)
        },
        Range {
            page: 1,
            ..range(3, 1)
        },
    ] {
        assert_eq!(
            planner.classify_range(Rev::CMIS_5_3, wrong),
            Ok(RangeClassified::Uncovered)
        );
        assert_eq!(
            planner.plan_observation(&request(Rev::CMIS_5_3, vec![wrong])),
            Err(PlanRefusal::Uncovered { range: wrong })
        );
    }
}

#[test]
fn neighbors_flags_and_mixed_ranges_keep_their_baseline_refusals() {
    use openxcvr_cmis::generated::union::CmisLowerMemory;
    let table = CmisLowerMemory::read_classifications();
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_5_3).unwrap();
    for class in &table[4..=7] {
        assert_eq!(
            *class,
            ByteClassification::from_masks(0, 0x0f, 0xf0, 0, &[], ChangeSource::Dynamic)
        );
    }
    for class in &table[8..=11] {
        assert_eq!(
            *class,
            ByteClassification::from_masks(
                0xff,
                0,
                0,
                0,
                &[ReadEffect::ClearOnRead],
                ChangeSource::Dynamic
            )
        );
    }
    for (requested, byte, reason) in [
        (range(3, 2), 4, NonPassiveReason::MetadataAbsent),
        (range(3, 9), 4, NonPassiveReason::MetadataAbsent),
        (range(4, 8), 4, NonPassiveReason::MetadataAbsent),
        (range(8, 4), 8, NonPassiveReason::ClearOnRead),
        (range(9, 3), 9, NonPassiveReason::ClearOnRead),
    ] {
        assert_eq!(
            planner.classify_range(Rev::CMIS_5_3, requested),
            Ok(RangeClassified::NonPassive { byte, reason })
        );
        assert_eq!(
            planner.plan_observation(&request(Rev::CMIS_5_3, vec![requested])),
            Err(PlanRefusal::NonPassive { byte, reason })
        );
    }
    // Separate safe + consuming ranges remain a typed consuming-effect refusal,
    // not a malformed-range/parser failure and not widened passive admission.
    assert_eq!(
        planner.plan_observation(&request(Rev::CMIS_5_3, vec![range(3, 1), range(8, 4)])),
        Err(PlanRefusal::NonPassive {
            byte: 8,
            reason: NonPassiveReason::ClearOnRead
        })
    );
    assert_eq!(
        planner.classify_range(Rev::CMIS_5_3, range(0, 4)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        })
    );
}

fn query(pattern: &str) -> openxcvr_core::field_catalog::FieldQuery<'_> {
    use openxcvr_core::field_catalog::{FieldQuery, LaneSelection};
    FieldQuery {
        pattern,
        lane: LaneSelection::Unspecified,
        page: Some("lower"),
        bank: 0,
    }
}

#[test]
fn reserved_catalog_row_is_public_tagged_and_revision_isolated() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_core::field_catalog::{resolve, AccessKind, CatalogError, FieldKind};
    // Revision projection may split existing rows; count the new identity,
    // not a global span-table size that freezes incorrect base metadata.
    assert_eq!(
        CATALOG
            .rows
            .iter()
            .filter(|row| row.name == "reserved_module_status_cmis53")
            .count(),
        1
    );
    let rows = resolve(&CATALOG, Rev::CMIS_5_3, &query("reserved_module_status_*")).unwrap();
    assert_eq!(rows.len(), 1);
    let entry = &rows[0];
    let row = entry.row;
    assert_eq!(row.name, "reserved_module_status_cmis53");
    assert_eq!(row.since, Some(Rev::CMIS_5_3));
    assert_eq!(row.until, None);
    assert_eq!(row.kind, FieldKind::Bitmap);
    assert_eq!(row.category, Some("Reserved"));
    assert_eq!(row.tags, &["Reserved"]);
    assert_eq!(row.access, AccessKind::Ro);
    assert_eq!(row.read_effects, &[ReadEffect::None]);
    assert_eq!(row.change_source, Some(ChangeSource::Static));
    assert_eq!(row.bit_offset, 4);
    assert_eq!(row.bit_width, 4);
    assert_eq!(entry.range(), range(3, 1));
    for rev in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
    ] {
        assert!(matches!(
            resolve(&CATALOG, rev, &query(row.name)),
            Err(CatalogError::NoMatch { .. })
        ));
        let glob = resolve(&CATALOG, rev, &query("*")).unwrap();
        assert!(!glob.iter().any(|e| e.name() == row.name));
    }
    let old = resolve(&CATALOG, Rev::CMIS_4_0, &query("reserved_module_status_*")).unwrap();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].name(), "reserved_module_status_cmis4");
    assert_eq!(old[0].row.since, Some(Rev::CMIS_4_0));
    assert_eq!(old[0].row.until, Some(Rev::CMIS_5_0));
}

#[test]
fn bitmap_observation_obtains_supplied_bytes_but_never_produces_a_number() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_core::field_catalog::resolve;
    use openxcvr_core::observation::{
        observe_field_with, AcquisitionContext, ObservationState, ObserveOptions,
    };
    use std::cell::RefCell;
    struct RecordingView {
        observation: RangeObservation,
        requested: RefCell<Vec<Range>>,
    }
    impl ByteView for RecordingView {
        fn get(&self, range: Range) -> Option<&RangeObservation> {
            self.requested.borrow_mut().push(range);
            self.observation
                .range
                .contains(range)
                .then_some(&self.observation)
        }
    }
    let entry = resolve(
        &CATALOG,
        Rev::CMIS_5_3,
        &query("reserved_module_status_cmis53"),
    )
    .unwrap()[0];
    // Deliberately supply every possible byte, including nonzero Reserved bits;
    // the bitmap is not a numeric interpretation even after bytes are obtained.
    for value in 0..=255 {
        let view = RecordingView {
            observation: RangeObservation {
                range: range(1, 3),
                outcome: RangeOutcome::Bytes(vec![0x53, 0, value]),
                stamp: ObservationStamp {
                    generation: ModuleGeneration(1),
                    epoch: CacheEpoch(2),
                    started_at: MonoTime(3),
                    completed_at: MonoTime(4),
                    source: ObservationSource::Supplied,
                    freshness: Freshness::Fresh,
                    consumed_effects: vec![],
                },
                requester: ReadRequester::PassiveInspection,
            },
            requested: RefCell::new(vec![]),
        };
        let observed = observe_field_with(
            &entry,
            &view,
            ModuleGeneration(1),
            CacheEpoch(2),
            &ObserveOptions::default(),
            &AcquisitionContext::default(),
        );
        assert_eq!(observed.state, ObservationState::UnsupportedField);
        assert_eq!(observed.value, None);
        assert!(
            view.requested.borrow().contains(&range(1, 1)),
            "revision bytes obtained"
        );
        assert!(
            view.requested.borrow().contains(&range(3, 1)),
            "field bytes obtained before refusal"
        );
    }
}

#[test]
fn existing_status_query_identity_permission_and_decode_are_preserved() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_cmis::generated::{union::CmisLowerMemory, CmisModuleState};
    use openxcvr_core::field_catalog::{resolve, AccessKind, FieldKind, Scaling};
    for rev in CATALOG.supported_revs {
        for (name, display, kind, offset, width, enum_ref) in [
            (
                "module_state",
                "ModuleState",
                FieldKind::U3,
                1,
                3,
                Some("CmisModuleState"),
            ),
            (
                "interrupt_deasserted",
                "InterruptDeasserted",
                FieldKind::Bool,
                0,
                1,
                None,
            ),
        ] {
            let entries = resolve(&CATALOG, *rev, &query(name)).unwrap();
            assert_eq!(entries.len(), 1);
            let entry = entries[0];
            assert_eq!(entry.name(), name);
            assert_eq!(entry.row.display_name, display);
            assert_eq!(entry.range(), range(3, 1));
            assert_eq!(entry.row.kind, kind);
            assert_eq!(entry.bit_offset, offset);
            assert_eq!(entry.width_bits(), width);
            assert_eq!(entry.row.enum_ref, enum_ref);
            assert_eq!(entry.row.scaling, Scaling::None);
            assert_eq!(entry.row.access, AccessKind::Ro);
            assert_eq!(entry.row.category, Some("Status"));
            assert_eq!(entry.row.tags, &["Status"]);
            assert_eq!(entry.row.change_source, Some(ChangeSource::Dynamic));
        }
    }
    for byte in 0..=255 {
        let mut bytes = [0u8; 128];
        bytes[3] = byte;
        let decoded = CmisLowerMemory::decode(&bytes);
        assert_eq!(decoded.module_state, CmisModuleState::from((byte >> 1) & 7));
        assert_eq!(decoded.interrupt_deasserted, byte & 1 != 0);
        assert_eq!(
            CmisLowerMemory::decode_module_state_supplied(&[byte], 3).unwrap(),
            decoded.module_state
        );
    }
}

#[test]
fn cmis53_status_catalog_effects_match_the_qualified_schema() {
    use openxcvr_cmis::generated::catalog::CATALOG;
    use openxcvr_core::field_catalog::resolve;
    // Required accepted contract: metadata exposed by the catalog must agree
    // with the schema's 5.3 qualification, not just the live planner's table.
    // Projection must preserve earlier revision resets, not just repair 5.3.
    for (revision, effect) in [
        (Rev::CMIS_4_0, ReadEffect::None),
        (Rev::CMIS_5_0, ReadEffect::Unknown),
        (Rev::CMIS_5_1, ReadEffect::Unknown),
        (Rev::CMIS_5_2, ReadEffect::Unknown),
        (Rev::CMIS_5_3, ReadEffect::None),
    ] {
        for name in ["module_state", "interrupt_deasserted"] {
            let entries = resolve(&CATALOG, revision, &query(name)).unwrap();
            assert_eq!(entries.len(), 1, "{revision:?} {name}");
            assert_eq!(
                entries[0].row.read_effects,
                &[effect],
                "{revision:?} {name}"
            );
        }
    }
}
