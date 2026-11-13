//! Task 22: the extended state-machine schema format.
//!
//! Synthetic YAML exercises `openxcvr.state_machine.v2` (state classes,
//! timeout sources, host policy, terminal states, guards as data, per-state
//! and per-transition since/until) and the fail-closed refusals; the last
//! section loads every existing file under `schemas/cmis/state-machines/`.

use openxcvr_codegen::ir::state_machines::{
    Cardinality, Guard, HostPolicyKey, RevRange, Scalar, ScopeKind, SideEffect, StateChangeOp,
    StateClass, StateEncoding, StateMachineFormat, StateMachineSchema, TerminalExit, TimeoutSource,
    TimerDuration, TriggerKind,
};
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::state_machines::{
    load_state_machine_schema, parse_state_machine_schema, ParseStateMachineError,
};
use std::path::{Path, PathBuf};

fn parse(yaml: &str) -> StateMachineSchema {
    parse_state_machine_schema(yaml).unwrap_or_else(|e| panic!("parse failed: {e}"))
}

fn reject(yaml: &str) -> ParseStateMachineError {
    match parse_state_machine_schema(yaml) {
        Ok(s) => panic!("expected a parse error, got schema {}", s.id),
        Err(e) => e,
    }
}

/// Assert the error is `Invalid` and its message contains `needle`.
fn assert_invalid(err: ParseStateMachineError, needle: &str) {
    match &err {
        ParseStateMachineError::Invalid { message, .. } if message.contains(needle) => {}
        other => panic!("expected Invalid containing {needle:?}, got {other:?} ({other})"),
    }
}

/// Assert a serde-level refusal (unknown key, wrong type) mentioning `needle`.
fn assert_yaml_error(err: ParseStateMachineError, needle: &str) {
    match &err {
        ParseStateMachineError::Yaml { source } if source.to_string().contains(needle) => {}
        other => panic!("expected a YAML error mentioning {needle:?}, got {other:?} ({other})"),
    }
}

/// A complete, valid v2 module state machine. Tests mutate it by string
/// replacement so each refusal differs from a known-good schema by one edit.
const V2: &str = r#"
schema: openxcvr.state_machine.v2
kind: state_machine
id: SyntheticMsm
title: Synthetic module state machine
spec: { id: cmis, revision: "5.3" }
state_enum: CmisModuleState
scope: { kind: per_module, cardinality: 1 }
initial_state: Resetting
host_policy:
  poll_cadence:
    value_ms: 50
    description: Host poll interval for the state register.
    provenance: { source: "the kernel module plugins/cmis/power.c:65" }
  readiness_backstop:
    value_ms: 10000
    provenance: { source: "the kernel module plugins/cmis/power.c:68" }
states:
  - name: Resetting
    class: transient
    reported: false
    is_initial: true
  - name: MgmtInit
    class: transient
    reported: false
    timeout:
      - source: standard
        timer: tMgmtInit
        duration_ms: 2000
        provenance: { table: "CMIS 5.3 Table 10-2" }
      - source: host_policy
        policy: readiness_backstop
  - name: ModuleLowPwr
    enum_name: module_low_pwr
    enum_value: 0x1
    class: stable
  - name: ModulePwrUp
    enum_name: module_pwr_up
    enum_value: 0x2
    class: transient
    timeout:
      - source: standard
        timer: MaxDurationModulePwrUp
        duration_ms_field: page_01h_advertising.max_duration_module_pwr_up
        on_expiry: ModuleFault
      - source: host_policy
        policy: readiness_backstop
  - name: ModuleReady
    enum_name: module_ready
    enum_value: 0x3
    class: stable
  - name: ModuleFault
    enum_name: module_fault
    enum_value: 0x5
    class: fault
    is_terminal: true
  - name: LegacyOnly
    enum_name: legacy_only
    enum_value: 0x6
    class: stable
    until: "4.0"
terminal_states:
  - { state: ModuleFault, exit: management_reset }
