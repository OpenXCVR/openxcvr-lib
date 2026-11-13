// Red-first behaviour pins for the bounded retry subset of task 64
// (`types::management::operations`): a zero-retry policy whose terminal
// decisions preserve the shared `ActuationOutcome` distinction, keeping an
// uncertain outcome uncertain instead of collapsing it into success or
// failure or replaying it.
//
// Retry budgets, backoff, deadlines, and every other task-64 type and
// function are owed by task 64 and deliberately not exercised here.
use openxcvr_core::types::management::operations::{retry_decide, RetryDecision, RetryPolicy};
use openxcvr_core::types::management::{ActuationOutcome, MonoDuration, MonoTime};

const NOW: MonoTime = MonoTime(1_000);

fn policy() -> RetryPolicy {
    RetryPolicy
}

#[test]
fn a_confirmed_outcome_is_terminal_success() {
    assert_eq!(
        retry_decide(ActuationOutcome::Confirmed, 0, NOW, policy()),
        RetryDecision::Terminal(ActuationOutcome::Confirmed)
    );
}

#[test]
fn a_failed_outcome_is_terminal_failure() {
    assert_eq!(
        retry_decide(
            ActuationOutcome::Failed {
                reason: "nack".to_owned()
            },
            0,
            NOW,
            policy()
        ),
        RetryDecision::Terminal(ActuationOutcome::Failed {
            reason: "nack".to_owned()
        })
    );
}

#[test]
fn an_uncertain_outcome_stays_uncertain_and_terminal() {
    let uncertain = ActuationOutcome::Uncertain {
        reason: "timeout after write".to_owned(),
    };
    assert_eq!(
        retry_decide(uncertain.clone(), 0, NOW, policy()),
        RetryDecision::Terminal(uncertain)
    );
}

#[test]
fn no_outcome_is_retried_at_any_attempt_count() {
    let outcomes = [
        ActuationOutcome::Confirmed,
        ActuationOutcome::Failed {
            reason: String::new(),
        },
        ActuationOutcome::Uncertain {
            reason: String::new(),
        },
    ];
    for outcome in outcomes {
        for attempt in [0u32, 1, 7] {
            let decision = retry_decide(outcome.clone(), attempt, NOW, policy());
            assert_eq!(decision, RetryDecision::Terminal(outcome.clone()));
        }
    }
}

// ─── Task 64 remainder: lease, transfer, bits, RMW, power/reset/protection ──
use openxcvr_core::types::management::cache::{write_invalidations, CacheKey, CacheLine};
use openxcvr_core::types::management::operations::*;
use openxcvr_core::types::management::{
    CacheEpoch, ModuleGeneration, OperationId, PowerGeneration, ReadEffect, ReadRequester,
};

const GEN: ModuleGeneration = ModuleGeneration(7);
const EPOCH: CacheEpoch = CacheEpoch(9);
/// The caller-minted operation identity the plan/event fixtures stamp
/// (fresh and strictly increasing per module instance, as the driver's
/// counter convention requires).
const OP: OperationId = OperationId(64);

/// CMIS 5.3 byte 26 bit positions as supplied family descriptors (the
/// values the family layer resolves from the schema; tests supply them
/// like a caller would).
fn cmis_b26() -> PowerControlBits {
    PowerControlBits {
        low_pwr_request_sw: 1 << 4,
        low_pwr_allow_hw: 1 << 6,
        software_reset: 1 << 3,
    }
}

fn fresh_read() -> RmwReadFacts {
    RmwReadFacts {
        generation: GEN,
        epoch: EPOCH,
    }
}

fn excluded() -> ExclusionContract {
    ExclusionContract::MutationExcluded {
        generation: GEN,
        epoch: EPOCH,
    }
}

fn facts_at(identity: OperationId, epoch: CacheEpoch, current: Option<u8>) -> PowerFacts {
    PowerFacts {
        current_byte: current,
        rmw_read: Some(RmwReadFacts {
            generation: GEN,
            epoch,
        }),
        exclusion: Some(ExclusionContract::MutationExcluded {
            generation: GEN,
            epoch,
        }),
        bits: cmis_b26(),
        identity,
        generation: GEN,
        epoch,
    }
}

fn facts(current: Option<u8>) -> PowerFacts {
    facts_at(OP, EPOCH, current)
}

