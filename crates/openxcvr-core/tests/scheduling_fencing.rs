// Behaviour pins for task 63 (`types::management::scheduling`): the FIFO
// lane with stale/deadline retirement and selector-unconfirmed deferral,
// the donor's lexicographic priority/page-affinity/FIFO ranking with both
// anti-starvation gates, exclusive-session tokens, the request life
// cycle, the presence-probe governor and parent health admission.
use openxcvr_core::types::management::scheduling::*;
use openxcvr_core::types::management::{
    CacheEpoch, EventSequence, ModuleGeneration, MonoDuration, MonoTime, OperationId, Selection,
};

const GEN: ModuleGeneration = ModuleGeneration(7);
const EPOCH: CacheEpoch = CacheEpoch(9);
const NOW: MonoTime = MonoTime(1_000);

fn operation(id: u64) -> OperationId {
    OperationId(id)
}

fn selection(bank: u8, page: u8) -> Selection {
    Selection {
        address: 0x50,
        bank,
        page,
    }
}

/// A fresh, untargeted, undated request with the given FIFO sequence.
fn request(operation_id: u64, sequence: u64) -> RequestMeta {
    RequestMeta {
        operation: operation(operation_id),
        sequence: EventSequence(sequence),
        generation: GEN,
        epoch: EPOCH,
        target: None,
        deadline: None,
    }
}

fn knowledge() -> SelectorKnowledge {
    SelectorKnowledge {
        generation: GEN,
        epoch: EPOCH,
        selection: None,
    }
}

fn policy() -> SchedulingPolicy {
    SchedulingPolicy::default()
}

fn choose(
    requests: &[RequestMeta],
    knowledge: SelectorKnowledge,
) -> (ScheduleDecision, SchedulerState) {
    choose_at(requests, knowledge, NOW)
}

fn choose_at(
    requests: &[RequestMeta],
    knowledge: SelectorKnowledge,
    now: MonoTime,
) -> (ScheduleDecision, SchedulerState) {
    choose_next(
        requests,
        &SchedulerState::default(),
        &knowledge,
        now,
        &policy(),
    )
}

