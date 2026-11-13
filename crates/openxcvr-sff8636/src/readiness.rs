//! Pure SFF-8636 monitor-data readiness: a time-bounded poll plan over
//! caller-supplied observations, mirroring the CMIS
//! `readiness_begin`/`readiness_step` shape.
//!
//! What the specification defines (SFF-8636 Rev 2.12 § 6.2.2, Table 6-3
//! note): the `Data_Not_Ready` bit (Lower:2 bit 0) is asserted high during
//! free-side reset, power-up reset, and until a valid suite of monitor
//! readings exists; once all monitor readings are valid it is set low until
//! power-down or reset. Upon completion of power-up reset the module asserts
//! `IntL` (Lower:2 bit 1) low while de-asserting `Data_Not_Ready` low, and
//! "`The IntL bit will remain asserted until a read is performed of the
//! Data_Not_Ready bit (Byte 2)`" — so the very read that observes readiness
//! also acknowledges the startup interrupt. That read effect is recorded in
//! the schema on `interrupt_deasserted` (`acknowledge_startup_interrupt`,
//! byte scope) and every plan in this module resolves it from the generated
//! catalog, never from a literal here.
//!
//! What the specification does not define: any startup state machine, any
//! fault or reserved encoding in the ready bit, and any startup time bound —
//! § 4.4.2 leaves power-on timing "in the relevant hardware specification".
//! So this is not an FSM: it is a monotone readiness predicate plus a
//! deadline-bounded poll plan, and both the poll interval and the deadline
//! span must come from the caller's policy (no default is provided or
//! invented). The library never reads, writes, sleeps, or owns a clock: the
//! caller performs every read, supplies each observation with a validity
//! stamp, and stores the returned state between steps.

use openxcvr_core::field_catalog::{resolve, CatalogError, FieldQuery, LaneSelection};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::supplied::{field_bytes, SpanError};
use openxcvr_core::types::management::{
    generation_valid, CacheEpoch, Freshness, ModuleGeneration, MonoDuration, MonoTime,
    ObservationStamp, ReadEffect,
};

use crate::generated::catalog::CATALOG;

/// Lower-memory byte 2, the status-indicators byte carrying `Flat_mem`
/// (bit 2), `IntL` (bit 1) and `Data_Not_Ready` (bit 0) — SFF-8636
/// Rev 2.12 § 6.2.2, Table 6-3. The readiness walk reads exactly this byte.
const STATUS_BYTE_OFFSET: usize = 2;

/// Caller-supplied policy for one readiness invocation. SFF-8636 sets no
/// startup timing (§ 4.4.2 defers it to the hardware specification), so
/// both values come from the caller's configuration. No `Default` is
/// provided: a policy must be chosen, not assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8636ReadinessPolicy {
    /// Poll cadence between non-terminal steps. Must be nonzero and not
    /// exceed `deadline_span`.
    pub poll_interval: MonoDuration,
    /// Host backstop for the whole walk, anchored at the invocation start.
    /// Must be nonzero.
    pub deadline_span: MonoDuration,
}

/// Why an SFF-8636 readiness invocation was refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Sff8636ReadinessError {
    /// Only catalog SFF-8636 revisions are admitted; every other context
    /// (a future revision, a foreign family) fails closed before any
    /// decode.
    #[error("unsupported revision: {0:?}")]
    UnsupportedRevision(Rev),
    /// The policy's poll interval or deadline span is zero, or the
    /// interval exceeds the span.
    #[error("readiness policy requires nonzero poll interval and deadline span, interval <= span")]
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
    /// The supplied bytes do not cover the status byte (Lower:2).
    #[error("supplied span does not cover the status byte")]
    Span(#[from] SpanError),
    /// The schema catalog could not resolve the status-byte field at the
    /// walk's revision: reported, never bypassed with a hand-built plan.
    #[error("catalog resolution failed: {0}")]
    Catalog(#[from] CatalogError),
}

/// The two status bits the walk observes in Lower:2 (§ 6.2.2, Table 6-3):
/// `Data_Not_Ready` (bit 0; `true` = monitor data not yet valid) and `IntL`
/// (bit 1; `true` = IntL not asserted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8636StatusBits {
    /// `true` while the free-side device has no valid monitor data.
    pub data_not_ready: bool,
    /// `true` when the IntL output is not asserted.
    pub intl_deasserted: bool,
}

