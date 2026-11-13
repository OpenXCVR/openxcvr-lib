//! Pure, generic state-machine runtime: the descriptor types the
//! behaviour tables are built from, plus the pure
//! evaluators over them.
//!
//! The library parses and decides and never acts on a module on its own
//! initiative. Everything here is data in, decision out: a caller supplies
//! the current state, the event that happened, and a [`GuardContext`] over
//! already-observed facts; the routines return the next state, a planned
//! transition sequence, or a typed refusal. No I/O, no clocks, no resident
//! state, no timers are started or cancelled — side effects are returned as
//! data for the caller (the daemon) to execute.
//!
//! Generated tables are `&'static FsmTable<S>` values (design decision:
//! static plain-data tables, no dynamic dispatch, no heap), so every type
//! here is plain data usable in `const`/`static` context.

use std::fmt;

/// Behavioural class of a state, when the schema models one
/// (`openxcvr.state_machine.v2` `class:`). `None` means the source schema
/// does not model classes (v1); terminal-ness is separately available from
/// [`FsmTable::terminal_exit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmStateClass {
    /// A settled state the machine rests in.
    Stable,
    /// A passing state the machine moves through on a timer or event.
    Transient,
    /// A fault state entered on error.
    Fault,
    /// A terminal state exited only by the recorded terminal exit.
    Terminal,
}

/// A literal a guard compares a field against or a state-change side effect
/// assigns. `Symbol` is an enum member or other symbolic name from the
/// schema (e.g. the target state name written into a state register).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmScalar {
    /// A boolean literal.
    Bool(bool),
    /// An integer literal.
    Int(i64),
    /// A symbolic name (enum member or other schema name).
    Symbol(&'static str),
}

impl FsmScalar {
    /// Render for diagnostics; never a parse format.
    pub fn render(self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Symbol(s) => s.to_string(),
        }
    }
}

/// An observed field value supplied by the caller's [`GuardContext`].
/// `Name` supplies a symbolic value (a field decoded to a named member).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmValue<'a> {
    /// An observed boolean.
    Bool(bool),
    /// An observed integer.
    Int(i64),
    /// An observed symbolic value (a decoded named member).
    Name(&'a str),
}

impl FsmValue<'_> {
    /// Whether this observed value satisfies the schema literal.
    ///
    /// Fail closed: a `Name` only satisfies the same `Symbol` by exact
    /// string equality, and mismatched kinds never satisfy.
    fn satisfies(self, expected: FsmScalar) -> bool {
        match (self, expected) {
            (Self::Bool(o), FsmScalar::Bool(e)) => o == e,
            (Self::Int(o), FsmScalar::Int(e)) => o == e,
            (Self::Name(o), FsmScalar::Symbol(e)) => o == e,
            _ => false,
        }
    }
}

/// What kind of happening drives a transition, as recorded by the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmTriggerKind {
    /// A write to a module register.
    RegisterWrite,
    /// A read of a module register.
    RegisterRead,
    /// A standard timer's expiry.
    Timer,
    /// An internal (module-generated) happening.
    Internal,
    /// An externally sourced signal.
    ExternalEvent,
}

/// The trigger recorded on a transition edge, as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTrigger {
    /// The kind of happening this trigger fires on.
    pub kind: FsmTriggerKind,
    /// `register_write` / `register_read` triggers: the field path written
    /// or read, when the schema names one.
    pub register: Option<&'static str>,
    /// `register_write` triggers: the value written, when recorded.
    pub value: Option<FsmScalar>,
    /// `external_event` triggers: the signal name, when recorded.
    pub signal: Option<&'static str>,
    /// `timer` triggers: the standard timer that expired, when recorded.
    pub timer: Option<&'static str>,
    /// `timer` triggers: the standard's duration for the timer, when
    /// recorded.
    pub duration: Option<FsmTimerDuration>,
    /// The schema's recorded condition prose for this trigger, when present.
    /// Data for diagnostics; matching never consults it.
    pub condition: Option<&'static str>,
}

