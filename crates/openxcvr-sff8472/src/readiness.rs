//! Pure SFF-8472 A2h monitor-data readiness: a time-bounded poll plan over
//! caller-supplied observations, mirroring the CMIS
//! `readiness_begin`/`readiness_step` shape.
//!
//! What the specification defines (SFF-8472 v12.5a § 9.8, Table 9-16 and
//! its note): the `Data_Not_Ready` bit (A2h byte 110, bit 0) "is 1 during
//! module power up and prior to the first valid A/D reading. Once the first
//! valid A/D reading occurs, the bit is set to 0 until the device is
//! powered down. The bit must be set '0' within 1 second of power up"
//! (Table 8-7 `t_data`, 1000 ms max). That is a monotone
//! ready-for-management predicate with a spec-bounded settle time — no
//! startup state machine, no interrupt handshake, and no read effect on
//! byte 110 (nothing in the spec's description of the bit or its note
//! changes module state on read). Table 8-7 also bounds the 2-wire bus
//! hardware ready time (`t_serial`, 300 ms from power on) — a
//! caller transport-admission fact, not part of this walk.
//!
//! The A2h diagnostic surface itself is optional: § 8.8 (Table 8-5, A0h
//! byte 92 bit 6) makes digital diagnostic monitoring an implemented/not
//! capability. A module without it has no A2h ready sequencing to model,
//! so [`readiness_begin`] refuses with
//! [`Sff8472ReadinessError::DiagnosticsNotImplemented`] instead of
//! inventing a walk. § 8.9 may additionally require the address-change
//! sequence before A2h access — the caller's transport concern; this
//! library performs no I/O. The caller performs every read, supplies each
//! observation with a validity stamp, and stores the returned state
//! between steps.

use openxcvr_core::field_catalog::{resolve, CatalogError, FieldQuery, LaneSelection};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::supplied::{field_bytes, SpanError};
use openxcvr_core::types::management::{
    generation_valid, CacheEpoch, Freshness, ModuleGeneration, MonoDuration, MonoTime,
    ObservationStamp, ReadEffect,
};

use crate::generated::catalog::CATALOG;

/// Table 8-7 `t_serial`: 300 ms max from power on until the module is
/// ready for data transmission over the 2-wire bus. A caller
/// transport-admission fact; the readiness walk does not enforce it.
pub const T_SERIAL_MAX_MS: u64 = 300;

/// Table 8-7 `t_data`: 1000 ms max "from power on to data ready" — § 9.8:
/// "The bit must be set '0' within 1 second of power up." The spec anchors
/// this bound at power on; a walk anchored at its own (later) start is a
/// conservative superset, since a walk cannot begin before power on.
pub const T_DATA_MAX_MS: u64 = 1_000;

/// A2h byte 110, the optional status/control bits byte whose bit 0 is
/// `Data_Not_Ready` — SFF-8472 v12.5a § 9.8, Table 9-16. The readiness
/// walk reads exactly this byte of the A2h base page.
const STATUS_BYTE_OFFSET: usize = 110;

/// Caller-supplied policy for one readiness invocation. The deadline span
/// defaults from the spec's `t_data` bound via [`Self::for_t_data`]; the
/// poll cadence has no spec value and must come from the caller's
/// configuration. No `Default` is provided: a policy must be chosen, not
/// assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8472ReadinessPolicy {
    /// Poll cadence between non-terminal steps. Must be nonzero and not
    /// exceed `data_ready_span`.
    pub poll_interval: MonoDuration,
    /// Host backstop for the whole walk, anchored at the invocation start.
    /// Must be nonzero.
    pub data_ready_span: MonoDuration,
}

