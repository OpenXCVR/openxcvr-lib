use openxcvr_cmis::vendor_programming::{
    plan_vendor_programming, ProgrammingValue, VendorProgrammingRefusal as Refusal,
    VendorProgrammingStep as Step,
};
use openxcvr_core::{types::management::Range, Rev};

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
fn oui_sequence_is_exact_secret_free_and_uses_generated_checksum() {
    let password = [0, 0, 0x10, 0x11];
    let plan = plan_vendor_programming(
        Rev::CMIS_4_0,
        Some(&password),
        &[(range(145, 3), vec![0x48, 0xb0, 0x2d])],
    )
    .unwrap();
    assert!(plan.admission.contains("Table 8-15"));
    assert!(plan.admission.contains("§8.2.10"));
    assert_eq!(plan.steps.len(), 8);
    assert!(
        matches!(&plan.steps[0], Step::ConfirmPage00 { read, expected: [0, 0] } if read.physical_range == range(126, 2))
    );
    assert!(matches!(&plan.steps[1], Step::PasswordEntry { range: r } if *r == range(122, 4)));
    assert!(
        matches!(&plan.steps[2], Step::Write { range: r, value: ProgrammingValue::Supplied(bytes) } if *r == range(145, 3) && bytes == &[0x48, 0xb0, 0x2d])
    );
    assert!(
        matches!(&plan.steps[3], Step::FreshRead(read) if read.physical_range == range(128, 94))
    );
    assert!(
        matches!(&plan.steps[4], Step::Checksum { covers, target } if *covers == range(128, 94) && *target == range(222, 1))
    );
    assert!(
        matches!(&plan.steps[5], Step::Write { range: r, value: ProgrammingValue::Checksum } if *r == range(222, 1))
    );
    assert!(
        matches!(&plan.steps[6], Step::VerifyRead { read, expected: ProgrammingValue::Supplied(bytes) } if read.physical_range == range(145, 3) && bytes == &[0x48, 0xb0, 0x2d])
    );
    assert!(
        matches!(&plan.steps[7], Step::VerifyRead { read, expected: ProgrammingValue::Checksum } if read.physical_range == range(222, 1))
    );
    let fresh = [255u8; 94];
    assert_eq!(plan.checksum(&fresh).unwrap(), 255u8.wrapping_mul(94));
    assert_eq!(
        plan.checksum(&fresh[..93]),
        Err(Refusal::InvalidChecksumInput)
    );
    assert!(!format!("{plan:?}").contains("password: ["));
}

#[test]
fn all_identity_bytes_are_allowed_without_widening_or_field_splitting() {
    let plan = plan_vendor_programming(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[(range(129, 61), vec![7; 61])],
    )
    .unwrap();
    let writes: Vec<_> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Write { range, .. } => Some(*range),
            _ => None,
        })
        .collect();
    assert_eq!(writes, vec![range(129, 61), range(222, 1)]);
}

#[test]
fn malformed_or_unadmitted_input_returns_only_a_typed_refusal() {
    for offset in 0..=255 {
        if (129..=189).contains(&offset) {
            continue;
        }
        assert_eq!(
            plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[(range(offset, 1), vec![0])]),
            Err(Refusal::FieldNotAdmitted(range(offset, 1)))
        );
    }
    for r in [
        range(128, 2),
        range(189, 2),
        Range {
            address: 0x51,
            ..range(145, 3)
        },
        Range {
            bank: 1,
            ..range(145, 3)
        },
        Range {
            page: 1,
            ..range(145, 3)
        },
    ] {
        assert_eq!(
            plan_vendor_programming(
                Rev::CMIS_4_0,
                Some(&[0; 4]),
                &[(r, vec![0; r.len as usize])]
            ),
            Err(Refusal::FieldNotAdmitted(r))
        );
    }
    assert_eq!(
        plan_vendor_programming(Rev::CMIS_4_0, None, &[(range(145, 3), vec![0; 3])]),
        Err(Refusal::MissingPassword)
    );
    assert_eq!(
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[]),
        Err(Refusal::EmptyRanges)
    );
    assert_eq!(
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[(range(145, 0), vec![])]),
        Err(Refusal::EmptyRanges)
    );
    assert_eq!(
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[(range(145, 3), vec![0])]),
        Err(Refusal::InvalidLength(range(145, 3)))
    );
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_3,
        Rev {
            family: openxcvr_core::rev::Family::Sff8472,
            major: 4,
            minor: 0,
        },
    ] {
        assert_eq!(
            plan_vendor_programming(revision, Some(&[0; 4]), &[(range(145, 3), vec![0; 3])]),
            Err(Refusal::UnsupportedRevision(revision))
        );
    }
}

