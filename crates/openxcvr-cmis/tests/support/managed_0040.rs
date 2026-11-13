//! CMIS 5.3 Module Features; shares the existing offline selector/reducer owner.
use super::*;

#[test]
fn baseline_0041_action_sequence() {
    for method in [0, 0x80] {
        for limit in 1..=8 {
            let mut f = facts();
            f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, method, 0]);
            f.qualification.reads[2].0.len = 20;
            f.qualification.reads[2].1.truncate(20);
            f.qualification.backend_read_limit = limit;
            f.qualification.backend_write_limit = limit;
            let (plan, state) =
                admit_firmware_management_features(request(0x41), &f, bounds(), MonoTime(0))
                    .unwrap();
            let mut o = Owner {
                plan,
                state,
                live: live(),
                now: 0,
                actions: vec![],
            };
            while o.state.terminal().is_none() {
                let a = o.next();
                let b = match &a {
                    CdbAction::Read(r) => match r.purpose() {
                        CdbReadPurpose::Arm => vec![0xc7],
                        CdbReadPurpose::CompletionFlag => vec![0xff],
                        CdbReadPurpose::ReplyHeader => {
                            let start = usize::from(r.range().offset - 134);
                            [18, 255][start..start + usize::from(r.range().len)].to_vec()
                        }
                        CdbReadPurpose::ReplyBody => vec![0; usize::from(r.range().len)],
                        _ => vec![1],
                    },
                    _ => vec![],
                };
                o.answer(a, &b);
            }
            assert!(matches!(
                o.state.terminal(),
                Some(CdbOutcome::CompletedFirmwareManagementFeatures { .. })
            ));
            println!("TRACE-0041-{method}-{limit}: {:?}", o.actions);
        }
    }
}

use openxcvr_cmis::adapter::cdb::{CdbCommand, CdbOp, CmisCdbModuleFeatures};
use openxcvr_core::{error::ModuleError, transport::Transport};

#[derive(Default)]
struct TransportSpy {
    reads: usize,
    writes: usize,
    probes: usize,
}
impl TransportSpy {
    fn traffic(&self) -> usize {
        self.reads + self.writes + self.probes
    }
}
impl Transport for TransportSpy {
    fn read(&mut self, _: u8, _: u8, _: u8, _: u8, b: &mut [u8]) -> Result<(), ModuleError> {
        self.reads += 1;
        b.fill(1);
        Ok(())
    }
    fn write(&mut self, _: u8, _: u8, _: u8, _: u8, _: &[u8]) -> Result<(), ModuleError> {
        self.writes += 1;
        Ok(())
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        self.probes += 1;
        Ok(())
    }
}
// A consumer must classify BEFORE any capability acquisition or selector work.
// The library classification itself has no transport parameter or I/O authority.
fn classified_start(t: &mut TransportSpy, rev: Rev, op: u16) -> Result<(), CdbQueryRefusal> {
    let query = classify_managed_cdb_query(rev, op)?;
    assert_eq!(query.opcode(), op);
    t.probe().unwrap();
    t.read(0x50, 1, 0, 163, &mut [0; 4]).unwrap();
    let mut o = owner(module_facts());
    let a = o.next();
    let CdbAction::Read(r) = a else { panic!() };
    t.read(
        r.range().address,
        r.range().page,
        r.range().bank,
        r.range().offset,
        &mut vec![0; usize::from(r.range().len)],
    )
    .unwrap();
    Ok(())
}
fn module_facts() -> CdbFacts {
    let mut f = facts();
    f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, 0, 0]);
    // Full possible LPL plus the reply header, not a fabricated backend grant.
    f.qualification.reads[2].0.len = 122;
    let byte = f.qualification.reads[2].1[0].clone();
    f.qualification.reads[2].1.resize(122, byte);
    f
}
fn owner(f: CdbFacts) -> Owner {
    let (plan, state) = admit_module_features(request(0x40), &f, bounds(), MonoTime(0)).unwrap();
    assert_eq!(plan.opcode(), 0x0040);
    assert_eq!(plan.audit_description().opcode, 0x0040);
    Owner {
        plan,
        state,
        live: live(),
        now: 0,
        actions: vec![],
    }
}
fn checksum(b: &[u8]) -> u8 {
    !b.iter().fold(0u8, |s, b| s.wrapping_add(*b))
}
fn bytes_for(a: &CdbAction, len: u8, check: u8, body: &[u8]) -> Vec<u8> {
    match a {
        CdbAction::Read(r) => match r.purpose() {
            CdbReadPurpose::Arm => vec![0xc7],
            CdbReadPurpose::CompletionFlag => vec![0xff],
            CdbReadPurpose::ReplyHeader => {
                let start = usize::from(r.range().offset - 134);
                [len, check][start..start + usize::from(r.range().len)].to_vec()
            }
            CdbReadPurpose::ReplyBody => {
                let start = usize::from(r.range().offset - 136);
                assert!(start + usize::from(r.range().len) <= usize::from(len));
                body[start..start + usize::from(r.range().len)].to_vec()
            }
            _ => vec![1],
        },
        _ => vec![],
    }
}
fn complete(o: &mut Owner, len: u8, check: u8, body: &[u8]) {
    while o.state.terminal().is_none() {
        let a = o.next();
        let b = bytes_for(&a, len, check, body);
        o.answer(a, &b);
    }
}

