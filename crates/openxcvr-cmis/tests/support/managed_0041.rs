//! CMIS 5.3 0041h uses the existing offline owner and reducer.
use super::*;

fn features_facts() -> CdbFacts {
    let mut f = facts();
    // No qualification for bytes 154..245: the old footprint must not be required.
    f.qualification.reads[2].0.len = 20;
    f.qualification.reads[2].1.truncate(20);
    f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, 0, 0]);
    f
}
fn owner(f: CdbFacts) -> Owner {
    let (plan, state) =
        admit_firmware_management_features(request(0x41), &f, bounds(), MonoTime(0)).unwrap();
    assert_eq!(plan.opcode(), 0x0041);
    assert_eq!(plan.audit_description().opcode, 0x0041);
    Owner {
        plan,
        state,
        live: live(),
        now: 0,
        actions: vec![],
    }
}
fn reply_bytes(a: &CdbAction, length: u8, checksum: u8, body: &[u8]) -> Vec<u8> {
    match a {
        CdbAction::Read(r) => match r.purpose() {
            CdbReadPurpose::Arm => vec![0xc7],
            CdbReadPurpose::CompletionFlag => vec![0xff],
            CdbReadPurpose::ReplyHeader => {
                let start = usize::from(r.range().offset - 134);
                [length, checksum][start..start + usize::from(r.range().len)].to_vec()
            }
            CdbReadPurpose::ReplyBody => {
                assert!(r.range().end() <= 154);
                let start = usize::from(r.range().offset - 136);
                body[start..start + usize::from(r.range().len)].to_vec()
            }
            _ => vec![1],
        },
        _ => vec![],
    }
}
fn complete(o: &mut Owner, len: u8, checksum: u8, body: &[u8]) {
    while o.state.terminal().is_none() {
        let a = o.next();
        let b = reply_bytes(&a, len, checksum, body);
        o.answer(a, &b);
    }
}

#[test]
fn admission_is_closed_and_independent_of_0100() {
    let f = features_facts();
    assert!(admit_firmware_management_features(request(0x41), &f, bounds(), MonoTime(0)).is_ok());
    for opcode in 0..=u16::MAX {
        if opcode != 0x41 {
            assert_eq!(
                admit_firmware_management_features(request(opcode), &f, bounds(), MonoTime(0)),
                Err(CdbRefusal::RequestNotAdmitted)
            );
        }
    }
    for n in 0..11 {
        let mut r = request(0x41);
        match n {
            0 => r.revision = Rev::CMIS_5_2,
            1 => r.revision = Rev::CMIS_4_0,
            2 => r.address = 0x51,
            3 => r.instance = 0,
            4 => r.instance = 2,
            5 => r.bank = 1,
            6 => r.lpl = &[0],
            7 => r.epl_len = 1,
            8 => r.credential_supplied = true,
            9 => {
                r.revision = Rev {
                    family: openxcvr_core::rev::Family::Cmis,
                    major: 6,
                    minor: 0,
                }
            }
            _ => {
                r.revision = Rev {
                    family: openxcvr_core::rev::Family::Sff8636,
                    major: 5,
                    minor: 3,
                }
            }
        }
        assert_eq!(
            admit_firmware_management_features(r, &f, bounds(), MonoTime(0)),
            Err(CdbRefusal::RequestNotAdmitted)
        );
    }
    for n in 0..10 {
        let mut f = features_facts();
        match n {
            0 => f.qualification.reads[0].1[0].uncovered_mask = 1,
            1 => f.qualification.reads[1].1[0].known_effects.clear(),
            2 => f.qualification.reads[2].0.len = 19,
            3 => f.qualification.stop_terminated_writes = false,
            4 => f.qualification.durable_audit_ready = false,
            5 => f.qualification.exclusion_enforceable = false,
            6 => f.qualification.selector_authority_ready = false,
            7 => f.no_outstanding_cdb = false,
            8 => f.revision_byte = 0x52,
            _ => f.qualification.writes.clear(),
        }
        assert!(
            admit_firmware_management_features(request(0x41), &f, bounds(), MonoTime(0)).is_err()
        );
    }
    assert!(admit_get_firmware_info(request(0x41), &facts(), bounds(), MonoTime(0)).is_err());
    assert!(admit_get_firmware_info(request(0x100), &f, bounds(), MonoTime(0)).is_err());
}

