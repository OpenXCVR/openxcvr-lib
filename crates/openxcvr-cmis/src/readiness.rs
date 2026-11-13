//! Pure, internal-D4 CMIS **4.0** management readiness, not full Readiness/Power.
//!
//! CMIS 4.0 §6.3.1.6 permits ignored transactions during MgmtInit and delegates
//! initialization timing to the form-factor contract. There is no default policy.
//! §6.3.1.7 / §6.3.1.9 and Table 8-3 justify LowPwr/Ready as manageable; this
//! bounded policy does not recover faults or infer electrical/power qualification.
//! The caller owns transport admission, fresh observations, clock origin and I/O.
use crate::generated::{v4_0::CmisLowerMemory, CmisModuleState};
use openxcvr_core::{
    supplied::SpanError,
    types::management::{
        generation_valid, CacheEpoch, Freshness, ModuleGeneration, MonoDuration, MonoTime,
        ObservationStamp,
    },
    Rev,
};

/// Required finite nanosecond policy from the caller's source/configuration.
/// Nonzero cadence may not exceed the deadline span. No `Default` is provided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmis4ReadinessPolicy {
    deadline_span: MonoDuration,
    poll_interval: MonoDuration,
}
impl Cmis4ReadinessPolicy {
    /// Validate and construct a policy: nonzero span and interval, and the
    /// interval no longer than the span.
    pub fn new(
        deadline_span: MonoDuration,
        poll_interval: MonoDuration,
    ) -> Result<Self, Cmis4ReadinessError> {
        if deadline_span.0 == 0 || poll_interval.0 == 0 || poll_interval > deadline_span {
            return Err(Cmis4ReadinessError::InvalidPolicy);
        }
        Ok(Self {
            deadline_span,
            poll_interval,
        })
    }
}

/// Why a CMIS 4.0 readiness invocation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Cmis4ReadinessError {
    /// The policy's deadline span or poll interval is zero, or the interval
    /// exceeds the deadline span.
    #[error("readiness policy requires nonzero deadline span and poll interval, interval <= span")]
    InvalidPolicy,
    /// Only the revision this bounded policy models is admitted.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// A deadline or poll computation overflowed the monotonic clock.
    #[error("monotonic time overflow")]
    TimeOverflow,
    /// A supplied time is earlier than the anchor it must follow.
    #[error("reversed monotonic times")]
    ReversedTime,
    /// The observation's stamp is not fresh for this generation/epoch.
    #[error("stale observation")]
    StaleObservation,
    /// The supplied bytes do not cover the module-state field; the original
    /// span error is preserved as the source.
    #[error("supplied span does not cover the module state field")]
    Span(#[source] SpanError),
}

/// A transport fact, not an interpretation of adapter failure as module NACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmis4ReadinessTransportError {
    /// The MgmtInit/reset exchange was NACKed (expected transient).
    StartupNack,
    /// A retryable module exchange failure the caller classified.
    TransientError,
    /// Bridge continuity is uncertain.
    BridgeLost,
}

/// Only a supplied span is decoded; unobserved bytes are never zero-filled.
/// `TransientError` is a caller-classified retryable module exchange failure,
/// never adapter absence/failure. Uncertain bridge continuity is `BridgeLost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmis4ReadinessSample<'a> {
    /// Supplied lower-memory bytes with the window's first byte at `offset`.
    Supplied {
        /// The supplied window bytes.
        bytes: &'a [u8],
        /// The window's first byte offset.
        offset: usize,
    },
    /// The MgmtInit/reset exchange was NACKed.
    StartupNack,
    /// A caller-classified retryable exchange failure.
    TransientError,
    /// Bridge continuity is uncertain.
    BridgeLost,
}

/// One supplied observation with its validity stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmis4ReadinessObservation<'a> {
    /// The observation's provenance stamp.
    pub stamp: ObservationStamp,
    /// What the observation carries.
    pub sample: Cmis4ReadinessSample<'a>,
}

/// Instructions/facts only: no reads, writes, sleeps, selectors or clock access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmis4ReadinessOutcome {
    /// Poll again at `next_poll_at`, against the anchored `deadline`.
    PollAt {
        /// When to poll next.
        next_poll_at: MonoTime,
        /// The invocation's anchored deadline.
        deadline: MonoTime,
        /// The last accepted observation's state, when one decoded.
        last_state: Option<CmisModuleState>,
        /// The last accepted transport classification, when one exists.
        last_error: Option<Cmis4ReadinessTransportError>,
    },
    /// The module reports a manageable steady state.
    Manageable(CmisModuleState),
    /// The anchored deadline expired.
    Timeout {
        /// The deadline that expired.
        deadline: MonoTime,
        /// The last accepted observation's state, when one decoded.
        last_state: Option<CmisModuleState>,
        /// The last accepted transport classification, when one exists.
        last_error: Option<Cmis4ReadinessTransportError>,
    },
    /// Bridge continuity is uncertain; readiness stops.
    BridgeLost {
        /// The last accepted observation's state, when one decoded.
        last_state: Option<CmisModuleState>,
        /// The transport classification that lost the bridge.
        last_error: Cmis4ReadinessTransportError,
    },
}