impl Sff8472ReadinessPolicy {
    /// The spec-derived policy: the Table 8-7 `t_data` bound
    /// ([`T_DATA_MAX_MS`]) as the walk-anchored span. Conservative against
    /// the power-on-anchored bound (the walk starts at or after power on);
    /// a caller that knows the power-on time may compute a tighter span.
    pub fn for_t_data(poll_interval: MonoDuration) -> Self {
        Self {
            poll_interval,
            data_ready_span: MonoDuration(T_DATA_MAX_MS),
        }
    }
}

/// Why an SFF-8472 readiness invocation was refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Sff8472ReadinessError {
    /// Only catalog SFF-8472 revisions are admitted; every other context
    /// (a future revision, a foreign family) fails closed before any
    /// decode.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The module does not implement digital diagnostic monitoring
    /// (§ 8.8, Table 8-5: A0h byte 92 bit 6 clear), so the spec defines
    /// no A2h ready sequencing for it — there is nothing to poll.
    #[error("digital diagnostic monitoring not implemented: no A2h readiness to walk")]
    DiagnosticsNotImplemented,
    /// The policy's poll interval or data-ready span is zero, or the
    /// interval exceeds the span.
    #[error(
        "readiness policy requires nonzero poll interval and data ready span, interval <= span"
    )]
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
    /// The supplied bytes do not cover the status byte (A2h:110).
    #[error("supplied span does not cover the status byte")]
    Span(#[from] SpanError),
    /// The schema catalog could not resolve the status-byte field at the
    /// walk's revision: reported, never bypassed with a hand-built plan.
    #[error("catalog resolution failed: {0}")]
    Catalog(#[from] CatalogError),
}

/// One observation offered to the reducer. `Supplied` carries the A2h
/// base-page window containing byte 110 (the window's first byte at
/// `offset`); only a supplied span is decoded, unobserved bytes are never
/// zero-filled. `TransientError` is a caller-classified retryable exchange
/// failure, never adapter absence; uncertain bridge continuity is
/// `BridgeLost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sff8472ReadinessSample<'a> {
    /// A supplied A2h base-page window and the page offset its first byte
    /// came from.
    Supplied {
        /// The supplied A2h base-page window.
        bytes: &'a [u8],
        /// The page offset the window's first byte came from.
        offset: usize,
    },
    /// A caller-classified retryable exchange failure: transient, it never
    /// renews any anchor.
    TransientError,
    /// The bridge's continuity is uncertain; readiness stops.
    BridgeLost,
}

/// One supplied observation with its validity stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sff8472ReadinessObservation<'a> {
    /// The observation's validity stamp.
    pub stamp: ObservationStamp,
    /// What was observed.
    pub sample: Sff8472ReadinessSample<'a>,
}

/// A planned read described as data. The library never performs it; the
/// caller reads `len` bytes at `offset` in the page window at 2-wire
/// `address` and offers the result as the next `Supplied` observation.
/// `read_effects` is the schema-recorded effect of that read, resolved
/// from the generated catalog — empty today: the spec defines no read
/// effect on byte 110 (§ 9.8 describes a live status bit, not a latched
/// or acknowledged one), so the poll is side-effect-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8472PlannedRead {
    /// Seven-bit 2-wire address of the page window (A2h: 0x51).
    pub address: u8,
    /// Byte offset of the status byte within the page window.
    pub offset: u8,
    /// Length to read in bytes.
    pub len: u16,
    /// Schema-recorded read effects of this read.
    pub read_effects: &'static [ReadEffect],
}

/// What one step decided. Terminal decisions replay unchanged on later
/// steps. No decision performs or schedules I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sff8472ReadinessDecision {
    /// Continue polling: read the planned byte at/after `at`, against the
    /// anchored `deadline`.
    PollAt {
        /// The earliest time to perform the planned read.
        at: MonoTime,
        /// The anchored deadline the walk expires at.
        deadline: MonoTime,
        /// The read to perform, as data.
        read: Sff8472PlannedRead,
    },
    /// The module has valid monitor data (`Data_Not_Ready` low): the
    /// § 9.8 ready-for-management condition.
    Ready,
    /// The anchored deadline expired. `last` is the last accepted
    /// observation's `Data_Not_Ready` value, when one was accepted.
    Timeout {
        /// The last accepted observation's `Data_Not_Ready` value, when
        /// one was accepted.
        last: Option<bool>,
    },
    /// The bridge's continuity is uncertain; readiness stops.
    BridgeLost,
}

