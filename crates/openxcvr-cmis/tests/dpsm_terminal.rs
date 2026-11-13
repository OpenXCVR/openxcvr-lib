//! Revision-qualified, Tx-off-aware DPSM terminal semantics (Issue #1 Ask 2).
//!
//! Three contracts live here:
//!
//! 1. The Page 11h `dp_init_pending_lane` revision window: the field is
//!    introduced in CMIS 5.0 (Table 8-96); CMIS 3.0/4.0 reserve bytes
//!    235-239, so the per-revision Page 11h specialisations must not cover
//!    byte 235 before 5.0.
//! 2. The pure terminal predicate `DpState::is_terminal_for_rev` and its
//!    companion `expected_active_terminal_for_rev`, swept exhaustively over
//!    the six catalog revisions × 8 `DpState` variants × `tx_disabled` ×
//!    `dp_init_pending` (384 cells) against an independent literal oracle
//!    transcribed from the spec state machines — never derived from the
//!    predicate under test.
//! 3. Fail-closed revision gating: non-catalog revisions return
//!    `Err(DpsmError::UnsupportedRevision(_))` from every new entry point.
//!
//! Standards basis (per revision):
//!
//! - CMIS 3.0 (§ 1.5.3 Figure 9, Table 8, Table 66): a four-state machine —
//!   Deactivated, Init, Deinit, Activated. `DataPathActivated` is the sole
//!   steady terminal state, Tx controls never withhold it (Table 8: its exit
//!   conditions are host power/low-power requests only), and raw 0x5..=0x7
//!   are reserved (`Unknown(_)` after `from_nibble_for_rev`), so never
//!   terminal.
//! - CMIS 4.0 (Table 8-58, § 6.3.2.4, Table 6-17): eight states.
//!   `DPInitialized` is a steady state while `DPDeactivateS` stays TRUE —
//!   i.e. while Tx is disabled or force-squelched (Eq. 6-12/6-13 shape).
//!   Page 11h bytes 235-239 are Reserved in 4.0, so `dp_init_pending` has no
//!   meaning there and does not park `DPInitialized`.
//! - CMIS 5.0-5.3 (Table 8-84, § 6.3.3.6, Table 6-18): same Tx-off terminal
//!   as 4.0, plus `DPInitPendingLane` (Page 11h:235, since 5.0). Accepting
//!   `DPInitialized` while that bit is set is the host/adapter
//!   deferred-provisioning operational acceptance policy (matching
//!   the kernel module's `plugins/cmis/cmis_dpsm.h`: a deferred-init module
//!   parks lanes at `DPInitialized` until the apply trigger) — an
//!   explicitly approved operational stop point, not completed hardware
//!   commissioning, and not a normative consequence of the 5.x equations:
//!   with the pending bit set, Eq. 6-10 instead drives an
//!   intervention-free DPSM cycle through DPDeinit/DPDeactivated/DPInit,
//!   so the state machine itself does not park `DPInitialized`.

use openxcvr_cmis::adapter::{DpState, DpsmError};
use openxcvr_cmis::generated::{v3_0, v4_0, v5_3, CmisDataPathState};
use openxcvr_core::types::management::{ByteClassification, ChangeSource};
use openxcvr_core::{Family, Rev};

