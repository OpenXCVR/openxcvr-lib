//! Pure scheduling/exclusion/CDB/generation/admission rules for one
//! module instance (task 63). No type here owns a device, queue, timer,
//! clock, waiters, callback, lock, handle or executor.
//!
//! Delivered: FIFO ordering by sequence with stale-generation and deadline
//! retirement and selector-unconfirmed deferral ([`choose_next`]); the
//! donor's full lexicographic ranking — priority class, page affinity,
//! FIFO — with both anti-starvation gates ([`choose_next_ranked`]); vector
//! yield between eligible segments ([`vector_yield`]); the exclusive
//! session token lifecycle ([`exclusive_step`]); the request life cycle
//! with its terminate-once rule ([`request_step`]); the CDB transaction
//! lifecycle reducers ([`cdb_admit`], [`cdb_advance`], [`cdb_finish_once`],
//! [`poll_defer`]); the presence-probe rate governor
//! ([`governor_step`]); and parent health admission
//! ([`health_admit`], [`health_grant`]).
//!
//! Callers compare these decisions with their own traces. This module
//! does not read a fixture file.
use super::{
    generation_valid, CacheEpoch, EventSequence, ModuleGeneration, MonoDuration, MonoTime,
    OperationId, Selection,
};

/// One schedulable request's supplied metadata.
///
/// `sequence` is the request's FIFO position as the caller supplied it: the
/// oldest request is the one with the smallest sequence, regardless of the
/// slice's order. `target` is the upper-memory bank/page the request's
/// accepted plan addresses, when it addresses one at all — `None` means the
/// plan touches only lower memory, which no selector state gates. `deadline`
/// is the supplied time after which the request may no longer start, when the
/// requester named one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestMeta {
    /// The operation this request belongs to.
    pub operation: OperationId,
    /// FIFO position: smaller is older.
    pub sequence: EventSequence,
    /// The generation this request was stamped with.
    pub generation: ModuleGeneration,
    /// The cache epoch this request was stamped with.
    pub epoch: CacheEpoch,
    /// The upper-memory bank/page the plan addresses, when it addresses one.
    pub target: Option<Selection>,
    /// The time after which the request may no longer start, when it has one.
    pub deadline: Option<MonoTime>,
}

/// What the single selector authority currently knows about the module.
///
/// Supplied per call by the instance owner; the library advances nothing here.
/// `selection` is the bank/page the owner currently holds *confirmed*, when it
/// holds one — an unconfirmed selection is [`None`], never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorKnowledge {
    /// The generation currently governing the module.
    pub generation: ModuleGeneration,
    /// The cache epoch currently governing the module.
    pub epoch: CacheEpoch,
    /// The selector selection currently confirmed, when one is.
    pub selection: Option<Selection>,
}

/// The scheduling state one module's queue stores and passes back.
///
/// Two counters, both donor `struct xcvr_port` fields with their sched.c
/// semantics: the high-priority service streak driving the priority
/// anti-starvation gate (`port->high_prio_streak`) and the no-page-switch
/// service streak driving the page gate (`port->same_page_count`). Segment
/// cursors and every other task-63 state field are owed. The caller owns
/// these counters between calls and must apply the donor's resets the
/// chooser cannot see: a page switch the owner performed itself (including
/// confirming a selection this chooser surfaced) and a served multi-page
/// vector (sched.c:774-786 resets `same_page_count` after a vector because
/// it inherently spans pages) both reset [`Self::page_affinity_streak`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SchedulerState {
    /// Consecutive high-priority Run decisions served since the last
    /// normal-priority Run (donor `port->high_prio_streak`,
    /// the kernel module core/sched.c). Resets to 0 whenever a normal request
    /// runs (including through the inversion) and when no high-priority
    /// request is chosen.
    pub high_priority_streak: u32,
    /// Consecutive served requests that performed no page switch (donor
    /// `port->same_page_count`, sched.c:1029-1032: incremented after every
    /// successful no-switch service, reset by an actual switch and by a
    /// served vector). Drives the page anti-starvation gate exactly as the
    /// donor's gate 1 reads it.
    pub page_affinity_streak: u32,
}

/// The supplied policy knobs the delivered rules consume.
///
/// The delivered subset consumes one knob: the anti-starvation cap.
/// Priority weights, yield gates and every other task-63 policy knob are
/// owed. The parameter type stays the accepted seam for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulingPolicy {
    /// After this many consecutive high-priority services, a runnable
    /// normal-priority request outranks High for one decision (the donor's
    /// normal-priority inversion). `0` inverts on every decision.
    pub anti_starvation_cap: u32,
}

impl SchedulingPolicy {
    /// The donor's anti-starvation cap, with its provenance: the kernel module
    /// core/sched.c:27 `#define XCVR_ANTI_STARVE_CAP 16`, driving BOTH gates
    /// the donor evaluates together after each service (sched.c:1218-1235):
    /// gate 1 (`force_different`, too many consecutive no-page-switch
    /// services with a different-page waiter) and gate 2 (`force_normal`,
    /// too many consecutive high-priority services with a normal waiter —
    /// sched.c:1420-1432: "too many consecutive high_prio reqs with a normal
    /// waiter → demote high_prio for one iteration so bulk/cold-dump work
    /// is not starved indefinitely").
    pub const DONOR_ANTI_STARVE_CAP: u32 = 16;
}

impl Default for SchedulingPolicy {
    fn default() -> Self {
        Self {
            anti_starvation_cap: Self::DONOR_ANTI_STARVE_CAP,
        }
    }
}

/// Why a request is retired without ever running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleTerminal {
    /// The request's `(generation, epoch)` stamp no longer governs the module
    /// (the shared `generation_valid` predicate failed). The daemon's fencing
    /// terminal maps here.
    Stale {
        /// The retired request's captured generation.
        generation: ModuleGeneration,
        /// The retired request's captured epoch.
        epoch: CacheEpoch,
    },
    /// The request's deadline passed before it was chosen: it may no longer
    /// start. A request at exactly its deadline is not yet past it.
    DeadlinePassed {
        /// The deadline the request was admitted with.
        deadline: MonoTime,
        /// The supplied time of this decision.
        at: MonoTime,
    },
}

/// Why a request is deferred instead of run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deferral {
    /// The request's target selection needs upper memory and the supplied
    /// selector knowledge does not confirm that selection. The request is
    /// never run under an unconfirmed selection; the instance owner must
    /// confirm (or the module fence) before it can be chosen.
    SelectorUnconfirmed {
        /// The upper-memory selection the request targets.
        target: Selection,
        /// The selection the selector knowledge actually confirmed.
        confirmed: Option<Selection>,
    },
}

/// What the routine chose for one module queue, one decision per call.
///
/// One decision names one request; the caller re-asks with the remaining set
/// after applying it. The bounded subset retires and defers at most the one
/// request the decision names, never a whole set silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleDecision {
    /// Run this request next: one instance, one operation at a time.
    Run {
        /// The request to run.
        operation: OperationId,
    },
    /// Retire this request with the typed terminal; it never runs.
    Retire {
        /// The request to retire.
        operation: OperationId,
        /// Why the request is retired.
        terminal: ScheduleTerminal,
    },
    /// Defer this request; it is not run now. Deferral names one request and
    /// never blocks a later runnable one.
    Defer {
        /// The request to defer.
        operation: OperationId,
        /// Why the request is deferred.
        reason: Deferral,
    },
    /// No queued request exists.
    Idle,
}

/// Choose the next request for one module queue, oldest first.
///
/// The requests are examined in one pass by ascending `sequence`, and each
/// request is checked stale first, then deadline, then selector confirmation.
/// A request whose stamp fails the shared `generation_valid` predicate
/// against the selector knowledge retires with a typed stale terminal — a
/// retired stamp fences publication, so stale is checked before that same
/// request's deadline, not before every other request in the queue. A
/// request past its deadline (one at exactly its deadline is not yet past
/// it) retires with a typed deadline terminal. A request whose target
/// selection needs upper memory is deferred — never run — unless the
/// selector knowledge confirms exactly that selection; deferral does not
/// block later runnable requests. The first request that retires or runs ends
/// the pass and is the decision. If nothing runs or retires but something is
/// deferred, the oldest deferral is returned so the caller knows the queue
/// is not idle; with no requests at all the decision is idle.
/// One request's examination result: the checks of [`choose_next`], in the
/// same order (stale, then deadline, then selector confirmation), as one
/// private value both the FIFO chooser and the ranked chooser walk. Single
/// definition, no drift between the two orderings.
#[derive(Clone, Copy)]
enum Examined<'r> {
    Run(&'r RequestMeta),
    Retire(&'r RequestMeta, ScheduleTerminal),
    Defer(&'r RequestMeta, Deferral),
}

fn examine<'r>(
    request: &'r RequestMeta,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
) -> Examined<'r> {
    if !generation_valid(
        request.generation,
        request.epoch,
        selector_knowledge.generation,
        selector_knowledge.epoch,
    ) {
        return Examined::Retire(
            request,
            ScheduleTerminal::Stale {
                generation: request.generation,
                epoch: request.epoch,
            },
        );
    }
    if request.deadline.is_some_and(|deadline| now > deadline) {
        return Examined::Retire(
            request,
            ScheduleTerminal::DeadlinePassed {
                deadline: request.deadline.expect("checked deadline is Some"),
                at: now,
            },
        );
    }
    if let Some(target) = request.target {
        if selector_knowledge.selection != Some(target) {
            return Examined::Defer(
                request,
                Deferral::SelectorUnconfirmed {
                    target,
                    confirmed: selector_knowledge.selection,
                },
            );
        }
    }
    Examined::Run(request)
}