transitions:
  - id: reset_to_mgmt_init
    from: Resetting
    to: MgmtInit
    trigger: { kind: internal, condition: "Reset complete." }
  - id: mgmt_init_to_low_pwr
    from: MgmtInit
    to: ModuleLowPwr
    trigger: { kind: internal }
  - id: low_pwr_to_pwr_up
    from: ModuleLowPwr
    to: ModulePwrUp
    trigger:
      kind: register_write
      register: lower_memory.low_pwr_request_sw
      value: 0
    guard:
      - { field: lower_memory.low_pwr_allow_request_hw, equals: false }
      - forall: { field: page_11h_lane_status.dp_state_host_lane, equals: DPDeactivated }
    side_effects:
      - { kind: state_change, target: lower_memory.module_state, operation: assign, value: ModulePwrUp }
      - { kind: start_timer, timer: MaxDurationModulePwrUp, duration_ms_field: page_01h_advertising.max_duration_module_pwr_up }
      - { kind: log_event, event_id: msm_power_up_started }
    since: "4.0"
  - id: pwr_up_to_ready
    from: ModulePwrUp
    to: ModuleReady
    trigger: { kind: internal }
    guard: null
    side_effects:
      - { kind: cancel_timer, timer: MaxDurationModulePwrUp }
      - { kind: spawn_machine, machine: CmisDataPathStateMachine }
  - id: legacy_edge
    from: ModuleLowPwr
    to: LegacyOnly
    trigger: { kind: external_event, signal: LowPwrRequestHW }
    until: "4.0"
  - id: fault_from_ready
    from: ModuleReady
    to: ModuleFault
    trigger: { kind: internal }
    side_effects:
      - { kind: terminate_machine, machine: CmisDataPathStateMachine }
      - { kind: state_change, target: lower_memory.fault_flag, operation: set }
"#;

/// `V2` with `from` replaced by `to` exactly once (panics if `from` is absent
/// or ambiguous, so a refusal test can never pass by editing nothing).
fn v2_with(from: &str, to: &str) -> String {
    assert_eq!(
        V2.matches(from).count(),
        1,
        "edit anchor {from:?} must be unique"
    );
    V2.replacen(from, to, 1)
}

fn schema_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis/state-machines")
}

// ── v2 is accepted and keeps everything ────────────────────────────────────

#[test]
fn accepts_v2_namespace_alongside_v1() {
    assert_eq!(parse(V2).format, StateMachineFormat::V2);
    let v1 = parse(
        r#"
schema: openxcvr.state_machine.v1
kind: state_machine
id: Tiny
initial_state: A
states:
  - { name: A, enum_value: 1, is_initial: true }
transitions: []
"#,
    );
    assert_eq!(v1.format, StateMachineFormat::V1);
}

#[test]
fn keeps_state_classes_and_reporting() {
    let s = parse(V2);
    let class = |n: &str| s.state(n).expect(n).class;
    assert_eq!(class("Resetting"), Some(StateClass::Transient));
    assert_eq!(class("ModuleLowPwr"), Some(StateClass::Stable));
    assert_eq!(class("ModulePwrUp"), Some(StateClass::Transient));
    assert_eq!(class("ModuleFault"), Some(StateClass::Fault));
    assert_eq!(
        s.state("MgmtInit").unwrap().encoding,
        StateEncoding::Unreported
    );
    assert_eq!(
        s.state("ModuleFault").unwrap().encoding,
        StateEncoding::Reported {
            enum_name: Some("module_fault".into()),
            value: 5
        }
    );
    assert_eq!(s.initial_state, "Resetting");
    assert_eq!(s.states.len(), 7, "no state dropped");
}

#[test]
fn keeps_terminal_states() {
    let s = parse(V2);
    assert_eq!(s.terminal_states.len(), 1);
    assert_eq!(
        s.terminal_exit("ModuleFault"),
        Some(TerminalExit::ManagementReset)
    );
    assert_eq!(s.terminal_exit("ModuleReady"), None);
}