#[test]
fn exact_staging_bounds_projection_and_checksum_exclude_byte_154() {
    // Bytes 136..153 plus poisoned undefined byte 154. The latter is never read.
    let mut memory = [
        0xa6, 0x8f, 0x33, 0xff, 0xff, 0x10, 0x11, 1, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde,
        0xf0, 0x24, 0x68, 0,
    ];
    for method in [0, 0x80] {
        for limit in 1..=8 {
            for poison in [0, 0xff] {
                memory[18] = poison;
                let mut f = features_facts();
                f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, method, 0]);
                f.qualification.backend_read_limit = limit;
                f.qualification.backend_write_limit = limit;
                let mut o = owner(f);
                let checksum = !memory[..18].iter().fold(0u8, |s, b| s.wrapping_add(*b));
                complete(&mut o, 18, checksum, &memory);
                let outcome = o.state.terminal().unwrap();
                let CdbOutcome::CompletedFirmwareManagementFeatures { reply, evidence } = outcome
                else {
                    panic!("{outcome:?}")
                };
                assert_eq!(reply.bytes, memory[..18]);
                let typed = outcome.firmware_management_features().unwrap();
                assert_eq!(typed.reserved_136, 0xa6);
                assert_eq!(typed.duration_multiplier, 10);
                assert_eq!(typed.max_duration_start, 0x1234);
                assert_eq!(typed.max_duration_copy, 0x2468);
                assert!(outcome.firmware_info().unwrap().is_none());
                assert!(outcome.firmware_images().is_none());
                assert_eq!(outcome.audit_reason(), CdbTerminalReason::Completed);
                assert_eq!(evidence.checksum_valid, Some(true));
                assert!(!evidence.mailbox_unresolved);
                assert_eq!(evidence.consumed[0].1.bytes(), Some(&[0xc7][..]));
                assert_eq!(evidence.consumed[1].1.bytes(), Some(&[0xff][..]));
                let writes: Vec<_> = o
                    .actions
                    .iter()
                    .filter_map(|a| match a {
                        CdbAction::Write(w) => Some(w),
                        _ => None,
                    })
                    .collect();
                let parameters: Vec<u8> = writes
                    .iter()
                    .filter(|w| w.purpose() == CdbWritePurpose::Parameters)
                    .flat_map(|w| w.bytes().iter().copied())
                    .collect();
                assert_eq!(parameters, [0, 0, 0, 0xbe]);
                assert_eq!(writes[writes.len() - 2].range(), range(128, 1, 0x9f));
                assert_eq!(writes[writes.len() - 2].bytes(), &[0]);
                assert_eq!(writes.last().unwrap().range(), range(129, 1, 0x9f));
                assert_eq!(writes.last().unwrap().bytes(), &[0x41]);
                assert_eq!(
                    writes
                        .iter()
                        .filter(|w| w.purpose() == CdbWritePurpose::Trigger)
                        .count(),
                    1
                );
                let upper_reads: Vec<_> = o
                    .actions
                    .iter()
                    .filter_map(|a| match a {
                        CdbAction::Read(r) if r.range().page == 0x9f => Some(r.range()),
                        _ => None,
                    })
                    .collect();
                let offsets: Vec<u32> = upper_reads
                    .iter()
                    .flat_map(|r| u32::from(r.offset)..r.end())
                    .collect();
                assert_eq!(offsets, (134..154).collect::<Vec<_>>());
                assert!(o.event(CdbEvent::Drive).is_empty());
            }
        }
    }
}

#[test]
fn module_failure_statuses_are_not_completed_features() {
    for status in [0x40, 0x42, 0x45] {
        let mut o = owner(features_facts());
        o.through_status(status);
        let outcome = o.state.terminal().unwrap();
        assert!(
            matches!(outcome, CdbOutcome::Failed { raw_status, result, evidence } if *raw_status == status && *result == status & 63 && !evidence.mailbox_unresolved)
        );
        assert!(outcome.firmware_management_features().is_none());
    }
    let mut o = Owner::new(facts());
    let body = [0; 110];
    o.through_status(1);
    // The existing 0100h owner's qualified footprint is intentionally unchanged.
    let a = o.next();
    o.answer(a, &[110, 255]);
    while o.state.terminal().is_none() {
        let a = o.next();
        let CdbAction::Read(r) = &a else { panic!() };
        let len = usize::from(r.range().len);
        o.answer(a, &body[..len]);
    }
    assert!(o
        .state
        .terminal()
        .unwrap()
        .firmware_management_features()
        .is_none());
}

