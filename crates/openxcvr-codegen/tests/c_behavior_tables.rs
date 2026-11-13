//! Integration tests for the C behaviour-table emitter
//! (`emitters::c::state_machines`): schema parity and emission determinism
//! over the real CMIS state-machine schemas, plus cross-surface parity
//! against the Rust tables (the same schema must produce the same
//! revision coverage, state sets and initial states in both languages).
//!
//! : the tables are pure data `the kernel module` may
//! compile in; the table format is owned by openxcvr-lib. These tests pin
//! that contract: nothing here bends to the output — a failing assertion
//! means the generator (or the schema) changed meaning.

use std::collections::BTreeSet;
use std::path::Path;

use openxcvr_codegen::emitters::c::{
    emit_shared_types_header, emit_state_machine_c_header, emit_state_machine_c_source,
};
use openxcvr_codegen::ir::version::Rev;
use openxcvr_codegen::parser::index::IndexLoader;
use openxcvr_codegen::parser::{enums, state_machines};

const SCHEMAS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas");

fn load_cmis() -> (
    Vec<openxcvr_codegen::ir::state_machines::StateMachineSchema>,
    Vec<openxcvr_codegen::ir::enums::EnumSchema>,
) {
    let enum_paths = IndexLoader::collect_enum_schemas(Path::new(SCHEMAS), Some("cmis"))
        .expect("collect cmis enums");
    let enum_schemas: Vec<_> = enum_paths
        .iter()
        .map(|p| enums::load_enum_schema(p).expect("enum schema loads"))
        .collect();
    let machine_paths =
        IndexLoader::collect_state_machine_schemas(Path::new(SCHEMAS), Some("cmis"))
            .expect("collect cmis state machines");
    let machines: Vec<_> = machine_paths
        .iter()
        .map(|p| state_machines::load_state_machine_schema(p).expect("state machine schema loads"))
        .collect();
    (machines, enum_schemas)
}

#[test]
fn module_machine_emits_schema_parity_tables() {
    let (machines, enums) = load_cmis();
    let module = machines
        .iter()
        .find(|m| m.id == "CmisModuleStateMachine")
        .expect("CmisModuleStateMachine is collected");
    let header = emit_state_machine_c_header(module, &enums).expect("header emits");
    let source = emit_state_machine_c_source(module, &enums).expect("source emits");

    // Schema parity: every declared state — the three unreported CMIS 5.3
    // § 6.3.2 states included — appears in the machine-space enum, in
    // schema order, and the initial state is the unreported Reset.
    for state in &module.states {
        assert!(
            source.contains(&format!(".schema_name = \"{}\"", state.name)),
            "state {} missing from the emitted tables:\n{source}",
            state.name
        );
    }
    assert!(header.contains("typedef enum openxcvr_cmis_module_state_space {"));
    for (index, name) in ["Resetting", "Reset", "MgmtInit"].iter().enumerate() {
        assert!(
            header.contains(&format!(
                "OPENXCVR_CMIS_MODULE_STATE_SPACE_{} = {index},",
                match *name {
                    "MgmtInit" => "MGMT_INIT".to_string(),
                    other => other.to_ascii_uppercase(),
                }
            )),
            "unreported state {name} missing from the space enum:\n{header}"
        );
    }
    assert!(source.contains(".initial = OPENXCVR_CMIS_MODULE_STATE_SPACE_RESET,"));

    // Reported states carry the registry's ModuleState values: 4 is
    // ModulePwrDn and 5 is ModuleFault (the kmod correction), never the
    // donor's 4-as-fault / 5-ignored decode.
    assert!(source.contains(
        ".state = OPENXCVR_CMIS_MODULE_STATE_SPACE_MODULE_PWR_DN, \
         .state_class = OPENXCVR_FSM_STATE_CLASS_TRANSIENT, .reported = true, \
         .reported_value = 0x4"
    ));
    assert!(source.contains(
        ".state = OPENXCVR_CMIS_MODULE_STATE_SPACE_MODULE_FAULT, \
         .state_class = OPENXCVR_FSM_STATE_CLASS_FAULT, .reported = true, \
         .reported_value = 0x5"
    ));

    // Host policy: kmod's 50 ms cadence and 10 s backstop as named
    // constants, kept apart from the standard timers.
    assert!(header.contains(
        "#define OPENXCVR_CMIS_MODULE_STATE_MACHINE_HOST_POLICY_POLL_CADENCE_MS UINT64_C(50)"
    ));
    assert!(header.contains(
        "#define OPENXCVR_CMIS_MODULE_STATE_MACHINE_HOST_POLICY_READINESS_BACKSTOP_MS \
         UINT64_C(10000)"
    ));

    // Standard timers as standard sources: tMgmtInit fixed, ModulePwrUp
    // advertised through page 01h, expiring into ModuleFault.
    assert!(source.contains(".timer = \"tMgmtInit\""));
    assert!(source
        .contains(".duration = { OPENXCVR_FSM_TIMER_FIXED_MS, { .fixed_ms = UINT64_C(2000) } }"));
    assert!(
        source.contains(".advertised_field = \"page_01h_advertising.max_duration_module_pwr_up\"")
    );
    assert!(source
        .contains("OPENXCVR_FSM_TIMEOUT_SOURCE_STANDARD, .timer = \"ModulePwrUpMaxDuration\""));
}

