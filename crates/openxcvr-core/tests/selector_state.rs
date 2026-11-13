use openxcvr_core::{
    rev::{Family, Rev},
    selector::*,
    types::management::{
        ActuationOutcome, ByteClassification, CacheEpoch, ChangeSource, ModuleGeneration,
        OperationId, Range, RangeOutcome, ReadEffect, ReadFootprint, ReadRequester, ReadStep,
        Selection,
    },
};

fn target(bank: u8, page: u8) -> Selection {
    Selection {
        address: 0x50,
        bank,
        page,
    }
}
fn passive() -> ByteClassification {
    ByteClassification::from_masks(0xff, 0, 0, 0, &[ReadEffect::None], ChangeSource::Dynamic)
}
fn footprint(revision: Rev) -> ReadFootprint {
    ReadFootprint {
        revision,
        address: 0x50,
        lower: std::array::from_fn(|_| passive()),
        upper_page00: std::array::from_fn(|_| passive()),
    }
}
fn cmis(revision: Rev) -> SelectorLayout {
    SelectorLayout::cmis_banked(revision, &footprint(revision)).unwrap()
}
fn sff() -> SelectorLayout {
    SelectorLayout::sff8636_page_only(ReadStep {
        physical_range: Range {
            address: 0x50,
            bank: 0,
            page: 0,
            offset: 127,
            len: 1,
        },
        requester: ReadRequester::PassiveInspection,
        classifications: vec![passive()],
        declared_effects: vec![ReadEffect::None],
    })
    .unwrap()
}
fn fresh() -> SelectorState {
    SelectorState::new(ModuleGeneration(1), CacheEpoch(2))
}
fn start(
    state: &SelectorState,
    selection: Selection,
    layout: &SelectorLayout,
    id: u64,
) -> SelectorState {
    let plan = selector_plan(state, selection, layout, OperationId(id)).unwrap();
    selector_begin(state, &plan)
}
fn supply(state: &SelectorState, result: SelectorResult) -> SelectorOutcome {
    let stamp = state.pending().unwrap().stamp;
    selector_complete(state, &SelectorSuppliedResult { stamp, result })
}
fn read(state: &SelectorState, bytes: &[u8]) -> SelectorOutcome {
    supply(
        state,
        SelectorResult::Read(RangeOutcome::Bytes(bytes.to_vec())),
    )
}
fn verified(selection: Selection, layout: &SelectorLayout) -> SelectorState {
    let state = start(&fresh(), selection, layout, 1);
    let state = read(&state, &[selection.bank, selection.page]).state;
    read(&state, &[selection.bank, selection.page]).state
}
fn write(state: &SelectorState) -> (u8, u8) {
    match state.pending().unwrap().step {
        SelectorStep::Write { range, value } => (range.offset, value),
        other => panic!("expected write, got {other:?}"),
    }
}
fn denied(outcome: &SelectorOutcome) {
    assert_eq!(outcome.data_permission, DataPermission::Denied);
}

#[test]
fn unknown_reads_then_writes_only_differences_then_verifies_both_cmis_revisions() {
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let layout = cmis(revision);
        let plan = selector_plan(&fresh(), target(3, 0x11), &layout, OperationId(1)).unwrap();
        assert_eq!(plan.generation(), ModuleGeneration(1));
        assert_eq!(plan.epoch(), CacheEpoch(2));
        assert_eq!(plan.attempt(), OperationId(1));
        assert_eq!(plan.data_permission(), DataPermission::Denied);
        assert_eq!(plan.steps().len(), 1);
        assert_eq!(layout.revision(), Some(revision));
        let state = selector_begin(&fresh(), &plan);
        assert_eq!(
            state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Unknown
        );
        assert!(
            matches!(state.pending().unwrap().step, SelectorStep::Readback { purpose: ReadbackPurpose::Initial, ref read } if read.physical_range.offset == 126 && read.physical_range.len == 2)
        );
        let first = read(&state, &[2, 0x10]);
        denied(&first);
        assert_eq!(write(&first.state), (126, 3));
        let second = supply(
            &first.state,
            SelectorResult::Write(ActuationOutcome::Confirmed),
        );
        denied(&second);
        assert_eq!(write(&second.state), (127, 0x11));
        let third = supply(
            &second.state,
            SelectorResult::Write(ActuationOutcome::Confirmed),
        );
        denied(&third);
        assert!(matches!(
            third.state.pending().unwrap().step,
            SelectorStep::Readback {
                purpose: ReadbackPurpose::Verification,
                ..
            }
        ));
        let done = read(&third.state, &[3, 0x11]);
        assert_eq!(
            done.data_permission,
            DataPermission::Granted(target(3, 0x11))
        );
        assert_eq!(
            done.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Confirmed(target(3, 0x11))
        );
        assert!(done.state.pending().is_none());
    }
}