/// One happening the caller observed and offers to the machine. Names are
/// caller-supplied and matched against the schema-recorded trigger names by
/// exact string equality; a trigger that records no name matches any event
/// of its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmEvent<'a> {
    /// A write to the named register.
    RegisterWrite {
        /// The register written.
        register: &'a str,
    },
    /// A read of the named register.
    RegisterRead {
        /// The register read.
        register: &'a str,
    },
    /// The named standard timer expired.
    TimerExpired {
        /// The timer that expired.
        timer: &'a str,
    },
    /// An external signal arrived.
    ExternalSignal {
        /// The signal that arrived.
        signal: &'a str,
    },
    /// An internal module-generated happening.
    Internal,
}

impl FsmTrigger {
    /// Whether this recorded trigger fires on the supplied event.
    fn fires_on(&self, event: FsmEvent<'_>) -> bool {
        match (self.kind, event) {
            (FsmTriggerKind::RegisterWrite, FsmEvent::RegisterWrite { register }) => {
                self.register.is_none_or(|r| r == register)
            }
            (FsmTriggerKind::RegisterRead, FsmEvent::RegisterRead { register }) => {
                self.register.is_none_or(|r| r == register)
            }
            (FsmTriggerKind::Timer, FsmEvent::TimerExpired { timer }) => {
                self.timer.is_none_or(|t| t == timer)
            }
            (FsmTriggerKind::ExternalEvent, FsmEvent::ExternalSignal { signal }) => {
                self.signal.is_none_or(|s| s == signal)
            }
            (FsmTriggerKind::Internal, FsmEvent::Internal) => true,
            _ => false,
        }
    }
}

/// A guard predicate recorded on an edge, as plain data. `Equals` is the
/// field's first instance (if banked); `ForAll` is every instance — the
/// bank/instance projection is the caller's job when it supplies values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmGuard {
    /// The field's first instance equals the value (if banked).
    Equals {
        /// The guarded field's path.
        field: &'static str,
        /// The value the field must carry.
        value: FsmScalar,
    },
    /// Every instance of the field equals the value.
    ForAll {
        /// The guarded field's path.
        field: &'static str,
        /// The value every instance must carry.
        value: FsmScalar,
    },
}

/// The caller's view of the fields the guards reference. Pure lookup over
/// already-observed facts: the trait reads nothing itself.
///
/// Returning `None` for a field means the caller cannot supply it, and every
/// guard over it fails closed — an unsupplied field never satisfies a guard.
pub trait GuardContext {
    /// The observed value of the field at `path`, when the caller can
    /// supply it.
    fn field(&self, path: &str) -> Option<FsmValue<'_>>;
}

/// A guard context with no facts: every guard fails closed. Useful for
/// callers that only follow unguarded edges.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoGuards;

impl GuardContext for NoGuards {
    fn field(&self, _path: &str) -> Option<FsmValue<'_>> {
        None
    }
}

/// The operation a state-change side effect performs on a register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmStateChangeOp {
    /// Set the field (to its asserted/one value).
    Set,
    /// Clear the field.
    Clear,
    /// Assign the named scalar value to the field.
    Assign,
}

/// Where a timer's duration comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmTimerDuration {
    /// A fixed duration the standard sets, in milliseconds.
    FixedMs(u64),
    /// path of the field advertising the duration (page 01h).
    AdvertisedField(&'static str),
}

/// An effect the edge has, returned as data for the caller to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmSideEffect {
    /// Change a module register.
    StateChange {
        /// The register to change.
        target: &'static str,
        /// The operation to perform on it.
        operation: FsmStateChangeOp,
        /// The value to assign; present exactly when `operation` is
        /// `Assign`.
        value: Option<FsmScalar>,
    },
    /// Emit a log event.
    LogEvent {
        /// The schema's event identifier.
        event_id: &'static str,
    },
    /// Start a named timer.
    StartTimer {
        /// The timer to start.
        timer: &'static str,
        /// The timer's duration source.
        duration: FsmTimerDuration,
    },
    /// Cancel a named timer.
    CancelTimer {
        /// The timer to cancel.
        timer: &'static str,
    },
    /// Spawn a named sub-machine.
    SpawnMachine {
        /// The machine to spawn.
        machine: &'static str,
    },
    /// Terminate a named sub-machine.
    TerminateMachine {
        /// The machine to terminate.
        machine: &'static str,
    },
}

