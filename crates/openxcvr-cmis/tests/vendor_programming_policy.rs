use openxcvr_cmis::{
    generated::v4_0::CmisPage00hAdministrative as Page,
    vendor_programming::{
        plan_vendor_programming, plan_vendor_programming_with_options, ChecksumPolicy,
        ProgrammingDisposition, ProgrammingValue, VendorProgrammingObservation,
        VendorProgrammingOptions, VendorProgrammingPolicyRefusal as Refusal,
        VendorProgrammingScope, VendorProgrammingStep as Step,
    },
};
use openxcvr_core::{
    types::management::{
        CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp,
        OperationId, Range,
    },
    Rev,
};

fn scope() -> VendorProgrammingScope {
    VendorProgrammingScope {
        operation: OperationId(7),
        generation: ModuleGeneration(2),
        epoch: CacheEpoch(3),
        exclusive: true,
        started_at: MonoTime(10),
        now: MonoTime(20),
    }
}
fn stamp() -> ObservationStamp {
    ObservationStamp {
        generation: scope().generation,
        epoch: scope().epoch,
        started_at: MonoTime(11),
        completed_at: MonoTime(12),
        source: ObservationSource::DaemonObserved,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    }
}
fn r(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}
fn image() -> [u8; 128] {
    let mut page = [7; 128];
    Page::recompute_checksums(&mut page);
    page
}
fn observation<'a>(
    page: &'a [u8],
    stamp: &'a ObservationStamp,
) -> VendorProgrammingObservation<'a> {
    VendorProgrammingObservation {
        operation: scope().operation,
        stamp,
        selectors: [0, 0],
        page,
    }
}
fn options(policy: ChecksumPolicy) -> VendorProgrammingOptions {
    VendorProgrammingOptions {
        checksum_policy: policy,
    }
}

#[test]
fn default_retains_legacy_plan_and_adds_required_checksum_validity_readback() {
    let ranges = [(r(145, 3), vec![1, 2, 3])];
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &ranges,
        VendorProgrammingOptions::default(),
        scope(),
        None,
    )
    .unwrap();
    assert_eq!(
        plan.plan,
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap()
    );
    assert_eq!(plan.checksum_policy, ChecksumPolicy::Maintain);
    assert_eq!(plan.disposition, ProgrammingDisposition::RequiresExecution);
    assert_eq!(plan.finish_reads.len(), 1);
    assert_eq!(plan.finish_reads[0].physical_range, r(128, 95));
    assert!(plan.plan.steps.iter().any(|s| matches!(s, Step::Write { range, value: ProgrammingValue::Checksum } if *range == r(222, 1))));
    assert!(plan.plan.steps.iter().any(|s| matches!(s, Step::VerifyRead { read, expected: ProgrammingValue::Checksum } if read.physical_range == r(222, 1))));
}

#[test]
fn opt_out_keeps_data_readback_and_reports_invalid_checksum() {
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(r(145, 3), vec![1, 2, 3])],
        options(ChecksumPolicy::Preserve),
        scope(),
        None,
    )
    .unwrap();
    assert!(!plan
        .plan
        .steps
        .iter()
        .any(|s| matches!(s, Step::Write { range, .. } if *range == r(222, 1))));
    assert!(plan.plan.steps.iter().any(|s| matches!(s, Step::VerifyRead { read, expected: ProgrammingValue::Supplied(_) } if read.physical_range == r(145, 3))));
    assert_eq!(plan.finish_reads[0].physical_range, r(128, 95));
    let mut page = image();
    page[94] ^= 1;
    assert!(!plan.checksum_evidence(&page[..95]).unwrap().valid);
    assert_eq!(
        plan.plan.steps.len(),
        plan.plan.unconfirmed_step_retries.len()
    );
}

#[test]
fn fresh_equality_is_no_change_and_has_no_password_payload_or_checksum_write() {
    let page = image();
    let stamp = stamp();
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let plan = plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &[(r(145, 3), page[17..20].to_vec())],
            options(policy),
            scope(),
            Some(&observation(&page[..95], &stamp)),
        )
        .unwrap();
        assert_eq!(plan.disposition, ProgrammingDisposition::VerifiedNoChange);
        assert!(plan.plan.steps.is_empty());
        assert!(plan.plan.invalidations.is_empty());
        assert!(plan.finish_reads.is_empty());
        assert!(plan.initial_checksum.unwrap().valid);
    }
}

