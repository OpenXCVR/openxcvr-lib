//! Generated module-state-machine table assertions (task 26 surface; the
//! flat-memory edge assertions are this change's). The schema is the
//! source of truth: these tests consume the generated tables only.
use openxcvr_cmis::generated::fsm::cmis_module_state_machine::{
    for_rev_major_byte, for_revision, CmisModuleStateSpace, HOST_POLICY_POLL_CADENCE_MS,
    HOST_POLICY_READINESS_BACKSTOP_MS,
};
use openxcvr_cmis::generated::{v3_0, v4_0, v5_0, v5_1, v5_2, v5_3, v5_4, CmisModuleState};
use openxcvr_core::fsm::{
    FsmGuard, FsmScalar, FsmStateClass, FsmTerminalExit, FsmTimeoutSource, FsmTimerDuration,
    FsmTriggerKind,
};
use openxcvr_core::rev::Rev;

/// CMIS 5.3 § 6.3.2.3 Table 6-15's flat-memory MgmtInit exit: a flat
/// memory module transitions to ModuleReady without host interaction,
/// readiness simply assumed after tMgmtInit, guarded by the MemoryModel
/// bit (lower byte 2 bit 7). Published since CMIS 4.0 (§ 6.3.1.2
/// Table 6-13); CMIS 3.0's flat passive-copper table has no such row.
#[test]
fn the_flat_memory_mgmt_init_to_module_ready_edge_is_present_since_4_0_only() {
    for revision in [
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let table = for_revision(revision).expect("catalog revision delivers a table");
        let edge = table
            .transitions
            .iter()
            .find(|transition| transition.id == "mgmt_init_to_module_ready_flat")
            .unwrap_or_else(|| panic!("{revision:?} must carry the flat-memory edge"));
        assert_eq!(edge.from, CmisModuleStateSpace::MgmtInit);
        assert_eq!(edge.to, CmisModuleStateSpace::ModuleReady);
        // The tMgmtInit timer trigger: assumed readiness, not host action.
        assert_eq!(edge.trigger.kind, FsmTriggerKind::Timer);
        assert_eq!(edge.trigger.timer, Some("tMgmtInit"));
        assert_eq!(edge.trigger.duration, Some(FsmTimerDuration::FixedMs(2000)));
        // The flat-memory guard: MemoryModel bit set (byte 2 bit 7).
        assert_eq!(
            edge.guards,
            &[FsmGuard::Equals {
                field: "lower_memory.memory_model",
                value: FsmScalar::Bool(true),
            }]
        );
    }
    // CMIS 3.0 predates the table: its flat passive-copper exit runs
    // MgmtInit → ModuleLowPwr only, so the edge must be absent.
    let table = for_revision(Rev::CMIS_3_0).expect("3.0 delivers a table");
    assert!(
        !table
            .transitions
            .iter()
            .any(|transition| transition.id == "mgmt_init_to_module_ready_flat"),
        "CMIS 3.0 has no MgmtInit → ModuleReady row"
    );
    // The paged walk's edge stays beside it in every revision that has
    // both: one machine, two memory-model-disjoint MgmtInit exits.
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let table = for_revision(revision).expect("catalog revision");
        assert!(
            table
                .transitions
                .iter()
                .any(|transition| transition.id == "mgmt_init_to_module_low_pwr"),
            "{revision:?} keeps the paged MgmtInit exit"
        );
    }
}

// ─── Task 26 contract suite (beyond the flat-edge anchor above) ────────────
//
// The schema is the source of truth: these tests consume the generated
// tables and the generated per-revision readers only, cross-checking the
// generated surface against schemas/cmis/state-machines/module-state-machine.yaml
// so an emitter regression (a dropped timeout, an invented timer, a borrowed
// class) fails here instead of downstream.

use std::path::PathBuf;

fn module_state_machine_schema() -> serde_norway::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/cmis/state-machines/module-state-machine.yaml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_norway::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

const CATALOG_REVS: [Rev; 7] = [
    Rev::CMIS_3_0,
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
    Rev::CMIS_5_4,
];