/// One state's descriptor in a behaviour table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmStateDesc<S: 'static> {
    /// The state value itself.
    pub state: S,
    /// `None` when the source schema does not model classes.
    pub class: Option<FsmStateClass>,
    /// Whether the module can report this state in its state register.
    pub reported: bool,
    /// The state's recorded invariant prose, for diagnostics.
    pub invariants: &'static [&'static str],
}

/// One edge in a behaviour table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTransition<S: 'static> {
    /// Stable schema identifier (`id`, else `from->to`).
    pub id: &'static str,
    /// The state the edge leaves.
    pub from: S,
    /// The state the edge enters.
    pub to: S,
    /// The recorded trigger that fires the edge.
    pub trigger: FsmTrigger,
    /// The guards all of which must be satisfied.
    pub guards: &'static [FsmGuard],
    /// The edge's side effects, returned as data for the caller.
    pub side_effects: &'static [FsmSideEffect],
}

/// How a terminal state is exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmTerminalExit {
    /// Exited only by a power cycle.
    PowerCycle,
    /// Exited by a management reset.
    ManagementReset,
    /// Exited by a software recovery action.
    SoftwareRecovery,
}

/// One recorded terminal-exit: which state, exited how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTerminalExitDesc<S: 'static> {
    /// The terminal state.
    pub state: S,
    /// How the state is exited.
    pub exit: FsmTerminalExit,
}

/// Which host choice a host-policy timeout references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmHostPolicyKey {
    /// The host's poll cadence choice.
    PollCadence,
    /// The host's readiness backstop choice.
    ReadinessBackstop,
}

/// Whose timer a state's timeout is, mirroring the schema's timeout source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmTimeoutSource {
    /// A timer the standard defines.
    Standard {
        /// The standard's name for the timer.
        timer: &'static str,
        /// The timer's duration source.
        duration: FsmTimerDuration,
    },
    /// A host-side choice from the schema's `host_policy` block. It carries
    /// no duration and no module-moving `on_expiry`.
    HostPolicy(FsmHostPolicyKey),
    /// The v1 schema records a timeout without saying whose timer it is.
    Unattributed(FsmTimerDuration),
}

/// One state's timeout, as recorded by the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTimeout<S: 'static> {
    /// The state the timeout is recorded on.
    pub state: S,
    /// Whose timer it is and what its duration is.
    pub source: FsmTimeoutSource,
    /// The state entered when a module-side timer expires. Never set for
    /// host policy, whose expiry does not move the module. `None` also
    /// covers a standard timer recorded without a destination.
    pub on_expiry: Option<S>,
}

/// A complete behaviour table for one machine at one revision: plain data,
/// usable as a `static`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTable<S: 'static> {
    /// Schema id, e.g. `CmisModuleStateMachine`.
    pub id: &'static str,
    /// The machine's initial state.
    pub initial: S,
    /// Every state descriptor, in schema order.
    pub states: &'static [FsmStateDesc<S>],
    /// Every edge, in schema order (the evaluate tie-break).
    pub transitions: &'static [FsmTransition<S>],
    /// The recorded terminal exits.
    pub terminal_exits: &'static [FsmTerminalExitDesc<S>],
    /// The recorded per-state timeouts.
    pub timeouts: &'static [FsmTimeout<S>],
}

/// What evaluating one event against a table produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmStep<'t, S: 'static> {
    /// No edge from `current` matches the event with the supplied guard
    /// context. The machine stays where it was; nothing was executed.
    NoMatch,
    /// The matched edge: its destination state and the edge itself (guards
    /// already satisfied, side effects still owed to the caller).
    Advance {
        /// The destination state.
        to: S,
        /// The matched edge, with its side-effect plan.
        transition: &'t FsmTransition<S>,
    },
}

