use openxcvr_cmis::managed_cdb::*;
use openxcvr_core::{types::management::*, Rev};

fn request(opcode: u16) -> CdbRequest<'static> {
    CdbRequest {
        revision: Rev::CMIS_5_3,
        opcode,
        address: 0x50,
        instance: 1,
        bank: 0,
        lpl: &[],
        epl_len: 0,
        credential_supplied: false,
    }
}
fn identity() -> CdbIdentity {
    CdbIdentity {
        operation: OperationId(10),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        module: "fixture-module".into(),
    }
}
fn range(offset: u8, len: u16, page: u8) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page,
        offset,
        len,
    }
}
fn observation(range: Range, bytes: Vec<u8>, now: u64) -> RangeObservation {
    RangeObservation {
        range,
        outcome: RangeOutcome::Bytes(bytes),
        requester: ReadRequester::ExplicitClient,
        stamp: ObservationStamp {
            generation: ModuleGeneration(1),
            epoch: CacheEpoch(2),
            started_at: MonoTime(now),
            completed_at: MonoTime(now),
            source: ObservationSource::DaemonObserved,
            freshness: Freshness::Fresh,
            consumed_effects: if range.offset == 8 {
                vec![ReadEffect::ClearOnRead]
            } else {
                vec![]
            },
        },
    }
}
fn facts() -> CdbFacts {
    CdbFacts {
        identity: identity(),
        revision_byte: 0x53,
        capabilities: observation(range(163, 4, 1), vec![0x40, 0, 0, 0], 0),
        qualification: CdbQualification {
            identity: identity(),
            reference: "reviewed-fixture-only".into(),
            valid_until: MonoTime(10_000),
            reads: vec![
                (
                    range(8, 1, 0),
                    vec![ByteClassification::from_masks(
                        255,
                        0,
                        0,
                        0,
                        &[ReadEffect::ClearOnRead, ReadEffect::None],
                        ChangeSource::Dynamic,
                    )],
                ),
                (
                    range(37, 1, 0),
                    vec![ByteClassification::from_masks(
                        255,
                        0,
                        0,
                        0,
                        &[ReadEffect::None],
                        ChangeSource::Dynamic,
                    )],
                ),
                (
                    range(134, 112, 0x9f),
                    vec![
                        ByteClassification::from_masks(
                            255,
                            0,
                            0,
                            0,
                            &[ReadEffect::None],
                            ChangeSource::Dynamic
                        );
                        112
                    ],
                ),
            ],
            writes: vec![range(128, 8, 0x9f)],
            backend_read_limit: 8,
            backend_write_limit: 8,
            stop_terminated_writes: true,
            read_nack_preserves_custody: true,
            selector_authority_ready: true,
            durable_audit_ready: true,
            exclusion_enforceable: true,
        },
        operating_state: CdbOperatingState::ModuleReady,
        no_outstanding_cdb: true,
        advertised_completion: None,
        historical_0041: None,
    }
}
fn bounds() -> CdbBounds {
    CdbBounds {
        operation_deadline: MonoTime(1000),
        preflight_deadline: MonoTime(100),
        completion_budget: MonoDuration(500),
        poll_interval: MonoDuration(5),
        max_polls: 20,
    }
}
fn admitted() -> (ManagedCdbPlan, ManagedCdbState) {
    admit_get_firmware_info(request(0x0100), &facts(), bounds(), MonoTime(0)).unwrap()
}
fn live() -> CdbLiveFacts {
    CdbLiveFacts {
        identity: identity(),
        custody: true,
        audit_ready: true,
        selection: None,
    }
}
#[test]
fn singleton_refuses_every_other_u16_before_plan() {
    for op in 0..=u16::MAX {
        if op != 0x0100 {
            assert!(
                admit_get_firmware_info(request(op), &facts(), bounds(), MonoTime(0)).is_err(),
                "{op:04x}"
            );
        }
    }
}
#[test]
fn admission_tuple_and_missing_facts_refused() {
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev {
            family: openxcvr_core::rev::Family::Sff8636,
            major: 5,
            minor: 3,
        },
        Rev {
            family: openxcvr_core::rev::Family::Cmis,
            major: 6,
            minor: 0,
        },
    ] {
        let mut r = request(0x100);
        r.revision = revision;
        assert!(admit_get_firmware_info(r, &facts(), bounds(), MonoTime(0)).is_err());
    }
    for n in 0..6 {
        let mut r = request(0x100);
        match n {
            0 => r.lpl = &[0],
            1 => r.epl_len = 1,
            2 => r.instance = 2,
            3 => r.bank = 1,
            4 => r.address = 0x51,
            _ => r.credential_supplied = true,
        }
        assert!(admit_get_firmware_info(r, &facts(), bounds(), MonoTime(0)).is_err());
    }
    let mut f = facts();
    f.qualification.reads[0].1[0].uncovered_mask = 0x38;
    assert!(admit_get_firmware_info(request(0x100), &f, bounds(), MonoTime(0)).is_err());
    for bits in [0, 0xc0] {
        let mut f = facts();
        f.capabilities.outcome = RangeOutcome::Bytes(vec![bits, 0, 0, 0]);
        assert!(admit_get_firmware_info(request(0x100), &f, bounds(), MonoTime(0)).is_err());
    }
}
#[test]
fn first_step_is_exact_live_status_read_and_busy_stages_nothing() {
    let (p, s) = admitted();
    let l = live();
    let r = cdb_step(&p, &s, CdbEvent::Drive, &l, MonoTime(0)).unwrap();
    let CdbAction::Read(step) = &r.effects[0] else {
        panic!("not read")
    };
    assert_eq!(step.range(), range(37, 1, 0));
    let id = step.id();
    let r = cdb_step(
        &p,
        &r.state,
        CdbEvent::Dispatched {
            id,
            at: MonoTime(0),
        },
        &l,
        MonoTime(0),
    )
    .unwrap();
    let r = cdb_step(
        &p,
        &r.state,
        CdbEvent::ReadResult {
            id,
            observation: observation(range(37, 1, 0), vec![0xc0], 1),
            exchange: CdbReadExchange::Success,
        },
        &l,
        MonoTime(1),
    )
    .unwrap();
    assert_eq!(r.effects, vec![CdbAction::WaitUntil(MonoTime(6))]);
    let r = cdb_step(&p, &r.state, CdbEvent::Drive, &l, MonoTime(100)).unwrap();
    assert!(r.effects.iter().any(|a| matches!(
        a,
        CdbAction::Finish(outcome) if matches!(outcome.as_ref(), CdbOutcome::BusyTimeout { stage: CdbTimeoutStage::Preflight, .. })
    )));
}
#[test]
fn cancellation_and_boundary_before_io_offer_no_transfer() {
    for event in [CdbEvent::Cancel, CdbEvent::BoundaryLost] {
        let (p, s) = admitted();
        let r = cdb_step(&p, &s, event, &live(), MonoTime(0)).unwrap();
        assert!(r.effects.iter().any(|a| matches!(
            a,
            CdbAction::Finish(outcome) if matches!(outcome.as_ref(), CdbOutcome::StoppedBeforeTrigger { .. })
        )));
        assert!(
            cdb_step(&p, &r.state, CdbEvent::Drive, &live(), MonoTime(1))
                .unwrap()
                .effects
                .is_empty()
        );
    }
}
#[test]
fn audit_description_is_full_opcode_and_payload_free() {
    let (p, _) = admitted();
    let d = p.audit_description();
    assert_eq!(d.opcode, 0x0100);
    assert_eq!(d.revision, Rev::CMIS_5_3);
    assert_eq!(d.classification, "singleton-informational");
    let debug = format!("{d:?}");
    assert!(!debug.contains("bytes"));
    assert!(!debug.contains("payload"));
}

