use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE, Family, Rev};

use openxcvr_core::fsm::{FsmTable, FsmTransition};

use crate::generated::fsm::cmis_data_path_state_machine;
use crate::generated::CmisDataPathState;
use crate::module::CmisTransport;

const DPSM_CONTROL_PAGE: u8 = 0x10;
const DPSM_STATE_PAGE: u8 = 0x11;
const DPSM_STATE_OFFSET: u8 = 128;
const DPSM_DEACTIVATE_OFFSET: u8 = 128;
const DPSM_ACTIVATE_OFFSET: u8 = 143;
const MAX_DP_ID: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One data path's state-machine state (Page 11h data path state nibble).
pub enum DpState {
    /// The data path is deactivated.
    Deactivated,
    /// The data path is initializing.
    Init,
    /// The data path is de-initializing.
    Deinit,
    /// The data path is activated.
    Activated,
    /// Transmit optics are turning on.
    TxTurnOn,
    /// Transmit optics are turning off.
    TxTurnOff,
    /// The data path is fully initialized.
    Initialized,
    /// A reserved code the revision does not name, carried raw.
    Unknown(u8),
}

impl DpState {
    /// Decode a Data Path State nibble according to CMIS 4.0+ (CMIS 4.0 Table 8-58;
    /// CMIS 5.3 Table 8-84).
    pub fn from_nibble(raw: u8) -> Self {
        match raw {
            0x1 => Self::Deactivated,
            0x2 => Self::Init,
            0x3 => Self::Deinit,
            0x4 => Self::Activated,
            0x5 => Self::TxTurnOn,
            0x6 => Self::TxTurnOff,
            0x7 => Self::Initialized,
            other => Self::Unknown(other),
        }
    }

    /// Decode a Data Path State nibble according to CMIS 3.0 (CMIS 3.0 Table 66).
    ///
    /// CMIS 3.0 defines only states 0x1 (Deactivated), 0x2 (Init), 0x3 (Deinit),
    /// and 0x4 (Activated). Encodings 0x0 and 0x5..=0xF are reserved and return
    /// [`DpState::Unknown(raw)`][DpState::Unknown]. Equivalent to
    /// [`from_nibble_for_rev`](Self::from_nibble_for_rev) at
    /// [`Rev::CMIS_3_0`].
    pub fn from_nibble_v3(raw: u8) -> Self {
        Self::from_nibble_for_rev(raw, Rev::CMIS_3_0)
    }

    /// Decode a Data Path State nibble for a specific CMIS revision context.
    ///
    /// Qualified conversion (schema-owned since the revision-window work):
    /// a raw nibble maps to a named variant only when the revision context is
    /// exactly one of the six catalog revisions AND the schema-owned generated
    /// projection names that raw there. The sequence is:
    ///
    /// 1. the context must be CMIS-family, with both version components
    ///    packable into a MajorRevision byte (`major <= 15`, `minor <= 15`);
    /// 2. the packed byte must round-trip through
    ///    `Rev::from_major_byte_cmis` back to exactly this `rev` — so
    ///    3.1/4.1/5.5 or any other non-catalog context never borrows a
    ///    catalog table;
    /// 3. the generated factory
    ///    [`CmisDataPathState::for_revision`](crate::generated::CmisDataPathState::for_revision)
    ///    must bind for that byte;
    /// 4. a *nonzero* raw named by that revision's projection selects the
    ///    unchanged typed [`from_nibble`](Self::from_nibble) conversion.
    ///
    /// Everything else — out-of-catalog contexts, raw 0 (reserved in every
    /// revision, rendered `Reserved0` only at the display layer), reserved
    /// raws 5..=7 at pre-4.0 revisions, and raws 8..=255 — returns
    /// [`DpState::Unknown(raw)`][DpState::Unknown], preserving the raw
    /// losslessly (no masking; raw 0x10 stays `Unknown(16)`). The semantic
    /// conversion is never the string authority: rendering is the generated
    /// projection's job.
    pub fn from_nibble_for_rev(raw: u8, rev: Rev) -> Self {
        if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
            let byte = (rev.major << 4) | rev.minor;
            if Rev::from_major_byte_cmis(byte) == Some(rev) {
                if let Some(project) = crate::generated::CmisDataPathState::for_revision(byte) {
                    if raw != 0 && project(raw).is_some() {
                        return Self::from_nibble(raw);
                    }
                }
            }
        }
        Self::Unknown(raw)
    }

    /// Terminal-for-`rev` predicate for a Data Path state, Tx-off aware.
    /// Pure: no I/O, no transport — the caller supplies already-projected
    /// facts.
    ///
    /// * `tx_disabled` is the Data-Path-wide disjunction of the Tx output
    ///   controls (OutputDisableTx at Page 10h byte 130 and
    ///   OutputSquelchForceTx at byte 132 — Eq. 6-12/6-13:
    ///   `DPDeactivateS = DPReDeinitS OR DPTxDisableT OR DPTxForceSquelchT`).
    ///   A single disabled or squelched media lane parks the whole Data
    ///   Path; the caller owns the media-lane → host-lane projection. The
    ///   name is revision-neutral because byte 130 prints `TxN Disable` in
    ///   3.0/4.0 and `OutputDisableTxN` in 5.0+ — same bit, same polarity.
    /// * `dp_init_pending` is this lane's `DPInitPendingLane` bit
    ///   (Page 11h byte 235, since 5.0). Accepting `Initialized` while
    ///   that bit is set is the host/adapter deferred-provisioning
    ///   operational acceptance policy, matching the kernel module's
    ///   `plugins/cmis/cmis_dpsm.h` (a deferred-init module parks lanes
    ///   at `DPInitialized` until the apply trigger): it is an
    ///   operational stop point, not completed hardware commissioning,
    ///   and not a normative consequence of the 5.x equations — Eq. 6-10
    ///   (`DPReinitT = DPInitPendingLane`) instead drives an
    ///   intervention-free reconfiguration cycle through
    ///   DPDeinit/DPDeactivated/DPInit rather than parking
    ///   `DPInitialized`. At 4.0 the byte is reserved, so the flag
    ///   contributes nothing there (it is ignored, not an error — a 4.0
    ///   caller reading byte 235 sees reserved zeros).
    ///
    /// Per-revision terminal set:
    ///
    /// * CMIS 3.0 (Figure 9, Table 8, Table 66): `Activated` only. The 3.0
    ///   machine has four states and no Tx parking state — the Tx controls
    ///   never withhold `DataPathActivated` (Table 8 lists only host
    ///   power/low-power exits), and `dp_init_pending` does not exist.
    ///   Reserved raws 0x5..=0x7 arrive as `Unknown(_)` and are never
    ///   terminal.
    /// * CMIS 4.0 (Table 8-58, § 6.3.2.4, Table 6-17): `Activated`, plus
    ///   `Initialized` while `tx_disabled` — `DPInitialized` is a steady
    ///   state while `DPDeactivateS` stays TRUE.
    /// * CMIS 5.0-5.3 (Table 8-84, § 6.3.3.6, Table 6-18): as 4.0, plus
    ///   `Initialized` while `dp_init_pending` — the deferred-provisioning
    ///   operational acceptance policy (see above); an approved host-side
    ///   choice, not a printed state-table row and not a derived
    ///   consequence of the exit equations.
    ///
    /// Every other state — `Deactivated`, `Init`, `Deinit`, `TxTurnOn`,
    /// `TxTurnOff`, `Unknown(_)` — is transient or reserved at every
    /// revision and never terminal.
    ///
    /// Fails closed with [`DpsmError::UnsupportedRevision`] for any
    /// non-catalog revision context (3.1, 4.1, 5.5, foreign families) —
    /// decided by the same packed-byte round-trip as
    /// [`from_nibble_for_rev`](Self::from_nibble_for_rev), so the two can
    /// never disagree about which revisions are in the catalog.
    pub fn is_terminal_for_rev(
        self,
        rev: Rev,
        tx_disabled: bool,
        dp_init_pending: bool,
    ) -> Result<bool, DpsmError> {
        validate_catalog_cmis_rev(rev)?;
        Ok(match rev {
            Rev::CMIS_3_0 => matches!(self, Self::Activated),
            Rev::CMIS_4_0 => {
                matches!(self, Self::Activated)
                    || (tx_disabled && matches!(self, Self::Initialized))
            }
            // 5.0–5.3 (the only remaining catalog revisions).
            _ => {
                matches!(self, Self::Activated)
                    || ((tx_disabled || dp_init_pending) && matches!(self, Self::Initialized))
            }
        })
    }

    /// The single state a successful data-path activation must reach for
    /// `rev`, given the same projected Tx/pending facts as
    /// [`is_terminal_for_rev`](Self::is_terminal_for_rev):
    ///
    /// * CMIS 3.0: `Activated` (regardless of `tx_disabled` — the 3.0
    ///   machine has no Tx parking state).
    /// * CMIS 4.0: `Initialized` when Tx is held off, else `Activated`.
    /// * CMIS 5.0–5.3: `Initialized` when Tx is held off or init is pending,
    ///   else `Activated`.
    ///
    /// The returned state always satisfies `is_terminal_for_rev` under the
    /// same flags. Fails closed with [`DpsmError::UnsupportedRevision`] for
    /// non-catalog revision contexts.
    pub fn expected_active_terminal_for_rev(
        rev: Rev,
        tx_disabled: bool,
        dp_init_pending: bool,
    ) -> Result<Self, DpsmError> {
        validate_catalog_cmis_rev(rev)?;
        Ok(match rev {
            Rev::CMIS_3_0 => Self::Activated,
            Rev::CMIS_4_0 => {
                if tx_disabled {
                    Self::Initialized
                } else {
                    Self::Activated
                }
            }
            // 5.0–5.3 (the only remaining catalog revisions).
            _ => {
                if tx_disabled || dp_init_pending {
                    Self::Initialized
                } else {
                    Self::Activated
                }
            }
        })
    }
}