/// Turn one examined request into its decision; the caller names the
/// carried state the decision leaves behind. A Run is always a no-switch
/// service under this model (a request whose target needs an unconfirmed
/// selection defers), so every Run advances the page streak and a Run of
/// the High lane advances the priority streak while a Normal Run clears
/// it; retirements serve nobody and carry both streaks.
fn decide(examined: Examined<'_>, state: SchedulerState) -> (ScheduleDecision, SchedulerState) {
    match examined {
        Examined::Run(request) => (
            ScheduleDecision::Run {
                operation: request.operation,
            },
            state,
        ),
        Examined::Retire(request, terminal) => (
            ScheduleDecision::Retire {
                operation: request.operation,
                terminal,
            },
            state,
        ),
        Examined::Defer(request, reason) => (
            ScheduleDecision::Defer {
                operation: request.operation,
                reason,
            },
            state,
        ),
    }
}

/// Walk the ordered queue: the first request that retires or runs ends the
/// pass and is the decision; the first deferral is remembered but never
/// blocks a later runnable request. With no requests the decision is idle.
fn scan(
    ordered: &[&RequestMeta],
    previous: &SchedulerState,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
) -> (ScheduleDecision, SchedulerState) {
    let mut deferred: Option<Examined<'_>> = None;
    for request in ordered {
        match examine(request, selector_knowledge, now) {
            Examined::Defer(_, _) if deferred.is_none() => {
                deferred = Some(examine(request, selector_knowledge, now))
            }
            examined @ (Examined::Run(_) | Examined::Retire(_, _)) => {
                // The unranked FIFO chooser serves no priority lane: every
                // run is a normal service and clears the donor's priority
                // streak; every run is also a no-switch service and advances
                // the page streak; a retirement serves nobody and carries
                // both.
                let state = match examined {
                    Examined::Run(_) => SchedulerState {
                        high_priority_streak: 0,
                        page_affinity_streak: previous.page_affinity_streak + 1,
                    },
                    _ => *previous,
                };
                return decide(examined, state);
            }
            _ => {}
        }
    }
    match deferred {
        Some(examined) => decide(examined, *previous),
        // Every request ran, retired or deferred on earlier calls: an empty
        // scan can only happen with no requests, handled by the caller.
        None => unreachable!("a nonempty scan defers, retires or runs one request"),
    }
}

/// Request priority for [`choose_next_ranked`]. The donor's two classes:
/// donor-normal work and the high-priority lane the donor admits ahead of
/// it. Priority ranks BEFORE affinity and FIFO within a rank; ties inside
/// a rank stay oldest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    /// The ordinary lane: bulk/cold-dump work.
    Normal,
    /// The latency-sensitive lane, admitted ahead of normal work.
    High,
}

/// A [`RequestMeta`] plus the priority the caller stamped it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrioritizedRequestMeta {
    /// The request's supplied metadata.
    pub meta: RequestMeta,
    /// The priority the caller stamped it with.
    pub priority: RequestPriority,
}

/// Choose the next request for one module queue, oldest first (the FIFO
/// subset of [`choose_next_ranked`]): stale and past-deadline requests
/// retire with typed terminals, an unconfirmed upper-memory target defers
/// without blocking later runnable requests, and the oldest runnable
/// request runs. `_policy` is the accepted seam for the ranked lane's
/// knobs; this lane consumes none of them.
pub fn choose_next(
    requests: &[RequestMeta],
    previous: &SchedulerState,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
    _policy: &SchedulingPolicy,
) -> (ScheduleDecision, SchedulerState) {
    if requests.is_empty() {
        return (ScheduleDecision::Idle, *previous);
    }
    // Oldest first by the supplied FIFO sequence, stable for equal sequences
    // (the slice order breaks ties; a caller issuing duplicate sequences owns
    // that ambiguity).
    let mut oldest_first: Vec<&RequestMeta> = requests.iter().collect();
    oldest_first.sort_by_key(|request| request.sequence);
    scan(&oldest_first, previous, selector_knowledge, now)
}

/// Priority-aware ranking: the donor's `xcvr_pop_next_req` selection
/// (sched.c:296-373) as one lexicographic pass over the supplied requests
/// — priority class (KEY1), page affinity (KEY2), FIFO (KEY3) — with both
/// anti-starvation gates the donor evaluates together (sched.c:1218-1235).
/// The per-request checks and their order are exactly [`choose_next`]'s —
/// one `examine`, two orderings.
///
/// A high-priority stale or past-deadline request still retires (it is
/// examined first in score order), and a deferral never blocks a later
/// runnable request — EXCEPT under the armed page gate, where the
/// different-page request the donor would have served is surfaced to the
/// owner as the owed page switch (see [`scan_ranked`]). Vector yield is
/// delivered as [`vector_yield`]; mid-flight preemption is the caller's to
/// apply from that decision.
pub fn choose_next_ranked(
    requests: &[PrioritizedRequestMeta],
    previous: &SchedulerState,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
    policy: &SchedulingPolicy,
) -> (ScheduleDecision, SchedulerState) {
    if requests.is_empty() {
        return (ScheduleDecision::Idle, *previous);
    }
    let force_prio = previous.high_priority_streak >= policy.anti_starvation_cap;
    let force_page = previous.page_affinity_streak >= policy.anti_starvation_cap
        && requests
            .iter()
            .any(|r| !same_page(&r.meta, selector_knowledge));
    // The donor's packed score, computed once per request: KEY1 (priority
    // class, inverted under the armed priority gate) dominates KEY2 (page
    // affinity, inverted under the armed page gate); FIFO breaks ties
    // through stable sort (the caller's slice order breaks equal keys,
    // exactly like `choose_next`).
    let mut runnable_normal = false;
    let mut scored: Vec<&PrioritizedRequestMeta> = Vec::with_capacity(requests.len());
    for request in requests {
        if matches!(
            examine(&request.meta, selector_knowledge, now),
            Examined::Run(_)
        ) && request.priority == RequestPriority::Normal
        {
            runnable_normal = true;
        }
        scored.push(request);
    }
    // KEY1 inverts only when the gate can actually serve somebody (a
    // runnable normal request exists) — the verified adaptation below.
    let invert_prio = force_prio && runnable_normal;
    scored.sort_by_key(|request| {
        let key1 = match (invert_prio, request.priority) {
            (true, RequestPriority::Normal) | (false, RequestPriority::High) => 1u8,
            _ => 0,
        };
        let key2 = u8::from(same_page(&request.meta, selector_knowledge) != force_page);
        // Highest score first: KEY1 and KEY2 descending, FIFO ascending.
        (
            std::cmp::Reverse(key1),
            std::cmp::Reverse(key2),
            request.meta.sequence,
        )
    });
    scan_ranked(&scored, previous, selector_knowledge, now, force_page)
}

/// Whether a request is served without a page switch under the supplied
/// selector knowledge — the donor's `xcvr_req_same_page` (sched.c:66-73):
/// `port->sel_known && req->bank == port->current_bank && req->page ==
/// port->current_page`. Mapped to the library's types: the selector
/// knowledge must hold a confirmed selection at all (an unknown selector
/// cannot establish affinity — the donor's poisoned selector "matches
/// nothing, so the next access reprograms it"), and the request's target
/// must equal it. A request with no upper-memory target (`target: None`)
/// follows the donor's convention that a lower-memory request addresses
/// the lower page (bank 0, page 0): it is same-page exactly while that page
/// is the confirmed selection.
fn same_page(request: &RequestMeta, selector_knowledge: &SelectorKnowledge) -> bool {
    selector_knowledge.selection.is_some_and(|confirmed| {
        let (bank, page) = request
            .target
            .map_or((0, 0), |target| (target.bank, target.page));
        confirmed.bank == bank && confirmed.page == page
    })
}