#[test]
fn factories_fail_closed_exactly_on_catalog_coverage() {
    let (machines, enums) = load_cmis();
    // Every collected machine covers the whole CMIS catalog today; the
    // emitted byte factory must arm exactly those revisions and refuse
    // everything else, including 0x00, a malformed byte and 5.5.
    for machine in &machines {
        let source = emit_state_machine_c_source(machine, &enums).expect("source emits");
        let mut covered = BTreeSet::new();
        for rev in Rev::CMIS_ALL {
            if source.contains(&format!(
                "case {:#04x}: return &openxcvr_{}_fsm_v",
                rev.major_byte_cmis(),
                openxcvr_codegen::ir::naming::camel_to_snake(&machine.id)
            )) {
                covered.insert(*rev);
            }
        }
        assert_eq!(
            covered,
            Rev::CMIS_ALL.iter().copied().collect::<BTreeSet<_>>(),
            "{} must cover exactly the CMIS catalog",
            machine.id
        );
        // Fail-closed default in both factories; unknown bytes and a 5.5
        // context have no arm.
        assert!(source.contains("default: return 0;"));
        assert!(!source.contains("case 0x00:"));
        assert!(!source.contains("case 0x55:"));
        assert!(!source.contains("case ((5u << 8) | 5u)"));
    }
}

#[test]
fn all_reported_machines_reuse_the_registry_enum() {
    let (machines, enums) = load_cmis();
    let data_path = machines
        .iter()
        .find(|m| m.id == "CmisDataPathStateMachine")
        .expect("CmisDataPathStateMachine is collected");
    let header = emit_state_machine_c_header(data_path, &enums).expect("header emits");
    // No machine-space enum for an all-reported machine: it includes the
    // generated registry enum header instead, so the C tables and the C
    // enums/ tree cannot drift apart.
    assert!(!header.contains("_space"));
    assert!(header.contains("#include \"enums/cmis_data_path_state.h\""));
}

#[test]
fn emission_is_deterministic_across_the_whole_family() {
    let (machines, enums) = load_cmis();
    assert!(
        !machines.is_empty(),
        "the cmis spec collects state machines"
    );
    for machine in &machines {
        let h1 = emit_state_machine_c_header(machine, &enums).expect("header");
        let s1 = emit_state_machine_c_source(machine, &enums).expect("source");
        let h2 = emit_state_machine_c_header(machine, &enums).expect("header again");
        let s2 = emit_state_machine_c_source(machine, &enums).expect("source again");
        assert_eq!(
            h1, h2,
            "{}: header emission must be byte-deterministic",
            machine.id
        );
        assert_eq!(
            s1, s2,
            "{}: source emission must be byte-deterministic",
            machine.id
        );
    }
    let shared1 = emit_shared_types_header();
    let shared2 = emit_shared_types_header();
    assert_eq!(shared1, shared2, "shared header must be byte-deterministic");
}

#[test]
fn c_tables_match_the_rust_tables_per_revision() {
    // Cross-surface parity: the same schema must produce the same revision
    // coverage, per-revision state sets and initial states in the C tables
    // and in the Rust tables. Two emitters disagreeing means one of them
    // disagrees with the schema.
    let (machines, enums) = load_cmis();
    for machine in &machines {
        let rust = openxcvr_codegen::emitters::rust::state_machines::emit_state_machine_rust(
            machine, &enums,
        )
        .expect("rust emits");
        let c = emit_state_machine_c_source(machine, &enums).expect("c emits");
        for rev in Rev::CMIS_ALL {
            let suffix = rev.dirname().to_uppercase();
            let snake = openxcvr_codegen::ir::naming::camel_to_snake(&machine.id);
            let rust_has = rust.contains(&format!("pub static FSM_V{suffix}:"));
            let c_has = c.contains(&format!("openxcvr_{snake}_fsm_v{suffix} = {{"));
            assert_eq!(
                rust_has,
                c_has,
                "{}: C and Rust disagree about the table for CMIS {}",
                machine.id,
                rev.as_str()
            );
            if !rust_has {
                continue;
            }
            // Same per-revision state count and transition count.
            let rust_states = rust
                .split(&format!("static STATES_V{suffix}:"))
                .nth(1)
                .and_then(|s| s.split("];").next())
                .map(|s| s.matches("FsmStateDesc {").count())
                .unwrap_or_default();
            let c_states = c
                .split(&format!(
                    "static const openxcvr_fsm_state_desc openxcvr_{snake}_v{suffix}_states[]"
                ))
                .nth(1)
                .and_then(|s| s.split("};").next())
                .map(|s| s.matches(".schema_name = ").count())
                .unwrap_or_default();
            assert_eq!(
                rust_states,
                c_states,
                "{}: state count disagrees for CMIS {}",
                machine.id,
                rev.as_str()
            );
            let rust_transitions = rust
                .split(&format!("static TRANSITIONS_V{suffix}:"))
                .nth(1)
                .and_then(|s| s.split("];").next())
                .map(|s| s.matches("FsmTransition {").count())
                .unwrap_or_default();
            let c_transitions = c
                .split(&format!(
                    "static const openxcvr_fsm_transition openxcvr_{snake}_v{suffix}_transitions[]"
                ))
                .nth(1)
                .and_then(|s| s.split("};").next())
                .map(|s| s.matches(".id = ").count())
                .unwrap_or_default();
            assert_eq!(
                rust_transitions,
                c_transitions,
                "{}: transition count disagrees for CMIS {}",
                machine.id,
                rev.as_str()
            );
        }
    }
}