/// The generated per-revision Page 11h decoders are revision-qualified
/// (B1 regression): the v3_0 specialisation decodes `dp_state_host_lane`
/// through `CmisDataPathState::from_rev(0x30, ..)`, so the CMIS 3.0
/// Table 66 reserved raws 0x5..=0x7 fold into `Unknown(_)` instead of
/// borrowing the since-4.0 `DpTxTurnOn`/`DpTxTurnOff`/`DpInitialized`
/// names, while raws 0x1..=0x4 keep their revision-independent names. The
/// v4_0 specialisation accepts the full since-4.0 encoding: raw 0x7 is
/// `DpInitialized` there.
///
/// Raw 0x0 is the one reserved encoding the schema *names*: the
/// unannotated `reserved_0` entry applies at every revision, so
/// `from_rev` returns the named `Reserved0` variant, not `Unknown(0)`.
/// That is deliberately different from the adapter's `DpState` convention
/// (`from_nibble_for_rev` maps raw 0 to `DpState::Unknown(0)` because the
/// adapter enum has no `Reserved0` variant); both agree raw 0 is reserved,
/// never a real state and never terminal.
#[test]
fn generated_v3_0_page_11h_decode_is_revision_qualified() {
    let v3_0_raw: [(u8, CmisDataPathState); 8] = [
        (0x00, CmisDataPathState::Reserved0),
        (0x01, CmisDataPathState::DpDeactivated),
        (0x02, CmisDataPathState::DpInit),
        (0x03, CmisDataPathState::DpDeinit),
        (0x04, CmisDataPathState::DpActivated),
        (0x05, CmisDataPathState::Unknown(5)),
        (0x06, CmisDataPathState::Unknown(6)),
        (0x07, CmisDataPathState::Unknown(7)),
    ];
    for (raw, expected) in v3_0_raw {
        let mut buf = [0u8; 128];
        buf[0] = raw;
        assert_eq!(
            v3_0::CmisPage11hLaneStatus::decode(&buf).dp_state_host_lane[0],
            expected,
            "v3_0 Page 11h decode of dp_state raw {raw:#04x}"
        );
    }

    let mut buf = [0u8; 128];
    buf[0] = 0x07;
    assert_eq!(
        v4_0::CmisPage11hLaneStatus::decode(&buf).dp_state_host_lane[0],
        CmisDataPathState::DpInitialized,
        "v4_0 Page 11h decode of raw 0x07: the since-4.0 encoding names it"
    );
}

/// Page 11h byte 235 (`DPInitPendingLane<lane>`) is a CMIS 5.0 introduction:
/// CMIS 4.0 prints bytes 235-239 as Reserved. With `since: "5.0"` recorded in
/// `schemas/cmis/fields/page-11h-lane-status-part-05.yaml`, the 4.0
/// specialisation drops the field and classifies the byte fully uncovered,
/// while the 5.3 specialisation covers all eight lane bits.
#[test]
fn page_11h_byte_235_dp_init_pending_lane_is_covered_only_since_5_0() {
    let v4_0_classes = v4_0::CmisPage11hLaneStatus::read_classifications();
    // No field addresses byte 235 at 4.0: every bit is uncovered, nothing is
    // readable or unknown, and the byte carries no field-derived change
    // source. Page-relative index: offset 235 - page base 128.
    assert_eq!(
        v4_0_classes[235 - 128],
        ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
        "CMIS 4.0: Page 11h bytes 235-239 are reserved; byte 235 must be uncovered"
    );

    let v5_3_classes = v5_3::CmisPage11hLaneStatus::read_classifications();
    assert_eq!(
        v5_3_classes[235 - 128].uncovered_mask,
        0x00,
        "CMIS 5.3: DPInitPendingLane<1..8> covers every bit of Page 11h byte 235"
    );
    // The eight lane bits are accounted for as field coverage (readable or
    // conservatively unknown — the schema records no read effect for this
    // status bit, so the classification stays fail-closed, never invented).
    assert_eq!(
        v5_3_classes[235 - 128].readable_mask | v5_3_classes[235 - 128].unknown_mask,
        0xff,
        "CMIS 5.3: all eight lane bits of byte 235 must be accounted for"
    );
}

