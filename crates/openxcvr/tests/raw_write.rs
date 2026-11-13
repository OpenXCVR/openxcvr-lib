//! Pure raw-write plan tests: no transport, execution, checksum, password or
//! hardware. The three outcomes under test are STORED, MISMATCH and READFAIL.
use openxcvr::{
    core::{
        rev::{Family, Rev},
        types::management::{
            operations::RetryDecision, ActuationOutcome, Invalidation, Range, Selection,
        },
    },
    raw::{
        preflight, AddressModel, Purpose, RawDescription, RawRegion, RawRequest, RawTarget,
        RefusalReason, UnsupportedCell, WriteBytes,
    },
    raw_write::{
        classify_readback, plan_raw_write, RawReadback, RawWritePlan, RawWritePlanRefusal,
        RawWriteStep, RawWriteVerdict,
    },
};

fn target(offset: u8, len: u16) -> RawTarget {
    RawTarget {
        model: AddressModel::Cmis,
        range: Range {
            address: 0x50,
            bank: 0,
            page: 0,
            offset,
            len,
        },
    }
}

fn description(
    targets: &[RawTarget],
    payloads: &[&[u8]],
    purpose: Purpose,
) -> Result<RawDescription, openxcvr::raw::RawRefusal> {
    let regions: Vec<_> = targets
        .iter()
        .zip(payloads)
        .map(|(&target, payload)| RawRegion {
            target,
            write_bytes: (purpose == Purpose::Write).then(|| WriteBytes::new(payload)),
        })
        .collect();
    preflight(&RawRequest {
        revision: Rev::CMIS_4_0,
        purpose,
        regions: &regions,
    })
}

/// A description whose targets and payloads agree, so planning cannot refuse
/// on payload grounds; target choice may still be inadmissible.
fn planned<'a>(
    targets: &[RawTarget],
    payloads: &[&'a [u8]],
) -> Result<RawWritePlan<'a>, openxcvr::raw::RawRefusal> {
    let description = description(targets, payloads, Purpose::Write)?;
    let counted: Vec<_> = payloads.iter().map(|p| WriteBytes::new(p)).collect();
    plan_raw_write(&description, &counted).map_err(|_| unreachable!("consistent inputs plan"))
}

fn refused_reason(targets: &[RawTarget], payloads: &[&[u8]]) -> RefusalReason {
    description(targets, payloads, Purpose::Write)
        .unwrap_err()
        .reason()
}

fn write_steps(plan: &RawWritePlan<'_>) -> Vec<(Range, Vec<u8>)> {
    plan.steps
        .iter()
        .filter_map(|step| match step {
            RawWriteStep::Write { range, bytes } => Some((*range, bytes.expose().to_vec())),
            _ => None,
        })
        .collect()
}

fn verify_steps(plan: &RawWritePlan<'_>) -> Vec<(Range, Vec<u8>)> {
    plan.steps
        .iter()
        .filter_map(|step| match step {
            RawWriteStep::VerifyRead { read, expected } => {
                Some((read.physical_range, expected.expose().to_vec()))
            }
            _ => None,
        })
        .collect()
}

fn overlaps(range: Range, offset: u8, end: u8) -> bool {
    u32::from(range.offset) < u32::from(end) && u32::from(offset) < range.end()
}

#[test]
fn plan_129_145_writes_exactly_the_requested_bytes_and_verifies_the_same_range() {
    let payload = b"RAWWRITEPLAN123!";
    let targets = [target(129, 16)];
    let payloads: Vec<&[u8]> = vec![payload];
    let plan = planned(&targets, &payloads).unwrap();

    // Exactly the requested bytes, in one write step, no widening.
    assert_eq!(
        write_steps(&plan),
        vec![(targets[0].range, payload.to_vec())]
    );

    // Exactly one same-range verify read carrying the written bytes.
    assert_eq!(
        verify_steps(&plan),
        vec![(targets[0].range, payload.to_vec())]
    );

    // One selector confirmation READ of bytes 126..128 expecting bank/page 0/0;
    // it is the only step outside the requested range.
    let confirms: Vec<_> = plan
        .steps
        .iter()
        .filter(|step| matches!(step, RawWriteStep::ConfirmSelection { .. }))
        .collect();
    assert_eq!(confirms.len(), 1);
    match confirms[0] {
        RawWriteStep::ConfirmSelection { read, expected } => {
            assert_eq!(read.physical_range, target(126, 2).range);
            assert_eq!(*expected, [0u8, 0]);
        }
        _ => unreachable!(),
    }
    assert_eq!(plan.steps.len(), 3);

    // No byte-222 step, no 122..126 password-area step, no selector write.
    for (range, _) in write_steps(&plan).iter().chain(verify_steps(&plan).iter()) {
        assert!(
            !overlaps(*range, 222, 223),
            "unrequested byte 222: {range:?}"
        );
        assert!(!overlaps(*range, 122, 126), "password area: {range:?}");
        assert!(!overlaps(*range, 126, 128), "selector bytes: {range:?}");
    }
    assert_eq!(
        plan.required_selection,
        Selection {
            address: 0x50,
            bank: 0,
            page: 0
        }
    );

    // Invalidations cover only the requested range; no password pin invalidation.
    assert_eq!(
        plan.invalidations,
        vec![Invalidation::Covered(targets[0].range)]
    );

    // Every unconfirmed step is terminal uncertain: never retried, never continued.
    assert_eq!(plan.unconfirmed_step_retries.len(), plan.steps.len());
    for decision in &plan.unconfirmed_step_retries {
        assert!(matches!(
            decision,
            RetryDecision::Terminal(ActuationOutcome::Uncertain { reason })
                if reason == "step-unconfirmed"
        ));
    }
    assert_eq!(plan.revision, Rev::CMIS_4_0);
}