/// The ranked walk: the donor's lexicographic selection with the two
/// anti-starvation gates, adapted to the pure deferral model.
///
/// Selection (donor `xcvr_pop_next_req`, sched.c:296-373): the requests
/// arrive pre-sorted by (KEY1, KEY2, KEY3) — priority class before page
/// affinity before FIFO — and the first request that retires or runs in
/// that order is the decision. KEY1 inverts under the armed priority gate
/// exactly as the donor demotes the high lane for one decision
/// (sched.c:1218-1235); KEY2 inverts under the armed page gate exactly as
/// the donor's `force_different` prefers a different-page request
/// (sched.c:1185-1195). When both gates are armed and no request satisfies
/// both, KEY1 dominates — the donor's documented corner case
/// (sched.c:283-290): the priority inversion is served this decision and
/// the page yield is deferred by exactly one decision, bounded and
/// self-correcting (the inversion clears the priority streak while the
/// page streak keeps the page gate armed for the next decision).
///
/// Two adaptations, both verified against the donor and kept deliberately:
///
/// 1. The priority gate fires only when a normal request is *runnable*,
///    not merely queued. The donor's `xcvr_has_waiter_normal` counts every
///    queued normal request, but the donor's pop can serve any queued
///    request; this chooser additionally retires and defers per-request, so
///    inverting onto a request that would only defer or retire would serve
///    nobody and would not clear the streak (sched.c:1420-1432's own
///    argument — the gate exists to serve the starved work, once, now).
/// 2. Under the armed page gate, the different-page request the donor
///    would have popped and served (switching pages as part of the service)
///    cannot run here: a request whose target needs an unconfirmed
///    selection defers. Surfacing that deferral IS this model's page
///    switch: the owner reads it as the request to confirm the target
///    selection (task 36's selector planning owns the actual write), which
///    is why the surfaced deferral resets the page streak — the donor
///    resets `same_page_count` at the executed switch (sched.c:1026-1028),
///    and the reset at the surfaced one is the bounded equivalent (if the
///    owner abandons the switch, the streak re-accumulates and the gate
///    re-fires after another cap services). Without this surfacing,
///    same-page work would be chosen forever and the different-page request
///    would starve — exactly what the donor's gate 1 exists to prevent. A
///    deferral that is NOT the gate's different-page winner never blocks a
///    later runnable request, matching the landed FIFO-chooser rule.
fn scan_ranked(
    ranked: &[&PrioritizedRequestMeta],
    previous: &SchedulerState,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
    force_page: bool,
) -> (ScheduleDecision, SchedulerState) {
    let mut deferred: Option<Examined<'_>> = None;
    for request in ranked {
        let examined = examine(&request.meta, selector_knowledge, now);
        match examined {
            Examined::Retire(..) => {
                // A retire ends the pass; it serves nobody and carries both
                // streaks (the donor's pop has no retirement at all — this
                // per-request check is this chooser's own, shared with the
                // FIFO lane).
                return decide(examined, *previous);
            }
            Examined::Run(_) => {
                let priority = request.priority;
                return decide(
                    examined,
                    SchedulerState {
                        high_priority_streak: match priority {
                            RequestPriority::High => previous.high_priority_streak + 1,
                            RequestPriority::Normal => 0,
                        },
                        // A Run never switches pages in this model: the
                        // owner switches outside, when it confirms a
                        // selection. The donor increments its no-switch
                        // streak on exactly this service (sched.c:1029).
                        page_affinity_streak: previous.page_affinity_streak + 1,
                    },
                );
            }
            Examined::Defer(..) => {
                if force_page && !same_page(&request.meta, selector_knowledge) {
                    // The armed page gate's different-page winner: surface
                    // the owed page switch and reset the page streak (the
                    // donor resets at its executed switch; see above).
                    return decide(
                        examined,
                        SchedulerState {
                            high_priority_streak: previous.high_priority_streak,
                            page_affinity_streak: 0,
                        },
                    );
                }
                if deferred.is_none() {
                    deferred = Some(examined);
                }
            }
        }
    }
    match deferred {
        Some(examined) => decide(examined, *previous),
        None => unreachable!("a nonempty scan defers, retires or runs one request"),
    }
}

/// One in-flight CDB transaction's fencing facts, supplied per call by the
/// instance owner. The library keeps no live transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbInFlight {
    /// The generation the in-flight transaction was admitted under.
    pub generation: ModuleGeneration,
    /// The cache epoch the in-flight transaction was admitted under.
    pub epoch: CacheEpoch,
}

/// Why a CDB request is fenced from admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbFenceReason {
    /// The request's stamp no longer governs the module (the shared
    /// `generation_valid` predicate failed): a fenced CDB request never
    /// publishes into a replacement instance.
    Stale {
        /// The fenced request's captured generation.
        generation: ModuleGeneration,
        /// The fenced request's captured epoch.
        epoch: CacheEpoch,
    },
    /// A CDB transaction is already in flight. CDB fencing is distinct
    /// from burst exclusion: one CDB transaction at a time per module,
    /// whatever its generation — the in-flight facts are reported back so
    /// the caller can distinguish this fence from a stale stamp.
    TransactionInFlight {
        /// The generation of the in-flight transaction.
        in_flight_generation: ModuleGeneration,
        /// The epoch of the in-flight transaction.
        in_flight_epoch: CacheEpoch,
    },
    /// The request's deadline passed before admission. A CDB request is
    /// never left indefinitely poll-deferred past its deadline: expiry
    /// retires it here instead.
    DeadlinePassed {
        /// The deadline the request was admitted with.
        deadline: MonoTime,
        /// The supplied time of this decision.
        at: MonoTime,
    },
}

/// The admission decision for one CDB request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbAdmission {
    /// The request may run now.
    Admit,
    /// The request is fenced with the typed reason; it never runs under
    /// the reported condition.
    Fence {
        /// Why the request is fenced.
        reason: CdbFenceReason,
    },
    /// The request's target selection needs upper memory and the supplied
    /// selector knowledge does not confirm it: deferred, not fenced.
    Defer(Deferral),
}

/// CDB admission (task 63): pure decision over one supplied request, the
/// optionally in-flight CDB transaction's facts, and selector knowledge.
///
/// Order of the checks: stale stamps fence first (a retired stamp fences
/// publication, before any same-stamp deadline); an in-flight CDB
/// transaction fences next (one CDB at a time, regardless of generation —
/// fencing, not burst exclusion); a passed deadline fences rather than
/// deferring, so expiry can never leave the request indefinitely
/// poll-deferred; an unconfirmed selector target defers; otherwise the
/// request is admitted. No queue, token, clock or transport is touched.
pub fn cdb_admit(
    request: &RequestMeta,
    in_flight: Option<&CdbInFlight>,
    selector_knowledge: &SelectorKnowledge,
    now: MonoTime,
) -> CdbAdmission {
    if !generation_valid(
        request.generation,
        request.epoch,
        selector_knowledge.generation,
        selector_knowledge.epoch,
    ) {
        return CdbAdmission::Fence {
            reason: CdbFenceReason::Stale {
                generation: request.generation,
                epoch: request.epoch,
            },
        };
    }
    if let Some(in_flight) = in_flight {
        return CdbAdmission::Fence {
            reason: CdbFenceReason::TransactionInFlight {
                in_flight_generation: in_flight.generation,
                in_flight_epoch: in_flight.epoch,
            },
        };
    }
    if request.deadline.is_some_and(|deadline| now > deadline) {
        return CdbAdmission::Fence {
            reason: CdbFenceReason::DeadlinePassed {
                deadline: request.deadline.expect("checked deadline is Some"),
                at: now,
            },
        };
    }
    if let Some(target) = request.target {
        if selector_knowledge.selection != Some(target) {
            return CdbAdmission::Defer(Deferral::SelectorUnconfirmed {
                target,
                confirmed: selector_knowledge.selection,
            });
        }
    }
    CdbAdmission::Admit
}

/// Matching-generation exclusive-token validation (task 63): a token minted
/// under `(generation, epoch)` is valid only while exactly that pair still
/// governs the module. Pure wrapper over the shared `generation_valid`
/// predicate, so tokens and every other stamp can never disagree about
/// what governs.
pub fn token_valid(
    token_generation: ModuleGeneration,
    token_epoch: CacheEpoch,
    selector_knowledge: &SelectorKnowledge,
) -> bool {
    generation_valid(
        token_generation,
        token_epoch,
        selector_knowledge.generation,
        selector_knowledge.epoch,
    )
}

/// The in-flight request's yield facts, supplied per call by the instance
/// owner (task 63's segment cursor and yield eligibility). Pure data: the
/// library owns no in-flight request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YieldFacts {
    /// The running request's priority.
    pub priority: RequestPriority,
    /// True only for a multi-segment (vector) transfer the owner can pause
    /// BETWEEN segments: the donor's heap-style reads. A scalar or
    /// read-modify-write request is never yieldable — it is one segment,
    /// and splitting it mid-flight is forbidden (task 63: "never split a
    /// scalar/RMW or nonyieldable request").
    pub segmentable: bool,
    /// Segments still to run after the current one.
    pub remaining_segments: u32,
    /// The caller's cursor: the index of the next segment to run when the
    /// request resumes (preserved across a yield, never rewound).
    pub segment_cursor: u32,
}

/// One queued waiter the running request could yield to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YieldWaiter {
    /// The waiter's priority.
    pub priority: RequestPriority,
}

/// What the vector-yield reducer decided, as data for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorYieldDecision {
    /// Keep running to the next segment boundary: no eligible
    /// higher-priority waiter, or the request is not yieldable (scalar,
    /// RMW, or nothing left to yield). `cursor` is the caller's cursor,
    /// unchanged.
    Continue {
        /// The caller's segment cursor, unchanged.
        cursor: u32,
    },
    /// Yield between segments: the caller re-queues the remaining segments
    /// at the HEAD of its queue with `resume_cursor` (head-requeue, so the
    /// continuation is not itself starved behind later arrivals), then
    /// serves the waiter. The cursor is preserved exactly — no segment is
    /// replayed and none is skipped.
    Yield {
        /// The segment index to resume at.
        resume_cursor: u32,
        /// Always `true`: the remainder re-queues at the queue head.
        requeue_head: bool,
    },
}

/// Vector yield (task 63): whether an in-flight segmented request yields
/// at the next segment boundary because a higher-priority waiter is
/// queued. The donor rule (sched.c:137-141, `xcvr_should_preempt_locked`):
/// a heap-owned vector read yields mid-vector when a high-priority waiter
/// is queued, so latency-sensitive work need not wait head-of-line behind a
/// bulk read's remaining segments — but only a segmentable, non-scalar
/// request with segments left can yield.
///
/// The anti-starvation budget is deliberately NOT re-checked here, for the
/// donor's own reason (sched.c:142-145): the preempted continuation is
/// itself a normal waiter, so once the high-priority streak reaches the
/// cap the chooser's inversion serves it — it advances at least one
/// segment per cap services and cannot be starved. The caller applies the
/// requeue and runs the waiter; nothing here owns the queue.
pub fn vector_yield(running: &YieldFacts, waiters: &[YieldWaiter]) -> VectorYieldDecision {
    let eligible = running.segmentable && running.remaining_segments > 0;
    let higher_waiter = waiters.iter().any(|w| w.priority > running.priority);
    if eligible && higher_waiter {
        VectorYieldDecision::Yield {
            resume_cursor: running.segment_cursor,
            requeue_head: true,
        }
    } else {
        VectorYieldDecision::Continue {
            cursor: running.segment_cursor,
        }
    }
}