/// Page 11h bytes 132, 133 (`OutputStatusRx/Tx<lane>`) and 153
/// (`OutputStatusChangedFlagRx<lane>`) are CMIS 5.0 introductions: CMIS 3.0
/// Table 64 and CMIS 4.0 Table 8-56 print bytes 132-133 and 153 as Reserved,
/// and the CMIS 5.0 change log (physical page 5, printed page 3) lists
/// all three registers as new. With `since: "5.0"` recorded in
/// `schemas/cmis/fields/page-11h-lane-status-part-01.yaml` (bytes 132/133)
/// and `-part-03.yaml` (byte 153), the 4.0 specialisation drops the fields
/// and classifies the bytes fully uncovered, while the 5.3 specialisation
/// keeps them covered: byte 153 (a clear-on-read latched flag) reads as
/// readable, and bytes 132/133 (status bits with no recorded read effect)
/// are conservatively classified unknown — field coverage either way, never
/// uncovered.
#[test]
fn page_11h_output_status_bytes_132_133_153_are_covered_only_since_5_0() {
    let v4_0_classes = v4_0::CmisPage11hLaneStatus::read_classifications();
    // No field addresses bytes 132/133/153 at 4.0: every bit is uncovered,
    // nothing is readable or unknown, and the bytes carry no field-derived
    // change source. Page-relative indices: offsets 132/133/153 - 128.
    for (byte, index) in [(132, 4), (133, 5), (153, 25)] {
        assert_eq!(
            v4_0_classes[index],
            ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static),
            "CMIS 4.0: Page 11h byte {byte} is Reserved (Table 8-56); it must be uncovered"
        );
    }

    let v5_3_classes = v5_3::CmisPage11hLaneStatus::read_classifications();
    for (byte, index) in [(132, 4), (133, 5), (153, 25)] {
        assert_eq!(
            v5_3_classes[index].uncovered_mask, 0x00,
            "CMIS 5.3: OutputStatus register on Page 11h byte {byte} covers every bit"
        );
        // The eight lane bits are accounted for as field coverage: readable
        // (byte 153, clear-on-read latched flag) or conservatively unknown
        // (bytes 132/133, status bits with no recorded read effect) — the
        // classification stays fail-closed, never invented.
        assert_eq!(
            v5_3_classes[index].readable_mask | v5_3_classes[index].unknown_mask,
            0xff,
            "CMIS 5.3: all eight lane bits of byte {byte} must be accounted for"
        );
    }
    // Byte 153 specifically carries the latched OutputStatusChangedFlagRx
    // bits: a real readable field at 5.3 (readable_mask set), not merely
    // conservatively unknown.
    assert_eq!(
        v5_3_classes[25].readable_mask, 0xff,
        "CMIS 5.3: OutputStatusChangedFlagRx<1..8> is a readable clear-on-read flag"
    );
}

/// The six DPSM catalog revisions: the exact set for which
/// `CmisDataPathState::for_revision` binds a schema-owned state table.
const CATALOG: [Rev; 6] = [
    Rev::CMIS_3_0,
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
];

/// The eight `DpState` variants, including the reserved-raw catch-all.
const STATES: [DpState; 8] = [
    DpState::Deactivated,
    DpState::Init,
    DpState::Deinit,
    DpState::Activated,
    DpState::TxTurnOn,
    DpState::TxTurnOff,
    DpState::Initialized,
    DpState::Unknown(0xA),
];