#[test]
fn differing_byte_or_invalid_default_checksum_requires_execution() {
    let mut page = image();
    let stamp = stamp();
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(r(145, 3), vec![7, 7, 8])],
        VendorProgrammingOptions::default(),
        scope(),
        Some(&observation(&page[..95], &stamp)),
    )
    .unwrap();
    assert_eq!(plan.disposition, ProgrammingDisposition::RequiresExecution);
    assert!(plan
        .plan
        .steps
        .iter()
        .any(|s| matches!(s, Step::PasswordEntry { .. })));
    page[94] ^= 1;
    let ranges = [(r(145, 3), vec![7; 3])];
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let plan = plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &ranges,
            options(policy),
            scope(),
            Some(&observation(&page[..95], &stamp)),
        )
        .unwrap();
        assert!(!plan.initial_checksum.unwrap().valid);
        assert_eq!(
            plan.disposition,
            if policy == ChecksumPolicy::Maintain {
                ProgrammingDisposition::RequiresExecution
            } else {
                ProgrammingDisposition::VerifiedNoChange
            }
        );
    }
}

#[test]
fn missing_observation_never_infers_no_change_and_short_observation_refuses() {
    let page = image();
    let stamp = stamp();
    let ranges = [(r(145, 3), vec![7; 3])];
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &ranges,
        VendorProgrammingOptions::default(),
        scope(),
        None,
    )
    .unwrap();
    assert_eq!(plan.disposition, ProgrammingDisposition::RequiresExecution);
    assert!(plan.initial_checksum.is_none());
    assert_eq!(
        plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &ranges,
            VendorProgrammingOptions::default(),
            scope(),
            Some(&observation(&page[..94], &stamp))
        )
        .unwrap_err(),
        Refusal::InvalidObservation
    );
}

#[test]
fn cached_stale_wrong_operation_selector_and_time_evidence_refuse() {
    let page = image();
    let ranges = [(r(145, 3), vec![7; 3])];
    for bad in 0..9 {
        let mut stamp = stamp();
        match bad {
            0 => stamp.freshness = Freshness::Historical,
            1 => stamp.source = ObservationSource::Supplied,
            2 => stamp.generation = ModuleGeneration(8),
            3 => stamp.epoch = CacheEpoch(8),
            4 => stamp.started_at = MonoTime(9),
            5 => stamp.completed_at = MonoTime(21),
            6 => stamp.completed_at = MonoTime(0),
            _ => (),
        }
        let mut obs = observation(&page[..95], &stamp);
        if bad == 7 {
            obs.operation = OperationId(8);
        }
        if bad == 8 {
            obs.selectors = [1, 0];
        }
        assert!(plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &ranges,
            VendorProgrammingOptions::default(),
            scope(),
            Some(&obs)
        )
        .is_err());
    }
}

#[test]
fn fence_and_exclusion_loss_refuse_consumption_without_replay() {
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(r(145, 3), vec![1; 3])],
        VendorProgrammingOptions::default(),
        scope(),
        None,
    )
    .unwrap();
    for bad in 0..5 {
        let mut current = scope();
        match bad {
            0 => current.exclusive = false,
            1 => current.generation = ModuleGeneration(8),
            2 => current.epoch = CacheEpoch(8),
            3 => current.operation = OperationId(8),
            _ => current.started_at = MonoTime(11),
        }
        assert_eq!(plan.validate_scope(current), Err(Refusal::ScopeLost));
        assert!(
            plan_vendor_programming_with_options(
                Rev::CMIS_4_0,
                Some(&[0; 4]),
                &[(r(145, 3), vec![1; 3])],
                VendorProgrammingOptions::default(),
                current,
                None
            )
            .is_ok()
                == current.exclusive
        );
    }
    use openxcvr_core::types::management::{operations::RetryDecision, ActuationOutcome};
    assert!(plan
        .plan
        .unconfirmed_step_retries
        .iter()
        .chain(&plan.finish_read_retries)
        .all(|r| matches!(
            r,
            RetryDecision::Terminal(ActuationOutcome::Uncertain { .. })
        )));
}

