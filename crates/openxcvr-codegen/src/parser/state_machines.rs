//! Parser for `openxcvr.state_machine.v1` and
//! `openxcvr.state_machine.v2` schemas, with fail-loud revision validation.
//!
//! Mirrors `parser::pages`: namespace allow-list, structured error type, and
//! per-`rev` validation at parse time. Unlike the earlier scaffold, the raw
//! input types deny unknown keys and the parser validates the graph, so an
//! input the IR cannot represent is an error rather than being dropped or
//! defaulted. Keys that only v2 defines are refused in a v1 file
//! ([`ParseStateMachineError::RequiresV2`]). See
//! [`crate::ir::state_machines`] for the format.

use crate::ir::pages::{AliasedToEntry, RenamedFromEntry};
use crate::ir::state_machines::{
    Cardinality, Guard, HostPolicy, HostPolicyKey, HostPolicyValue, RevRange, Scalar, Scope,
    ScopeKind, SideEffect, State, StateChangeOp, StateClass, StateEncoding, StateMachineFormat,
    StateMachineSchema, StateTimeout, TerminalExit, TerminalState, TimeoutSource, TimerDuration,
    Transition, TransitionChange, Trigger, TriggerKind,
};
use crate::ir::version::Rev;
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use serde_norway::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// Recognised state-machine schema namespace strings. v2 is the extended form.
pub(crate) const ALLOWED_STATE_MACHINE_SCHEMAS: &[&str] =
    &["openxcvr.state_machine.v1", "openxcvr.state_machine.v2"];

