//! The owner supplies correlated live completions, never encoding/cache bytes.
use openxcvr::{
    core::{types::management::*, Rev},
    programming::{
        prepare_named_vendor_programming, ChecksumPolicy, FieldValue,
        NamedProgrammingContinuation as Continuation, PreparedNamedProgramming,
        ProgrammingComparisonPrelude, ProgrammingContinuationRefusal as Refusal,
        ProgrammingReadReceipt, ProgrammingRefusal, VendorProgrammingOptions,
        VendorProgrammingScope,
    },
};
use openxcvr_cmis::{
    read_planning::CmisReadPlanner,
    vendor_programming::{
        plan_vendor_programming_with_options, ProgrammingDisposition, VendorProgrammingObservation,
        VendorProgrammingStep,
    },
};

const PASSWORD: [u8; 4] = [17, 34, 51, 68];
fn scope() -> VendorProgrammingScope {
    VendorProgrammingScope {
        operation: OperationId(11),
        generation: ModuleGeneration(12),
        epoch: CacheEpoch(13),
        exclusive: true,
        started_at: MonoTime(100),
        now: MonoTime(200),
    }
}
fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}
fn options(policy: ChecksumPolicy) -> VendorProgrammingOptions {
    VendorProgrammingOptions {
        checksum_policy: policy,
    }
}
fn prepare(policy: ChecksumPolicy) -> PreparedNamedProgramming {
    prepare_named_vendor_programming(
        Rev::CMIS_4_0,
        Some(&PASSWORD),
        &[("vendor_rev", FieldValue::Text("A1"))],
        options(policy),
    )
    .unwrap()
}
fn prelude() -> ProgrammingComparisonPrelude {
    prepare(ChecksumPolicy::Maintain)
        .comparison_prelude(scope())
        .unwrap()
}
fn page(equal: bool, valid: bool) -> Vec<u8> {
    let mut page = vec![0; 95];
    // Reuse the canonical checksum authority rather than a second algorithm.
    let named = openxcvr::programming::plan_named_vendor_programming(
        Rev::CMIS_4_0,
        Some(&PASSWORD),
        &[0; 128],
        &[("vendor_rev", FieldValue::Text("A1"))],
    )
    .unwrap();
    let start = usize::from(named.fields[0].range.offset) - 128;
    page[start..start + 2].copy_from_slice(if equal { b"A1" } else { b"B1" });
    page[94] = named.plan.checksum(&page[..94]).unwrap();
    if !valid {
        page[94] ^= 1;
    }
    page
}
fn receipts(equal: bool, valid: bool) -> Vec<ProgrammingReadReceipt> {
    [vec![0x40], vec![0, 0], page(equal, valid)]
        .into_iter()
        .enumerate()
        .map(|(step, bytes)| ProgrammingReadReceipt {
            operation: scope().operation,
            step,
            confirmed: true,
            observation: RangeObservation {
                range: [range(1, 1), range(126, 2), range(128, 95)][step],
                outcome: RangeOutcome::Bytes(bytes),
                requester: ReadRequester::ExplicitClient,
                stamp: ObservationStamp {
                    generation: scope().generation,
                    epoch: scope().epoch,
                    started_at: MonoTime(110 + step as u64 * 20),
                    completed_at: MonoTime(120 + step as u64 * 20),
                    source: ObservationSource::DaemonObserved,
                    freshness: Freshness::Fresh,
                    consumed_effects: vec![],
                },
            },
        })
        .collect()
}
fn assert_refusal(reads: &[ProgrammingReadReceipt], check: impl FnOnce(&Refusal) -> bool) {
    match prelude().continue_programming(Some(&PASSWORD), reads, scope()) {
        Err(error) => assert!(check(&error), "unexpected refusal: {error:?}"),
        Ok(_) => panic!("released a continuation from invalid receipts"),
    }
}