/// One anchored invocation. Private anchors cannot be renewed by an arrival,
/// NACK, poll, or state change. Terminal outcomes remain terminal. This owns
/// no device, cache, clock or lifecycle; the existing consumer owner drives it.
#[derive(Debug, Clone)]
pub struct Cmis4Readiness {
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    invocation_at: MonoTime,
    deadline: MonoTime,
    policy: Cmis4ReadinessPolicy,
    last_state: Option<CmisModuleState>,
    last_error: Option<Cmis4ReadinessTransportError>,
    terminal: Option<Cmis4ReadinessOutcome>,
}
impl Cmis4Readiness {
    /// Start one anchored invocation; the deadline is fixed at
    /// `invocation_at + policy.deadline_span` and never renews.
    pub fn new(
        revision: Rev,
        generation: ModuleGeneration,
        epoch: CacheEpoch,
        invocation_at: MonoTime,
        policy: Cmis4ReadinessPolicy,
    ) -> Result<Self, Cmis4ReadinessError> {
        if revision != Rev::CMIS_4_0 {
            return Err(Cmis4ReadinessError::UnsupportedRevision(revision));
        }
        let deadline = invocation_at
            .checked_add(policy.deadline_span)
            .map_err(|_| Cmis4ReadinessError::TimeOverflow)?;
        Ok(Self {
            generation,
            epoch,
            invocation_at,
            deadline,
            policy,
            last_state: None,
            last_error: None,
            terminal: None,
        })
    }

    /// Reduce supplied facts at `now`; `None` permits expiry without I/O.
    /// An invalid observation returns an error without changing the reducer.
    /// All stamps must use this invocation's monotonic origin, generation and
    /// epoch. A valid late observation cannot rescue an expired invocation.
    /// At the exact deadline only LowPwr/Ready can succeed. Last state and
    /// last error are independent diagnostics (a state does not erase an error).
    pub fn step(
        &mut self,
        now: MonoTime,
        observation: Option<&Cmis4ReadinessObservation<'_>>,
    ) -> Result<Cmis4ReadinessOutcome, Cmis4ReadinessError> {
        now.checked_duration_since(self.invocation_at)
            .map_err(|_| Cmis4ReadinessError::ReversedTime)?;
        if let Some(terminal) = self.terminal {
            return Ok(terminal);
        }
        if let Some(observation) = observation {
            let stamp = &observation.stamp;
            stamp
                .completed_at
                .checked_duration_since(stamp.started_at)
                .map_err(|_| Cmis4ReadinessError::ReversedTime)?;
            if !generation_valid(stamp.generation, stamp.epoch, self.generation, self.epoch)
                || stamp.freshness != Freshness::Fresh
                || stamp.started_at < self.invocation_at
                || stamp.completed_at > now
            {
                return Err(Cmis4ReadinessError::StaleObservation);
            }
            if observation.sample == Cmis4ReadinessSample::BridgeLost {
                let outcome = Cmis4ReadinessOutcome::BridgeLost {
                    last_state: self.last_state,
                    last_error: Cmis4ReadinessTransportError::BridgeLost,
                };
                self.last_error = Some(Cmis4ReadinessTransportError::BridgeLost);
                self.terminal = Some(outcome);
                return Ok(outcome);
            }
        }
        // Late data is not decoded or published; keep the last in-budget facts.
        if now > self.deadline {
            return Ok(self.expire());
        }
        let mut last_state = self.last_state;
        let mut last_error = self.last_error;
        if let Some(observation) = observation {
            match observation.sample {
                Cmis4ReadinessSample::Supplied { bytes, offset } => {
                    let state = CmisLowerMemory::decode_module_state_supplied(bytes, offset)
                        .map_err(Cmis4ReadinessError::Span)?;
                    last_state = Some(state);
                    if matches!(
                        state,
                        CmisModuleState::ModuleLowPwr | CmisModuleState::ModuleReady
                    ) {
                        let outcome = Cmis4ReadinessOutcome::Manageable(state);
                        self.terminal = Some(outcome);
                        return Ok(outcome);
                    }
                }
                Cmis4ReadinessSample::StartupNack => {
                    last_error = Some(Cmis4ReadinessTransportError::StartupNack);
                }
                Cmis4ReadinessSample::TransientError => {
                    last_error = Some(Cmis4ReadinessTransportError::TransientError);
                }
                Cmis4ReadinessSample::BridgeLost => unreachable!("handled before expiry"),
            }
        }
        // Overflow refuses without publishing partially updated diagnostics.
        let next_poll_at = if now < self.deadline {
            Some(
                now.checked_add(self.policy.poll_interval)
                    .map_err(|_| Cmis4ReadinessError::TimeOverflow)?
                    .min(self.deadline),
            )
        } else {
            None
        };
        self.last_state = last_state;
        self.last_error = last_error;
        match next_poll_at {
            None => Ok(self.expire()),
            Some(next_poll_at) => Ok(Cmis4ReadinessOutcome::PollAt {
                next_poll_at,
                deadline: self.deadline,
                last_state: self.last_state,
                last_error: self.last_error,
            }),
        }
    }