#[test]
fn keeps_standard_timers_apart_from_host_policy() {
    let s = parse(V2);
    let pwr_up = &s.state("ModulePwrUp").unwrap().timeouts;
    assert_eq!(pwr_up.len(), 2);
    assert_eq!(
        pwr_up[0].source,
        TimeoutSource::Standard {
            timer: "MaxDurationModulePwrUp".into(),
            duration: TimerDuration::AdvertisedField(
                "page_01h_advertising.max_duration_module_pwr_up".into()
            ),
        }
    );
    assert_eq!(pwr_up[0].on_expiry.as_deref(), Some("ModuleFault"));
    assert_eq!(
        pwr_up[1].source,
        TimeoutSource::HostPolicy(HostPolicyKey::ReadinessBackstop)
    );
    assert_eq!(
        pwr_up[1].on_expiry, None,
        "a host deadline never moves the module"
    );

    let mgmt = &s.state("MgmtInit").unwrap().timeouts;
    assert_eq!(
        mgmt[0].source,
        TimeoutSource::Standard {
            timer: "tMgmtInit".into(),
            duration: TimerDuration::FixedMs(2000),
        }
    );
    assert!(mgmt[0].provenance.is_some(), "timer provenance kept");

    let policy = s.host_policy.as_ref().expect("host_policy kept");
    let cadence = policy.get(HostPolicyKey::PollCadence).unwrap();
    let backstop = policy.get(HostPolicyKey::ReadinessBackstop).unwrap();
    assert_eq!(cadence.value_ms, 50);
    assert_eq!(backstop.value_ms, 10_000);
    assert!(backstop.provenance["source"]
        .as_str()
        .unwrap()
        .contains("power.c:68"));
    assert!(cadence.description.is_some());
}

#[test]
fn keeps_guards_as_data() {
    let s = parse(V2);
    let t = s
        .transitions
        .iter()
        .find(|t| t.id.as_deref() == Some("low_pwr_to_pwr_up"))
        .unwrap();
    assert_eq!(
        t.guards,
        vec![
            Guard::Equals {
                field: "lower_memory.low_pwr_allow_request_hw".into(),
                value: Scalar::Bool(false),
            },
            Guard::ForAll {
                field: "page_11h_lane_status.dp_state_host_lane".into(),
                value: Scalar::Symbol("DPDeactivated".into()),
            },
        ]
    );
    assert_eq!(t.trigger.kind, TriggerKind::RegisterWrite);
    assert_eq!(
        t.trigger.register.as_deref(),
        Some("lower_memory.low_pwr_request_sw")
    );
    assert_eq!(t.trigger.value, Some(Scalar::Int(0)));
    assert_eq!(
        t.side_effects[0],
        SideEffect::StateChange {
            target: "lower_memory.module_state".into(),
            operation: StateChangeOp::Assign,
            value: Some(Scalar::Symbol("ModulePwrUp".into())),
        }
    );
    assert!(matches!(t.side_effects[1], SideEffect::StartTimer { .. }));
    let fault = s
        .transitions
        .iter()
        .find(|t| t.id.as_deref() == Some("fault_from_ready"))
        .unwrap();
    assert_eq!(
        fault.side_effects[1],
        SideEffect::StateChange {
            target: "lower_memory.fault_flag".into(),
            operation: StateChangeOp::Set,
            value: None,
        }
    );
    let ready = s
        .transitions
        .iter()
        .find(|t| t.id.as_deref() == Some("pwr_up_to_ready"))
        .unwrap();
    assert!(ready.guards.is_empty(), "guard: null -> no predicates");
}

#[test]
fn keeps_per_state_and_per_transition_since_until() {
    let s = parse(V2);
    assert_eq!(
        s.state("LegacyOnly").unwrap().revs,
        RevRange {
            since: None,
            until: Some(Rev::V4_0)
        }
    );
    let in_30: Vec<&str> = s.states_in(Rev::V3_0).map(|x| x.name.as_str()).collect();
    let in_53: Vec<&str> = s.states_in(Rev::V5_3).map(|x| x.name.as_str()).collect();
    assert!(in_30.contains(&"LegacyOnly"));
    assert!(!in_53.contains(&"LegacyOnly"));

    let ids = |rev| -> Vec<String> {
        s.transitions_in(rev)
            .map(|t| t.id.clone().unwrap())
            .collect()
    };
    assert!(ids(Rev::V3_0).contains(&"legacy_edge".to_string()));
    assert!(!ids(Rev::V3_0).contains(&"low_pwr_to_pwr_up".to_string()));
    assert!(ids(Rev::V4_0).contains(&"low_pwr_to_pwr_up".to_string()));
    assert!(!ids(Rev::V4_0).contains(&"legacy_edge".to_string()));
}

#[test]
fn per_timeout_since_until_is_kept() {
    let yaml = v2_with(
        "        timer: tMgmtInit\n        duration_ms: 2000\n",
        "        timer: tMgmtInit\n        duration_ms: 2000\n        since: \"5.0\"\n",
    );
    let s = parse(&yaml);
    let t = &s.state("MgmtInit").unwrap().timeouts[0];
    assert_eq!(t.revs.since, Some(Rev::V5_0));
    assert!(!t.revs.contains(Rev::V4_0));
}