#[test]
fn initial_match_skips_all_writes_but_not_verification() {
    let state = start(&fresh(), target(0, 1), &cmis(Rev::CMIS_4_0), 1);
    let learned = read(&state, &[0, 1]);
    denied(&learned);
    assert_eq!(
        learned.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Unknown
    );
    assert!(matches!(
        learned.state.pending().unwrap().step,
        SelectorStep::Readback {
            purpose: ReadbackPurpose::Verification,
            ..
        }
    ));
    assert_eq!(
        read(&learned.state, &[0, 1]).data_permission,
        DataPermission::Granted(target(0, 1))
    );
}

#[test]
fn confirmed_match_skips_write_but_requires_same_boundary_and_verification() {
    let layout = cmis(Rev::CMIS_5_3);
    let known = verified(target(2, 0x11), &layout);
    let state = start(&known, target(2, 0x11), &layout, 2);
    assert_eq!(
        state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Unknown
    );
    assert!(matches!(
        state.pending().unwrap().step,
        SelectorStep::Readback {
            purpose: ReadbackPurpose::Verification,
            ..
        }
    ));
    assert_eq!(
        read(&state, &[2, 0x11]).data_permission,
        DataPermission::Granted(target(2, 0x11))
    );
    for (generation, epoch) in [
        (ModuleGeneration(2), CacheEpoch(2)),
        (ModuleGeneration(1), CacheEpoch(3)),
    ] {
        assert_eq!(
            known.knowledge(generation, epoch),
            SelectorKnowledge::Unknown
        );
        let retired = known.at(generation, epoch);
        assert_eq!(
            retired.knowledge(generation, epoch),
            SelectorKnowledge::Unknown
        );
        let next = start(&retired, target(2, 0x11), &layout, 2);
        assert!(matches!(
            next.pending().unwrap().step,
            SelectorStep::Readback {
                purpose: ReadbackPurpose::Initial,
                ..
            }
        ));
    }
}

#[test]
fn bank_and_page_writes_are_independently_minimal() {
    let layout = cmis(Rev::CMIS_5_3);
    let known = verified(target(2, 0x10), &layout);
    for (selection, expected) in [(target(3, 0x10), (126, 3)), (target(2, 0x11), (127, 0x11))] {
        let state = start(&known, selection, &layout, 2);
        assert_eq!(write(&state), expected);
        let after = supply(&state, SelectorResult::Write(ActuationOutcome::Confirmed));
        denied(&after);
        assert!(matches!(
            after.state.pending().unwrap().step,
            SelectorStep::Readback {
                purpose: ReadbackPurpose::Verification,
                ..
            }
        ));
    }
    let state = start(&fresh(), target(0, 1), &layout, 1);
    let next = read(&state, &[7, 0x11]);
    assert_eq!(write(&next.state), (126, 0));
}

#[test]
fn sff_page_only_never_reads_or_writes_bank_126() {
    for page in 0..=u8::MAX {
        let layout = sff();
        let state = start(&fresh(), target(0, page), &layout, 1);
        assert!(
            matches!(state.pending().unwrap().step, SelectorStep::Readback { ref read, .. } if read.physical_range.offset == 127 && read.physical_range.len == 1)
        );
        let state = read(&state, &[page.wrapping_add(1)]).state;
        assert_eq!(write(&state), (127, page));
        let state = supply(&state, SelectorResult::Write(ActuationOutcome::Confirmed)).state;
        assert!(
            matches!(state.pending().unwrap().step, SelectorStep::Readback { ref read, .. } if read.physical_range.offset == 127 && read.physical_range.len == 1)
        );
        assert_eq!(
            read(&state, &[page]).data_permission,
            DataPermission::Granted(target(0, page))
        );
    }
    assert_eq!(
        selector_plan(&fresh(), target(1, 0x11), &sff(), OperationId(1)),
        Err(SelectorRefusal::InvalidSelection)
    );
}

