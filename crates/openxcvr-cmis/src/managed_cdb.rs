//! Externally driven, singleton CMIS **5.3** CDB 0040h, 0041h or 0100h operation.
//!
//! This module owns no transport, clock, scheduler, audit sink or selector writes.
//! The owner must durably serialize each returned state *before* dispatching its
//! next action. `Dispatched` is a pre-dispatch authorization/latch event: persist
//! its reduction and check `dispatch_authorized` before calling the backend.
//! Never dispatch an offer fenced by that reduction. Cloning/replaying an older state is not a retry mechanism.
//! Qualifications are supplied reviewed backend/profile evidence, not discovery
//! permission. Production admission still requires the consumer's own acceptance.
use crate::adapter::cdb::{
    validate_reply_checksum, validate_reply_length, CdbCommand, CdbOp,
    CmisCdbFirmwareManagementFeatures, CmisCdbFirmwareManagementFeaturesRequest,
    CmisCdbFirmwareManagementFeaturesResponse, CmisCdbGetFirmwareInfo,
    CmisCdbGetFirmwareInfoRequest, CmisCdbGetFirmwareInfoResponse, CmisCdbModuleFeatures,
    CmisCdbModuleFeaturesRequest, CmisCdbModuleFeaturesResponse,
};
use openxcvr_core::{
    selector::{SelectorInvalidation, SelectorKnowledge, SelectorState},
    types::management::*,
    Rev,
};

/// Closed raw-query identity, not execution admission or discovery permission.
/// CMIS 5.3 §9.4.1 Table 9-8 (PDF p.295) is the sole admitted query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedCdbQuery {
    /// CDB 0040h (Get Module Features), the sole admitted raw query.
    ModuleFeatures,
}
impl ManagedCdbQuery {
    /// The query's CDB opcode.
    pub fn opcode(self) -> u16 {
        match self {
            Self::ModuleFeatures => CdbOp::ModuleFeatures.opcode(),
        }
    }
}
/// Raw-query refusal only; existing information-command admissions are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CdbQueryRefusal {
    /// The opcode is in the firmware-management group, not a raw query.
    #[error("opcode is in the firmware-management group, not a raw query")]
    FirmwareManagement,
    /// The opcode is not an admitted query for any revision.
    #[error("opcode is not an admitted CDB query")]
    NotAnAdmittedQuery,
    /// The opcode is not admitted at the supplied revision.
    #[error("opcode is not admitted at this revision")]
    NotAdmittedAtRevision,
}
/// Pure classification BEFORE capability acquisition, selection or mailbox I/O.
/// Revision refusal takes precedence: only CMIS 5.3 is qualified here. At 5.3,
/// 0100h..011Fh is firmware management (Table 9-1, PDF p.287); 0041h remains
/// exclusive to its existing information path. Unknown/control/security/vendor
/// opcodes fail closed. Schema side_effects and lower-bound revision metadata
/// are deliberately not admission authority.
pub fn classify_managed_cdb_query(
    revision: Rev,
    opcode: u16,
) -> Result<ManagedCdbQuery, CdbQueryRefusal> {
    if revision != Rev::CMIS_5_3 {
        return Err(CdbQueryRefusal::NotAdmittedAtRevision);
    }
    match opcode {
        0x0100..=0x011f => Err(CdbQueryRefusal::FirmwareManagement),
        op if op == ManagedCdbQuery::ModuleFeatures.opcode() => Ok(ManagedCdbQuery::ModuleFeatures),
        _ => Err(CdbQueryRefusal::NotAnAdmittedQuery),
    }
}

const TARGET: Selection = Selection {
    address: 0x50,
    bank: 0,
    page: 0x9f,
};
fn range(offset: u8, len: u16, page: u8) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page,
        offset,
        len,
    }
}