#[test]
fn scope_is_kept() {
    let s = parse(V2);
    let scope = s.scope.as_ref().unwrap();
    assert_eq!(scope.kind, ScopeKind::PerModule);
    assert_eq!(scope.cardinality, Cardinality::Fixed(1));
}

// ── fail closed: nothing dropped, defaulted or misrepresented ─────────────

#[test]
fn rejects_unknown_namespace() {
    let err = reject(&v2_with(
        "openxcvr.state_machine.v2",
        "openxcvr.state_machine.v3",
    ));
    assert!(matches!(
        err,
        ParseStateMachineError::UnsupportedNamespace { ref namespace } if namespace == "openxcvr.state_machine.v3"
    ));
}

#[test]
fn rejects_unknown_keys_everywhere() {
    for (from, to, key) in [
        (
            "title: Synthetic",
            "colour: red\ntitle: Synthetic",
            "colour",
        ),
        (
            "    class: stable\n    until",
            "    class: stable\n    stray: 1\n    until",
            "stray",
        ),
        (
            "        timer: tMgmtInit\n",
            "        timer: tMgmtInit\n        jitter: 5\n",
            "jitter",
        ),
        (
            "    value_ms: 10000\n",
            "    value_ms: 10000\n    units: ms\n",
            "units",
        ),
        (
            "    trigger: { kind: internal, condition",
            "    weight: 2\n    trigger: { kind: internal, condition",
            "weight",
        ),
        (
            "trigger: { kind: external_event, signal: LowPwrRequestHW }",
            "trigger: { kind: external_event, signal: LowPwrRequestHW, edge: rising }",
            "edge",
        ),
        (
            "{ kind: cancel_timer, timer: MaxDurationModulePwrUp }",
            "{ kind: cancel_timer, timer: MaxDurationModulePwrUp, why: x }",
            "why",
        ),
        (
            "  - { state: ModuleFault, exit: management_reset }",
            "  - { state: ModuleFault, exit: management_reset, note: x }",
            "note",
        ),
        (
            "scope: { kind: per_module, cardinality: 1 }",
            "scope: { kind: per_module, cardinality: 1, lanes: 8 }",
            "lanes",
        ),
    ] {
        assert_yaml_error(reject(&v2_with(from, to)), key);
    }
}

#[test]
fn v2_state_requires_class() {
    assert_invalid(
        reject(&v2_with(
            "    enum_value: 0x3\n    class: stable\n",
            "    enum_value: 0x3\n",
        )),
        "require `class`",
    );
    assert_invalid(
        reject(&v2_with(
            "    enum_value: 0x3\n    class: stable\n",
            "    enum_value: 0x3\n    class: steady\n",
        )),
        "unknown class",
    );
}

#[test]
fn state_without_encoding_must_say_unreported() {
    assert_invalid(
        reject(&v2_with(
            "  - name: Resetting\n    class: transient\n    reported: false\n",
            "  - name: Resetting\n    class: transient\n",
        )),
        "reported: false",
    );
    assert_invalid(
        reject(&v2_with(
            "  - name: MgmtInit\n    class: transient\n    reported: false\n",
            "  - name: MgmtInit\n    class: transient\n    reported: false\n    enum_value: 0x7\n",
        )),
        "no enum_value",
    );
}

#[test]
fn v2_timeout_requires_explicit_source() {
    assert_invalid(
        reject(&v2_with(
            "      - source: standard\n        timer: tMgmtInit\n",
            "      - timer: tMgmtInit\n",
        )),
        "require `source`",
    );
    assert_invalid(
        reject(&v2_with(
            "      - source: standard\n        timer: tMgmtInit\n",
            "      - source: vendor\n        timer: tMgmtInit\n",
        )),
        "unknown timeout source",
    );
    // A standard timer must name its timer and its duration.
    assert_invalid(
        reject(&v2_with("        timer: tMgmtInit\n", "")),
        "requires `timer`",
    );
    assert_invalid(
        reject(&v2_with("        duration_ms: 2000\n", "")),
        "duration_ms or duration_ms_field",
    );
    assert_invalid(
        reject(&v2_with(
            "        duration_ms: 2000\n",
            "        duration_ms: 2000\n        duration_ms_field: page_01h_advertising.x\n",
        )),
        "mutually exclusive",
    );
}

