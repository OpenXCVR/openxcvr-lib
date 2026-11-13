//! Fail-closed revision invariant regression tests (review blockers).
//!
//! A revision-qualified entry point must refuse a revision context that is
//! not one of the catalog CMIS revisions (3.0, 4.0, 5.0-5.4) BEFORE any
//! transport I/O: no read, no write — decided by a runtime gate, never a
//! debug-only assertion, and never an open-ended "at least `since`"
//! admission. This file pins that invariant with a counting transport:
//! every refusal case asserts both the typed error and `reads == 0 &&
//! writes == 0`.
//!
//! Non-catalog contexts exercised everywhere: the unknown future revision
//! CMIS 5.5, the unadmitted intermediates 3.1/4.1, and foreign families
//! (SFF-8472, SFF-8636, C-CMIS, ELSFP).

use openxcvr_cmis::adapter::{
    Cdb, CdbCommand, CdbError, CdbOp, CmisCdbModuleFeatures, CmisCdbModuleFeaturesRequest, Dpsm,
    DpsmError, ModuleState, Msm, MsmError, NpState, Npsm, NpsmError,
};
use openxcvr_cmis::generated::CmisModuleState;
use openxcvr_cmis::module::CmisTransport;
use openxcvr_cmis::readiness::{
    readiness_begin, readiness_step, CmisReadinessDecision, CmisReadinessError,
    CmisReadinessObservation, CmisReadinessPolicy, CmisReadinessSample,
};
use openxcvr_core::error::ModuleError;
use openxcvr_core::transport::Transport;
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoDuration, MonoTime, ObservationSource,
    ObservationStamp,
};
use openxcvr_core::{Family, Rev};

/// Every non-catalog context the gates must refuse: the unknown future
/// revision, the unadmitted intermediates, and foreign families.
fn non_catalog_contexts() -> [Rev; 7] {
    [
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        },
        Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        },
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        },
        Rev {
            family: Family::CCmis,
            major: 1,
            minor: 4,
        },
        Rev {
            family: Family::Elsfp,
            major: 1,
            minor: 0,
        },
    ]
}

/// A transport that only counts: reads answer zeros (refusal paths must
/// never reach them), writes are recorded and must stay at zero.
#[derive(Default)]
struct CountingTransport {
    reads: usize,
    writes: usize,
}

impl Transport for CountingTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        self.reads += 1;
        buf.fill(0);
        Ok(())
    }

    fn write(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        _data: &[u8],
    ) -> Result<(), ModuleError> {
        self.writes += 1;
        Ok(())
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}

impl CmisTransport for CountingTransport {}

/// CDB: `send_checked` must refuse a non-catalog revision context before
/// any transport I/O — mapped opcodes and unmapped opcodes alike. The old
/// gate compared [`Rev`] ordering only: `available_until == None` made
/// every revision at or past `available_since` admissible, so CMIS 5.5
/// and foreign families (which sort past every CMIS `since` by family
/// discriminant) reached the frame write in release builds.
#[test]
fn cdb_send_checked_refuses_non_catalog_revisions_without_io() {
    for rev in non_catalog_contexts() {
        let mut transport = CountingTransport::default();
        let result = Cdb::send_checked::<CmisCdbModuleFeatures, _>(
            &mut transport,
            &CmisCdbModuleFeaturesRequest,
            rev,
            3,
        );
        assert!(
            matches!(result, Err(CdbError::UnsupportedRevision { detected }) if detected == rev),
            "{rev:?}: expected UnsupportedRevision, got {result:?}"
        );
        assert_eq!(transport.writes, 0, "{rev:?}: no frame may be written");
        assert_eq!(transport.reads, 0, "{rev:?}: no status poll may be read");
    }
}

/// An opcode with no `CdbOp` mapping rides no timeline, but a non-catalog
/// revision context is still refused before the wire: the gate validates
/// the revision itself.
struct UnmappedProbe;
#[derive(Debug, Default)]
struct UnmappedProbeRequest;
#[derive(Debug)]
struct UnmappedProbeResponse;
impl CdbCommand for UnmappedProbe {
    type Request = UnmappedProbeRequest;
    type Response = UnmappedProbeResponse;
    const CMD_ID: u16 = 0x7F7F; // not a CdbOp opcode
    fn encode_lpl(_req: &Self::Request) -> Vec<u8> {
        Vec::new()
    }
    fn decode_reply(_status: u8, _lpl: &[u8]) -> Result<Self::Response, CdbError> {
        Ok(UnmappedProbeResponse)
    }
}