#[test]
fn single_byte_plan_writes_and_verifies_exactly_one_byte() {
    let targets = [target(129, 1)];
    let payloads: Vec<&[u8]> = vec![&[0xab]];
    let plan = planned(&targets, &payloads).unwrap();
    assert_eq!(write_steps(&plan), vec![(targets[0].range, vec![0xab])]);
    assert_eq!(verify_steps(&plan), vec![(targets[0].range, vec![0xab])]);
    assert_eq!(plan.steps.len(), 3);
}

#[test]
fn multi_target_plan_preserves_caller_order_without_widening_or_extra_bytes() {
    let targets = [target(129, 16), target(222, 1)];
    let payloads: Vec<&[u8]> = vec![b"RAWWRITEPLAN123!", &[0x5a]];
    let plan = planned(&targets, &payloads).unwrap();

    // Requested byte 222 IS written when the caller asks for it; the plan adds
    // no byte the caller did not request, in caller order.
    assert_eq!(
        write_steps(&plan),
        vec![
            (targets[0].range, payloads[0].to_vec()),
            (targets[1].range, payloads[1].to_vec()),
        ]
    );
    assert_eq!(
        verify_steps(&plan),
        vec![
            (targets[0].range, payloads[0].to_vec()),
            (targets[1].range, payloads[1].to_vec()),
        ]
    );
    assert_eq!(plan.steps.len(), 5);
    let total_written: usize = write_steps(&plan).iter().map(|(_, b)| b.len()).sum();
    assert_eq!(total_written, 17);
    assert_eq!(
        plan.invalidations,
        vec![
            Invalidation::Covered(targets[0].range),
            Invalidation::Covered(targets[1].range)
        ]
    );
    assert!(!plan
        .invalidations
        .iter()
        .any(|invalidation| matches!(invalidation, Invalidation::Password)));
}

#[test]
fn inadmissible_requests_refuse_before_any_plan_or_action() {
    // Secret / write-only: both password areas, whole physical spans.
    for offset in [118, 121, 122, 125] {
        assert_eq!(
            refused_reason(&[target(offset, 1)], &[&[0]]),
            RefusalReason::SecretOrWriteOnly,
            "{offset}"
        );
    }
    // Control/command: selector bytes are never raw-written.
    for offset in [126, 127] {
        assert_eq!(
            refused_reason(&[target(offset, 1)], &[&[0]]),
            RefusalReason::ControlOrCommand,
            "{offset}"
        );
    }
    assert_eq!(
        refused_reason(&[target(26, 1)], &[&[0]]),
        RefusalReason::SecretOrWriteOnly
    );
    // Unknown coverage: reserved / vendor-specific semantics never pass.
    for offset in [27, 221] {
        assert_eq!(
            refused_reason(&[target(offset, 1)], &[&[0]]),
            RefusalReason::UnknownCoverage,
            "{offset}"
        );
    }
    // Wrong revision: only CMIS 4.0 is admitted.
    let regions = [RawRegion {
        target: target(129, 1),
        write_bytes: Some(WriteBytes::new(&[0])),
    }];
    assert_eq!(
        preflight(&RawRequest {
            revision: Rev::CMIS_5_3,
            purpose: Purpose::Write,
            regions: &regions,
        })
        .unwrap_err()
        .reason(),
        RefusalReason::Unsupported(UnsupportedCell::Revision)
    );
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    assert_eq!(
        preflight(&RawRequest {
            revision: rev,
            purpose: Purpose::Write,
            regions: &regions,
        })
        .unwrap_err()
        .reason(),
        RefusalReason::Unsupported(UnsupportedCell::Family)
    );
    // Bank != 0 and page != 00h.
    let mut banked = target(128, 1);
    banked.range.bank = 1;
    assert_eq!(
        refused_reason(&[banked], &[&[0]]),
        RefusalReason::Unsupported(UnsupportedCell::Bank)
    );
    let mut paged = target(128, 1);
    paged.range.page = 1;
    assert_eq!(
        refused_reason(&[paged], &[&[0]]),
        RefusalReason::Unsupported(UnsupportedCell::Page)
    );
    // Missing or mismatched payload count.
    let regions = [RawRegion {
        target: target(129, 1),
        write_bytes: None,
    }];
    assert_eq!(
        preflight(&RawRequest {
            revision: Rev::CMIS_4_0,
            purpose: Purpose::Write,
            regions: &regions,
        })
        .unwrap_err()
        .reason(),
        RefusalReason::MissingPayload
    );
    assert_eq!(
        refused_reason(&[target(129, 16)], &[&[0; 15]]),
        RefusalReason::PayloadLength
    );
}