/// Why a requested target state cannot be planned from the current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmPathError {
    /// No sequence of edges in this table leads from the current state to
    /// the target.
    Unreachable {
        /// The table that was searched.
        table: &'static str,
        /// The starting state, rendered.
        from: String,
        /// The unreachable target, rendered.
        to: String,
    },
}

impl fmt::Display for FsmPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { table, from, to } => {
                write!(f, "{table}: no path from {from} to {to}")
            }
        }
    }
}

impl std::error::Error for FsmPathError {}

/// Whether a guard is satisfied by the caller's supplied facts.
///
/// Fail closed: a field the caller cannot supply never satisfies a guard,
/// and neither does a value of the wrong kind.
pub fn guard_satisfied(guard: &FsmGuard, ctx: &dyn GuardContext) -> bool {
    let (field, value) = match guard {
        FsmGuard::Equals { field, value } | FsmGuard::ForAll { field, value } => (*field, *value),
    };
    // `ForAll` over banked fields and `Equals` over the first instance are
    // the same lookup here: the caller's GuardContext projection decides
    // which instances it answers for, and an unsupplied field fails both.
    ctx.field(field)
        .is_some_and(|observed| observed.satisfies(value))
}

impl<S: Copy + Eq + 'static> FsmTable<S> {
    /// The descriptor of `state`, if the table knows it.
    pub fn state_desc(&self, state: S) -> Option<&FsmStateDesc<S>> {
        self.states.iter().find(|d| d.state == state)
    }

    /// Whether `state` is terminal in this table, and how it is exited.
    pub fn terminal_exit(&self, state: S) -> Option<FsmTerminalExit> {
        self.terminal_exits
            .iter()
            .find(|t| t.state == state)
            .map(|t| t.exit)
    }

    /// Whether `state` is terminal in this table.
    pub fn is_terminal(&self, state: S) -> bool {
        self.terminal_exit(state).is_some()
    }

    /// The timeout recorded for `state`, if any.
    pub fn timeout_for(&self, state: S) -> Option<&FsmTimeout<S>> {
        self.timeouts.iter().find(|t| t.state == state)
    }

    /// Edges leaving `state`, in schema order.
    pub fn edges_from(&self, state: S) -> impl Iterator<Item = &FsmTransition<S>> {
        self.transitions.iter().filter(move |t| t.from == state)
    }

    /// Evaluate one observed event against the table from `current`.
    ///
    /// The first edge in schema order whose trigger fires on the event and
    /// whose guards are all satisfied by the supplied context wins; schema
    /// order is the tie-break the schema author recorded. No edge matches
    /// → [`FsmStep::NoMatch`] and the caller changes nothing (fail closed).
    /// Matching an edge changes nothing here either: the caller receives the
    /// destination and the side-effect plan and decides whether to execute.
    pub fn evaluate<'t>(
        &'t self,
        current: S,
        event: FsmEvent<'_>,
        ctx: &dyn GuardContext,
    ) -> FsmStep<'t, S> {
        for edge in self.transitions {
            if edge.from != current || !edge.trigger.fires_on(event) {
                continue;
            }
            if edge.guards.iter().all(|g| guard_satisfied(g, ctx)) {
                return FsmStep::Advance {
                    to: edge.to,
                    transition: edge,
                };
            }
        }
        FsmStep::NoMatch
    }
}