/// Stable identity of the operation: which operation, under which module
/// generation and cache epoch, against which module identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbIdentity {
    /// The caller-allocated operation ID (fresh, strictly increasing per
    /// module instance).
    pub operation: OperationId,
    /// The module generation this operation runs under.
    pub generation: ModuleGeneration,
    /// The cache epoch this operation runs under.
    pub epoch: CacheEpoch,
    /// Stable identity within this generation/epoch, not a display name.
    pub module: String,
}
/// One command request as the caller parsed it; every field must match the
/// admitted closed contract exactly.
#[derive(Debug)]
pub struct CdbRequest<'a> {
    /// The revision the caller detected.
    pub revision: Rev,
    /// The CDB opcode requested.
    pub opcode: u16,
    /// The CDB instance address (0x50).
    pub address: u8,
    /// The CDB instance number (1).
    pub instance: u8,
    /// The bank selector (0).
    pub bank: u8,
    /// The request's LPL payload (must be empty for admitted commands).
    pub lpl: &'a [u8],
    /// The declared EPL length (must be 0 for admitted commands).
    pub epl_len: u16,
    /// Whether a credential was supplied (admitted commands take none).
    pub credential_supplied: bool,
}
/// The module's operating state as the caller established it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbOperatingState {
    /// The module is in its ready state.
    ModuleReady,
    /// A reviewed qualification explicitly permits this query in low power.
    QualifiedModuleLowPwr,
    /// Not established, or a state that restricts CDB use: refuse.
    UnknownOrRestricted,
}
/// A supplied review witness. Every physical byte, including collateral and
/// reserved bits, must be covered. A bit-6-only flag witness cannot pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbQualification {
    /// The identity the qualification was reviewed for.
    pub identity: CdbIdentity,
    /// Provenance reference of the review that issued the qualification.
    pub reference: String,
    /// Monotonic time after which the qualification no longer admits.
    pub valid_until: MonoTime,
    /// Qualified read ranges with their per-byte classifications; every
    /// physical byte must be covered.
    pub reads: Vec<(Range, Vec<ByteClassification>)>,
    /// Exact full-byte write permission, never inferred from selector permission.
    pub writes: Vec<Range>,
    /// The backend's per-transfer read limit.
    pub backend_read_limit: u16,
    /// The backend's per-transfer write limit.
    pub backend_write_limit: u16,
    /// The backend can terminate writes with a STOP condition.
    pub stop_terminated_writes: bool,
    /// A qualified NACK on a read preserves custody of the operation.
    pub read_nack_preserves_custody: bool,
    /// The single selector authority is ready to program the CDB page.
    pub selector_authority_ready: bool,
    /// A durable audit sink is attached for the operation's lifetime.
    pub durable_audit_ready: bool,
    /// Covers all CDB instances, selectors, reply buffers and any read of byte 8.
    pub exclusion_enforceable: bool,
}
/// The reviewed backend facts one admission consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbFacts {
    /// The identity the facts were established under.
    pub identity: CdbIdentity,
    /// The module's reported revision byte (0x53 for CMIS 5.3).
    pub revision_byte: u8,
    /// Qualified, fresh, exact Page 01h bytes 163..166 from this instance.
    pub capabilities: RangeObservation,
    /// The reviewed qualification covering the request.
    pub qualification: CdbQualification,
    /// The module's established operating state.
    pub operating_state: CdbOperatingState,
    /// No other CDB operation is outstanding on this instance.
    pub no_outstanding_cdb: bool,
    /// Previously validated 0040h bound and its source reference; never fetched.
    pub advertised_completion: Option<(MonoDuration, String)>,
    /// Previously validated 0041h provenance only; absence does not refuse.
    pub historical_0041: Option<String>,
}
/// The operation's caller-supplied timing bounds. All must be nonzero and
/// internally consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbBounds {
    /// Absolute time the whole operation must finish by.
    pub operation_deadline: MonoTime,
    /// Absolute time the pre-trigger stages must finish by.
    pub preflight_deadline: MonoTime,
    /// Budget from trigger to completion.
    pub completion_budget: MonoDuration,
    /// Interval between busy polls.
    pub poll_interval: MonoDuration,
    /// Maximum number of busy polls.
    pub max_polls: u32,
}
/// Why a managed CDB operation was refused before any state was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CdbRefusal {
    /// The request is not one of the admitted commands.
    #[error("CDB request not admitted")]
    RequestNotAdmitted,
    /// The supplied facts are missing, incomplete or stale.
    #[error("missing or stale CDB facts")]
    MissingOrStaleFacts,
    /// The request needs more CDB instances than the qualification covers.
    #[error("unsupported number of CDB instances")]
    UnsupportedInstances,
    /// The module is not in an operating state that permits the query.
    #[error("module operating state does not permit the command")]
    OperatingState,
    /// The supplied review qualification does not cover the request.
    #[error("qualification does not cover the request")]
    Qualification,
    /// The supplied operation bounds are not internally valid.
    #[error("invalid operation bounds")]
    InvalidBounds,
}
/// Why a supplied event or receipt does not fit the audited CDB operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CdbInputError {
    /// The completion time precedes the issue time.
    #[error("completion time precedes its issue time")]
    BackwardTime,
    /// The event does not correspond to the state machine's expected step.
    #[error("event does not match the expected step")]
    WrongStep,
    /// The event is not valid in the current state.
    #[error("event is not valid in the current state")]
    WrongEvent,
    /// A result arrived for an operation that was never dispatched.
    #[error("result arrived for an operation that was never dispatched")]
    NotDispatched,
    /// The observation is not qualified evidence.
    #[error("observation is not qualified evidence")]
    InvalidObservation,
    /// The result time falls outside the operation's deadline window.
    #[error("result time is outside the operation window")]
    InvalidResultTime,
    /// The operation's time budget overflowed the monotonic clock.
    #[error("operation time budget overflowed")]
    TimeOverflow,
    /// The result does not match the audited plan.
    #[error("result does not match the audited plan")]
    PlanMismatch,
}
/// Whether the module may continue processing the command in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbProcessingMode {
    /// Foreground only: the module holds the mailbox until completion.
    Foreground,
    /// The module supports background completion of the command.
    BackgroundCapable,
}
/// The module's advertised trigger method (CMIS 5.3 Table 9-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbTriggerMethod {
    /// Triggered by a single trigger write or any CmdID write.
    SingleOrCmdid,
    /// Triggered only by a write that includes the CmdID bytes.
    IncludingCmdid,
}
/// Payload-free audit record of an admitted command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbAuditDescription {
    /// Audit record format version.
    pub version: u8,
    /// The admitted command's opcode.
    pub opcode: u16,
    /// The revision the command runs at.
    pub revision: Rev,
    /// The operation identity the audit belongs to.
    pub identity: CdbIdentity,
    /// Admission classification of the command.
    pub classification: &'static str,
    /// Provenance reference of the admitting qualification.
    pub qualification_reference: String,
    /// The operation's timing bounds.
    pub bounds: CdbBounds,
    /// Where the completion bound came from (advertised or caller policy).
    pub completion_source: String,
    /// The effective completion bound (advertised, capped by policy).
    pub completion_bound: MonoDuration,
    /// Foreground access-holdoff bound, when the module is foreground-only.
    pub foreground_holdoff: Option<MonoDuration>,
    /// The module's advertised processing mode.
    pub processing_mode: CdbProcessingMode,
    /// The module's advertised trigger method.
    pub trigger_method: CdbTriggerMethod,
    /// Previously validated 0041h provenance, when one exists.
    pub historical_0041: Option<String>,
    /// Every write the plan intends, as payload-free descriptors.
    pub intended_writes: Vec<CdbWriteDescription>,
}
/// Payload-free exact write purpose. Identity/revision/timing policy come from
/// CdbAuditDescription; issuance/result times come from CdbIssuedStep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbWriteDescription {
    /// The byte range the write addresses.
    pub range: Range,
    /// Why the write exists in the plan.
    pub purpose: CdbWritePurpose,
    /// The requester the write runs for.
    pub requester: ReadRequester,
}
/// Closed, private command policy: callers cannot substitute bytes, bounds or
/// a decoder. All admissions use the same state machine and qualification rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandDescriptor {
    ModuleFeatures,
    FirmwareInfo,
    FirmwareManagementFeatures,
}
/// Only 0040h adopts CMIS 5.3 §9.2.4 (PDF p.288). Existing information
/// commands retain their exact-length contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyLengthPolicy {
    Exact,
    KnownPrefix,
}
impl CommandDescriptor {
    fn opcode(self) -> u16 {
        match self {
            Self::ModuleFeatures => CmisCdbModuleFeatures::CMD_ID,
            Self::FirmwareInfo => CmisCdbGetFirmwareInfo::CMD_ID,
            Self::FirmwareManagementFeatures => CmisCdbFirmwareManagementFeatures::CMD_ID,
        }
    }
    fn reply_len(self) -> usize {
        match self {
            Self::ModuleFeatures => CmisCdbModuleFeatures::MIN_REPLY_LPL_LEN,
            Self::FirmwareInfo => CmisCdbGetFirmwareInfo::MIN_REPLY_LPL_LEN,
            Self::FirmwareManagementFeatures => {
                CmisCdbFirmwareManagementFeatures::MIN_REPLY_LPL_LEN
            }
        }
    }
    fn reply_length_policy(self) -> ReplyLengthPolicy {
        match self {
            Self::ModuleFeatures => ReplyLengthPolicy::KnownPrefix,
            Self::FirmwareInfo | Self::FirmwareManagementFeatures => ReplyLengthPolicy::Exact,
        }
    }
    fn reply_capacity(self) -> usize {
        match self.reply_length_policy() {
            ReplyLengthPolicy::Exact => self.reply_len(),
            ReplyLengthPolicy::KnownPrefix => 120,
        }
    }
    fn accepts_length(self, len: usize) -> bool {
        self.reply_length_policy() == ReplyLengthPolicy::KnownPrefix || len == self.reply_len()
    }
    fn parameters(self) -> [u8; 4] {
        // EPLLength=0, LPLLength=0, one's complement of the two CMDID bytes.
        let [high, low] = self.opcode().to_be_bytes();
        [0, 0, 0, !high.wrapping_add(low)]
    }
    fn validate_length(self, len: usize) -> bool {
        match self {
            Self::ModuleFeatures => validate_reply_length::<CmisCdbModuleFeatures>(len).is_ok(),
            Self::FirmwareInfo => validate_reply_length::<CmisCdbGetFirmwareInfo>(len).is_ok(),
            Self::FirmwareManagementFeatures => {
                validate_reply_length::<CmisCdbFirmwareManagementFeatures>(len).is_ok()
            }
        }
    }
    fn completed(self, body: &[u8], evidence: CdbEvidence) -> CdbOutcome {
        match self {
            Self::ModuleFeatures => CdbOutcome::CompletedModuleFeatures {
                opcode: self.opcode(),
                actual_reply_length: body.len() as u8,
                bytes: body.to_vec(),
                reply: CmisCdbModuleFeatures::decode_reply(1, &body[..self.reply_len()])
                    .expect("validated known-prefix 0040h codec"),
                evidence,
            },
            Self::FirmwareInfo => CdbOutcome::Completed {
                reply: CmisCdbGetFirmwareInfo::decode_reply(1, body)
                    .expect("validated fixed-length 0100h codec"),
                evidence,
            },
            Self::FirmwareManagementFeatures => CdbOutcome::CompletedFirmwareManagementFeatures {
                reply: CmisCdbFirmwareManagementFeatures::decode_reply(1, body)
                    .expect("validated fixed-length 0041h codec"),
                evidence,
            },
        }
    }
}