#[test]
fn a_lease_refuses_absent_unentitled_unknown_and_expired_facts() {
    let now = MonoTime(1_000);
    let lease = LeaseState {
        generation: GEN,
        epoch: EPOCH,
        expires_at: Some(MonoTime(2_000)),
    };
    let facts = LeaseFacts {
        entitled: true,
        rights_known: true,
    };
    // Valid: granted.
    assert_eq!(
        lease_gate(Some(&lease), facts, now, GEN, EPOCH),
        LeaseDecision::Granted
    );
    // No lease at all.
    assert_eq!(
        lease_gate(None, facts, now, GEN, EPOCH),
        LeaseDecision::Refused(LeaseRefusal::NoLease)
    );
    // Unentitled and unknown rights.
    assert_eq!(
        lease_gate(
            Some(&lease),
            LeaseFacts {
                entitled: false,
                rights_known: true
            },
            now,
            GEN,
            EPOCH
        ),
        LeaseDecision::Refused(LeaseRefusal::NotEntitled)
    );
    assert_eq!(
        lease_gate(
            Some(&lease),
            LeaseFacts {
                entitled: true,
                rights_known: false
            },
            now,
            GEN,
            EPOCH
        ),
        LeaseDecision::Refused(LeaseRefusal::UnknownRights)
    );
    // Expiry equality fails closed for grants: at exactly the expiry the
    // lease is already expired (one tick before it is not).
    assert_eq!(
        lease_gate(Some(&lease), facts, MonoTime(2_000), GEN, EPOCH),
        LeaseDecision::Refused(LeaseRefusal::Expired {
            at: MonoTime(2_000),
            expires_at: MonoTime(2_000)
        })
    );
    assert_eq!(
        lease_gate(Some(&lease), facts, MonoTime(1_999), GEN, EPOCH),
        LeaseDecision::Granted
    );
    // A lease verified under a replaced stamp never transfers.
    assert_eq!(
        lease_gate(Some(&lease), facts, now, ModuleGeneration(GEN.0 + 1), EPOCH),
        LeaseDecision::Refused(LeaseRefusal::StaleStamp {
            lease_generation: GEN,
            lease_epoch: EPOCH
        })
    );
}

#[test]
fn transfer_choice_splits_at_the_half_boundary_and_the_limit_never_widens() {
    let range = |offset: u8, len: u16| openxcvr_core::types::management::Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    };
    let limits = TransferLimits {
        max_transfer_len: 100,
        split_at_half: true,
    };
    // A span crossing the half boundary splits at 128 without widening.
    let choice = transfer_choose(range(120, 20), limits).unwrap();
    let spans: Vec<_> = choice
        .iter()
        .map(|c| (c.range.offset, c.range.len))
        .collect();
    assert_eq!(spans, vec![(120, 8), (128, 12)]);
    // The transfer limit caps each piece.
    let choice = transfer_choose(range(0, 250), limits).unwrap();
    let spans: Vec<_> = choice
        .iter()
        .map(|c| (c.range.offset, c.range.len))
        .collect();
    assert_eq!(spans, vec![(0, 100), (100, 28), (128, 100), (228, 22)]);
    // Zero limit and malformed ranges refuse.
    assert_eq!(
        transfer_choose(
            range(0, 4),
            TransferLimits {
                max_transfer_len: 0,
                split_at_half: true
            }
        ),
        Err(TransferRefusal::InvalidLimit)
    );
    assert_eq!(
        transfer_choose(range(250, 20), limits),
        Err(TransferRefusal::InvalidRange(range(250, 20)))
    );
    assert_eq!(
        validate_range(range(0, 0), limits),
        Err(TransferRefusal::InvalidRange(range(0, 0)))
    );
}

#[test]
fn bit_level_writes_refuse_the_first_nonwritable_bit() {
    let request = |len: u16| WriteRequest {
        range: openxcvr_core::types::management::Range {
            address: 0x50,
            bank: 0,
            page: 0,
            offset: 26,
            len,
        },
    };
    // Byte 26: the three control bits are writable; squelch (bit 5) and
    // bank-broadcast (bit 7) are preserved and not ours to touch.
    let b26 = BitAccess {
        writable_mask: (1 << 3) | (1 << 4) | (1 << 6),
    };
    assert_eq!(
        validate_write_bits(&request(1), &[1 << 4], &[b26]),
        BitDecision::Allowed
    );
    assert_eq!(
        validate_write_bits(&request(1), &[1 << 3 | 1 << 4], &[b26]),
        BitDecision::Allowed
    );
    assert_eq!(
        validate_write_bits(&request(1), &[1 << 5], &[b26]),
        BitDecision::Refused(BitRefusal::BitNotWritable { offset: 26, bit: 5 })
    );
    assert_eq!(
        validate_write_bits(&request(1), &[1 << 7 | 1 << 4], &[b26]),
        BitDecision::Refused(BitRefusal::BitNotWritable { offset: 26, bit: 7 })
    );
    assert_eq!(
        validate_write_bits(&request(0), &[], &[b26]),
        BitDecision::Refused(BitRefusal::EmptyRange)
    );
    assert_eq!(
        validate_write_bits(&request(2), &[0, 0], &[b26]),
        BitDecision::Refused(BitRefusal::MaskMismatch {
            expected: 2,
            got: 1
        })
    );
}