#[test]
fn host_policy_never_masquerades_as_a_standard_timer() {
    // A host-policy timeout cannot carry a timer name, a duration or an expiry
    // target: those belong to standard timers only.
    let anchor =
        "      - source: host_policy\n        policy: readiness_backstop\n  - name: ModuleLowPwr";
    for (extra, key) in [
        ("        timer: MaxDurationModulePwrUp\n", "timer"),
        ("        duration_ms: 10000\n", "duration_ms"),
        ("        on_expiry: ModuleFault\n", "on_expiry"),
    ] {
        let yaml = v2_with(
            anchor,
            &format!(
                "      - source: host_policy\n        policy: readiness_backstop\n{extra}  - name: ModuleLowPwr"
            ),
        );
        assert_invalid(reject(&yaml), key);
    }
    // Cadence is not a deadline.
    assert_invalid(
        reject(&v2_with(
            anchor,
            "      - source: host_policy\n        policy: poll_cadence\n  - name: ModuleLowPwr",
        )),
        "not a deadline",
    );
    // A referenced host policy must be defined.
    assert_invalid(
        reject(&v2_with(
            "  readiness_backstop:\n    value_ms: 10000\n    provenance: { source: \"the kernel module plugins/cmis/power.c:68\" }\n",
            "",
        )),
        "does not define",
    );
    // A standard timer cannot point at a host policy.
    assert_invalid(
        reject(&v2_with(
            "        timer: tMgmtInit\n",
            "        timer: tMgmtInit\n        policy: readiness_backstop\n",
        )),
        "policy",
    );
}

#[test]
fn host_policy_requires_provenance_and_a_value() {
    assert_invalid(
        reject(&v2_with(
            "    provenance: { source: \"the kernel module plugins/cmis/power.c:65\" }\n",
            "",
        )),
        "requires provenance",
    );
    assert_invalid(
        reject(&v2_with("    value_ms: 50\n", "    value_ms: 0\n")),
        "positive",
    );
    assert_yaml_error(reject(&v2_with("    value_ms: 50\n", "")), "value_ms");
}

#[test]
fn at_most_one_timeout_per_source_per_revision() {
    let yaml = v2_with(
        "        duration_ms: 2000\n",
        "        duration_ms: 2000\n      - source: standard\n        timer: tOther\n        duration_ms: 5\n",
    );
    assert_invalid(reject(&yaml), "more than one timeout");
}

#[test]
fn only_transient_states_carry_timeouts() {
    let yaml = v2_with(
        "    enum_value: 0x3\n    class: stable\n",
        "    enum_value: 0x3\n    class: stable\n    timeout: { source: host_policy, policy: readiness_backstop }\n",
    );
    assert_invalid(reject(&yaml), "only a transient state");
}

#[test]
fn terminal_states_must_agree_with_flags_and_classes() {
    // Listed but not flagged.
    assert_invalid(
        reject(&v2_with(
            "    class: fault\n    is_terminal: true\n",
            "    class: fault\n",
        )),
        "is_terminal",
    );
    // A stable state cannot halt the machine.
    assert_invalid(
        reject(&v2_with(
            "  - { state: ModuleFault, exit: management_reset }",
            "  - { state: ModuleFault, exit: management_reset }\n  - { state: ModuleReady, exit: power_cycle }",
        )),
        "cannot be terminal",
    );
    assert_invalid(
        reject(&v2_with("exit: management_reset }", "exit: reboot }")),
        "unknown exit",
    );
    assert_invalid(
        reject(&v2_with(
            "  - { state: ModuleFault, exit: management_reset }",
            "  - { state: ModuleFault, exit: management_reset }\n  - { state: Nowhere, exit: power_cycle }",
        )),
        "names no declared state",
    );
}

#[test]
fn guards_outside_adr_0010_are_refused() {
    let anchor = "      - { field: lower_memory.low_pwr_allow_request_hw, equals: false }";
    for (replacement, needle) in [
        (
            "      - { field: lower_memory.x, not_equals: false }",
            "exactly { field, equals }",
        ),
        (
            "      - { field: lower_memory.x, equals: false, note: y }",
            "exactly { field, equals }",
        ),
        (
            "      - \"module_state == ModuleReady\"",
            "predicate mapping",
        ),
        ("      - forall: lower_memory.x", "predicate mapping"),
        (
            "      - { field: lower_memory.x, equals: [1, 2] }",
            "equals",
        ),
        (
            "      - { field: lower_memory.x, equals: 1.5 }",
            "not an integer",
        ),
    ] {
        assert_invalid(reject(&v2_with(anchor, replacement)), needle);
    }
}