/// The host-CDB transaction lifecycle (task 63; donor cdb_txn.c A4). The
/// daemon owns the live transaction, its references and its finisher; the
/// reducer moves plain state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbTxnState {
    /// Quiescent: no transaction in flight; admission is possible.
    Idle,
    /// Admitted, nothing issued yet.
    Admitted,
    /// The first side-effectful staging write issued.
    Staging,
    /// The trigger command was written; awaiting CdbCommandStatus.
    Triggered,
    /// The submitter timed out after the first write; the finisher keeps
    /// driving bus ops, so the transaction is still active.
    Quarantine,
    /// Terminal: the object is quiescent and re-admittable.
    Done,
}

impl CdbTxnState {
    /// Whether a transaction is in flight (donor `xcvr_cdb_txn_active`):
    /// every state except the quiescent IDLE/DONE, including quarantine
    /// whose finisher is still driving bus ops.
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle | Self::Done)
    }
}

/// One lifecycle event offered to [`cdb_advance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbEvent {
    /// Admission succeeded (the caller checked the quiescent state and
    /// the drained reference count — its facts, not this reducer's).
    Admit,
    /// The first side-effectful staging write issued.
    Staged,
    /// The trigger command issued.
    Triggered,
    /// The submitter timed out after the first write.
    Quarantined,
}

/// What [`cdb_advance`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbAdvance {
    /// The event moved the transaction to this state.
    Moved(CdbTxnState),
    /// The event is illegal from the current state: rejected, state
    /// unchanged (the caller keeps its transaction exactly as it was).
    Illegal {
        /// The state the illegal event arrived in.
        from: CdbTxnState,
        /// The rejected event.
        event: CdbEvent,
    },
}

/// Advance the CDB transaction lifecycle by one event (task 63; donor
/// `__txn_advance`, cdb_txn.c:106-125: non-terminal transitions only,
/// rejected unless the transaction is exactly in the expected state).
///
/// Legal moves: `Admit` only from the quiescent IDLE/DONE (the caller's
/// drained-reference fact is its own); `Staged` from ADMITTED;
/// `Triggered` from ADMITTED or STAGING (a command may run with no staging
/// writes); `Quarantined` from STAGING or TRIGGERED. The terminals DONE
/// and IDLE are reachable ONLY through [`cdb_finish_once`] and a fresh
/// admit — advancing into them is refused, exactly as the donor rejects
/// it (`WARN_ON_ONCE(to == DONE || to == IDLE)`). Pure: no lock, no
/// refcount, no completion is touched.
pub fn cdb_advance(previous: CdbTxnState, event: CdbEvent) -> CdbAdvance {
    use CdbEvent as E;
    use CdbTxnState as S;
    let moved = |to: S| CdbAdvance::Moved(to);
    match (previous, event) {
        (S::Idle | S::Done, E::Admit) => moved(S::Admitted),
        (S::Admitted, E::Staged) => moved(S::Staging),
        (S::Admitted | S::Staging, E::Triggered) => moved(S::Triggered),
        (S::Staging | S::Triggered, E::Quarantined) => moved(S::Quarantine),
        (from, event) => CdbAdvance::Illegal { from, event },
    }
}

/// What [`cdb_finish_once`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbFinish {
    /// This caller won the one-shot terminal transition: the transaction
    /// is DONE.
    Finished,
    /// The transaction was already DONE: exactly one caller ever wins
    /// (the donor's `terminated` flag), and the loser's outcome is
    /// discarded — never applied twice.
    AlreadyFinished,
}

/// The one-shot CDB terminal transition (task 63; donor
/// `__txn_finish_once`, cdb_txn.c:167-193). From any non-terminal state
/// the first caller wins and the transaction becomes DONE; a second
/// finisher — or a finish of an already-DONE transaction — loses with
/// [`CdbFinish::AlreadyFinished`] and changes nothing. The outcome rides
/// along as the caller's fact; the reducer moves only state.
pub fn cdb_finish_once(previous: CdbTxnState) -> (CdbTxnState, CdbFinish) {
    match previous {
        CdbTxnState::Done => (CdbTxnState::Done, CdbFinish::AlreadyFinished),
        _ => (CdbTxnState::Done, CdbFinish::Finished),
    }
}

/// poll_defer (task 63): whether a background poller defers its bus
/// traffic while a CDB transaction owns the command interface. A wedged
/// transaction can never indefinitely suppress monitoring: defer only
/// while the transaction is active AND its deadline has not expired
/// (donor `xcvr_cdb_txn_poll_defer`: `active && !deadline_expired`).
pub fn poll_defer(active: bool, deadline_expired: bool) -> bool {
    active && !deadline_expired
}

// ── Exclusive sessions (task 63; donor sched.c:1244-1298) ──────────────────

/// A nonzero exclusive-session token (donor `io_excl_token`, minted from
/// the monotonically advancing `io_excl_gen` that skips zero). Opaque to
/// the library beyond nonzero-ness and equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExclusiveToken(pub u64);

impl ExclusiveToken {
    /// The no-token value: never minted, never valid (the donor's
    /// `xcvr_io_excl_end(port, 0)` is a no-op and `xcvr_io_excl_valid(port,
    /// 0)` is false).
    pub const NONE: ExclusiveToken = ExclusiveToken(0);

    /// Whether this is a minted (nonzero) token.
    pub fn is_minted(self) -> bool {
        self.0 != 0
    }
}

/// One live exclusive session: a supplied owner/operation and the module
/// generation/epoch the token was minted under (task 63: "Exclusive
/// admission binds a supplied owner/operation and module generation to a
/// fresh nonzero token"). Plain data — the daemon owns the session object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExclusiveSession {
    /// The operation that owns the session.
    pub owner: OperationId,
    /// The module generation the token was minted under.
    pub generation: ModuleGeneration,
    /// The cache epoch the token was minted under.
    pub epoch: CacheEpoch,
    /// The minted token.
    pub token: ExclusiveToken,
}

/// The exclusive-session state the caller stores and supplies back. Pure
/// data; no lock, refcount or queue lives here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExclusiveState {
    /// The live session, when one is active.
    pub session: Option<ExclusiveSession>,
    /// The next token value to mint (donor `io_excl_gen`). Advanced by every
    /// begin so a fresh token never reuses a live or retired value; skips
    /// zero at mint time.
    pub next_token: u64,
}

/// One event offered to [`exclusive_step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusiveEvent {
    /// Begin a session: mint a fresh nonzero token bound to the supplied
    /// owner and the CURRENT module generation/epoch.
    Begin {
        /// The operation that will own the session.
        owner: OperationId,
        /// The generation to bind the token to.
        generation: ModuleGeneration,
        /// The epoch to bind the token to.
        epoch: CacheEpoch,
    },
    /// Enqueue admission for one request carrying `token`: the atomic
    /// validate-and-admit the donor performs under its lock (its
    /// `__xcvr_io_run_excl_timed` enqueue re-checks active-and-matching
    /// before the request becomes visible to pop — sched.c:1602-1610).
    Enqueue {
        /// The token the admission request carries.
        token: ExclusiveToken,
        /// The request asking for admission.
        request: RequestMeta,
    },
    /// End the session named by `token`.
    End {
        /// The token of the session to end.
        token: ExclusiveToken,
    },
    /// Removal, expiry or owner loss: tear the session down with explicit
    /// recovery effects (the donor's plain end leaves queued member
    /// requests untouched and re-kicks the worker — a teardown gap this
    /// reducer does not inherit; task 63: "Removal/expiry/owner loss
    /// returns explicit invalidate-and-recover effects").
    Invalidate,
}

/// Why an enqueue was refused admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusiveRefusal {
    /// No session is active at all.
    NotActive,
    /// A session is active but the token is not its own: a stale token
    /// from a retired session never admits (and never disturbs the live
    /// one).
    StaleToken {
        /// The token the request offered.
        offered: ExclusiveToken,
        /// The live session's token.
        live: ExclusiveToken,
    },
    /// The token matches, but the session's `(generation, epoch)` stamp no
    /// longer governs the module (the shared `generation_valid` predicate
    /// failed): the session is fenced — its members must not publish into
    /// a replacement instance.
    StaleSession {
        /// The generation the fenced session was minted under.
        session_generation: ModuleGeneration,
        /// The epoch the fenced session was minted under.
        session_epoch: CacheEpoch,
    },
}

/// One explicit recovery effect of an invalidation, as data for the daemon
/// to apply (the owner of every queue and fence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusiveEffect {
    /// Retire every member request still carrying this token: they may not
    /// run after the teardown.
    RetireMemberRequests {
        /// The token of the torn-down session.
        token: ExclusiveToken,
    },
    /// Fence publication for the torn-down session's generation: results
    /// stamped under it never publish into a replacement instance.
    FenceGeneration {
        /// The generation to fence.
        generation: ModuleGeneration,
        /// The epoch to fence.
        epoch: CacheEpoch,
    },
}