#[test]
fn rmw_needs_fresh_data_and_a_real_exclusion_contract() {
    assert_eq!(
        rmw_prerequisites(None, Some(excluded()), GEN, EPOCH),
        RmwDecision::Refused(RmwRefusal::MissingRead)
    );
    assert_eq!(
        rmw_prerequisites(Some(fresh_read()), None, GEN, EPOCH),
        RmwDecision::Refused(RmwRefusal::NoExclusion)
    );
    // Stale read data refuses: the write targets a newer generation.
    assert_eq!(
        rmw_prerequisites(
            Some(RmwReadFacts {
                generation: ModuleGeneration(GEN.0 - 1),
                epoch: EPOCH
            }),
            Some(excluded()),
            GEN,
            EPOCH
        ),
        RmwDecision::Refused(RmwRefusal::StaleRead {
            read_generation: ModuleGeneration(GEN.0 - 1),
            read_epoch: EPOCH
        })
    );
    // An exclusion contract from another stamp is not proof either.
    assert_eq!(
        rmw_prerequisites(
            Some(fresh_read()),
            Some(ExclusionContract::MutationExcluded {
                generation: ModuleGeneration(GEN.0 - 1),
                epoch: EPOCH
            }),
            GEN,
            EPOCH
        ),
        RmwDecision::Refused(RmwRefusal::NoExclusion)
    );
    // Fresh data plus a live contract admits; a token contract admits the
    // plan (its liveness is checked at execution).
    assert_eq!(
        rmw_prerequisites(Some(fresh_read()), Some(excluded()), GEN, EPOCH),
        RmwDecision::Admitted
    );
    assert_eq!(
        rmw_prerequisites(
            Some(fresh_read()),
            Some(ExclusionContract::ExclusiveToken { token: 5 }),
            GEN,
            EPOCH
        ),
        RmwDecision::Admitted
    );
}

#[test]
fn the_power_rmw_preserves_unrelated_fields_and_advances_the_epoch() {
    // Donor byte-26 arithmetic over supplied CMIS bits: force sets bit 4
    // and always clears the reset bit; release clears bits 4 and 6; bits
    // 5 and 7 always survive.
    let bits = cmis_b26();
    let squelch_and_bcast = (1 << 5) | (1 << 7);
    let (forced, owned) = power_byte(squelch_and_bcast, PowerRequest::ForceLowPower, bits);
    assert_eq!(forced, squelch_and_bcast | (1 << 4));
    assert_eq!(owned, (1 << 3) | (1 << 4) | (1 << 6));
    let (released, _) = power_byte(
        (1 << 3) | (1 << 4) | (1 << 6) | squelch_and_bcast,
        PowerRequest::Release,
        bits,
    );
    assert_eq!(released, squelch_and_bcast);
    // The plan's write step carries the computed byte and owned mask, and
    // a successful commit advances the cache epoch.
    let decision = power_request_step(
        PowerState::Idle,
        PowerRequest::ForceLowPower,
        facts(Some(0x00)),
    );
    let PowerPlanDecision::Plan(plan) = decision else {
        panic!("must plan: {decision:?}")
    };
    assert_eq!(
        plan.steps,
        vec![
            OperationStep::PrerequisiteRead {
                intent: openxcvr_core::types::management::ReadIntent {
                    requester: ReadRequester::ExplicitClient,
                    exact_ranges: vec![openxcvr_core::types::management::Range {
                        address: 0x50,
                        bank: 0,
                        page: 0,
                        offset: 26,
                        len: 1
                    }],
                    allowed_effects: vec![ReadEffect::None],
                }
            },
            OperationStep::FieldPreservingWrite {
                range: openxcvr_core::types::management::Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 26,
                    len: 1
                },
                value: vec![1 << 4],
                owned_mask: (1 << 3) | (1 << 4) | (1 << 6),
            },
            OperationStep::AdvanceCacheEpoch,
        ]
    );
    // Missing read data or no exclusion refuses; no descriptor refuses.
    let no_byte = PowerFacts {
        current_byte: None,
        ..facts(Some(0x00))
    };
    assert!(matches!(
        power_request_step(PowerState::Idle, PowerRequest::ForceLowPower, no_byte),
        PowerPlanDecision::Refused(PowerRefusal::Rmw(RmwRefusal::MissingRead))
    ));
    let no_exclusion = PowerFacts {
        rmw_read: None,
        exclusion: None,
        ..facts(Some(0x00))
    };
    assert!(matches!(
        power_request_step(PowerState::Idle, PowerRequest::ForceLowPower, no_exclusion),
        PowerPlanDecision::Refused(PowerRefusal::Rmw(_))
    ));
    let no_bits = PowerFacts {
        bits: PowerControlBits {
            low_pwr_request_sw: 0,
            low_pwr_allow_hw: 0,
            software_reset: 0,
        },
        ..facts(Some(0x00))
    };
    assert!(matches!(
        power_request_step(PowerState::Idle, PowerRequest::ForceLowPower, no_bits),
        PowerPlanDecision::Refused(PowerRefusal::MissingControlBits)
    ));
}