#[test]
fn invalid_lengths_checksum_and_short_body_are_not_completed() {
    for (len, reason) in [
        (0, CdbInvalidReply::Truncated),
        (17, CdbInvalidReply::Truncated),
        (19, CdbInvalidReply::UnexpectedLength),
        (120, CdbInvalidReply::UnexpectedLength),
        (121, CdbInvalidReply::ReservedLength),
        (239, CdbInvalidReply::ReservedLength),
        (240, CdbInvalidReply::EplUnsupported),
        (255, CdbInvalidReply::EplUnsupported),
        (18, CdbInvalidReply::Checksum),
    ] {
        let mut o = owner(features_facts());
        complete(&mut o, len, 0, &[0; 18]);
        assert!(
            matches!(o.state.terminal(), Some(CdbOutcome::InvalidReply { reason: r, .. }) if *r == reason)
        );
        assert!(o
            .state
            .terminal()
            .unwrap()
            .firmware_management_features()
            .is_none());
    }
    let mut o = owner(features_facts());
    o.through_status(1);
    let a = o.next();
    o.answer(a, &[18, 255]);
    let a = o.next();
    o.answer(a, &[0]);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::InvalidReply {
            reason: CdbInvalidReply::ByteCount,
            ..
        })
    ));
}

#[test]
fn interruptions_at_every_transfer_retain_outstanding_results_and_do_not_replay() {
    for cause in 0..7 {
        for cut in 0..13 {
            let mut o = owner(features_facts());
            for _ in 0..cut {
                let a = o.next();
                let b = reply_bytes(&a, 18, 255, &[0; 18]);
                o.answer(a, &b);
                if o.state.terminal().is_some() {
                    break;
                }
            }
            if o.state.terminal().is_some() {
                continue;
            }
            let a = o.next();
            o.dispatch(&a);
            match cause {
                0 => {
                    o.event(CdbEvent::Cancel);
                }
                1 => {
                    o.event(CdbEvent::BoundaryLost);
                }
                2 => {
                    o.live.audit_ready = false;
                    o.event(CdbEvent::Drive);
                }
                3 => {
                    o.now = 1000;
                    o.event(CdbEvent::Drive);
                }
                4 => {
                    o.now = 10_000; // admission qualification expires
                    o.event(CdbEvent::Drive);
                }
                5 => {
                    o.live.custody = false;
                    o.event(CdbEvent::Drive);
                }
                _ => {
                    o.live.identity.generation = ModuleGeneration(2);
                    o.event(CdbEvent::Drive);
                }
            }
            let terminal = o.state.terminal().cloned();
            assert!(terminal.is_some());
            assert!(o.event(CdbEvent::Drive).is_empty());
            o.now += 1;
            match a {
                CdbAction::Read(r) => {
                    o.event(CdbEvent::ReadResult {
                        id: r.id(),
                        observation: observation(
                            r.range(),
                            vec![0; usize::from(r.range().len)],
                            o.now,
                        ),
                        exchange: CdbReadExchange::Success,
                    });
                }
                CdbAction::Write(w) => {
                    o.event(CdbEvent::WriteResult {
                        id: w.id(),
                        outcome: ActuationOutcome::Confirmed,
                        completed_at: MonoTime(o.now),
                    });
                }
                _ => panic!(),
            }
            assert_eq!(o.state.terminal(), terminal.as_ref());
            assert!(o.state.outstanding().is_none());
            assert!(o.event(CdbEvent::Drive).is_empty());
        }
    }
}