/// Validate that `rev` is one of the six DPSM catalog revisions (3.0, 4.0,
/// 5.0, 5.1, 5.2, 5.3) and return its packed MajorRevision byte.
///
/// The check is deliberately identical to the one inside
/// [`DpState::from_nibble_for_rev`]: the context must be CMIS-family with
/// both version components packable into a MajorRevision byte
/// (`major <= 15`, `minor <= 15`), the packed byte must round-trip through
/// [`Rev::from_major_byte_cmis`] back to exactly this `rev`, and the
/// schema-owned generated projection
/// [`CmisDataPathState::for_revision`](crate::generated::CmisDataPathState::for_revision)
/// must bind for that byte. Anything else — the 3.1/4.1/5.5 neighbors,
/// foreign families, unpackable components — fails closed with
/// [`DpsmError::UnsupportedRevision`] so a revision-qualified entry point
/// never borrows a catalog state machine it was not given.
fn validate_catalog_cmis_rev(rev: Rev) -> Result<u8, DpsmError> {
    if rev.family == Family::Cmis && rev.major <= 15 && rev.minor <= 15 {
        let byte = (rev.major << 4) | rev.minor;
        if Rev::from_major_byte_cmis(byte) == Some(rev)
            && crate::generated::CmisDataPathState::for_revision(byte).is_some()
        {
            return Ok(byte);
        }
    }
    Err(DpsmError::UnsupportedRevision(rev))
}

// ── Generated-enum deduplication ──────────────────────────────────────
//
// `DpState` keeps its long-public variant names (Deactivated, Init, ..)
// so every existing consumer keeps compiling; the lossless conversions
// below make the generated `CmisDataPathState` and this enum one semantic
// identity. Raw values round-trip exactly: `Unknown(v)` maps to the
// generated `Unknown(v)` (and the schema-named `Reserved0` raw 0 maps to
// `Unknown(0)` and back — value-preserving, never a named-state invention).

impl From<CmisDataPathState> for DpState {
    fn from(state: CmisDataPathState) -> Self {
        match state {
            CmisDataPathState::Reserved0 => Self::Unknown(0),
            CmisDataPathState::DpDeactivated => Self::Deactivated,
            CmisDataPathState::DpInit => Self::Init,
            CmisDataPathState::DpDeinit => Self::Deinit,
            CmisDataPathState::DpActivated => Self::Activated,
            CmisDataPathState::DpTxTurnOn => Self::TxTurnOn,
            CmisDataPathState::DpTxTurnOff => Self::TxTurnOff,
            CmisDataPathState::DpInitialized => Self::Initialized,
            CmisDataPathState::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl From<DpState> for CmisDataPathState {
    fn from(state: DpState) -> Self {
        match state {
            DpState::Deactivated => Self::DpDeactivated,
            DpState::Init => Self::DpInit,
            DpState::Deinit => Self::DpDeinit,
            DpState::Activated => Self::DpActivated,
            DpState::TxTurnOn => Self::DpTxTurnOn,
            DpState::TxTurnOff => Self::DpTxTurnOff,
            DpState::Initialized => Self::DpInitialized,
            DpState::Unknown(0) => Self::Reserved0,
            DpState::Unknown(v) => Self::Unknown(v),
        }
    }
}

/// Why a DPSM operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum DpsmError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The data path ID is outside 1..=8.
    InvalidDpId(u8),
    /// The data path is not in one of the required states.
    WrongState {
        /// The states the operation requires.
        required: Vec<DpState>,
        /// The state actually read.
        actual: DpState,
    },
    /// The poll budget expired before the target state arrived.
    Timeout,
    /// The revision context is not one of the six DPSM catalog revisions
    /// (3.0, 4.0, 5.0, 5.1, 5.2, 5.3). Every revision-qualified DPSM entry
    /// point fails closed with this before any transport I/O rather than
    /// silently borrowing a catalog state machine.
    UnsupportedRevision(Rev),
    /// The requested target state cannot be reached from the current state
    /// through the revision's generated behaviour table: no sequence of
    /// edges connects them. Never a transport outcome; pure plan refusal.
    UnreachableTransition {
        /// The state the transition would leave.
        from: DpState,
        /// The state that cannot be reached.
        to: DpState,
    },
}

impl fmt::Display for DpsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "DPSM transport error: {err}"),
            Self::InvalidDpId(dp_id) => write!(f, "invalid DPSM data path id {dp_id}"),
            Self::WrongState { required, actual } => {
                write!(
                    f,
                    "DPSM wrong state: required {required:?}, actual {actual:?}"
                )
            }
            Self::Timeout => write!(f, "DPSM timed out"),
            Self::UnsupportedRevision(rev) => {
                write!(f, "unsupported DPSM CMIS revision {rev:?}")
            }
            Self::UnreachableTransition { from, to } => {
                write!(f, "no DPSM transition path from {from:?} to {to:?}")
            }
        }
    }
}

impl Error for DpsmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ModuleError> for DpsmError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// Data-path state-machine helper over any CMIS transport.
pub struct Dpsm;