#[test]
fn a_force_arriving_during_a_release_supersedes_it_and_a_stale_release_never_commits() {
    let _bits = cmis_b26();
    let pending_release = PowerState::Pending {
        request: PowerRequest::Release,
        power_generation: PowerGeneration(4),
    };
    let decision = power_request_step(
        pending_release,
        PowerRequest::ForceLowPower,
        facts(Some(0x00)),
    );
    let PowerPlanDecision::SupersedesPending {
        superseded_generation,
        ..
    } = decision
    else {
        panic!("force during release must supersede: {decision:?}")
    };
    assert_eq!(superseded_generation, PowerGeneration(4));
    // The stale release result (generation 4) cannot commit afterwards:
    // its result is discarded without touching the newer pending force.
    let (state, decision) = power_result_step(
        PowerState::Pending {
            request: PowerRequest::ForceLowPower,
            power_generation: PowerGeneration(5),
        },
        PowerGeneration(4),
        &ActuationOutcome::Confirmed,
    );
    assert_eq!(decision, PowerResultDecision::StaleResult);
    assert_eq!(
        state,
        PowerState::Pending {
            request: PowerRequest::ForceLowPower,
            power_generation: PowerGeneration(5)
        }
    );
}

#[test]
fn power_results_commit_only_on_a_confirmed_matching_generation() {
    let pending = PowerState::Pending {
        request: PowerRequest::ForceLowPower,
        power_generation: PowerGeneration(9),
    };
    let (state, decision) =
        power_result_step(pending, PowerGeneration(9), &ActuationOutcome::Confirmed);
    assert_eq!(decision, PowerResultDecision::Committed);
    assert_eq!(
        state,
        PowerState::Committed {
            request: PowerRequest::ForceLowPower,
            power_generation: PowerGeneration(9)
        }
    );
    let (_, decision) = power_result_step(
        pending,
        PowerGeneration(9),
        &ActuationOutcome::Failed {
            reason: "nack".to_owned(),
        },
    );
    assert_eq!(decision, PowerResultDecision::FailedRetryable);
    let (state, decision) = power_result_step(
        pending,
        PowerGeneration(9),
        &ActuationOutcome::Uncertain {
            reason: "timeout".to_owned(),
        },
    );
    assert_eq!(decision, PowerResultDecision::UncertainPending);
    assert_eq!(state, pending);
}

#[test]
fn bind_reconciliation_is_read_only_and_keeps_the_donor_guards() {
    let forced = ReconcileFacts {
        force_lowpwr: true,
        force_generation: PowerGeneration(3),
        committed_generation: PowerGeneration(0),
    };
    // No pending force: guarded, no publication.
    assert_eq!(
        reconcile_power(
            ReconcileModuleState::LowPwr,
            ReconcileFacts {
                force_lowpwr: false,
                ..forced
            }
        ),
        ReconcileDecision::Guarded
    );
    // A committed actuation of the current generation owns the state.
    assert_eq!(
        reconcile_power(
            ReconcileModuleState::LowPwr,
            ReconcileFacts {
                committed_generation: PowerGeneration(3),
                ..forced
            }
        ),
        ReconcileDecision::Guarded
    );
    // LowPwr publishes low and KEEPS the intent (nothing was actuated).
    assert_eq!(
        reconcile_power(ReconcileModuleState::LowPwr, forced),
        ReconcileDecision::Publish {
            low_power: true,
            clear_force: false
        }
    );
    // PwrUp/Ready/Fault publish and clear the force.
    for state in [
        ReconcileModuleState::PwrUp,
        ReconcileModuleState::Ready,
        ReconcileModuleState::Fault,
    ] {
        assert_eq!(
            reconcile_power(state, forced),
            ReconcileDecision::Publish {
                low_power: false,
                clear_force: true
            }
        );
    }
    // Standard-correct divergence (module-state-4-5): code 4 is PwrDn, a
    // transient return toward low power — the intent is kept, not cleared
    // as the donor's erroneous FAULT mapping would.
    assert_eq!(
        reconcile_power(ReconcileModuleState::PwrDn, forced),
        ReconcileDecision::Publish {
            low_power: true,
            clear_force: false
        }
    );
}