/// An admitted command plan: closed command identity, audit record and
/// qualification. Nothing in it is caller-substitutable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCdbPlan {
    command: CommandDescriptor,
    audit: CdbAuditDescription,
    qualification: CdbQualification,
    read_limit: u16,
    write_limit: u16,
}
impl ManagedCdbPlan {
    /// Full admitted command identity; retain this before discarding the plan.
    pub fn opcode(&self) -> u16 {
        self.command.opcode()
    }
    /// Payload-free intent. Each issued write also has a payload-free descriptor.
    pub fn audit_description(&self) -> &CdbAuditDescription {
        &self.audit
    }
}
/// Identity of one step inside an operation: the operation plus a
/// strictly increasing sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CdbStepId {
    operation: OperationId,
    sequence: u64,
}
impl CdbStepId {
    /// The owning operation's ID.
    pub fn operation(self) -> OperationId {
        self.operation
    }
    /// The step's sequence number within the operation.
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}
/// Why one read step exists in the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbReadPurpose {
    /// Check the CDB busy flag before staging.
    Preflight,
    /// Consume (arm) the module's notification bits before staging.
    Arm,
    /// Re-check the busy flag immediately before the trigger write.
    RecheckIdle,
    /// Poll the completion flag after the trigger.
    CompletionFlag,
    /// Read the completion status after the flag is set.
    CompletionStatus,
    /// Read the reply header (length and checksum).
    ReplyHeader,
    /// Read the reply body.
    ReplyBody,
}
/// Why one write step exists in the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbWritePurpose {
    /// Write the command parameters bytes (mailbox 130..133).
    Parameters,
    /// Seed the reply header bytes (mailbox 134..135).
    SeedReplyHeader,
    /// Write the CmdID high byte (mailbox 128).
    CmdidHigh,
    /// Write the trigger byte (mailbox 129, CmdID low).
    Trigger,
}
/// One offered read: identity, exact range and purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbReadStep {
    id: CdbStepId,
    range: Range,
    purpose: CdbReadPurpose,
}
impl CdbReadStep {
    /// The step's identity.
    pub fn id(&self) -> CdbStepId {
        self.id
    }
    /// The exact range to read.
    pub fn range(&self) -> Range {
        self.range
    }
    /// Why the read exists.
    pub fn purpose(&self) -> CdbReadPurpose {
        self.purpose
    }
    /// The requester every CDB read runs as.
    pub fn requester(&self) -> ReadRequester {
        ReadRequester::ExplicitClient
    }
    /// CDB reads are never served from cache: always false.
    /// CDB reads are never served from cache: always false.
    pub fn cache_allowed(&self) -> bool {
        false
    }
    /// The read's [`ReadIntent`], with the clear-on-read effect a byte-8
    /// (notification) read knowingly consumes.
    pub fn intent(&self) -> ReadIntent {
        ReadIntent {
            requester: self.requester(),
            exact_ranges: vec![self.range],
            allowed_effects: if self.range.offset == 8 {
                vec![ReadEffect::None, ReadEffect::ClearOnRead]
            } else {
                vec![ReadEffect::None]
            },
        }
    }
}
/// One offered write: identity, exact range, payload and purpose. The
/// payload is dispatch data, never audit data.
#[derive(Clone, PartialEq, Eq)]
pub struct CdbWriteStep {
    id: CdbStepId,
    range: Range,
    bytes: Vec<u8>,
    purpose: CdbWritePurpose,
}
impl std::fmt::Debug for CdbWriteStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdbWriteStep")
            .field("id", &self.id)
            .field("range", &self.range)
            .field("purpose", &self.purpose)
            .finish_non_exhaustive()
    }
}
impl CdbWriteStep {
    /// The step's identity.
    pub fn id(&self) -> CdbStepId {
        self.id
    }
    /// The exact range to write.
    pub fn range(&self) -> Range {
        self.range
    }
    /// For dispatch only; never put this in the audit log.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Why the write exists.
    pub fn purpose(&self) -> CdbWritePurpose {
        self.purpose
    }
    /// Every CDB write must be STOP-terminated.
    pub fn stop_required(&self) -> bool {
        true
    }
    /// The payload-free audit descriptor of this write.
    pub fn audit_description(&self) -> CdbWriteDescription {
        CdbWriteDescription {
            range: self.range,
            purpose: self.purpose,
            requester: ReadRequester::ExplicitClient,
        }
    }
}
/// One effect a reduction returns for the owner to execute, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdbAction {
    /// Ask the single selector authority to program the CDB page and confirm.
    RequireSelection {
        /// The step awaiting the selection.
        id: CdbStepId,
        /// The selection the CDB page requires.
        target: Selection,
    },
    /// Execute this read through the backend.
    Read(CdbReadStep),
    /// Execute this write through the backend (STOP-terminated).
    Write(CdbWriteStep),
    /// Do not drive the next effect before this time.
    WaitUntil(MonoTime),
    /// Consumer must allocate a fresh selector invalidation OperationId newer
    /// than ALL allocated selector plans, and apply the existing core reducer.
    InvalidateSelector(SelectorInvalidation),
    /// Clear-on-read history for owning observers, separate from the audit log.
    RetainConsumedObservation {
        /// The step whose read consumed the observation.
        id: CdbStepId,
        /// The consumed observation to retain.
        observation: RangeObservation,
    },
    /// Treat the mailbox as unresolved; the owner must not reuse it until
    /// re-established.
    Quarantine {
        /// Whether a trigger write may have reached the module.
        trigger_may_have_executed: bool,
        /// The still-outstanding step, if any.
        outstanding: Option<CdbStepId>,
    },
    /// The operation reached its terminal outcome; publish it once.
    Finish(Box<CdbOutcome>),
}
/// How the backend classified one read exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbReadExchange {
    /// The read completed normally.
    Success,
    /// A NACK the qualification says preserves custody (continuable).
    QualifiedContinuityPreservingNack,
    /// The I2C ownership was lost to another master.
    OwnershipLost,
    /// A failure the backend cannot classify further.
    UnknownFailure,
}
/// One externally supplied event the owner reports to the reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdbEvent {
    /// Drive the machine: offer the next effect if any is due.
    Drive,
    /// The selector authority confirmed the CDB-page selection for a step.
    SelectionConfirmed {
        /// The step whose selection was confirmed.
        id: CdbStepId,
    },
    /// The backend accepted a offered step for dispatch (pre-dispatch latch).
    Dispatched {
        /// The step being dispatched.
        id: CdbStepId,
        /// The dispatch time (must equal the reducer's `now`).
        at: MonoTime,
    },
    /// A read completed with the supplied observation.
    ReadResult {
        /// The step that read.
        id: CdbStepId,
        /// The read's observation.
        observation: RangeObservation,
        /// How the exchange was classified.
        exchange: CdbReadExchange,
    },
    /// A write completed with the supplied outcome.
    WriteResult {
        /// The step that wrote.
        id: CdbStepId,
        /// The write's outcome.
        outcome: ActuationOutcome,
        /// Backend completion time, which can precede delivery at `now`.
        completed_at: MonoTime,
    },
    /// The caller cancelled the operation.
    Cancel,
    /// The module's generation/epoch boundary was lost.
    BoundaryLost,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Evidence that the selector authority confirmed the CDB-page selection