#[test]
fn unsupported_revisions_layouts_and_unqualified_readbacks_are_refused() {
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_2,
        Rev {
            family: Family::Sff8472,
            major: 1,
            minor: 0,
        },
    ] {
        assert_eq!(
            SelectorLayout::cmis_banked(revision, &footprint(revision)),
            Err(SelectorRefusal::UnsupportedLayout)
        );
    }
    assert_eq!(
        SelectorLayout::cmis_banked(Rev::CMIS_4_0, &footprint(Rev::CMIS_5_3)),
        Err(SelectorRefusal::UnqualifiedReadback)
    );
    let mut bad = footprint(Rev::CMIS_4_0);
    bad.lower[126].known_effects.clear();
    assert_eq!(
        SelectorLayout::cmis_banked(Rev::CMIS_4_0, &bad),
        Err(SelectorRefusal::UnqualifiedReadback)
    );
    bad.lower[126] = passive();
    bad.lower[127].unknown_mask = 1;
    assert_eq!(
        SelectorLayout::cmis_banked(Rev::CMIS_4_0, &bad),
        Err(SelectorRefusal::UnqualifiedReadback)
    );
    for selection in [
        target(1, 1),
        Selection {
            address: 0x51,
            ..target(0, 1)
        },
    ] {
        assert_eq!(
            selector_plan(&fresh(), selection, &cmis(Rev::CMIS_4_0), OperationId(1)),
            Err(SelectorRefusal::InvalidSelection)
        );
    }
}

#[test]
fn write_nack_failed_and_uncertain_poison_immediately_and_need_matching_recovery() {
    for result in [
        SelectorResult::Nack,
        SelectorResult::Write(ActuationOutcome::Failed {
            reason: "failed".into(),
        }),
        SelectorResult::Write(ActuationOutcome::Uncertain {
            reason: "uncertain".into(),
        }),
    ] {
        let layout = cmis(Rev::CMIS_4_0);
        let known = verified(target(0, 0), &layout);
        let state = start(&known, target(3, 0x11), &layout, 2);
        let outcome = supply(&state, result);
        denied(&outcome);
        assert_eq!(
            outcome.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Poisoned
        );
        assert!(matches!(
            outcome.state.pending().unwrap().step,
            SelectorStep::Readback {
                purpose: ReadbackPurpose::Verification,
                ..
            }
        ));
        assert_eq!(
            read(&outcome.state, &[3, 0x11]).data_permission,
            DataPermission::Granted(target(3, 0x11))
        );
        let mismatch = read(&outcome.state, &[0, 0]);
        denied(&mismatch);
        assert_eq!(
            mismatch.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Poisoned
        );
    }
}

#[test]
fn failed_nacked_uncertain_absent_short_and_mismatched_readbacks_deny() {
    let results = vec![
        SelectorResult::Nack,
        SelectorResult::Read(RangeOutcome::Absent),
        SelectorResult::Read(RangeOutcome::Failed {
            reason: "failed".into(),
        }),
        SelectorResult::Read(RangeOutcome::Uncertain {
            reason: "uncertain".into(),
        }),
        SelectorResult::Read(RangeOutcome::Bytes(vec![0])),
        SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1, 2])),
    ];
    for verification in [false, true] {
        for result in results.clone() {
            let mut state = start(&fresh(), target(0, 1), &cmis(Rev::CMIS_5_3), 1);
            if verification {
                state = read(&state, &[0, 1]).state;
            }
            let outcome = supply(&state, result);
            denied(&outcome);
            assert_eq!(
                outcome.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
                SelectorKnowledge::Poisoned
            );
            assert!(outcome.state.pending().is_none());
        }
    }
    let state = start(&fresh(), target(0, 1), &cmis(Rev::CMIS_5_3), 1);
    let state = read(&state, &[0, 1]).state;
    let mismatch = read(&state, &[0, 2]);
    denied(&mismatch);
    assert_eq!(
        mismatch.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Poisoned
    );
}