#[test]
fn soft_reset_plans_a_field_preserving_write_and_hard_reset_is_typed_unsupported() {
    let _bits = cmis_b26();
    let policy = ResetPolicy {
        poll_interval: MonoDuration(50),
        backstop: MonoDuration(10_000),
    };
    let inputs = |request: ResetRequest, policy: ResetPolicy| ResetInputs {
        request,
        facts: facts(Some(0x00)),
        policy,
    };
    assert_eq!(
        plan_reset(inputs(ResetRequest::Hard, policy)),
        ResetPlanDecision::Refused(ResetRefusal::HardResetUnsupported)
    );
    let decision = plan_reset(inputs(ResetRequest::Soft, policy));
    let ResetPlanDecision::Plan(plan) = decision else {
        panic!("soft reset must plan: {decision:?}")
    };
    // The write sets only the reset bit, then the epoch advances, then
    // the confirmation read polls the bit's clear.
    assert!(matches!(
        plan.steps[0],
        OperationStep::FieldPreservingWrite { .. }
    ));
    assert_eq!(plan.steps[1], OperationStep::AdvanceCacheEpoch);
    assert!(matches!(
        plan.steps[2],
        OperationStep::ConfirmationRead { .. }
    ));
    if let OperationStep::FieldPreservingWrite {
        value, owned_mask, ..
    } = &plan.steps[0]
    {
        assert_eq!(value, &vec![1 << 3]);
        assert_eq!(owned_mask, &(1 << 3));
    }
    // An unusable policy refuses: zero interval, zero backstop, interval
    // beyond the backstop.
    for bad in [
        ResetPolicy {
            poll_interval: MonoDuration(0),
            backstop: MonoDuration(10_000),
        },
        ResetPolicy {
            poll_interval: MonoDuration(50),
            backstop: MonoDuration(0),
        },
        ResetPolicy {
            poll_interval: MonoDuration(20_000),
            backstop: MonoDuration(10_000),
        },
    ] {
        assert_eq!(
            plan_reset(inputs(ResetRequest::Soft, bad)),
            ResetPlanDecision::Refused(ResetRefusal::InvalidPolicy)
        );
    }
}

#[test]
fn protection_yields_a_qualified_plan_or_a_typed_unsupported_never_an_invented_route() {
    let request = ProtectionRequest { high_alarm: true };
    let inputs = |request: ProtectionRequest, route: ProtectionRoute, current: Option<u8>| {
        ProtectionInputs {
            request,
            route,
            facts: facts(current),
        }
    };
    // A qualified software route plans the byte-26 force.
    assert!(matches!(
        plan_protection(inputs(
            request,
            ProtectionRoute::SoftwareLowPower,
            Some(0x00)
        )),
        ProtectionDecision::Plan(_)
    ));
    // A threshold-decided alarm (the donor field would not act) still may
    // yield the qualified plan: the distinction is preserved, not lost.
    let threshold = ProtectionRequest { high_alarm: false };
    assert!(matches!(
        plan_protection(inputs(
            threshold,
            ProtectionRoute::SoftwareLowPower,
            Some(0x00)
        )),
        ProtectionDecision::Plan(_)
    ));
    // No qualified route: typed unsupported, the trigger preserved.
    assert_eq!(
        plan_protection(inputs(
            request,
            ProtectionRoute::NoQualifiedRoute,
            Some(0x00)
        )),
        ProtectionDecision::Unsupported(ProtectionUnsupported { request })
    );
    // Even a qualified route with missing RMW facts never invents an
    // effect: the trigger records as unenforced.
    assert_eq!(
        plan_protection(inputs(request, ProtectionRoute::SoftwareLowPower, None)),
        ProtectionDecision::Unsupported(ProtectionUnsupported { request })
    );
}