/// One observation offered to the reducer. `Supplied` carries the
/// lower-memory window containing byte 2 (the window's first byte at
/// `offset`); only a supplied span is decoded, unobserved bytes are never
/// zero-filled. `TransientError` is a caller-classified retryable exchange
/// failure, never adapter absence; uncertain bridge continuity is
/// `BridgeLost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sff8636ReadinessSample<'a> {
    /// A supplied lower-memory window and the page offset its first byte
    /// came from.
    Supplied {
        /// The supplied lower-memory window.
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
pub struct Sff8636ReadinessObservation<'a> {
    /// The observation's validity stamp.
    pub stamp: ObservationStamp,
    /// What was observed.
    pub sample: Sff8636ReadinessSample<'a>,
}

/// A planned read described as data. The library never performs it; the
/// caller reads `len` bytes at `offset` in the page window at 2-wire
/// `address` and offers the result as the next `Supplied` observation.
/// `read_effects` is the schema-recorded effect of that read, resolved
/// from the generated catalog at the walk's revision — today
/// `AcknowledgeStartupInterrupt`, because § 6.2.2 keeps the power-up IntL
/// assertion latched until byte 2 is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8636PlannedRead {
    /// Seven-bit 2-wire address of the page window (lower memory: 0x50).
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
pub enum Sff8636ReadinessDecision {
    /// Continue polling: read the planned byte at/after `at`, against the
    /// anchored `deadline`.
    PollAt {
        /// The earliest time to perform the planned read.
        at: MonoTime,
        /// The anchored deadline the walk expires at.
        deadline: MonoTime,
        /// The read to perform, as data.
        read: Sff8636PlannedRead,
    },
    /// The free-side device has valid monitor data (`Data_Not_Ready` low),
    /// the spec's ready-for-management condition. `status` carries both
    /// observed bits; per § 6.2.2 the IntL bit reads asserted (0) at
    /// power-up-reset completion and de-asserted once acknowledged.
    /// `acknowledged_startup_interrupt` is `true` when the schema records
    /// that this read of byte 2 acknowledged the startup IntL assertion
    /// (the caller may record the same fact in the observation stamp's
    /// `consumed_effects`).
    Ready {
        /// Both status bits as observed in Lower:2.
        status: Sff8636StatusBits,
        /// Whether the schema records that this read of byte 2
        /// acknowledged the startup IntL assertion.
        acknowledged_startup_interrupt: bool,
    },
    /// The anchored deadline expired. `last` is the last accepted
    /// observation's status bits, when one was accepted.
    Timeout {
        /// The last accepted observation's status bits, when one was
        /// accepted.
        last: Option<Sff8636StatusBits>,
    },
    /// The bridge's continuity is uncertain; readiness stops.
    BridgeLost,
}

/// The readiness state the caller stores and supplies back each step.
/// Pure data: the library holds no resident instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sff8636ReadinessState {
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
    /// The last accepted observation's status bits, when one was accepted.
    pub last: Option<Sff8636StatusBits>,
    /// The terminal decision, once reached; later steps replay it.
    pub terminal: Option<Sff8636ReadinessDecision>,
}

/// The read the walk plans at `revision`, resolved from the schema
/// catalog's `interrupt_deasserted` row (the field carrying the recorded
/// byte-2 acknowledge effect). Pure data: callers use it for the first
/// read before the first step and for every poll in between.
pub fn planned_read(revision: Rev) -> Result<Sff8636PlannedRead, Sff8636ReadinessError> {
    let entries = resolve(
        &CATALOG,
        revision,
        &FieldQuery {
            pattern: "interrupt_deasserted",
            lane: LaneSelection::Unspecified,
            page: Some("lower"),
            bank: 0,
        },
    )?;
    // A single-name, single-page query resolves to exactly one entry; a
    // catalog that ever disagrees is reported, not guessed.
    let row = entries
        .first()
        .ok_or_else(|| CatalogError::NoMatch {
            pattern: "interrupt_deasserted".to_string(),
            rev: revision,
        })?
        .row;
    Ok(Sff8636PlannedRead {
        address: row.address,
        offset: u8::try_from(row.offset).map_err(|_| CatalogError::NoMatch {
            pattern: "interrupt_deasserted".to_string(),
            rev: revision,
        })?,
        len: row.len,
        read_effects: row.read_effects,
    })
}