/// What [`exclusive_step`] decided, with the state it leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusiveOutcome {
    /// The session began with this fresh nonzero token.
    Began {
        /// The freshly minted token.
        token: ExclusiveToken,
    },
    /// A session is already active (donor `-EBUSY`): nothing changed.
    BeginDenied,
    /// The request is admitted under the live session: the caller may
    /// enqueue it atomically with this result.
    EnqueueAdmitted,
    /// The request is refused admission with the typed reason; the live
    /// session (if any) is untouched.
    EnqueueRefused {
        /// Why the request was refused.
        reason: ExclusiveRefusal,
    },
    /// The matching token ended the session.
    Ended,
    /// A stale or tokenless end: it cannot affect a newer session or mint
    /// one (the donor silently ignores it — a stale end must never disturb
    /// a newer token).
    EndIgnored {
        /// The ignored token the end offered.
        offered: ExclusiveToken,
    },
    /// The session (if any) was torn down; the daemon must apply the
    /// returned recovery effects.
    Invalidated {
        /// The recovery effects to apply.
        effects: Vec<ExclusiveEffect>,
    },
}

/// Advance the exclusive-session lifecycle by one event (task 63; donor
/// `xcvr_io_excl_begin`/`xcvr_io_excl_end`/`xcvr_io_excl_valid`,
/// sched.c:1244-1298, plus the pop-time token gate sched.c:325-331).
/// Pure: the reducer mints tokens from the supplied counter, binds stamps
/// and returns effects; the daemon owns serialization and applies the
/// result atomically (task 63: "Validate and return enqueue admission as
/// one reducer result for the daemon to apply atomically under its own
/// serialization").
///
/// Begin while active is denied. Enqueue requires an active session, the
/// exact token, and a session stamp that still governs `(current_generation,
/// current_epoch)`. End with a mismatched or zero token is ignored — a
/// stale end cannot affect a newer token. Invalidate always tears down and
/// returns the explicit recovery effects; with no live session it is a
/// no-op with no effects.
pub fn exclusive_step(
    previous: ExclusiveState,
    event: ExclusiveEvent,
    current_generation: ModuleGeneration,
    current_epoch: CacheEpoch,
) -> (ExclusiveState, ExclusiveOutcome) {
    match event {
        ExclusiveEvent::Begin {
            owner,
            generation,
            epoch,
        } => {
            if previous.session.is_some() {
                return (previous, ExclusiveOutcome::BeginDenied);
            }
            // Mint the next nonzero token; wrap skips zero like the donor's
            // `do { gen++ } while (gen == 0)`.
            let mut next = previous.next_token.wrapping_add(1);
            if next == 0 {
                next = 1;
            }
            let token = ExclusiveToken(next);
            let state = ExclusiveState {
                session: Some(ExclusiveSession {
                    owner,
                    generation,
                    epoch,
                    token,
                }),
                next_token: next,
            };
            (state, ExclusiveOutcome::Began { token })
        }
        ExclusiveEvent::Enqueue { token, .. } => {
            let Some(session) = previous.session else {
                return (
                    previous,
                    ExclusiveOutcome::EnqueueRefused {
                        reason: ExclusiveRefusal::NotActive,
                    },
                );
            };
            if session.token != token {
                return (
                    previous,
                    ExclusiveOutcome::EnqueueRefused {
                        reason: ExclusiveRefusal::StaleToken {
                            offered: token,
                            live: session.token,
                        },
                    },
                );
            }
            if !generation_valid(
                session.generation,
                session.epoch,
                current_generation,
                current_epoch,
            ) {
                return (
                    previous,
                    ExclusiveOutcome::EnqueueRefused {
                        reason: ExclusiveRefusal::StaleSession {
                            session_generation: session.generation,
                            session_epoch: session.epoch,
                        },
                    },
                );
            }
            (previous, ExclusiveOutcome::EnqueueAdmitted)
        }
        ExclusiveEvent::End { token } => {
            if token.is_minted()
                && previous
                    .session
                    .is_some_and(|session| session.token == token)
            {
                (
                    ExclusiveState {
                        session: None,
                        next_token: previous.next_token,
                    },
                    ExclusiveOutcome::Ended,
                )
            } else {
                (previous, ExclusiveOutcome::EndIgnored { offered: token })
            }
        }
        ExclusiveEvent::Invalidate => match previous.session {
            Some(session) => (
                ExclusiveState {
                    session: None,
                    next_token: previous.next_token,
                },
                ExclusiveOutcome::Invalidated {
                    effects: vec![
                        ExclusiveEffect::RetireMemberRequests {
                            token: session.token,
                        },
                        ExclusiveEffect::FenceGeneration {
                            generation: session.generation,
                            epoch: session.epoch,
                        },
                    ],
                },
            ),
            None => (
                previous,
                ExclusiveOutcome::Invalidated {
                    effects: Vec::new(),
                },
            ),
        },
    }
}

// ── Request life (task 63; donor include/xcvr/sched.h enum xcvr_io_life) ────

/// The lifecycle of one request (task 63's RequestLife; donor `enum
/// xcvr_io_life`, sched.h:17-38): queued, running, cancelled-draining, or
/// one of the two terminals. A request terminates exactly once — the
/// donor's completion fires once (its `xcvr_req_complete` cmpxchg moves
/// RUNNING→COMPLETED and everything else records DRAINED).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestLife {
    /// On the queue, not yet (or not currently) executing.
    Queued,
    /// The worker is executing a segment batch of this request.
    Running,
    /// A submitter abandoned the wait: the worker must drain, never
    /// resume, this request.
    CancelRequested,
    /// Terminal — completion fired exactly once.
    Completed,
    /// Terminal — cancelled before/without completing normally.
    Drained,
}

impl RequestLife {
    /// Whether the life has reached a terminal state (the donor's terminals
    /// never re-enter the machine).
    pub fn is_terminal(self) -> bool {
        matches!(self, RequestLife::Completed | RequestLife::Drained)
    }
}

/// What a drained in-flight WRITE reports about its application. There is
/// no "rolled back" value and never will be: a write that reached the wire
/// may have been applied, and the library never claims a rollback it
/// cannot prove (task 63: "cancellation of in-flight writes reports
/// pending/uncertain application and never claims rollback").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteApplication {
    /// The write never started: nothing was applied.
    NotStarted,
    /// The write was issued but its outcome is not yet known.
    Pending,
    /// The write's outcome cannot be established (transport loss, timeout):
    /// it may or may not have been applied.
    Uncertain,
}

/// One event offered to [`request_step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestLifeEvent {
    /// The worker dequeues and starts executing the request.
    Start,
    /// A submitter abandons the wait: arm the drain.
    CancelRequested,
    /// The worker reached the terminal transition with its own outcome in
    /// hand (the outcome rides with the caller; this reducer moves only
    /// life).
    Finish,
    /// The request is drained without completing normally: cancelled at a
    /// segment boundary, drained from the queue, or abandoned pre-run. A
    /// drained WRITE must report its application; a read reports `None`.
    Drained {
        /// The drained write's application report, when it was a write.
        write_application: Option<WriteApplication>,
    },
}

/// What [`request_step`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifeStep {
    /// The event moved the request to this life.
    Moved(RequestLife),
    /// The event is illegal from the current life (a start of an abandoned
    /// request, a cancel of a terminal, a second drain): rejected, life
    /// unchanged.
    Illegal {
        /// The life the illegal event arrived in.
        from: RequestLife,
    },
    /// The request had already terminated: exactly one terminal transition
    /// ever wins, and the loser changes nothing (the donor's
    /// `xcvr_req_complete` cmpxchg loser records DRAINED without re-firing
    /// completion; a second finisher here is told it lost).
    AlreadyTerminal {
        /// The terminal life the late event arrived in.
        from: RequestLife,
    },
}

/// Advance one request's life by one event (task 63). Pure: no refcount,
/// completion or callback is touched. Legal moves, exactly the donor's:
/// `Start` only from QUEUED; `CancelRequested` from QUEUED or RUNNING;
/// `Finish` from RUNNING completes, and from CANCEL_REQUESTED — where the
/// worker did finish but the submitter had already abandoned — drains; a
/// `Finish` of a never-run request drains (the donor's "never-run drained
/// req"); `Drained` from every non-terminal life. Any event on a terminal
/// life is [`LifeStep::AlreadyTerminal`] — a request terminates once.
pub fn request_step(previous: RequestLife, event: RequestLifeEvent) -> LifeStep {
    use RequestLife as L;
    use RequestLifeEvent as E;
    if previous.is_terminal() {
        return LifeStep::AlreadyTerminal { from: previous };
    }
    match (previous, event) {
        (L::Queued, E::Start) => LifeStep::Moved(L::Running),
        (L::Queued | L::Running, E::CancelRequested) => LifeStep::Moved(L::CancelRequested),
        (L::Running, E::Finish) => LifeStep::Moved(L::Completed),
        (L::CancelRequested, E::Finish) => LifeStep::Moved(L::Drained),
        (L::Queued, E::Finish) => LifeStep::Moved(L::Drained),
        (L::Queued | L::Running | L::CancelRequested, E::Drained { .. }) => {
            LifeStep::Moved(L::Drained)
        }
        (from, _) => LifeStep::Illegal { from },
    }
}

// ── Presence-probe governor (task 63; donor sched.c:1814-2323) ─────────────

/// The governor's token-bucket policy, as supplied per call. The donor's
/// values (sched.h:288-290: `XCVR_PROBE_GOV_BURST 32`,
/// `XCVR_PROBE_GOV_REFILL_PER_TICK 1`, `XCVR_PROBE_GOV_BACKOFF_MS 100`)
/// are exposed as [`Self::DONOR_DEFAULT`] with that provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernorPolicy {
    /// Bucket depth: the maximum tokens the bucket holds (donor burst).
    pub burst: u64,
    /// Tokens refilled per elapsed tick.
    pub refill_per_tick: u64,
    /// One refill tick of monotonic time (the donor's jiffy).
    pub tick: MonoDuration,
    /// The defer-reschedule baseline the donor floors with its caller's own
    /// backoff (sched.h:272-276). Returned as the retry cadence when the
    /// bucket math yields no earlier time.
    pub backoff: MonoDuration,
}