/// The readiness state the caller stores and supplies back each step.
/// Pure data: the library holds no resident instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8472ReadinessState {
    /// The walk's revision; revalidated against the catalog at every step.
    pub revision: Rev,
    /// The invocation's module generation; observations must match it.
    pub generation: ModuleGeneration,
    /// The invocation's cache epoch; observations must match it.
    pub epoch: CacheEpoch,
    /// The invocation's start; the deadline anchors here and is never
    /// renewed by arrivals, errors, or bit changes.
    pub started_at: MonoTime,
    /// The anchored expiry time.
    pub deadline: MonoTime,
    /// The last accepted observation's `Data_Not_Ready` value, when one
    /// was accepted.
    pub last: Option<bool>,
    /// The terminal decision, once reached; later steps replay it.
    pub terminal: Option<Sff8472ReadinessDecision>,
}

/// The read the walk plans at `revision`, resolved from the schema
/// catalog's `data_not_ready` row. Pure data: callers use it for the
/// first read before the first step and for every poll in between.
pub fn planned_read(revision: Rev) -> Result<Sff8472PlannedRead, Sff8472ReadinessError> {
    let entries = resolve(
        &CATALOG,
        revision,
        &FieldQuery {
            pattern: "data_not_ready",
            lane: LaneSelection::Unspecified,
            page: Some("A2h"),
            bank: 0,
        },
    )?;
    let row = entries
        .first()
        .ok_or_else(|| CatalogError::NoMatch {
            pattern: "data_not_ready".to_string(),
            rev: revision,
        })?
        .row;
    Ok(Sff8472PlannedRead {
        address: row.address,
        offset: u8::try_from(row.offset).map_err(|_| CatalogError::NoMatch {
            pattern: "data_not_ready".to_string(),
            rev: revision,
        })?,
        len: row.len,
        read_effects: row.read_effects,
    })
}

/// The rules a readiness policy must satisfy, shared by
/// [`readiness_begin`] and every [`readiness_step`] call (a zero poll
/// interval would schedule `PollAt { at: now }` and spin the caller).
fn policy_is_valid(policy: &Sff8472ReadinessPolicy) -> bool {
    policy.poll_interval.0 != 0
        && policy.data_ready_span.0 != 0
        && policy.poll_interval <= policy.data_ready_span
}