#[derive(Debug, thiserror::Error)]
pub enum ParseStateMachineError {
    #[error("YAML parse error: {source}")]
    Yaml {
        #[from]
        source: serde_norway::Error,
    },
    // keep in sync with ALLOWED_STATE_MACHINE_SCHEMAS
    #[error("unsupported state-machine schema namespace: {namespace}; allowed: openxcvr.state_machine.v1, openxcvr.state_machine.v2")]
    UnsupportedNamespace { namespace: String },
    /// A `since`, `until`, `changed_in[].rev`, `renamed_from.rev`, or
    /// `aliased_to[].rev` string on a transition is not a valid CMIS revision.
    /// mandates fail-loud here, exactly as `PageField` rev validation
    /// does — a typo'd revision must never silently widen a transition's scope.
    #[error("transition {transition}: invalid rev string {value:?} in {key}")]
    InvalidRevString {
        transition: String,
        key: &'static str,
        value: String,
    },
    /// A revision string on a state or a state timeout is not a valid CMIS
    /// revision.
    #[error("state {state}: invalid rev string {value:?} in {key}")]
    InvalidStateRevString {
        state: String,
        key: &'static str,
        value: String,
    },
    /// A key only `openxcvr.state_machine.v2` defines appears in a v1 file.
    #[error("{context}: `{key}` requires schema openxcvr.state_machine.v2")]
    RequiresV2 { context: String, key: &'static str },
    /// The schema is well-formed YAML but violates the format.
    #[error("{context}: {message}")]
    Invalid { context: String, message: String },
}

type PResult<T> = std::result::Result<T, ParseStateMachineError>;

fn invalid<T>(context: impl Into<String>, message: impl Into<String>) -> PResult<T> {
    Err(ParseStateMachineError::Invalid {
        context: context.into(),
        message: message.into(),
    })
}

/// Deserialize a key that is present (even as `null`) to `Some(value)`, so
/// `key: null` is distinguishable from an absent key.
fn present_value<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

// ── raw input types (deny unknown keys) ────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSchema {
    /// Checked from the untyped value before the strict decode.
    #[allow(dead_code)]
    schema: String,
    kind: String,
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    spec: Option<Value>,
    #[serde(default)]
    provenance: Option<Value>,
    #[serde(default)]
    state_enum: Option<String>,
    #[serde(default)]
    parent_machine: Option<String>,
    #[serde(default)]
    lifecycle_dependency: Option<String>,
    #[serde(default)]
    scope: Option<RawScope>,
    initial_state: String,
    states: Vec<RawState>,
    #[serde(default)]
    terminal_states: Option<Vec<RawTerminalState>>,
    #[serde(default)]
    host_policy: Option<RawHostPolicy>,
    #[serde(default)]
    transitions: Vec<RawTransition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScope {
    kind: String,
    #[serde(default)]
    cardinality: Option<u32>,
    #[serde(default)]
    cardinality_field: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawState {
    name: String,
    #[serde(default)]
    enum_name: Option<String>,
    #[serde(default)]
    enum_value: Option<u64>,
    #[serde(default)]
    is_initial: Option<bool>,
    #[serde(default)]
    is_terminal: Option<bool>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    invariants: Vec<String>,
    /// `null`, one timeout mapping, or (v2) a list of them.
    #[serde(default)]
    timeout: Option<Value>,
    // v2 only
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    reported: Option<bool>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    provenance: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTimeout {
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    duration_ms_field: Option<String>,
    #[serde(default)]
    on_expiry: Option<String>,
    // v2 only
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    timer: Option<String>,
    #[serde(default)]
    policy: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    provenance: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTerminalState {
    state: String,
    exit: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHostPolicy {
    #[serde(default)]
    poll_cadence: Option<RawHostPolicyValue>,
    #[serde(default)]
    readiness_backstop: Option<RawHostPolicyValue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHostPolicyValue {
    value_ms: u64,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    provenance: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTransition {
    #[serde(default)]
    id: Option<String>,
    from: String,
    to: String,
    trigger: RawTrigger,
    /// `null` or a list of § 6 predicates.
    #[serde(default)]
    guard: Option<Vec<Value>>,
    #[serde(default)]
    side_effects: Option<Vec<RawSideEffect>>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    provenance: Option<Value>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    changed_in: Option<Vec<RawTransitionChange>>,
    #[serde(default)]
    renamed_from: Option<RawRenamedFrom>,
    #[serde(default)]
    aliased_to: Option<Vec<RawAliasedTo>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTransitionChange {
    rev: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    provenance: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRenamedFrom {
    rev: String,
    old_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAliasedTo {
    rev: String,
    new_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTrigger {
    kind: String,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    register: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    value: Option<Value>,
    #[serde(default)]
    signal: Option<String>,
    #[serde(default)]
    timer: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    duration_ms_field: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSideEffect {
    kind: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    value: Option<Value>,
    #[serde(default)]
    event_id: Option<String>,
    #[serde(default)]
    timer: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    duration_ms_field: Option<String>,
    #[serde(default)]
    machine: Option<String>,
}

// ── helpers ────────────────────────────────────────────────────────────────

fn rev_range(
    since: Option<&str>,
    until: Option<&str>,
    bad: impl Fn(&'static str, &str) -> ParseStateMachineError,
    context: &str,
) -> PResult<RevRange> {
    let parse = |v: Option<&str>, key: &'static str| -> PResult<Option<Rev>> {
        v.map(|s| Rev::parse(s).map_err(|_| bad(key, s)))
            .transpose()
    };
    let range = RevRange {
        since: parse(since, "since")?,
        until: parse(until, "until")?,
    };
    if range.revs().is_empty() {
        return invalid(
            context,
            format!(
                "since {since:?} / until {until:?} covers no supported revision (until is exclusive)"
            ),
        );
    }
    Ok(range)
}

fn is_subset(inner: &RevRange, outer: &RevRange) -> bool {
    inner.revs().is_subset(&outer.revs())
}

fn scalar(value: &Value, context: &str, key: &str) -> PResult<Scalar> {
    match value {
        Value::Bool(b) => Ok(Scalar::Bool(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Ok(Scalar::Int(i)),
            None => invalid(context, format!("{key}: {n} is not an integer")),
        },
        Value::String(s) => Ok(Scalar::Symbol(s.clone())),
        other => invalid(
            context,
            format!("{key}: expected a bool, integer or name, got {other:?}"),
        ),
    }
}

fn duration(
    ms: Option<u64>,
    field: Option<String>,
    context: &str,
) -> PResult<Option<TimerDuration>> {
    match (ms, field) {
        (Some(_), Some(_)) => invalid(
            context,
            "duration_ms and duration_ms_field are mutually exclusive",
        ),
        (Some(ms), None) => Ok(Some(TimerDuration::FixedMs(ms))),
        (None, Some(f)) => Ok(Some(TimerDuration::AdvertisedField(f))),
        (None, None) => Ok(None),
    }
}

/// Reject keys that are present but meaningless for the variant being built.
fn forbid(context: &str, what: &str, present: &[(&str, bool)]) -> PResult<()> {
    let extra: Vec<&str> = present
        .iter()
        .filter(|(_, p)| *p)
        .map(|(k, _)| *k)
        .collect();
    if extra.is_empty() {
        Ok(())
    } else {
        invalid(
            context,
            format!("{what} does not take {}", extra.join(", ")),
        )
    }
}

fn required(value: Option<String>, context: &str, what: &str, key: &str) -> PResult<String> {
    match value {
        Some(v) => Ok(v),
        None => invalid(context, format!("{what} requires `{key}`")),
    }
}

fn non_null(value: Option<Value>) -> Option<Value> {
    value.filter(|v| !v.is_null())
}

// ── scope, states, timeouts ────────────────────────────────────────────────

fn convert_scope(raw: RawScope) -> PResult<Scope> {
    let kind = match raw.kind.as_str() {
        "per_module" => ScopeKind::PerModule,
        "per_lane" => ScopeKind::PerLane,
        "per_data_path" => ScopeKind::PerDataPath,
        "per_network_path" => ScopeKind::PerNetworkPath,
        other => return invalid("scope", format!("unknown scope kind {other:?}")),
    };
    let cardinality = match (raw.cardinality, raw.cardinality_field) {
        (Some(n), None) => Cardinality::Fixed(n),
        (None, Some(f)) => Cardinality::Field(f),
        _ => {
            return invalid(
                "scope",
                "exactly one of cardinality / cardinality_field is required",
            )
        }
    };
    Ok(Scope { kind, cardinality })
}

fn convert_class(value: &str, context: &str) -> PResult<StateClass> {
    match value {
        "stable" => Ok(StateClass::Stable),
        "transient" => Ok(StateClass::Transient),
        "fault" => Ok(StateClass::Fault),
        "terminal" => Ok(StateClass::Terminal),
        other => invalid(
            context,
            format!("unknown class {other:?}; expected stable, transient, fault or terminal"),
        ),
    }
}

fn convert_timeouts(
    format: StateMachineFormat,
    state: &str,
    state_revs: &RevRange,
    value: Option<Value>,
) -> PResult<Vec<StateTimeout>> {
    let context = format!("state {state}: timeout");
    let raws: Vec<Value> = match value {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Sequence(items)) => {
            if format == StateMachineFormat::V1 {
                return Err(ParseStateMachineError::RequiresV2 {
                    context,
                    key: "timeout list",
                });
            }
            items
        }
        Some(mapping @ Value::Mapping(_)) => vec![mapping],
        Some(other) => {
            return invalid(
                context,
                format!("expected null, a mapping or a list, got {other:?}"),
            )
        }
    };
    let mut out = Vec::with_capacity(raws.len());
    for raw in raws {
        let raw: RawTimeout = serde_norway::from_value(raw)?;
        out.push(convert_timeout(format, state, state_revs, raw)?);
    }
    Ok(out)
}

fn convert_timeout(
    format: StateMachineFormat,
    state: &str,
    state_revs: &RevRange,
    raw: RawTimeout,
) -> PResult<StateTimeout> {
    let context = format!("state {state}: timeout");
    if format == StateMachineFormat::V1 {
        for (key, present) in [
            ("source", raw.source.is_some()),
            ("timer", raw.timer.is_some()),
            ("policy", raw.policy.is_some()),
            ("since", raw.since.is_some()),
            ("until", raw.until.is_some()),
            ("provenance", raw.provenance.is_some()),
        ] {
            if present {
                return Err(ParseStateMachineError::RequiresV2 { context, key });
            }
        }
        let Some(d) = duration(raw.duration_ms, raw.duration_ms_field, &context)? else {
            return invalid(context, "requires duration_ms or duration_ms_field");
        };
        return Ok(StateTimeout {
            source: TimeoutSource::Unattributed(d),
            on_expiry: raw.on_expiry,
            revs: *state_revs,
            provenance: None,
        });
    }

    let revs = rev_range(
        raw.since.as_deref(),
        raw.until.as_deref(),
        |key, value| ParseStateMachineError::InvalidStateRevString {
            state: state.to_string(),
            key: if key == "since" {
                "timeout.since"
            } else {
                "timeout.until"
            },
            value: value.to_string(),
        },
        &context,
    )?;
    if !is_subset(&revs, state_revs) {
        return invalid(
            context,
            "timeout revisions extend beyond the state's own since/until",
        );
    }
    let source = match raw.source.as_deref() {
        Some("standard") => {
            forbid(
                &context,
                "a standard timer",
                &[("policy", raw.policy.is_some())],
            )?;
            let timer = required(raw.timer, &context, "a standard timer", "timer")?;
            let Some(d) = duration(raw.duration_ms, raw.duration_ms_field, &context)? else {
                return invalid(
                    context,
                    "a standard timer requires duration_ms or duration_ms_field",
                );
            };
            TimeoutSource::Standard { timer, duration: d }
        }
        Some("host_policy") => {
            forbid(
                &context,
                "a host_policy timeout",
                &[
                    ("timer", raw.timer.is_some()),
                    ("duration_ms", raw.duration_ms.is_some()),
                    ("duration_ms_field", raw.duration_ms_field.is_some()),
                    ("on_expiry", raw.on_expiry.is_some()),
                ],
            )?;
            match raw.policy.as_deref() {
                Some("readiness_backstop") => {
                    TimeoutSource::HostPolicy(HostPolicyKey::ReadinessBackstop)
                }
                Some(other) => {
                    return invalid(
                        context,
                        format!("host_policy timeout policy {other:?} is not a deadline; expected readiness_backstop"),
                    )
                }
                None => return invalid(context, "a host_policy timeout requires `policy`"),
            }
        }
        Some(other) => {
            return invalid(
                context,
                format!("unknown timeout source {other:?}; expected standard or host_policy"),
            )
        }
        None => return invalid(context, "v2 timeouts require `source`"),
    };
    Ok(StateTimeout {
        source,
        on_expiry: raw.on_expiry,
        revs,
        provenance: non_null(raw.provenance),
    })
}

fn convert_state(format: StateMachineFormat, raw: RawState) -> PResult<(State, bool, bool)> {
    let context = format!("state {}", raw.name);
    if format == StateMachineFormat::V1 {
        for (key, present) in [
            ("class", raw.class.is_some()),
            ("reported", raw.reported.is_some()),
            ("since", raw.since.is_some()),
            ("until", raw.until.is_some()),
            ("provenance", raw.provenance.is_some()),
        ] {
            if present {
                return Err(ParseStateMachineError::RequiresV2 { context, key });
            }
        }
    }
    let name = raw.name.clone();
    let revs = rev_range(
        raw.since.as_deref(),
        raw.until.as_deref(),
        |key, value| ParseStateMachineError::InvalidStateRevString {
            state: name.clone(),
            key,
            value: value.to_string(),
        },
        &context,
    )?;
    let encoding = match (raw.reported, raw.enum_value) {
        (Some(false), None) => {
            if raw.enum_name.is_some() {
                return invalid(context, "an unreported state has no enum_name");
            }
            StateEncoding::Unreported
        }
        (Some(false), Some(_)) => return invalid(context, "an unreported state has no enum_value"),
        (_, Some(value)) => StateEncoding::Reported {
            enum_name: raw.enum_name,
            value,
        },
        (_, None) => {
            return invalid(
                context,
                if format == StateMachineFormat::V1 {
                    "requires enum_value"
                } else {
                    "requires enum_value, or `reported: false` for a state the module cannot report"
                },
            )
        }
    };
    let class = match (&raw.class, format) {
        (Some(c), _) => Some(convert_class(c, &context)?),
        (None, StateMachineFormat::V1) => None,
        (None, StateMachineFormat::V2) => return invalid(context, "v2 states require `class`"),
    };
    let timeouts = convert_timeouts(format, &raw.name, &revs, raw.timeout)?;
    if !timeouts.is_empty() && class.is_some() && class != Some(StateClass::Transient) {
        return invalid(context, "only a transient state carries a timeout");
    }
    for rev in revs.revs() {
        let count = |pred: fn(&TimeoutSource) -> bool| {
            timeouts
                .iter()
                .filter(|t| t.revs.contains(rev) && pred(&t.source))
                .count()
        };
        if count(|s| !matches!(s, TimeoutSource::HostPolicy(_))) > 1
            || count(|s| matches!(s, TimeoutSource::HostPolicy(_))) > 1
        {
            return invalid(
                context,
                format!(
                    "more than one timeout of the same source applies in revision {}",
                    rev.as_str()
                ),
            );
        }
    }
    let state = State {
        name: raw.name,
        encoding,
        class,
        description: raw.description,
        invariants: raw.invariants,
        timeouts,
        revs,
        provenance: non_null(raw.provenance),
    };
    Ok((
        state,
        raw.is_initial.unwrap_or(false),
        raw.is_terminal.unwrap_or(false),
    ))
}

fn convert_host_policy(raw: RawHostPolicy) -> PResult<HostPolicy> {
    let value = |v: Option<RawHostPolicyValue>, key: &str| -> PResult<Option<HostPolicyValue>> {
        let Some(v) = v else { return Ok(None) };
        let context = format!("host_policy.{key}");
        if v.value_ms == 0 {
            return invalid(context, "value_ms must be positive");
        }
        let Some(provenance) = non_null(v.provenance) else {
            return invalid(context, "requires provenance");
        };
        Ok(Some(HostPolicyValue {
            value_ms: v.value_ms,
            description: v.description,
            provenance,
        }))
    };
    let policy = HostPolicy {
        poll_cadence: value(raw.poll_cadence, "poll_cadence")?,
        readiness_backstop: value(raw.readiness_backstop, "readiness_backstop")?,
    };
    if policy.poll_cadence.is_none() && policy.readiness_backstop.is_none() {
        return invalid("host_policy", "an empty host_policy block records nothing");
    }
    Ok(policy)
}

// ── transitions ────────────────────────────────────────────────────────────

fn convert_trigger(raw: RawTrigger, context: &str) -> PResult<Trigger> {
    let context = format!("{context}: trigger");
    let kind = match raw.kind.as_str() {
        "register_write" => TriggerKind::RegisterWrite,
        "register_read" => TriggerKind::RegisterRead,
        "timer" => TriggerKind::Timer,
        "internal" => TriggerKind::Internal,
        "external_event" => TriggerKind::ExternalEvent,
        other => return invalid(context, format!("unknown trigger kind {other:?}")),
    };
    let what = format!("a {} trigger", raw.kind);
    let has_duration = raw.duration_ms.is_some() || raw.duration_ms_field.is_some();
    let value = match &raw.value {
        None => None,
        Some(v) => Some(scalar(v, &context, "value")?),
    };
    let mut trigger = Trigger {
        kind,
        condition: raw.condition,
        register: None,
        value: None,
        signal: None,
        timer: None,
        duration: None,
    };
    match kind {
        TriggerKind::RegisterWrite | TriggerKind::RegisterRead => {
            forbid(
                &context,
                &what,
                &[
                    ("signal", raw.signal.is_some()),
                    ("timer", raw.timer.is_some()),
                    ("duration", has_duration),
                ],
            )?;
            trigger.register = Some(required(raw.register, &context, &what, "register")?);
            trigger.value = value;
        }
        TriggerKind::Timer => {
            forbid(
                &context,
                &what,
                &[
                    ("register", raw.register.is_some()),
                    ("value", raw.value.is_some()),
                    ("signal", raw.signal.is_some()),
                ],
            )?;
            trigger.timer = raw.timer;
            trigger.duration = duration(raw.duration_ms, raw.duration_ms_field, &context)?;
            if trigger.duration.is_none() {
                return invalid(
                    context,
                    "a timer trigger requires duration_ms or duration_ms_field",
                );
            }
        }
        TriggerKind::Internal => forbid(
            &context,
            &what,
            &[
                ("register", raw.register.is_some()),
                ("value", raw.value.is_some()),
                ("signal", raw.signal.is_some()),
                ("timer", raw.timer.is_some()),
                ("duration", has_duration),
            ],
        )?,
        TriggerKind::ExternalEvent => {
            forbid(
                &context,
                &what,
                &[
                    ("register", raw.register.is_some()),
                    ("value", raw.value.is_some()),
                    ("timer", raw.timer.is_some()),
                    ("duration", has_duration),
                ],
            )?;
            trigger.signal = raw.signal;
        }
    }
    Ok(trigger)
}

/// § 6: `{ field, equals }` or `forall: { field, equals }`. `equals`
/// is the only defined comparator; anything else is refused.
fn convert_guard(value: &Value, context: &str) -> PResult<Guard> {
    let context = format!("{context}: guard");
    let Value::Mapping(map) = value else {
        return invalid(
            context,
            format!("expected a predicate mapping, got {value:?}"),
        );
    };
    let predicate = |map: &serde_norway::Mapping| -> PResult<(String, Scalar)> {
        let keys: BTreeSet<&str> = map.keys().filter_map(|k| k.as_str()).collect();
        if map.len() != 2 || keys != BTreeSet::from(["field", "equals"]) {
            return invalid(
                &context,
                format!(
                    "a predicate is exactly {{ field, equals }}; got keys {:?}",
                    map.keys().collect::<Vec<_>>()
                ),
            );
        }
        let Some(field) = map.get("field").and_then(Value::as_str) else {
            return invalid(&context, "`field` must be a path string");
        };
        let value = scalar(&map["equals"], &context, "equals")?;
        Ok((field.to_string(), value))
    };
    if map.len() == 1 {
        if let Some(inner) = map.get("forall") {
            let Value::Mapping(inner) = inner else {
                return invalid(context, "`forall` takes a predicate mapping");
            };
            let (field, value) = predicate(inner)?;
            return Ok(Guard::ForAll { field, value });
        }
    }
    let (field, value) = predicate(map)?;
    Ok(Guard::Equals { field, value })
}

fn convert_side_effect(raw: RawSideEffect, context: &str) -> PResult<SideEffect> {
    let context = format!("{context}: side_effect {}", raw.kind);
    let what = format!("a {} side effect", raw.kind);
    let has_duration = raw.duration_ms.is_some() || raw.duration_ms_field.is_some();
    let all = [
        ("target", raw.target.is_some()),
        ("operation", raw.operation.is_some()),
        ("value", raw.value.is_some()),
        ("event_id", raw.event_id.is_some()),
        ("timer", raw.timer.is_some()),
        ("duration", has_duration),
        ("machine", raw.machine.is_some()),
    ];
    let only = |allowed: &[&str]| -> PResult<()> {
        let rest: Vec<(&str, bool)> = all
            .iter()
            .copied()
            .filter(|(k, _)| !allowed.contains(k))
            .collect();
        forbid(&context, &what, &rest)
    };
    Ok(match raw.kind.as_str() {
        "state_change" => {
            only(&["target", "operation", "value"])?;
            let target = required(raw.target, &context, &what, "target")?;
            let operation = match raw.operation.as_deref() {
                Some("set") => StateChangeOp::Set,
                Some("clear") => StateChangeOp::Clear,
                Some("assign") => StateChangeOp::Assign,
                Some(other) => return invalid(context, format!("unknown operation {other:?}")),
                None => return invalid(context, "requires `operation`"),
            };
            let value = match (&raw.value, operation) {
                (Some(v), StateChangeOp::Assign) => Some(scalar(v, &context, "value")?),
                (None, StateChangeOp::Assign) => {
                    return invalid(context, "operation assign requires `value`")
                }
                (Some(_), _) => return invalid(context, "only operation assign takes `value`"),
                (None, _) => None,
            };
            SideEffect::StateChange {
                target,
                operation,
                value,
            }
        }
        "log_event" => {
            only(&["event_id"])?;
            SideEffect::LogEvent {
                event_id: required(raw.event_id, &context, &what, "event_id")?,
            }
        }
        "start_timer" => {
            only(&["timer", "duration"])?;
            let timer = required(raw.timer, &context, &what, "timer")?;
            let Some(duration) = duration(raw.duration_ms, raw.duration_ms_field, &context)? else {
                return invalid(context, "requires duration_ms or duration_ms_field");
            };
            SideEffect::StartTimer { timer, duration }
        }
        "cancel_timer" => {
            only(&["timer"])?;
            SideEffect::CancelTimer {
                timer: required(raw.timer, &context, &what, "timer")?,
            }
        }
        "spawn_machine" => {
            only(&["machine"])?;
            SideEffect::SpawnMachine {
                machine: required(raw.machine, &context, &what, "machine")?,
            }
        }
        "terminate_machine" => {
            only(&["machine"])?;
            SideEffect::TerminateMachine {
                machine: required(raw.machine, &context, &what, "machine")?,
            }
        }
        other => return invalid(context, format!("unknown side-effect kind {other:?}")),
    })
}

/// Validate every revision string carried by a single transition. Returns the
/// first malformed revision encountered.
fn validate_transition_revs(t: &RawTransition) -> PResult<()> {
    let label =
        t.id.clone()
            .unwrap_or_else(|| format!("{}->{}", t.from, t.to));
    let check = |value: &str, key: &'static str| -> PResult<()> {
        Rev::parse(value)
            .map(|_| ())
            .map_err(|_| ParseStateMachineError::InvalidRevString {
                transition: label.clone(),
                key,
                value: value.to_string(),
            })
    };
    if let Some(s) = &t.since {
        check(s, "since")?;
    }
    if let Some(s) = &t.until {
        check(s, "until")?;
    }
    for entry in t.changed_in.iter().flatten() {
        check(&entry.rev, "changed_in[].rev")?;
    }
    if let Some(entry) = &t.renamed_from {
        check(&entry.rev, "renamed_from.rev")?;
    }
    for entry in t.aliased_to.iter().flatten() {
        check(&entry.rev, "aliased_to[].rev")?;
    }
    Ok(())
}

fn convert_transition(raw: RawTransition) -> PResult<Transition> {
    validate_transition_revs(&raw)?;
    let label = raw
        .id
        .clone()
        .unwrap_or_else(|| format!("{}->{}", raw.from, raw.to));
    let context = format!("transition {label}");
    let revs = rev_range(
        raw.since.as_deref(),
        raw.until.as_deref(),
        |_, _| unreachable!("transition revs validated above"),
        &context,
    )?;
    let trigger = convert_trigger(raw.trigger, &context)?;
    let guards = raw
        .guard
        .iter()
        .flatten()
        .map(|g| convert_guard(g, &context))
        .collect::<PResult<Vec<_>>>()?;
    let side_effects = raw
        .side_effects
        .into_iter()
        .flatten()
        .map(|s| convert_side_effect(s, &context))
        .collect::<PResult<Vec<_>>>()?;
    let changed_in = raw
        .changed_in
        .into_iter()
        .flatten()
        .map(|c| TransitionChange {
            rev: Rev::parse(&c.rev).expect("validated above"),
            note: c.note,
            provenance: non_null(c.provenance),
        })
        .collect();
    Ok(Transition {
        id: raw.id,
        from: raw.from,
        to: raw.to,
        trigger,
        guards,
        side_effects,
        description: raw.description,
        provenance: non_null(raw.provenance),
        revs,
        changed_in,
        renamed_from: raw.renamed_from.map(|r| RenamedFromEntry {
            rev: r.rev,
            old_name: r.old_name,
        }),
        aliased_to: raw
            .aliased_to
            .into_iter()
            .flatten()
            .map(|a| AliasedToEntry {
                rev: a.rev,
                new_name: a.new_name,
            })
            .collect(),
    })
}

// ── schema ─────────────────────────────────────────────────────────────────

fn convert_terminal(raw: RawTerminalState) -> PResult<TerminalState> {
    let exit = match raw.exit.as_str() {
        "power_cycle" => TerminalExit::PowerCycle,
        "management_reset" => TerminalExit::ManagementReset,
        "software_recovery" => TerminalExit::SoftwareRecovery,
        other => {
            return invalid(
                format!("terminal_states {}", raw.state),
                format!("unknown exit {other:?}; expected power_cycle, management_reset or software_recovery"),
            )
        }
    };
    Ok(TerminalState {
        state: raw.state,
        exit,
    })
}

/// Parse and validate a state-machine schema from a YAML string.
///
/// Rejects unknown schema namespaces, unknown keys, keys v1 does not define
/// in a v1 file, malformed revision strings, and graphs that do not close
/// over their declared states (see [`ParseStateMachineError`]).
pub fn parse_state_machine_schema(yaml: &str) -> PResult<StateMachineSchema> {
    // Check the namespace before the strict decode, so a file in a future
    // namespace is reported as such rather than as an unknown key.
    let value: Value = serde_norway::from_str(yaml)?;
    let namespace = value.get("schema").and_then(Value::as_str).unwrap_or("");
    let format = match namespace {
        "openxcvr.state_machine.v1" => StateMachineFormat::V1,
        "openxcvr.state_machine.v2" => StateMachineFormat::V2,
        other => {
            return Err(ParseStateMachineError::UnsupportedNamespace {
                namespace: other.to_string(),
            })
        }
    };
    debug_assert!(ALLOWED_STATE_MACHINE_SCHEMAS.contains(&format.namespace()));
    let raw: RawSchema = serde_norway::from_value(value)?;
    if raw.kind != "state_machine" {
        return invalid(
            raw.id,
            format!("kind must be state_machine, got {:?}", raw.kind),
        );
    }
    if format == StateMachineFormat::V1 && raw.host_policy.is_some() {
        return Err(ParseStateMachineError::RequiresV2 {
            context: raw.id,
            key: "host_policy",
        });
    }
    let id = raw.id;

    // States.
    if raw.states.is_empty() {
        return invalid(&id, "a state machine declares at least one state");
    }
    let mut states = Vec::with_capacity(raw.states.len());
    let mut initial_flags = Vec::new();
    let mut terminal_flags = BTreeSet::new();
    for raw_state in raw.states {
        let (state, is_initial, is_terminal) = convert_state(format, raw_state)?;
        if states.iter().any(|s: &State| s.name == state.name) {
            return invalid(format!("state {}", state.name), "duplicate state name");
        }
        if is_initial {
            initial_flags.push(state.name.clone());
        }
        if is_terminal {
            terminal_flags.insert(state.name.clone());
        }
        states.push(state);
    }
    let find = |name: &str| states.iter().find(|s| s.name == name);

    // Reported encodings are unique within every revision.
    for rev in Rev::CMIS_ALL {
        let mut seen: Vec<(u64, &str)> = Vec::new();
        for s in states.iter().filter(|s| s.revs.contains(*rev)) {
            if let StateEncoding::Reported { value, .. } = s.encoding {
                if let Some((_, other)) = seen.iter().find(|(v, _)| *v == value) {
                    return invalid(
                        format!("state {}", s.name),
                        format!(
                            "enum_value {value:#x} is also {other} in revision {}",
                            rev.as_str()
                        ),
                    );
                }
                seen.push((value, &s.name));
            }
        }
    }

    // Initial state: exactly one flagged, equal to initial_state, present in
    // every supported revision.
    let initial_context = format!("initial_state {}", raw.initial_state);
    match initial_flags.as_slice() {
        [only] if *only == raw.initial_state => {}
        _ => {
            return invalid(
                initial_context,
                format!("exactly one state must have is_initial: true and equal initial_state; flagged: {initial_flags:?}"),
            )
        }
    }
    if find(&raw.initial_state).map(|s| s.revs.revs()) != Some(RevRange::ALL.revs()) {
        return invalid(
            initial_context,
            "the initial state must exist in every supported revision",
        );
    }

    // Terminal states: listed exactly where is_terminal is set, and only
    // fault or terminal classes halt the machine.
    let mut terminal_states = Vec::new();
    for raw_terminal in raw.terminal_states.into_iter().flatten() {
        let t = convert_terminal(raw_terminal)?;
        let context = format!("terminal_states {}", t.state);
        let Some(state) = find(&t.state) else {
            return invalid(context, "names no declared state");
        };
        if terminal_states
            .iter()
            .any(|x: &TerminalState| x.state == t.state)
        {
            return invalid(context, "listed twice");
        }
        if let Some(class) = state.class {
            if !matches!(class, StateClass::Fault | StateClass::Terminal) {
                return invalid(context, format!("a {class:?} state cannot be terminal"));
            }
        }
        terminal_states.push(t);
    }
    for s in &states {
        let listed = terminal_states.iter().any(|t| t.state == s.name);
        if listed != terminal_flags.contains(&s.name) {
            return invalid(
                format!("state {}", s.name),
                "is_terminal must be true exactly when the state is listed in terminal_states",
            );
        }
        if s.class == Some(StateClass::Terminal) && !listed {
            return invalid(
                format!("state {}", s.name),
                "a terminal-class state must be listed in terminal_states",
            );
        }
    }

    // Host policy and timeout references.
    let host_policy = raw.host_policy.map(convert_host_policy).transpose()?;
    for s in &states {
        for t in &s.timeouts {
            let context = format!("state {}: timeout", s.name);
            if let TimeoutSource::HostPolicy(key) = t.source {
                if host_policy.as_ref().and_then(|p| p.get(key)).is_none() {
                    return invalid(
                        context,
                        format!("references host_policy {key:?}, which the schema does not define"),
                    );
                }
            }
            if let Some(target) = &t.on_expiry {
                match find(target) {
                    None => {
                        return invalid(
                            context,
                            format!("on_expiry {target} names no declared state"),
                        )
                    }
                    Some(to) if !is_subset(&t.revs, &to.revs) => {
                        return invalid(
                            context,
                            format!(
                            "on_expiry {target} does not exist in every revision of the timeout"
                        ),
                        )
                    }
                    Some(_) => {}
                }
            }
        }
    }

    // Transitions: closed graph, revisions within both endpoints, unique ids.
    let mut transitions = Vec::with_capacity(raw.transitions.len());
    for raw_transition in raw.transitions {
        let t = convert_transition(raw_transition)?;
        let context = format!("transition {}", t.label());
        if let Some(tid) = &t.id {
            if transitions
                .iter()
                .any(|x: &Transition| x.id.as_ref() == Some(tid))
            {
                return invalid(context, "duplicate transition id");
            }
        }
        for (end, name) in [("from", &t.from), ("to", &t.to)] {
            match find(name) {
                None => return invalid(&context, format!("{end} {name} names no declared state")),
                Some(s) if !is_subset(&t.revs, &s.revs) => {
                    return invalid(
                        &context,
                        format!("{end} {name} does not exist in every revision of the transition"),
                    )
                }
                Some(_) => {}
            }
        }
        if let Some(change) = t.changed_in.iter().find(|c| !t.revs.contains(c.rev)) {
            return invalid(
                context,
                format!(
                    "changed_in rev {} is outside the transition's since/until",
                    change.rev.as_str()
                ),
            );
        }
        transitions.push(t);
    }

    Ok(StateMachineSchema {
        format,
        id,
        title: raw.title,
        spec: non_null(raw.spec),
        provenance: non_null(raw.provenance),
        state_enum: raw.state_enum,
        parent_machine: raw.parent_machine,
        lifecycle_dependency: raw.lifecycle_dependency,
        scope: raw.scope.map(convert_scope).transpose()?,
        initial_state: raw.initial_state,
        states,
        terminal_states,
        host_policy,
        transitions,
    })
}

/// Read and parse a state-machine schema from a file on disk.
///
/// State machines have no `field_includes` merge step (unlike pages), so this
/// is a straight read + parse with path context on error.
pub fn load_state_machine_schema(path: &Path) -> Result<StateMachineSchema> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_state_machine_schema(&content)
        .map_err(anyhow::Error::from)
        .with_context(|| {
            format!(
                "failed to parse state machine schema from {}",
                path.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "schema: openxcvr.state_machine.v1\nkind: state_machine\nid: test-fsm\ninitial_state: A\nstates:\n  - { name: A, enum_value: 1, is_initial: true }\n  - { name: B, enum_value: 2 }\n  - { name: C, enum_value: 3 }\n";

    /// B5 Step 3a: the transition version vocabulary parses `since`/`until`
    /// off real transition edges.
    #[test]
    fn parses_transition_since_until() {
        let yaml = format!(
            "{HEAD}transitions:\n  - {{ from: A, to: B, trigger: {{ kind: timer, duration_ms: 100 }}, since: \"4.0\" }}\n  - {{ from: B, to: C, trigger: {{ kind: timer, duration_ms: 100 }}, until: \"5.0\" }}\n"
        );
        let parsed = parse_state_machine_schema(&yaml).expect("parse");
        assert_eq!(parsed.transitions[0].revs.since, Some(Rev::V4_0));
        assert_eq!(parsed.transitions[1].revs.until, Some(Rev::V5_0));
    }

    /// fail-loud: a malformed `since` revision is rejected at parse
    /// time; the label falls back to `from->to`.
    #[test]
    fn rejects_invalid_since_rev_string() {
        let yaml = format!(
            "{HEAD}transitions:\n  - {{ from: A, to: B, trigger: {{ kind: internal }}, since: \"v4.0\" }}\n"
        );
        match parse_state_machine_schema(&yaml).expect_err("must reject bad rev") {
            ParseStateMachineError::InvalidRevString {
                transition,
                key,
                value,
            } => {
                assert_eq!(
                    (transition.as_str(), key, value.as_str()),
                    ("A->B", "since", "v4.0")
                );
            }
            other => panic!("expected InvalidRevString, got {other:?}"),
        }
    }

    /// The transition `id:` is preferred over `from->to` in the error label.
    #[test]
    fn invalid_rev_error_uses_transition_id_when_present() {
        let yaml = format!(
            "{HEAD}transitions:\n  - {{ id: a_to_b, from: A, to: B, trigger: {{ kind: internal }}, until: \"5.x\" }}\n"
        );
        match parse_state_machine_schema(&yaml).expect_err("must reject bad rev") {
            ParseStateMachineError::InvalidRevString { transition, .. } => {
                assert_eq!(transition, "a_to_b");
            }
            other => panic!("expected InvalidRevString, got {other:?}"),
        }
    }

    /// v1 and v2 are both accepted; anything else is refused by namespace.
    #[test]
    fn namespace_allow_list() {
        assert!(parse_state_machine_schema(&format!("{HEAD}transitions: []\n")).is_ok());
        for ns in ALLOWED_STATE_MACHINE_SCHEMAS {
            assert!(ns.starts_with("openxcvr.state_machine.v"));
        }
        let v9 = HEAD.replace("state_machine.v1", "state_machine.v9");
        match parse_state_machine_schema(&v9).expect_err("must reject v9") {
            ParseStateMachineError::UnsupportedNamespace { namespace } => {
                assert_eq!(namespace, "openxcvr.state_machine.v9");
            }
            other => panic!("expected UnsupportedNamespace, got {other:?}"),
        }
    }

    /// An explicit `since: "3.0"` on the initial state still covers every
    /// supported revision.
    #[test]
    fn explicit_full_range_initial_state_is_accepted() {
        let yaml = "schema: openxcvr.state_machine.v2\nkind: state_machine\nid: t\ninitial_state: A\nstates:\n  - { name: A, enum_value: 1, class: stable, is_initial: true, since: \"3.0\" }\n";
        parse_state_machine_schema(yaml).expect("explicit full range");
    }
}