impl GovernorPolicy {
    /// The donor's governor policy constants (sched.h:288-290), with a 10 ms
    /// tick standing in for the kernel jiffy (the donor's own `HZ` default;
    /// the caller supplies its real tick when it has one).
    pub const DONOR_DEFAULT: GovernorPolicy = GovernorPolicy {
        burst: 32,
        refill_per_tick: 1,
        tick: MonoDuration(10_000_000),
        backoff: MonoDuration(100_000_000),
    };
}

/// The governor state the caller stores and supplies back: the bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernorState {
    /// Tokens currently available (capped at the policy's burst).
    pub tokens: u64,
    /// The monotonic stamp of the last refill (donor `last_refill`; 64-bit
    /// stamps make the arithmetic wrap-free at any idle interval).
    pub last_refill: MonoTime,
}

/// The caller-owned reservation/waiter facts one governor decision
/// consumes (task 63: "Governor decisions consume caller-owned parent
/// reservations/waiters"). The daemon owns the FIFO list; the reducer
/// consumes its shape. Topology is a REQUIRED FACT, never inferred from a
/// lock: `None` means the caller does not know whether a shared parent
/// governs this port, and an unknown topology defers rather than guessing
/// an ungoverned one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernorFacts {
    /// Whether a shared-parent governor governs this port's probes:
    /// `Some(true)` governed, `Some(false)` positively known ungoverned
    /// (the donor's not-yet-bound cage adapter admits), `None` unknown —
    /// defer, never invent an ungoverned parent.
    pub governed: Option<bool>,
    /// The number of FIFO reservations currently queued in the governor.
    pub queued_waiters: usize,
    /// This caller's position in that FIFO (`None` when it is not queued).
    pub position: Option<usize>,
}

/// What one governor decision returned, as data for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernorDecision {
    /// Admitted: one token was consumed (the returned state reflects it).
    /// For a queued caller this is the FIFO admission the donor guarantees
    /// by construction (a waiter is admitted once the fill covers every
    /// waiter ahead of it).
    Admit,
    /// Deferred: retry at `retry_at`. `queued` says whether this decision
    /// established (or kept) a FIFO reservation for the caller — the donor
    /// queues every deferred caller that supplied a key, so deferral is
    /// starvation-free by construction; `position` is the caller's place in
    /// the FIFO.
    Defer {
        /// The earliest time a retry may be attempted.
        retry_at: MonoTime,
        /// Whether this decision established (or kept) a FIFO
        /// reservation for the caller.
        queued: bool,
        /// The caller's place in the FIFO, when queued.
        position: Option<usize>,
    },
    /// The caller's reservation was cancelled, never admitted: the
    /// every-abandon-path-cancels rule (the donor's `cancel_wait`, plus the
    /// attach-failure and teardown purges). No token changes hands.
    Cancelled,
}

/// What the caller is asking of the governor (task 63: its decisions
/// "return FIFO admission, defer time or cancellation").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernorRequest {
    /// Ask for admission to run one probe now.
    Try,
    /// Abandon a queued reservation (a cancelled probe never consumes a
    /// token and never keeps its slot — a later waiter's position only
    /// shrinks).
    CancelWait,
}

/// One presence-probe rate decision (task 63; donor
/// `xcvr_probe_governor_try_locked`, sched.c:2090-2230). Pure over the
/// supplied bucket, waiter facts, time and policy: the returned state
/// carries the refilled-and-decremented bucket; the daemon owns the wait
/// list, timers and probes.
///
/// Refill is the donor's exact arithmetic: `tokens = min(tokens +
/// elapsed_ticks * refill_per_tick, burst)`, elapsed measured against
/// `last_refill`. Admission is the donor's two rules: a QUEUED caller is
/// admitted only once the fill covers every waiter ahead of it (`tokens >
/// position`), and a NON-QUEUED caller only ever takes the surplus beyond
/// the queued reservations (`tokens > queued_waiters`) — reservations are
/// one token each, so a surplus taker can never strand a declared waiter.
/// A deferred non-queued caller is told to queue (`queued: true`, its next
/// position), matching the donor's tail-insert. Unknown topology defers
/// without queueing (the caller must resolve its facts first); a
/// positively ungoverned parent admits without consuming a token, exactly
/// as the donor's not-yet-bound adapter returns `XCVR_PROBE_OK`. A
/// `CancelWait` never touches the bucket.
pub fn governor_step(
    previous: GovernorState,
    request: GovernorRequest,
    facts: GovernorFacts,
    now: MonoTime,
    policy: GovernorPolicy,
) -> (GovernorState, GovernorDecision) {
    if matches!(request, GovernorRequest::CancelWait) {
        return (previous, GovernorDecision::Cancelled);
    }
    if facts.governed == Some(false) {
        // Positively ungoverned: unconstrained admission, no bucket change.
        return (previous, GovernorDecision::Admit);
    }
    if facts.governed.is_none() {
        // Unknown topology: defer without queueing — never invent an
        // ungoverned (or governed) parent the caller did not state.
        let retry_at = now.checked_add(policy.backoff).unwrap_or(now);
        return (
            previous,
            GovernorDecision::Defer {
                retry_at,
                queued: false,
                position: None,
            },
        );
    }
    // Refill: u64 saturation mirrors the donor's checked-overflow clamp.
    let mut tokens = previous.tokens;
    if now > previous.last_refill {
        let elapsed = now.0.saturating_sub(previous.last_refill.0);
        let ticks = elapsed / policy.tick.0.max(1);
        let filled = tokens.saturating_add(ticks.saturating_mul(policy.refill_per_tick));
        tokens = filled.min(policy.burst);
    }
    let last_refill = if now > previous.last_refill {
        now
    } else {
        previous.last_refill
    };
    let admit = match facts.position {
        Some(position) => tokens > u64::try_from(position).unwrap_or(u64::MAX),
        None => tokens > facts.queued_waiters as u64,
    };
    if admit {
        (
            GovernorState {
                tokens: tokens.saturating_sub(1),
                last_refill,
            },
            GovernorDecision::Admit,
        )
    } else {
        // When could one more admission happen? The donor defers and lets
        // the caller reschedule at its backoff; this reducer also names the
        // earliest fill time that could admit the caller — pure arithmetic
        // over the bucket, never a scheduled wait.
        let needed = match facts.position {
            Some(position) => u64::try_from(position + 1).unwrap_or(u64::MAX),
            None => facts.queued_waiters as u64 + 1,
        };
        let missing = needed.saturating_sub(tokens);
        let ticks_needed = missing.div_ceil(policy.refill_per_tick.max(1));
        let earliest = now
            .checked_add(MonoDuration(ticks_needed.saturating_mul(policy.tick.0)))
            .unwrap_or(now);
        let retry_at = earliest.max(now.checked_add(policy.backoff).unwrap_or(now));
        let position = facts.position.or(Some(facts.queued_waiters));
        (
            GovernorState {
                tokens,
                last_refill,
            },
            GovernorDecision::Defer {
                retry_at,
                queued: true,
                position,
            },
        )
    }
}

// ── Parent health admission (task 63; donor core/safety.c:590-744) ────────

/// The physical parent's health classification the caller observed
/// (donor FSM: HEALTHY → SUSPECT → SICK on consecutive watchdog trips;
/// SICK denies admission — "never enter a SICK parent").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentHealth {
    /// No watchdog trips observed.
    Healthy,
    /// One watchdog trip: degraded but still admitting traffic.
    Suspect,
    /// Consecutive trips: a wedged parent; only the recovery lane may
    /// enter.
    Sick,
}

/// One parked waiter's facts (the daemon owns the waiter itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthWaiter {
    /// Whether the waiter is the high-priority lane.
    pub high_priority: bool,
    /// How many low-priority grants bypassed this waiter while parked
    /// (donor `npassed`; a reader grant does NOT reset other readers'
    /// counts).
    pub npassed: u32,
    /// Whether this is the reserved recovery waiter (the donor's
    /// `recovery_parked` lane: at most one exists, it always wins the next
    /// grant, and it is the only waiter admissible into a SICK parent).
    pub recovery: bool,
}

/// The health/waiter facts one admission or grant consumes. Every fact is
/// supplied: a missing health state refuses (fail closed — never invent a
/// healthy recovery), and the waiter list is the daemon's own data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthFacts<'a> {
    /// Whether the caller knows the parent object to be dead/shut down
    /// (donor `-ENODEV`), before any health observation is consulted.
    pub dead: bool,
    /// The parent's observed health, when the caller knows it. `None` is
    /// unknown: refuse, never guess healthy.
    pub state: Option<ParentHealth>,
    /// Whether a transfer is currently in flight on the parent (donor
    /// `inflight`).
    pub inflight: bool,
    /// The parent's owner-generation counter (donor `owner_gen`): admission
    /// mints `owner_generation + 1`.
    pub owner_generation: u64,
    /// The FIFO of parked waiters, oldest first.
    pub waiters: &'a [HealthWaiter],
}

/// The health-admission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthRequest {
    /// Whether the requesting traffic is the high-priority lane.
    pub high_priority: bool,
}

/// Why an admission was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthRefusal {
    /// The caller could not observe the parent's health: fail closed,
    /// never an invented healthy recovery (task 63: "Missing health/
    /// topology facts defer/refuse rather than inventing a healthy
    /// recovery").
    FactsMissing,
    /// The parent is SICK: the donor refuses admission with `-EIO` — never
    /// submit against a wedged master. Only the recovery lane may enter,
    /// and only through [`health_grant`].
    Sick,
    /// The parent object is dead/shut down (donor `-ENODEV`).
    Dead,
}