/// Test owner: no I/O; selector witnesses are produced by the real core reducer.
struct Owner {
    plan: ManagedCdbPlan,
    state: ManagedCdbState,
    live: CdbLiveFacts,
    now: u64,
    actions: Vec<CdbAction>,
}
impl Owner {
    fn new(facts: CdbFacts) -> Self {
        let (plan, state) =
            admit_get_firmware_info(request(0x100), &facts, bounds(), MonoTime(0)).unwrap();
        Self {
            plan,
            state,
            live: live(),
            now: 0,
            actions: vec![],
        }
    }
    fn event(&mut self, event: CdbEvent) -> Vec<CdbAction> {
        let r = cdb_step(
            &self.plan,
            &self.state,
            event,
            &self.live,
            MonoTime(self.now),
        )
        .unwrap();
        self.state = r.state;
        self.actions.extend(r.effects.clone());
        r.effects
    }
    fn next(&mut self) -> CdbAction {
        loop {
            let actions = self.event(CdbEvent::Drive);
            assert_eq!(actions.len(), 1, "{actions:?}");
            match actions[0].clone() {
                CdbAction::WaitUntil(t) => self.now = t.0,
                CdbAction::RequireSelection { id, target } => {
                    use openxcvr_core::selector::*;
                    let byte = ByteClassification::from_masks(
                        255,
                        0,
                        0,
                        0,
                        &[ReadEffect::None],
                        ChangeSource::Dynamic,
                    );
                    let footprint = ReadFootprint {
                        revision: Rev::CMIS_5_3,
                        address: 0x50,
                        lower: std::array::from_fn(|_| byte.clone()),
                        upper_page00: std::array::from_fn(|_| byte.clone()),
                    };
                    let layout = SelectorLayout::cmis_banked(Rev::CMIS_5_3, &footprint).unwrap();
                    let s = SelectorState::new(ModuleGeneration(1), CacheEpoch(2));
                    let p = selector_plan(&s, target, &layout, OperationId(id.sequence() + 100))
                        .unwrap();
                    let mut s = selector_begin(&s, &p);
                    while let Some(pending) = s.pending() {
                        s = selector_complete(
                            &s,
                            &SelectorSuppliedResult {
                                stamp: pending.stamp,
                                result: SelectorResult::Read(RangeOutcome::Bytes(vec![0, 0x9f])),
                            },
                        )
                        .state;
                    }
                    self.live.selection = Some(CdbSelectionEvidence {
                        for_step: id,
                        core: s,
                    });
                    self.event(CdbEvent::SelectionConfirmed { id });
                }
                a => return a,
            }
        }
    }
    fn dispatch(&mut self, action: &CdbAction) {
        let id = match action {
            CdbAction::Read(r) => r.id(),
            CdbAction::Write(w) => w.id(),
            _ => panic!("not transfer"),
        };
        let effects = self.event(CdbEvent::Dispatched {
            id,
            at: MonoTime(self.now),
        });
        if effects
            .iter()
            .any(|e| matches!(e, CdbAction::InvalidateSelector(_)))
        {
            self.live.selection = None;
        }
    }
    fn answer(&mut self, action: CdbAction, data: &[u8]) -> Vec<CdbAction> {
        self.dispatch(&action);
        self.now += 1;
        match action {
            CdbAction::Read(r) => self.event(CdbEvent::ReadResult {
                id: r.id(),
                observation: observation(r.range(), data.to_vec(), self.now),
                exchange: CdbReadExchange::Success,
            }),
            CdbAction::Write(w) => self.event(CdbEvent::WriteResult {
                id: w.id(),
                outcome: ActuationOutcome::Confirmed,
                completed_at: MonoTime(self.now),
            }),
            _ => panic!("not transfer"),
        }
    }
    fn through_trigger(&mut self) {
        loop {
            let a = self.next();
            let trigger = matches!(&a,CdbAction::Write(w) if w.purpose()==CdbWritePurpose::Trigger);
            let data = match &a {
                CdbAction::Read(r) if r.purpose() == CdbReadPurpose::Arm => vec![0xc7],
                _ => vec![1],
            };
            self.answer(a, &data);
            if trigger {
                break;
            }
        }
    }
    fn through_status(&mut self, status: u8) {
        self.through_trigger();
        let a = self.next();
        self.answer(a, &[0x40]);
        let a = self.next();
        self.answer(a, &[status]);
    }
}
#[test]
fn exact_staging_both_trigger_methods_repeated_opcode_and_valid_reply() {
    for method in [0, 0x80] {
        for _ in 0..2 {
            let mut f = facts();
            f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, method, 0]);
            let mut o = Owner::new(f);
            o.through_status(1);
            let body: Vec<u8> = (0..110).collect();
            let checksum = !body.iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
            let a = o.next();
            o.answer(a, &[110, checksum]);
            while o.state.terminal().is_none() {
                let a = o.next();
                let CdbAction::Read(r) = &a else { panic!() };
                assert!(r.range().len <= 8);
                assert!(r.range().end() <= 246);
                let start = usize::from(r.range().offset - 136);
                let end = start + usize::from(r.range().len);
                o.answer(a, &body[start..end]);
            }
            let CdbOutcome::Completed { reply, evidence } = o.state.terminal().unwrap() else {
                panic!("{:?}", o.state.terminal())
            };
            assert_eq!(reply.firmware_status, vec![0]);
            assert_eq!(reply.image_information, vec![1]);
            assert_eq!(reply.factory_boot_firmware, body[74..110]);
            assert!(evidence.fresh_completion.is_some());
            assert_eq!(evidence.checksum_valid, Some(true));
            assert_eq!(evidence.consumed[0].1.bytes(), Some(&[0xc7][..]));
            let writes: Vec<_> = o
                .actions
                .iter()
                .filter_map(|a| {
                    if let CdbAction::Write(w) = a {
                        Some((w.range(), w.bytes().to_vec()))
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                writes,
                vec![
                    (range(130, 4, 0x9f), vec![0, 0, 0, 0xfe]),
                    (range(134, 2, 0x9f), vec![255, 0]),
                    (range(128, 1, 0x9f), vec![1]),
                    (range(129, 1, 0x9f), vec![0])
                ]
            );
            assert_eq!(
                o.actions
                    .iter()
                    .filter(|a| matches!(a, CdbAction::InvalidateSelector(_)))
                    .count(),
                2
            );
            assert_eq!(
                o.actions
                    .iter()
                    .filter(|a| matches!(a, CdbAction::RequireSelection { .. }))
                    .count(),
                2
            );
            assert!(o.event(CdbEvent::Drive).is_empty());
        }
    }
}
#[test]
fn stale_flag_and_old_idle_are_not_completion() {
    let mut o = Owner::new(facts());
    o.through_trigger();
    let a = o.next();
    o.answer(a, &[0]);
    assert!(o.state.evidence().fresh_completion.is_none());
    let CdbAction::Read(r) = o.next() else {
        panic!()
    };
    assert_eq!(r.purpose(), CdbReadPurpose::CompletionFlag);
}
#[test]
fn header_length_and_checksum_rejections() {
    for (len, expected) in [
        (0, CdbInvalidReply::Truncated),
        (109, CdbInvalidReply::Truncated),
        (111, CdbInvalidReply::UnexpectedLength),
        (120, CdbInvalidReply::UnexpectedLength),
        (121, CdbInvalidReply::ReservedLength),
        (239, CdbInvalidReply::ReservedLength),
        (240, CdbInvalidReply::EplUnsupported),
        (255, CdbInvalidReply::EplUnsupported),
        (110, CdbInvalidReply::Checksum),
    ] {
        let mut o = Owner::new(facts());
        o.through_status(1);
        let a = o.next();
        o.answer(a, &[len, 0]);
        if len == 110 {
            while o.state.terminal().is_none() {
                let a = o.next();
                let CdbAction::Read(r) = &a else { panic!() };
                let b = vec![0; usize::from(r.range().len)];
                o.answer(a, &b);
            }
        }
        assert!(
            matches!(o.state.terminal(),Some(CdbOutcome::InvalidReply{reason,..}) if *reason==expected)
        );
    }
}
#[test]
fn interruptions_at_every_transfer_are_finish_once_and_never_replay() {
    for cancel in [true, false] {
        for cut in 0..26 {
            let mut o = Owner::new(facts());
            for _ in 0..cut {
                if o.state.terminal().is_some() {
                    break;
                }
                let a = o.next();
                let b = match &a {
                    CdbAction::Read(r) => match r.purpose() {
                        CdbReadPurpose::Arm => vec![0xc7],
                        CdbReadPurpose::CompletionFlag => vec![0x40],
                        CdbReadPurpose::ReplyHeader => vec![110, 255],
                        CdbReadPurpose::ReplyBody => vec![0; usize::from(r.range().len)],
                        _ => vec![1],
                    },
                    _ => vec![],
                };
                o.answer(a, &b);
            }
            if o.state.terminal().is_some() {
                continue;
            }
            let a = o.next();
            o.dispatch(&a);
            let triggered = o.state.evidence().trigger_may_have_executed;
            o.event(if cancel {
                CdbEvent::Cancel
            } else {
                CdbEvent::BoundaryLost
            });
            assert!(
                matches!(
                    o.state.terminal(),
                    Some(CdbOutcome::AmbiguousAfterTrigger { .. })
                ) == triggered
            );
            assert!(o.event(CdbEvent::Drive).is_empty());
            let terminal = o.state.terminal().cloned();
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
fn deadline_with_outstanding_read_finishes_without_waiting_for_result() {
    let mut o = Owner::new(facts());
    let a = o.next();
    o.dispatch(&a);
    o.now = 100;
    o.event(CdbEvent::Drive);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::StoppedBeforeTrigger {
            reason: CdbStopReason::DeadlineExpired,
            ..
        })
    ));
    assert!(o.state.outstanding().is_some());
}
#[test]
fn one_byte_backend_limits_cover_header_and_split_staging() {
    let mut f = facts();
    f.qualification.backend_read_limit = 1;
    f.qualification.backend_write_limit = 1;
    let mut o = Owner::new(f);
    o.through_status(1);
    let a = o.next();
    let CdbAction::Read(r) = &a else { panic!() };
    assert_eq!(r.range(), range(134, 1, 0x9f));
    o.answer(a, &[110]);
    let a = o.next();
    let CdbAction::Read(r) = &a else { panic!() };
    assert_eq!(r.range(), range(135, 1, 0x9f));
    o.answer(a, &[255]);
    while o.state.terminal().is_none() {
        let a = o.next();
        o.answer(a, &[0]);
    }
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::Completed { .. })
    ));
}