#[test]
fn preparation_needs_no_image_for_every_admitted_field() {
    for (name, value) in [
        ("vendor_name", FieldValue::Text("ABCDEFGHIJKLMNOP")),
        ("vendor_oui", FieldValue::Bytes(&[1, 2, 3])),
        ("vendor_pn", FieldValue::Text("ABCDEFGHIJKLMNOP")),
        ("vendor_rev", FieldValue::Text("A1")),
        ("vendor_sn", FieldValue::Text("ABCDEFGHIJKLMNOP")),
        ("date_code_year", FieldValue::Text("26")),
        ("date_code_month", FieldValue::Text("10")),
        ("date_code_day_of_month", FieldValue::Text("24")),
        ("date_code_lot_code", FieldValue::Text("AB")),
    ] {
        let prepared = prepare_named_vendor_programming(
            Rev::CMIS_4_0,
            Some(&PASSWORD),
            &[(name, value)],
            Default::default(),
        )
        .unwrap();
        assert_eq!(prepared.fields().len(), 1);
        assert_eq!(prepared.fields()[0].name, name);
        let description = prepared.description();
        assert_eq!(description.revision, Rev::CMIS_4_0);
        assert_eq!(description.fields, prepared.fields());
        assert_eq!(description.ranges.len(), 1);
    }
}

#[test]
fn preparation_refuses_the_whole_request_before_any_prelude() {
    for proposals in [
        vec![],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("vendor_rev", FieldValue::Text("B1")),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("unknown", FieldValue::Text("x")),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("vendor_*", FieldValue::Text("x")),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("page_checksum", FieldValue::Bytes(&[0])),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("vendor_oui", FieldValue::Text("abc")),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("vendor_oui", FieldValue::Bytes(&[0; 4])),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("clei_code", FieldValue::Text("0123456789")),
        ],
        vec![
            ("vendor_rev", FieldValue::Text("A1")),
            ("low_pwr_request_sw", FieldValue::Bytes(&[1])),
        ],
        vec![("vendor_rev", FieldValue::Text("A\n"))],
    ] {
        assert!(matches!(
            prepare_named_vendor_programming(
                Rev::CMIS_4_0,
                Some(&PASSWORD),
                &proposals,
                Default::default()
            ),
            Err(Refusal::Proposal(_))
        ));
    }
    assert!(matches!(
        prepare_named_vendor_programming(
            Rev::CMIS_5_3,
            Some(&PASSWORD),
            &[("vendor_rev", FieldValue::Text("A1"))],
            Default::default()
        ),
        Err(Refusal::Proposal(ProgrammingRefusal::UnsupportedRevision(
            Rev::CMIS_5_3
        )))
    ));
    assert!(matches!(
        prepare_named_vendor_programming(
            Rev::CMIS_4_0,
            None,
            &[("vendor_rev", FieldValue::Text("A1"))],
            Default::default()
        ),
        Err(Refusal::Proposal(ProgrammingRefusal::Vendor(_)))
    ));
}

#[test]
fn prelude_is_exact_ordered_qualified_40_metadata_not_a_selector_grant() {
    let prelude = prelude();
    assert_eq!(prelude.revision(), Rev::CMIS_4_0);
    assert_eq!(
        prelude.required_selection(),
        Selection {
            address: 0x50,
            bank: 0,
            page: 0
        }
    );
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    for (step, expected) in prelude
        .reads()
        .iter()
        .zip([range(1, 1), range(126, 2), range(128, 95)])
    {
        assert_eq!(step.physical_range, expected);
        assert_eq!(step.requester, ReadRequester::ExplicitClient);
        assert_eq!(step.declared_effects, [ReadEffect::None]);
        assert_eq!(step.classifications.len(), usize::from(expected.len));
        assert!(step
            .classifications
            .iter()
            .all(ByteClassification::is_qualified_none));
        assert!(matches!(
            planner.classify_range(Rev::CMIS_4_0, expected),
            Ok(RangeClassified::Passive { .. })
        ));
    }
    let legacy = openxcvr::programming::plan_named_vendor_programming(
        Rev::CMIS_4_0,
        Some(&PASSWORD),
        &[0; 128],
        &[("vendor_rev", FieldValue::Text("A1"))],
    )
    .unwrap();
    let VendorProgrammingStep::ConfirmPage00 { read, .. } = &legacy.plan.steps[0] else {
        panic!()
    };
    assert_eq!(read.classifications, prelude.reads()[1].classifications);
    // Legacy metadata/effect declarations are not overwritten by the prelude.
    assert!(read.declared_effects.is_empty());
    assert!(matches!(
        planner.classify_range(Rev::CMIS_4_0, range(223, 1)),
        Ok(RangeClassified::NonPassive { .. })
    ));
    let reads = receipts(true, true);
    for count in 0..=3 {
        prelude.validate_prefix(&reads[..count], scope()).unwrap();
    }
}