#[test]
fn classification_exhausts_u16_with_zero_total_transport_traffic() {
    let f = module_facts();
    for op in 0..=u16::MAX {
        let mut spy = TransportSpy::default();
        if op == 0x40 {
            assert_eq!(
                classify_managed_cdb_query(Rev::CMIS_5_3, op),
                Ok(ManagedCdbQuery::ModuleFeatures)
            );
            classified_start(&mut spy, Rev::CMIS_5_3, op).unwrap();
            assert_eq!(spy.traffic(), 3); // positive control: not a disconnected spy
        } else {
            let reason = if (0x100..=0x11f).contains(&op) {
                CdbQueryRefusal::FirmwareManagement
            } else {
                CdbQueryRefusal::NotAnAdmittedQuery
            };
            assert_eq!(
                classified_start(&mut spy, Rev::CMIS_5_3, op),
                Err(reason),
                "{op:04x}"
            );
            assert_eq!(
                admit_module_features(request(op), &f, bounds(), MonoTime(0)),
                Err(CdbRefusal::RequestNotAdmitted)
            );
            assert_eq!(spy.traffic(), 0, "{op:04x}");
        }
    }
    // Raw refusal is independent of the two existing information admissions.
    assert!(
        admit_firmware_management_features(request(0x41), &facts(), bounds(), MonoTime(0)).is_ok()
    );
    assert!(admit_get_firmware_info(request(0x100), &facts(), bounds(), MonoTime(0)).is_ok());
}

#[test]
fn unsupported_revisions_refuse_before_any_traffic() {
    for family in [
        openxcvr_core::rev::Family::Cmis,
        openxcvr_core::rev::Family::Sff8636,
        openxcvr_core::rev::Family::Sff8472,
        openxcvr_core::rev::Family::CCmis,
        openxcvr_core::rev::Family::Elsfp,
    ] {
        for (major, minor) in [
            (3, 0),
            (4, 0),
            (5, 0),
            (5, 1),
            (5, 2),
            (5, 3),
            (5, 4),
            (6, 0),
            (255, 255),
        ] {
            let revision = Rev {
                family,
                major,
                minor,
            };
            if revision == Rev::CMIS_5_3 {
                continue;
            }
            for op in [0x40, 0x41, 0x100, 0x380, 0xffff] {
                let mut spy = TransportSpy::default();
                assert_eq!(
                    classified_start(&mut spy, revision, op),
                    Err(CdbQueryRefusal::NotAdmittedAtRevision)
                );
                assert_eq!(spy.traffic(), 0);
                let mut r = request(op);
                r.revision = revision;
                assert_eq!(
                    admit_module_features(r, &module_facts(), bounds(), MonoTime(0)),
                    Err(CdbRefusal::RequestNotAdmitted)
                );
            }
        }
    }
}