    fn expire(&mut self) -> Cmis4ReadinessOutcome {
        let outcome = Cmis4ReadinessOutcome::Timeout {
            deadline: self.deadline,
            last_state: self.last_state,
            last_error: self.last_error,
        };
        self.terminal = Some(outcome);
        outcome
    }
}

// ── CMIS-family readiness (revision-generic) ──────────────────────────────
//
// Beyond the 4.0-pinned `Cmis4Readiness` above (whose d4 pins stay green),
// the family-generic reducer applies the same pure-readiness discipline to
// every catalog CMIS revision (3.0-5.4): the revision gate fails closed
// off-catalog, the observed state byte decodes through the revision's
// generated decode, and the host choices the standard does not set come
// from the machine schema's emitted host_policy constants (kmod's 50 ms
// poll cadence and 10 s readiness backstop, plugins/cmis/power.c:76/79) —
// no unexplained literals.
//
// The bounded subset keeps the donor's rules: ModuleLowPwr/ModuleReady are
// the manageable states; ModulePwrUp/ModulePwrDn and a startup (MgmtInit)
// NACK are transient; ModuleFault and reserved encodings are typed
// failures; the deadline anchors at the invocation start and extends only
// on a transient's first observation by the advertised duration plus
// margin, never on repeats, intervening NACKs or re-observation. What the
// full task-32 contract adds on the facade (ReadIntent/footprint
// validation, per-requester poll plans) stays owed there.

use crate::generated::fsm::cmis_module_state_machine;
use openxcvr_core::supplied::field_bytes;

/// Caller-supplied policy for one readiness invocation. Every value comes
/// from a named source: the defaults are the schema-emitted host-policy
/// constants, the margin is the caller's configuration, and the advertised
/// durations are the page 01h values the caller read — the library never
/// reads them itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmisReadinessPolicy {
    /// Poll cadence between non-terminal steps. The schema's default is
    /// [`cmis_module_state_machine::HOST_POLICY_POLL_CADENCE_MS`].
    pub poll_interval: MonoDuration,
    /// Host backstop for the whole walk, anchored at the invocation start.
    /// The schema's default is
    /// [`cmis_module_state_machine::HOST_POLICY_READINESS_BACKSTOP_MS`].
    pub backstop: MonoDuration,
    /// Caller-configured safety margin added to an advertised duration when
    /// a first-seen transient extends the deadline.
    pub margin: MonoDuration,
    /// The module's advertised maximum ModulePwrUp duration (page 01h),
    /// when the caller read it.
    pub advertised_pwr_up: Option<MonoDuration>,
    /// The module's advertised maximum ModulePwrDn duration (page 01h),
    /// when the caller read it.
    pub advertised_pwr_dn: Option<MonoDuration>,
}

impl CmisReadinessPolicy {
    /// The schema-default policy: the emitted host-policy constants plus a
    /// zero margin and no advertised durations. Callers that read the
    /// advertised page 01h durations supply them.
    pub fn schema_default() -> Self {
        Self {
            poll_interval: MonoDuration(cmis_module_state_machine::HOST_POLICY_POLL_CADENCE_MS),
            backstop: MonoDuration(cmis_module_state_machine::HOST_POLICY_READINESS_BACKSTOP_MS),
            margin: MonoDuration(0),
            advertised_pwr_up: None,
            advertised_pwr_dn: None,
        }
    }
}

/// Why a family-generic readiness invocation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CmisReadinessError {
    /// Only catalog CMIS revisions are admitted; every other context (a
    /// future revision, a foreign family) fails closed before any decode.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The policy's poll interval or backstop is zero, or the interval
    /// exceeds the backstop.
    #[error("readiness policy requires nonzero poll interval and backstop, interval <= backstop")]
    InvalidPolicy,
    /// A deadline or poll computation overflowed the monotonic clock.
    #[error("monotonic time overflow")]
    TimeOverflow,
    /// A supplied time is earlier than the anchor it must follow.
    #[error("reversed monotonic times")]
    ReversedTime,
    /// The observation's stamp is not fresh for this generation/epoch, or
    /// it predates the invocation start / postdates `now`.
    #[error("stale observation")]
    StaleObservation,
    /// The supplied bytes do not cover the module-state byte.
    #[error("supplied span does not cover the module state field")]
    Span(#[from] SpanError),
}