#[test]
fn stale_identity_generation_epoch_and_step_never_resurrect_knowledge() {
    let layout = cmis(Rev::CMIS_4_0);
    let state = start(&fresh(), target(0, 1), &layout, 1);
    let state = read(&state, &[0, 1]).state;
    let stamp = state.pending().unwrap().stamp;
    for stale in [
        SelectorStamp {
            attempt: OperationId(0),
            ..stamp
        },
        SelectorStamp {
            generation: ModuleGeneration(0),
            ..stamp
        },
        SelectorStamp {
            epoch: CacheEpoch(0),
            ..stamp
        },
        SelectorStamp { step: 0, ..stamp },
    ] {
        let outcome = selector_complete(
            &state,
            &SelectorSuppliedResult {
                stamp: stale,
                result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1])),
            },
        );
        denied(&outcome);
        assert_eq!(
            outcome.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Poisoned
        );
        let late = selector_complete(
            &outcome.state,
            &SelectorSuppliedResult {
                stamp,
                result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1])),
            },
        );
        denied(&late);
        assert_eq!(
            late.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Poisoned
        );
    }
    for retired in [
        state.at(ModuleGeneration(2), CacheEpoch(2)),
        state.at(ModuleGeneration(1), CacheEpoch(3)),
    ] {
        let outcome = selector_complete(
            &retired,
            &SelectorSuppliedResult {
                stamp,
                result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1])),
            },
        );
        denied(&outcome);
        assert!(outcome.state.pending().is_none());
    }
}

#[test]
fn invalidation_fences_inflight_and_not_yet_begun_plans_for_every_reason() {
    let layout = cmis(Rev::CMIS_5_3);
    let known = verified(target(0, 1), &layout);
    let old_plan = selector_plan(&known, target(0, 1), &layout, OperationId(2)).unwrap();
    for reason in [
        SelectorInvalidation::DataTransferFailed,
        SelectorInvalidation::SelectorWrite,
        SelectorInvalidation::Control26Write,
        SelectorInvalidation::CdbCommand,
        SelectorInvalidation::Reset,
        SelectorInvalidation::PowerTransition,
        SelectorInvalidation::PowerUncertain,
        SelectorInvalidation::ProfileUncertain,
        SelectorInvalidation::OwnershipUncertain,
        SelectorInvalidation::Explicit,
    ] {
        let active = selector_begin(&known, &old_plan);
        let old = active.pending().unwrap().stamp;
        let retired = selector_invalidate(&active, reason, OperationId(3));
        assert_eq!(
            retired.knowledge(ModuleGeneration(1), CacheEpoch(2)),
            SelectorKnowledge::Poisoned
        );
        assert!(retired.pending().is_none());
        denied(&selector_complete(
            &retired,
            &SelectorSuppliedResult {
                stamp: old,
                result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1])),
            },
        ));
        let invalid = selector_invalidate(&known, reason, OperationId(3));
        assert!(selector_begin(&invalid, &old_plan).pending().is_none());
        let recovering = start(&invalid, target(0, 1), &layout, 4);
        assert!(matches!(
            recovering.pending().unwrap().step,
            SelectorStep::Readback {
                purpose: ReadbackPurpose::Initial,
                ..
            }
        ));
    }
}

#[test]
fn attempt_ids_are_not_reusable_and_busy_planning_is_refused() {
    let layout = cmis(Rev::CMIS_4_0);
    let known = verified(target(0, 1), &layout);
    assert_eq!(
        selector_plan(&known, target(0, 1), &layout, OperationId(1)),
        Err(SelectorRefusal::RetiredAttempt)
    );
    let plan = selector_plan(&known, target(0, 1), &layout, OperationId(2)).unwrap();
    let state = selector_begin(&known, &plan);
    assert_eq!(
        selector_plan(&state, target(0, 1), &layout, OperationId(3)),
        Err(SelectorRefusal::Busy)
    );
    assert!(selector_begin(&state, &plan).pending().is_none());
    let done = read(&state, &[0, 1]);
    assert!(selector_begin(&done.state, &plan).pending().is_none());
}

#[test]
fn wrong_result_kind_and_duplicate_completion_poison() {
    let layout = cmis(Rev::CMIS_4_0);
    let state = start(&fresh(), target(0, 1), &layout, 1);
    let wrong = supply(&state, SelectorResult::Write(ActuationOutcome::Confirmed));
    denied(&wrong);
    assert_eq!(
        wrong.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Poisoned
    );
    let state = read(&state, &[0, 1]).state;
    let stamp = state.pending().unwrap().stamp;
    let done = read(&state, &[0, 1]);
    let duplicate = selector_complete(
        &done.state,
        &SelectorSuppliedResult {
            stamp,
            result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 1])),
        },
    );
    denied(&duplicate);
    assert_eq!(
        duplicate
            .state
            .knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Poisoned
    );
}