#[test]
fn equal_valid_is_sealed_no_change_under_both_policies() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let continuation = prepare(policy)
            .comparison_prelude(scope())
            .unwrap()
            .continue_programming(Some(&PASSWORD), &receipts(true, true), scope())
            .unwrap();
        let Continuation::VerifiedNoChange(proof) = continuation else {
            panic!("expected no change")
        };
        assert_eq!(proof.fields()[0].name, "vendor_rev");
        assert!(proof.checksum().valid);
        assert_eq!(proof.options(), options(policy));
        proof.validate_scope(scope()).unwrap();
    }
}

#[test]
fn equal_invalid_repairs_by_default_but_preserve_reports_invalid() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let continuation = prepare(policy)
            .comparison_prelude(scope())
            .unwrap()
            .continue_programming(Some(&PASSWORD), &receipts(true, false), scope())
            .unwrap();
        match (policy, continuation) {
            (ChecksumPolicy::Maintain, Continuation::Execute(execution)) => {
                assert!(!execution.policy_plan().initial_checksum.unwrap().valid);
                assert!(execution
                    .policy_plan()
                    .plan
                    .steps
                    .iter()
                    .any(|step| matches!(step, VendorProgrammingStep::Checksum { .. })));
            }
            (ChecksumPolicy::Preserve, Continuation::VerifiedNoChange(proof)) => {
                assert!(!proof.checksum().valid)
            }
            _ => panic!("incorrect checksum policy decision"),
        }
    }
}

#[test]
fn differing_byte_releases_only_the_existing_canonical_policy_envelope() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        for valid in [false, true] {
            let prepared = prepare(policy);
            let description = prepared.description();
            let ranges = description.ranges.to_vec();
            let reads = receipts(false, valid);
            let observed = VendorProgrammingObservation {
                operation: scope().operation,
                stamp: &reads[2].observation.stamp,
                selectors: [0, 0],
                page: reads[2].observation.bytes().unwrap(),
            };
            let canonical = plan_vendor_programming_with_options(
                Rev::CMIS_4_0,
                Some(&PASSWORD),
                &ranges,
                options(policy),
                scope(),
                Some(&observed),
            )
            .unwrap();
            let Continuation::Execute(execution) = prepared
                .comparison_prelude(scope())
                .unwrap()
                .continue_programming(Some(&PASSWORD), &reads, scope())
                .unwrap()
            else {
                panic!("expected execution")
            };
            assert_eq!(execution.policy_plan(), &canonical);
            assert_eq!(execution.fields()[0].name, "vendor_rev");
            assert_eq!(execution.description().ranges, ranges);
            assert_eq!(canonical.finish_reads[0].physical_range, range(128, 95));
            assert_eq!(
                canonical.disposition,
                ProgrammingDisposition::RequiresExecution
            );
            assert!(matches!(
                canonical.plan.steps[0],
                VendorProgrammingStep::ConfirmPage00 { .. }
            ));
            assert!(matches!(
                canonical.plan.steps[1],
                VendorProgrammingStep::PasswordEntry { .. }
            ));
        }
    }
}