/// The rules a readiness policy must satisfy, shared by
/// [`readiness_begin`] and every [`readiness_step`] call (a zero poll
/// interval would schedule `PollAt { at: now }` and spin the caller).
fn policy_is_valid(policy: &Sff8636ReadinessPolicy) -> bool {
    policy.poll_interval.0 != 0
        && policy.deadline_span.0 != 0
        && policy.poll_interval <= policy.deadline_span
}

/// Begin a readiness walk at `now` for a catalog SFF-8636 revision. The
/// deadline anchors at the invocation start; a non-catalog revision fails
/// closed before anything is decoded.
pub fn readiness_begin(
    revision: Rev,
    generation: ModuleGeneration,
    epoch: CacheEpoch,
    now: MonoTime,
    policy: Sff8636ReadinessPolicy,
) -> Result<Sff8636ReadinessState, Sff8636ReadinessError> {
    if !Rev::known_for(Family::Sff8636).contains(&revision) {
        return Err(Sff8636ReadinessError::UnsupportedRevision(revision));
    }
    if !policy_is_valid(&policy) {
        return Err(Sff8636ReadinessError::InvalidPolicy);
    }
    // Resolve the plan now: a walk whose planned read the catalog cannot
    // describe never begins.
    planned_read(revision)?;
    let deadline = now
        .checked_add(policy.deadline_span)
        .map_err(|_| Sff8636ReadinessError::TimeOverflow)?;
    Ok(Sff8636ReadinessState {
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
    previous: &Sff8636ReadinessState,
    observation: Option<&Sff8636ReadinessObservation<'_>>,
    now: MonoTime,
    policy: Sff8636ReadinessPolicy,
) -> Result<(Sff8636ReadinessState, Sff8636ReadinessDecision), Sff8636ReadinessError> {
    if !Rev::known_for(Family::Sff8636).contains(&previous.revision) {
        return Err(Sff8636ReadinessError::UnsupportedRevision(
            previous.revision,
        ));
    }
    if !policy_is_valid(&policy) {
        return Err(Sff8636ReadinessError::InvalidPolicy);
    }
    now.checked_duration_since(previous.started_at)
        .map_err(|_| Sff8636ReadinessError::ReversedTime)?;
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
            .map_err(|_| Sff8636ReadinessError::ReversedTime)?;
        if !generation_valid(
            stamp.generation,
            stamp.epoch,
            previous.generation,
            previous.epoch,
        ) || stamp.freshness != Freshness::Fresh
            || stamp.started_at < previous.started_at
            || stamp.completed_at > now
        {
            return Err(Sff8636ReadinessError::StaleObservation);
        }
    }
    if let Some(Sff8636ReadinessSample::BridgeLost) = observation.map(|o| o.sample) {
        let decision = Sff8636ReadinessDecision::BridgeLost;
        state.terminal = Some(decision);
        return Ok((state, decision));
    }
    // A late observation is not decoded or published.
    if now > previous.deadline {
        let decision = Sff8636ReadinessDecision::Timeout {
            last: previous.last,
        };
        state.terminal = Some(decision);
        return Ok((state, decision));
    }
    let acknowledged_startup_interrupt = read
        .read_effects
        .contains(&ReadEffect::AcknowledgeStartupInterrupt);
    let decision = match observation.map(|o| o.sample) {
        Some(Sff8636ReadinessSample::Supplied { bytes, offset }) => {
            let byte = field_bytes(bytes, offset, STATUS_BYTE_OFFSET, 1)?[0];
            let status = Sff8636StatusBits {
                data_not_ready: byte & 1 != 0,
                intl_deasserted: (byte >> 1) & 1 != 0,
            };
            state.last = Some(status);
            if !status.data_not_ready {
                Sff8636ReadinessDecision::Ready {
                    status,
                    acknowledged_startup_interrupt,
                }
            } else {
                poll_decision(&state, now, policy, read)?
            }
        }
        // A retryable exchange failure is transient; it never renews the
        // anchor.
        Some(Sff8636ReadinessSample::TransientError) | None => {
            poll_decision(&state, now, policy, read)?
        }
        Some(Sff8636ReadinessSample::BridgeLost) => unreachable!("handled above"),
    };
    if !matches!(decision, Sff8636ReadinessDecision::PollAt { .. }) {
        state.terminal = Some(decision);
    }
    Ok((state, decision))
}

/// The next poll time: `now + cadence`, clamped to the deadline; at the
/// deadline exactly, the walk is out of time rather than scheduled.
fn poll_decision(
    state: &Sff8636ReadinessState,
    now: MonoTime,
    policy: Sff8636ReadinessPolicy,
    read: Sff8636PlannedRead,
) -> Result<Sff8636ReadinessDecision, Sff8636ReadinessError> {
    if now >= state.deadline {
        return Ok(Sff8636ReadinessDecision::Timeout { last: state.last });
    }
    let at = now
        .checked_add(policy.poll_interval)
        .map_err(|_| Sff8636ReadinessError::TimeOverflow)?
        .min(state.deadline);
    Ok(Sff8636ReadinessDecision::PollAt {
        at,
        deadline: state.deadline,
        read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::v2_12::Sff8636LowerMemory;
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

    fn policy() -> Sff8636ReadinessPolicy {
        Sff8636ReadinessPolicy {
            poll_interval: MonoDuration(50),
            deadline_span: MonoDuration(10_000),
        }
    }

    fn begin(rev: Rev, at: MonoTime) -> Result<Sff8636ReadinessState, Sff8636ReadinessError> {
        readiness_begin(rev, ModuleGeneration(1), CacheEpoch(1), at, policy())
    }

    fn supplied<'a>(bytes: &'a [u8], at: MonoTime) -> Sff8636ReadinessObservation<'a> {
        Sff8636ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), at),
            sample: Sff8636ReadinessSample::Supplied { bytes, offset: 0 },
        }
    }

    /// Byte 2 with `data_not_ready` at bit 0 and `intl_deasserted` at
    /// bit 1 (§ 6.2.2, Table 6-3).
    fn byte2(data_not_ready: bool, intl_deasserted: bool) -> [u8; 3] {
        [
            0,
            0,
            u8::from(data_not_ready) | (u8::from(intl_deasserted) << 1),
        ]
    }

    #[test]
    fn planned_read_is_resolved_from_the_schema_catalog() {
        for rev in Rev::known_for(Family::Sff8636) {
            let read = planned_read(*rev).expect("catalog resolves the status byte");
            assert_eq!(read.address, 0x50, "lower memory 2-wire address, {rev:?}");
            assert_eq!(read.offset, 2, "status byte is Lower:2, {rev:?}");
            assert_eq!(read.len, 1, "{rev:?}");
            // Pins the schema decision: the byte-2 read acknowledges the
            // startup IntL assertion (§ 6.2.2 Table 6-3 note) at every
            // catalog revision.
            assert!(
                read.read_effects
                    .contains(&ReadEffect::AcknowledgeStartupInterrupt),
                "schema must record the byte-2 acknowledge effect, {rev:?}"
            );
        }
    }

    #[test]
    fn every_catalog_revision_is_admitted_and_ready_decodes() {
        let now = MonoTime(1_000);
        for rev in Rev::known_for(Family::Sff8636) {
            let state = begin(*rev, now).expect("catalog revision is admitted");
            // § 6.2.2: on power-up-reset completion IntL is asserted (0)
            // while Data_Not_Ready is de-asserted (0).
            let bytes = byte2(false, false);
            let (state, decision) =
                readiness_step(&state, Some(&supplied(&bytes, now)), now, policy())
                    .expect("step succeeds");
            assert_eq!(
                decision,
                Sff8636ReadinessDecision::Ready {
                    status: Sff8636StatusBits {
                        data_not_ready: false,
                        intl_deasserted: false,
                    },
                    acknowledged_startup_interrupt: true,
                },
                "{rev:?}"
            );
            // Terminal decisions replay unchanged.
            let (state2, decision2) =
                readiness_step(&state, None, MonoTime(2_000), policy()).expect("step");
            assert_eq!(decision2, decision);
            assert_eq!(state2, state);
        }
    }

    /// `readiness_step` revalidates the caller-restored state's revision at
    /// every step — before the terminal replay — so a tampered or foreign
    /// revision context is refused, never decoded.
    #[test]
    fn off_catalog_and_tampered_revisions_fail_closed() {
        let now = MonoTime(1_000);
        let future = Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 13,
        };
        assert!(matches!(
            begin(future, now).unwrap_err(),
            Sff8636ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
        let foreign = Rev::CMIS_5_3;
        assert!(matches!(
            begin(foreign, now).unwrap_err(),
            Sff8636ReadinessError::UnsupportedRevision(rev) if rev == foreign
        ));
        // Tampered mid-walk state: revision swapped after a valid begin...
        let mut tampered = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        tampered.revision = future;
        let ready_bytes = byte2(false, true);
        assert!(matches!(
            readiness_step(&tampered, Some(&supplied(&ready_bytes, now)), now, policy())
                .unwrap_err(),
            Sff8636ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
        // ...and before terminal replay: a latched terminal never launders
        // the tampered revision through.
        let mut replay = tampered;
        replay.terminal = Some(Sff8636ReadinessDecision::Ready {
            status: Sff8636StatusBits {
                data_not_ready: false,
                intl_deasserted: true,
            },
            acknowledged_startup_interrupt: true,
        });
        assert!(matches!(
            readiness_step(&replay, None, MonoTime(2_000), policy()).unwrap_err(),
            Sff8636ReadinessError::UnsupportedRevision(rev) if rev == future
        ));
    }

    #[test]
    fn not_ready_polls_on_with_the_plan_and_records_last_bits() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        // DNR still set: poll on, carrying the planned read as data.
        let bytes = byte2(true, true);
        let (state, decision) =
            readiness_step(&state, Some(&supplied(&bytes, now)), now, policy()).expect("step");
        assert_eq!(
            decision,
            Sff8636ReadinessDecision::PollAt {
                at: MonoTime(1_050),
                deadline: MonoTime(11_000),
                read: planned_read(Rev::SFF8636_2_12).unwrap(),
            }
        );
        assert_eq!(
            state.last,
            Some(Sff8636StatusBits {
                data_not_ready: true,
                intl_deasserted: true,
            })
        );
        // A retryable exchange failure polls on without touching `last`.
        let error = Sff8636ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), MonoTime(1_050)),
            sample: Sff8636ReadinessSample::TransientError,
        };
        let (state, decision) =
            readiness_step(&state, Some(&error), MonoTime(1_050), policy()).expect("step");
        assert!(matches!(decision, Sff8636ReadinessDecision::PollAt { .. }));
        assert_eq!(
            state.last,
            Some(Sff8636StatusBits {
                data_not_ready: true,
                intl_deasserted: true,
            })
        );
        // The window offset is honoured: byte 2 anywhere in the span.
        // This window starts at page byte 1, so byte 2 lives at window
        // index 1 (field_offset - supplied_offset).
        let window = [0x00, 0x01]; // DNR set, IntL asserted
        let observation = Sff8636ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), MonoTime(1_100)),
            sample: Sff8636ReadinessSample::Supplied {
                bytes: &window,
                offset: 1,
            },
        };
        let (_, decision) =
            readiness_step(&state, Some(&observation), MonoTime(1_100), policy()).expect("step");
        assert!(matches!(decision, Sff8636ReadinessDecision::PollAt { .. }));
    }

    #[test]
    fn bridge_loss_is_terminal() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        let lost = Sff8636ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: Sff8636ReadinessSample::BridgeLost,
        };
        let (state, decision) = readiness_step(&state, Some(&lost), now, policy()).expect("step");
        assert_eq!(decision, Sff8636ReadinessDecision::BridgeLost);
        assert_eq!(state.terminal, Some(decision));
    }

    #[test]
    fn expiry_is_typed_and_late_observations_never_rescue_it() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        // Step at the deadline: out of time, with no accepted observation.
        let (state, decision) =
            readiness_step(&state, None, MonoTime(11_000), policy()).expect("step");
        assert_eq!(decision, Sff8636ReadinessDecision::Timeout { last: None });
        // A late ready observation cannot rescue the expired walk.
        let ready_bytes = byte2(false, true);
        let ready = supplied(&ready_bytes, MonoTime(11_001));
        let (_, decision) =
            readiness_step(&state, Some(&ready), MonoTime(11_002), policy()).expect("step");
        assert_eq!(decision, Sff8636ReadinessDecision::Timeout { last: None });
        // An accepted pre-expiry observation is carried in the timeout.
        let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        let not_ready = byte2(true, false);
        let (state, _) =
            readiness_step(&state, Some(&supplied(&not_ready, now)), now, policy()).expect("step");
        let (_, decision) = readiness_step(&state, None, MonoTime(11_000), policy()).expect("step");
        assert_eq!(
            decision,
            Sff8636ReadinessDecision::Timeout {
                last: Some(Sff8636StatusBits {
                    data_not_ready: true,
                    intl_deasserted: false,
                })
            }
        );
    }

    #[test]
    fn stale_span_and_policy_refusals_leave_the_state_untouched() {
        let now = MonoTime(1_000);
        let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
        // Wrong generation.
        let bytes = byte2(false, true);
        let mut wrong = supplied(&bytes, now);
        wrong.stamp.generation = ModuleGeneration(2);
        assert!(matches!(
            readiness_step(&state, Some(&wrong), now, policy()).unwrap_err(),
            Sff8636ReadinessError::StaleObservation
        ));
        // Non-fresh.
        let mut stale = supplied(&bytes, now);
        stale.stamp.freshness = Freshness::Historical;
        assert!(matches!(
            readiness_step(&state, Some(&stale), now, policy()).unwrap_err(),
            Sff8636ReadinessError::StaleObservation
        ));
        // Postdates now.
        let mut future = supplied(&bytes, MonoTime(2_000));
        future.stamp.started_at = MonoTime(2_000);
        future.stamp.completed_at = MonoTime(2_000);
        assert!(matches!(
            readiness_step(&state, Some(&future), now, policy()).unwrap_err(),
            Sff8636ReadinessError::StaleObservation
        ));
        // Predates the invocation start.
        let mut early = supplied(&bytes, MonoTime(1));
        early.stamp.started_at = MonoTime(1);
        early.stamp.completed_at = MonoTime(1);
        assert!(matches!(
            readiness_step(&state, Some(&early), now, policy()).unwrap_err(),
            Sff8636ReadinessError::StaleObservation
        ));
        // A span that does not cover byte 2 is a typed span error.
        let short = Sff8636ReadinessObservation {
            stamp: stamp(ModuleGeneration(1), CacheEpoch(1), now),
            sample: Sff8636ReadinessSample::Supplied {
                bytes: &[0u8; 2],
                offset: 0,
            },
        };
        assert!(matches!(
            readiness_step(&state, Some(&short), now, policy()),
            Err(Sff8636ReadinessError::Span(_))
        ));
        // Reversed time refuses.
        assert!(matches!(
            readiness_step(&state, None, MonoTime(1), policy()).unwrap_err(),
            Sff8636ReadinessError::ReversedTime
        ));
        // Invalid policies are refused at begin and at every step.
        let mut zero_interval = policy();
        zero_interval.poll_interval = MonoDuration(0);
        assert!(matches!(
            readiness_begin(
                Rev::SFF8636_2_12,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                zero_interval
            )
            .unwrap_err(),
            Sff8636ReadinessError::InvalidPolicy
        ));
        assert!(matches!(
            readiness_step(&state, None, now, zero_interval).unwrap_err(),
            Sff8636ReadinessError::InvalidPolicy
        ));
        let mut inverted = policy();
        inverted.deadline_span = MonoDuration(1);
        assert!(matches!(
            readiness_begin(
                Rev::SFF8636_2_12,
                ModuleGeneration(1),
                CacheEpoch(1),
                now,
                inverted
            )
            .unwrap_err(),
            Sff8636ReadinessError::InvalidPolicy
        ));
        // None of the refusals changed the walk.
        assert_eq!(state.last, None);
        assert_eq!(state.terminal, None);
    }

    /// Parity with the generated decoder: the walk's bit extraction from a
    /// supplied window must agree with the revision-specialised generated
    /// `Sff8636LowerMemory` decode of the same lower page, bit for bit.
    #[test]
    fn bit_extraction_matches_the_generated_decoder() {
        for byte in [0x00, 0x01, 0x02, 0x03, 0x05, 0x06, 0x07] {
            let mut page = [0u8; 128];
            page[2] = byte;
            let decoded = Sff8636LowerMemory::decode(&page);
            let now = MonoTime(1_000);
            let state = begin(Rev::SFF8636_2_12, now).expect("catalog revision");
            let (state, _) =
                readiness_step(&state, Some(&supplied(&page[..3], now)), now, policy())
                    .expect("step");
            assert_eq!(
                state.last,
                Some(Sff8636StatusBits {
                    data_not_ready: decoded.data_not_ready,
                    intl_deasserted: decoded.interrupt_deasserted,
                }),
                "byte 2 = {byte:#04x}"
            );
        }
    }
}