#[test]
fn admitted_set_is_explicit_and_matches_registry_and_codec_identity() {
    let admitted: Vec<_> = CdbOp::ALL
        .into_iter()
        .filter(|op| classify_managed_cdb_query(Rev::CMIS_5_3, op.opcode()).is_ok())
        .collect();
    assert_eq!(admitted, vec![CdbOp::ModuleFeatures]);
    assert_eq!(
        ManagedCdbQuery::ModuleFeatures.opcode(),
        CmisCdbModuleFeatures::CMD_ID
    );
    assert_eq!(CdbOp::ModuleFeatures.name(), "CmisCdbModuleFeatures");
    let schema: serde_norway::Value = serde_norway::from_str(include_str!(
        "../../../../schemas/cmis/cdb-commands/0040-module-features.yaml"
    ))
    .unwrap();
    assert_eq!(schema["id"].as_str(), Some(CdbOp::ModuleFeatures.name()));
    assert_eq!(
        schema["cmd_id"].as_u64(),
        Some(u64::from(ManagedCdbQuery::ModuleFeatures.opcode()))
    );
    assert_eq!(
        schema["reply"]["lpl"]["length"].as_u64(),
        Some(CmisCdbModuleFeatures::MIN_REPLY_LPL_LEN as u64)
    );
    assert_eq!(schema["provenance"]["table"].as_str(), Some("Table 9-8"));
    // Empty side_effects is deliberately NOT raw admission authority.
    let loopback: serde_norway::Value = serde_norway::from_str(include_str!(
        "../../../../schemas/cmis/cdb-commands/0380-loopbacks.yaml"
    ))
    .unwrap();
    assert!(loopback["behavior"]["side_effects"]
        .as_sequence()
        .unwrap()
        .is_empty());
    assert_eq!(
        classify_managed_cdb_query(Rev::CMIS_5_3, CdbOp::Loopbacks.opcode()),
        Err(CdbQueryRefusal::NotAnAdmittedQuery)
    );
}