#[test]
fn foreground_nack_is_bounded_but_unknown_failure_is_not_busy() {
    for (exchange, expected_timeout) in [
        (CdbReadExchange::QualifiedContinuityPreservingNack, true),
        (CdbReadExchange::UnknownFailure, false),
        (CdbReadExchange::OwnershipLost, false),
    ] {
        let mut o = Owner::new(facts());
        o.through_trigger();
        let a = o.next();
        let CdbAction::Read(r) = &a else { panic!() };
        let id = r.id();
        let range = r.range();
        o.dispatch(&a);
        o.now = if expected_timeout { o.now + 1 } else { 1000 };
        let mut obs = observation(range, vec![], o.now);
        obs.outcome = RangeOutcome::Failed {
            reason: "fixture NACK".into(),
        };
        o.event(CdbEvent::ReadResult {
            id,
            observation: obs,
            exchange,
        });
        if expected_timeout {
            assert!(o.state.terminal().is_none());
            o.now = 1000;
            o.event(CdbEvent::Drive);
            assert!(matches!(
                o.state.terminal(),
                Some(CdbOutcome::BusyTimeout {
                    stage: CdbTimeoutStage::AfterTrigger,
                    ..
                })
            ));
        } else {
            assert!(matches!(
                o.state.terminal(),
                Some(CdbOutcome::AmbiguousAfterTrigger { .. })
            ));
        }
        assert!(o.state.evidence().trigger_may_have_executed);
    }
}
#[test]
fn correlated_status_requires_exact_success_busy_ignores_failed_bit() {
    for (status, reason) in [
        (0, CdbInvalidReply::UnexpectedStatus),
        (2, CdbInvalidReply::UnexpectedStatus),
        (0xc1, CdbInvalidReply::BusyAfterCompletion),
    ] {
        let mut o = Owner::new(facts());
        o.through_status(status);
        assert!(
            matches!(o.state.terminal(),Some(CdbOutcome::InvalidReply{reason:r,..}) if *r==reason)
        );
    }
    let mut o = Owner::new(facts());
    o.through_status(0x42);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::Failed {
            raw_status: 0x42,
            result: 2,
            ..
        })
    ));
}
#[test]
fn result_ids_stamps_lengths_and_dispatch_are_checked() {
    let mut o = Owner::new(facts());
    let a = o.next();
    let CdbAction::Read(r) = a else { panic!() };
    let event = CdbEvent::ReadResult {
        id: r.id(),
        observation: observation(r.range(), vec![1], 0),
        exchange: CdbReadExchange::Success,
    };
    assert_eq!(
        cdb_step(&o.plan, &o.state, event, &o.live, MonoTime(0)).unwrap_err(),
        CdbInputError::NotDispatched
    );
    o.dispatch(&CdbAction::Read(r.clone()));
    let mut bad = observation(r.range(), vec![1], 1);
    bad.stamp.epoch = CacheEpoch(3);
    assert_eq!(
        cdb_step(
            &o.plan,
            &o.state,
            CdbEvent::ReadResult {
                id: r.id(),
                observation: bad,
                exchange: CdbReadExchange::Success
            },
            &o.live,
            MonoTime(1)
        )
        .unwrap_err(),
        CdbInputError::InvalidObservation
    );
    o.now = 1;
    o.event(CdbEvent::ReadResult {
        id: r.id(),
        observation: observation(r.range(), vec![], 1),
        exchange: CdbReadExchange::Success,
    });
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::StoppedBeforeTrigger { .. })
    ));
    assert_eq!(
        cdb_step(&o.plan, &o.state, CdbEvent::Drive, &o.live, MonoTime(0)).unwrap_err(),
        CdbInputError::BackwardTime
    );
    let mut other = Owner::new(facts());
    let CdbAction::Read(other_read) = other.next() else {
        panic!()
    };
    other.answer(CdbAction::Read(other_read.clone()), &[1]);
    assert_eq!(
        cdb_step(
            &other.plan,
            &other.state,
            CdbEvent::Dispatched {
                id: other_read.id(),
                at: MonoTime(1)
            },
            &other.live,
            MonoTime(1)
        )
        .unwrap_err(),
        CdbInputError::WrongStep
    );
}
#[test]
fn audit_failure_and_selection_loss_stop_writes_without_rollback() {
    for audit in [false, true] {
        let mut o = Owner::new(facts());
        let a = o.next();
        o.answer(a, &[1]);
        let a = o.next();
        if audit {
            o.live.audit_ready = false;
        } else {
            o.live.selection = None;
        }
        let CdbAction::Write(w) = a else { panic!() };
        o.event(CdbEvent::Dispatched {
            id: w.id(),
            at: MonoTime(o.now),
        });
        assert!(matches!(
            o.state.terminal(),
            Some(CdbOutcome::StoppedBeforeTrigger { .. })
        ));
        assert!(o
            .state
            .evidence()
            .issued
            .iter()
            .all(|s| s.write_purpose.is_none()));
    }
    let mut o = Owner::new(facts());
    o.through_trigger();
    o.live.audit_ready = false;
    o.event(CdbEvent::Drive);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::AmbiguousAfterTrigger {
            reason: CdbStopReason::AuditUnavailable,
            ..
        })
    ));
}
#[test]
fn all_missing_facts_and_invalid_bounds_refuse_before_plan() {
    for n in 0..15 {
        let mut f = facts();
        match n {
            0 => f.revision_byte = 0x52,
            1 => f.no_outstanding_cdb = false,
            2 => f.identity.module.clear(),
            3 => f.qualification.valid_until = MonoTime(0),
            4 => f.qualification.durable_audit_ready = false,
            5 => f.qualification.exclusion_enforceable = false,
            6 => f.qualification.stop_terminated_writes = false,
            7 => f.qualification.backend_read_limit = 0,
            8 => f.qualification.backend_write_limit = 0,
            9 => f.qualification.selector_authority_ready = false,
            10 => f.operating_state = CdbOperatingState::UnknownOrRestricted,
            11 => f.capabilities.stamp.freshness = Freshness::Historical,
            12 => f.qualification.identity.epoch = CacheEpoch(3),
            13 => f.capabilities.stamp.epoch = CacheEpoch(3),
            _ => f.qualification.reads.clear(),
        };
        assert!(
            admit_get_firmware_info(request(0x100), &f, bounds(), MonoTime(0)).is_err(),
            "case {n}"
        );
    }
    for n in 0..6 {
        let mut b = bounds();
        match n {
            0 => b.max_polls = 0,
            1 => b.poll_interval = MonoDuration(0),
            2 => b.completion_budget = MonoDuration(0),
            3 => b.preflight_deadline = MonoTime(0),
            4 => b.preflight_deadline = MonoTime(1001),
            _ => b.operation_deadline = MonoTime(0),
        };
        assert_eq!(
            admit_get_firmware_info(request(0x100), &facts(), b, MonoTime(0)).unwrap_err(),
            CdbRefusal::InvalidBounds
        );
    }
    let mut f = facts();
    f.qualification.valid_until = MonoTime(u64::MAX);
    f.capabilities.stamp.completed_at = MonoTime(u64::MAX - 10);
    let b = CdbBounds {
        operation_deadline: MonoTime(u64::MAX),
        preflight_deadline: MonoTime(u64::MAX),
        completion_budget: MonoDuration(50),
        poll_interval: MonoDuration(1),
        max_polls: 1,
    };
    assert_eq!(
        admit_get_firmware_info(request(0x100), &f, b, MonoTime(u64::MAX - 10)).unwrap_err(),
        CdbRefusal::InvalidBounds
    );
}
#[test]
fn foreground_holdoff_policy_and_advertised_completion_are_recorded() {
    for (a, b, ns) in [
        (0, 80, 0),
        (0, 0, 80_000_000),
        (0, 128, 160_000_000),
        (31, 128, 4_960_000_000),
    ] {
        let mut f = facts();
        f.capabilities.outcome = RangeOutcome::Bytes(vec![0x40, 0, a, b]);
        f.advertised_completion = Some((MonoDuration(300), "validated-0040-capture".into()));
        let (p, _) = admit_get_firmware_info(request(0x100), &f, bounds(), MonoTime(0)).unwrap();
        assert_eq!(
            p.audit_description().foreground_holdoff,
            Some(MonoDuration(ns))
        );
        assert_eq!(p.audit_description().completion_bound, MonoDuration(300));
    }
}
#[test]
fn short_reply_chunk_is_invalid_without_zero_padding() {
    let mut o = Owner::new(facts());
    o.through_status(1);
    let a = o.next();
    o.answer(a, &[110, 255]);
    let a = o.next();
    o.answer(a, &[0]);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::InvalidReply {
            reason: CdbInvalidReply::ByteCount,
            ..
        })
    ));
    assert_eq!(o.state.evidence().reads.last().unwrap().actual_length, 1);
}