/// The health-admission decision (task 63; donor
/// `xcvr_health_admit_prepare`, safety.c:665-696, over supplied facts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionDecision {
    /// Admitted immediately: the slot was free with no queued waiters, and
    /// the caller's next owner generation is `generation + 1` (the donor
    /// mints `++owner_gen` and clears quarantine on take).
    Admit {
        /// The next owner generation for the admitted caller.
        generation: u64,
    },
    /// Park in FIFO at `position`. The caller supplies the grown waiter
    /// list back on the next call; a parked high-priority request at or
    /// beyond the fair bound is boosted by [`health_grant`].
    Wait {
        /// The FIFO index the waiter parks at.
        position: usize,
    },
    /// Refused with the typed reason.
    Refuse {
        /// Why the admission was refused.
        reason: HealthRefusal,
    },
}

/// Whether one submission may enter the physical parent now (task 63).
/// Pure over the supplied facts. Dead and SICK parents refuse, and a
/// missing health fact refuses — fail closed. A free slot with an empty
/// queue admits immediately with the next owner generation; an occupied
/// slot or a queued waiter always parks FIFO — the donor admits parked
/// waiters ONLY through the release-time grant ([`health_grant`]), so
/// parking here preserves FIFO order against the direct-take race.
pub fn health_admit(
    facts: &HealthFacts<'_>,
    _request: HealthRequest,
    _now: MonoTime,
) -> AdmissionDecision {
    if facts.dead {
        return AdmissionDecision::Refuse {
            reason: HealthRefusal::Dead,
        };
    }
    let Some(state) = facts.state else {
        return AdmissionDecision::Refuse {
            reason: HealthRefusal::FactsMissing,
        };
    };
    if state == ParentHealth::Sick {
        return AdmissionDecision::Refuse {
            reason: HealthRefusal::Sick,
        };
    }
    if !facts.inflight && facts.waiters.is_empty() {
        return AdmissionDecision::Admit {
            generation: facts.owner_generation + 1,
        };
    }
    AdmissionDecision::Wait {
        position: facts.waiters.len(),
    }
}

/// The donor's per-reader starvation bound (include/xcvr/health.h:76:
/// `#define XCVR_HEALTH_FAIR_BOUND 2 /* max low-prio grants past a parked
/// reader */`).
pub const HEALTH_FAIR_BOUND: u32 = 2;

/// What a release-time grant decided, as data for the daemon to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthGrant {
    /// The FIFO index of the waiter the slot is handed to, when one is
    /// granted (the donor's direct ownership handoff keeps `inflight` set).
    /// `None` means the slot simply went idle.
    pub grant: Option<usize>,
    /// The indexes of parked HIGH-priority waiters whose `npassed` the
    /// daemon must increment: the donor counts every low-priority grant
    /// that bypasses each parked high-prio reader, and a reader grant does
    /// NOT reset other readers' counts.
    pub bypassed: Vec<usize>,
}

/// Pick the waiter a freed slot goes to (task 63; donor
/// `xcvr_health_grant_next_locked`, safety.c:604-663). Pure over the
/// supplied waiter facts.
///
/// The reserved recovery waiter always wins the next grant (at most one
/// exists and each observation is bounded, so ordinary readers lose at
/// most one grant per recovery poll) and is the only lane admissible into
/// a SICK parent. Otherwise FIFO, with the donor's per-reader starvation
/// bound: when the FIFO head is not high-priority and the oldest parked
/// high-priority waiter's bypass count has reached
/// [`HEALTH_FAIR_BOUND`], that waiter is boosted instead — so no parked
/// reader ever observes more than the bound's low-priority grants. A
/// low-priority grant increments every parked high-priority waiter's
/// count (returned as `bypassed`). No waiters: the slot goes idle.
pub fn health_grant(facts: &HealthFacts<'_>) -> HealthGrant {
    // Recovery lane first, exactly as the donor scans for it.
    if let Some(index) = facts.waiters.iter().position(|w| w.recovery) {
        return HealthGrant {
            grant: Some(index),
            bypassed: Vec::new(),
        };
    }
    if facts.waiters.is_empty() {
        return HealthGrant {
            grant: None,
            bypassed: Vec::new(),
        };
    }
    let head = 0usize;
    let oldest_high = facts.waiters.iter().position(|w| w.high_priority);
    let mut pick = head;
    if !facts.waiters[head].high_priority {
        if let Some(high_index) = oldest_high {
            if facts.waiters[high_index].npassed >= HEALTH_FAIR_BOUND {
                pick = high_index;
            }
        }
    }
    // A low-priority grant bypasses every parked high-priority waiter.
    let bypassed = if !facts.waiters[pick].high_priority {
        facts
            .waiters
            .iter()
            .enumerate()
            .filter(|(index, waiter)| *index != pick && waiter.high_priority)
            .map(|(index, _)| index)
            .collect()
    } else {
        Vec::new()
    };
    HealthGrant {
        grant: Some(pick),
        bypassed,
    }
}

#[cfg(test)]
mod ranked_and_cdb_tests {
    use super::*;

    const GEN: ModuleGeneration = ModuleGeneration(7);
    const EPOCH: CacheEpoch = CacheEpoch(9);
    const NOW: MonoTime = MonoTime(1_000);

    fn request(operation_id: u64, sequence: u64) -> RequestMeta {
        RequestMeta {
            operation: OperationId(operation_id),
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

    fn ranked(
        requests: &[PrioritizedRequestMeta],
        knowledge: SelectorKnowledge,
    ) -> (ScheduleDecision, SchedulerState) {
        choose_next_ranked(
            requests,
            &SchedulerState::default(),
            &knowledge,
            NOW,
            &policy(),
        )
    }

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

    #[test]
    fn priority_ranks_before_fifo() {
        // A newer high-priority request runs before an older normal one.
        let requests = [
            prioritized(1, 10, RequestPriority::Normal),
            prioritized(2, 20, RequestPriority::High),
        ];
        let (decision, _) = ranked(&requests, knowledge());
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(2)
            }
        );
    }