#[test]
fn triggers_and_side_effects_are_typed() {
    assert_invalid(
        reject(&v2_with(
            "trigger: { kind: internal }\n    guard: null",
            "trigger: { kind: magic }\n    guard: null",
        )),
        "unknown trigger kind",
    );
    assert_invalid(
        reject(&v2_with(
            "      register: lower_memory.low_pwr_request_sw\n",
            "",
        )),
        "requires `register`",
    );
    assert_invalid(
        reject(&v2_with(
            "trigger: { kind: external_event, signal: LowPwrRequestHW }",
            "trigger: { kind: internal, signal: LowPwrRequestHW }",
        )),
        "does not take signal",
    );
    assert_invalid(
        reject(&v2_with(
            "operation: assign, value: ModulePwrUp }",
            "operation: assign }",
        )),
        "requires `value`",
    );
    assert_invalid(
        reject(&v2_with(
            "target: lower_memory.fault_flag, operation: set }",
            "target: lower_memory.fault_flag, operation: set, value: 1 }",
        )),
        "only operation assign",
    );
    assert_invalid(
        reject(&v2_with(
            "{ kind: spawn_machine, machine: CmisDataPathStateMachine }",
            "{ kind: fork_machine, machine: CmisDataPathStateMachine }",
        )),
        "unknown side-effect kind",
    );
    assert_invalid(
        reject(&v2_with(
            "{ kind: log_event, event_id: msm_power_up_started }",
            "{ kind: log_event, event_id: msm_power_up_started, machine: X }",
        )),
        "does not take machine",
    );
}

#[test]
fn graph_must_close_over_declared_states() {
    assert_invalid(
        reject(&v2_with("    to: MgmtInit\n", "    to: Nowhere\n")),
        "names no declared state",
    );
    assert_invalid(
        reject(&v2_with(
            "        on_expiry: ModuleFault\n",
            "        on_expiry: Nowhere\n",
        )),
        "names no declared state",
    );
    assert_invalid(
        reject(&v2_with(
            "  - id: mgmt_init_to_low_pwr\n",
            "  - id: reset_to_mgmt_init\n",
        )),
        "duplicate transition id",
    );
    assert_invalid(
        reject(&v2_with("    enum_value: 0x6\n", "    enum_value: 0x5\n")),
        "is also ModuleFault in revision 3.0",
    );
}

#[test]
fn initial_state_is_checked() {
    assert_invalid(
        reject(&v2_with(
            "initial_state: Resetting",
            "initial_state: ModuleLowPwr",
        )),
        "is_initial",
    );
    assert_invalid(
        reject(&v2_with(
            "    reported: false\n    is_initial: true\n",
            "    reported: false\n    is_initial: true\n    since: \"4.0\"\n",
        )),
        "every supported revision",
    );
}

#[test]
fn revision_bounds_are_validated_and_consistent() {
    // Malformed revision on a state.
    match reject(&v2_with(
        "    until: \"4.0\"\nterminal_states",
        "    until: \"4.x\"\nterminal_states",
    )) {
        ParseStateMachineError::InvalidStateRevString { state, key, value } => {
            assert_eq!(
                (state.as_str(), key, value.as_str()),
                ("LegacyOnly", "until", "4.x")
            );
        }
        other => panic!("expected InvalidStateRevString, got {other:?}"),
    }
    // Malformed revision on a transition.
    match reject(&v2_with(
        "    since: \"4.0\"\n  - id: pwr_up_to_ready",
        "    since: \"v4\"\n  - id: pwr_up_to_ready",
    )) {
        ParseStateMachineError::InvalidRevString {
            transition, key, ..
        } => {
            assert_eq!((transition.as_str(), key), ("low_pwr_to_pwr_up", "since"));
        }
        other => panic!("expected InvalidRevString, got {other:?}"),
    }
    // Empty range.
    assert_invalid(
        reject(&v2_with(
            "    until: \"4.0\"\nterminal_states",
            "    since: \"5.0\"\n    until: \"4.0\"\nterminal_states",
        )),
        "covers no supported revision",
    );
    // A transition may not outlive an endpoint.
    assert_invalid(
        reject(&v2_with(
            "    trigger: { kind: external_event, signal: LowPwrRequestHW }\n    until: \"4.0\"\n",
            "    trigger: { kind: external_event, signal: LowPwrRequestHW }\n",
        )),
        "does not exist in every revision",
    );
    // A timeout may not outlive its state.
    assert_invalid(
        reject(
            &v2_with(
                "  - name: MgmtInit\n    class: transient\n",
                "  - name: MgmtInit\n    class: transient\n    since: \"5.0\"\n",
            )
            .replacen(
                "        duration_ms: 2000\n",
                "        duration_ms: 2000\n        since: \"4.0\"\n",
                1,
            ),
        ),
        "beyond the state",
    );
}