#[test]
fn multiple_ranges_are_verified_individually_and_overlaps_refused() {
    let ranges = [(range(129, 2), vec![1, 2]), (range(188, 2), vec![3, 4])];
    let plan = plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap();
    let verify: Vec<_> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::VerifyRead { read, .. } => Some(read.physical_range),
            _ => None,
        })
        .collect();
    assert_eq!(verify, vec![range(129, 2), range(188, 2), range(222, 1)]);
    assert_eq!(
        plan_vendor_programming(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &[(range(145, 3), vec![1; 3]), (range(146, 1), vec![2])]
        ),
        Err(Refusal::OverlappingRanges)
    );
}

// An intentionally tiny test executor: real I/O, fencing, password lifetime,
// uncertainty settlement and serialization remain daemon-owned, not library API.
fn execute_fake(
    selectors: [u8; 2],
    fail_write: Option<usize>,
) -> (Vec<Range>, Vec<Range>, [u8; 256], Result<(), &'static str>) {
    let password = [0x12, 0x34, 0x56, 0x78];
    let plan = plan_vendor_programming(
        Rev::CMIS_4_0,
        Some(&password),
        &[(range(145, 3), vec![0x48, 0xb0, 0x2d])],
    )
    .unwrap();
    let mut memory = [5u8; 256];
    memory[126..128].copy_from_slice(&selectors);
    let mut reads = vec![];
    let mut writes = vec![];
    let mut fresh = vec![];
    let mut checksum = None;
    let resolve = |value: &ProgrammingValue, checksum: Option<u8>| match value {
        ProgrammingValue::Supplied(bytes) => bytes.clone(),
        ProgrammingValue::Checksum => vec![checksum.unwrap()],
    };
    let result = (|| {
        for step in &plan.steps {
            match step {
                Step::ConfirmPage00 { read, expected } => {
                    reads.push(read.physical_range);
                    if memory[126..128] != *expected {
                        return Err("selector_not_at_page_00h");
                    }
                }
                Step::PasswordEntry { range: r } => {
                    writes.push(*r);
                    memory[122..126].copy_from_slice(&password);
                    // Transport confirmation only. Password area self-clears;
                    // reading it would not verify the entry, even in this fake.
                    memory[122..126].fill(0);
                }
                Step::Write { range: r, value } => {
                    writes.push(*r);
                    let bytes = resolve(value, checksum);
                    memory[r.offset as usize..r.end() as usize].copy_from_slice(&bytes);
                    if fail_write == Some(writes.len()) {
                        return Err("uncertain");
                    }
                }
                Step::FreshRead(read) => {
                    reads.push(read.physical_range);
                    fresh = memory[128..222].to_vec();
                }
                Step::Checksum { .. } => checksum = Some(plan.checksum(&fresh).unwrap()),
                Step::VerifyRead { read, expected } => {
                    let r = read.physical_range;
                    reads.push(r);
                    if memory[r.offset as usize..r.end() as usize] != resolve(expected, checksum) {
                        return Err("verify_mismatch");
                    }
                }
            }
        }
        Ok(())
    })();
    (reads, writes, memory, result)
}