#[test]
fn received_lengths_full_bytes_known_prefix_and_exact_transfer_order() {
    for len in [36u8, 37, 120] {
        let body: Vec<u8> = (0..len)
            .map(|i| i.wrapping_mul(13).wrapping_add(0xa5))
            .collect();
        for method in [0, 0x80] {
            for limit in 1..=8 {
                for _ in 0..2 {
                    // a repeated opcode is a new isolated trigger, not a cache hit
                    let mut f = module_facts();
                    f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, method, 0]);
                    f.qualification.backend_read_limit = limit;
                    f.qualification.backend_write_limit = limit;
                    let mut o = owner(f);
                    complete(&mut o, len, checksum(&body), &body);
                    let outcome = o.state.terminal().unwrap();
                    let CdbOutcome::CompletedModuleFeatures {
                        opcode,
                        actual_reply_length,
                        bytes,
                        reply,
                        evidence,
                    } = outcome
                    else {
                        panic!("{outcome:?}")
                    };
                    assert_eq!(*opcode, 0x0040);
                    assert_eq!(*actual_reply_length, len);
                    assert_eq!(bytes, &body);
                    assert_eq!(
                        reply,
                        &CmisCdbModuleFeatures::decode_reply(1, &body[..36]).unwrap()
                    );
                    assert_eq!(reply.cdb_flags, body[..1]);
                    assert_eq!(reply.reserved_137, body[1..2]); // nonzero reserved bytes retained, not rejected
                    assert_eq!(reply.supported_cmds_0000_00ff, body[2..34]);
                    assert_eq!(
                        reply.max_completion_time,
                        u16::from_be_bytes([body[34], body[35]])
                    );
                    assert_eq!(evidence.actual_reply_length, Some(len));
                    assert_eq!(evidence.checksum_valid, Some(true));
                    assert!(!evidence.mailbox_unresolved);
                    assert_eq!(outcome.audit_reason(), CdbTerminalReason::Completed);
                    assert!(outcome.firmware_info().unwrap().is_none());
                    assert!(outcome.firmware_images().is_none());
                    assert!(outcome.firmware_management_features().is_none());
                    let writes: Vec<_> = o
                        .actions
                        .iter()
                        .filter_map(|a| {
                            if let CdbAction::Write(w) = a {
                                Some(w)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(
                        writes
                            .iter()
                            .filter(|w| w.purpose() == CdbWritePurpose::Parameters)
                            .flat_map(|w| w.bytes().iter().copied())
                            .collect::<Vec<_>>(),
                        [0, 0, 0, 0xbf]
                    );
                    assert_eq!(writes[writes.len() - 2].range(), range(128, 1, 0x9f));
                    assert_eq!(writes[writes.len() - 2].bytes(), &[0]);
                    assert_eq!(writes.last().unwrap().range(), range(129, 1, 0x9f));
                    assert_eq!(writes.last().unwrap().bytes(), &[0x40]);
                    assert_eq!(
                        writes
                            .iter()
                            .filter(|w| w.purpose() == CdbWritePurpose::Trigger)
                            .count(),
                        1
                    );
                    let offsets: Vec<_> = o
                        .actions
                        .iter()
                        .filter_map(|a| {
                            if let CdbAction::Read(r) = a {
                                (r.range().page == 0x9f).then_some(r.range())
                            } else {
                                None
                            }
                        })
                        .flat_map(|r| u32::from(r.offset)..r.end())
                        .collect();
                    assert_eq!(offsets, (134..136 + u32::from(len)).collect::<Vec<_>>());
                    assert!(o.event(CdbEvent::Drive).is_empty());
                }
            }
        }
    }
}

#[test]
fn length_and_received_checksum_failures_are_typed() {
    for len in 0..=u8::MAX {
        if (36..=120).contains(&len) {
            continue;
        }
        let mut o = owner(module_facts());
        complete(&mut o, len, 0xff, &[]);
        let expected = match len {
            0..=35 => CdbInvalidReply::Truncated,
            121..=239 => CdbInvalidReply::ReservedLength,
            _ => CdbInvalidReply::EplUnsupported,
        };
        assert!(
            matches!(o.state.terminal(), Some(CdbOutcome::InvalidReply {reason, evidence}) if *reason == expected && evidence.actual_reply_length == Some(len))
        );
        assert!(!o
            .actions
            .iter()
            .any(|a| matches!(a, CdbAction::Read(r) if r.purpose() == CdbReadPurpose::ReplyBody)));
    }
    for len in [36u8, 37, 120] {
        let body = vec![1; usize::from(len)];
        for pos in 0..usize::from(len) {
            let mut corrupt = body.clone();
            corrupt[pos] ^= 0x80;
            let mut o = owner(module_facts());
            complete(&mut o, len, checksum(&body), &corrupt);
            assert!(
                matches!(o.state.terminal(), Some(CdbOutcome::InvalidReply { reason: CdbInvalidReply::Checksum, evidence }) if evidence.checksum_valid == Some(false))
            );
        }
    }
    let mut o = owner(module_facts());
    o.through_status(1);
    let a = o.next();
    o.answer(a, &[36, 255]);
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
fn strict_request_and_full_reply_qualification_are_required() {
    for n in 0..6 {
        let mut r = request(0x40);
        match n {
            0 => r.address = 0x51,
            1 => r.instance = 2,
            2 => r.bank = 1,
            3 => r.lpl = &[0],
            4 => r.epl_len = 1,
            _ => r.credential_supplied = true,
        }
        assert_eq!(
            admit_module_features(r, &module_facts(), bounds(), MonoTime(0)),
            Err(CdbRefusal::RequestNotAdmitted)
        );
    }
    for n in 0..8 {
        let mut f = module_facts();
        match n {
            0 => {
                f.qualification.reads[2].0.len = 121;
                f.qualification.reads[2].1.truncate(121);
            }
            1 => f.qualification.reads[2].1[121].uncovered_mask = 1,
            2 => f.qualification.stop_terminated_writes = false,
            3 => f.qualification.exclusion_enforceable = false,
            4 => f.qualification.durable_audit_ready = false,
            5 => f.qualification.selector_authority_ready = false,
            6 => f.qualification.backend_read_limit = 0,
            _ => f.qualification.writes.clear(),
        }
        assert_eq!(
            admit_module_features(request(0x40), &f, bounds(), MonoTime(0)),
            Err(CdbRefusal::Qualification)
        );
    }
}

#[test]
fn stale_completion_is_not_success_and_trigger_is_never_replayed() {
    let mut o = owner(module_facts());
    o.through_trigger(); // arming flag was already 0xc7
    let a = o.next();
    assert!(matches!(&a, CdbAction::Read(r) if r.purpose() == CdbReadPurpose::CompletionFlag));
    o.answer(a, &[0x07]); // fresh observation has no command-complete flag
    assert!(o.state.terminal().is_none());
    assert_eq!(o.state.evidence().fresh_completion, None);
    complete(&mut o, 36, 255, &[0; 36]);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::CompletedModuleFeatures { .. })
    ));
    assert_eq!(
        o.actions
            .iter()
            .filter(|a| matches!(a,CdbAction::Write(w) if w.purpose() == CdbWritePurpose::Trigger))
            .count(),
        1
    );
    assert!(o.event(CdbEvent::Drive).is_empty());
}