#[test]
fn stale_flags_busy_recheck_and_selection_loss_cannot_authorize_completion() {
    let mut o = owner(features_facts());
    o.through_trigger(); // arming read returned 0xc7, including a stale complete bit
    let a = o.next();
    o.answer(a, &[0]);
    assert!(o.state.evidence().fresh_completion.is_none());
    assert!(o.state.terminal().is_none());
    let CdbAction::Read(r) = o.next() else {
        panic!()
    };
    assert_eq!(r.purpose(), CdbReadPurpose::CompletionFlag);

    let mut o = owner(features_facts());
    loop {
        let a = o.next();
        if matches!(&a, CdbAction::Read(r) if r.purpose() == CdbReadPurpose::RecheckIdle) {
            o.answer(a, &[0x80]);
            break;
        }
        o.answer(a, &[1]);
    }
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::StoppedBeforeTrigger {
            reason: CdbStopReason::BusyBeforeTrigger,
            ..
        })
    ));
    assert!(!o.state.evidence().trigger_may_have_executed);

    let mut o = owner(features_facts());
    o.through_status(1);
    let old = o.state.evidence().selections[0].core.clone();
    let actions = o.event(CdbEvent::Drive);
    let CdbAction::RequireSelection { id, .. } = actions[0] else {
        panic!()
    };
    o.live.selection = Some(CdbSelectionEvidence {
        for_step: id,
        core: old,
    });
    o.event(CdbEvent::SelectionConfirmed { id });
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::AmbiguousAfterTrigger {
            reason: CdbStopReason::SelectionLost,
            ..
        })
    ));
    assert!(o.event(CdbEvent::Drive).is_empty());
}

#[test]
fn trigger_overflow_and_mismatched_plan_cannot_dispatch() {
    let mut f = features_facts();
    f.qualification.valid_until = MonoTime(u64::MAX);
    let mut b = bounds();
    b.operation_deadline = MonoTime(u64::MAX);
    let (plan, state) =
        admit_firmware_management_features(request(0x41), &f, b, MonoTime(0)).unwrap();
    let mut o = Owner {
        plan,
        state,
        live: live(),
        now: 0,
        actions: vec![],
    };
    let (other, _) = admitted();
    assert_eq!(
        cdb_step(&other, &o.state, CdbEvent::Drive, &o.live, MonoTime(0)).unwrap_err(),
        CdbInputError::PlanMismatch
    );
    loop {
        let a = o.next();
        if let CdbAction::Write(w) = &a {
            if w.purpose() == CdbWritePurpose::Trigger {
                let now = MonoTime(u64::MAX - 10);
                assert_eq!(
                    cdb_step(
                        &o.plan,
                        &o.state,
                        CdbEvent::Dispatched {
                            id: w.id(),
                            at: now
                        },
                        &o.live,
                        now
                    )
                    .unwrap_err(),
                    CdbInputError::TimeOverflow
                );
                assert!(!o.state.dispatch_authorized(w.id()));
                assert!(!o.state.evidence().trigger_may_have_executed);
                break;
            }
        }
        o.answer(a, &[1]);
    }
}

#[test]
fn all_target_octets_and_epl_lengths_are_closed_before_plan() {
    let f = features_facts();
    for field in 0..3 {
        for value in 0..=255 {
            let allowed = match field {
                0 => 0x50,
                1 => 1,
                _ => 0,
            };
            if value == allowed {
                continue;
            }
            let mut r = request(0x41);
            match field {
                0 => r.address = value,
                1 => r.instance = value,
                _ => r.bank = value,
            }
            assert_eq!(
                admit_firmware_management_features(r, &f, bounds(), MonoTime(0)),
                Err(CdbRefusal::RequestNotAdmitted)
            );
        }
    }
    for len in 1..=u16::MAX {
        let mut r = request(0x41);
        r.epl_len = len;
        assert_eq!(
            admit_firmware_management_features(r, &f, bounds(), MonoTime(0)),
            Err(CdbRefusal::RequestNotAdmitted)
        );
    }
    for revision in [Rev::CMIS_3_0, Rev::CMIS_5_0, Rev::CMIS_5_1] {
        let mut r = request(0x41);
        r.revision = revision;
        assert_eq!(
            admit_firmware_management_features(r, &f, bounds(), MonoTime(0)),
            Err(CdbRefusal::RequestNotAdmitted)
        );
    }
}