/// Independent literal oracle for the terminal predicate, transcribed from
/// the per-revision state machines (see the module docs for citations):
///
/// | rev | `tx_disabled` | `dp_init_pending` | terminal states |
/// |---|---|---|---|
/// | 3.0 | any | any | `Activated` only |
/// | 4.0 | false | any | `Activated` only |
/// | 4.0 | true | any | `Activated`, `Initialized` |
/// | 5.0-5.3 | false | false | `Activated` only |
/// | 5.0-5.3 | true or pending | any | `Activated`, `Initialized` |
/// | any | — | — | `Unknown(_)`: never |
fn oracle_terminal(rev: Rev, state: DpState, tx_disabled: bool, dp_init_pending: bool) -> bool {
    match rev {
        // 3.0 Figure 9/Table 8: Tx controls never withhold Activated, and
        // the pending bit does not exist.
        Rev::CMIS_3_0 => matches!(state, DpState::Activated),
        // 4.0: Tx-off parks DPInitialized (§ 6.3.2.4), but Page 11h:235 is
        // reserved, so dp_init_pending contributes nothing.
        Rev::CMIS_4_0 => {
            matches!(state, DpState::Activated)
                || (tx_disabled && matches!(state, DpState::Initialized))
        }
        // 5.0-5.3: the Tx-off leg is the normative Initialized terminal
        // (§ 6.3.3.6, Eq. 6-12/6-13); DPInitPending acceptance is the
        // host/adapter deferred-provisioning operational policy (kmod
        // cmis_dpsm.h parity), not an Eq. 6-10 consequence.
        _ => {
            matches!(state, DpState::Activated)
                || ((tx_disabled || dp_init_pending) && matches!(state, DpState::Initialized))
        }
    }
}

/// Independent literal oracle for the per-revision state decode, transcribed
/// from the spec tables (3.0 Table 66; 4.0+ Table 8-58/8-84): raws 1-4 named
/// everywhere; raws 5-7 named only from 4.0 on; raw 0 and 8-15 reserved at
/// every revision.
fn oracle_state(rev: Rev, raw: u8) -> DpState {
    match (rev, raw) {
        (_, 0x1) => DpState::Deactivated,
        (_, 0x2) => DpState::Init,
        (_, 0x3) => DpState::Deinit,
        (_, 0x4) => DpState::Activated,
        (Rev::CMIS_3_0, _) => DpState::Unknown(raw),
        (_, 0x5) => DpState::TxTurnOn,
        (_, 0x6) => DpState::TxTurnOff,
        (_, 0x7) => DpState::Initialized,
        (_, other) => DpState::Unknown(other),
    }
}

/// Full 384-cell matrix: 6 catalog revisions × 16 raw nibbles ×
/// `tx_disabled` × `dp_init_pending`. Every cell pins BOTH the
/// revision-qualified decode (`from_nibble_for_rev` must agree with the
/// literal state oracle — the sixteen raws project onto all eight
/// `DpState` variants) AND the terminal predicate against the literal
/// terminal oracle. A failure names the exact (revision, raw, flags) cell.
#[test]
fn terminal_matrix_384_cells_six_revisions_sixteen_raws_both_flags() {
    let mut cells = 0usize;
    for rev in CATALOG {
        for raw in 0..=15u8 {
            let decoded = DpState::from_nibble_for_rev(raw, rev);
            let expected_state = oracle_state(rev, raw);
            assert_eq!(decoded, expected_state, "decode at {rev:?} raw {raw:#04x}");
            for tx_disabled in [false, true] {
                for dp_init_pending in [false, true] {
                    let expected =
                        oracle_terminal(rev, expected_state, tx_disabled, dp_init_pending);
                    assert_eq!(
                        decoded
                            .is_terminal_for_rev(rev, tx_disabled, dp_init_pending)
                            .unwrap(),
                        expected,
                        "{rev:?} raw {raw:#04x} ({expected_state:?}) \
                         tx_disabled={tx_disabled} dp_init_pending={dp_init_pending}"
                    );
                    cells += 1;
                }
            }
        }
    }
    assert_eq!(
        cells, 384,
        "six revisions × sixteen raws × two flags × two flags"
    );
}