/// for one step.
pub struct CdbSelectionEvidence {
    /// The exact RequireSelection request satisfied by the selector authority.
    pub for_step: CdbStepId,
    /// The selector authority's core state after confirming.
    pub core: SelectorState,
}
/// The live owner facts supplied with every event: who holds the module,
/// whether the audit sink is attached, and the current selection evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbLiveFacts {
    /// The identity the live facts were established under.
    pub identity: CdbIdentity,
    /// The caller still holds exclusive custody of the module.
    pub custody: bool,
    /// The durable audit sink is still attached.
    pub audit_ready: bool,
    /// The selector authority's confirmation evidence, when one holds.
    pub selection: Option<CdbSelectionEvidence>,
}
/// Which stage a busy-timeout fired in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbTimeoutStage {
    /// The module stayed busy before the trigger was written.
    Preflight,
    /// The module stayed busy after the trigger was written.
    AfterTrigger,
}
/// Why the operation stopped without a protocol outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbStopReason {
    /// The caller cancelled the operation.
    Cancelled,
    /// The module's generation/epoch boundary was lost.
    Boundary,
    /// The operation's deadline expired.
    DeadlineExpired,
    /// Exclusive custody of the module was lost.
    CustodyLost,
    /// The durable audit sink became unavailable.
    AuditUnavailable,
    /// The backend transport failed.
    Transport,
    /// The module was still busy immediately before the trigger write.
    BusyBeforeTrigger,
    /// The CDB-page selection was lost or never confirmed.
    SelectionLost,
    /// The admission qualification expired mid-operation.
    QualificationExpired,
}
/// Why a reply failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbInvalidReply {
    /// The reply is shorter than the command's minimum.
    Truncated,
    /// The reply length does not match the command's exact-length contract.
    UnexpectedLength,
    /// The reply length falls in a reserved range.
    ReservedLength,
    /// The reply declares an EPL, which this surface does not support.
    EplUnsupported,
    /// The transport supplied a different byte count than the read asked for.
    ByteCount,
    /// The reply checksum does not match.
    Checksum,
    /// The completion status byte is not the expected `01h`.
    UnexpectedStatus,
    /// The module reports busy after signalling completion.
    BusyAfterCompletion,
}
/// One dispatched step's issue record (payload-free audit data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbIssuedStep {
    /// The step's identity.
    pub id: CdbStepId,
    /// The range the step addressed.
    pub range: Range,
    /// The write's purpose, when the step was a write.
    pub write_purpose: Option<CdbWritePurpose>,
    /// When the step was dispatched.
    pub dispatched_at: MonoTime,
    /// When the backend completed it, when known.
    pub completed_at: Option<MonoTime>,
    /// The step's outcome, when known.
    pub outcome: Option<ActuationOutcome>,
    /// Whether the write may have been applied (uncertain path).
    pub possibly_applied: bool,
}
/// One read's retained evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbReadEvidence {
    /// The step that read.
    pub id: CdbStepId,
    /// The range that was read.
    pub range: Range,
    /// The read's provenance stamp.
    pub stamp: ObservationStamp,
    /// How many bytes the transport actually supplied.
    pub actual_length: usize,
    /// How the exchange was classified.
    pub exchange: CdbReadExchange,
}
/// The full evidence trail one operation accumulates. Contains complete
/// collateral flags — project through [`Self::audit_description`] before
/// writing to an audit log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CdbEvidence {
    /// Whether a trigger write may have reached the module.
    pub trigger_may_have_executed: bool,
    /// When the trigger was dispatched, when it was.
    pub trigger_at: Option<MonoTime>,
    /// The completion deadline derived at trigger time.
    pub completion_deadline: Option<MonoTime>,
    /// The foreground-holdoff deadline, when one applies.
    pub holdoff_deadline: Option<MonoTime>,
    /// Every dispatched step's issue record.
    pub issued: Vec<CdbIssuedStep>,
    /// Every read's retained evidence.
    pub reads: Vec<CdbReadEvidence>,
    /// Full collateral flags and provenance, NOT a payload-free audit record.
    pub consumed: Vec<(CdbStepId, RangeObservation)>,
    /// Steps that requested a CDB-page selection.
    pub selection_requests: Vec<CdbStepId>,
    /// The selection confirmations received.
    pub selections: Vec<CdbSelectionEvidence>,
    /// The step whose read observed the completion flag set.
    pub fresh_completion: Option<CdbStepId>,
    /// The raw completion status byte, when read.
    pub raw_status: Option<u8>,
    /// The reply length the header advertised.
    pub actual_reply_length: Option<u8>,
    /// Whether the reply checksum validated, when checked.
    pub checksum_valid: Option<bool>,
    /// The still-outstanding step, when one exists.
    pub outstanding: Option<CdbStepId>,
    /// The CDB mailbox is (still) in an unresolved state.
    pub mailbox_unresolved: bool,
}
/// The one-shot terminal outcome of an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdbOutcome {
    /// Validated 0040h reply: every received byte (36..=120), with typed decode
    /// limited to the 36 bytes defined by CMIS 5.3 Table 9-8. This is protocol
    /// completion, not storage verification or permission for another command.
    CompletedModuleFeatures {
        /// The command's opcode (0040h).
        opcode: u16,
        /// The reply length the header advertised.
        actual_reply_length: u8,
        /// Every received reply byte.
        bytes: Vec<u8>,
        /// The typed decode of the 36 defined bytes.
        reply: CmisCdbModuleFeaturesResponse,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// Completed 0041h, separate to preserve the existing 0100h reply type.
    CompletedFirmwareManagementFeatures {
        /// The decoded 0041h reply.
        reply: CmisCdbFirmwareManagementFeaturesResponse,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// Completed 0100h (Get Firmware Information).
    Completed {
        /// The decoded 0100h reply.
        reply: CmisCdbGetFirmwareInfoResponse,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// The module returned a CDB error status.
    Failed {
        /// The raw completion status byte.
        raw_status: u8,
        /// The 6-bit CDB error code.
        result: u8,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// The module stayed busy until a deadline expired.
    BusyTimeout {
        /// Which stage the timeout fired in.
        stage: CdbTimeoutStage,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// Stopped after a trigger that may have executed: the command may have
    /// applied; the mailbox is unresolved.
    AmbiguousAfterTrigger {
        /// Why the operation stopped.
        reason: CdbStopReason,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
    /// Refused before any I/O was issued.
    RefusedBeforeIo {
        /// Why the request was refused.
        reason: CdbRefusal,
    },
    /// Stopped before the trigger was written; nothing was applied.
    StoppedBeforeTrigger {
        /// Why the operation stopped.
        reason: CdbStopReason,
        /// The staged effects accumulated before the stop.
        staged_effects: CdbEvidence,
    },
    /// A reply failed validation.
    InvalidReply {
        /// Why the reply was rejected.
        reason: CdbInvalidReply,
        /// The evidence trail of the operation.
        evidence: CdbEvidence,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preflight,
    SelectStage,
    Stage(u8, usize),
    Arm,
    Recheck,
    Trigger,
    Flag,
    Status,
    SelectReply,
    Header,
    Body,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingKind {
    Selection,
    Read(CdbReadStep),
    Write(CdbWriteStep),
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    id: CdbStepId,
    kind: PendingKind,
    dispatched: Option<MonoTime>,
}
/// The serializable state of one in-flight operation. The owner durably
/// persists each reduction before dispatching its next action; replaying an
/// older state is not a retry mechanism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCdbState {
    identity: CdbIdentity,
    admission: ManagedCdbPlan,
    phase: Phase,
    sequence: u64,
    last_now: MonoTime,
    pending: Option<Pending>,
    selected: Option<CdbStepId>,
    polls: u32,
    wait_until: Option<MonoTime>,
    evidence: CdbEvidence,
    reply: Vec<u8>,
    reply_checksum: u8,
    header: Vec<u8>,
    nack_wait: bool,
    terminal: Option<CdbOutcome>,
}
impl ManagedCdbState {
    /// The evidence accumulated so far.
    pub fn evidence(&self) -> &CdbEvidence {
        &self.evidence
    }
    /// The terminal outcome, once one was latched.
    pub fn terminal(&self) -> Option<&CdbOutcome> {
        self.terminal.as_ref()
    }
    /// True only after the pre-dispatch latch was accepted, before its result.
    /// A terminal/boundary/cancel reduction always fences a prior offer.
    pub fn dispatch_authorized(&self, id: CdbStepId) -> bool {
        self.terminal.is_none()
            && self
                .pending
                .as_ref()
                .is_some_and(|p| p.id == id && p.dispatched.is_some())
    }
    /// The still-outstanding step, when one exists.
    pub fn outstanding(&self) -> Option<CdbStepId> {
        self.pending.as_ref().map(|p| p.id)
    }
}
/// One reduction's result: the state to persist and the ordered effects to
/// execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbReduction {
    /// The state after the reduction; persist it before the next dispatch.
    pub state: ManagedCdbState,
    /// The effects to execute, in order.
    pub effects: Vec<CdbAction>,
}

fn qualified_read(q: &CdbQualification, wanted: Range, consuming: bool) -> bool {
    q.reads.iter().any(|(r, classes)| {
        r.validate().is_ok()
            && r.contains(wanted)
            && classes.len() == usize::from(r.len)
            && classes[usize::from(wanted.offset - r.offset)
                ..usize::from(wanted.offset - r.offset) + usize::from(wanted.len)]
                .iter()
                .all(|c| {
                    c.readable_mask == 255
                        && c.unknown_mask == 0
                        && c.uncovered_mask == 0
                        && c.write_only_mask == 0
                        && !c.known_effects.is_empty()
                        && (!consuming || c.known_effects.contains(&ReadEffect::ClearOnRead))
                        && c.known_effects.iter().all(|e| {
                            *e == ReadEffect::None || (consuming && *e == ReadEffect::ClearOnRead)
                        })
                })
    })
}
/// Validate the entire logical request and all supplied prerequisites before
/// constructing any plan. This is not CdbOp's lower-bound revision test.
pub fn admit_get_firmware_info(
    request: CdbRequest<'_>,
    facts: &CdbFacts,
    bounds: CdbBounds,
    now: MonoTime,
) -> Result<(ManagedCdbPlan, ManagedCdbState), CdbRefusal> {
    if request.revision != Rev::CMIS_5_3
        || request.opcode != 0x0100
        || request.address != 0x50
        || request.instance != 1
        || request.bank != 0
        || !request.lpl.is_empty()
        || request.epl_len != 0
        || request.credential_supplied
    {
        return Err(CdbRefusal::RequestNotAdmitted);
    }
    admit_command(request, facts, bounds, now, CommandDescriptor::FirmwareInfo)
}

/// Strict CMIS 5.3 0041h admission; does not widen 0100h admission or trigger
/// prerequisite discovery. All qualification/identity/bounds facts are supplied.
pub fn admit_firmware_management_features(
    request: CdbRequest<'_>,
    facts: &CdbFacts,
    bounds: CdbBounds,
    now: MonoTime,
) -> Result<(ManagedCdbPlan, ManagedCdbState), CdbRefusal> {
    if request.revision != Rev::CMIS_5_3
        || request.opcode != 0x0041
        || request.address != 0x50
        || request.instance != 1
        || request.bank != 0
        || !request.lpl.is_empty()
        || request.epl_len != 0
        || request.credential_supplied
    {
        return Err(CdbRefusal::RequestNotAdmitted);
    }
    admit_command(
        request,
        facts,
        bounds,
        now,
        CommandDescriptor::FirmwareManagementFeatures,
    )
}

/// Strict CMIS 5.3 0040h admission through the shared managed reducer.
/// Classification is not sufficient: every backend/owner fact is still required.
/// Full possible reply capacity (134..255) must be qualified up front, but only
/// the header and actual received length are read. No prerequisite is acquired.
pub fn admit_module_features(
    request: CdbRequest<'_>,
    facts: &CdbFacts,
    bounds: CdbBounds,
    now: MonoTime,
) -> Result<(ManagedCdbPlan, ManagedCdbState), CdbRefusal> {
    if classify_managed_cdb_query(request.revision, request.opcode)
        != Ok(ManagedCdbQuery::ModuleFeatures)
        || request.address != 0x50
        || request.instance != 1
        || request.bank != 0
        || !request.lpl.is_empty()
        || request.epl_len != 0
        || request.credential_supplied
    {
        return Err(CdbRefusal::RequestNotAdmitted);
    }
    admit_command(
        request,
        facts,
        bounds,
        now,
        CommandDescriptor::ModuleFeatures,
    )
}

fn admit_command(
    request: CdbRequest<'_>,
    facts: &CdbFacts,
    bounds: CdbBounds,
    now: MonoTime,
    command: CommandDescriptor,
) -> Result<(ManagedCdbPlan, ManagedCdbState), CdbRefusal> {
    let q = &facts.qualification;
    let caps = &facts.capabilities;
    if facts.revision_byte != 0x53
        || facts.identity.module.is_empty()
        || q.identity != facts.identity
        || q.reference.is_empty()
        || q.valid_until <= now
        || !facts.no_outstanding_cdb
        || caps.range != range(163, 4, 1)
        || caps
            .stamp
            .validate(facts.identity.generation, facts.identity.epoch)
            .is_err()
        || caps.stamp.freshness != Freshness::Fresh
        || caps.stamp.completed_at > now
    {
        return Err(CdbRefusal::MissingOrStaleFacts);
    }
    let cap = caps.bytes().ok_or(CdbRefusal::MissingOrStaleFacts)?;
    if !matches!(cap[0] >> 6, 1 | 2) {
        return Err(CdbRefusal::UnsupportedInstances);
    }
    if facts.operating_state == CdbOperatingState::UnknownOrRestricted {
        return Err(CdbRefusal::OperatingState);
    }
    if q.backend_read_limit == 0
        || q.backend_write_limit == 0
        || !q.stop_terminated_writes
        || !q.selector_authority_ready
        || !q.durable_audit_ready
        || !q.exclusion_enforceable
        || !qualified_read(q, range(8, 1, 0), true)
        || !qualified_read(q, range(37, 1, 0), false)
        || !qualified_read(
            q,
            range(134, 2 + command.reply_capacity() as u16, 0x9f),
            false,
        )
        || !(128..=135).all(|offset| {
            q.writes
                .iter()
                .any(|r| r.validate().is_ok() && r.contains(range(offset, 1, 0x9f)))
        })
    {
        return Err(CdbRefusal::Qualification);
    }
    if bounds.operation_deadline <= now
        || bounds.preflight_deadline <= now
        || bounds.preflight_deadline > bounds.operation_deadline
        || bounds.completion_budget.0 == 0
        || bounds.poll_interval.0 == 0
        || bounds.max_polls == 0
        || now.checked_add(bounds.completion_budget).is_err()
        || now.checked_add(bounds.poll_interval).is_err()
    {
        return Err(CdbRefusal::InvalidBounds);
    }
    let (completion_bound, completion_source) = match &facts.advertised_completion {
        Some((d, source)) if d.0 > 0 && !source.is_empty() => (
            MonoDuration(d.0.min(bounds.completion_budget.0)),
            format!("advertised-0040:{source};capped-by-policy"),
        ),
        Some(_) => return Err(CdbRefusal::InvalidBounds),
        None => (
            bounds.completion_budget,
            "caller-policy-not-advertised".into(),
        ),
    };
    let background = cap[0] & 0x20 != 0;
    let holdoff = if background {
        None
    } else {
        let ms = if cap[3] & 0x80 != 0 {
            u64::from((cap[2] & 31).max(1)) * 160
        } else {
            80 - u64::from((cap[3] & 127).min(80))
        };
        Some(MonoDuration(ms * 1_000_000))
    };
    // All closed descriptors use their existing empty request codecs.
    debug_assert!(
        match command {
            CommandDescriptor::ModuleFeatures =>
                CmisCdbModuleFeatures::encode_lpl(&CmisCdbModuleFeaturesRequest),
            CommandDescriptor::FirmwareInfo =>
                CmisCdbGetFirmwareInfo::encode_lpl(&CmisCdbGetFirmwareInfoRequest),
            CommandDescriptor::FirmwareManagementFeatures =>
                CmisCdbFirmwareManagementFeatures::encode_lpl(
                    &CmisCdbFirmwareManagementFeaturesRequest
                ),
        }
        .is_empty()
    );
    let plan = ManagedCdbPlan {
        command,
        audit: CdbAuditDescription {
            version: 1,
            opcode: command.opcode(),
            revision: request.revision,
            identity: facts.identity.clone(),
            classification: "singleton-informational",
            qualification_reference: q.reference.clone(),
            bounds,
            completion_source,
            completion_bound,
            foreground_holdoff: holdoff,
            processing_mode: if background {
                CdbProcessingMode::BackgroundCapable
            } else {
                CdbProcessingMode::Foreground
            },
            trigger_method: if cap[2] & 0x80 == 0 {
                CdbTriggerMethod::SingleOrCmdid
            } else {
                CdbTriggerMethod::IncludingCmdid
            },
            historical_0041: facts.historical_0041.clone(),
            intended_writes: intended_writes(q.backend_write_limit.min(8)),
        },
        qualification: q.clone(),
        read_limit: q.backend_read_limit.min(8),
        write_limit: q.backend_write_limit.min(8),
    };
    let state = ManagedCdbState {
        identity: facts.identity.clone(),
        admission: plan.clone(),
        phase: Phase::Preflight,
        sequence: 0,
        last_now: now,
        pending: None,
        selected: None,
        polls: 0,
        wait_until: None,
        evidence: CdbEvidence::default(),
        reply: Vec::with_capacity(command.reply_capacity()),
        reply_checksum: 0,
        header: Vec::with_capacity(2),
        nack_wait: false,
        terminal: None,
    };
    Ok((plan, state))
}

fn intended_writes(limit: u16) -> Vec<CdbWriteDescription> {
    let mut descriptions = Vec::new();
    for (offset, len, purpose) in [
        (130, 4, CdbWritePurpose::Parameters),
        (134, 2, CdbWritePurpose::SeedReplyHeader),
        (128, 1, CdbWritePurpose::CmdidHigh),
        (129, 1, CdbWritePurpose::Trigger),
    ] {
        let mut done = 0;
        while done < len {
            let n = (len - done).min(limit);
            descriptions.push(CdbWriteDescription {
                range: range(offset + done as u8, n, 0x9f),
                purpose,
                requester: ReadRequester::ExplicitClient,
            });
            done += n;
        }
    }
    descriptions
}
fn selection_valid(s: &ManagedCdbState, live: &CdbLiveFacts, id: CdbStepId) -> bool {
    live.selection.as_ref().is_some_and(|v| {
        v.for_step == id
            && v.core.pending().is_none()
            && v.core.knowledge(s.identity.generation, s.identity.epoch)
                == SelectorKnowledge::Confirmed(TARGET)
    })
}
fn finish(s: &mut ManagedCdbState, effects: &mut Vec<CdbAction>, outcome: CdbOutcome) {
    if s.terminal.is_some() {
        return;
    }
    if s.evidence.issued.iter().any(|i| i.write_purpose.is_some()) {
        effects.push(CdbAction::InvalidateSelector(
            SelectorInvalidation::CdbCommand,
        ));
    }
    if s.evidence.mailbox_unresolved || s.pending.as_ref().is_some_and(|p| p.dispatched.is_some()) {
        effects.push(CdbAction::Quarantine {
            trigger_may_have_executed: s.evidence.trigger_may_have_executed,
            outstanding: s.pending.as_ref().map(|p| p.id),
        });
    }
    s.selected = None;
    s.terminal = Some(outcome.clone());
    effects.push(CdbAction::Finish(Box::new(outcome)));
}
fn stop(s: &mut ManagedCdbState, effects: &mut Vec<CdbAction>, reason: CdbStopReason) {
    if s.terminal.is_some() {
        return;
    }
    // Unsubmitted offers are fenced, but every actually outstanding result stays
    // owned/accounted for. No cleanup writes or automatic settlement commands.
    if s.pending.as_ref().is_some_and(|p| p.dispatched.is_none()) {
        s.pending = None;
    }
    s.evidence.outstanding = s.pending.as_ref().map(|p| p.id);
    let outcome = if s.evidence.trigger_may_have_executed {
        s.evidence.mailbox_unresolved = true;
        CdbOutcome::AmbiguousAfterTrigger {
            reason,
            evidence: s.evidence.clone(),
        }
    } else {
        CdbOutcome::StoppedBeforeTrigger {
            reason,
            staged_effects: s.evidence.clone(),
        }
    };
    finish(s, effects, outcome);
}
fn timeout(s: &mut ManagedCdbState, effects: &mut Vec<CdbAction>) {
    if s.terminal.is_some() {
        return;
    }
    // Reply acquisition is no longer a busy/holdoff wait. Nor can an
    // unanswered first preflight read establish that the module was busy.
    if matches!(s.phase, Phase::SelectReply | Phase::Header | Phase::Body)
        || (!s.evidence.trigger_may_have_executed && (s.phase != Phase::Preflight || s.polls == 0))
    {
        stop(s, effects, CdbStopReason::DeadlineExpired);
        return;
    }
    if s.pending.as_ref().is_some_and(|p| p.dispatched.is_none()) {
        s.pending = None;
    }
    s.evidence.outstanding = s.pending.as_ref().map(|p| p.id);
    let stage = if s.evidence.trigger_may_have_executed {
        s.evidence.mailbox_unresolved = true;
        CdbTimeoutStage::AfterTrigger
    } else {
        CdbTimeoutStage::Preflight
    };
    finish(
        s,
        effects,
        CdbOutcome::BusyTimeout {
            stage,
            evidence: s.evidence.clone(),
        },
    );
}
fn invalid(s: &mut ManagedCdbState, effects: &mut Vec<CdbAction>, reason: CdbInvalidReply) {
    finish(
        s,
        effects,
        CdbOutcome::InvalidReply {
            reason,
            evidence: s.evidence.clone(),
        },
    );
}
fn deadline(plan: &ManagedCdbPlan, s: &ManagedCdbState) -> MonoTime {
    let execution = s
        .evidence
        .completion_deadline
        .unwrap_or(if s.phase == Phase::Preflight {
            plan.audit.bounds.preflight_deadline
        } else {
            plan.audit.bounds.operation_deadline
        });
    if s.nack_wait {
        s.evidence
            .holdoff_deadline
            .map_or(execution, |t| t.min(execution))
    } else {
        execution
    }
}
fn wait(
    plan: &ManagedCdbPlan,
    s: &mut ManagedCdbState,
    effects: &mut Vec<CdbAction>,
    now: MonoTime,
) -> Result<(), CdbInputError> {
    if s.polls >= plan.audit.bounds.max_polls || now >= deadline(plan, s) {
        timeout(s, effects);
        return Ok(());
    }
    let at = now
        .checked_add(plan.audit.bounds.poll_interval)
        .map_err(|_| CdbInputError::TimeOverflow)?
        .min(deadline(plan, s));
    s.wait_until = Some(at);
    effects.push(CdbAction::WaitUntil(at));
    Ok(())
}
fn next_id(s: &mut ManagedCdbState) -> CdbStepId {
    // Two finite u32 poll budgets plus bounded staging/header/120-byte chunks.
    s.sequence += 1;
    CdbStepId {
        operation: s.identity.operation,
        sequence: s.sequence,
    }
}
fn offer_read(
    s: &mut ManagedCdbState,
    effects: &mut Vec<CdbAction>,
    r: Range,
    purpose: CdbReadPurpose,
) {
    let step = CdbReadStep {
        id: next_id(s),
        range: r,
        purpose,
    };
    s.pending = Some(Pending {
        id: step.id,
        kind: PendingKind::Read(step.clone()),
        dispatched: None,
    });
    effects.push(CdbAction::Read(step));
}
fn offer_write(
    s: &mut ManagedCdbState,
    effects: &mut Vec<CdbAction>,
    r: Range,
    bytes: Vec<u8>,
    purpose: CdbWritePurpose,
) {
    let step = CdbWriteStep {
        id: next_id(s),
        range: r,
        bytes,
        purpose,
    };
    s.pending = Some(Pending {
        id: step.id,
        kind: PendingKind::Write(step.clone()),
        dispatched: None,
    });
    effects.push(CdbAction::Write(step));
}
fn offer(
    plan: &ManagedCdbPlan,
    s: &mut ManagedCdbState,
    live: &CdbLiveFacts,
    effects: &mut Vec<CdbAction>,
    now: MonoTime,
) -> Result<(), CdbInputError> {
    if s.terminal.is_some() {
        return Ok(());
    }
    if now >= deadline(plan, s) {
        timeout(s, effects);
        return Ok(());
    }
    if s.pending.is_some() {
        return Ok(());
    }
    if let Some(at) = s.wait_until {
        if now < at {
            effects.push(CdbAction::WaitUntil(at));
            return Ok(());
        }
        s.wait_until = None;
    }
    let upper = matches!(
        s.phase,
        Phase::Stage(..) | Phase::Trigger | Phase::Header | Phase::Body
    );
    if upper && !s.selected.is_some_and(|id| selection_valid(s, live, id)) {
        stop(s, effects, CdbStopReason::SelectionLost);
        return Ok(());
    }
    match s.phase {
        Phase::Preflight => {
            if s.polls >= plan.audit.bounds.max_polls {
                timeout(s, effects);
            } else {
                offer_read(s, effects, range(37, 1, 0), CdbReadPurpose::Preflight);
            }
        }
        Phase::SelectStage | Phase::SelectReply => {
            let id = next_id(s);
            s.pending = Some(Pending {
                id,
                kind: PendingKind::Selection,
                dispatched: None,
            });
            s.evidence.selection_requests.push(id);
            effects.push(CdbAction::RequireSelection { id, target: TARGET });
        }
        Phase::Stage(part, offset) => {
            let parameters = plan.command.parameters();
            let [high, _] = plan.opcode().to_be_bytes();
            let cmdid_high = [high];
            let (base, data, purpose): (u8, &[u8], _) = match part {
                0 => (130, &parameters, CdbWritePurpose::Parameters),
                1 => (134, &[0xff, 0], CdbWritePurpose::SeedReplyHeader),
                _ => (128, &cmdid_high, CdbWritePurpose::CmdidHigh),
            };
            let len = (data.len() - offset).min(usize::from(plan.write_limit));
            offer_write(
                s,
                effects,
                range(base + offset as u8, len as u16, 0x9f),
                data[offset..offset + len].to_vec(),
                purpose,
            );
        }
        Phase::Arm => offer_read(s, effects, range(8, 1, 0), CdbReadPurpose::Arm),
        Phase::Recheck => offer_read(s, effects, range(37, 1, 0), CdbReadPurpose::RecheckIdle),
        Phase::Trigger => offer_write(
            s,
            effects,
            range(129, 1, 0x9f),
            vec![plan.opcode().to_be_bytes()[1]],
            CdbWritePurpose::Trigger,
        ),
        Phase::Flag => {
            if s.polls >= plan.audit.bounds.max_polls {
                timeout(s, effects);
            } else {
                offer_read(s, effects, range(8, 1, 0), CdbReadPurpose::CompletionFlag);
            }
        }
        Phase::Status => {
            if s.polls >= plan.audit.bounds.max_polls {
                timeout(s, effects);
            } else {
                offer_read(
                    s,
                    effects,
                    range(37, 1, 0),
                    CdbReadPurpose::CompletionStatus,
                );
            }
        }
        Phase::Header => offer_read(
            s,
            effects,
            range(
                134 + s.header.len() as u8,
                (2 - s.header.len()).min(usize::from(plan.read_limit)) as u16,
                0x9f,
            ),
            CdbReadPurpose::ReplyHeader,
        ),
        Phase::Body => {
            let received = usize::from(s.header[0]); // validated descriptor policy
            let len = (received - s.reply.len()).min(usize::from(plan.read_limit));
            offer_read(
                s,
                effects,
                range(136 + s.reply.len() as u8, len as u16, 0x9f),
                CdbReadPurpose::ReplyBody,
            );
        }
    }
    Ok(())
}

/// Reduce exactly one supplied event. No transfer result can authorize another
/// transfer until its ID, dispatch, stamp, timing, custody and byte count pass.
/// Terminal client outcomes are one-shot; late outstanding results update only
/// retained settlement evidence, never the earlier client terminal.
pub fn cdb_step(
    plan: &ManagedCdbPlan,
    previous: &ManagedCdbState,
    event: CdbEvent,
    live: &CdbLiveFacts,
    now: MonoTime,
) -> Result<CdbReduction, CdbInputError> {
    if previous.admission != *plan {
        return Err(CdbInputError::PlanMismatch);
    }
    if now < previous.last_now {
        return Err(CdbInputError::BackwardTime);
    }
    let mut s = previous.clone();
    s.last_now = now;
    let mut effects = Vec::new();
    let lost =
        live.identity != s.identity || !live.custody || matches!(event, CdbEvent::BoundaryLost);
    // Account for a result even on the event that reports a boundary/cancel.
    // No further effect can be offered once the terminal latch is set.
    if lost {
        let reason = if live.identity != s.identity || matches!(event, CdbEvent::BoundaryLost) {
            CdbStopReason::Boundary
        } else {
            CdbStopReason::CustodyLost
        };
        stop(&mut s, &mut effects, reason);
    }
    if matches!(event, CdbEvent::Cancel) {
        stop(&mut s, &mut effects, CdbStopReason::Cancelled);
    }
    if !live.audit_ready {
        stop(&mut s, &mut effects, CdbStopReason::AuditUnavailable);
    }
    if now >= plan.qualification.valid_until {
        stop(&mut s, &mut effects, CdbStopReason::QualificationExpired);
    }
    match event {
        CdbEvent::Cancel | CdbEvent::BoundaryLost => {}
        CdbEvent::Drive => offer(plan, &mut s, live, &mut effects, now)?,
        CdbEvent::SelectionConfirmed { id } => {
            if s.terminal.is_some() {
                return Ok(CdbReduction { state: s, effects });
            }
            let p = s.pending.as_ref().ok_or(CdbInputError::WrongStep)?;
            if p.id != id {
                return Err(CdbInputError::WrongStep);
            }
            if p.kind != PendingKind::Selection {
                return Err(CdbInputError::WrongEvent);
            }
            let reused = s.phase == Phase::SelectReply
                && live
                    .selection
                    .as_ref()
                    .is_some_and(|v| s.evidence.selections.iter().any(|old| old.core == v.core));
            if !selection_valid(&s, live, id) || reused {
                stop(&mut s, &mut effects, CdbStopReason::SelectionLost);
            } else {
                s.selected = Some(id);
                s.evidence.selections.push(live.selection.clone().unwrap());
                s.pending = None;
                s.phase = if s.phase == Phase::SelectStage {
                    Phase::Stage(0, 0)
                } else {
                    Phase::Header
                };
            }
        }
        CdbEvent::Dispatched { id, at } => {
            if s.terminal.is_some() {
                return Ok(CdbReduction { state: s, effects });
            }
            let p = s.pending.as_ref().ok_or(CdbInputError::WrongStep)?;
            if p.id != id {
                return Err(CdbInputError::WrongStep);
            }
            if p.dispatched.is_some() || at != now || matches!(p.kind, PendingKind::Selection) {
                return Err(CdbInputError::WrongEvent);
            }
            if now >= deadline(plan, &s) {
                stop(&mut s, &mut effects, CdbStopReason::DeadlineExpired);
            } else {
                let (r, purpose) = match &p.kind {
                    PendingKind::Read(v) => (v.range, None),
                    PendingKind::Write(v) => (v.range, Some(v.purpose)),
                    _ => unreachable!(),
                };
                if r.offset >= 128 && !s.selected.is_some_and(|id| selection_valid(&s, live, id)) {
                    stop(&mut s, &mut effects, CdbStopReason::SelectionLost);
                } else {
                    // Derive checked bounds before accepting a trigger dispatch.
                    let execution = if purpose == Some(CdbWritePurpose::Trigger) {
                        Some(
                            now.checked_add(plan.audit.completion_bound)
                                .map_err(|_| CdbInputError::TimeOverflow)?
                                .min(plan.audit.bounds.operation_deadline),
                        )
                    } else {
                        None
                    };
                    let holdoff = if execution.is_some() {
                        plan.audit
                            .foreground_holdoff
                            .map(|d| now.checked_add(d))
                            .transpose()
                            .map_err(|_| CdbInputError::TimeOverflow)?
                    } else {
                        None
                    };
                    s.pending.as_mut().unwrap().dispatched = Some(at);
                    s.evidence.outstanding = Some(id);
                    s.evidence.issued.push(CdbIssuedStep {
                        id,
                        range: r,
                        write_purpose: purpose,
                        dispatched_at: at,
                        completed_at: None,
                        outcome: None,
                        possibly_applied: purpose.is_some(),
                    });
                    if purpose == Some(CdbWritePurpose::Trigger) {
                        s.evidence.trigger_may_have_executed = true;
                        s.evidence.trigger_at = Some(at);
                        s.evidence.mailbox_unresolved = true;
                        s.evidence.completion_deadline = execution;
                        s.evidence.holdoff_deadline = holdoff.map(|t| t.min(execution.unwrap()));
                        s.selected = None;
                        s.polls = 0;
                        effects.push(CdbAction::InvalidateSelector(
                            SelectorInvalidation::CdbCommand,
                        ));
                    }
                }
            }
        }
        CdbEvent::WriteResult {
            id,
            outcome,
            completed_at,
        } => {
            let p = s.pending.clone().ok_or(CdbInputError::WrongStep)?;
            if p.id != id {
                return Err(CdbInputError::WrongStep);
            }
            let dispatched = p.dispatched.ok_or(CdbInputError::NotDispatched)?;
            if completed_at < dispatched || completed_at > now {
                return Err(CdbInputError::InvalidResultTime);
            }
            let PendingKind::Write(w) = p.kind else {
                return Err(CdbInputError::WrongEvent);
            };
            let issued = s
                .evidence
                .issued
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or(CdbInputError::WrongStep)?;
            issued.completed_at = Some(completed_at);
            issued.outcome = Some(outcome.clone());
            s.pending = None;
            s.evidence.outstanding = None;
            if s.terminal.is_none() {
                // Unknown write failure is never evidence of module busy,
                // even when its answer is delivered after a policy deadline.
                if outcome != ActuationOutcome::Confirmed {
                    stop(&mut s, &mut effects, CdbStopReason::Transport);
                } else if now >= deadline(plan, &s) {
                    timeout(&mut s, &mut effects);
                } else if w.purpose == CdbWritePurpose::Trigger {
                    s.phase = Phase::Flag;
                    wait(plan, &mut s, &mut effects, now)?;
                } else if let Phase::Stage(part, offset) = s.phase {
                    let end = offset + w.bytes.len();
                    let total = match part {
                        0 => 4,
                        1 => 2,
                        _ => 1,
                    };
                    s.phase = if end < total {
                        Phase::Stage(part, end)
                    } else if part < 2 {
                        Phase::Stage(part + 1, 0)
                    } else {
                        Phase::Arm
                    };
                }
            }
        }
        CdbEvent::ReadResult {
            id,
            observation,
            exchange,
        } => {
            let p = s.pending.clone().ok_or(CdbInputError::WrongStep)?;
            if p.id != id {
                return Err(CdbInputError::WrongStep);
            }
            let dispatched = p.dispatched.ok_or(CdbInputError::NotDispatched)?;
            let PendingKind::Read(r) = p.kind else {
                return Err(CdbInputError::WrongEvent);
            };
            if observation.range != r.range
                || observation
                    .stamp
                    .validate(s.identity.generation, s.identity.epoch)
                    .is_err()
                || observation.stamp.freshness != Freshness::Fresh
                || observation.stamp.source != ObservationSource::DaemonObserved
                || observation.requester != ReadRequester::ExplicitClient
                || observation.stamp.started_at < dispatched
                || observation.stamp.completed_at > now
            {
                return Err(CdbInputError::InvalidObservation);
            }
            let bytes = observation.bytes();
            let actual = match &observation.outcome {
                RangeOutcome::Bytes(b) => b.len(),
                _ => 0,
            };
            s.evidence.reads.push(CdbReadEvidence {
                id,
                range: r.range,
                stamp: observation.stamp.clone(),
                actual_length: actual,
                exchange,
            });
            s.evidence
                .issued
                .iter_mut()
                .find(|i| i.id == id)
                .unwrap()
                .completed_at = Some(observation.stamp.completed_at);
            if exchange == CdbReadExchange::Success && r.range.offset == 8 {
                s.evidence.consumed.push((id, observation.clone()));
                effects.push(CdbAction::RetainConsumedObservation {
                    id,
                    observation: observation.clone(),
                });
            }
            s.pending = None;
            s.evidence.outstanding = None;
            if s.terminal.is_none() {
                let qualified_nack = exchange == CdbReadExchange::QualifiedContinuityPreservingNack
                    && plan.qualification.read_nack_preserves_custody
                    && matches!(s.phase, Phase::Preflight | Phase::Flag | Phase::Status);
                if exchange != CdbReadExchange::Success && !qualified_nack {
                    stop(&mut s, &mut effects, CdbStopReason::Transport);
                } else if now >= deadline(plan, &s) {
                    timeout(&mut s, &mut effects);
                } else if qualified_nack {
                    s.polls += 1;
                    s.nack_wait = true;
                    // The advertised foreground bound limits *access holdoff*,
                    // not successful flag polls. Zero means no NACK grace.
                    if s.evidence.holdoff_deadline.is_some_and(|t| now >= t) {
                        timeout(&mut s, &mut effects);
                    } else {
                        wait(plan, &mut s, &mut effects, observation.stamp.completed_at)?;
                    }
                } else if r.range.offset == 8
                    && !observation
                        .stamp
                        .consumed_effects
                        .contains(&ReadEffect::ClearOnRead)
                {
                    stop(&mut s, &mut effects, CdbStopReason::Transport);
                } else if let Some(b) = bytes {
                    s.nack_wait = false;
                    read_success(
                        plan,
                        &mut s,
                        &mut effects,
                        id,
                        r.purpose,
                        b,
                        observation.stamp.completed_at,
                    )?;
                } else if s.evidence.trigger_may_have_executed {
                    invalid(&mut s, &mut effects, CdbInvalidReply::ByteCount);
                } else {
                    stop(&mut s, &mut effects, CdbStopReason::Transport);
                }
            }
        }
    }
    Ok(CdbReduction { state: s, effects })
}
fn read_success(
    plan: &ManagedCdbPlan,
    s: &mut ManagedCdbState,
    effects: &mut Vec<CdbAction>,
    id: CdbStepId,
    purpose: CdbReadPurpose,
    b: &[u8],
    end: MonoTime,
) -> Result<(), CdbInputError> {
    match purpose {
        CdbReadPurpose::Preflight => {
            s.polls += 1;
            if b[0] & 0x80 != 0 {
                wait(plan, s, effects, end)?;
            } else {
                s.phase = Phase::SelectStage;
            }
        }
        CdbReadPurpose::Arm => s.phase = Phase::Recheck, // Old bit 6 is deliberately ignored.
        CdbReadPurpose::RecheckIdle => {
            if b[0] & 0x80 != 0 {
                stop(s, effects, CdbStopReason::BusyBeforeTrigger);
            } else {
                s.phase = Phase::Trigger;
            }
        }
        CdbReadPurpose::CompletionFlag => {
            s.polls += 1;
            if b[0] & 0x40 != 0 {
                s.evidence.fresh_completion = Some(id);
                s.phase = Phase::Status;
            } else {
                wait(plan, s, effects, end)?;
            }
        }
        CdbReadPurpose::CompletionStatus => {
            s.evidence.raw_status = Some(b[0]);
            if b[0] & 0x80 != 0 {
                invalid(s, effects, CdbInvalidReply::BusyAfterCompletion);
            } else if b[0] & 0x40 != 0 {
                s.evidence.mailbox_unresolved = false;
                finish(
                    s,
                    effects,
                    CdbOutcome::Failed {
                        raw_status: b[0],
                        result: b[0] & 63,
                        evidence: s.evidence.clone(),
                    },
                );
            } else if b[0] != 1 {
                invalid(s, effects, CdbInvalidReply::UnexpectedStatus);
            } else {
                s.phase = Phase::SelectReply;
            }
        }
        CdbReadPurpose::ReplyHeader => {
            s.header.extend_from_slice(b);
            if s.header.len() < 2 {
                return Ok(());
            }
            let len = s.header[0];
            s.evidence.actual_reply_length = Some(len);
            s.reply_checksum = s.header[1];
            // Share the legacy adapter's minimum and LPL capacity validation.
            let length_valid = plan.command.validate_length(usize::from(len));
            let reason = match len {
                0..=120 if !length_valid => Some(CdbInvalidReply::Truncated),
                0..=120 if plan.command.accepts_length(usize::from(len)) => None,
                0..=120 => Some(CdbInvalidReply::UnexpectedLength),
                121..=239 => Some(CdbInvalidReply::ReservedLength),
                _ => Some(CdbInvalidReply::EplUnsupported),
            };
            if let Some(reason) = reason {
                invalid(s, effects, reason);
            } else {
                s.phase = Phase::Body;
            }
        }
        CdbReadPurpose::ReplyBody => {
            s.reply.extend_from_slice(b);
            if s.reply.len() == usize::from(s.header[0]) {
                s.evidence.checksum_valid =
                    Some(validate_reply_checksum(&s.reply, s.reply_checksum).is_ok());
                if s.evidence.checksum_valid != Some(true) {
                    invalid(s, effects, CdbInvalidReply::Checksum);
                } else {
                    s.evidence.mailbox_unresolved = false;
                    let outcome = plan.command.completed(&s.reply, s.evidence.clone());
                    finish(s, effects, outcome);
                }
            }
        }
    }
    Ok(())
}

/// Availability-aware view of a fully validated reply. Raw records are retained
/// in Completed; absent ImageInformation bits never fabricate zero versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbFirmwareImages<'a> {
    /// Image A's version string, when advertised.
    pub image_a: Option<&'a [u8]>,
    /// Image B's version string, when advertised.
    pub image_b: Option<&'a [u8]>,
    /// Factory-boot image's version string, when advertised.
    pub factory_boot: Option<&'a [u8]>,
}
impl CdbOutcome {
    /// The firmware images a completed 0100h reply advertises; `None` for
    /// any other outcome or absent ImageInformation bits (never fabricated).
    pub fn firmware_images(&self) -> Option<CdbFirmwareImages<'_>> {
        let Self::Completed { reply, .. } = self else {
            return None;
        };
        let info = *reply.image_information.first()?;
        Some(CdbFirmwareImages {
            image_a: (info & 1 != 0).then_some(reply.image_a_firmware.as_slice()),
            image_b: (info & 2 != 0).then_some(reply.image_b_firmware.as_slice()),
            factory_boot: (info & 4 != 0).then_some(reply.factory_boot_firmware.as_slice()),
        })
    }
}

/// A payload-free projection for audit sinks. Never serialize CdbEvidence itself
/// to an audit log: its consumed history intentionally contains complete flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdbAuditEvidence {
    /// Whether a trigger write may have reached the module.
    pub trigger_may_have_executed: bool,
    /// Every dispatched step's issue record.
    pub issued: Vec<CdbIssuedStep>,
    /// Identity references of the consumed clear-on-read observations
    /// (values deliberately excluded).
    pub consumed_read_references: Vec<CdbStepId>,
    /// The step whose read observed the completion flag set.
    pub fresh_completion: Option<CdbStepId>,
    /// The raw completion status byte, when read.
    pub raw_status: Option<u8>,
    /// The reply length the header advertised.
    pub actual_reply_length: Option<u8>,
    /// Whether the reply checksum validated, when checked.
    pub checksum_valid: Option<bool>,
    /// The still-outstanding step, when one exists.
    pub outstanding: Option<CdbStepId>,
    /// The CDB mailbox is (still) in an unresolved state.
    pub mailbox_unresolved: bool,
}
impl CdbEvidence {
    /// The payload-free projection for audit sinks.
    pub fn audit_description(&self) -> CdbAuditEvidence {
        CdbAuditEvidence {
            trigger_may_have_executed: self.trigger_may_have_executed,
            issued: self.issued.clone(),
            consumed_read_references: self.consumed.iter().map(|(id, _)| *id).collect(),
            fresh_completion: self.fresh_completion,
            raw_status: self.raw_status,
            actual_reply_length: self.actual_reply_length,
            checksum_valid: self.checksum_valid,
            outstanding: self.outstanding,
            mailbox_unresolved: self.mailbox_unresolved,
        }
    }
}
/// Terminal classification for audit logging, without firmware bytes or
/// consumed flag values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdbTerminalReason {
    /// The command completed with a validated reply.
    Completed,
    /// The module returned a CDB error status.
    Failed,
    /// The module stayed busy until a deadline expired, at this stage.
    BusyTimeout(CdbTimeoutStage),
    /// Stopped after a possibly-executed trigger, for this reason.
    Ambiguous(CdbStopReason),
    /// Refused before any I/O, for this reason.
    Refused(CdbRefusal),
    /// Stopped before the trigger, for this reason.
    Stopped(CdbStopReason),
    /// A reply failed validation, for this reason.
    InvalidReply(CdbInvalidReply),
}
impl CdbOutcome {
    /// Terminal classification without firmware bytes or consumed flag values.
    pub fn audit_reason(&self) -> CdbTerminalReason {
        match self {
            Self::Completed { .. }
            | Self::CompletedFirmwareManagementFeatures { .. }
            | Self::CompletedModuleFeatures { .. } => CdbTerminalReason::Completed,
            Self::Failed { .. } => CdbTerminalReason::Failed,
            Self::BusyTimeout { stage, .. } => CdbTerminalReason::BusyTimeout(*stage),
            Self::AmbiguousAfterTrigger { reason, .. } => CdbTerminalReason::Ambiguous(*reason),
            Self::RefusedBeforeIo { reason } => CdbTerminalReason::Refused(*reason),
            Self::StoppedBeforeTrigger { reason, .. } => CdbTerminalReason::Stopped(*reason),
            Self::InvalidReply { reason, .. } => CdbTerminalReason::InvalidReply(*reason),
        }
    }
}