#[test]
fn qualified_generated_footprints_do_not_replace_backend_and_owner_qualification() {
    // Source gaps are now closed, but schema evidence is not a production
    // backend/owner receipt. Each of these independent gates still refuses.
    use openxcvr_cmis::generated::union::CmisLowerMemory;
    let lower = CmisLowerMemory::read_classifications();
    let mut f = facts();
    f.qualification.reads[0].1 = vec![lower[8].clone()];
    f.qualification.reads[1].1 = vec![lower[37].clone()];
    for gate in 0..5 {
        let mut unqualified = f.clone();
        match gate {
            0 => unqualified.qualification.backend_read_limit = 0,
            1 => unqualified.qualification.stop_terminated_writes = false,
            2 => unqualified.qualification.selector_authority_ready = false,
            3 => unqualified.qualification.exclusion_enforceable = false,
            _ => unqualified.qualification.durable_audit_ready = false,
        }
        assert_eq!(
            admit_get_firmware_info(request(0x100), &unqualified, bounds(), MonoTime(0))
                .unwrap_err(),
            CdbRefusal::Qualification
        );
    }
}
#[test]
fn consuming_read_needs_effect_provenance_and_retains_even_short_observation() {
    let mut o = Owner::new(facts());
    loop {
        let a = o.next();
        if matches!(&a,CdbAction::Read(r) if r.purpose()==CdbReadPurpose::Arm) {
            let CdbAction::Read(r) = &a else {
                unreachable!()
            };
            let (id, range) = (r.id(), r.range());
            o.dispatch(&a);
            o.now += 1;
            let mut obs = observation(range, vec![], o.now);
            obs.stamp.consumed_effects.clear();
            o.event(CdbEvent::ReadResult {
                id,
                observation: obs,
                exchange: CdbReadExchange::Success,
            });
            break;
        } else {
            o.answer(a, &[1]);
        }
    }
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::StoppedBeforeTrigger { .. })
    ));
    assert_eq!(o.state.evidence().consumed.len(), 1);
    assert!(!o.state.evidence().trigger_may_have_executed);
}
#[test]
fn post_trigger_holdoff_zero_and_finite_poll_budget_never_become_unbounded() {
    let mut f = facts();
    f.capabilities.outcome = RangeOutcome::Bytes(vec![0x40, 0, 0, 80]);
    let mut o = Owner::new(f);
    o.through_trigger();
    let a = o.next();
    let CdbAction::Read(r) = &a else { panic!() };
    let (id, range) = (r.id(), r.range());
    o.dispatch(&a);
    o.now += 1;
    let mut obs = observation(range, vec![], o.now);
    obs.outcome = RangeOutcome::Absent;
    o.event(CdbEvent::ReadResult {
        id,
        observation: obs,
        exchange: CdbReadExchange::QualifiedContinuityPreservingNack,
    });
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::BusyTimeout {
            stage: CdbTimeoutStage::AfterTrigger,
            ..
        })
    ));
    let mut o = Owner::new(facts());
    o.through_trigger();
    for _ in 0..20 {
        let a = o.next();
        o.answer(a, &[0]);
    }
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::BusyTimeout {
            stage: CdbTimeoutStage::AfterTrigger,
            ..
        })
    ));
}
#[test]
fn cancel_before_dispatch_fences_offered_trigger() {
    let mut o = Owner::new(facts());
    loop {
        let a = o.next();
        if let CdbAction::Write(w) = &a {
            if w.purpose() == CdbWritePurpose::Trigger {
                let id = w.id();
                o.event(CdbEvent::Cancel);
                o.event(CdbEvent::Dispatched {
                    id,
                    at: MonoTime(o.now),
                });
                assert!(!o.state.dispatch_authorized(id));
                assert!(!o.state.evidence().trigger_may_have_executed);
                break;
            }
        }
        o.answer(a, &[1]);
    }
}