#[test]
fn missing_duplicate_out_of_order_extra_and_wrong_coordinate_receipts_refuse() {
    let reads = receipts(true, true);
    for count in 0..3 {
        assert_refusal(
            &reads[..count],
            |e| matches!(e, Refusal::UnexpectedRead { step } if *step == count),
        );
    }
    let mut reads = receipts(true, true);
    reads[1].step = 0;
    assert_refusal(&reads, |e| matches!(e, Refusal::UnexpectedRead { step: 1 }));
    let mut reads = receipts(true, true);
    reads.swap(0, 1);
    assert_refusal(&reads, |e| matches!(e, Refusal::UnexpectedRead { step: 0 }));
    let mut reads = receipts(true, true);
    reads.push(receipts(true, true).remove(2));
    assert_refusal(&reads, |e| matches!(e, Refusal::UnexpectedRead { step: 3 }));
    for bad in [
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
        range(129, 95),
        range(128, 94),
    ] {
        let mut reads = receipts(true, true);
        reads[2].observation.range = bad;
        assert_refusal(&reads, |e| matches!(e, Refusal::UnexpectedRead { step: 2 }));
    }
}

#[test]
fn unconfirmed_failed_absent_uncertain_short_or_long_reads_never_release_a_plan() {
    for step in 0..3 {
        let mut reads = receipts(true, true);
        reads[step].confirmed = false;
        assert_refusal(
            &reads,
            |e| matches!(e, Refusal::UnconfirmedRead { step: s } if *s == step),
        );
        for outcome in [
            RangeOutcome::Absent,
            RangeOutcome::Failed {
                reason: "transport".into(),
            },
            RangeOutcome::Uncertain {
                reason: "timeout".into(),
            },
        ] {
            let mut reads = receipts(true, true);
            reads[step].observation.outcome = outcome;
            assert_refusal(
                &reads,
                |e| matches!(e, Refusal::UnconfirmedRead { step: s } if *s == step),
            );
        }
        for delta in [-1, 1] {
            let mut reads = receipts(true, true);
            let expected = reads[step].observation.range.len;
            let received = (i32::from(expected) + delta) as usize;
            reads[step].observation.outcome = RangeOutcome::Bytes(vec![0; received]);
            assert_refusal(
                &reads,
                |e| matches!(e, Refusal::ShortRead { expected: n, received: m, .. } if *n == expected && *m == received),
            );
        }
    }
}

#[test]
fn receipt_operation_requester_freshness_source_and_consumed_effects_are_bound() {
    for step in 0..3 {
        for variant in 0..5 {
            let mut reads = receipts(true, true);
            match variant {
                0 => reads[step].operation = OperationId(99),
                1 => reads[step].observation.requester = ReadRequester::PassiveInspection,
                2 => reads[step].observation.stamp.freshness = Freshness::Historical,
                3 => reads[step].observation.stamp.source = ObservationSource::Supplied,
                _ => reads[step].observation.stamp.consumed_effects = vec![ReadEffect::None],
            }
            assert_refusal(&reads, |e| matches!(e, Refusal::ContextMismatch));
        }
    }
}

#[test]
fn stale_generation_epoch_and_reversed_intervals_are_typed_input_refusals() {
    for step in 0..3 {
        for variant in 0..3 {
            let mut reads = receipts(true, true);
            match variant {
                0 => reads[step].observation.stamp.generation = ModuleGeneration(99),
                1 => reads[step].observation.stamp.epoch = CacheEpoch(99),
                _ => reads[step].observation.stamp.completed_at = MonoTime(0),
            }
            assert_refusal(&reads, |e| matches!(e, Refusal::Input(_)));
        }
    }
}

#[test]
fn receipts_must_be_ordered_and_inside_the_same_exclusion_interval() {
    for variant in 0..4 {
        let mut reads = receipts(true, true);
        match variant {
            0 => reads[0].observation.stamp.started_at = MonoTime(99),
            1 => reads[2].observation.stamp.completed_at = MonoTime(201),
            2 => reads[1].observation.stamp.started_at = MonoTime(119),
            _ => {
                reads[2].observation.stamp.started_at = MonoTime(110);
                reads[2].observation.stamp.completed_at = MonoTime(120);
            }
        }
        assert_refusal(&reads, |e| matches!(e, Refusal::ContextMismatch));
    }
}