/// One observation offered to the reducer. `Supplied` carries the
/// lower-memory window containing byte 3 (the window's first byte at
/// `offset`); only a supplied span is decoded, unobserved bytes are never
/// zero-filled. `StartupNack` is the MgmtInit/reset NACK the caller
/// classified — an expected transient, never interpreted as module
/// failure. `TransientError` is a caller-classified retryable exchange
/// failure; `BridgeLost` is uncertain bridge continuity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmisReadinessSample<'a> {
    /// Supplied lower-memory bytes with the window's first byte at `offset`.
    Supplied {
        /// The supplied window bytes.
        bytes: &'a [u8],
        /// The window's first byte offset.
        offset: usize,
    },
    /// The MgmtInit/reset NACK the caller classified.
    StartupNack,
    /// A caller-classified retryable exchange failure.
    TransientError,
    /// Uncertain bridge continuity.
    BridgeLost,
}

/// One supplied observation with its validity stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmisReadinessObservation<'a> {
    /// The observation's provenance stamp.
    pub stamp: ObservationStamp,
    /// What the observation carries.
    pub sample: CmisReadinessSample<'a>,
}

/// What one step decided. Terminal decisions replay unchanged on later
/// steps. No decision performs or schedules I/O: `PollAt` names the next
/// poll time, and the caller owns every read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmisReadinessDecision {
    /// Continue polling at `at`, against the anchored `deadline`.
    PollAt {
        /// When to poll next.
        at: MonoTime,
        /// The invocation's anchored deadline.
        deadline: MonoTime,
    },
    /// The module reports a manageable steady state: LowPwr or Ready.
    Manageable {
        /// The manageable state the module reports.
        state: CmisModuleState,
    },
    /// The module reports a fault steady state.
    Fault {
        /// The fault state the module reports.
        state: CmisModuleState,
    },
    /// The module reports a reserved encoding the revision's decode does
    /// not name: a typed failure, never a borrowed nearest state.
    Reserved {
        /// The reserved raw code the module reported.
        raw: u8,
    },
    /// The anchored deadline expired. `last` is the last accepted
    /// observation's classification, when one was accepted.
    Timeout {
        /// The last accepted observation's state, when one decoded.
        last: Option<CmisModuleState>,
    },
    /// The bridge's continuity is uncertain; readiness stops.
    BridgeLost,
}

/// The readiness state the caller stores and supplies back each step.
/// Pure data: the library holds no resident instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmisReadinessState {
    /// The revision this invocation runs at.
    pub revision: Rev,
    /// The module generation the invocation runs under.
    pub generation: ModuleGeneration,
    /// The cache epoch the invocation runs under.
    pub epoch: CacheEpoch,
    /// The invocation's start; the deadline anchors here and is never
    /// renewed by arrivals, NACKs or state changes.
    /// The invocation's start time.
    pub started_at: MonoTime,
    /// The invocation's anchored deadline.
    pub deadline: MonoTime,
    /// First observation of each transient state; repeats never extend.
    /// First observation of ModulePwrUp; repeats never extend.
    pub first_seen_pwr_up: Option<MonoTime>,
    /// First observation of ModulePwrDn; repeats never extend.
    pub first_seen_pwr_dn: Option<MonoTime>,
    /// The last accepted observation's decoded state, when one decoded.
    pub last_state: Option<CmisModuleState>,
    /// The terminal decision, once reached; later steps replay it.
    pub terminal: Option<CmisReadinessDecision>,
}

/// The rules a readiness invocation's policy must satisfy, shared by
/// `readiness_begin` and every `readiness_step` call: nonzero poll
/// interval and backstop, interval <= backstop. `readiness_step` accepts
/// a fresh policy per call, so it revalidates with these same rules
/// instead of trusting the caller's begin-time validation.
fn policy_is_valid(policy: &CmisReadinessPolicy) -> bool {
    policy.poll_interval.0 != 0 && policy.backstop.0 != 0 && policy.poll_interval <= policy.backstop
}

/// Begin a readiness walk at `now` for a catalog CMIS revision. The
/// deadline anchors at the invocation start; a non-catalog revision fails
/// closed before anything is decoded.
pub fn readiness_begin(
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    now: MonoTime,
    policy: CmisReadinessPolicy,
) -> Result<CmisReadinessState, CmisReadinessError> {
    if cmis_module_state_machine::for_revision(revision).is_none() {
        return Err(CmisReadinessError::UnsupportedRevision(revision));
    }
    if !policy_is_valid(&policy) {
        return Err(CmisReadinessError::InvalidPolicy);
    }
    let deadline = now
        .checked_add(policy.backstop)
        .map_err(|_| CmisReadinessError::TimeOverflow)?;
    Ok(CmisReadinessState {
        revision,
        generation,
        epoch,
        started_at: now,
        deadline,
        first_seen_pwr_up: None,
        first_seen_pwr_dn: None,
        last_state: None,
        terminal: None,
    })
}

