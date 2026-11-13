//! IR for `openxcvr.state_machine.v1` and `openxcvr.state_machine.v2`
//! schemas.
//!
//! The parser (`crate::parser::state_machines`) deserializes into private raw
//! types that deny unknown keys, validates them, and builds this IR. Every key
//! either schema version accepts is represented here, so nothing an author
//! writes is silently dropped: an input the format cannot represent is a parse
//! error.
//!
//! # v2 additions over v1
//!
//! - `states[].class`: `stable | transient | fault | terminal` (required in v2).
//!   A fault state that halts the machine keeps class `fault` and is listed in
//!   `terminal_states`; class `terminal` is for non-fault halting states.
//! - `states[].reported`: `false` marks a state the module cannot report in its
//!   state register (CMIS Resetting/Reset/MgmtInit, which byte 3 cannot carry);
//!   such a state has no `enum_value`. A v2 state without `enum_value` must say
//!   `reported: false`.
//! - `states[].since` / `until` and `transitions[].since` / `until`:
//!   revision bounds (`until` exclusive, as for page fields).
//! - `states[].timeout` may be one entry or a list, each with an explicit
//!   `source`:
//!   - `standard`: a timer the standard defines (`timer:` name such as
//!     `MaxDurationModulePwrUp` or `tMgmtInit`) with exactly one of
//!     `duration_ms` / `duration_ms_field`, optional `on_expiry`, `since`,
//!     `until`, `provenance`;
//!   - `host_policy`: `policy: readiness_backstop`, referencing the top-level
//!     `host_policy` block. It carries no duration and no `on_expiry`: a host
//!     deadline never moves the module and never masquerades as a standard
//!     timer.
//! - Top-level `host_policy`: `poll_cadence` and `readiness_backstop`, each
//!   `{ value_ms, provenance, description? }`. Host choices the standard does
//!   not set (kmod's 50 ms cadence and 10 s backstop) live only here.
//!
//! v1 timeouts carry no source; they are kept as
//! [`TimeoutSource::Unattributed`] rather than being claimed as standard timers.

use crate::ir::pages::{AliasedToEntry, RenamedFromEntry};
use crate::ir::version::Rev;
use std::collections::BTreeSet;

/// Schema namespace version a file was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateMachineFormat {
    V1,
    V2,
}

impl StateMachineFormat {
    pub fn namespace(self) -> &'static str {
        match self {
            StateMachineFormat::V1 => "openxcvr.state_machine.v1",
            StateMachineFormat::V2 => "openxcvr.state_machine.v2",
        }
    }
}

/// revision bounds: `since` inclusive, `until` exclusive. `None`
/// means unbounded on that side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RevRange {
    pub since: Option<Rev>,
    pub until: Option<Rev>,
}

impl RevRange {
    /// Unbounded on both sides.
    pub const ALL: RevRange = RevRange {
        since: None,
        until: None,
    };

    pub fn contains(&self, rev: Rev) -> bool {
        self.since.is_none_or(|s| rev >= s) && self.until.is_none_or(|u| rev < u)
    }

    /// The supported CMIS revisions this range covers.
    pub fn revs(&self) -> BTreeSet<Rev> {
        Rev::CMIS_ALL
            .iter()
            .copied()
            .filter(|r| self.contains(*r))
            .collect()
    }
}

/// A parsed, validated state-machine schema.
#[derive(Debug, Clone)]
pub struct StateMachineSchema {
    pub format: StateMachineFormat,
    pub id: String,
    pub title: Option<String>,
    /// Standard citation block (`spec: { id, revision, errata }`), kept as data.
    pub spec: Option<serde_norway::Value>,
    /// Machine-level provenance block, kept as data.
    pub provenance: Option<serde_norway::Value>,
    /// The `openxcvr.enum.v2` registry naming this machine's reported states.
    pub state_enum: Option<String>,
    pub parent_machine: Option<String>,
    pub lifecycle_dependency: Option<String>,
    pub scope: Option<Scope>,
    pub initial_state: String,
    pub states: Vec<State>,
    pub terminal_states: Vec<TerminalState>,
    /// v2 only; always `None` for v1.
    pub host_policy: Option<HostPolicy>,
    pub transitions: Vec<Transition>,
}

impl StateMachineSchema {
    pub fn state(&self, name: &str) -> Option<&State> {
        self.states.iter().find(|s| s.name == name)
    }

    /// States present in `rev`, in schema order.
    pub fn states_in(&self, rev: Rev) -> impl Iterator<Item = &State> {
        self.states.iter().filter(move |s| s.revs.contains(rev))
    }

    /// Transitions present in `rev`, in schema order.
    pub fn transitions_in(&self, rev: Rev) -> impl Iterator<Item = &Transition> {
        self.transitions
            .iter()
            .filter(move |t| t.revs.contains(rev))
    }