#[test]
fn cdb_send_checked_refuses_unmapped_opcode_on_non_catalog_rev() {
    let mut transport = CountingTransport::default();
    let result = Cdb::send_checked::<UnmappedProbe, _>(
        &mut transport,
        &UnmappedProbeRequest,
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 5,
        },
        3,
    );
    assert!(matches!(
        result,
        Err(CdbError::UnsupportedRevision {
            detected: Rev {
                family: Family::Cmis,
                major: 5,
                minor: 5
            }
        })
    ));
    assert_eq!(transport.writes, 0);
    assert_eq!(transport.reads, 0);
}

/// The per-op since/until windows keep their meaning inside the catalog:
/// 4.0 predates RMON (5.3), 5.3 admits it, and 5.4 removed 0281h.
#[test]
fn cdb_check_rev_keeps_window_semantics_inside_the_catalog() {
    assert!(matches!(
        CdbOp::RmonStatistics.check_rev(Rev::CMIS_4_0),
        Err(CdbError::UnsupportedInRev { .. })
    ));
    assert!(CdbOp::RmonStatistics.check_rev(Rev::CMIS_5_3).is_ok());
    assert!(CdbOp::ModuleFeatures.check_rev(Rev::CMIS_4_0).is_ok());
    assert!(CdbOp::GetModuleTime.check_rev(Rev::CMIS_5_4).is_ok());
    assert!(matches!(
        CdbOp::DataMonitoringAdvertisements.check_rev(Rev::CMIS_5_4),
        Err(CdbError::RemovedInRev { .. })
    ));
}

/// DPSM: `state_for_rev` must refuse every non-catalog revision with
/// [`DpsmError::UnsupportedRevision`] before the Page 11h read — the
/// sibling `wait_for_terminal` contract, not a debug assertion and not a
/// lossless `Unknown(raw)` after performing I/O.
#[test]
fn dpsm_state_for_rev_refuses_non_catalog_before_any_io() {
    for rev in non_catalog_contexts() {
        let mut transport = CountingTransport::default();
        let result = Dpsm::state_for_rev(&mut transport, rev, 0);
        assert!(
            matches!(result, Err(DpsmError::UnsupportedRevision(rejected)) if rejected == rev),
            "{rev:?}: expected UnsupportedRevision, got {result:?}"
        );
        assert_eq!(
            transport.reads, 0,
            "{rev:?}: no read may precede the refusal"
        );
        assert_eq!(transport.writes, 0, "{rev:?}: state_for_rev never writes");
    }
}

/// MSM: `wait_for_state_for_rev` must validate the revision at entry —
/// not only inside the poll loop — so even `max_polls == 0` fails closed
/// with [`MsmError::UnsupportedRevision`] instead of falling through to
/// `Timeout`, with zero I/O.
#[test]
fn msm_wait_for_state_for_rev_validates_revision_at_entry() {
    for rev in non_catalog_contexts() {
        for max_polls in [0, 2] {
            let mut transport = CountingTransport::default();
            let result =
                Msm::wait_for_state_for_rev(&mut transport, rev, ModuleState::Ready, max_polls);
            assert!(
                matches!(result, Err(MsmError::UnsupportedRevision(rejected)) if rejected == rev),
                "{rev:?} max_polls={max_polls}: expected UnsupportedRevision, got {result:?}"
            );
            assert_eq!(
                transport.reads, 0,
                "{rev:?} max_polls={max_polls}: no read may precede the refusal"
            );
            assert_eq!(transport.writes, 0);
        }
    }
}