#[test]
fn oldest_sequence_runs_first() {
    let requests = [request(3, 30), request(1, 10), request(2, 20)];
    let (decision, _state) = choose(&requests, knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
}

// The deadline-bearing request is the oldest one examined, so the boundary
// is actually reached: an off-by-one retirement rule (`now >= deadline`)
// retires this request instead of running it.
#[test]
fn a_fresh_request_at_its_deadline_is_not_retired() {
    let mut oldest = request(1, 10);
    oldest.deadline = Some(NOW);
    let (decision, _state) = choose(&[oldest, request(2, 20)], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
}

// The companion one tick past the same boundary: only the request's `now`
// moved, and the retirement fires.
#[test]
fn a_request_one_tick_past_its_deadline_retires_with_a_typed_deadline_terminal() {
    let mut at_deadline = request(1, 10);
    at_deadline.deadline = Some(NOW);
    let (decision, _state) = choose_at(&[at_deadline], knowledge(), MonoTime(NOW.0 + 1));
    assert_eq!(
        decision,
        ScheduleDecision::Retire {
            operation: operation(1),
            terminal: ScheduleTerminal::DeadlinePassed {
                deadline: NOW,
                at: MonoTime(NOW.0 + 1),
            },
        }
    );
}

#[test]
fn a_stale_generation_stamp_retires_with_a_typed_stale_terminal() {
    let mut stale = request(1, 10);
    stale.generation = ModuleGeneration(GEN.0 + 1);
    let (decision, _state) = choose(&[stale], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Retire {
            operation: operation(1),
            terminal: ScheduleTerminal::Stale {
                generation: ModuleGeneration(GEN.0 + 1),
                epoch: EPOCH,
            },
        }
    );
}

#[test]
fn a_stale_epoch_stamp_retires_with_a_typed_stale_terminal() {
    let mut stale = request(1, 10);
    stale.epoch = CacheEpoch(EPOCH.0 + 1);
    let (decision, _state) = choose(&[stale], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Retire {
            operation: operation(1),
            terminal: ScheduleTerminal::Stale {
                generation: GEN,
                epoch: CacheEpoch(EPOCH.0 + 1),
            },
        }
    );
}

#[test]
fn an_older_stale_request_retires_before_a_newer_runnable_one() {
    let mut stale = request(1, 10);
    stale.generation = ModuleGeneration(GEN.0 + 1);
    let requests = [stale, request(2, 20)];
    let (decision, _state) = choose(&requests, knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Retire {
            operation: operation(1),
            terminal: ScheduleTerminal::Stale {
                generation: ModuleGeneration(GEN.0 + 1),
                epoch: EPOCH,
            },
        }
    );
}

#[test]
fn a_past_deadline_retires_with_a_typed_deadline_terminal() {
    let mut expired = request(1, 10);
    expired.deadline = Some(MonoTime(NOW.0 - 1));
    let (decision, _state) = choose(&[expired], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Retire {
            operation: operation(1),
            terminal: ScheduleTerminal::DeadlinePassed {
                deadline: MonoTime(NOW.0 - 1),
                at: NOW,
            },
        }
    );
}

#[test]
fn an_unconfirmed_target_selection_is_deferred_never_run() {
    let mut paged = request(1, 10);
    paged.target = Some(selection(0, 1));
    let (decision, _state) = choose(&[paged], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Defer {
            operation: operation(1),
            reason: Deferral::SelectorUnconfirmed {
                target: selection(0, 1),
                confirmed: None,
            },
        }
    );
}

#[test]
fn a_deferred_request_does_not_block_a_newer_runnable_one() {
    let mut paged = request(1, 10);
    paged.target = Some(selection(2, 3));
    let requests = [paged, request(2, 20)];
    let (decision, _state) = choose(&requests, knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(2)
        }
    );
}

#[test]
fn a_target_selection_confirmed_by_selector_knowledge_runs() {
    let mut paged = request(1, 10);
    paged.target = Some(selection(2, 3));
    let knowledge = SelectorKnowledge {
        generation: GEN,
        epoch: EPOCH,
        selection: Some(selection(2, 3)),
    };
    let (decision, _state) = choose(&[paged], knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
}

#[test]
fn no_requests_choose_idle() {
    let (decision, _state) = choose(&[], knowledge());
    assert_eq!(decision, ScheduleDecision::Idle);
}

#[test]
fn only_deferred_requests_choose_the_oldest_deferral() {
    let mut older = request(1, 10);
    older.target = Some(selection(0, 1));
    let mut newer = request(2, 20);
    newer.target = Some(selection(0, 2));
    let (decision, _state) = choose(&[older, newer], knowledge());
    assert_eq!(
        decision,
        ScheduleDecision::Defer {
            operation: operation(1),
            reason: Deferral::SelectorUnconfirmed {
                target: selection(0, 1),
                confirmed: None,
            },
        }
    );
}

// ─── Task 63 remainder: page affinity, both fairness gates ─────────────────
//
// The donor's lexicographic selection (sched.c:296-373): KEY1 priority
// class dominates KEY2 page affinity, FIFO breaks ties; both
// anti-starvation gates (sched.c:1218-1235) evaluated together, KEY1
// dominating the both-armed corner case.

fn prioritized(
    operation_id: u64,
    sequence: u64,
    priority: RequestPriority,
) -> PrioritizedRequestMeta {
    PrioritizedRequestMeta {
        meta: request(operation_id, sequence),
        priority,
    }
}

fn ranked_at(
    requests: &[PrioritizedRequestMeta],
    previous: &SchedulerState,
    knowledge: SelectorKnowledge,
) -> (ScheduleDecision, SchedulerState) {
    choose_next_ranked(requests, previous, &knowledge, NOW, &policy())
}

fn page_confirmed(bank: u8, page: u8) -> SelectorKnowledge {
    SelectorKnowledge {
        generation: GEN,
        epoch: EPOCH,
        selection: Some(selection(bank, page)),
    }
}

#[test]
fn page_affinity_ranks_before_fifo_within_a_priority_class() {
    // Both runnable: the same-page upper-memory request beats an OLDER
    // lower-memory request while bank 1 page 0x11 is confirmed (the
    // donor's KEY2: a lower-memory request addresses the lower page,
    // bank 0 / page 0, and is different-page while another page is
    // selected).
    let mut on_page = prioritized(1, 20, RequestPriority::High);
    on_page.meta.target = Some(selection(1, 0x11));
    let lower = prioritized(2, 10, RequestPriority::High);
    let knowledge = page_confirmed(1, 0x11);
    let (decision, _) = ranked_at(&[on_page, lower], &SchedulerState::default(), knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
}

#[test]
fn priority_ranks_before_page_affinity() {
    // The older normal same-page request loses to the newer high-priority
    // different-page runnable request: KEY1 dominates KEY2.
    let mut on_page = prioritized(1, 10, RequestPriority::Normal);
    on_page.meta.target = Some(selection(1, 0x11));
    let lower = prioritized(2, 20, RequestPriority::High);
    let knowledge = page_confirmed(1, 0x11);
    let (decision, _) = ranked_at(&[on_page, lower], &SchedulerState::default(), knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(2)
        }
    );
}

#[test]
fn an_unknown_selection_establishes_no_affinity_for_anyone() {
    // With no confirmed selection nothing is same-page (the donor's
    // poisoned selector matches nothing), so same-class ranking falls
    // back to FIFO: the older lower-memory request runs first.
    let knowledge = knowledge();
    let (decision, _) = ranked_at(
        &[
            prioritized(1, 10, RequestPriority::Normal),
            prioritized(2, 20, RequestPriority::Normal),
        ],
        &SchedulerState::default(),
        knowledge,
    );
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
}

#[test]
fn the_page_gate_serves_a_runnable_different_page_request_first() {
    // Gate 1 armed (page streak at the cap, a different-page waiter
    // exists): KEY2 inverts, so the older lower-memory runnable request
    // outranks the same-page upper-memory request within the class. Like
    // the donor's skip_select request, this service performs no page
    // switch, so the page streak advances rather than resetting.
    let mut on_page = prioritized(1, 20, RequestPriority::Normal);
    on_page.meta.target = Some(selection(1, 0x11));
    let lower = prioritized(2, 10, RequestPriority::Normal);
    let knowledge = page_confirmed(1, 0x11);
    let at_cap = SchedulerState {
        page_affinity_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
        ..SchedulerState::default()
    };
    let (decision, state) = ranked_at(&[on_page, lower], &at_cap, knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(2)
        }
    );
    assert_eq!(
        state.page_affinity_streak,
        SchedulingPolicy::DONOR_ANTI_STARVE_CAP + 1
    );
}

#[test]
fn the_page_gate_surfaces_the_owed_page_switch_and_resets_the_streak() {
    // Gate 1 armed but the only different-page waiter is selector
    // unconfirmed (it needs the owner to confirm bank 2): the chooser
    // surfaces that deferral — this model's page switch — instead of
    // serving same-page work forever, and resets the page streak (the
    // donor resets at its executed switch).
    let mut on_page = prioritized(1, 10, RequestPriority::High);
    on_page.meta.target = Some(selection(1, 0x11));
    let mut elsewhere = prioritized(2, 20, RequestPriority::High);
    elsewhere.meta.target = Some(selection(2, 0x11));
    let knowledge = page_confirmed(1, 0x11);
    let at_cap = SchedulerState {
        page_affinity_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
        ..SchedulerState::default()
    };
    let (decision, state) = ranked_at(&[on_page, elsewhere], &at_cap, knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Defer {
            operation: operation(2),
            reason: Deferral::SelectorUnconfirmed {
                target: selection(2, 0x11),
                confirmed: Some(selection(1, 0x11)),
            }
        }
    );
    assert_eq!(state.page_affinity_streak, 0);
}

#[test]
fn the_page_gate_stays_dark_below_the_cap() {
    // The same queue one tick below the cap: affinity ranks normally and
    // the unconfirmed request never surfaces.
    let mut on_page = prioritized(1, 10, RequestPriority::High);
    on_page.meta.target = Some(selection(1, 0x11));
    let mut elsewhere = prioritized(2, 20, RequestPriority::High);
    elsewhere.meta.target = Some(selection(2, 0x11));
    let knowledge = page_confirmed(1, 0x11);
    let below_cap = SchedulerState {
        page_affinity_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP - 1,
        ..SchedulerState::default()
    };
    let (decision, state) = ranked_at(&[on_page, elsewhere], &below_cap, knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
    // The Run advanced the no-switch streak to the cap; the gate arms for
    // the NEXT decision, exactly as the donor's post-service evaluation.
    assert_eq!(
        state.page_affinity_streak,
        SchedulingPolicy::DONOR_ANTI_STARVE_CAP
    );
}

#[test]
fn both_gates_armed_the_priority_inversion_wins_and_defers_the_page_yield() {
    // The donor's documented corner case (sched.c:283-290): both gates
    // fire, no request satisfies both — the runnable normal request is
    // same-page, the different-page waiter is high-priority and
    // unconfirmed. KEY1 dominates: the inversion runs the normal request
    // this decision (clearing the priority streak), and the page yield is
    // deferred exactly one decision (the page streak stays armed).
    let mut on_page_normal = prioritized(1, 10, RequestPriority::Normal);
    on_page_normal.meta.target = Some(selection(1, 0x11));
    let mut elsewhere_high = prioritized(2, 20, RequestPriority::High);
    elsewhere_high.meta.target = Some(selection(2, 0x11));
    let knowledge = page_confirmed(1, 0x11);
    let both_armed = SchedulerState {
        high_priority_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
        page_affinity_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
    };
    let (decision, state) = ranked_at(&[on_page_normal, elsewhere_high], &both_armed, knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Run {
            operation: operation(1)
        }
    );
    assert_eq!(state.high_priority_streak, 0);
    assert!(state.page_affinity_streak > SchedulingPolicy::DONOR_ANTI_STARVE_CAP);
    // The next decision: the priority gate has cleared, the page gate is
    // still armed — the owed page switch surfaces now. Bounded, exactly
    // one decision late.
    let mut on_page_normal_again = on_page_normal;
    on_page_normal_again.meta.sequence = EventSequence(30);
    let (decision, _) = ranked_at(&[on_page_normal_again, elsewhere_high], &state, knowledge);
    assert_eq!(
        decision,
        ScheduleDecision::Defer {
            operation: operation(2),
            reason: Deferral::SelectorUnconfirmed {
                target: selection(2, 0x11),
                confirmed: Some(selection(1, 0x11)),
            }
        }
    );
}

// ─── Task 63 remainder: exclusive sessions ──────────────────────────────────

#[test]
fn exclusive_begin_mints_a_fresh_nonzero_token_and_denies_a_concurrent_begin() {
    let state = ExclusiveState::default();
    let (state, began) = exclusive_step(
        state,
        ExclusiveEvent::Begin {
            owner: operation(5),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token } = began else {
        panic!("first begin must mint: {began:?}")
    };
    assert_ne!(token, ExclusiveToken::NONE);
    assert!(token.is_minted());
    // A second begin while the session is live is denied, state unchanged.
    let (state2, denied) = exclusive_step(
        state,
        ExclusiveEvent::Begin {
            owner: operation(6),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    assert_eq!(denied, ExclusiveOutcome::BeginDenied);
    assert_eq!(state2, state);
    // After a clean end, the next begin mints a DIFFERENT token.
    let (state, ended) = exclusive_step(state, ExclusiveEvent::End { token }, GEN, EPOCH);
    assert_eq!(ended, ExclusiveOutcome::Ended);
    let (_, began_again) = exclusive_step(
        state,
        ExclusiveEvent::Begin {
            owner: operation(6),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token: next } = began_again else {
        panic!("re-begin must mint: {began_again:?}")
    };
    assert_ne!(next, token);
}

#[test]
fn a_stale_end_cannot_affect_a_newer_token() {
    let (state, first) = exclusive_step(
        ExclusiveState::default(),
        ExclusiveEvent::Begin {
            owner: operation(1),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token: old } = first else {
        panic!()
    };
    let (state, _) = exclusive_step(state, ExclusiveEvent::End { token: old }, GEN, EPOCH);
    let (state, second) = exclusive_step(
        state,
        ExclusiveEvent::Begin {
            owner: operation(2),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token: live } = second else {
        panic!()
    };
    // The retired token's end is ignored: the live session survives.
    let (state_after, outcome) =
        exclusive_step(state, ExclusiveEvent::End { token: old }, GEN, EPOCH);
    assert_eq!(outcome, ExclusiveOutcome::EndIgnored { offered: old });
    assert_eq!(state_after, state);
    // A zero token is never a session's token.
    let (_, outcome) = exclusive_step(
        state,
        ExclusiveEvent::End {
            token: ExclusiveToken::NONE,
        },
        GEN,
        EPOCH,
    );
    assert_eq!(
        outcome,
        ExclusiveOutcome::EndIgnored {
            offered: ExclusiveToken::NONE
        }
    );
    assert_ne!(live, ExclusiveToken::NONE);
}

#[test]
fn enqueue_admission_is_atomic_and_refuses_stale_tokens_and_sessions() {
    let (state, began) = exclusive_step(
        ExclusiveState::default(),
        ExclusiveEvent::Begin {
            owner: operation(1),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token } = began else {
        panic!()
    };
    // Matching token, governing stamp: admitted.
    let req = request(9, 1);
    let (_, admitted) = exclusive_step(
        state,
        ExclusiveEvent::Enqueue {
            token,
            request: req,
        },
        GEN,
        EPOCH,
    );
    assert_eq!(admitted, ExclusiveOutcome::EnqueueAdmitted);
    // A foreign token is refused with the live token named.
    let (_, refused) = exclusive_step(
        state,
        ExclusiveEvent::Enqueue {
            token: ExclusiveToken(999),
            request: req,
        },
        GEN,
        EPOCH,
    );
    assert_eq!(
        refused,
        ExclusiveOutcome::EnqueueRefused {
            reason: ExclusiveRefusal::StaleToken {
                offered: ExclusiveToken(999),
                live: token,
            }
        }
    );
    // A matching token under a replaced generation fences the session.
    let (_, fenced) = exclusive_step(
        state,
        ExclusiveEvent::Enqueue {
            token,
            request: req,
        },
        ModuleGeneration(GEN.0 + 1),
        EPOCH,
    );
    assert_eq!(
        fenced,
        ExclusiveOutcome::EnqueueRefused {
            reason: ExclusiveRefusal::StaleSession {
                session_generation: GEN,
                session_epoch: EPOCH,
            }
        }
    );
    // No session at all.
    let (_, none) = exclusive_step(
        ExclusiveState::default(),
        ExclusiveEvent::Enqueue {
            token,
            request: req,
        },
        GEN,
        EPOCH,
    );
    assert_eq!(
        none,
        ExclusiveOutcome::EnqueueRefused {
            reason: ExclusiveRefusal::NotActive,
        }
    );
}

#[test]
fn invalidation_returns_explicit_recover_effects_not_a_silent_teardown() {
    let (state, began) = exclusive_step(
        ExclusiveState::default(),
        ExclusiveEvent::Begin {
            owner: operation(1),
            generation: GEN,
            epoch: EPOCH,
        },
        GEN,
        EPOCH,
    );
    let ExclusiveOutcome::Began { token } = began else {
        panic!()
    };
    let (state, outcome) = exclusive_step(state, ExclusiveEvent::Invalidate, GEN, EPOCH);
    let ExclusiveOutcome::Invalidated { effects } = outcome else {
        panic!("invalidation must return effects: {outcome:?}")
    };
    assert_eq!(
        effects,
        vec![
            ExclusiveEffect::RetireMemberRequests { token },
            ExclusiveEffect::FenceGeneration {
                generation: GEN,
                epoch: EPOCH,
            },
        ]
    );
    assert_eq!(state.session, None);
    // Invalidating with no session is a no-op with no effects.
    let (_, outcome) = exclusive_step(state, ExclusiveEvent::Invalidate, GEN, EPOCH);
    assert_eq!(
        outcome,
        ExclusiveOutcome::Invalidated {
            effects: Vec::new()
        }
    );
}

// ─── Task 63 remainder: request life ────────────────────────────────────────

#[test]
fn a_request_terminates_exactly_once() {
    use RequestLife as L;
    use RequestLifeEvent as E;
    // The legal walk: queued → running → completed.
    assert_eq!(
        request_step(L::Queued, E::Start),
        LifeStep::Moved(L::Running)
    );
    assert_eq!(
        request_step(L::Running, E::Finish),
        LifeStep::Moved(L::Completed)
    );
    // A second finisher loses; every event on a terminal is refused.
    assert_eq!(
        request_step(L::Completed, E::Finish),
        LifeStep::AlreadyTerminal { from: L::Completed }
    );
    assert_eq!(
        request_step(L::Drained, E::Start),
        LifeStep::AlreadyTerminal { from: L::Drained }
    );
    // A start of an abandoned request is illegal: the worker must drain.
    assert_eq!(
        request_step(L::CancelRequested, E::Start),
        LifeStep::Illegal {
            from: L::CancelRequested
        }
    );
}

#[test]
fn an_in_flight_write_cancellation_reports_pending_or_uncertain_application_never_rollback() {
    use RequestLife as L;
    use RequestLifeEvent as E;
    // Cancel mid-flight, then drain at the boundary: the write's
    // application is reported uncertain — there is no rolled-back value.
    assert_eq!(
        request_step(L::Running, E::CancelRequested),
        LifeStep::Moved(L::CancelRequested)
    );
    assert_eq!(
        request_step(
            L::CancelRequested,
            E::Drained {
                write_application: Some(WriteApplication::Uncertain),
            }
        ),
        LifeStep::Moved(L::Drained)
    );
    // A write drained before it started reports nothing applied.
    assert_eq!(
        request_step(
            L::Queued,
            E::Drained {
                write_application: Some(WriteApplication::NotStarted),
            }
        ),
        LifeStep::Moved(L::Drained)
    );
    // A read drains with no application report at all.
    assert_eq!(
        request_step(
            L::Running,
            E::Drained {
                write_application: None,
            }
        ),
        LifeStep::Moved(L::Drained)
    );
    // The worker finishing an abandoned write drains it (the donor's
    // cmpxchg loser records DRAINED).
    assert_eq!(
        request_step(L::CancelRequested, E::Finish),
        LifeStep::Moved(L::Drained)
    );
}

// ─── Task 63 remainder: governor and health admission ──────────────────────

#[test]
fn the_governor_refills_admits_and_defers_with_a_fifo_reservation() {
    let policy = GovernorPolicy {
        burst: 4,
        refill_per_tick: 1,
        tick: MonoDuration(10),
        backoff: MonoDuration(100),
    };
    let start = GovernorState {
        tokens: 4,
        last_refill: NOW,
    };
    // A non-queued caller with a full bucket takes the surplus: admit.
    let facts = GovernorFacts {
        governed: Some(true),
        queued_waiters: 0,
        position: None,
    };
    let (state, decision) = governor_step(start, GovernorRequest::Try, facts, NOW, policy);
    assert_eq!(decision, GovernorDecision::Admit);
    assert_eq!(state.tokens, 3);
    // Empty bucket, no waiters: defer with a queued reservation at the
    // tail and a retry time one refill tick out.
    let empty = GovernorState {
        tokens: 0,
        last_refill: NOW,
    };
    let (state, decision) = governor_step(empty, GovernorRequest::Try, facts, NOW, policy);
    let GovernorDecision::Defer {
        retry_at,
        queued,
        position,
    } = decision
    else {
        panic!("empty bucket must defer: {decision:?}")
    };
    assert!(queued);
    assert_eq!(position, Some(0));
    assert!(retry_at > NOW);
    assert_eq!(state.tokens, 0);
    // A queued caller at position 2 is admitted only once the fill covers
    // the two waiters ahead: tokens must exceed the position.
    let queued_facts = GovernorFacts {
        governed: Some(true),
        queued_waiters: 3,
        position: Some(2),
    };
    let (_, denied) = governor_step(
        GovernorState {
            tokens: 2,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        queued_facts,
        NOW,
        policy,
    );
    assert!(matches!(denied, GovernorDecision::Defer { .. }));
    let (_, admitted) = governor_step(
        GovernorState {
            tokens: 3,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        queued_facts,
        NOW,
        policy,
    );
    assert_eq!(admitted, GovernorDecision::Admit);
    // A surplus taker never takes a reserved token.
    let surplus_facts = GovernorFacts {
        governed: Some(true),
        queued_waiters: 2,
        position: None,
    };
    let (_, denied) = governor_step(
        GovernorState {
            tokens: 2,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        surplus_facts,
        NOW,
        policy,
    );
    assert!(matches!(denied, GovernorDecision::Defer { .. }));
    let (_, admitted) = governor_step(
        GovernorState {
            tokens: 3,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        surplus_facts,
        NOW,
        policy,
    );
    assert_eq!(admitted, GovernorDecision::Admit);
    // Refill is capped at the burst.
    let (_, refill) = governor_step(
        GovernorState {
            tokens: 0,
            last_refill: MonoTime(NOW.0 - 1000),
        },
        GovernorRequest::Try,
        facts,
        NOW,
        policy,
    );
    assert!(matches!(refill, GovernorDecision::Admit));
}

#[test]
fn an_unknown_topology_defers_and_an_ungoverned_parent_admits_without_a_token() {
    let policy = GovernorPolicy::DONOR_DEFAULT;
    let facts = GovernorFacts {
        governed: None,
        queued_waiters: 0,
        position: None,
    };
    let (state, decision) = governor_step(
        GovernorState {
            tokens: 32,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        facts,
        NOW,
        policy,
    );
    // Unknown topology: defer WITHOUT queueing — the caller must resolve
    // its facts; a guessed admission is never returned.
    let GovernorDecision::Defer {
        queued, position, ..
    } = decision
    else {
        panic!("unknown topology must defer: {decision:?}")
    };
    assert!(!queued);
    assert_eq!(position, None);
    assert_eq!(state.tokens, 32);
    // Positively ungoverned: admission is unconstrained and consumes no
    // token (the donor's not-yet-bound cage adapter).
    let ungoverned = GovernorFacts {
        governed: Some(false),
        queued_waiters: 0,
        position: None,
    };
    let (state, decision) = governor_step(
        GovernorState {
            tokens: 0,
            last_refill: NOW,
        },
        GovernorRequest::Try,
        ungoverned,
        NOW,
        policy,
    );
    assert_eq!(decision, GovernorDecision::Admit);
    assert_eq!(state.tokens, 0);
    // A cancelled wait never consumes a token.
    let (state, decision) = governor_step(
        GovernorState {
            tokens: 5,
            last_refill: NOW,
        },
        GovernorRequest::CancelWait,
        GovernorFacts {
            governed: Some(true),
            queued_waiters: 1,
            position: Some(0),
        },
        NOW,
        policy,
    );
    assert_eq!(decision, GovernorDecision::Cancelled);
    assert_eq!(state.tokens, 5);
}

#[test]
fn health_admission_refuses_sick_dead_and_missing_facts_and_parks_fifo() {
    let healthy_free = HealthFacts {
        dead: false,
        state: Some(ParentHealth::Healthy),
        inflight: false,
        owner_generation: 4,
        waiters: &[],
    };
    // Free slot, empty queue: immediate admission with the next owner
    // generation.
    assert_eq!(
        health_admit(
            &healthy_free,
            HealthRequest {
                high_priority: false
            },
            NOW
        ),
        AdmissionDecision::Admit { generation: 5 }
    );
    // Occupied slot: park FIFO.
    let busy = HealthFacts {
        inflight: true,
        ..healthy_free
    };
    assert_eq!(
        health_admit(
            &busy,
            HealthRequest {
                high_priority: false
            },
            NOW
        ),
        AdmissionDecision::Wait { position: 0 }
    );
    // Queued waiters (even with a free slot): still park — FIFO order is
    // preserved against the direct-take race.
    let parked = HealthFacts {
        waiters: &[HealthWaiter {
            high_priority: false,
            npassed: 0,
            recovery: false,
        }],
        ..healthy_free
    };
    assert_eq!(
        health_admit(
            &parked,
            HealthRequest {
                high_priority: true
            },
            NOW
        ),
        AdmissionDecision::Wait { position: 1 }
    );
    // SICK: refused, never a guessed admission.
    let sick = HealthFacts {
        state: Some(ParentHealth::Sick),
        ..healthy_free
    };
    assert_eq!(
        health_admit(
            &sick,
            HealthRequest {
                high_priority: true
            },
            NOW
        ),
        AdmissionDecision::Refuse {
            reason: HealthRefusal::Sick
        }
    );
    // Missing facts: refused — never an invented healthy recovery.
    let unknown = HealthFacts {
        state: None,
        ..healthy_free
    };
    assert_eq!(
        health_admit(
            &unknown,
            HealthRequest {
                high_priority: true
            },
            NOW
        ),
        AdmissionDecision::Refuse {
            reason: HealthRefusal::FactsMissing
        }
    );
    // Dead object: refused.
    let dead = HealthFacts {
        dead: true,
        ..healthy_free
    };
    assert_eq!(
        health_admit(
            &dead,
            HealthRequest {
                high_priority: true
            },
            NOW
        ),
        AdmissionDecision::Refuse {
            reason: HealthRefusal::Dead
        }
    );
}

#[test]
fn the_grant_picks_the_recovery_lane_then_fifo_with_the_fair_bound() {
    fn facts(waiters: &[HealthWaiter]) -> HealthFacts<'_> {
        HealthFacts {
            dead: false,
            state: Some(ParentHealth::Healthy),
            inflight: true,
            owner_generation: 1,
            waiters,
        }
    }
    // The reserved recovery waiter always wins the next grant.
    let with_recovery = [
        HealthWaiter {
            high_priority: false,
            npassed: 0,
            recovery: false,
        },
        HealthWaiter {
            high_priority: false,
            npassed: 0,
            recovery: true,
        },
    ];
    let grant = health_grant(&facts(&with_recovery));
    assert_eq!(grant.grant, Some(1));
    assert!(grant.bypassed.is_empty());
    // FIFO head wins while below the bound, and a low-priority grant
    // counts against every parked high-priority waiter.
    let queue = [
        HealthWaiter {
            high_priority: false,
            npassed: 0,
            recovery: false,
        },
        HealthWaiter {
            high_priority: true,
            npassed: 0,
            recovery: false,
        },
        HealthWaiter {
            high_priority: true,
            npassed: 0,
            recovery: false,
        },
    ];
    let grant = health_grant(&facts(&queue));
    assert_eq!(grant.grant, Some(0));
    assert_eq!(grant.bypassed, vec![1, 2]);
    // At the bound the oldest parked high-priority waiter is boosted.
    let boosted = [
        HealthWaiter {
            high_priority: false,
            npassed: 0,
            recovery: false,
        },
        HealthWaiter {
            high_priority: true,
            npassed: HEALTH_FAIR_BOUND,
            recovery: false,
        },
    ];
    let grant = health_grant(&facts(&boosted));
    assert_eq!(grant.grant, Some(1));
    assert!(grant.bypassed.is_empty());
    // No waiters: the slot goes idle.
    let grant = health_grant(&facts(&[]));
    assert_eq!(grant.grant, None);
}