#[test]
fn revision_and_selector_mismatch_refuse_the_prefix_before_dependent_reads() {
    let mut reads = receipts(true, true);
    reads[0].observation.outcome = RangeOutcome::Bytes(vec![0x53]);
    assert!(matches!(
        prelude().validate_prefix(&reads[..1], scope()),
        Err(Refusal::RevisionMismatch {
            expected: Rev::CMIS_4_0,
            reported: 0x53
        })
    ));
    assert_refusal(&reads, |e| matches!(e, Refusal::RevisionMismatch { .. }));
    for selector in [[1, 0], [0, 1], [1, 1]] {
        let mut reads = receipts(true, true);
        reads[1].observation.outcome = RangeOutcome::Bytes(selector.to_vec());
        assert!(matches!(
            prelude().validate_prefix(&reads[..2], scope()),
            Err(Refusal::SelectorChangeRequired { .. })
        ));
        assert_refusal(&reads, |e| {
            matches!(e, Refusal::SelectorChangeRequired { .. })
        });
    }
}

#[test]
fn scope_loss_reacquisition_or_boundary_changes_cannot_revive_continuation() {
    for variant in 0..7 {
        let mut current = scope();
        match variant {
            0 => current.exclusive = false,
            1 => current.operation = OperationId(99),
            2 => current.generation = ModuleGeneration(99),
            3 => current.epoch = CacheEpoch(99),
            4 => current.started_at = MonoTime(101),
            5 => current.now = MonoTime(199),
            _ => current.started_at = MonoTime(201),
        }
        assert!(matches!(
            prelude().validate_prefix(&[], current),
            Err(Refusal::ScopeLost)
        ));
        assert!(matches!(
            prelude().continue_programming(Some(&PASSWORD), &receipts(true, true), current),
            Err(Refusal::ScopeLost)
        ));
        let Continuation::VerifiedNoChange(proof) = prelude()
            .continue_programming(Some(&PASSWORD), &receipts(true, true), scope())
            .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            proof.validate_scope(current),
            Err(Refusal::ScopeLost)
        ));
        assert!(proof.outcome_facts(current).is_err());
    }
    let mut bad = scope();
    bad.exclusive = false;
    assert!(matches!(
        prepare(ChecksumPolicy::Maintain).comparison_prelude(bad),
        Err(Refusal::ScopeLost)
    ));
    let mut bad = scope();
    bad.now = MonoTime(99);
    assert!(matches!(
        prepare(ChecksumPolicy::Maintain).comparison_prelude(bad),
        Err(Refusal::ScopeLost)
    ));
}

#[test]
fn local_scratch_or_current_encoding_is_never_a_comparison_receipt() {
    assert_refusal(&[], |e| matches!(e, Refusal::UnexpectedRead { step: 0 }));
    let mut reads = receipts(true, true);
    reads[2].observation.stamp.source = ObservationSource::Supplied;
    assert_refusal(&reads, |e| matches!(e, Refusal::ContextMismatch));
    // Even a whole encoding image is not an exact 95-byte completion.
    reads[2].observation.stamp.source = ObservationSource::DaemonObserved;
    reads[2].observation.outcome = RangeOutcome::Bytes(vec![0; 128]);
    assert_refusal(&reads, |e| {
        matches!(e, Refusal::ShortRead { received: 128, .. })
    });
}

#[test]
fn continuation_rechecks_password_presence_even_for_equal_bytes() {
    assert!(matches!(
        prelude().continue_programming(None, &receipts(true, true), scope()),
        Err(Refusal::Policy(_))
    ));
}

#[test]
fn no_change_outcome_facts_require_the_sealed_proof_and_live_scope() {
    let Continuation::VerifiedNoChange(proof) = prepare(ChecksumPolicy::Preserve)
        .comparison_prelude(scope())
        .unwrap()
        .continue_programming(Some(&PASSWORD), &receipts(true, false), scope())
        .unwrap()
    else {
        panic!()
    };
    let facts = proof.outcome_facts(scope()).unwrap();
    assert_eq!(facts.evidence(), "stored_bytes_verified");
    assert_eq!(facts.target_write(), "not_submitted");
    assert_eq!(facts.target_readback(), "matched");
    assert!(facts.no_change());
    assert!(!facts.possible_application());
    assert!(!facts.initial_checksum().valid);
    assert_eq!(facts.initial_checksum(), facts.final_checksum());
    let description = proof.description();
    assert_eq!(description.options, options(ChecksumPolicy::Preserve));
    assert_eq!(description.fields, proof.fields());
    assert_eq!(description.ranges[0].1, b"A1");
}