/// CMIS 5.3 § 6.3.2 / Table 8-7: the reported ModuleState codes. Code 4 is
/// ModulePwrDn (the transient return toward low power) and code 5 is
/// ModuleFault — the module-state-4-5 parity difference is anchored here.
/// Reserved codes never borrow a named state and stay distinct from each
/// other.
#[test]
fn codes_4_and_5_decode_to_pwrdn_and_fault_and_reserved_codes_stay_distinct() {
    let cases = [
        (0x00, CmisModuleState::Reserved0),
        (0x01, CmisModuleState::ModuleLowPwr),
        (0x02, CmisModuleState::ModulePwrUp),
        (0x03, CmisModuleState::ModuleReady),
        (0x04, CmisModuleState::ModulePwrDn),
        (0x05, CmisModuleState::ModuleFault),
    ];
    for (code, expected) in cases {
        assert_eq!(CmisModuleState::from(code), expected, "code {code:#04x}");
        assert_eq!(expected.repr(), code);
    }
    // Reserved 6 and 7 are distinct unknowns, never Reserved0 and never a
    // nearest named state.
    let six = CmisModuleState::from(0x06);
    let seven = CmisModuleState::from(0x07);
    assert_eq!(six, CmisModuleState::Unknown(6));
    assert_eq!(seven, CmisModuleState::Unknown(7));
    assert_ne!(six, seven);
    assert_ne!(six, CmisModuleState::Reserved0);
    // The resolver mirrors the decode both ways for every schema state.
    for state in [
        CmisModuleStateSpace::ModuleLowPwr,
        CmisModuleStateSpace::ModulePwrUp,
        CmisModuleStateSpace::ModuleReady,
        CmisModuleStateSpace::ModulePwrDn,
        CmisModuleStateSpace::ModuleFault,
    ] {
        let reported = state.reported().expect("reported state");
        assert_eq!(CmisModuleStateSpace::from_reported(reported), Some(state));
        assert_eq!(
            CmisModuleStateSpace::from_schema_name(state.schema_name()),
            Some(state)
        );
    }
}

/// CMIS 5.3 § 6.3.2.2: Resetting, Reset and MgmtInit apply to the module
/// but are "not reported to the host through the Memory Map" — no byte-3
/// encoding ever decodes to them, and their machine variants report no
/// register value.
#[test]
fn no_byte3_encoding_is_resetting_reset_or_mgmt_init() {
    for code in 0u8..=255 {
        let decoded = CmisModuleState::from(code);
        let resolved = CmisModuleStateSpace::from_reported(decoded);
        assert_ne!(
            resolved,
            Some(CmisModuleStateSpace::Resetting),
            "code {code:#04x} must not decode to Resetting"
        );
        assert_ne!(
            resolved,
            Some(CmisModuleStateSpace::Reset),
            "code {code:#04x} must not decode to Reset"
        );
        assert_ne!(
            resolved,
            Some(CmisModuleStateSpace::MgmtInit),
            "code {code:#04x} must not decode to MgmtInit"
        );
    }
    for unreported in [
        CmisModuleStateSpace::Resetting,
        CmisModuleStateSpace::Reset,
        CmisModuleStateSpace::MgmtInit,
    ] {
        assert_eq!(unreported.reported(), None);
        assert_eq!(
            CmisModuleStateSpace::from_reported(CmisModuleState::Reserved0),
            None
        );
    }
    // A schema name the machine does not declare resolves to nothing —
    // never a nearest state.
    assert_eq!(CmisModuleStateSpace::from_schema_name("Teleported"), None);
}