#[test]
fn audit_projection_contains_descriptions_not_dispatch_or_history_payloads() {
    let mut o = Owner::new(facts());
    o.through_status(1);
    let body: Vec<u8> = vec![0; 110];
    let a = o.next();
    o.answer(a, &[110, 255]);
    while o.state.terminal().is_none() {
        let a = o.next();
        let CdbAction::Read(r) = &a else { panic!() };
        let len = usize::from(r.range().len);
        o.answer(a, &body[..len]);
    }
    let audit = o.state.evidence().audit_description();
    assert_eq!(audit.consumed_read_references.len(), 2);
    assert_eq!(audit.raw_status, Some(1));
    assert_eq!(audit.actual_reply_length, Some(110));
    assert_eq!(audit.checksum_valid, Some(true));
    let debug = format!("{audit:?}");
    for forbidden in [
        "Bytes(",
        "firmware_status",
        "image_information",
        "c7",
        "[199]",
    ] {
        assert!(!debug.contains(forbidden));
    }
    let descriptions = &o.plan.audit_description().intended_writes;
    assert_eq!(descriptions.len(), 4);
    assert_eq!(descriptions.last().unwrap().range, range(129, 1, 0x9f));
    let outcome = o.state.terminal().unwrap();
    assert_eq!(outcome.audit_reason(), CdbTerminalReason::Completed);
    let images = outcome.firmware_images().unwrap();
    assert!(images.image_a.is_none());
    assert!(images.image_b.is_none());
    assert!(images.factory_boot.is_none());
}

