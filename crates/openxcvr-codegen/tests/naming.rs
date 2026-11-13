//! Round-trip naming tests. Asserts the authoritative translation
//! table and the three resolved consistency questions through the public
//! `NamingPolicy` impls, so the contract is verified from outside the crate
//! (the same surface the emitters consume).

use openxcvr_codegen::ir::naming::{
    CNaming, CSharpNaming, GoNaming, JavaNaming, NamingPolicy, PhpNaming, PythonNaming, RustNaming,
    TypeScriptNaming,
};

/// Translation table row: `field tx_bias_ma`.
#[test]
fn table_field_tx_bias_ma() {
    assert_eq!(RustNaming.field_name("tx_bias_ma"), "tx_bias_ma");
    assert_eq!(GoNaming.field_name("tx_bias_ma"), "TxBiasMa");
    assert_eq!(TypeScriptNaming.field_name("tx_bias_ma"), "txBiasMa");
    assert_eq!(JavaNaming.field_name("tx_bias_ma"), "txBiasMa");
    assert_eq!(CSharpNaming.field_name("tx_bias_ma"), "TxBiasMa");
    assert_eq!(CNaming.field_name("tx_bias_ma"), "tx_bias_ma");
    assert_eq!(PhpNaming.field_name("tx_bias_ma"), "txBiasMa");
    assert_eq!(PythonNaming.field_name("tx_bias_ma"), "tx_bias_ma");
}

/// Translation table row: `variant laser_age_media_lane`.
/// Resolved question 1 (digit boundary is N/A here) + question 2 (prefix is
/// emitter-owned; bare variant only).
#[test]
fn table_variant_laser_age_media_lane() {
    let id = "laser_age_media_lane";
    for p in [
        &RustNaming as &dyn NamingPolicy,
        &GoNaming,
        &TypeScriptNaming,
        &CSharpNaming,
        &PhpNaming,
    ] {
        assert_eq!(p.enum_variant(id), "LaserAgeMediaLane");
    }
    assert_eq!(JavaNaming.enum_variant(id), "LASER_AGE_MEDIA_LANE");
    assert_eq!(CNaming.enum_variant(id), "LASER_AGE_MEDIA_LANE");
}

/// Translation table row: `type CmisModuleState` (schema ids are PascalCase).
#[test]
fn table_type_cmis_module_state() {
    let id = "CmisModuleState";
    for p in [
        &RustNaming as &dyn NamingPolicy,
        &GoNaming,
        &TypeScriptNaming,
        &JavaNaming,
        &CSharpNaming,
    ] {
        assert_eq!(p.type_name(id), "CmisModuleState");
    }
    // C composes `openxcvr_<type_name>_t` around the snake form.
    assert_eq!(CNaming.type_name(id), "cmis_module_state");
}

/// Translation table row: `module cmis`.
#[test]
fn table_module_cmis() {
    assert_eq!(RustNaming.module_name("cmis"), "cmis");
    assert_eq!(CNaming.module_name("cmis"), "cmis");
    for p in [
        &GoNaming as &dyn NamingPolicy,
        &TypeScriptNaming,
        &JavaNaming,
        &CSharpNaming,
    ] {
        assert_eq!(p.module_name("cmis"), "Cmis");
    }
}

/// Resolved question 1 — digit boundary is source-driven: PascalCase drops the
/// source underscore, SCREAMING_SNAKE preserves it. No digit insertion.
#[test]
fn resolved_q1_digit_boundary() {
    assert_eq!(GoNaming.enum_variant("reserved_0"), "Reserved0");
    assert_eq!(RustNaming.enum_variant("reserved_0"), "Reserved0");
    assert_eq!(CNaming.enum_variant("reserved_0"), "RESERVED_0");
    assert_eq!(JavaNaming.enum_variant("reserved_0"), "RESERVED_0");
}

/// Resolved question 3 — reserved/custom ranges survive every transform.
#[test]
fn resolved_q3_reserved_and_custom_ranges() {
    assert_eq!(GoNaming.enum_variant("reserved_35_76"), "Reserved3576");
    assert_eq!(CNaming.enum_variant("reserved_35_76"), "RESERVED_35_76");
    assert_eq!(
        CSharpNaming.enum_variant("custom_observables_100_127"),
        "CustomObservables100127"
    );
    assert_eq!(
        JavaNaming.enum_variant("custom_observables_100_127"),
        "CUSTOM_OBSERVABLES_100_127"
    );
}