#[test]
fn a_mixed_multifield_request_executes_all_canonical_targets_not_just_differences() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let proposals = [
            ("vendor_oui", FieldValue::Bytes(&[1, 2, 3][..])),
            ("vendor_rev", FieldValue::Text("A1")),
        ];
        let prepared = prepare_named_vendor_programming(
            Rev::CMIS_4_0,
            Some(&PASSWORD),
            &proposals,
            options(policy),
        )
        .unwrap();
        let ranges = prepared.description().ranges.to_vec();
        let reads = receipts(true, true); // rev equal, OUI different
        let observed = VendorProgrammingObservation {
            operation: scope().operation,
            stamp: &reads[2].observation.stamp,
            selectors: [0, 0],
            page: reads[2].observation.bytes().unwrap(),
        };
        let canonical = plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&PASSWORD),
            &ranges,
            options(policy),
            scope(),
            Some(&observed),
        )
        .unwrap();
        let Continuation::Execute(execution) = prepared
            .comparison_prelude(scope())
            .unwrap()
            .continue_programming(Some(&PASSWORD), &reads, scope())
            .unwrap()
        else {
            panic!("not all fields equal")
        };
        assert_eq!(execution.policy_plan(), &canonical);
        assert_eq!(execution.fields().len(), 2);
        for (range, _) in ranges {
            assert!(execution.policy_plan().plan.steps.iter().any(|step| matches!(step, VendorProgrammingStep::Write { range: written, .. } if *written == range)));
        }
    }
}

#[test]
fn no_change_is_only_sealed_when_canonical_work_and_finish_reads_are_empty() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        for valid in [false, true] {
            if policy == ChecksumPolicy::Maintain && !valid {
                continue;
            }
            let prepared = prepare(policy);
            let ranges = prepared.description().ranges.to_vec();
            let reads = receipts(true, valid);
            let observed = VendorProgrammingObservation {
                operation: scope().operation,
                stamp: &reads[2].observation.stamp,
                selectors: [0, 0],
                page: reads[2].observation.bytes().unwrap(),
            };
            let canonical = plan_vendor_programming_with_options(
                Rev::CMIS_4_0,
                Some(&PASSWORD),
                &ranges,
                options(policy),
                scope(),
                Some(&observed),
            )
            .unwrap();
            assert!(canonical.plan.steps.is_empty());
            assert!(canonical.plan.invalidations.is_empty());
            assert!(canonical.plan.unconfirmed_step_retries.is_empty());
            assert!(canonical.finish_reads.is_empty());
            assert!(canonical.finish_read_retries.is_empty());
            assert!(matches!(
                prepared
                    .comparison_prelude(scope())
                    .unwrap()
                    .continue_programming(Some(&PASSWORD), &reads, scope()),
                Ok(Continuation::VerifiedNoChange(_))
            ));
        }
    }
}

#[test]
fn proof_bearing_values_are_not_clone() {
    // If any type gains Clone, both impls fit `_` and inference fails. Unlike
    // assert_not_clone(&value), this checks the opaque type, not its reference.
    trait AmbiguousIfClone<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfClone<()> for T {}
    struct Invalid;
    impl<T: Clone> AmbiguousIfClone<Invalid> for T {}
    use openxcvr::programming::{
        NamedProgrammingExecution, NamedProgrammingNoChangeFacts, VerifiedNamedNoChange,
    };
    let _ = <PreparedNamedProgramming as AmbiguousIfClone<_>>::check;
    let _ = <ProgrammingComparisonPrelude as AmbiguousIfClone<_>>::check;
    let _ = <NamedProgrammingExecution as AmbiguousIfClone<_>>::check;
    let _ = <VerifiedNamedNoChange as AmbiguousIfClone<_>>::check;
    let _ = <NamedProgrammingNoChangeFacts<'_> as AmbiguousIfClone<_>>::check;
}