/// Reduce one supplied observation at `now`. Pure: the returned state and
/// decision are data; the caller stores the state and executes nothing
/// through the library. A valid late observation cannot rescue an expired
/// deadline, and a terminal decision replays unchanged.
///
/// Fail closed on the supplied state and policy at EVERY step, before the
/// terminal replay: `previous` is caller-restored data, so its revision is
/// revalidated against the catalog (the same predicate as
/// [`readiness_begin`]) — a tampered or foreign revision context is
/// refused with [`CmisReadinessError::UnsupportedRevision`], never decoded
/// through the revision-blind generated `CmisModuleState::from_rev` — and
/// the per-call `policy` is revalidated with the same rules as
/// [`readiness_begin`] (a zero poll interval would schedule
/// `PollAt { at: now }` and spin the caller).
pub fn readiness_step(
    previous: &CmisReadinessState,
    observation: Option<&CmisReadinessObservation<'_>>,
    now: MonoTime,
    policy: CmisReadinessPolicy,
) -> Result<(CmisReadinessState, CmisReadinessDecision), CmisReadinessError> {
    if cmis_module_state_machine::for_revision(previous.revision).is_none() {
        return Err(CmisReadinessError::UnsupportedRevision(previous.revision));
    }
    if !policy_is_valid(&policy) {
        return Err(CmisReadinessError::InvalidPolicy);
    }
    now.checked_duration_since(previous.started_at)
        .map_err(|_| CmisReadinessError::ReversedTime)?;
    if let Some(terminal) = previous.terminal {
        return Ok((*previous, terminal));
    }
    let mut state = *previous;
    if let Some(observation) = observation {
        let stamp = &observation.stamp;
        stamp
            .completed_at
            .checked_duration_since(stamp.started_at)
            .map_err(|_| CmisReadinessError::ReversedTime)?;
        if !generation_valid(
            stamp.generation,
            stamp.epoch,
            previous.generation,
            previous.epoch,
        ) || stamp.freshness != Freshness::Fresh
            || stamp.started_at < previous.started_at
            || stamp.completed_at > now
        {
            return Err(CmisReadinessError::StaleObservation);
        }
    }
    // A late observation is not decoded or published.
    if now > previous.deadline {
        let decision = CmisReadinessDecision::Timeout {
            last: previous.last_state,
        };
        state.terminal = Some(decision);
        return Ok((state, decision));
    }
    let decision = match observation.map(|o| o.sample) {
        Some(CmisReadinessSample::Supplied { bytes, offset }) => {
            let byte = field_bytes(bytes, offset, usize::from(MODULE_STATE_OFFSET), 1)?[0];
            let field = (byte >> 1) & 0x07;
            // Every step revalidated the state's revision against the
            // catalog at entry, so this packed byte is the revision's own
            // BCD encoding.
            let rev_byte = (previous.revision.major << 4) | previous.revision.minor;
            let decoded = CmisModuleState::from_rev(rev_byte, field);
            match decoded {
                CmisModuleState::ModuleLowPwr | CmisModuleState::ModuleReady => {
                    CmisReadinessDecision::Manageable { state: decoded }
                }
                CmisModuleState::ModuleFault => CmisReadinessDecision::Fault {
                    state: CmisModuleState::ModuleFault,
                },
                transient @ (CmisModuleState::ModulePwrUp | CmisModuleState::ModulePwrDn) => {
                    record_transient(&mut state, transient, now, policy)?;
                    poll_decision(&state, now, policy)?
                }
                other => CmisReadinessDecision::Reserved {
                    raw: match other {
                        CmisModuleState::Reserved0 => 0,
                        CmisModuleState::Unknown(raw) => raw,
                        _ => field,
                    },
                },
            }
        }
        Some(CmisReadinessSample::StartupNack) | Some(CmisReadinessSample::TransientError) => {
            // An expected MgmtInit/reset NACK or a retryable error is
            // transient; it never renews any anchor.
            poll_decision(&state, now, policy)?
        }
        Some(CmisReadinessSample::BridgeLost) => CmisReadinessDecision::BridgeLost,
        None => poll_decision(&state, now, policy)?,
    };
    // Terminal decisions latch; transient ones name the next poll.
    if !matches!(decision, CmisReadinessDecision::PollAt { .. }) {
        state.terminal = Some(decision);
    }
    if let Some(CmisReadinessSample::Supplied { bytes, offset }) = observation.map(|o| o.sample) {
        let byte = field_bytes(bytes, offset, usize::from(MODULE_STATE_OFFSET), 1)?[0];
        let rev_byte = (previous.revision.major << 4) | previous.revision.minor;
        state.last_state = Some(CmisModuleState::from_rev(rev_byte, (byte >> 1) & 0x07));
    }
    Ok((state, decision))
}