/// Begin a readiness walk at `now` for a catalog SFF-8472 revision whose
/// module implements digital diagnostic monitoring (`diagnostics_implemented`
/// is the caller-decoded A0h byte 92 bit 6, § 8.8 Table 8-5). A module
/// without diagnostics is refused: the spec defines no A2h ready
/// sequencing for it. The deadline anchors at the invocation start; a
/// non-catalog revision fails closed before anything is decoded.
pub fn readiness_begin(
    revision: Rev,
    diagnostics_implemented: bool,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    now: MonoTime,
    policy: Sff8472ReadinessPolicy,
) -> Result<Sff8472ReadinessState, Sff8472ReadinessError> {
    if !Rev::known_for(Family::Sff8472).contains(&revision) {
        return Err(Sff8472ReadinessError::UnsupportedRevision(revision));
    }
    if !diagnostics_implemented {
        return Err(Sff8472ReadinessError::DiagnosticsNotImplemented);
    }
    if !policy_is_valid(&policy) {
        return Err(Sff8472ReadinessError::InvalidPolicy);
    }
    // Resolve the plan now: a walk whose planned read the catalog cannot
    // describe never begins.
    planned_read(revision)?;
    let deadline = now
        .checked_add(policy.data_ready_span)
        .map_err(|_| Sff8472ReadinessError::TimeOverflow)?;
    Ok(Sff8472ReadinessState {
        revision,
        generation,
        epoch,
        started_at: now,
        deadline,
        last: None,
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
/// revalidated against the catalog — a tampered or foreign revision
/// context is refused, never decoded — and the per-call `policy` is
/// revalidated with the same rules as [`readiness_begin`].
pub fn readiness_step(
    previous: &Sff8472ReadinessState,
    observation: Option<&Sff8472ReadinessObservation<'_>>,
    now: MonoTime,
    policy: Sff8472ReadinessPolicy,
) -> Result<(Sff8472ReadinessState, Sff8472ReadinessDecision), Sff8472ReadinessError> {
    if !Rev::known_for(Family::Sff8472).contains(&previous.revision) {
        return Err(Sff8472ReadinessError::UnsupportedRevision(
            previous.revision,
        ));
    }
    if !policy_is_valid(&policy) {
        return Err(Sff8472ReadinessError::InvalidPolicy);
    }
    now.checked_duration_since(previous.started_at)
        .map_err(|_| Sff8472ReadinessError::ReversedTime)?;
    if let Some(terminal) = previous.terminal {
        return Ok((*previous, terminal));
    }
    let read = planned_read(previous.revision)?;
    let mut state = *previous;
    if let Some(observation) = observation {
        let stamp = &observation.stamp;
        stamp
            .completed_at
            .checked_duration_since(stamp.started_at)
            .map_err(|_| Sff8472ReadinessError::ReversedTime)?;
        if !generation_valid(
            stamp.generation,
            stamp.epoch,
            previous.generation,
            previous.epoch,
        ) || stamp.freshness != Freshness::Fresh
            || stamp.started_at < previous.started_at
            || stamp.completed_at > now
        {
            return Err(Sff8472ReadinessError::StaleObservation);
        }
    }
    if let Some(Sff8472ReadinessSample::BridgeLost) = observation.map(|o| o.sample) {
        let decision = Sff8472ReadinessDecision::BridgeLost;
        state.terminal = Some(decision);
        return Ok((state, decision));
    }
    // A late observation is not decoded or published.
    if now > previous.deadline {
        let decision = Sff8472ReadinessDecision::Timeout {
            last: previous.last,
        };
        state.terminal = Some(decision);
        return Ok((state, decision));
    }
    let decision = match observation.map(|o| o.sample) {
        Some(Sff8472ReadinessSample::Supplied { bytes, offset }) => {
            let byte = field_bytes(bytes, offset, STATUS_BYTE_OFFSET, 1)?[0];
            let data_not_ready = byte & 1 != 0;
            state.last = Some(data_not_ready);
            if !data_not_ready {
                Sff8472ReadinessDecision::Ready
            } else {
                poll_decision(&state, now, policy, read)?
            }
        }
        // A retryable exchange failure is transient; it never renews the
        // anchor.
        Some(Sff8472ReadinessSample::TransientError) | None => {
            poll_decision(&state, now, policy, read)?
        }
        Some(Sff8472ReadinessSample::BridgeLost) => unreachable!("handled above"),
    };
    if !matches!(decision, Sff8472ReadinessDecision::PollAt { .. }) {
        state.terminal = Some(decision);
    }
    Ok((state, decision))
}

/// The next poll time: `now + cadence`, clamped to the deadline; at the
/// deadline exactly, the walk is out of time rather than scheduled.
fn poll_decision(
    state: &Sff8472ReadinessState,
    now: MonoTime,
    policy: Sff8472ReadinessPolicy,
    read: Sff8472PlannedRead,
) -> Result<Sff8472ReadinessDecision, Sff8472ReadinessError> {
    if now >= state.deadline {
        return Ok(Sff8472ReadinessDecision::Timeout { last: state.last });
    }
    let at = now
        .checked_add(policy.poll_interval)
        .map_err(|_| Sff8472ReadinessError::TimeOverflow)?
        .min(state.deadline);
    Ok(Sff8472ReadinessDecision::PollAt {
        at,
        deadline: state.deadline,
        read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::v12_5::PageA2hDiagnosticsBase;
    use openxcvr_core::types::management::ObservationSource;

    fn stamp(generation: ModuleGeneration, epoch: CacheEpoch, at: MonoTime) -> ObservationStamp {
        ObservationStamp {
            started_at: at,
            completed_at: at,
            generation,
            epoch,
            source: ObservationSource::Supplied,
            freshness: Freshness::Fresh,
            consumed_effects: vec![],
        }
    }

    fn policy() -> Sff8472ReadinessPolicy {
        Sff8472ReadinessPolicy {
            poll_interval: MonoDuration(100),
            data_ready_span: MonoDuration(T_DATA_MAX_MS),
        }
    }

    fn begin(rev: Rev, at: MonoTime) -> Result<Sff8472ReadinessState, Sff8472ReadinessError> {
        readiness_begin(rev, true, ModuleGeneration(1), CacheEpoch(1), at, policy())
    }

    fn supplied<'a>(bytes: &'a [u8], at: MonoTime) -> Sff8472ReadinessObservation<'a> {
        Sff8472ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), at),
            sample: Sff8472ReadinessSample::Supplied { bytes, offset: 0 },
        }
    }

    /// A minimal A2h window covering byte 110 with `data_not_ready` at
    /// bit 0 (§ 9.8, Table 9-16).
    fn window(data_not_ready: bool) -> [u8; 111] {
        let mut bytes = [0u8; 111];
        if data_not_ready {
            bytes[110] = 0x01;
        }
        bytes
    }

    #[test]
    fn for_t_data_derives_the_spec_span() {
        let policy = Sff8472ReadinessPolicy::for_t_data(MonoDuration(100));
        assert_eq!(policy.data_ready_span, MonoDuration(1_000));
        assert_eq!(policy.poll_interval, MonoDuration(100));
    }

    #[test]
    fn planned_read_is_resolved_from_the_schema_catalog() {
        for rev in Rev::known_for(Family::Sff8472) {
            let read = planned_read(*rev).expect("catalog resolves the status byte");
            assert_eq!(read.address, 0x51, "A2h 2-wire address, {rev:?}");
            assert_eq!(read.offset, 110, "status byte is A2h:110, {rev:?}");
            assert_eq!(read.len, 1, "{rev:?}");
            // The spec defines no read effect on byte 110: the schema
            // records none and the poll is side-effect-free.
            assert!(read.read_effects.is_empty(), "{rev:?}");
        }
    }

    #[test]
    fn every_catalog_revision_is_admitted_and_ready_decodes() {
        let now = MonoTime(1_000);
        for rev in Rev::known_for(Family::Sff8472) {
            let state = begin(*rev, now).expect("catalog revision is admitted");
            let bytes = window(false);
            let (state, decision) =
                readiness_step(&state, Some(&supplied(&bytes, now)), now, policy())
                    .expect("step succeeds");
            assert_eq!(decision, Sff8472ReadinessDecision::Ready, "{rev:?}");
            // Terminal decisions replay unchanged.
            let (state2, decision2) =
                readiness_step(&state, None, MonoTime(2_000), policy()).expect("step");
            assert_eq!(decision2, decision);
            assert_eq!(state2, state);
        }
    }

    #[test]
    fn modules_without_diagnostics_are_refused() {
        let now = MonoTime(1_000);
        assert!(matches!(
            readiness_begin(
                Rev::SFF8472_12_5,
                false,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                policy()
            )
            .unwrap_err(),
            Sff8472ReadinessError::DiagnosticsNotImplemented
        ));
    }

    /// `readiness_step` revalidates the caller-restored state's revision at
    /// every step — before the terminal replay — so a tampered or foreign
    /// revision context is refused, never decoded.
    #[test]
    fn off_catalog_and_tampered_revisions_fail_closed() {
        let now = MonoTime(1_000);
        let future = Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 6,
        };
        assert!(matches!(
            begin(future, now).unwrap_err(),
            Sff8472ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
        let foreign = Rev::CMIS_5_3;
        assert!(matches!(
            begin(foreign, now).unwrap_err(),
            Sff8472ReadinessError::UnsupportedRevision(rev) if rev == foreign
        ));
        // Tampered mid-walk state: revision swapped after a valid begin...
        let mut tampered = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        tampered.revision = future;
        let bytes = window(false);
        assert!(matches!(
            readiness_step(&tampered, Some(&supplied(&bytes, now)), now, policy()).unwrap_err(),
            Sff8472ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
        // ...and before terminal replay: a latched terminal never launders
        // the tampered revision through.
        let mut replay = tampered;
        replay.terminal = Some(Sff8472ReadinessDecision::Ready);
        assert!(matches!(
            readiness_step(&replay, None, MonoTime(2_000), policy()).unwrap_err(),
            Sff8472ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
    }

    #[test]
    fn not_ready_polls_on_with_the_plan_and_records_last_bit() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        // DNR still set: poll on, carrying the planned read as data.
        let bytes = window(true);
        let (state, decision) =
            readiness_step(&state, Some(&supplied(&bytes, now)), now, policy()).expect("step");
        assert_eq!(
            decision,
            Sff8472ReadinessDecision::PollAt {
                at: MonoTime(1_100),
                deadline: MonoTime(2_000),
                read: planned_read(Rev::SFF8472_12_5).unwrap(),
            }
        );
        assert_eq!(state.last, Some(true));
        // A retryable exchange failure polls on without touching `last`.
        let error = Sff8472ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), MonoTime(1_100)),
            sample: Sff8472ReadinessSample::TransientError,
        };
        let (_, decision) =
            readiness_step(&state, Some(&error), MonoTime(1_100), policy()).expect("step");
        assert!(matches!(decision, Sff8472ReadinessDecision::PollAt { .. }));
        // The window offset is honoured: byte 110 anywhere in the span.
        // This window starts at page byte 100, so byte 110 lives at
        // window index 10 (field_offset - supplied_offset).
        let offset_window = [0x60u8; 11];
        let observation = Sff8472ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), MonoTime(1_200)),
            sample: Sff8472ReadinessSample::Supplied {
                bytes: &offset_window,
                offset: 100,
            },
        };
        let (state, decision) =
            readiness_step(&state, Some(&observation), MonoTime(1_200), policy()).expect("step");
        assert_eq!(decision, Sff8472ReadinessDecision::Ready);
        assert_eq!(state.last, Some(false));
    }

    #[test]
    fn bridge_loss_is_terminal() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        let lost = Sff8472ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: Sff8472ReadinessSample::BridgeLost,
        };
        let (state, decision) = readiness_step(&state, Some(&lost), now, policy()).expect("step");
        assert_eq!(decision, Sff8472ReadinessDecision::BridgeLost);
        assert_eq!(state.terminal, Some(decision));
    }

    #[test]
    fn expiry_is_typed_and_late_observations_never_rescue_it() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        // Step at the deadline: out of time, with no accepted observation.
        let (state, decision) =
            readiness_step(&state, None, MonoTime(2_000), policy()).expect("step");
        assert_eq!(decision, Sff8472ReadinessDecision::Timeout { last: None });
        // A late ready observation cannot rescue the expired walk.
        let ready_bytes = window(false);
        let ready = supplied(&ready_bytes, MonoTime(2_001));
        let (_, decision) =
            readiness_step(&state, Some(&ready), MonoTime(2_002), policy()).expect("step");
        assert_eq!(decision, Sff8472ReadinessDecision::Timeout { last: None });
        // An accepted pre-expiry observation is carried in the timeout.
        let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        let (state, _) = readiness_step(&state, Some(&supplied(&window(true), now)), now, policy())
            .expect("step");
        let (_, decision) = readiness_step(&state, None, MonoTime(2_000), policy()).expect("step");
        assert_eq!(
            decision,
            Sff8472ReadinessDecision::Timeout { last: Some(true) }
        );
    }

    #[test]
    fn stale_span_and_policy_refusals_leave_the_state_untouched() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
        // Wrong generation.
        let bytes = window(false);
        let mut wrong = supplied(&bytes, now);
        wrong.stamp.generation = ModuleGeneration(2);
        assert!(matches!(
            readiness_step(&state, Some(&wrong), now, policy()).unwrap_err(),
            Sff8472ReadinessError::StaleObservation
        ));
        // Non-fresh.
        let mut stale = supplied(&bytes, now);
        stale.stamp.freshness = Freshness::Historical;
        assert!(matches!(
            readiness_step(&state, Some(&stale), now, policy()).unwrap_err(),
            Sff8472ReadinessError::StaleObservation
        ));
        // Postdates now.
        let mut future = supplied(&bytes, MonoTime(2_000));
        future.stamp.started_at = MonoTime(2_000);
        future.stamp.completed_at = MonoTime(2_000);
        assert!(matches!(
            readiness_step(&state, Some(&future), now, policy()).unwrap_err(),
            Sff8472ReadinessError::StaleObservation
        ));
        // A span that does not cover byte 110 is a typed span error.
        let short = Sff8472ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: Sff8472ReadinessSample::Supplied {
                bytes: &[0u8; 110],
                offset: 0,
            },
        };
        assert!(matches!(
            readiness_step(&state, Some(&short), now, policy()),
            Err(Sff8472ReadinessError::Span(_))
        ));
        // Reversed time refuses.
        assert!(matches!(
            readiness_step(&state, None, MonoTime(1), policy()).unwrap_err(),
            Sff8472ReadinessError::ReversedTime
        ));
        // Invalid policies are refused at begin and at every step.
        let mut zero_interval = policy();
        zero_interval.poll_interval = MonoDuration(0);
        assert!(matches!(
            readiness_begin(
                Rev::SFF8472_12_5,
                true,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                zero_interval
            )
            .unwrap_err(),
            Sff8472ReadinessError::InvalidPolicy
        ));
        assert!(matches!(
            readiness_step(&state, None, now, zero_interval).unwrap_err(),
            Sff8472ReadinessError::InvalidPolicy
        ));
        let mut inverted = policy();
        inverted.data_ready_span = MonoDuration(1);
        assert!(matches!(
            readiness_begin(
                Rev::SFF8472_12_5,
                true,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                inverted
            )
            .unwrap_err(),
            Sff8472ReadinessError::InvalidPolicy
        ));
        // None of the refusals changed the walk.
        assert_eq!(state.last, None);
        assert_eq!(state.terminal, None);
    }

    /// Parity with the generated decoder: the walk's bit extraction from a
    /// supplied window must agree with the revision-specialised generated
    /// `PageA2hDiagnosticsBase` decode of the same A2h page, bit for bit.
    #[test]
    fn bit_extraction_matches_the_generated_decoder() {
        for byte in [0x00, 0x01, 0x03, 0x05, 0xFF] {
            let mut page = [0u8; 256];
            page[110] = byte;
            let decoded = PageA2hDiagnosticsBase::decode(&page);
            let now = MonoTime(1_000);
            let state = begin(Rev::SFF8472_12_5, now).expect("catalog revision");
            let (state, _) =
                readiness_step(&state, Some(&supplied(&page[..111], now)), now, policy())
                    .expect("step");
            assert_eq!(
                state.last,
                Some(decoded.data_not_ready),
                "byte 110 = {byte:#04x}"
            );
        }
    }
}