/// The state classes and the fault terminal exit are generated per
/// revision from the schema's states block (CMIS 5.3 § 6.3.2 Figure 6-3):
/// every revision carries the same class assignment, and ModuleFault is
/// the terminal state exited by a management reset.
#[test]
fn every_revision_carries_the_schema_state_classes_and_the_fault_exit() {
    let schema = module_state_machine_schema();
    let declared: Vec<(&str, &str, bool)> = schema["states"]
        .as_sequence()
        .expect("states list")
        .iter()
        .map(|state| {
            let class = state["class"].as_str().expect("class");
            let class = match class {
                "stable" => "Stable",
                "transient" => "Transient",
                "fault" => "Fault",
                other => panic!("unknown schema class {other}"),
            };
            (
                state["name"].as_str().expect("name"),
                class,
                state["reported"].as_bool().unwrap_or(true),
            )
        })
        .collect();
    assert_eq!(declared.len(), 8, "the machine declares eight states");
    for revision in CATALOG_REVS {
        let table = for_revision(revision).expect("catalog revision delivers a table");
        assert_eq!(table.states.len(), declared.len(), "{revision:?}");
        for (name, class, reported) in &declared {
            let desc = table
                .states
                .iter()
                .find(|state| state.state.schema_name() == *name)
                .unwrap_or_else(|| panic!("{revision:?} lacks state {name}"));
            let expected_class = match *class {
                "Stable" => FsmStateClass::Stable,
                "Transient" => FsmStateClass::Transient,
                "Fault" => FsmStateClass::Fault,
                other => panic!("{other}"),
            };
            assert_eq!(desc.class, Some(expected_class), "{revision:?} {name}");
            assert_eq!(desc.reported, *reported, "{revision:?} {name}");
        }
        // Terminal states: ModuleFault, exited by a management reset.
        assert_eq!(table.terminal_exits.len(), 1, "{revision:?}");
        assert_eq!(
            table.terminal_exits[0].state,
            CmisModuleStateSpace::ModuleFault
        );
        assert_eq!(
            table.terminal_exits[0].exit,
            FsmTerminalExit::ManagementReset
        );
        // The schema's declared initial state is the table's initial.
        assert_eq!(
            table.initial,
            CmisModuleStateSpace::from_schema_name("Reset").unwrap()
        );
    }
}

/// Every transient state the schema gives a standard timer carries it in
/// every revision's generated table — an emitter that omits an applicable
/// transient timer fails here — and the generated tables invent no timer
/// the schema does not declare.
#[test]
fn every_applicable_transient_timer_is_generated_and_none_is_invented() {
    let schema = module_state_machine_schema();
    // Schema transient states with a timeout block: state -> (timer,
    // duration, on_expiry).
    struct DeclaredTimeout {
        state: String,
        timer: String,
        fixed_ms: Option<u64>,
        on_expiry: Option<String>,
    }
    let mut declared: Vec<DeclaredTimeout> = Vec::new();
    for state in schema["states"].as_sequence().expect("states list") {
        if state["class"].as_str() != Some("transient") {
            continue;
        }
        let timeouts = state["timeout"].as_sequence().cloned().unwrap_or_default();
        for timeout in &timeouts {
            assert_eq!(
                timeout["source"].as_str(),
                Some("standard"),
                "the module state machine declares only standard timers"
            );
            declared.push(DeclaredTimeout {
                state: state["name"].as_str().expect("name").to_owned(),
                timer: timeout["timer"].as_str().expect("timer").to_owned(),
                fixed_ms: timeout["duration_ms"].as_u64(),
                on_expiry: timeout["on_expiry"].as_str().map(str::to_owned),
            });
        }
    }
    assert_eq!(declared.len(), 3, "MgmtInit, ModulePwrUp and ModulePwrDn");
    for revision in CATALOG_REVS {
        let table = for_revision(revision).expect("catalog revision");
        assert_eq!(table.timeouts.len(), declared.len(), "{revision:?}");
        for declared in &declared {
            let name = declared.state.as_str();
            let state = CmisModuleStateSpace::from_schema_name(name).unwrap();
            let generated = table
                .timeouts
                .iter()
                .find(|timeout| timeout.state == state)
                .unwrap_or_else(|| {
                    panic!("{revision:?} omits the applicable transient timer of {name}")
                });
            let FsmTimeoutSource::Standard {
                timer: generated_timer,
                duration,
            } = generated.source
            else {
                panic!("{revision:?} {name}: timer is not attributed to the standard")
            };
            assert_eq!(generated_timer, declared.timer);
            match declared.fixed_ms {
                Some(ms) => assert_eq!(duration, FsmTimerDuration::FixedMs(ms)),
                None => {
                    let schema_field = schema["states"]
                        .as_sequence()
                        .unwrap()
                        .iter()
                        .find(|s| s["name"].as_str() == Some(name))
                        .unwrap()["timeout"][0]["duration_ms_field"]
                        .as_str()
                        .expect("duration_ms_field");
                    let advertised = match duration {
                        FsmTimerDuration::AdvertisedField(field) => field,
                        other => panic!("expected an advertised-field duration, got {other:?}"),
                    };
                    assert_eq!(advertised, schema_field);
                }
            }
            let expected_expiry = declared
                .on_expiry
                .as_deref()
                .and_then(CmisModuleStateSpace::from_schema_name);
            assert_eq!(generated.on_expiry, expected_expiry, "{revision:?} {name}");
        }
    }
}