#[test]
fn recheck_busy_and_reused_selector_confirmation_cannot_authorize_next_write_or_read() {
    let mut o = Owner::new(facts());
    loop {
        let a = o.next();
        if matches!(&a,CdbAction::Read(r) if r.purpose()==CdbReadPurpose::RecheckIdle) {
            o.answer(a, &[0xc0]);
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
    let mut o = Owner::new(facts());
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
}
#[test]
fn uncertain_trigger_is_one_shot_and_actual_late_write_time_is_retained() {
    let mut o = Owner::new(facts());
    loop {
        let a = o.next();
        if matches!(&a,CdbAction::Write(w) if w.purpose()==CdbWritePurpose::Trigger) {
            let CdbAction::Write(w) = &a else { panic!() };
            let id = w.id();
            o.dispatch(&a);
            let end = o.now + 1;
            o.now = 1000;
            o.event(CdbEvent::WriteResult {
                id,
                outcome: ActuationOutcome::Uncertain {
                    reason: "lost answer".into(),
                },
                completed_at: MonoTime(end),
            });
            assert_eq!(
                o.state.evidence().issued.last().unwrap().completed_at,
                Some(MonoTime(end))
            );
            assert!(matches!(
                o.state.terminal(),
                Some(CdbOutcome::AmbiguousAfterTrigger {
                    reason: CdbStopReason::Transport,
                    ..
                })
            ));
            assert!(o.state.evidence().trigger_may_have_executed);
            assert!(o.event(CdbEvent::Drive).is_empty());
            break;
        }
        o.answer(a, &[1]);
    }
}
#[test]
fn trigger_time_overflow_is_rejected_before_dispatch_latch() {
    let mut f = facts();
    f.qualification.valid_until = MonoTime(u64::MAX);
    let mut b = bounds();
    b.operation_deadline = MonoTime(u64::MAX);
    let (plan, state) = admit_get_firmware_info(request(0x100), &f, b, MonoTime(0)).unwrap();
    let mut o = Owner {
        plan,
        state,
        live: live(),
        now: 0,
        actions: vec![],
    };
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
fn deadline_after_proven_completion_is_missing_reply_not_a_busy_claim() {
    let mut o = Owner::new(facts());
    o.through_status(1);
    let a = o.next();
    o.dispatch(&a);
    o.now = 1000;
    o.event(CdbEvent::Drive);
    assert!(matches!(
        o.state.terminal(),
        Some(CdbOutcome::AmbiguousAfterTrigger {
            reason: CdbStopReason::DeadlineExpired,
            ..
        })
    ));
    assert_eq!(o.state.evidence().raw_status, Some(1));
    assert!(o.state.evidence().fresh_completion.is_some());
    assert!(o.state.outstanding().is_some());
    assert!(o.event(CdbEvent::Drive).is_empty());
}

#[test]
fn source_qualified_lower_bytes_flow_to_managed_consumption_with_collateral_retained() {
    use openxcvr_cmis::generated::{union::CmisLowerMemory, v5_3::CmisPage9FhCdbMessage};
    let lower = CmisLowerMemory::read_classifications();
    let upper = CmisPage9FhCdbMessage::read_classifications();
    let mut f = facts();
    f.qualification.reads[0].1 = vec![lower[8].clone()];
    f.qualification.reads[1].1 = vec![lower[37].clone()];
    f.qualification.reads[2].1 = upper[6..118].to_vec();
    assert_eq!(lower[8].known_effects, vec![ReadEffect::ClearOnRead]);
    assert_eq!(lower[37].known_effects, vec![ReadEffect::None]);
    assert_eq!(lower[37].change_source, ChangeSource::Dynamic);
    // All other facts remain explicit synthetic witnesses; schema evidence
    // does not prove a real backend, selector, exclusion or audit qualification.
    let mut o = Owner::new(f);
    o.through_trigger();
    assert_eq!(o.state.evidence().consumed.len(), 1);
    assert_eq!(
        o.state.evidence().consumed[0].1.bytes(),
        Some([0xc7].as_slice())
    );
    let completion = o.next();
    o.answer(completion, &[0xff]); // retain even reserved bits; do not mask collateral
    let status = o.next();
    o.answer(status, &[1]);
    assert_eq!(o.state.evidence().consumed.len(), 2);
    assert_eq!(
        o.state.evidence().consumed[1].1.bytes(),
        Some([0xff].as_slice())
    );
    for (_, observation) in &o.state.evidence().consumed {
        assert_eq!(
            observation.stamp.consumed_effects,
            vec![ReadEffect::ClearOnRead]
        );
    }
}

#[test]
fn baseline_0100_action_sequence() {
    for method in [0, 0x80] {
        for limit in 1..=8 {
            let mut f = facts();
            f.capabilities.outcome = RangeOutcome::Bytes(vec![0x60, 0, method, 0]);
            f.qualification.backend_read_limit = limit;
            f.qualification.backend_write_limit = limit;
            let mut o = Owner::new(f);
            o.through_status(1);
            let body: Vec<u8> = (0..110).collect();
            let checksum = !body.iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
            while o.state.terminal().is_none() {
                let a = o.next();
                let CdbAction::Read(r) = &a else { panic!() };
                let bytes = if r.purpose() == CdbReadPurpose::ReplyHeader {
                    let start = usize::from(r.range().offset - 134);
                    [110, checksum][start..start + usize::from(r.range().len)].to_vec()
                } else {
                    let start = usize::from(r.range().offset - 136);
                    body[start..start + usize::from(r.range().len)].to_vec()
                };
                o.answer(a, &bytes);
            }
            assert!(matches!(
                o.state.terminal(),
                Some(CdbOutcome::Completed { .. })
            ));
            println!("TRACE-{method}-{limit}: {:?}", o.actions);
        }
    }
}

#[path = "support/managed_0041.rs"]
mod features_0041;

#[path = "support/managed_0040.rs"]
mod features_0040;