/// Plan the shortest edge sequence from `current` to `target`, ignoring
/// guards: a structural plan the caller re-validates edge by edge with
/// [`FsmTable::evaluate`] as it executes (guards depend on facts only the
/// caller observes live).
///
/// Breadth-first over the table's edges, so the returned path is a
/// shortest path in edge count; within one length, schema order wins.
/// Returns the empty vector when `current == target` (already there: no
/// transition is needed), and [`FsmPathError::Unreachable`] when no path
/// exists.
pub fn plan_path<'t, S: Copy + Eq + fmt::Debug + 'static>(
    table: &'t FsmTable<S>,
    current: S,
    target: S,
) -> Result<Vec<&'t FsmTransition<S>>, FsmPathError> {
    if current == target {
        return Ok(Vec::new());
    }
    // BFS with per-state parent edges; the table is small (tens of edges),
    // so the visited map is a plain Vec over table states.
    let mut parent: Vec<(&'t FsmTransition<S>, S)> = Vec::new(); // (edge, from)
    let mut visited: Vec<S> = vec![current];
    let mut frontier: Vec<S> = vec![current];
    while !frontier.is_empty() {
        let mut next: Vec<S> = Vec::new();
        for from in &frontier {
            for edge in table.edges_from(*from) {
                if visited.contains(&edge.to) {
                    continue;
                }
                visited.push(edge.to);
                parent.push((edge, *from));
                if edge.to == target {
                    // Reconstruct: walk parents backwards.
                    let mut path: Vec<&'t FsmTransition<S>> = Vec::new();
                    let mut at = edge.to;
                    while at != current {
                        let (e, prev) = parent
                            .iter()
                            .rev()
                            .find(|(e, _)| e.to == at)
                            .copied()
                            .expect("BFS parent chain is complete");
                        path.push(e);
                        at = prev;
                    }
                    path.reverse();
                    return Ok(path);
                }
                next.push(edge.to);
            }
        }
        frontier = next;
    }
    Err(FsmPathError::Unreachable {
        table: table.id,
        from: format!("{current:?}"),
        to: format!("{target:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal two-state machine: A --(write reg)--> B with one guard,
    // B --(timer)--> A, plus a guarded A --> B second edge.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum St {
        A,
        B,
    }

    const G: FsmGuard = FsmGuard::Equals {
        field: "lower.allow",
        value: FsmScalar::Bool(false),
    };

    const EDGE_AB: FsmTransition<St> = FsmTransition {
        id: "a_to_b",
        from: St::A,
        to: St::B,
        trigger: FsmTrigger {
            kind: FsmTriggerKind::RegisterWrite,
            register: Some("lower.req"),
            value: Some(FsmScalar::Int(0)),
            signal: None,
            timer: None,
            duration: None,
            condition: None,
        },
        guards: &[G],
        side_effects: &[],
    };

    const EDGE_BA: FsmTransition<St> = FsmTransition {
        id: "b_to_a",
        from: St::B,
        to: St::A,
        trigger: FsmTrigger {
            kind: FsmTriggerKind::Timer,
            register: None,
            value: None,
            signal: None,
            timer: Some("MaxDuration"),
            duration: None,
            condition: None,
        },
        guards: &[],
        side_effects: &[],
    };

    const ST_DESCS: &[FsmStateDesc<St>] = &[
        FsmStateDesc {
            state: St::A,
            class: Some(FsmStateClass::Stable),
            reported: true,
            invariants: &["A invariant"],
        },
        FsmStateDesc {
            state: St::B,
            class: Some(FsmStateClass::Transient),
            reported: true,
            invariants: &[],
        },
    ];

    const TERMINALS: &[FsmTerminalExitDesc<St>] = &[FsmTerminalExitDesc {
        state: St::B,
        exit: FsmTerminalExit::ManagementReset,
    }];

    const TABLE: FsmTable<St> = FsmTable {
        id: "TestMachine",
        initial: St::A,
        states: ST_DESCS,
        transitions: &[EDGE_AB, EDGE_BA],
        terminal_exits: TERMINALS,
        timeouts: &[],
    };

    struct Ctx {
        allow: Option<bool>,
    }

    impl GuardContext for Ctx {
        fn field(&self, path: &str) -> Option<FsmValue<'_>> {
            match path {
                "lower.allow" => self.allow.map(FsmValue::Bool),
                _ => None,
            }
        }
    }

    #[test]
    fn unguarded_edge_fires_on_matching_event() {
        let step = TABLE.evaluate(
            St::B,
            FsmEvent::TimerExpired {
                timer: "MaxDuration",
            },
            &NoGuards,
        );
        assert_eq!(
            step,
            FsmStep::Advance {
                to: St::A,
                transition: &EDGE_BA,
            }
        );
    }

    #[test]
    fn guarded_edge_fails_closed_on_unsupplied_field() {
        let step = TABLE.evaluate(
            St::A,
            FsmEvent::RegisterWrite {
                register: "lower.req",
            },
            &NoGuards,
        );
        assert_eq!(step, FsmStep::NoMatch);
    }

    #[test]
    fn guarded_edge_fires_only_when_guard_satisfied() {
        let ctx_ok = Ctx { allow: Some(false) };
        let step = TABLE.evaluate(
            St::A,
            FsmEvent::RegisterWrite {
                register: "lower.req",
            },
            &ctx_ok,
        );
        assert!(matches!(step, FsmStep::Advance { to: St::B, .. }));

        let ctx_bad = Ctx { allow: Some(true) };
        let step = TABLE.evaluate(
            St::A,
            FsmEvent::RegisterWrite {
                register: "lower.req",
            },
            &ctx_bad,
        );
        assert_eq!(step, FsmStep::NoMatch);
    }

    #[test]
    fn named_trigger_requires_name_equality() {
        // A register write to a different register does not fire the edge.
        let step = TABLE.evaluate(
            St::A,
            FsmEvent::RegisterWrite {
                register: "other.reg",
            },
            &Ctx { allow: Some(false) },
        );
        assert_eq!(step, FsmStep::NoMatch);
        // A timer with a different name does not fire either.
        let step = TABLE.evaluate(St::B, FsmEvent::TimerExpired { timer: "Other" }, &NoGuards);
        assert_eq!(step, FsmStep::NoMatch);
    }

    #[test]
    fn wrong_event_kind_never_fires() {
        let step = TABLE.evaluate(St::A, FsmEvent::Internal, &Ctx { allow: Some(false) });
        assert_eq!(step, FsmStep::NoMatch);
    }

    #[test]
    fn state_queries() {
        assert_eq!(TABLE.initial, St::A);
        assert!(TABLE.is_terminal(St::B));
        assert_eq!(
            TABLE.terminal_exit(St::B),
            Some(FsmTerminalExit::ManagementReset)
        );
        assert!(!TABLE.is_terminal(St::A));
        assert_eq!(
            TABLE.state_desc(St::A).unwrap().class,
            Some(FsmStateClass::Stable)
        );
        assert_eq!(TABLE.edges_from(St::A).count(), 1);
    }

    #[test]
    fn plan_path_returns_shortest_path() {
        let path = plan_path(&TABLE, St::A, St::B).unwrap();
        assert_eq!(path, vec![&EDGE_AB]);
        let path = plan_path(&TABLE, St::B, St::A).unwrap();
        assert_eq!(path, vec![&EDGE_BA]);
        // Already at the target: no transitions.
        assert_eq!(
            plan_path(&TABLE, St::A, St::A).unwrap(),
            Vec::<&FsmTransition<St>>::new()
        );
    }

    #[test]
    fn plan_path_unreachable_is_typed() {
        // Table without any edge back from B: unreachable.
        const DEAD: FsmTable<St> = FsmTable {
            id: "Dead",
            initial: St::A,
            states: ST_DESCS,
            transitions: &[EDGE_AB],
            terminal_exits: TERMINALS,
            timeouts: &[],
        };
        let err = plan_path(&DEAD, St::B, St::A).unwrap_err();
        assert!(matches!(
            err,
            FsmPathError::Unreachable { table: "Dead", .. }
        ));
        assert_eq!(err.to_string(), "Dead: no path from B to A");
    }

    #[test]
    fn symbolic_guard_needs_exact_name() {
        const SYM: FsmGuard = FsmGuard::Equals {
            field: "lower.state",
            value: FsmScalar::Symbol("ModulePwrUp"),
        };
        struct SymCtx(Option<&'static str>);
        impl GuardContext for SymCtx {
            fn field(&self, path: &str) -> Option<FsmValue<'_>> {
                match path {
                    "lower.state" => self.0.map(FsmValue::Name),
                    _ => None,
                }
            }
        }
        assert!(guard_satisfied(&SYM, &SymCtx(Some("ModulePwrUp"))));
        assert!(!guard_satisfied(&SYM, &SymCtx(Some("ModuleReady"))));
        assert!(!guard_satisfied(&SYM, &SymCtx(None)));
    }
}