/// Record a transient's first observation and extend the deadline by the
/// advertised duration plus margin — only if that lands later than the
/// anchored deadline. Repeats and re-observation never extend.
fn record_transient(
    state: &mut CmisReadinessState,
    transient: CmisModuleState,
    now: MonoTime,
    policy: CmisReadinessPolicy,
) -> Result<(), CmisReadinessError> {
    let (seen, advertised) = match transient {
        CmisModuleState::ModulePwrUp => (&mut state.first_seen_pwr_up, policy.advertised_pwr_up),
        CmisModuleState::ModulePwrDn => (&mut state.first_seen_pwr_dn, policy.advertised_pwr_dn),
        _ => return Ok(()),
    };
    if seen.is_none() {
        *seen = Some(now);
        if let Some(advertised) = advertised {
            let extension = now
                .checked_add(advertised)
                .and_then(|at| at.checked_add(policy.margin))
                .map_err(|_| CmisReadinessError::TimeOverflow)?;
            if extension > state.deadline {
                state.deadline = extension;
            }
        }
    }
    Ok(())
}

/// The next poll time: `now + cadence`, clamped to the deadline; at the
/// deadline exactly, the walk is out of time rather than scheduled.
fn poll_decision(
    state: &CmisReadinessState,
    now: MonoTime,
    policy: CmisReadinessPolicy,
) -> Result<CmisReadinessDecision, CmisReadinessError> {
    if now >= state.deadline {
        return Ok(CmisReadinessDecision::Timeout {
            last: state.last_state,
        });
    }
    let at = now
        .checked_add(policy.poll_interval)
        .map_err(|_| CmisReadinessError::TimeOverflow)?
        .min(state.deadline);
    Ok(CmisReadinessDecision::PollAt {
        at,
        deadline: state.deadline,
    })
}

/// Lower-memory byte 3, the Module State register (CMIS 5.3 Table 8-5);
/// bits 3:1 carry the state.
const MODULE_STATE_OFFSET: u8 = 3;

#[cfg(test)]
mod family_readiness_tests {
    use super::*;
    use openxcvr_core::Family;

    fn stamp(generation: ModuleGeneration, epoch: CacheEpoch, at: MonoTime) -> ObservationStamp {
        ObservationStamp {
            started_at: at,
            completed_at: at,
            generation,
            epoch,
            source: openxcvr_core::types::management::ObservationSource::Supplied,
            freshness: Freshness::Fresh,
            consumed_effects: vec![],
        }
    }

    fn begin(rev: Rev, at: MonoTime) -> Result<CmisReadinessState, CmisReadinessError> {
        readiness_begin(
            rev,
            ModuleGeneration(1),
            CacheEpoch(1),
            at,
            CmisReadinessPolicy::schema_default(),
        )
    }