#[test]
fn every_knowledge_state_loses_authority_at_either_boundary() {
    let layout = cmis(Rev::CMIS_4_0);
    let known = verified(target(0, 1), &layout);
    let poisoned = selector_invalidate(&known, SelectorInvalidation::Explicit, OperationId(2));
    for state in [fresh(), known, poisoned] {
        for (generation, epoch) in [
            (ModuleGeneration(2), CacheEpoch(2)),
            (ModuleGeneration(1), CacheEpoch(3)),
        ] {
            assert_eq!(
                state.knowledge(generation, epoch),
                SelectorKnowledge::Unknown
            );
            let retired = state.at(generation, epoch);
            assert_eq!(retired.generation(), generation);
            assert_eq!(retired.epoch(), epoch);
            assert_eq!(
                retired.knowledge(generation, epoch),
                SelectorKnowledge::Unknown
            );
            assert!(retired.pending().is_none());
        }
    }
    let plan = selector_plan(&fresh(), target(0, 1), &layout, OperationId(1)).unwrap();
    let changed = fresh().at(ModuleGeneration(2), CacheEpoch(2));
    let rejected = selector_begin(&changed, &plan);
    assert_eq!(
        rejected.knowledge(ModuleGeneration(2), CacheEpoch(2)),
        SelectorKnowledge::Poisoned
    );
    assert!(rejected.pending().is_none());
}

#[test]
fn sff_confirmation_skips_writes_and_malformed_or_effectful_footprints_refuse() {
    let layout = sff();
    assert_eq!(layout.revision(), None);
    let state = start(&fresh(), target(0, 1), &layout, 1);
    let state = read(&state, &[1]).state;
    let known = read(&state, &[1]).state;
    let plan = selector_plan(&known, target(0, 1), &layout, OperationId(2)).unwrap();
    assert_eq!(plan.steps().len(), 1);
    let state = selector_begin(&known, &plan);
    assert_eq!(
        read(&state, &[1]).data_permission,
        DataPermission::Granted(target(0, 1))
    );
    let SelectorStep::Readback {
        read: qualified, ..
    } = plan.steps()[0].clone()
    else {
        panic!("read");
    };
    let mut variants = vec![];
    let mut wrong = qualified.clone();
    wrong.physical_range.offset = 126;
    variants.push(wrong);
    let mut wrong = qualified.clone();
    wrong.classifications.clear();
    variants.push(wrong);
    let mut wrong = qualified.clone();
    wrong.declared_effects.clear();
    variants.push(wrong);
    let mut wrong = qualified.clone();
    wrong.declared_effects.push(ReadEffect::ClearOnRead);
    variants.push(wrong);
    for field in 0..5 {
        let mut wrong = qualified.clone();
        match field {
            0 => wrong.classifications[0].readable_mask = 0xfe,
            1 => wrong.classifications[0].unknown_mask = 1,
            2 => wrong.classifications[0].uncovered_mask = 1,
            3 => wrong.classifications[0].write_only_mask = 1,
            _ => wrong.classifications[0].known_effects = vec![ReadEffect::Unknown],
        }
        variants.push(wrong);
    }
    for wrong in variants {
        assert_eq!(
            SelectorLayout::sff8636_page_only(wrong),
            Err(SelectorRefusal::UnqualifiedReadback)
        );
    }
}

#[test]
fn resolved_plan_is_ordered_and_read_results_cannot_complete_writes() {
    let layout = cmis(Rev::CMIS_5_3);
    let state = start(&fresh(), target(3, 0x11), &layout, 1);
    let stamp = state.pending().unwrap().stamp;
    let state = read(&state, &[2, 0x10]).state;
    assert_eq!(state.remaining_steps().len(), 3);
    assert!(
        matches!(state.remaining_steps()[0], SelectorStep::Write { range, value: 3 } if range.offset == 126)
    );
    assert!(
        matches!(state.remaining_steps()[1], SelectorStep::Write { range, value: 0x11 } if range.offset == 127)
    );
    assert!(matches!(
        state.remaining_steps()[2],
        SelectorStep::Readback {
            purpose: ReadbackPurpose::Verification,
            ..
        }
    ));
    let wrong = read(&state, &[3, 0x11]);
    denied(&wrong);
    assert_eq!(
        wrong.state.knowledge(ModuleGeneration(1), CacheEpoch(2)),
        SelectorKnowledge::Poisoned
    );
    let repeated = selector_complete(
        &state,
        &SelectorSuppliedResult {
            stamp,
            result: SelectorResult::Read(RangeOutcome::Bytes(vec![3, 0x11])),
        },
    );
    denied(&repeated);
    assert!(repeated.state.pending().is_none());
}