/// The host-policy block (kmod's 50 ms poll cadence and 10 s readiness
/// backstop, pinned to plugins/cmis/power.c) is emitted as named constants
/// beside the standard timers and NEVER masquerades as one: no generated
/// timeout source is a host policy, and no timer name in any table is a
/// host-policy key.
#[test]
fn host_policy_is_emitted_as_constants_and_never_masquerades_as_a_standard_timer() {
    assert_eq!(HOST_POLICY_POLL_CADENCE_MS, 50);
    assert_eq!(HOST_POLICY_READINESS_BACKSTOP_MS, 10_000);
    let schema = module_state_machine_schema();
    let host_policy = &schema["host_policy"];
    assert_eq!(host_policy["poll_cadence"]["value_ms"].as_u64(), Some(50));
    assert_eq!(
        host_policy["readiness_backstop"]["value_ms"].as_u64(),
        Some(10_000)
    );
    for revision in CATALOG_REVS {
        let table = for_revision(revision).expect("catalog revision");
        for timeout in table.timeouts {
            assert!(
                matches!(timeout.source, FsmTimeoutSource::Standard { .. }),
                "{revision:?}: a host-policy value must never be a table timer"
            );
            if let FsmTimeoutSource::Standard { timer, .. } = timeout.source {
                assert_ne!(timer, "poll_cadence", "{revision:?}");
                assert_ne!(timer, "readiness_backstop", "{revision:?}");
            }
        }
    }
}

/// Generated functions accept supplied bytes and supported revisions only:
/// a zero revision, a future revision and a malformed packed-BCD byte all
/// fail closed to None — never a default revision, never a nearest table.
#[test]
fn unsupported_revision_contexts_fail_closed() {
    use openxcvr_core::Family;
    let refused = [
        Rev {
            family: Family::Cmis,
            major: 0,
            minor: 0,
        },
        Rev {
            family: Family::Cmis,
            major: 0,
            minor: 6,
        },
        Rev {
            family: Family::Cmis,
            major: 2,
            minor: 9,
        },
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 9,
        },
        Rev {
            family: Family::Cmis,
            major: 6,
            minor: 0,
        },
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 11,
        },
    ];
    for revision in refused {
        assert_eq!(
            for_revision(revision),
            None,
            "{revision:?} must fail closed"
        );
    }
    // The packed-BCD byte entry point mirrors the refusal.
    let refused_bytes = [0x00u8, 0x06, 0x2F, 0x35, 0x59, 0x5F, 0xFF, 0x31];
    for byte in refused_bytes {
        assert_eq!(
            for_rev_major_byte(byte),
            None,
            "byte {byte:#04x} must fail closed"
        );
    }
    // Every supported revision resolves through both entry points.
    for (revision, byte) in [
        (Rev::CMIS_3_0, 0x30u8),
        (Rev::CMIS_4_0, 0x40),
        (Rev::CMIS_5_0, 0x50),
        (Rev::CMIS_5_1, 0x51),
        (Rev::CMIS_5_2, 0x52),
        (Rev::CMIS_5_3, 0x53),
        (Rev::CMIS_5_4, 0x54),
    ] {
        assert!(for_revision(revision).is_some());
        assert!(for_rev_major_byte(byte).is_some());
    }
}