/// The eight `DpState` variants swept directly (192 cells): the named
/// states, the Tx transients, and the reserved-raw catch-all — independent
/// of how any raw happens to decode.
#[test]
fn terminal_matrix_eight_variants_direct() {
    let mut cells = 0usize;
    for rev in CATALOG {
        for state in STATES {
            for tx_disabled in [false, true] {
                for dp_init_pending in [false, true] {
                    let expected = oracle_terminal(rev, state, tx_disabled, dp_init_pending);
                    assert_eq!(
                        state
                            .is_terminal_for_rev(rev, tx_disabled, dp_init_pending)
                            .unwrap(),
                        expected,
                        "{rev:?} {state:?} tx_disabled={tx_disabled} \
                         dp_init_pending={dp_init_pending}"
                    );
                    cells += 1;
                }
            }
        }
    }
    assert_eq!(
        cells, 192,
        "six revisions × eight variants × two flags × two flags"
    );
}

/// CMIS 3.0 refutation of a blanket Tx-off rule: with `tx_disabled = true`,
/// `Initialized` (a state that does not even exist in the 3.0 machine) and
/// every other non-`Activated` state stay non-terminal; only `Activated`
/// settles.
#[test]
fn cmis_3_0_tx_disabled_still_requires_activated() {
    for tx_disabled in [false, true] {
        for dp_init_pending in [false, true] {
            assert!(
                !DpState::Initialized
                    .is_terminal_for_rev(Rev::CMIS_3_0, tx_disabled, dp_init_pending)
                    .unwrap(),
                "3.0 tx_disabled={tx_disabled}: Initialized does not exist in the 3.0 machine"
            );
            assert!(
                DpState::Activated
                    .is_terminal_for_rev(Rev::CMIS_3_0, tx_disabled, dp_init_pending)
                    .unwrap(),
                "3.0 tx_disabled={tx_disabled}: Activated is the sole terminal state"
            );
        }
    }
}

/// CMIS 3.0 reserves raws 0x5..=0x7 (Table 66); `from_nibble_for_rev`
/// preserves them as `Unknown(raw)`, and `Unknown(_)` is never terminal at
/// any revision — a reserved encoding is not a Tx-off parking state.
#[test]
fn cmis_3_0_reserved_raw_0x7_is_unknown_and_never_terminal() {
    assert_eq!(
        DpState::from_nibble_for_rev(0x7, Rev::CMIS_3_0),
        DpState::Unknown(7)
    );
    for tx_disabled in [false, true] {
        for dp_init_pending in [false, true] {
            assert!(
                !DpState::Unknown(7)
                    .is_terminal_for_rev(Rev::CMIS_3_0, tx_disabled, dp_init_pending)
                    .unwrap(),
                "3.0 raw 0x7 is reserved: tx/pending flags cannot make it terminal"
            );
        }
    }
    // Same for the other 3.0-reserved Tx raws, at every catalog revision:
    // Unknown(_) is never terminal anywhere.
    for rev in CATALOG {
        for raw in [0x5u8, 0x6, 0x7] {
            assert!(
                !DpState::Unknown(raw)
                    .is_terminal_for_rev(rev, true, true)
                    .unwrap(),
                "{rev:?}: Unknown({raw:#04x}) is never terminal"
            );
        }
    }
}

/// The 4.0/5.0 divide for `dp_init_pending`: Page 11h byte 235 is reserved
/// in CMIS 4.0, so a set pending bit with Tx enabled must NOT make
/// `Initialized` terminal there — while at 5.0-5.3 the deferred-provisioning
/// operational acceptance policy does.
#[test]
fn dp_init_pending_accepts_initialized_only_since_5_0() {
    // 4.0, Tx enabled, pending set: Initialized is NOT terminal.
    assert!(
        !DpState::Initialized
            .is_terminal_for_rev(Rev::CMIS_4_0, false, true)
            .unwrap(),
        "4.0: byte 235 is reserved, so dp_init_pending cannot park Initialized"
    );
    // 5.0-5.3, Tx enabled, pending set: Initialized IS terminal (the
    // deferred-provisioning operational acceptance policy).
    for rev in [Rev::CMIS_5_0, Rev::CMIS_5_1, Rev::CMIS_5_2, Rev::CMIS_5_3] {
        assert!(
            DpState::Initialized
                .is_terminal_for_rev(rev, false, true)
                .unwrap(),
            "{rev:?}: deferred-provisioning acceptance makes Initialized terminal"
        );
    }
    // And the Tx-off leg is terminal at both 4.0 and 5.3.
    assert!(DpState::Initialized
        .is_terminal_for_rev(Rev::CMIS_4_0, true, false)
        .unwrap());
    assert!(DpState::Initialized
        .is_terminal_for_rev(Rev::CMIS_5_3, true, false)
        .unwrap());
}