#[test]
fn checksum_value_is_the_generated_helpers_value_and_short_final_read_refuses() {
    let page = image();
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(r(145, 3), vec![1; 3])],
        VendorProgrammingOptions::default(),
        scope(),
        None,
    )
    .unwrap();
    let evidence = plan.checksum_evidence(&page[..95]).unwrap();
    assert_eq!(evidence.computed, page[94]);
    assert_eq!(evidence.stored, page[94]);
    assert!(evidence.valid);
    assert_eq!(
        plan.checksum_evidence(&page[..94]),
        Err(Refusal::InvalidObservation)
    );
}

// Software executor double only: no hardware qualification. Exercises the
// returned legacy steps followed by the new mandatory final checksum read.
#[test]
fn default_and_opt_out_execute_data_readback_and_publish_actual_checksum_validity() {
    for policy in [ChecksumPolicy::Maintain, ChecksumPolicy::Preserve] {
        let target = vec![1, 2, 3];
        let plan = plan_vendor_programming_with_options(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &[(r(145, 3), target.clone())],
            options(policy),
            scope(),
            None,
        )
        .unwrap();
        let mut page = image();
        let mut fresh = Vec::new();
        let mut checksum = None;
        let mut writes = Vec::new();
        let mut verified = Vec::new();
        for step in &plan.plan.steps {
            plan.validate_scope(scope()).unwrap();
            match step {
                Step::ConfirmPage00 { expected, .. } => assert_eq!(*expected, [0, 0]),
                Step::PasswordEntry { range } => writes.push(*range),
                Step::Write { range, value } => {
                    writes.push(*range);
                    let bytes = match value {
                        ProgrammingValue::Supplied(bytes) => bytes.clone(),
                        ProgrammingValue::Checksum => vec![checksum.unwrap()],
                    };
                    let start = usize::from(range.offset) - 128;
                    page[start..start + bytes.len()].copy_from_slice(&bytes);
                }
                Step::FreshRead(read) => {
                    let start = usize::from(read.physical_range.offset) - 128;
                    fresh = page[start..start + usize::from(read.physical_range.len)].to_vec();
                }
                Step::Checksum { .. } => checksum = Some(plan.plan.checksum(&fresh).unwrap()),
                Step::VerifyRead { read, expected } => {
                    let bytes = match expected {
                        ProgrammingValue::Supplied(bytes) => bytes.clone(),
                        ProgrammingValue::Checksum => vec![checksum.unwrap()],
                    };
                    let start = usize::from(read.physical_range.offset) - 128;
                    assert_eq!(page[start..start + bytes.len()], bytes);
                    verified.push(read.physical_range);
                }
            }
        }
        assert!(verified.contains(&r(145, 3)));
        assert_eq!(page[17..20], target);
        let final_read = &plan.finish_reads[0];
        assert_eq!(final_read.physical_range, r(128, 95));
        let evidence = plan.checksum_evidence(&page[..95]).unwrap();
        assert_eq!(evidence.valid, policy == ChecksumPolicy::Maintain);
        assert_eq!(
            writes.contains(&r(222, 1)),
            policy == ChecksumPolicy::Maintain
        );
    }
}

#[test]
fn some_equal_some_changed_still_plans_and_bad_later_range_never_partially_plans() {
    let page = image();
    let stamp = stamp();
    let ranges = [(r(145, 3), vec![7; 3]), (r(164, 2), vec![7, 8])];
    let plan = plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &ranges,
        VendorProgrammingOptions::default(),
        scope(),
        Some(&observation(&page[..95], &stamp)),
    )
    .unwrap();
    assert_eq!(plan.disposition, ProgrammingDisposition::RequiresExecution);
    assert!(plan
        .plan
        .steps
        .iter()
        .any(|s| matches!(s, Step::Write { range, .. } if *range == r(164, 2))));
    assert!(plan_vendor_programming_with_options(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(r(145, 3), vec![7; 3]), (r(200, 1), vec![8])],
        VendorProgrammingOptions::default(),
        scope(),
        Some(&observation(&page[..95], &stamp)),
    )
    .is_err());
}