impl Dpsm {
    /// Read the raw 4-bit Data Path State nibble for `dp_id` from Page 11h
    /// (CMIS 5.3 § 8.6.2). This is the un-projected on-wire value; callers that
    /// need a revision-correct [`DpState`] should use
    /// [`current_state`](Self::current_state) (canonical 4.0+ encoding) or
    /// [`state_for_rev`](Self::state_for_rev) (handles pre-4.0 modules).
    pub fn read_raw_nibble<T: CmisTransport>(
        transport: &mut T,
        dp_id: u8,
    ) -> Result<u8, DpsmError> {
        if dp_id >= MAX_DP_ID {
            return Err(DpsmError::InvalidDpId(dp_id));
        }

        // Pages 10h/11h are lane-banked in groups of 8 lanes: bank k covers
        // lanes 8k+1..=8k+8 (CMIS 5.3 § 8.2), so dp 0..7 all live in bank 0.
        let bank = dp_id / 8;
        let mut buf = [0u8; 4];
        transport.read(
            I2C_ADDR_MODULE,
            DPSM_STATE_PAGE,
            bank,
            DPSM_STATE_OFFSET,
            &mut buf,
        )?;

        let nibble_pos = dp_id % 8;
        let byte = buf[(nibble_pos / 2) as usize];
        Ok(if nibble_pos % 2 == 0 {
            byte & 0x0F
        } else {
            (byte >> 4) & 0x0F
        })
    }

    /// Decode the current Data Path State for `dp_id`, assuming the canonical
    /// CMIS 4.0+ 8-state encoding (CMIS 4.0 Table 8-58; CMIS 5.3 § 6.3.3). For a
    /// module that may run the pre-4.0 encoding, use
    /// [`state_for_rev`](Self::state_for_rev).
    pub fn current_state<T: CmisTransport>(
        transport: &mut T,
        dp_id: u8,
    ) -> Result<DpState, DpsmError> {
        Ok(DpState::from_nibble(Self::read_raw_nibble(
            transport, dp_id,
        )?))
    }

    /// Decode the current Data Path State for `dp_id`, applying the
    /// revision-qualified state encoding for `rev`.
    ///
    /// Fail closed like the sibling [`wait_for_terminal`](Self::wait_for_terminal):
    /// `rev` must be one of the catalog CMIS revisions, checked by the
    /// same runtime catalog gate (`validate_catalog_cmis_rev`) — a
    /// non-catalog context (3.1, 4.1, 5.5, a foreign family) is refused
    /// with [`DpsmError::UnsupportedRevision`] BEFORE any transport I/O,
    /// never a debug-only assertion and never a read that answers
    /// `Unknown(raw)` for a revision the schema does not model. On a
    /// catalog revision the read contract is unchanged: exactly one
    /// four-byte Page 11h state read (128..132, bank of `dp_id`), no
    /// writes, and the conversion is the qualified
    /// [`DpState::from_nibble_for_rev`](DpState::from_nibble_for_rev).
    pub fn state_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        dp_id: u8,
    ) -> Result<DpState, DpsmError> {
        validate_catalog_cmis_rev(rev)?;
        let raw = Self::read_raw_nibble(transport, dp_id)?;
        Ok(DpState::from_nibble_for_rev(raw, rev))
    }

    /// Poll `current_state` (the canonical 4.0+ decode) until it equals
    /// `target`, up to `max_polls` reads.
    ///
    /// Revision-blind and Tx-blind: it cannot express "terminal for the
    /// module's revision with Tx held off". For a revision-qualified,
    /// Tx-off-aware wait use
    /// [`wait_for_terminal`](Self::wait_for_terminal).
    pub fn wait_for<T: CmisTransport>(
        transport: &mut T,
        dp_id: u8,
        target: DpState,
        max_polls: u32,
    ) -> Result<(), DpsmError> {
        for _ in 0..max_polls {
            if Self::current_state(transport, dp_id)? == target {
                return Ok(());
            }
        }

        Err(DpsmError::Timeout)
    }

    /// Poll the revision-qualified Data Path State for `dp_id` until it is
    /// terminal for `rev` under the supplied Tx/pending facts, and return
    /// the state reached. See
    /// [`DpState::is_terminal_for_rev`](DpState::is_terminal_for_rev) for
    /// the per-revision terminal set and the meaning of `tx_disabled` and
    /// `dp_init_pending` (the caller owns projecting Page 10h bytes 130/132
    /// and the Page 11h:235 bit — snapshot coherence across those reads is
    /// likewise the caller's contract, mirroring the kernel module's
    /// `plugins/cmis/cmis_dpsm.h`).
    ///
    /// Reads through [`state_for_rev`](Self::state_for_rev) (the
    /// revision-qualified decode, so pre-4.0 modules decode correctly):
    /// exactly one four-byte Page 11h state read per poll, never a write.
    ///
    /// Validation precedes I/O: an unsupported revision or an invalid
    /// `dp_id` returns before any transport read — zero I/O even when
    /// `max_polls == 0` — so a bad context can never half-succeed on the
    /// wire.
    ///
    /// Returns `Err(DpsmError::Timeout)` when `max_polls` polls complete
    /// without a terminal state.
    pub fn wait_for_terminal<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        dp_id: u8,
        tx_disabled: bool,
        dp_init_pending: bool,
        max_polls: u32,
    ) -> Result<DpState, DpsmError> {
        validate_catalog_cmis_rev(rev)?;
        if dp_id >= MAX_DP_ID {
            return Err(DpsmError::InvalidDpId(dp_id));
        }

        for _ in 0..max_polls {
            let state = Self::state_for_rev(transport, rev, dp_id)?;
            if state.is_terminal_for_rev(rev, tx_disabled, dp_init_pending)? {
                return Ok(state);
            }
        }

        Err(DpsmError::Timeout)
    }

    /// Write the SCS0 ApplyDPInit lane mask for `dp_id` (Page 10h bank of
    /// `dp_id`, byte 143, one lane bit) and then poll
    /// [`wait_for_terminal`](Self::wait_for_terminal) until the Data Path
    /// reaches its revision-qualified, Tx-aware terminal state; returns the
    /// state reached.
    ///
    /// The write is byte-for-byte the same as [`activate`](Self::activate);
    /// only the success predicate differs — `activate` waits for
    /// `Initialized` unconditionally, which never completes for a Tx-on
    /// 4.0+ module and never matches the 3.0 encoding, while this waits for
    /// the state the module's revision and Tx configuration actually
    /// settle on.
    ///
    /// Validation precedes I/O: an unsupported revision or an invalid
    /// `dp_id` returns before any transport write.
    pub fn activate_for_rev<T: CmisTransport>(
        transport: &mut T,
        rev: Rev,
        dp_id: u8,
        tx_disabled: bool,
        dp_init_pending: bool,
        max_polls: u32,
    ) -> Result<DpState, DpsmError> {
        validate_catalog_cmis_rev(rev)?;
        if dp_id >= MAX_DP_ID {
            return Err(DpsmError::InvalidDpId(dp_id));
        }

        transport.write(
            I2C_ADDR_MODULE,
            DPSM_CONTROL_PAGE,
            dp_id / 8,
            DPSM_ACTIVATE_OFFSET,
            &[1u8 << (dp_id % 8)],
        )?;
        Self::wait_for_terminal(
            transport,
            rev,
            dp_id,
            tx_disabled,
            dp_init_pending,
            max_polls,
        )
    }

    /// Write the SCS0 ApplyDPInit lane mask for `dp_id` and wait for
    /// `DpState::Initialized` (the CMIS 4.0+ Tx-off parking state).
    ///
    /// Revision-blind and Tx-blind: with Tx enabled the Data Path proceeds
    /// past `DPInitialized` to `DPActivated` (so this wait never completes),
    /// and on a CMIS 3.0 module raw 0x7 is reserved. For a revision-qualified,
    /// Tx-off-aware activation use
    /// [`activate_for_rev`](Self::activate_for_rev).
    pub fn activate<T: CmisTransport>(
        transport: &mut T,
        dp_id: u8,
        max_polls: u32,
    ) -> Result<(), DpsmError> {
        if dp_id >= MAX_DP_ID {
            return Err(DpsmError::InvalidDpId(dp_id));
        }

        transport.write(
            I2C_ADDR_MODULE,
            DPSM_CONTROL_PAGE,
            dp_id / 8,
            DPSM_ACTIVATE_OFFSET,
            &[1u8 << (dp_id % 8)],
        )?;
        Self::wait_for(transport, dp_id, DpState::Initialized, max_polls)
    }

    /// Deactivate one data path and wait for it to return to Deactivated.
    pub fn deactivate<T: CmisTransport>(
        transport: &mut T,
        dp_id: u8,
        max_polls: u32,
    ) -> Result<(), DpsmError> {
        if dp_id >= MAX_DP_ID {
            return Err(DpsmError::InvalidDpId(dp_id));
        }

        let bank = dp_id / 8;
        let mut deinit = [0u8; 1];
        transport.read(
            I2C_ADDR_MODULE,
            DPSM_CONTROL_PAGE,
            bank,
            DPSM_DEACTIVATE_OFFSET,
            &mut deinit,
        )?;
        deinit[0] |= 1u8 << (dp_id % 8);
        transport.write(
            I2C_ADDR_MODULE,
            DPSM_CONTROL_PAGE,
            bank,
            DPSM_DEACTIVATE_OFFSET,
            &deinit,
        )?;
        Self::wait_for(transport, dp_id, DpState::Deactivated, max_polls)
    }
}