#[test]
fn a_planned_write_invalidates_exactly_its_covered_lines() {
    let key = |page: u8| CacheKey {
        address: 0x50,
        bank: 0,
        page,
        half: 0,
    };
    let line = |page: u8| CacheLine {
        key: key(page),
        valid_from: 0,
        valid_len: 128,
        epoch: EPOCH,
        pinned: false,
    };
    let lines = [line(0), line(0x11)];
    let written = openxcvr_core::types::management::Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 26,
        len: 1,
    };
    // Only the written page's line is invalidated; the other page's line
    // survives (targeted invalidation, never a blanket one).
    assert_eq!(write_invalidations(&lines, &written), vec![key(0)]);
    // A write on another page touches neither.
    let elsewhere = openxcvr_core::types::management::Range {
        page: 0x01,
        ..written
    };
    assert!(write_invalidations(&lines, &elsewhere).is_empty());
}

// ── The operation driver over the plan steps (task 64) ─────────────────────
//
// The donor sysfs request/commit/rollback sequence as a pure driver: the
// caller executes the plan's steps in order and reports each outcome with
// its live generation; a moved generation supersedes every path, failure
// under the owning generation rolls back (restoring the supplied
// pre-write bytes), and an uncertain outcome is never replayed or rolled
// back.

fn force_plan_under(identity: OperationId, current: u8) -> (OperationState, OperationPlan) {
    let PowerPlanDecision::Plan(plan) = power_request_step(
        PowerState::Idle,
        PowerRequest::ForceLowPower,
        facts_at(identity, EPOCH, Some(current)),
    ) else {
        panic!("the force plan must be plannable over fresh excluded facts")
    };
    // Request the plan from idle: the donor sets its knob and bumps its
    // generation before any hardware effect (the optimistic request).
    let state = OperationState::Requested {
        plan: plan.clone(),
        cursor: 0,
    };
    (state, plan)
}

fn force_plan() -> (OperationState, OperationPlan) {
    force_plan_under(OP, 0xA0)
}

#[test]
fn a_fully_confirmed_plan_commits_under_the_matching_generation() {
    let (state, plan) = force_plan();
    assert_eq!(plan.steps.len(), 3);
    // Each confirmed step advances the cursor and returns the next step
    // in order; the last confirmation commits. The epoch in effect for a
    // step is the plan's stamp until the epoch-advance step runs: the
    // read and the write confirm under EPOCH, and the AdvanceCacheEpoch
    // step itself is confirmed under the epoch it advanced to.
    let expected_epochs = [EPOCH, EPOCH, CacheEpoch(EPOCH.0 + 1)];
    let mut state = state;
    for (expected, epoch) in expected_epochs.iter().enumerate() {
        let (next, decision) = operation_step(
            state,
            OperationEvent::Confirmed {
                live_generation: GEN,
                operation: plan.identity,
                step: expected,
                epoch: *epoch,
            },
        );
        state = next;
        if expected + 1 == plan.steps.len() {
            assert_eq!(decision, OperationDecision::Committed);
            assert_eq!(
                state,
                OperationState::Committed {
                    generation: plan.generation
                }
            );
        } else {
            assert_eq!(
                decision,
                OperationDecision::Continue(plan.steps[expected + 1].clone())
            );
        }
    }
    // Anything after the commit is stale and changes nothing.
    let (after, decision) = operation_step(
        state.clone(),
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan.identity,
            step: 2,
            epoch: CacheEpoch(EPOCH.0 + 1),
        },
    );
    assert_eq!(decision, OperationDecision::StaleResult);
    assert_eq!(after, state);
}