#[test]
fn planning_refusals_produce_no_steps_or_partial_plan() {
    // A read description never yields a write plan.
    let targets = [target(129, 1)];
    let read = description(&targets, &[&[0]], Purpose::Read).unwrap();
    assert_eq!(
        plan_raw_write(&read, &[WriteBytes::new(&[0])]).unwrap_err(),
        RawWritePlanRefusal::PurposeNotWrite
    );
    // Payload count must match the description's target count.
    let write = description(&targets, &[&[0]], Purpose::Write).unwrap();
    assert_eq!(
        plan_raw_write(&write, &[]).unwrap_err(),
        RawWritePlanRefusal::PayloadCount
    );
    // A payload whose length disagrees with its target range refuses.
    assert_eq!(
        plan_raw_write(&write, &[WriteBytes::new(&[0, 1])]).unwrap_err(),
        RawWritePlanRefusal::PayloadLength
    );
    // Overlapping targets demand contradictory verification bytes.
    let duplicate = description(
        &[target(129, 16), target(129, 16)],
        &[b"RAWWRITEPLAN123!", b"OTHER VALUE TWOX"],
        Purpose::Write,
    )
    .unwrap();
    assert_eq!(
        plan_raw_write(
            &duplicate,
            &[
                WriteBytes::new(b"RAWWRITEPLAN123!"),
                WriteBytes::new(b"OTHER VALUE TWOX")
            ]
        )
        .unwrap_err(),
        RawWritePlanRefusal::OverlappingTargets
    );
    assert_eq!(
        description(
            &[target(129, 16), target(140, 2)],
            &[b"RAWWRITEPLAN123!", &[0, 1]],
            Purpose::Write
        )
        .unwrap()
        .targets()
        .len(),
        2
    );
}

#[test]
fn verdict_classifies_exact_match_as_stored() {
    let payload = b"RAWWRITEPLAN123!";
    let plan = planned(&[target(129, 16)], &[payload]).unwrap();
    assert_eq!(
        plan.verdict(0, RawReadback::Bytes(payload)),
        Some(RawWriteVerdict::Stored)
    );
    assert_eq!(
        classify_readback(target(129, 16).range, payload, RawReadback::Bytes(payload)),
        RawWriteVerdict::Stored
    );
}

#[test]
fn verdict_names_every_differing_absolute_offset() {
    let payload = b"RAWWRITEPLAN123!";
    let mut readback = payload.to_vec();
    readback[3] = !readback[3];
    assert_eq!(
        classify_readback(
            target(129, 16).range,
            payload,
            RawReadback::Bytes(&readback)
        ),
        RawWriteVerdict::Mismatch {
            differing_offsets: vec![132]
        }
    );
    readback[15] = !readback[15];
    assert_eq!(
        classify_readback(
            target(129, 16).range,
            payload,
            RawReadback::Bytes(&readback)
        ),
        RawWriteVerdict::Mismatch {
            differing_offsets: vec![132, 144]
        }
    );
    let plan = planned(&[target(129, 16)], &[payload]).unwrap();
    assert_eq!(
        plan.verdict(0, RawReadback::Bytes(&readback)),
        Some(RawWriteVerdict::Mismatch {
            differing_offsets: vec![132, 144]
        })
    );
    // A target index with no verify step answers None, never a guess.
    assert_eq!(plan.verdict(1, RawReadback::Bytes(payload)), None);
}

#[test]
fn verdict_reports_short_readback_and_read_failure_without_inventing_bytes() {
    let payload = b"RAWWRITEPLAN123!";
    let plan = planned(&[target(129, 16)], &[payload]).unwrap();
    // Short readback: the actual count is carried, nothing is padded.
    assert_eq!(
        plan.verdict(0, RawReadback::Bytes(&payload[..10])),
        Some(RawWriteVerdict::ShortRead { actual: 10 })
    );
    // Read failure: the actual count stays unknown.
    assert_eq!(
        plan.verdict(0, RawReadback::Failed),
        Some(RawWriteVerdict::ReadFailure)
    );
    assert_eq!(
        classify_readback(target(129, 16).range, payload, RawReadback::Failed),
        RawWriteVerdict::ReadFailure
    );
}

#[test]
fn plan_diagnostics_never_expose_payload_bytes() {
    let sentinel = b"SENTINEL_SECRET!";
    let plan = planned(&[target(129, 16)], &[sentinel]).unwrap();
    let diagnostics = format!("{plan:?}");
    for representation in [
        String::from_utf8_lossy(sentinel).into_owned(),
        format!("{sentinel:?}"),
        sentinel.iter().map(|b| format!("{b:02x}")).collect(),
    ] {
        assert!(
            !diagnostics.contains(&representation),
            "payload leaked: {diagnostics}"
        );
    }
}