/// Independent literal oracle for the expected active-terminal state.
fn oracle_expected(rev: Rev, tx_disabled: bool, dp_init_pending: bool) -> DpState {
    match rev {
        Rev::CMIS_3_0 => DpState::Activated,
        Rev::CMIS_4_0 => {
            if tx_disabled {
                DpState::Initialized
            } else {
                DpState::Activated
            }
        }
        _ => {
            if tx_disabled || dp_init_pending {
                DpState::Initialized
            } else {
                DpState::Activated
            }
        }
    }
}

/// 24-cell expected-terminal matrix, plus the coherence invariant: the
/// state a successful activation must reach is always terminal under the
/// same flags.
#[test]
fn expected_active_terminal_matrix_and_coherence() {
    let mut cells = 0usize;
    for rev in CATALOG {
        for tx_disabled in [false, true] {
            for dp_init_pending in [false, true] {
                let expected = oracle_expected(rev, tx_disabled, dp_init_pending);
                assert_eq!(
                    DpState::expected_active_terminal_for_rev(rev, tx_disabled, dp_init_pending)
                        .unwrap(),
                    expected,
                    "{rev:?} tx_disabled={tx_disabled} dp_init_pending={dp_init_pending}"
                );
                // Coherence: the expected terminal is itself terminal.
                assert!(
                    expected
                        .is_terminal_for_rev(rev, tx_disabled, dp_init_pending)
                        .unwrap(),
                    "{rev:?}: expected terminal {expected:?} must satisfy the predicate"
                );
                cells += 1;
            }
        }
    }
    assert_eq!(cells, 24, "six revisions × two flags × two flags");
}

/// Non-catalog revisions fail closed from every pure entry point:
/// the 3.1/4.1/5.5 neighbors, a foreign family (SFF-8472 3.0), and
/// C-CMIS 1.4.
#[test]
fn non_catalog_revisions_fail_closed_from_pure_entry_points() {
    let non_catalog = [
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
            family: Family::Cmis,
            major: 5,
            minor: 5,
        },
        Rev {
            family: Family::Sff8472,
            major: 3,
            minor: 0,
        },
        Rev {
            family: Family::CCmis,
            major: 1,
            minor: 4,
        },
    ];
    for rev in non_catalog {
        for tx_disabled in [false, true] {
            for dp_init_pending in [false, true] {
                assert!(
                    matches!(
                        DpState::Activated.is_terminal_for_rev(rev, tx_disabled, dp_init_pending),
                        Err(DpsmError::UnsupportedRevision(rejected)) if rejected == rev
                    ),
                    "{rev:?}: is_terminal_for_rev must fail closed"
                );
                assert!(
                    matches!(
                        DpState::expected_active_terminal_for_rev(rev, tx_disabled, dp_init_pending),
                        Err(DpsmError::UnsupportedRevision(rejected)) if rejected == rev
                    ),
                    "{rev:?}: expected_active_terminal_for_rev must fail closed"
                );
            }
        }
    }
}

/// The six catalog revisions round-trip through the validator's byte
/// packing — the public predicate accepts exactly the catalog set.
#[test]
fn catalog_revisions_are_accepted() {
    for rev in CATALOG {
        assert!(
            DpState::Activated
                .is_terminal_for_rev(rev, false, false)
                .is_ok(),
            "{rev:?} is a catalog revision and must be accepted"
        );
    }
}