    #[test]
    fn fifo_ties_hold_within_a_priority_class() {
        let requests = [
            prioritized(1, 20, RequestPriority::High),
            prioritized(2, 10, RequestPriority::High),
            prioritized(3, 5, RequestPriority::Normal),
        ];
        let (decision, _) = ranked(&requests, knowledge());
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(2)
            }
        );
    }

    #[test]
    fn a_high_priority_stale_request_retires_first() {
        let mut stale = prioritized(9, 1, RequestPriority::High);
        stale.meta.generation = ModuleGeneration(6);
        let requests = [stale, prioritized(1, 10, RequestPriority::Normal)];
        let knowledge = SelectorKnowledge {
            generation: GEN,
            epoch: EPOCH,
            selection: None,
        };
        let (decision, _) = choose_next_ranked(
            &requests,
            &SchedulerState::default(),
            &knowledge,
            NOW,
            &policy(),
        );
        assert_eq!(
            decision,
            ScheduleDecision::Retire {
                operation: OperationId(9),
                terminal: ScheduleTerminal::Stale {
                    generation: ModuleGeneration(6),
                    epoch: EPOCH,
                }
            }
        );
    }

    #[test]
    fn a_high_priority_deferral_never_blocks_a_runnable_normal_request() {
        let mut defer = prioritized(9, 1, RequestPriority::High);
        defer.meta.target = Some(Selection {
            address: 0x50,
            bank: 1,
            page: 0x11,
        });
        let requests = [defer, prioritized(1, 10, RequestPriority::Normal)];
        let (decision, _) = ranked(&requests, knowledge());
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(1)
            }
        );
    }

    #[test]
    fn empty_ranked_queue_is_idle() {
        let (decision, _) = ranked(&[], knowledge());
        assert_eq!(decision, ScheduleDecision::Idle);
    }

    #[test]
    fn cdb_admit_refuses_a_stale_stamp_before_anything_else() {
        let mut stale = request(1, 10);
        stale.generation = ModuleGeneration(6);
        let admission = cdb_admit(&stale, None, &knowledge(), NOW);
        assert_eq!(
            admission,
            CdbAdmission::Fence {
                reason: CdbFenceReason::Stale {
                    generation: ModuleGeneration(6),
                    epoch: EPOCH
                }
            }
        );
    }

    #[test]
    fn cdb_admit_fences_on_an_in_flight_transaction_even_same_generation() {
        let req = request(1, 10);
        // Same generation and epoch: still fenced — one CDB at a time.
        let in_flight = CdbInFlight {
            generation: GEN,
            epoch: EPOCH,
        };
        let admission = cdb_admit(&req, Some(&in_flight), &knowledge(), NOW);
        assert_eq!(
            admission,
            CdbAdmission::Fence {
                reason: CdbFenceReason::TransactionInFlight {
                    in_flight_generation: GEN,
                    in_flight_epoch: EPOCH
                }
            }
        );
    }

    #[test]
    fn cdb_admit_fences_a_passed_deadline_instead_of_deferring() {
        let mut late = request(1, 10);
        late.deadline = Some(MonoTime(NOW.0 - 1));
        let admission = cdb_admit(&late, None, &knowledge(), NOW);
        assert_eq!(
            admission,
            CdbAdmission::Fence {
                reason: CdbFenceReason::DeadlinePassed {
                    deadline: MonoTime(NOW.0 - 1),
                    at: NOW
                }
            }
        );
    }

    #[test]
    fn cdb_admit_defers_on_an_unconfirmed_selector_target() {
        let mut targeted = request(1, 10);
        targeted.target = Some(Selection {
            address: 0x50,
            bank: 1,
            page: 0x01,
        });
        let admission = cdb_admit(&targeted, None, &knowledge(), NOW);
        assert_eq!(
            admission,
            CdbAdmission::Defer(Deferral::SelectorUnconfirmed {
                target: Selection {
                    address: 0x50,
                    bank: 1,
                    page: 0x01,
                },
                confirmed: None
            })
        );
    }

    #[test]
    fn cdb_admit_admits_a_fresh_untargeted_request() {
        let req = request(1, 10);
        assert_eq!(
            cdb_admit(&req, None, &knowledge(), NOW),
            CdbAdmission::Admit
        );
    }

    #[test]
    fn anti_starvation_inversion_serves_a_runnable_normal_request_at_the_cap() {
        // At the donor cap with a runnable normal request, the high-priority
        // lane is demoted for one decision and the streak clears.
        let requests = [
            prioritized(1, 10, RequestPriority::Normal),
            prioritized(2, 20, RequestPriority::High),
        ];
        let at_cap = SchedulerState {
            high_priority_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
            ..SchedulerState::default()
        };
        let knowledge = knowledge();
        let (decision, state) = choose_next_ranked(&requests, &at_cap, &knowledge, NOW, &policy());
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(1)
            }
        );
        assert_eq!(state.high_priority_streak, 0);
    }

    #[test]
    fn below_the_cap_the_high_lane_ranks_first_and_counts_the_streak() {
        let requests = [
            prioritized(1, 10, RequestPriority::Normal),
            prioritized(2, 20, RequestPriority::High),
        ];
        let knowledge = knowledge();
        let (decision, state) = choose_next_ranked(
            &requests,
            &SchedulerState {
                high_priority_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP - 1,
                ..SchedulerState::default()
            },
            &knowledge,
            NOW,
            &policy(),
        );
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(2)
            }
        );
        assert_eq!(
            state.high_priority_streak,
            SchedulingPolicy::DONOR_ANTI_STARVE_CAP
        );
    }

    #[test]
    fn the_inversion_never_fires_without_a_runnable_normal_request() {
        // At the cap, but the only normal request defers (selector
        // unconfirmed): the high lane keeps ranking, nothing is inverted
        // onto a request that would serve nobody.
        let mut deferred = prioritized(1, 10, RequestPriority::Normal);
        deferred.meta.target = Some(Selection {
            address: 0x50,
            bank: 1,
            page: 0x11,
        });
        let requests = [deferred, prioritized(2, 20, RequestPriority::High)];
        let at_cap = SchedulerState {
            high_priority_streak: SchedulingPolicy::DONOR_ANTI_STARVE_CAP,
            ..SchedulerState::default()
        };
        let knowledge = knowledge();
        let (decision, state) = choose_next_ranked(&requests, &at_cap, &knowledge, NOW, &policy());
        assert_eq!(
            decision,
            ScheduleDecision::Run {
                operation: OperationId(2)
            }
        );
        // The high service still counts; the deferred normal did not clear it.
        assert_eq!(
            state.high_priority_streak,
            SchedulingPolicy::DONOR_ANTI_STARVE_CAP + 1
        );
    }

    #[test]
    fn a_normal_run_clears_the_streak_and_a_retirement_carries_it() {
        // A normal run clears.
        let requests = [prioritized(1, 10, RequestPriority::Normal)];
        let knowledge = knowledge();
        let (_, state) = choose_next_ranked(
            &requests,
            &SchedulerState {
                high_priority_streak: 9,
                ..SchedulerState::default()
            },
            &knowledge,
            NOW,
            &policy(),
        );
        assert_eq!(state.high_priority_streak, 0);
        // A retirement serves nobody and carries the streak.
        let mut stale = request(9, 1);
        stale.generation = ModuleGeneration(6);
        let (_, state) = choose_next(
            std::slice::from_ref(&stale),
            &SchedulerState {
                high_priority_streak: 9,
                ..SchedulerState::default()
            },
            &knowledge,
            NOW,
            &policy(),
        );
        assert_eq!(state.high_priority_streak, 9);
    }

    #[test]
    fn vector_yield_yields_only_segmentable_work_to_a_high_waiter() {
        let waiters_high = [YieldWaiter {
            priority: RequestPriority::High,
        }];
        // A segmentable bulk read with segments left yields, preserving
        // the cursor and re-queuing at the head.
        let running = YieldFacts {
            priority: RequestPriority::Normal,
            segmentable: true,
            remaining_segments: 3,
            segment_cursor: 7,
        };
        assert_eq!(
            vector_yield(&running, &waiters_high),
            VectorYieldDecision::Yield {
                resume_cursor: 7,
                requeue_head: true,
            }
        );
        // A scalar/RMW request never yields.
        let scalar = YieldFacts {
            priority: RequestPriority::Normal,
            segmentable: false,
            remaining_segments: 0,
            segment_cursor: 0,
        };
        assert_eq!(
            vector_yield(&scalar, &waiters_high),
            VectorYieldDecision::Continue { cursor: 0 }
        );
        // Nothing left to yield.
        let done = YieldFacts {
            priority: RequestPriority::Normal,
            segmentable: true,
            remaining_segments: 0,
            segment_cursor: 9,
        };
        assert_eq!(
            vector_yield(&done, &waiters_high),
            VectorYieldDecision::Continue { cursor: 9 }
        );
        // No higher-priority waiter: keep running.
        let normal_waiters = [YieldWaiter {
            priority: RequestPriority::Normal,
        }];
        assert_eq!(
            vector_yield(&running, &normal_waiters),
            VectorYieldDecision::Continue { cursor: 7 }
        );
        // A high-priority runner has no lane above it to yield to.
        let high_runner = YieldFacts {
            priority: RequestPriority::High,
            segmentable: true,
            remaining_segments: 2,
            segment_cursor: 1,
        };
        assert_eq!(
            vector_yield(&high_runner, &waiters_high),
            VectorYieldDecision::Continue { cursor: 1 }
        );
    }

    #[test]
    fn cdb_lifecycle_follows_the_donor_transitions() {
        use CdbEvent as E;
        use CdbTxnState as S;
        // Admit from either quiescent state.
        assert_eq!(
            cdb_advance(S::Idle, E::Admit),
            CdbAdvance::Moved(S::Admitted)
        );
        assert_eq!(
            cdb_advance(S::Done, E::Admit),
            CdbAdvance::Moved(S::Admitted)
        );
        // The staged/triggered/quarantine walk.
        assert_eq!(
            cdb_advance(S::Admitted, E::Staged),
            CdbAdvance::Moved(S::Staging)
        );
        assert_eq!(
            cdb_advance(S::Admitted, E::Triggered),
            CdbAdvance::Moved(S::Triggered)
        );
        assert_eq!(
            cdb_advance(S::Staging, E::Triggered),
            CdbAdvance::Moved(S::Triggered)
        );
        assert_eq!(
            cdb_advance(S::Staging, E::Quarantined),
            CdbAdvance::Moved(S::Quarantine)
        );
        assert_eq!(
            cdb_advance(S::Triggered, E::Quarantined),
            CdbAdvance::Moved(S::Quarantine)
        );
        // Illegal moves are rejected with the state unchanged: a double
        // admit, staging from the wrong state, a quarantined transaction
        // advancing again, and events on a quiescent transaction.
        assert_eq!(
            cdb_advance(S::Admitted, E::Admit),
            CdbAdvance::Illegal {
                from: S::Admitted,
                event: E::Admit
            }
        );
        assert_eq!(
            cdb_advance(S::Staging, E::Staged),
            CdbAdvance::Illegal {
                from: S::Staging,
                event: E::Staged
            }
        );
        assert_eq!(
            cdb_advance(S::Quarantine, E::Triggered),
            CdbAdvance::Illegal {
                from: S::Quarantine,
                event: E::Triggered
            }
        );
        assert_eq!(
            cdb_advance(S::Idle, E::Staged),
            CdbAdvance::Illegal {
                from: S::Idle,
                event: E::Staged
            }
        );
        // Terminals are reached only through finish-once.
        let (state, won) = cdb_finish_once(S::Triggered);
        assert_eq!((state, won), (S::Done, CdbFinish::Finished));
        // Exactly one caller wins: the second finisher loses and changes
        // nothing.
        assert_eq!(
            cdb_finish_once(state),
            (S::Done, CdbFinish::AlreadyFinished)
        );
        assert_eq!(
            cdb_finish_once(S::Quarantine),
            (S::Done, CdbFinish::Finished)
        );
        // Active: every in-flight state, including quarantine.
        assert!(S::Admitted.is_active());
        assert!(S::Quarantine.is_active());
        assert!(!S::Idle.is_active());
        assert!(!S::Done.is_active());
        // poll_defer: active gates monitoring, but an expired transaction
        // never suppresses it indefinitely.
        assert!(poll_defer(true, false));
        assert!(!poll_defer(true, true));
        assert!(!poll_defer(false, false));
        assert!(!poll_defer(false, true));
    }

    #[test]
    fn tokens_validate_only_under_their_exact_generation_and_epoch() {
        assert!(token_valid(GEN, EPOCH, &knowledge()));
        let bumped_epoch = SelectorKnowledge {
            generation: GEN,
            epoch: CacheEpoch(EPOCH.0 + 1),
            selection: None,
        };
        assert!(!token_valid(GEN, EPOCH, &bumped_epoch));
        let replaced = SelectorKnowledge {
            generation: ModuleGeneration(GEN.0 + 1),
            epoch: EPOCH,
            selection: None,
        };
        assert!(!token_valid(GEN, EPOCH, &replaced));
    }
}