// ── v1 stays v1: v2 vocabulary is refused rather than silently accepted ───

const V1: &str = r#"
schema: openxcvr.state_machine.v1
kind: state_machine
id: TinyV1
initial_state: A
states:
  - name: A
    enum_value: 1
    is_initial: true
    timeout: null
  - name: B
    enum_value: 2
    timeout: { duration_ms_field: page_01h_advertising.x, on_expiry: A }
transitions:
  - { from: A, to: B, trigger: { kind: internal } }
"#;

#[test]
fn v1_timeouts_are_unattributed_not_claimed_standard() {
    let s = parse(V1);
    assert_eq!(s.state("A").unwrap().class, None);
    assert_eq!(
        s.state("B").unwrap().timeouts[0].source,
        TimeoutSource::Unattributed(TimerDuration::AdvertisedField(
            "page_01h_advertising.x".into()
        ))
    );
    assert!(s.host_policy.is_none());
}

#[test]
fn v1_refuses_v2_only_keys() {
    for (from, to, key) in [
        (
            "    enum_value: 2\n",
            "    enum_value: 2\n    class: transient\n",
            "class",
        ),
        (
            "    enum_value: 2\n",
            "    enum_value: 2\n    since: \"4.0\"\n",
            "since",
        ),
        (
            "    enum_value: 2\n",
            "    enum_value: 2\n    reported: true\n",
            "reported",
        ),
        (
            "{ duration_ms_field: page_01h_advertising.x,",
            "{ source: standard, duration_ms_field: page_01h_advertising.x,",
            "source",
        ),
        (
            "initial_state: A",
            "initial_state: A\nhost_policy: { poll_cadence: { value_ms: 50, provenance: x } }",
            "host_policy",
        ),
    ] {
        assert_eq!(V1.matches(from).count(), 1, "anchor {from:?}");
        match reject(&V1.replacen(from, to, 1)) {
            ParseStateMachineError::RequiresV2 { key: k, .. } => assert_eq!(k, key),
            other => panic!("expected RequiresV2 for {key}, got {other:?}"),
        }
    }
    match reject(&V1.replacen(
        "timeout: { duration_ms_field: page_01h_advertising.x, on_expiry: A }",
        "timeout: [{ duration_ms_field: page_01h_advertising.x }]",
        1,
    )) {
        ParseStateMachineError::RequiresV2 { key, .. } => assert_eq!(key, "timeout list"),
        other => panic!("expected RequiresV2 for timeout list, got {other:?}"),
    }
}

// ── every existing schema file loads, with nothing dropped ────────────────

#[test]
fn loads_every_existing_state_machine_schema() {
    let mut loaded = Vec::new();
    for entry in std::fs::read_dir(schema_dir()).expect("schema dir") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "yaml") {
            let schema = load_state_machine_schema(&path)
                .unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
            // Cross-check against an independent untyped read so a state,
            // transition, guard, timeout or terminal entry cannot vanish.
            let raw: serde_norway::Value =
                serde_norway::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let seq_len = |key: &str| raw[key].as_sequence().map_or(0, |s| s.len());
            assert_eq!(schema.states.len(), seq_len("states"), "{}", path.display());
            assert_eq!(schema.transitions.len(), seq_len("transitions"));
            assert_eq!(schema.terminal_states.len(), seq_len("terminal_states"));
            let raw_states = raw["states"].as_sequence().unwrap();
            for (state, raw_state) in schema.states.iter().zip(raw_states) {
                assert_eq!(state.name, raw_state["name"].as_str().unwrap());
                assert_eq!(
                    state.timeouts.len(),
                    usize::from(!raw_state["timeout"].is_null())
                );
            }
            let raw_transitions = raw["transitions"].as_sequence().unwrap();
            for (t, raw_t) in schema.transitions.iter().zip(raw_transitions) {
                let guards = raw_t["guard"].as_sequence().map_or(0, |g| g.len());
                let effects = raw_t["side_effects"].as_sequence().map_or(0, |g| g.len());
                assert_eq!(t.guards.len(), guards, "{}", t.label());
                assert_eq!(t.side_effects.len(), effects, "{}", t.label());
                assert_eq!(t.revs.since.map(|r| r.as_str()), raw_t["since"].as_str());
                assert!(t.provenance.is_some() || raw_t["provenance"].is_null());
            }
            loaded.push(schema.id.clone());
        }
    }
    loaded.sort();
    assert_eq!(
        loaded,
        [
            "CmisDataPathStateMachine",
            "CmisModuleStateMachine",
            "CmisNetworkPathStateMachine"
        ]
    );
}