    /// The `terminal_states` entry for `state`, if it is terminal.
    pub fn terminal_exit(&self, state: &str) -> Option<TerminalExit> {
        self.terminal_states
            .iter()
            .find(|t| t.state == state)
            .map(|t| t.exit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub kind: ScopeKind,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    PerModule,
    PerLane,
    PerDataPath,
    PerNetworkPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cardinality {
    Fixed(u32),
    /// path of the field advertising the instance count.
    Field(String),
}

/// Behavioural class of a state (v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateClass {
    Stable,
    Transient,
    Fault,
    Terminal,
}

#[derive(Debug, Clone)]
pub struct State {
    pub name: String,
    pub encoding: StateEncoding,
    /// `None` only for v1 schemas, which carry no class.
    pub class: Option<StateClass>,
    pub description: Option<String>,
    pub invariants: Vec<String>,
    pub timeouts: Vec<StateTimeout>,
    pub revs: RevRange,
    pub provenance: Option<serde_norway::Value>,
}

/// How (or whether) the module reports a state in its state register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateEncoding {
    Reported {
        enum_name: Option<String>,
        value: u64,
    },
    /// The module cannot report this state (v2 `reported: false`).
    Unreported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTimeout {
    pub source: TimeoutSource,
    /// State entered when a module-side timer expires. Never set for host
    /// policy, whose expiry does not move the module.
    pub on_expiry: Option<String>,
    pub revs: RevRange,
    pub provenance: Option<serde_norway::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutSource {
    /// A timer the standard defines.
    Standard {
        timer: String,
        duration: TimerDuration,
    },
    /// A host-side choice from the schema's `host_policy` block.
    HostPolicy(HostPolicyKey),
    /// v1 timeout: the schema does not say whose timer it is.
    Unattributed(TimerDuration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerDuration {
    FixedMs(u64),
    /// path of the field advertising the duration.
    AdvertisedField(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPolicyKey {
    PollCadence,
    ReadinessBackstop,
}

/// Host choices the standard does not set, kept apart from standard timers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostPolicy {
    pub poll_cadence: Option<HostPolicyValue>,
    pub readiness_backstop: Option<HostPolicyValue>,
}

impl HostPolicy {
    pub fn get(&self, key: HostPolicyKey) -> Option<&HostPolicyValue> {
        match key {
            HostPolicyKey::PollCadence => self.poll_cadence.as_ref(),
            HostPolicyKey::ReadinessBackstop => self.readiness_backstop.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPolicyValue {
    pub value_ms: u64,
    pub description: Option<String>,
    /// Required: where the host choice comes from (e.g. kmod source lines).
    pub provenance: serde_norway::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalState {
    pub state: String,
    pub exit: TerminalExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalExit {
    PowerCycle,
    ManagementReset,
    SoftwareRecovery,
}

/// A single edge in the state graph.
#[derive(Debug, Clone)]
pub struct Transition {
    pub id: Option<String>,
    pub from: String,
    pub to: String,
    pub trigger: Trigger,
    /// § 6 structured guard predicates, as data. Empty for
    /// `guard: null` or an absent guard.
    pub guards: Vec<Guard>,
    pub side_effects: Vec<SideEffect>,
    pub description: Option<String>,
    pub provenance: Option<serde_norway::Value>,
    pub revs: RevRange,
    /// change records. Documentary only: a transition whose
    /// behaviour differs between revisions is written as separate transitions
    /// bounded by `since`/`until`.
    pub changed_in: Vec<TransitionChange>,
    pub renamed_from: Option<RenamedFromEntry>,
    pub aliased_to: Vec<AliasedToEntry>,
}

impl Transition {
    /// Stable identifier for messages: `id` if present, else `from->to`.
    pub fn label(&self) -> String {
        self.id
            .clone()
            .unwrap_or_else(|| format!("{}->{}", self.from, self.to))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionChange {
    pub rev: Rev,
    pub note: Option<String>,
    pub provenance: Option<serde_norway::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    RegisterWrite,
    RegisterRead,
    Timer,
    Internal,
    ExternalEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub condition: Option<String>,
    /// `register_write` / `register_read` only (required there).
    pub register: Option<String>,
    /// `register_write` / `register_read` only.
    pub value: Option<Scalar>,
    /// `external_event` only.
    pub signal: Option<String>,
    /// `timer` only.
    pub timer: Option<String>,
    /// `timer` only (required there).
    pub duration: Option<TimerDuration>,
}

/// A literal compared against or assigned to a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    /// An enum member or other symbolic name.
    Symbol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    /// `{ field, equals }`: the field (first instance if banked) equals `value`.
    Equals { field: String, value: Scalar },
    /// `forall: { field, equals }`: every instance of the field equals `value`.
    ForAll { field: String, value: Scalar },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChangeOp {
    Set,
    Clear,
    Assign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideEffect {
    StateChange {
        target: String,
        operation: StateChangeOp,
        /// Present exactly when `operation` is `Assign`.
        value: Option<Scalar>,
    },
    LogEvent {
        event_id: String,
    },
    StartTimer {
        timer: String,
        duration: TimerDuration,
    },
    CancelTimer {
        timer: String,
    },
    SpawnMachine {
        machine: String,
    },
    TerminateMachine {
        machine: String,
    },
}