/// The generated per-revision readers decode byte 3 through the same
/// Table 8-7 codes in every revision, and a missing supplied span is an
/// error — never a decoded Reserved state.
#[test]
fn generated_readers_decode_byte3_in_every_revision_and_missing_input_refuses() {
    macro_rules! assert_byte3 {
        ($module:ident, $byte3:expr, $expected:ident) => {{
            let mut buf = [0u8; 128];
            buf[3] = $byte3;
            let page = $module::CmisLowerMemory::decode(&buf);
            assert_eq!(
                page.module_state,
                openxcvr_cmis::generated::CmisModuleState::$expected,
                "{} byte3 {:#04x}",
                stringify!($module),
                $byte3
            );
            let supplied = $module::CmisLowerMemory::decode_module_state_supplied(&buf, 0);
            assert_eq!(supplied.expect("full page supplied"), page.module_state);
        }};
    }
    assert_byte3!(v3_0, 0x04, ModulePwrUp);
    assert_byte3!(v4_0, 0x04, ModulePwrUp);
    assert_byte3!(v5_0, 0x06, ModuleReady);
    assert_byte3!(v5_1, 0x08, ModulePwrDn);
    assert_byte3!(v5_2, 0x08, ModulePwrDn);
    assert_byte3!(v5_3, 0x08, ModulePwrDn);
    assert_byte3!(v5_3, 0x0A, ModuleFault);
    assert_byte3!(v5_4, 0x0A, ModuleFault);
    // A short buffer is a missing input, not a Reserved state.
    assert!(v5_3::CmisLowerMemory::decode_module_state_supplied(&[0x00, 0x00, 0x00], 0).is_err());
}

/// Guard availability is per revision: the flat-memory MgmtInit exit
/// carries the MemoryModel guard exactly where the edge exists (since
/// CMIS 4.0, asserted above), and the host-driven power-up edges carry
/// their LowPwrRequestSW / LowPwrAllowHW guards in every revision that has
/// the module state machine at all.
#[test]
fn power_up_edges_carry_their_register_guards_in_every_revision() {
    for revision in CATALOG_REVS {
        let table = for_revision(revision).expect("catalog revision");
        let sw = table
            .transitions
            .iter()
            .find(|transition| transition.id == "module_low_pwr_to_module_pwr_up_sw")
            .unwrap_or_else(|| panic!("{revision:?} lacks the SW power-up edge"));
        assert_eq!(sw.trigger.kind, FsmTriggerKind::RegisterWrite);
        assert_eq!(sw.trigger.register, Some("lower_memory.low_pwr_request_sw"));
        assert_eq!(sw.trigger.value, Some(FsmScalar::Int(0)));
        assert_eq!(
            sw.guards,
            &[FsmGuard::Equals {
                field: "lower_memory.low_pwr_allow_request_hw",
                value: FsmScalar::Bool(false),
            }]
        );
        let hw = table
            .transitions
            .iter()
            .find(|transition| transition.id == "module_low_pwr_to_module_pwr_up_hw")
            .unwrap_or_else(|| panic!("{revision:?} lacks the HW power-up edge"));
        assert_eq!(hw.trigger.kind, FsmTriggerKind::ExternalEvent);
        assert_eq!(hw.trigger.signal, Some("LowPwrRequestHW"));
        assert_eq!(
            hw.guards,
            &[
                FsmGuard::Equals {
                    field: "lower_memory.low_pwr_request_sw",
                    value: FsmScalar::Bool(false),
                },
                FsmGuard::Equals {
                    field: "lower_memory.low_pwr_allow_request_hw",
                    value: FsmScalar::Bool(true),
                },
            ]
        );
    }
}