impl Dpsm {
    /// The generated behaviour table for `rev`, or
    /// `None` for any non-catalog or non-CMIS revision context. Fail closed:
    /// callers must refuse to plan under an unsupported revision, never
    /// borrow the nearest table.
    pub fn fsm_for_rev(rev: Rev) -> Option<&'static FsmTable<CmisDataPathState>> {
        cmis_data_path_state_machine::for_revision(rev)
    }

    /// Pure, revision-aware plan of the intermediate transitions from
    /// `current` to `target` through the generated behaviour table — the
    /// executable per-edge steps between bulk waits, with guards and side
    /// effects still attached as data for the caller to execute through the
    /// transport it owns.
    ///
    /// Structural (BFS, shortest edge path, schema-order ties): guards are
    /// facts only the caller observes live, so each returned edge is
    /// re-validated with `FsmTable::evaluate` as the caller executes it.
    /// Fails closed: `UnsupportedRevision` for a non-catalog revision,
    /// `UnreachableTransition` when the revision's table has no connecting
    /// edge path. No transport I/O happens here.
    pub fn plan_transition_for_rev(
        rev: Rev,
        current: DpState,
        target: DpState,
    ) -> Result<Vec<FsmTransition<CmisDataPathState>>, DpsmError> {
        let table = Self::fsm_for_rev(rev).ok_or(DpsmError::UnsupportedRevision(rev))?;
        let from = CmisDataPathState::from(current);
        let to = CmisDataPathState::from(target);
        openxcvr_core::fsm::plan_path(table, from, to)
            .map(|path| path.into_iter().copied().collect())
            .map_err(|_| DpsmError::UnreachableTransition {
                from: current,
                to: target,
            })
    }
}

