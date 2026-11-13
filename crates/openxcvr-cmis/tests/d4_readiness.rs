//! Software-only internal-D4 traces; arbitrary nanosecond policy is test data, not a default.
use openxcvr_cmis::{
    generated::{v4_0::CmisLowerMemory, CmisModuleState},
    readiness::*,
};
use openxcvr_core::{supplied::SpanError, types::management::*, Rev};

fn policy() -> Cmis4ReadinessPolicy {
    Cmis4ReadinessPolicy::new(MonoDuration(100), MonoDuration(20)).unwrap()
}
fn begin() -> Cmis4Readiness {
    Cmis4Readiness::new(
        Rev::CMIS_4_0,
        ModuleGeneration(1),
        CacheEpoch(2),
        MonoTime(10),
        policy(),
    )
    .unwrap()
}
fn stamp(at: u64) -> ObservationStamp {
    ObservationStamp {
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        started_at: MonoTime(at),
        completed_at: MonoTime(at),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    }
}
fn obs(at: u64, sample: Cmis4ReadinessSample<'_>) -> Cmis4ReadinessObservation<'_> {
    Cmis4ReadinessObservation {
        stamp: stamp(at),
        sample,
    }
}
fn supplied(bytes: &[u8]) -> Cmis4ReadinessSample<'_> {
    Cmis4ReadinessSample::Supplied { bytes, offset: 3 }
}
fn poll(
    result: Cmis4ReadinessOutcome,
    next: u64,
    state: Option<CmisModuleState>,
    error: Option<Cmis4ReadinessTransportError>,
) {
    assert_eq!(
        result,
        Cmis4ReadinessOutcome::PollAt {
            next_poll_at: MonoTime(next),
            deadline: MonoTime(110),
            last_state: state,
            last_error: error
        }
    );
}
fn timeout(
    state: Option<CmisModuleState>,
    error: Option<Cmis4ReadinessTransportError>,
) -> Cmis4ReadinessOutcome {
    Cmis4ReadinessOutcome::Timeout {
        deadline: MonoTime(110),
        last_state: state,
        last_error: error,
    }
}