#[test]
fn a_failed_write_rolls_back_restoring_the_supplied_pre_write_bytes() {
    let (state, plan) = force_plan();
    // Step 0 (the prerequisite read) confirms; the write fails.
    let (state, _) = operation_step(
        state,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan.identity,
            step: 0,
            epoch: EPOCH,
        },
    );
    let original = [0xA0u8];
    let (state, decision) = operation_step(
        state,
        OperationEvent::Failed {
            live_generation: GEN,
            operation: plan.identity,
            step: 1,
            epoch: EPOCH,
            original: Some(&original),
        },
    );
    let OperationDecision::Rollback {
        restore:
            Some(OperationStep::FieldPreservingWrite {
                range,
                value,
                owned_mask,
            }),
    } = decision
    else {
        panic!("a failed write under the owning generation must roll back: {decision:?}")
    };
    // The compensating write restores exactly the supplied pre-write byte
    // over the same range and owned mask — the driver invents no bytes.
    let OperationStep::FieldPreservingWrite {
        range: written_range,
        value: written_value,
        owned_mask: written_mask,
    } = &plan.steps[1]
    else {
        panic!("step 1 of the force plan is the field-preserving write")
    };
    assert_eq!(range, *written_range);
    assert_eq!(owned_mask, *written_mask);
    // The restore byte is the pre-write byte, not the planned new byte.
    assert_ne!(value, written_value.clone());
    assert_eq!(value, vec![0xA0]);
    assert_eq!(owned_mask, cmis_b26().owned_mask());
    assert_eq!(
        state,
        OperationState::RolledBack {
            generation: plan.generation
        }
    );
    // Terminal: later results are stale.
    let (state2, decision) = operation_step(
        state.clone(),
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan.identity,
            step: 1,
            epoch: EPOCH,
        },
    );
    assert_eq!(decision, OperationDecision::StaleResult);
    assert_eq!(state2, state);
}

#[test]
fn a_failure_without_supplied_bytes_or_with_a_mismatched_length_never_invents_a_restore() {
    let (state, _) = force_plan();
    let (state, _) = operation_step(
        state,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: OP,
            step: 0,
            epoch: EPOCH,
        },
    );
    for original in [None, Some(&[0xA0u8, 0x00][..])] {
        let (next, decision) = operation_step(
            state.clone(),
            OperationEvent::Failed {
                live_generation: GEN,
                operation: OP,
                step: 1,
                epoch: EPOCH,
                original,
            },
        );
        assert_eq!(
            decision,
            OperationDecision::Rollback { restore: None },
            "no compensating write may be invented from {original:?}"
        );
        assert!(matches!(next, OperationState::RolledBack { .. }));
    }
}

#[test]
fn a_moved_generation_supersedes_every_path_and_never_rolls_back() {
    let (state, plan) = force_plan();
    let newer = ModuleGeneration(8);
    // A failure after the caller's generation moved: no rollback, no
    // compensating write — the newer request owns the state. The identity,
    // step and epoch otherwise match, so it is the generation that
    // supersedes.
    let (next, decision) = operation_step(
        state,
        OperationEvent::Failed {
            live_generation: newer,
            operation: plan.identity,
            step: 0,
            epoch: EPOCH,
            original: Some(&[0xA0]),
        },
    );
    assert_eq!(decision, OperationDecision::Superseded);
    assert_eq!(
        next,
        OperationState::Superseded {
            generation: plan.generation
        }
    );
    // A confirmation under the moved generation never commits either.
    let (state, _) = force_plan();
    let (next, decision) = operation_step(
        state,
        OperationEvent::Confirmed {
            live_generation: newer,
            operation: OP,
            step: 0,
            epoch: EPOCH,
        },
    );
    assert_eq!(decision, OperationDecision::Superseded);
    assert!(matches!(next, OperationState::Superseded { .. }));
}

#[test]
fn an_uncertain_step_stays_pending_and_is_never_replayed_or_rolled_back() {
    let (state, plan) = force_plan();
    let (next, decision) = operation_step(
        state.clone(),
        OperationEvent::Uncertain {
            live_generation: GEN,
            operation: OP,
            step: 0,
            epoch: EPOCH,
        },
    );
    assert_eq!(decision, OperationDecision::UncertainPending);
    // The operation stays pending at the same step, byte-for-byte.
    assert_eq!(next, state);
    // Once the caller confirms, execution continues in order.
    let (next, decision) = operation_step(
        next,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan.identity,
            step: 0,
            epoch: EPOCH,
        },
    );
    assert_eq!(decision, OperationDecision::Continue(plan.steps[1].clone()));
    assert!(matches!(next, OperationState::Requested { cursor: 1, .. }));
}

#[test]
fn results_with_nothing_in_flight_or_past_the_last_step_are_stale() {
    for state in [
        OperationState::Idle,
        OperationState::Committed {
            generation: ModuleGeneration(7),
        },
        OperationState::RolledBack {
            generation: ModuleGeneration(7),
        },
        OperationState::Superseded {
            generation: ModuleGeneration(7),
        },
    ] {
        let (next, decision) = operation_step(
            state.clone(),
            OperationEvent::Confirmed {
                live_generation: GEN,
                operation: OP,
                step: 0,
                epoch: EPOCH,
            },
        );
        assert_eq!(decision, OperationDecision::StaleResult);
        assert_eq!(next, state);
    }
}