/// NPSM: `wait_for_state_for_rev` validates the revision at entry, the
/// same entry-gate contract as MSM — zero I/O for every non-catalog
/// context, including the zero-poll wait.
#[test]
fn npsm_wait_for_state_for_rev_validates_revision_at_entry() {
    for rev in non_catalog_contexts() {
        for max_polls in [0, 2] {
            let mut transport = CountingTransport::default();
            let result =
                Npsm::wait_for_state_for_rev(&mut transport, rev, 0, NpState::Activated, max_polls);
            assert!(
                matches!(result, Err(NpsmError::UnsupportedRevision(rejected)) if rejected == rev),
                "{rev:?} max_polls={max_polls}: expected UnsupportedRevision, got {result:?}"
            );
            assert_eq!(
                transport.reads, 0,
                "{rev:?} max_polls={max_polls}: no read may precede the refusal"
            );
            assert_eq!(transport.writes, 0);
        }
    }
}

// ── readiness: tampered caller-restored state ─────────────────────────────

fn stamped_observation<'a>(bytes: &'a [u8], at: MonoTime) -> CmisReadinessObservation<'a> {
    CmisReadinessObservation {
        stamp: ObservationStamp {
            started_at: at,
            completed_at: at,
            generation: ModuleGeneration(1),
            epoch: CacheEpoch(1),
            source: ObservationSource::Supplied,
            freshness: Freshness::Fresh,
            consumed_effects: vec![],
        },
        sample: CmisReadinessSample::Supplied { bytes, offset: 0 },
    }
}

/// Begin a walk at a catalog revision, then tamper the caller-restored
/// state's revision to `rev`: `readiness_step` must revalidate the
/// supplied state's revision against the catalog at EVERY step and refuse
/// with [`CmisReadinessError::UnsupportedRevision`] — the generated
/// `CmisModuleState::from_rev` does not bind the revision, so an
/// unvalidated 5.5 state with a ModuleReady byte would otherwise decode
/// straight to `Manageable`.
#[test]
fn readiness_step_refuses_tampered_non_catalog_state() {
    let now = MonoTime(1_000);
    // Byte 3 field 3 = ModuleReady in bits 3:1.
    let ready_bytes: [u8; 4] = [0, 0, 0, 3 << 1];
    for rev in non_catalog_contexts() {
        let mut tampered = readiness_begin(
            Rev::CMIS_5_3,
            ModuleGeneration(1),
            CacheEpoch(1),
            now,
            CmisReadinessPolicy::schema_default(),
        )
        .expect("5.3 is a catalog revision");
        tampered.revision = rev;
        let result = readiness_step(
            &tampered,
            Some(&stamped_observation(&ready_bytes, now)),
            now,
            CmisReadinessPolicy::schema_default(),
        );
        assert!(
            matches!(result, Err(CmisReadinessError::UnsupportedRevision(rejected)) if rejected == rev),
            "{rev:?}: expected UnsupportedRevision, got {result:?}"
        );
    }
}

/// The revision revalidation runs BEFORE terminal replay: a tampered
/// state carrying an already-latched terminal decision is still refused,
/// never silently replayed.
#[test]
fn readiness_step_revalidates_revision_before_terminal_replay() {
    let now = MonoTime(1_000);
    let mut tampered = readiness_begin(
        Rev::CMIS_5_3,
        ModuleGeneration(1),
        CacheEpoch(1),
        now,
        CmisReadinessPolicy::schema_default(),
    )
    .expect("5.3 is a catalog revision");
    tampered.revision = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 5,
    };
    tampered.terminal = Some(CmisReadinessDecision::Manageable {
        state: CmisModuleState::ModuleReady,
    });
    assert!(matches!(
        readiness_step(
            &tampered,
            None,
            MonoTime(2_000),
            CmisReadinessPolicy::schema_default()
        ),
        Err(CmisReadinessError::UnsupportedRevision(_))
    ));
}

/// `readiness_step` accepts a fresh policy per call, so it revalidates it
/// with the same rules as `readiness_begin`: a zero poll interval (which
/// would schedule `PollAt { at: now }` and spin the caller) and an
/// interval exceeding the backstop are both refused with
/// [`CmisReadinessError::InvalidPolicy`] without consuming the step.
#[test]
fn readiness_step_rejects_invalid_policy_per_call() {
    let now = MonoTime(1_000);
    let state = readiness_begin(
        Rev::CMIS_5_3,
        ModuleGeneration(1),
        CacheEpoch(1),
        now,
        CmisReadinessPolicy::schema_default(),
    )
    .expect("5.3 is a catalog revision");
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