/// In CMIS 3.0 (Table 66), state encodings 1h..=4h correspond to Deactivated, Init,
/// Deinit, and Activated — identical to CMIS 4.0+ (Table 8-58). There is no 3->4
/// remap.
///
/// Note: Reserved values in 3.0 (0h, 5h..=Fh) cannot be projected to a 4.0+ raw nibble
/// without colliding with 4.0+ valid states; callers should use
/// [`DpState::from_nibble_v3`] or [`Dpsm::state_for_rev`] for revision-correct
/// state decoding.
pub fn project_v3_to_canonical(raw: u8) -> u8 {
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    use openxcvr_core::transport::Transport;

    // ── Generated-enum dedup + revision-aware planning ──

    #[test]
    fn dpstate_conversions_are_lossless_on_raw_values() {
        for raw in 0u8..=255 {
            let adapter = DpState::Unknown(raw);
            assert_eq!(
                DpState::from(CmisDataPathState::from(adapter)),
                adapter,
                "raw {raw} must round-trip losslessly"
            );
        }
        // Every named variant round-trips in both directions.
        let pairs = [
            (DpState::Deactivated, CmisDataPathState::DpDeactivated),
            (DpState::Init, CmisDataPathState::DpInit),
            (DpState::Deinit, CmisDataPathState::DpDeinit),
            (DpState::Activated, CmisDataPathState::DpActivated),
            (DpState::TxTurnOn, CmisDataPathState::DpTxTurnOn),
            (DpState::TxTurnOff, CmisDataPathState::DpTxTurnOff),
            (DpState::Initialized, CmisDataPathState::DpInitialized),
        ];
        for (adapter, generated) in pairs {
            assert_eq!(CmisDataPathState::from(adapter), generated);
            assert_eq!(DpState::from(generated), adapter);
        }
        // The schema-named reserved raw 0 is value-preserving both ways.
        assert_eq!(
            DpState::from(CmisDataPathState::Reserved0),
            DpState::Unknown(0)
        );
        assert_eq!(
            CmisDataPathState::from(DpState::Unknown(0)),
            CmisDataPathState::Reserved0
        );
    }

    #[test]
    fn dpsm_plan_transition_follows_generated_edges() {
        // Deactivated -> Activated goes through the generated graph
        // (Deactivated -> Init -> Initialized -> TxTurnOn -> Activated).
        let plan =
            Dpsm::plan_transition_for_rev(Rev::CMIS_5_3, DpState::Deactivated, DpState::Activated)
                .expect("a path must exist at 5.3");
        assert!(!plan.is_empty());
        assert_eq!(plan[0].from, CmisDataPathState::DpDeactivated);
        assert_eq!(plan.last().unwrap().to, CmisDataPathState::DpActivated);
        // Consecutive edges connect.
        for pair in plan.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
        // Already there: no transitions.
        let plan = Dpsm::plan_transition_for_rev(Rev::CMIS_5_3, DpState::Init, DpState::Init)
            .expect("current == target is the empty plan");
        assert!(plan.is_empty());
    }

    #[test]
    fn dpsm_plan_transition_fails_closed_on_unsupported_rev() {
        let rev = Rev {
            family: openxcvr_core::Family::Cmis,
            major: 5,
            minor: 5,
        };
        assert!(matches!(
            Dpsm::plan_transition_for_rev(rev, DpState::Deactivated, DpState::Activated),
            Err(DpsmError::UnsupportedRevision(_))
        ));
        // Foreign families never borrow a CMIS table either.
        let sff = Rev {
            family: openxcvr_core::Family::Sff8636,
            major: 2,
            minor: 12,
        };
        assert!(matches!(
            Dpsm::plan_transition_for_rev(sff, DpState::Deactivated, DpState::Activated),
            Err(DpsmError::UnsupportedRevision(_))
        ));
    }

    #[test]
    fn dpsm_plan_transition_is_revision_aware() {
        // TxTurnOn/Initialized exist since 4.0 in the schema, so a 3.0
        // table cannot reach them and the 3.0 graph is the four-state one.
        let unreachable = Dpsm::plan_transition_for_rev(
            Rev::CMIS_3_0,
            DpState::Deactivated,
            DpState::Initialized,
        );
        assert!(
            matches!(unreachable, Err(DpsmError::UnreachableTransition { .. })),
            "3.0 has no Initialized state to reach: {unreachable:?}"
        );
        // The 3.0 table keeps only the unbounded edges: Deactivated -> Init
        // exists, but no 3.0 edge reaches Activated (every such edge in the
        // schema is since 4.0), so that target is unreachable there too —
        // the plan follows the schema, not the union graph.
        let plan =
            Dpsm::plan_transition_for_rev(Rev::CMIS_3_0, DpState::Deactivated, DpState::Init)
                .expect("the unbounded Deactivated -> Init edge exists at 3.0");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].id, "dp_deactivated_to_dp_init");
        assert!(matches!(
            Dpsm::plan_transition_for_rev(Rev::CMIS_3_0, DpState::Deactivated, DpState::Activated),
            Err(DpsmError::UnreachableTransition { .. })
        ));
    }

    struct MockTransport {
        page_11h_banks: [[u8; 256]; 2],
        page_10h_control: [[u8; 256]; 2],
        page_10h_writes: Vec<(u8, u8, Vec<u8>)>,
        poll_sequence: VecDeque<[u8; 4]>,
        state_reads: usize,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                page_11h_banks: [[0u8; 256]; 2],
                page_10h_control: [[0u8; 256]; 2],
                page_10h_writes: Vec::new(),
                poll_sequence: VecDeque::new(),
                state_reads: 0,
            }
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            match (page, offset, buf.len()) {
                (DPSM_STATE_PAGE, DPSM_STATE_OFFSET, 4) => {
                    self.state_reads += 1;
                    if let Some(next) = self.poll_sequence.pop_front() {
                        buf.copy_from_slice(&next);
                    } else {
                        let start = offset as usize;
                        buf.copy_from_slice(
                            &self.page_11h_banks[bank as usize][start..start + buf.len()],
                        );
                    }
                }
                (DPSM_CONTROL_PAGE, DPSM_DEACTIVATE_OFFSET, 1) => {
                    buf[0] = self.page_10h_control[bank as usize][offset as usize];
                }
                _ => panic!(
                    "unexpected read page={page:#x} offset={offset} len={}",
                    buf.len()
                ),
            }
            Ok(())
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(page, DPSM_CONTROL_PAGE);
            self.page_10h_writes.push((bank, offset, data.to_vec()));
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_current_state_nibbles() {
        let mut transport = MockTransport::new();
        transport.page_11h_banks[0][128] = 0x43;

        assert_eq!(
            Dpsm::current_state(&mut transport, 0).unwrap(),
            DpState::Deinit
        );
        assert_eq!(
            Dpsm::current_state(&mut transport, 1).unwrap(),
            DpState::Activated
        );
    }

    #[test]
    fn test_wait_for_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);

        assert!(Dpsm::wait_for(&mut transport, 0, DpState::Activated, 3).is_ok());
    }

    #[test]
    fn test_activate_writes_requested_lane_mask() {
        let mut transport = MockTransport::new();
        // dp 7 is in-bank lane 8 of bank 0: bit 7 of byte 143; its state
        // nibble is the high nibble of the 4th state byte (Page 11h:131).
        transport.poll_sequence.push_back([0, 0, 0, 0x70]);
        Dpsm::activate(&mut transport, 7, 1).unwrap();
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x80])]);
    }

    #[test]
    fn test_deactivate_preserves_other_lane_bits() {
        let mut transport = MockTransport::new();
        // dp 6 is in-bank lane 7 of bank 0: bit 6 of the DPDeinitLane byte;
        // a lane-1 deinit request already pending must survive the RMW.
        transport.page_10h_control[0][128] = 0x01;
        transport.poll_sequence.push_back([0, 0, 0, 0x01]);
        Dpsm::deactivate(&mut transport, 6, 1).unwrap();
        assert_eq!(transport.page_10h_writes, vec![(0, 128, vec![0x41])]);
    }

    #[test]
    fn test_invalid_dp_id() {
        let mut transport = MockTransport::new();

        assert!(matches!(
            Dpsm::current_state(&mut transport, 8),
            Err(DpsmError::InvalidDpId(8))
        ));
        assert!(matches!(
            Dpsm::current_state(&mut transport, 9),
            Err(DpsmError::InvalidDpId(9))
        ));
        assert!(matches!(
            Dpsm::current_state(&mut transport, 255),
            Err(DpsmError::InvalidDpId(255))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 8),
            Err(DpsmError::InvalidDpId(8))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 9),
            Err(DpsmError::InvalidDpId(9))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 255),
            Err(DpsmError::InvalidDpId(255))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 8),
            Err(DpsmError::InvalidDpId(8))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 9),
            Err(DpsmError::InvalidDpId(9))
        ));
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 255),
            Err(DpsmError::InvalidDpId(255))
        ));
    }

    #[test]
    fn test_timeout() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);

        assert!(matches!(
            Dpsm::wait_for(&mut transport, 0, DpState::Activated, 3),
            Err(DpsmError::Timeout)
        ));
    }

    #[test]
    fn test_read_raw_nibble() {
        let mut transport = MockTransport::new();
        transport.page_11h_banks[0][128] = 0x43;
        // dp0 = low nibble, dp1 = high nibble — raw, no DpState decode.
        assert_eq!(Dpsm::read_raw_nibble(&mut transport, 0).unwrap(), 0x3);
        assert_eq!(Dpsm::read_raw_nibble(&mut transport, 1).unwrap(), 0x4);
    }

    #[test]
    fn test_project_v3_to_canonical() {
        // CMIS 3.0 encodings 1h..=4h are identical to CMIS 4.0+; there is no 3->4 remap.
        assert_eq!(project_v3_to_canonical(0x0), 0x0);
        assert_eq!(project_v3_to_canonical(0x1), 0x1);
        assert_eq!(project_v3_to_canonical(0x2), 0x2);
        assert_eq!(project_v3_to_canonical(0x3), 0x3);
        assert_eq!(project_v3_to_canonical(0x4), 0x4);
        assert_eq!(project_v3_to_canonical(0x5), 0x5);
    }

    #[test]
    fn test_state_for_rev_v3_deinit_and_activated() {
        let mut transport = MockTransport::new();
        // CMIS 3.0 Table 66: raw 0x3 = DPDeinit; raw 0x4 = DPActivated.
        transport.page_11h_banks[0][128] = 0x43;
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 0).unwrap(),
            DpState::Deinit
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 1).unwrap(),
            DpState::Activated
        );
    }

    #[test]
    fn test_exhaustive_nibbles_cmis_3_0() {
        // CMIS 3.0 Table 66: 0 reserved; 1 Deactivated; 2 Init; 3 Deinit; 4 Activated; 5-F reserved.
        for raw in 0x0..=0xFu8 {
            let state = DpState::from_nibble_for_rev(raw, Rev::CMIS_3_0);
            match raw {
                0x1 => assert_eq!(state, DpState::Deactivated),
                0x2 => assert_eq!(state, DpState::Init),
                0x3 => assert_eq!(state, DpState::Deinit),
                0x4 => assert_eq!(state, DpState::Activated),
                reserved => assert_eq!(state, DpState::Unknown(reserved)),
            }
            assert_eq!(state, DpState::from_nibble_v3(raw));
        }
    }

    #[test]
    fn test_exhaustive_nibbles_cmis_4_0() {
        // CMIS 4.0 Table 8-58: 0 reserved; 1 Deactivated; 2 Init; 3 Deinit; 4 Activated;
        // 5 TxTurnOn; 6 TxTurnOff; 7 Initialized; 8-F reserved.
        for raw in 0x0..=0xFu8 {
            let state = DpState::from_nibble_for_rev(raw, Rev::CMIS_4_0);
            match raw {
                0x1 => assert_eq!(state, DpState::Deactivated),
                0x2 => assert_eq!(state, DpState::Init),
                0x3 => assert_eq!(state, DpState::Deinit),
                0x4 => assert_eq!(state, DpState::Activated),
                0x5 => assert_eq!(state, DpState::TxTurnOn),
                0x6 => assert_eq!(state, DpState::TxTurnOff),
                0x7 => assert_eq!(state, DpState::Initialized),
                reserved => assert_eq!(state, DpState::Unknown(reserved)),
            }
            assert_eq!(state, DpState::from_nibble(raw));
        }
    }

    #[test]
    fn test_nonzero_mock_page_11h_state_for_rev() {
        let mut transport = MockTransport::new();
        // Page 11h:128..132 nonzero state register bytes:
        // Byte 128 (0x43): dp0 low nibble = 3 (Deinit), dp1 high nibble = 4 (Activated)
        // Byte 129 (0x15): dp2 low nibble = 5 (3.0: Unknown(5), 4.0: TxTurnOn), dp3 high nibble = 1 (Deactivated)
        // Byte 130 (0x76): dp4 low nibble = 6 (3.0: Unknown(6), 4.0: TxTurnOff), dp5 high nibble = 7 (3.0: Unknown(7), 4.0: Initialized)
        // Byte 131 (0xA2): dp6 low nibble = 2 (Init), dp7 high nibble = 0xA (Unknown(10) in both)
        transport.page_11h_banks[0][128] = 0x43;
        transport.page_11h_banks[0][129] = 0x15;
        transport.page_11h_banks[0][130] = 0x76;
        transport.page_11h_banks[0][131] = 0xA2;

        // In CMIS 3.0:
        // 3 maps to Deinit, 4 maps to Activated, 5 maps to Unknown(5).
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 0).unwrap(),
            DpState::Deinit
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 1).unwrap(),
            DpState::Activated
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 2).unwrap(),
            DpState::Unknown(5)
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 3).unwrap(),
            DpState::Deactivated
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 4).unwrap(),
            DpState::Unknown(6)
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 5).unwrap(),
            DpState::Unknown(7)
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 6).unwrap(),
            DpState::Init
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 7).unwrap(),
            DpState::Unknown(0xA)
        );

        // In CMIS 4.0:
        // 3 maps to Deinit, 4 maps to Activated, 5 maps to TxTurnOn.
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 0).unwrap(),
            DpState::Deinit
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 1).unwrap(),
            DpState::Activated
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 2).unwrap(),
            DpState::TxTurnOn
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 3).unwrap(),
            DpState::Deactivated
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 4).unwrap(),
            DpState::TxTurnOff
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 5).unwrap(),
            DpState::Initialized
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 6).unwrap(),
            DpState::Init
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 7).unwrap(),
            DpState::Unknown(0xA)
        );
    }

    #[test]
    fn test_state_for_rev_v4_no_projection() {
        let mut transport = MockTransport::new();
        // 4.0 raw 0x3 = DPDeinit; 4.0+ takes the raw nibble with no projection.
        transport.page_11h_banks[0][128] = 0x03;
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_4_0, 0).unwrap(),
            DpState::Deinit
        );
    }

    #[test]
    fn test_state_for_rev_direct_map_both_revs() {
        let mut transport = MockTransport::new();
        // raw 0x2 = Init in both encodings (no remap).
        transport.page_11h_banks[0][128] = 0x02;
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 0).unwrap(),
            DpState::Init
        );
        assert_eq!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_5_3, 0).unwrap(),
            DpState::Init
        );
    }

    /// The qualified conversion refuses every non-catalog revision context
    /// without raw aliasing: a foreign family, an unpackable component
    /// (major/minor > 15), and the 3.1/4.1/5.5 neighbors all preserve
    /// `Unknown(raw)` — including out-of-nibble raw `0x10 → Unknown(16)` —
    /// rather than borrowing the pre-4.0 or latest tables.
    #[test]
    fn qualified_state_rejects_invalid_context_without_raw_aliasing() {
        let invalid_contexts = [
            // Foreign families: the family discriminant sorts outside CMIS.
            Rev {
                family: Family::Sff8472,
                major: 3,
                minor: 0,
            },
            Rev {
                family: Family::Sff8636,
                major: 5,
                minor: 3,
            },
            Rev {
                family: Family::CCmis,
                major: 5,
                minor: 3,
            },
            Rev {
                family: Family::Elsfp,
                major: 4,
                minor: 0,
            },
            // CMIS family, but a component that cannot BCD-pack into a
            // MajorRevision byte.
            Rev {
                family: Family::Cmis,
                major: 19,
                minor: 0,
            },
            Rev {
                family: Family::Cmis,
                major: 3,
                minor: 16,
            },
            // CMIS family, packable, but outside the six catalog revisions.
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
        ];
        for rev in invalid_contexts {
            for raw in 0..=0x11u8 {
                assert_eq!(
                    DpState::from_nibble_for_rev(raw, rev),
                    DpState::Unknown(raw),
                    "raw {raw:#04x} must stay Unknown(raw) at non-catalog context {rev:?} \
                     — no aliasing through the pre-4.0/latest tables, no masking"
                );
            }
        }
        // Explicitly: out-of-nibble raw 0x10 stays lossless Unknown(16).
        assert_eq!(
            DpState::from_nibble_for_rev(0x10, Rev::CMIS_3_0),
            DpState::Unknown(16)
        );
    }

    /// 96-case semantic/display agreement across the six catalog revisions:
    /// the qualified semantic conversion (`from_nibble_for_rev`) against
    /// independent literal semantic expectations and the schema-owned
    /// generated display projection (`CmisDataPathState::for_revision`).
    /// Raw 0 is semantic `Unknown(0)` but displays `Reserved0`; named raws
    /// 1..=7 agree only where that revision's table names them (3.0: 1-4;
    /// 4.0+: 1-7); every other raw is `Unknown(raw)` with no display name.
    #[test]
    fn qualified_semantics_agree_with_generated_display_projection() {
        // Independent semantic expectations transcribed from the spec
        // tables (3.0 Table 66; 4.0+ Table 8-58/8-84).
        let semantic_v3_0: [Option<DpState>; 16] = [
            None,
            Some(DpState::Deactivated),
            Some(DpState::Init),
            Some(DpState::Deinit),
            Some(DpState::Activated),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let semantic_v4_0_plus: [Option<DpState>; 16] = [
            None,
            Some(DpState::Deactivated),
            Some(DpState::Init),
            Some(DpState::Deinit),
            Some(DpState::Activated),
            Some(DpState::TxTurnOn),
            Some(DpState::TxTurnOff),
            Some(DpState::Initialized),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        // Canonical display strings for the named semantic variants —
        // literals, not derived from the enum under test.
        let display_of = |state: &DpState| -> &'static str {
            match state {
                DpState::Deactivated => "DpDeactivated",
                DpState::Init => "DpInit",
                DpState::Deinit => "DpDeinit",
                DpState::Activated => "DpActivated",
                DpState::TxTurnOn => "DpTxTurnOn",
                DpState::TxTurnOff => "DpTxTurnOff",
                DpState::Initialized => "DpInitialized",
                DpState::Unknown(_) => unreachable!("named expectation"),
            }
        };
        let catalog = [
            (Rev::CMIS_3_0, 0x30u8),
            (Rev::CMIS_4_0, 0x40),
            (Rev::CMIS_5_0, 0x50),
            (Rev::CMIS_5_1, 0x51),
            (Rev::CMIS_5_2, 0x52),
            (Rev::CMIS_5_3, 0x53),
        ];
        let mut cases = 0usize;
        for (rev, byte) in catalog {
            let project = crate::generated::CmisDataPathState::for_revision(byte)
                .unwrap_or_else(|| panic!("catalog byte {byte:#04x} must be bound"));
            let expected = if rev == Rev::CMIS_3_0 {
                &semantic_v3_0
            } else {
                &semantic_v4_0_plus
            };
            for raw in 0..=15u8 {
                let semantic = DpState::from_nibble_for_rev(raw, rev);
                match expected[raw as usize] {
                    Some(named) => {
                        assert_eq!(semantic, named, "{rev:?} raw {raw:#04x} semantic");
                        assert_eq!(
                            project(raw),
                            Some(display_of(&named)),
                            "{rev:?} raw {raw:#04x} display must agree with the named semantic"
                        );
                    }
                    None => {
                        assert_eq!(
                            semantic,
                            DpState::Unknown(raw),
                            "{rev:?} raw {raw:#04x} must be Unknown(raw)"
                        );
                        if raw == 0 {
                            // Semantic Unknown(0) but display "Reserved0".
                            assert_eq!(project(raw), Some("Reserved0"));
                        } else {
                            assert_eq!(
                                project(raw),
                                None,
                                "{rev:?} raw {raw:#04x} must have no display name"
                            );
                        }
                    }
                }
                cases += 1;
            }
        }
        assert_eq!(cases, 96, "six catalog revisions × sixteen raws");
    }

    /// Fail-closed contract on non-catalog revision contexts (review
    /// blocker 3): `state_for_rev` refuses the 3.1/4.1/5.5 neighbors, a
    /// foreign family and an unpackable component with
    /// `UnsupportedRevision` BEFORE any transport I/O — zero Page 11h
    /// reads, zero writes. The sibling `wait_for_terminal` gate and this
    /// one share `validate_catalog_cmis_rev`, so they cannot disagree.
    #[test]
    fn state_for_rev_refuses_non_catalog_contexts_before_any_io() {
        let refused = [
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
                family: Family::Cmis,
                major: 19,
                minor: 0,
            },
        ];
        for rev in refused {
            let mut transport = MockTransport::new();
            assert!(
                matches!(
                    Dpsm::state_for_rev(&mut transport, rev, 0),
                    Err(DpsmError::UnsupportedRevision(rejected)) if rejected == rev
                ),
                "{rev:?}"
            );
            assert_eq!(
                transport.state_reads, 0,
                "{rev:?}: no read before validation"
            );
            assert!(
                transport.page_10h_writes.is_empty(),
                "{rev:?}: state_for_rev never writes"
            );
        }
        // Invalid dp_id keeps its typed error on a catalog revision.
        let mut transport = MockTransport::new();
        assert!(matches!(
            Dpsm::state_for_rev(&mut transport, Rev::CMIS_3_0, 8),
            Err(DpsmError::InvalidDpId(8))
        ));
    }

    // ── revision-qualified, Tx-off-aware terminal waits (Issue #1 Ask 2) ─────

    /// CMIS 4.0, Tx disabled: `DPInitialized` (raw 0x7) is a steady state
    /// while `DPDeactivateS` stays TRUE (4.0 § 6.3.2.4), so the first poll
    /// that reports 0x7 settles the wait — one 4-byte Page 11h read, no
    /// writes.
    #[test]
    fn wait_for_terminal_4_0_tx_disabled_accepts_initialized_first_poll() {
        let mut transport = MockTransport::new();
        // dp0 = low nibble of byte 128 = 0x7 (Initialized).
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_4_0, 0, true, false, 3).unwrap(),
            DpState::Initialized
        );
        assert_eq!(
            transport.state_reads, 1,
            "Tx-off Initialized settles on the first poll"
        );
        assert!(
            transport.page_10h_writes.is_empty(),
            "wait_for_terminal never writes"
        );
    }

    /// CMIS 4.0, Tx enabled: 0x7 is a transient on the way to
    /// `DPActivated`, so the wait must poll past two Initialized reports
    /// and settle only on 0x4 — the exact symptom the old
    /// `activate`-waits-for-Initialized loop cannot express for Tx-on.
    #[test]
    fn wait_for_terminal_4_0_tx_enabled_polls_past_initialized_to_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_4_0, 0, false, false, 5).unwrap(),
            DpState::Activated
        );
        assert_eq!(
            transport.state_reads, 3,
            "Tx-on must keep polling while the DP reports Initialized"
        );
        assert!(transport.page_10h_writes.is_empty());
    }

    /// CMIS 3.0, Tx disabled: the 3.0 machine (Figure 9 / Table 8) has no
    /// Tx-off parking state — raw 0x7 is reserved (`Unknown(7)`) and never
    /// terminal, so the wait polls through it to 0x4 (`Activated`), the
    /// sole steady terminal state in 3.0.
    #[test]
    fn wait_for_terminal_3_0_tx_disabled_still_requires_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_3_0, 0, true, false, 4).unwrap(),
            DpState::Activated
        );
        assert_eq!(transport.state_reads, 2);
        assert!(transport.page_10h_writes.is_empty());
    }

    /// CMIS 5.3, Tx enabled, `DPInitPendingLane` set (Page 11h:235, since
    /// 5.0): the deferred-provisioning operational acceptance policy
    /// (kmod `cmis_dpsm.h` parity) accepts `DPInitialized` here — the 5.x
    /// equations themselves would cycle the Data Path, not park it.
    #[test]
    fn wait_for_terminal_5_3_dp_init_pending_accepts_initialized() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_5_3, 0, false, true, 3).unwrap(),
            DpState::Initialized
        );
        assert_eq!(transport.state_reads, 1);
        assert!(transport.page_10h_writes.is_empty());
    }

    /// CMIS 5.3, Tx disabled, pending clear: the plain Tx-off leg.
    #[test]
    fn wait_for_terminal_5_3_tx_disabled_parks_initialized() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_5_3, 0, true, false, 3).unwrap(),
            DpState::Initialized
        );
    }

    /// CMIS 5.3, Tx enabled, pending clear: the plain Tx-on progression —
    /// `DPInitialized` is transient there, so the wait must poll past it
    /// and settle only on `DPActivated` (the state the 5.3 activation
    /// sequence actually reaches with Tx on).
    #[test]
    fn wait_for_terminal_5_3_tx_enabled_requires_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);
        assert_eq!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_5_3, 0, false, false, 4).unwrap(),
            DpState::Activated
        );
        assert_eq!(
            transport.state_reads, 2,
            "Tx-on keeps polling while the DP reports Initialized"
        );
        assert!(transport.page_10h_writes.is_empty());
    }

    /// Timeout and I/O contract: exactly one 4-byte Page 11h read per poll,
    /// never a write, `Err(Timeout)` when no poll lands a terminal state.
    #[test]
    fn wait_for_terminal_times_out_one_read_per_poll_no_writes() {
        let mut transport = MockTransport::new();
        // dp0 = 0x2 (Init): transient at every revision, never terminal.
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        transport.poll_sequence.push_back([0x22, 0, 0, 0]);
        assert!(matches!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_5_3, 0, true, true, 2),
            Err(DpsmError::Timeout)
        ));
        assert_eq!(transport.state_reads, 2);
        assert!(transport.page_10h_writes.is_empty());
    }

    /// Validation precedes I/O: an unsupported revision or an invalid dp_id
    /// performs zero transport reads and zero writes — even with
    /// `max_polls > 0` — so a bad context can never half-succeed on the
    /// wire.
    #[test]
    fn wait_for_terminal_validates_before_any_io() {
        let mut transport = MockTransport::new();
        let unsupported = Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        };
        assert!(matches!(
            Dpsm::wait_for_terminal(&mut transport, unsupported, 0, false, false, 5),
            Err(DpsmError::UnsupportedRevision(_))
        ));
        assert!(matches!(
            Dpsm::wait_for_terminal(
                &mut transport,
                Rev {
                    family: Family::Sff8472,
                    major: 3,
                    minor: 0,
                },
                0,
                false,
                false,
                5
            ),
            Err(DpsmError::UnsupportedRevision(_))
        ));
        assert!(matches!(
            Dpsm::wait_for_terminal(&mut transport, Rev::CMIS_3_0, 8, false, false, 0),
            Err(DpsmError::InvalidDpId(8))
        ));
        assert_eq!(transport.state_reads, 0, "no read before validation");
        assert!(
            transport.page_10h_writes.is_empty(),
            "no write before validation"
        );
    }

    /// `activate_for_rev` writes the same SCS0 ApplyDPInit lane mask as the
    /// legacy `activate` (Page 10h bank 0, byte 143, one lane bit), then
    /// waits under the revision-qualified predicate: 4.0 with Tx disabled
    /// settles on Initialized.
    #[test]
    fn activate_for_rev_writes_lane_mask_then_waits_qualified() {
        let mut transport = MockTransport::new();
        // dp 7 is in-bank lane 8 of bank 0: bit 7 of byte 143; its state
        // nibble is the high nibble of the 4th state byte (Page 11h:131).
        transport.poll_sequence.push_back([0, 0, 0, 0x70]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_4_0, 7, true, false, 1).unwrap(),
            DpState::Initialized
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x80])]);
    }

    /// `activate_for_rev` at 3.0 with Tx disabled still requires
    /// `Activated`: the write happens, then the qualified wait polls past
    /// the reserved 0x7 to 0x4.
    #[test]
    fn activate_for_rev_3_0_tx_disabled_waits_for_activated() {
        let mut transport = MockTransport::new();
        // dp0 = low nibble of byte 128: first poll 0x7 (reserved at 3.0,
        // `Unknown(7)`, never terminal), second poll 0x4 (Activated).
        transport.poll_sequence.push_back([0x07, 0, 0, 0]);
        transport.poll_sequence.push_back([0x04, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_3_0, 0, true, false, 3).unwrap(),
            DpState::Activated
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` at 3.0 with Tx enabled settles on `Activated`
    /// too: 3.0 has no Tx parking state, so Tx configuration never changes
    /// the expected terminal. The write is the same lane mask.
    #[test]
    fn activate_for_rev_3_0_tx_enabled_settles_activated() {
        let mut transport = MockTransport::new();
        // First poll 0x7 — reserved at 3.0 (`Unknown(7)`, never terminal) —
        // then 0x4 (Activated).
        transport.poll_sequence.push_back([0x07, 0, 0, 0]);
        transport.poll_sequence.push_back([0x04, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_3_0, 0, false, false, 3).unwrap(),
            DpState::Activated
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` at 4.0 with Tx enabled: the write happens, then
    /// the qualified wait polls past the transient `DPInitialized` report
    /// and settles on `DPActivated` — the state the legacy
    /// activate-waits-for-Initialized loop can never reach with Tx on.
    #[test]
    fn activate_for_rev_4_0_tx_enabled_settles_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_4_0, 0, false, false, 4).unwrap(),
            DpState::Activated
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` at 5.3, Tx disabled: the write happens, then the
    /// qualified wait accepts `DPInitialized` on the first poll (the Tx-off
    /// leg — normative at 5.3 § 6.3.3.6).
    #[test]
    fn activate_for_rev_5_3_tx_disabled_settles_initialized() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_5_3, 0, true, false, 3).unwrap(),
            DpState::Initialized
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` at 5.3 with `DPInitPendingLane` set: the
    /// deferred-provisioning operational acceptance policy (kmod
    /// `cmis_dpsm.h` parity) accepts `DPInitialized` on the first poll.
    #[test]
    fn activate_for_rev_5_3_dp_init_pending_settles_initialized() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_5_3, 0, false, true, 3).unwrap(),
            DpState::Initialized
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` at 5.3 with Tx enabled and pending clear: the
    /// full Tx-on activation — polls past the transient `DPInitialized`
    /// to `DPActivated`.
    #[test]
    fn activate_for_rev_5_3_tx_enabled_settles_activated() {
        let mut transport = MockTransport::new();
        transport.poll_sequence.push_back([0x77, 0, 0, 0]);
        transport.poll_sequence.push_back([0x44, 0, 0, 0]);
        assert_eq!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_5_3, 0, false, false, 4).unwrap(),
            DpState::Activated
        );
        assert_eq!(transport.page_10h_writes, vec![(0, 143, vec![0x01])]);
    }

    /// `activate_for_rev` validation precedes the Page 10h write: an
    /// unsupported revision (5.5 neighbor) leaves the transport untouched.
    #[test]
    fn activate_for_rev_validates_before_any_io() {
        let mut transport = MockTransport::new();
        assert!(matches!(
            Dpsm::activate_for_rev(
                &mut transport,
                Rev {
                    family: Family::Cmis,
                    major: 5,
                    minor: 5,
                },
                0,
                false,
                false,
                3
            ),
            Err(DpsmError::UnsupportedRevision(_))
        ));
        assert!(matches!(
            Dpsm::activate_for_rev(
                &mut transport,
                Rev {
                    family: Family::CCmis,
                    major: 1,
                    minor: 4,
                },
                0,
                false,
                false,
                3
            ),
            Err(DpsmError::UnsupportedRevision(_))
        ));
        assert!(matches!(
            Dpsm::activate_for_rev(&mut transport, Rev::CMIS_4_0, 8, false, false, 0),
            Err(DpsmError::InvalidDpId(8))
        ));
        assert_eq!(transport.state_reads, 0);
        assert!(transport.page_10h_writes.is_empty());
    }

    /// All five non-catalog revision contexts — the CMIS 3.1/4.1/5.5
    /// neighbors, a foreign family (SFF-8472 3.0), and C-CMIS 1.4 — are
    /// rejected by BOTH transport entry points with zero I/O: no Page 11h
    /// read, no Page 10h write. Exercised with `max_polls == 0` as well as
    /// `max_polls > 0`, so the zero-poll wait can never fall through to
    /// `Timeout` (or a zero-poll activate to a bare write) instead of the
    /// fail-closed rejection.
    #[test]
    fn non_catalog_revisions_reject_both_transport_entry_points_with_zero_io() {
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
            for max_polls in [0, 5] {
                let mut transport = MockTransport::new();
                assert!(
                    matches!(
                        Dpsm::wait_for_terminal(&mut transport, rev, 0, false, false, max_polls),
                        Err(DpsmError::UnsupportedRevision(_))
                    ),
                    "wait_for_terminal {rev:?} max_polls={max_polls}"
                );
                assert_eq!(
                    transport.state_reads, 0,
                    "wait_for_terminal {rev:?} max_polls={max_polls}: no read before validation"
                );
                assert!(
                    transport.page_10h_writes.is_empty(),
                    "wait_for_terminal {rev:?} max_polls={max_polls}: no write"
                );

                let mut transport = MockTransport::new();
                assert!(
                    matches!(
                        Dpsm::activate_for_rev(&mut transport, rev, 0, false, false, max_polls),
                        Err(DpsmError::UnsupportedRevision(_))
                    ),
                    "activate_for_rev {rev:?} max_polls={max_polls}"
                );
                assert_eq!(
                    transport.state_reads, 0,
                    "activate_for_rev {rev:?} max_polls={max_polls}: no read before validation"
                );
                assert!(
                    transport.page_10h_writes.is_empty(),
                    "activate_for_rev {rev:?} max_polls={max_polls}: no write before validation"
                );
            }
        }
    }

    /// The new error variant renders the revision it rejected.
    #[test]
    fn unsupported_revision_display_names_the_revision() {
        let rev = Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        };
        assert_eq!(
            DpsmError::UnsupportedRevision(rev).to_string(),
            format!("unsupported DPSM CMIS revision {rev:?}")
        );
    }
}