// The identity gate (fail closed): a result that names another
// operation, another step, or a cache epoch other than the one in effect
// for the cursor's step is refused with the typed mismatch and changes
// nothing — the plan stays pending at its cursor.

#[test]
fn a_late_result_from_another_operation_never_advances_or_commits_the_current_plan() {
    // The exact counterexample: plan A (operation 100, a force over
    // current byte 0xA0) runs at generation 7 / epoch 9 and confirms its
    // first step. The caller then issues plan B (operation 200, a force
    // over current byte 0x00 — a different write) at the SAME module
    // generation and epoch. A's late Confirmed results name A: each must
    // be refused with the typed operation mismatch, B's cursor must never
    // move, and replaying a full plan's worth of them must never commit B
    // without B's own steps completing (the every-step-confirmed
    // contract).
    let (state_a, plan_a) = force_plan_under(OperationId(100), 0xA0);
    let (state_a, decision) = operation_step(
        state_a,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan_a.identity,
            step: 0,
            epoch: EPOCH,
        },
    );
    assert!(matches!(decision, OperationDecision::Continue(_)));
    assert!(matches!(
        state_a,
        OperationState::Requested { cursor: 1, .. }
    ));
    let (state_b, plan_b) = force_plan_under(OperationId(200), 0x00);
    for step in 0..plan_b.steps.len() {
        let (next, decision) = operation_step(
            state_b.clone(),
            OperationEvent::Confirmed {
                live_generation: GEN,
                operation: OperationId(100),
                step,
                epoch: EPOCH,
            },
        );
        assert_eq!(
            decision,
            OperationDecision::RefusedResult(OperationEventRefusal::Operation {
                event: OperationId(100),
                plan: OperationId(200),
            }),
            "a late confirmation from operation A must be refused at step {step}"
        );
        assert_eq!(
            next, state_b,
            "the refusal must change no state at step {step}"
        );
    }
    // Control: the correctly-stamped result still advances B — the gate
    // refuses foreign results, it does not stall the plan's own.
    let (next, decision) = operation_step(
        state_b,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: OperationId(200),
            step: 0,
            epoch: EPOCH,
        },
    );
    assert!(matches!(decision, OperationDecision::Continue(_)));
    assert!(matches!(next, OperationState::Requested { cursor: 1, .. }));
}

#[test]
fn a_result_for_the_wrong_step_of_the_current_operation_never_advances() {
    // The same hole one layer in: the event names the right operation but
    // a step other than the cursor's — a late, duplicated or out-of-order
    // step result. The duplicate of step 0 must not advance the cursor
    // again, and a result for a later step must not skip ahead.
    let (state, plan) = force_plan();
    let (state, _) = operation_step(
        state,
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: plan.identity,
            step: 0,
            epoch: EPOCH,
        },
    );
    for wrong_step in [0usize, 2] {
        let (next, decision) = operation_step(
            state.clone(),
            OperationEvent::Confirmed {
                live_generation: GEN,
                operation: OP,
                step: wrong_step,
                epoch: EPOCH,
            },
        );
        assert_eq!(
            decision,
            OperationDecision::RefusedResult(OperationEventRefusal::Step {
                event: wrong_step,
                cursor: 1,
            }),
            "a result for step {wrong_step} must be refused at cursor 1"
        );
        assert_eq!(next, state);
    }
}

#[test]
fn a_result_observed_under_a_moved_epoch_never_counts_for_the_current_step() {
    // The epoch half of the counterexample: plan B is stamped epoch 10
    // (the cache epoch advanced after A's write); a result observed under
    // the old epoch 9 must be refused even when it names B and the right
    // step. Fail closed: the caller re-drives the step or re-plans.
    let PowerPlanDecision::Plan(plan_b) = power_request_step(
        PowerState::Idle,
        PowerRequest::ForceLowPower,
        facts_at(OperationId(300), CacheEpoch(EPOCH.0 + 1), Some(0x00)),
    ) else {
        panic!("plan B must be plannable over fresh excluded epoch-10 facts")
    };
    let state = OperationState::Requested {
        plan: plan_b,
        cursor: 0,
    };
    let (next, decision) = operation_step(
        state.clone(),
        OperationEvent::Confirmed {
            live_generation: GEN,
            operation: OperationId(300),
            step: 0,
            epoch: EPOCH,
        },
    );
    assert_eq!(
        decision,
        OperationDecision::RefusedResult(OperationEventRefusal::Epoch {
            event: EPOCH,
            expected: CacheEpoch(EPOCH.0 + 1),
        })
    );
    assert_eq!(next, state);
}