#[test]
fn supplied_decoder_covers_entire_byte_without_padding_or_evaluating_reserved_bits() {
    assert_eq!(CmisLowerMemory::MODULE_STATE_RANGE, (3, 1));
    for byte in 0..=255u8 {
        assert_eq!(
            CmisLowerMemory::decode_module_state_supplied(&[byte], 3).unwrap(),
            CmisModuleState::from((byte >> 1) & 7)
        );
    }
    for (bytes, offset) in [(&[][..], 3), (&[0][..], 2), (&[0][..], 4)] {
        assert_eq!(
            CmisLowerMemory::decode_module_state_supplied(bytes, offset),
            Err(SpanError::InsufficientInput { offset: 3, len: 1 })
        );
    }
    for (bytes, offset) in [(&[0][..], 256), (&[][..], usize::MAX)] {
        assert!(matches!(
            CmisLowerMemory::decode_module_state_supplied(bytes, offset),
            Err(SpanError::InvalidSpan { .. })
        ));
    }
}
#[test]
fn initial_nack_to_lowpwr_and_pwrup_to_ready() {
    let mut wait = begin();
    poll(
        wait.step(
            MonoTime(10),
            Some(&obs(10, Cmis4ReadinessSample::StartupNack)),
        )
        .unwrap(),
        30,
        None,
        Some(Cmis4ReadinessTransportError::StartupNack),
    );
    assert_eq!(
        wait.step(MonoTime(30), Some(&obs(30, supplied(&[2]))))
            .unwrap(),
        Cmis4ReadinessOutcome::Manageable(CmisModuleState::ModuleLowPwr)
    );
    let mut wait = begin();
    poll(
        wait.step(MonoTime(10), Some(&obs(10, supplied(&[4]))))
            .unwrap(),
        30,
        Some(CmisModuleState::ModulePwrUp),
        None,
    );
    assert_eq!(
        wait.step(MonoTime(30), Some(&obs(30, supplied(&[6]))))
            .unwrap(),
        Cmis4ReadinessOutcome::Manageable(CmisModuleState::ModuleReady)
    );
}
#[test]
fn repeated_transport_errors_and_all_nonmanageable_codes_exhaust_same_anchor() {
    for code in [0, 2, 4, 5, 6, 7] {
        let mut wait = begin();
        let state = Some(CmisModuleState::from(code));
        let data = [code << 1];
        poll(
            wait.step(MonoTime(10), Some(&obs(10, supplied(&data))))
                .unwrap(),
            30,
            state,
            None,
        );
        for at in [30, 50, 70, 90, 109] {
            let sample = if at == 30 {
                Cmis4ReadinessSample::StartupNack
            } else {
                Cmis4ReadinessSample::TransientError
            };
            let error = Some(if at == 30 {
                Cmis4ReadinessTransportError::StartupNack
            } else {
                Cmis4ReadinessTransportError::TransientError
            });
            poll(
                wait.step(MonoTime(at), Some(&obs(at, sample))).unwrap(),
                (at + 20).min(110),
                state,
                error,
            );
        }
        assert_eq!(
            wait.step(MonoTime(110), Some(&obs(110, supplied(&data))))
                .unwrap(),
            timeout(state, Some(Cmis4ReadinessTransportError::TransientError))
        );
    }
    let mut wait = begin();
    for at in [10, 30, 50, 70, 90] {
        poll(
            wait.step(
                MonoTime(at),
                Some(&obs(at, Cmis4ReadinessSample::StartupNack)),
            )
            .unwrap(),
            at + 20,
            None,
            Some(Cmis4ReadinessTransportError::StartupNack),
        );
    }
    assert_eq!(
        wait.step(
            MonoTime(110),
            Some(&obs(110, Cmis4ReadinessSample::StartupNack))
        )
        .unwrap(),
        timeout(None, Some(Cmis4ReadinessTransportError::StartupNack))
    );
}
#[test]
fn state_changes_do_not_renew_and_late_manageable_cannot_win() {
    let mut wait = begin();
    for (at, code) in [(10, 2), (60, 4), (100, 5)] {
        poll(
            wait.step(MonoTime(at), Some(&obs(at, supplied(&[code << 1]))))
                .unwrap(),
            (at + 20).min(110),
            Some(CmisModuleState::from(code)),
            None,
        );
    }
    assert_eq!(
        wait.step(MonoTime(111), Some(&obs(111, supplied(&[2]))))
            .unwrap(),
        timeout(Some(CmisModuleState::ModuleFault), None)
    );
    // Timeout is terminal even if a client supplies a new arrival.
    assert_eq!(
        wait.step(MonoTime(112), Some(&obs(112, supplied(&[6]))))
            .unwrap(),
        timeout(Some(CmisModuleState::ModuleFault), None)
    );
}
#[test]
fn exact_deadline_accepts_only_lowpwr_or_ready_and_can_expire_without_io() {
    for code in 0..=7 {
        let mut wait = begin();
        let result = wait
            .step(MonoTime(110), Some(&obs(110, supplied(&[code << 1]))))
            .unwrap();
        assert_eq!(
            result,
            if code == 1 || code == 3 {
                Cmis4ReadinessOutcome::Manageable(CmisModuleState::from(code))
            } else {
                timeout(Some(CmisModuleState::from(code)), None)
            }
        );
    }
    assert_eq!(
        begin().step(MonoTime(110), None).unwrap(),
        timeout(None, None)
    );
}
#[test]
fn policy_revision_and_time_fail_closed() {
    for (deadline, interval) in [(0, 1), (100, 0), (10, 11)] {
        assert_eq!(
            Cmis4ReadinessPolicy::new(MonoDuration(deadline), MonoDuration(interval)),
            Err(Cmis4ReadinessError::InvalidPolicy)
        );
    }
    for rev in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_3,
        Rev {
            family: openxcvr_core::rev::Family::Sff8636,
            major: 0,
            minor: 0,
        },
    ] {
        assert_eq!(
            Cmis4Readiness::new(
                rev,
                ModuleGeneration(1),
                CacheEpoch(2),
                MonoTime(10),
                policy()
            )
            .unwrap_err(),
            Cmis4ReadinessError::UnsupportedRevision(rev)
        );
    }
    assert_eq!(
        Cmis4Readiness::new(
            Rev::CMIS_4_0,
            ModuleGeneration(1),
            CacheEpoch(2),
            MonoTime(u64::MAX),
            policy()
        )
        .unwrap_err(),
        Cmis4ReadinessError::TimeOverflow
    );
    assert_eq!(
        begin().step(MonoTime(9), None),
        Err(Cmis4ReadinessError::ReversedTime)
    );
    let p = Cmis4ReadinessPolicy::new(MonoDuration(100), MonoDuration(100)).unwrap();
    let mut wait = Cmis4Readiness::new(
        Rev::CMIS_4_0,
        ModuleGeneration(1),
        CacheEpoch(2),
        MonoTime(u64::MAX - 100),
        p,
    )
    .unwrap();
    assert_eq!(
        wait.step(MonoTime(u64::MAX - 1), None),
        Err(Cmis4ReadinessError::TimeOverflow)
    );
}
#[test]
fn rejects_missing_stale_historical_future_and_reversed_observations() {
    assert!(matches!(
        begin().step(MonoTime(10), Some(&obs(10, supplied(&[])))),
        Err(Cmis4ReadinessError::Span(
            SpanError::InsufficientInput { .. }
        ))
    ));
    for variant in 0..6 {
        let mut observation = obs(10, supplied(&[2]));
        match variant {
            0 => observation.stamp.generation = ModuleGeneration(9),
            1 => observation.stamp.epoch = CacheEpoch(9),
            2 => observation.stamp.freshness = Freshness::Historical,
            3 => observation.stamp.started_at = MonoTime(9),
            4 => observation.stamp.completed_at = MonoTime(11),
            _ => observation.stamp.started_at = MonoTime(11),
        }
        assert_eq!(
            begin().step(MonoTime(10), Some(&observation)),
            Err(if variant == 5 {
                Cmis4ReadinessError::ReversedTime
            } else {
                Cmis4ReadinessError::StaleObservation
            })
        );
    }
}
#[test]
fn bridge_absence_is_distinct_immediate_terminal_not_a_startup_nack() {
    let mut wait = begin();
    poll(
        wait.step(MonoTime(10), Some(&obs(10, supplied(&[4]))))
            .unwrap(),
        30,
        Some(CmisModuleState::ModulePwrUp),
        None,
    );
    assert_eq!(
        wait.step(
            MonoTime(11),
            Some(&obs(11, Cmis4ReadinessSample::BridgeLost))
        )
        .unwrap(),
        Cmis4ReadinessOutcome::BridgeLost {
            last_state: Some(CmisModuleState::ModulePwrUp),
            last_error: Cmis4ReadinessTransportError::BridgeLost
        }
    );
    assert!(matches!(
        wait.step(MonoTime(30), Some(&obs(30, supplied(&[2]))))
            .unwrap(),
        Cmis4ReadinessOutcome::BridgeLost { .. }
    ));
}

#[test]
fn manageable_near_max_time_needs_no_next_poll_and_invalid_input_does_not_mutate() {
    let p = Cmis4ReadinessPolicy::new(MonoDuration(100), MonoDuration(100)).unwrap();
    let at = u64::MAX - 100;
    let mut wait = Cmis4Readiness::new(
        Rev::CMIS_4_0,
        ModuleGeneration(1),
        CacheEpoch(2),
        MonoTime(at),
        p,
    )
    .unwrap();
    assert_eq!(
        wait.step(
            MonoTime(u64::MAX - 1),
            Some(&obs(u64::MAX - 1, supplied(&[2])))
        )
        .unwrap(),
        Cmis4ReadinessOutcome::Manageable(CmisModuleState::ModuleLowPwr)
    );
    let mut wait = begin();
    assert!(wait
        .step(MonoTime(20), Some(&obs(20, supplied(&[]))))
        .is_err());
    poll(wait.step(MonoTime(20), None).unwrap(), 40, None, None);
    assert_eq!(
        wait.step(MonoTime(111), Some(&obs(110, supplied(&[6]))))
            .unwrap(),
        timeout(None, None)
    );
}