    fn supplied<'a>(bytes: &'a [u8], at: MonoTime) -> CmisReadinessObservation<'a> {
        CmisReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), at),
            sample: CmisReadinessSample::Supplied { bytes, offset: 0 },
        }
    }

    /// Byte 3 carrying `field` in bits 3:1 with the interrupt bit clear.
    fn byte3(field: u8) -> [u8; 4] {
        [0, 0, 0, field << 1]
    }

    #[test]
    fn every_catalog_revision_is_admitted_and_decodes_its_own_states() {
        let now = MonoTime(1_000);
        for rev in [
            Rev::CMIS_3_0,
            Rev::CMIS_4_0,
            Rev::CMIS_5_0,
            Rev::CMIS_5_1,
            Rev::CMIS_5_2,
            Rev::CMIS_5_3,
            Rev::CMIS_5_4,
        ] {
            let state = begin(rev, now).expect("catalog revision is admitted");
            let bytes = byte3(3); // ModuleReady
            let (state, decision) = readiness_step(
                &state,
                Some(&supplied(&bytes, now)),
                now,
                CmisReadinessPolicy::schema_default(),
            )
            .expect("step succeeds");
            assert_eq!(
                decision,
                CmisReadinessDecision::Manageable {
                    state: CmisModuleState::ModuleReady
                },
                "{rev:?}"
            );
            // Terminal decisions replay unchanged.
            let (state2, decision2) = readiness_step(
                &state,
                None,
                MonoTime(2_000),
                CmisReadinessPolicy::schema_default(),
            )
            .expect("step succeeds");
            assert_eq!(decision2, decision);
            assert_eq!(state2, state);
        }
    }

    #[test]
    fn off_catalog_revisions_fail_closed_before_anything() {
        let rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        assert_eq!(
            begin(rev, MonoTime(1_000)).unwrap_err(),
            CmisReadinessError::UnsupportedRevision(rev)
        );
        let sff = Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        };
        assert_eq!(
            begin(sff, MonoTime(1_000)).unwrap_err(),
            CmisReadinessError::UnsupportedRevision(sff)
        );
    }

    /// `readiness_step` revalidates the caller-restored state's revision at
    /// every step — before the terminal replay — because the generated
    /// `CmisModuleState::from_rev` does not bind the revision and would
    /// otherwise decode a tampered 5.5 state's ModuleReady byte straight
    /// to `Manageable` (review blocker 2).
    #[test]
    fn readiness_step_refuses_a_tampered_non_catalog_state() {
        let now = MonoTime(1_000);
        let tampered_rev = Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        };
        // Tampered mid-walk state: revision swapped after a valid begin.
        let mut tampered = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        tampered.revision = tampered_rev;
        let ready_bytes = byte3(3); // ModuleReady
        assert_eq!(
            readiness_step(
                &tampered,
                Some(&supplied(&ready_bytes, now)),
                now,
                CmisReadinessPolicy::schema_default(),
            )
            .unwrap_err(),
            CmisReadinessError::UnsupportedRevision(tampered_rev)
        );
        // ...and before terminal replay: a latched terminal never launders
        // the tampered revision through.
        let mut replay = tampered;
        replay.terminal = Some(CmisReadinessDecision::Manageable {
            state: CmisModuleState::ModuleReady,
        });
        assert_eq!(
            readiness_step(
                &replay,
                None,
                MonoTime(2_000),
                CmisReadinessPolicy::schema_default(),
            )
            .unwrap_err(),
            CmisReadinessError::UnsupportedRevision(tampered_rev)
        );
    }

    /// `readiness_step` revalidates the per-call policy with the same
    /// rules as `readiness_begin`: a zero poll interval would schedule
    /// `PollAt { at: now }` and spin the caller (review advisory).
    #[test]
    fn readiness_step_rejects_an_invalid_per_call_policy() {
        let now = MonoTime(1_000);
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        let mut zero_interval = CmisReadinessPolicy::schema_default();
        zero_interval.poll_interval = MonoDuration(0);
        assert_eq!(
            readiness_step(&state, None, now, zero_interval).unwrap_err(),
            CmisReadinessError::InvalidPolicy
        );
        let mut inverted = CmisReadinessPolicy::schema_default();
        inverted.backstop = MonoDuration(1);
        assert_eq!(
            readiness_step(&state, None, now, inverted).unwrap_err(),
            CmisReadinessError::InvalidPolicy
        );
    }

    #[test]
    fn transient_states_poll_and_extend_only_on_first_sight() {
        let now = MonoTime(1_000);
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        // PwrUp observed: first sight extends by the advertised duration
        // (20 s advertised + 0 margin, later than the anchored 10 s
        // backstop, so the deadline moves).
        let bytes = byte3(2);
        let mut policy = CmisReadinessPolicy::schema_default();
        policy.advertised_pwr_up = Some(MonoDuration(20_000));
        let (state, decision) =
            readiness_step(&state, Some(&supplied(&bytes, now)), now, policy).expect("step");
        assert_eq!(
            decision,
            CmisReadinessDecision::PollAt {
                at: MonoTime(1_050),
                deadline: MonoTime(21_000), // start + 20 s advertised extension
            }
        );
        assert_eq!(state.first_seen_pwr_up, Some(now));
        // A repeat observation never extends the anchor.
        let later = MonoTime(1_050);
        let (state, decision) =
            readiness_step(&state, Some(&supplied(&bytes, later)), later, policy).expect("step");
        assert_eq!(
            decision,
            CmisReadinessDecision::PollAt {
                at: MonoTime(1_100),
                deadline: MonoTime(21_000),
            }
        );
        assert_eq!(state.first_seen_pwr_up, Some(now));
        // Without an advertised duration, the anchored deadline stands: a
        // small advertised value never shortens it either.
        let plain = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        let (plain, decision) = readiness_step(
            &plain,
            Some(&supplied(&bytes, now)),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(
            decision,
            CmisReadinessDecision::PollAt {
                at: MonoTime(1_050),
                deadline: MonoTime(11_000), // the anchored 10 s backstop, never shortened
            }
        );
        assert_eq!(plain.first_seen_pwr_up, Some(now));
    }

    #[test]
    fn nack_and_transient_errors_are_transient_faults_and_reserved_are_typed() {
        let now = MonoTime(1_000);
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        // A startup NACK polls on, never renews the anchor, never reads as
        // a module fault.
        let nack = CmisReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: CmisReadinessSample::StartupNack,
        };
        let (state, decision) = readiness_step(
            &state,
            Some(&nack),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert!(matches!(decision, CmisReadinessDecision::PollAt { .. }));
        assert_eq!(state.deadline, MonoTime(11_000));
        // ModuleFault is a typed fault.
        let fault_bytes = byte3(5);
        let fault = supplied(&fault_bytes, now);
        let (_, decision) = readiness_step(
            &state,
            Some(&fault),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(
            decision,
            CmisReadinessDecision::Fault {
                state: CmisModuleState::ModuleFault
            }
        );
        // A reserved encoding is a typed failure, never a borrowed state.
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        let reserved_bytes = byte3(6);
        let reserved = supplied(&reserved_bytes, now);
        let (_, decision) = readiness_step(
            &state,
            Some(&reserved),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(decision, CmisReadinessDecision::Reserved { raw: 6 });
        // Bridge loss is terminal.
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        let lost = CmisReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: CmisReadinessSample::BridgeLost,
        };
        let (_, decision) = readiness_step(
            &state,
            Some(&lost),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(decision, CmisReadinessDecision::BridgeLost);
    }

    #[test]
    fn expiry_is_typed_and_late_observations_never_rescue_it() {
        let now = MonoTime(1_000);
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        // Step at the deadline: out of time.
        let (state, decision) = readiness_step(
            &state,
            None,
            MonoTime(11_000),
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(decision, CmisReadinessDecision::Timeout { last: None });
        // A late manageable observation cannot rescue the expired walk.
        let ready_bytes = byte3(3);
        let ready = supplied(&ready_bytes, MonoTime(11_001));
        let (_, decision) = readiness_step(
            &state,
            Some(&ready),
            MonoTime(11_002),
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert_eq!(decision, CmisReadinessDecision::Timeout { last: None });
    }

    #[test]
    fn stale_observations_are_refused_without_changing_state() {
        let now = MonoTime(1_000);
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        // Wrong generation.
        let wrong_bytes = byte3(3);
        let mut wrong = supplied(&wrong_bytes, now);
        wrong.stamp.generation = ModuleGeneration(2);
        let err = readiness_step(
            &state,
            Some(&wrong),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .unwrap_err();
        assert_eq!(err, CmisReadinessError::StaleObservation);
        // Non-fresh.
        let stale_bytes = byte3(3);
        let mut stale = supplied(&stale_bytes, now);
        stale.stamp.freshness = Freshness::Historical;
        assert_eq!(
            readiness_step(
                &state,
                Some(&stale),
                now,
                CmisReadinessPolicy::schema_default()
            )
            .unwrap_err(),
            CmisReadinessError::StaleObservation
        );
        // Postdates now.
        let future_bytes = byte3(3);
        let mut future = supplied(&future_bytes, MonoTime(2_000));
        future.stamp.started_at = MonoTime(2_000);
        future.stamp.completed_at = MonoTime(2_000);
        assert_eq!(
            readiness_step(
                &state,
                Some(&future),
                now,
                CmisReadinessPolicy::schema_default()
            )
            .unwrap_err(),
            CmisReadinessError::StaleObservation
        );
        // A span that does not cover byte 3 is a typed span error.
        let short = CmisReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: CmisReadinessSample::Supplied {
                bytes: &[0u8; 2],
                offset: 0,
            },
        };
        assert!(matches!(
            readiness_step(
                &state,
                Some(&short),
                now,
                CmisReadinessPolicy::schema_default()
            ),
            Err(CmisReadinessError::Span(_))
        ));
        // Reversed time refuses.
        assert_eq!(
            readiness_step(
                &state,
                None,
                MonoTime(1),
                CmisReadinessPolicy::schema_default()
            )
            .unwrap_err(),
            CmisReadinessError::ReversedTime
        );
        // Zero-cadence and inverted policies are invalid.
        let mut bad = CmisReadinessPolicy::schema_default();
        bad.poll_interval = MonoDuration(0);
        assert_eq!(
            readiness_begin(Rev::CMIS_5_3, ModuleGeneration(1), CacheEpoch(1), now, bad)
                .unwrap_err(),
            CmisReadinessError::InvalidPolicy
        );
        let mut inverted = CmisReadinessPolicy::schema_default();
        inverted.backstop = MonoDuration(1);
        assert_eq!(
            readiness_begin(
                Rev::CMIS_5_3,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                inverted
            )
            .unwrap_err(),
            CmisReadinessError::InvalidPolicy
        );
    }

    #[test]
    fn low_power_is_manageable_at_every_catalog_revision() {
        let now = MonoTime(1_000);
        let bytes = byte3(1);
        for rev in [Rev::CMIS_3_0, Rev::CMIS_4_0, Rev::CMIS_5_4] {
            let state = begin(rev, now).expect("catalog revision");
            let (_, decision) = readiness_step(
                &state,
                Some(&supplied(&bytes, now)),
                now,
                CmisReadinessPolicy::schema_default(),
            )
            .expect("step");
            assert_eq!(
                decision,
                CmisReadinessDecision::Manageable {
                    state: CmisModuleState::ModuleLowPwr
                },
                "{rev:?}"
            );
        }
        // The window offset is honoured: byte 3 anywhere in the span.
        // The supplied window starts at page offset 1, so byte 3 lives at
        // window index 2 (field_offset - supplied_offset).
        let window = [0x00, 0x00, 0x04, 0x00, 0x00, 0x00]; // PwrUp<<1
        let observation = CmisReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: CmisReadinessSample::Supplied {
                bytes: &window,
                offset: 1,
            },
        };
        let state = begin(Rev::CMIS_5_3, now).expect("catalog revision");
        let (state, decision) = readiness_step(
            &state,
            Some(&observation),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("step");
        assert!(matches!(decision, CmisReadinessDecision::PollAt { .. }));
        assert_eq!(state.last_state, Some(CmisModuleState::ModulePwrUp));
    }
}