#[test]
fn boundary_at_every_transfer_fences_late_results_without_trigger_replay() {
    let body = [0; 120];
    let mut baseline = owner(module_facts());
    complete(&mut baseline, 120, 255, &body);
    let count = baseline
        .actions
        .iter()
        .filter(|a| matches!(a, CdbAction::Read(_) | CdbAction::Write(_)))
        .count();
    for stop_at in 0..count {
        for dispatched in [false, true] {
            let mut o = owner(module_facts());
            for _ in 0..stop_at {
                let a = o.next();
                let b = bytes_for(&a, 120, 255, &body);
                o.answer(a, &b);
            }
            let a = o.next();
            if dispatched {
                o.dispatch(&a);
            }
            o.event(CdbEvent::BoundaryLost);
            let terminal = o.state.terminal().unwrap().clone();
            assert!(matches!(
                terminal,
                CdbOutcome::StoppedBeforeTrigger {
                    reason: CdbStopReason::Boundary,
                    ..
                } | CdbOutcome::AmbiguousAfterTrigger {
                    reason: CdbStopReason::Boundary,
                    ..
                }
            ));
            if dispatched {
                o.now += 1;
                let event = match &a {
                    CdbAction::Read(r) => CdbEvent::ReadResult {
                        id: r.id(),
                        observation: observation(r.range(), bytes_for(&a, 120, 255, &body), o.now),
                        exchange: CdbReadExchange::Success,
                    },
                    CdbAction::Write(w) => CdbEvent::WriteResult {
                        id: w.id(),
                        outcome: ActuationOutcome::Confirmed,
                        completed_at: MonoTime(o.now),
                    },
                    _ => panic!(),
                };
                assert!(o.event(event).iter().all(|e| !matches!(
                    e,
                    CdbAction::Read(_) | CdbAction::Write(_) | CdbAction::Finish(_)
                )));
            }
            assert_eq!(o.state.terminal(), Some(&terminal));
            assert!(o.event(CdbEvent::Drive).is_empty());
            assert!(o.actions.iter().filter(|a| matches!(a,CdbAction::Write(w) if w.purpose() == CdbWritePurpose::Trigger)).count() <= 1);
            assert_eq!(
                o.actions
                    .iter()
                    .filter(|a| matches!(a, CdbAction::Finish(_)))
                    .count(),
                1
            );
        }
    }
}