#[test]
fn fake_transport_confirms_selectors_then_programs_and_verifies_without_password_readback() {
    let (reads, writes, memory, result) = execute_fake([0, 0], None);
    assert_eq!(result, Ok(()));
    assert_eq!(writes, vec![range(122, 4), range(145, 3), range(222, 1)]);
    assert_eq!(
        reads,
        vec![range(126, 2), range(128, 94), range(145, 3), range(222, 1)]
    );
    assert_eq!(
        memory[222],
        memory[128..222].iter().fold(0u8, |a, b| a.wrapping_add(*b))
    );
    for selectors in [[1, 0], [0, 1]] {
        let (reads, writes, _, result) = execute_fake(selectors, None);
        assert_eq!(result, Err("selector_not_at_page_00h"));
        assert_eq!(reads, vec![range(126, 2)]);
        assert!(writes.is_empty());
    }
    let (_, writes, _, result) = execute_fake([0, 0], Some(2));
    assert_eq!(result, Err("uncertain"));
    assert_eq!(writes, vec![range(122, 4), range(145, 3)]); // no retry/checksum
}

#[test]
fn plan_lists_exact_operation_end_invalidations_without_selector_effects() {
    use openxcvr_core::types::management::Invalidation;

    let ranges = [(range(145, 3), vec![0x48, 0xb0, 0x2d])];
    let plan = plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap();
    assert_eq!(
        plan.invalidations,
        vec![
            Invalidation::Covered(range(122, 4)),
            Invalidation::Covered(range(145, 3)),
            Invalidation::Covered(range(222, 1)),
            Invalidation::Password,
        ]
    );
    let ranges = [(range(129, 2), vec![1, 2]), (range(188, 2), vec![3, 4])];
    let plan = plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap();
    assert_eq!(
        plan.invalidations,
        vec![
            Invalidation::Covered(range(122, 4)),
            Invalidation::Covered(range(129, 2)),
            Invalidation::Covered(range(188, 2)),
            Invalidation::Covered(range(222, 1)),
            Invalidation::Password,
        ]
    );
}

#[test]
fn every_unconfirmed_step_is_terminal_uncertain_never_retried() {
    use openxcvr_core::types::management::{operations::RetryDecision, ActuationOutcome};

    for ranges in [
        vec![(range(145, 3), vec![0x48, 0xb0, 0x2d])],
        vec![(range(129, 2), vec![1, 2]), (range(188, 2), vec![3, 4])],
    ] {
        let plan = plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap();
        assert_eq!(plan.unconfirmed_step_retries.len(), plan.steps.len());
        for decision in &plan.unconfirmed_step_retries {
            assert_eq!(
                decision,
                &RetryDecision::Terminal(ActuationOutcome::Uncertain {
                    reason: "step-unconfirmed".into(),
                })
            );
        }
    }
}

#[test]
fn validation_row_admits_only_vendor_programming_and_preserves_planner_refusals() {
    use openxcvr_cmis::vendor_programming::{validate_write, WriteRequest};
    use openxcvr_core::types::management::Capabilities;

    let capabilities = Capabilities::default();
    let ranges = vec![(range(145, 3), vec![0x48, 0xb0, 0x2d])];
    let request = WriteRequest::VendorProgramming {
        password: [0; 4],
        ranges: ranges.clone(),
    };
    assert_eq!(
        validate_write(&request, Rev::CMIS_4_0, &capabilities),
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges)
    );
    assert_eq!(
        validate_write(&request, Rev::CMIS_5_3, &capabilities),
        Err(Refusal::UnsupportedRevision(Rev::CMIS_5_3))
    );
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        assert_eq!(
            validate_write(&WriteRequest::Unsupported, revision, &capabilities),
            Err(Refusal::UnsupportedWriteClass)
        );
    }
    for r in [range(122, 4), range(189, 2), range(222, 1)] {
        assert_eq!(
            validate_write(
                &WriteRequest::VendorProgramming {
                    password: [0; 4],
                    ranges: vec![(r, vec![0; r.len as usize])],
                },
                Rev::CMIS_4_0,
                &capabilities,
            ),
            Err(Refusal::FieldNotAdmitted(r))
        );
    }
}