#[test]
fn module_state_machine_keeps_states_timeouts_and_terminal() {
    let s = load_state_machine_schema(&schema_dir().join("module-state-machine.yaml")).unwrap();
    // The unreported-state schema decision (Resetting/Reset/MgmtInit as
    // `reported: false`) moved the machine to openxcvr.state_machine.v2.
    assert_eq!(s.format, StateMachineFormat::V2);
    assert_eq!(
        s.terminal_exit("ModuleFault"),
        Some(TerminalExit::ManagementReset)
    );
    // The unreported reset phases: no enum value, class modelled.
    for name in ["Resetting", "Reset", "MgmtInit"] {
        let state = s
            .state(name)
            .unwrap_or_else(|| panic!("the machine must model the unreported state {name}"));
        assert_eq!(state.encoding, StateEncoding::Unreported, "{name}");
        assert!(state.class.is_some(), "{name}");
    }
    assert_eq!(s.initial_state, "Reset");
    // MgmtInit carries the standard's tMgmtInit duration (§ 10.1).
    let mgmt_init = s.state("MgmtInit").unwrap();
    assert!(matches!(
        mgmt_init.timeouts[0].source,
        TimeoutSource::Standard { .. }
    ));
    let host_policy = s.host_policy.as_ref().expect("host policy block");
    assert_eq!(host_policy.poll_cadence.as_ref().unwrap().value_ms, 50);
    assert_eq!(
        host_policy.readiness_backstop.as_ref().unwrap().value_ms,
        10_000
    );
    let pwr_up = s.state("ModulePwrUp").unwrap();
    assert_eq!(
        pwr_up.encoding,
        StateEncoding::Reported {
            enum_name: Some("module_pwr_up".into()),
            value: 2
        }
    );
    assert_eq!(pwr_up.timeouts[0].on_expiry.as_deref(), Some("ModuleFault"));
    assert_eq!(
        s.state("ModulePwrDn").unwrap().encoding,
        StateEncoding::Reported {
            enum_name: Some("module_pwr_dn".into()),
            value: 4
        }
    );
    let deactivate = s
        .transitions
        .iter()
        .find(|t| t.id.as_deref() == Some("module_ready_to_module_pwr_dn"))
        .unwrap();
    assert!(deactivate.guards.contains(&Guard::ForAll {
        field: "page_11h_lane_status.dp_state_host_lane".into(),
        value: Scalar::Symbol("DPDeactivated".into()),
    }));
    // Every reported state, and only reported states, has a Resetting-exit
    // edge (Table 6-12); ModuleFault's is optional and modelled.
    for name in [
        "ModuleLowPwr",
        "ModulePwrUp",
        "ModuleReady",
        "ModulePwrDn",
        "ModuleFault",
        "MgmtInit",
    ] {
        assert!(
            s.transitions
                .iter()
                .any(|t| t.from == name && t.to == "Resetting"),
            "{name} must have a Resetting-exit edge"
        );
    }
}

#[test]
fn data_path_state_machine_keeps_transition_since() {
    let s = load_state_machine_schema(&schema_dir().join("data-path-state-machine.yaml")).unwrap();
    let t = s
        .transitions
        .iter()
        .find(|t| t.id.as_deref() == Some("dp_apply_init_pending_from_initialized"))
        .unwrap();
    assert_eq!(t.revs.since, Some(Rev::V5_0));
    assert!(s.transitions_in(Rev::V4_0).all(|x| x.id != t.id));
}
