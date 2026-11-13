//! Centralized naming policy for all code emitters.
//!
//! Every mechanical casing transform an emitter needs lives here. The casing
//! *primitives* (`pascal_segments`, `camel_lower`, `camel_to_snake`,
//! `screaming_snake`) are ported verbatim from the former per-emitter helpers so
//! output is byte-identical; the [`NamingPolicy`] trait composes them per
//! language. No emitter calls `to_pascal_case` / `to_snake_case` /
//! `snake_to_camel` / `camel_to_snake` / `upper_snake` directly — those calls
//! exist only in this module.
//!
//! Input conventions (matching the schema IR):
//! - enum/page schema `id`s are PascalCase (e.g. `CmisModuleState`),
//! - field ids and enum-entry names are snake_case (e.g. `tx_bias_ma`,
//!   `module_ready`).

use heck::{ToPascalCase, ToSnakeCase};

// ---------------------------------------------------------------------------
// Casing primitives (verbatim ports — the single home for raw transforms)
// ---------------------------------------------------------------------------

/// snake_case → PascalCase (joined, no separators).
/// Former Go/PHP `snake_to_camel`. e.g. `module_low_pwr` → `ModuleLowPwr`,
/// `reserved_0` → `Reserved0`.
pub(crate) fn pascal_segments(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// snake_case → camelCase (lowercase-first). Former PHP `snake_to_camel_lc`.
/// e.g. `temp_mon_value` → `tempMonValue`.
pub(crate) fn camel_lower(s: &str) -> String {
    let mut words = s.split('_');
    let first = words.next().unwrap_or("").to_string();
    let rest: String = words
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect();
    first + &rest
}

/// PascalCase/snake → snake_case. Former C/OpenAPI `camel_to_snake`.
/// e.g. `CmisModuleState` → `cmis_module_state` (snake input passes through).
/// `pub` (not `pub(crate)`) so the codegen binary (`main.rs`) can derive
/// generated file names from it — the last casing primitive lifted out of the
/// driver per.
pub fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && idx > 0 {
            out.push('_');
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// PascalCase/snake → SCREAMING_SNAKE. Former C `upper_snake`.
/// e.g. `CmisModuleState` → `CMIS_MODULE_STATE`.
pub(crate) fn screaming_snake(s: &str) -> String {
    camel_to_snake(s).to_ascii_uppercase()
}

// ---------------------------------------------------------------------------
// NamingPolicy trait + per-language implementations
// ---------------------------------------------------------------------------

/// The cross-language naming contract. One impl per emitter.
///
/// - `field_name`: snake field id → language field/property accessor.
/// - `enum_variant`: snake entry id → **bare** variant identifier (the emitter
///   owns any type/namespace prefix).
/// - `type_name`: PascalCase schema id → language type name (the emitter owns any
///   `_t` suffix or `openxcvr_` namespace).
/// - `module_name`: spec id → module / package / namespace name.
pub trait NamingPolicy: Send + Sync {
    fn field_name(&self, id: &str) -> String;
    fn enum_variant(&self, id: &str) -> String;
    fn type_name(&self, id: &str) -> String;
    fn module_name(&self, id: &str) -> String;
}

/// Rust — bare PascalCase variants/types, snake fields/modules (wraps `heck`).
pub struct RustNaming;
impl NamingPolicy for RustNaming {
    fn field_name(&self, id: &str) -> String {
        id.to_snake_case()
    }
    fn enum_variant(&self, id: &str) -> String {
        id.to_pascal_case()
    }
    fn type_name(&self, id: &str) -> String {
        id.to_pascal_case()
    }
    fn module_name(&self, id: &str) -> String {
        id.to_snake_case()
    }
}

/// Go — exported PascalCase fields/variants; type used as-is (already Pascal);
/// enum variants are type-prefixed by the emitter.
pub struct GoNaming;
impl NamingPolicy for GoNaming {
    fn field_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn enum_variant(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn type_name(&self, id: &str) -> String {
        id.to_string()
    }
    fn module_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
}

/// C — snake fields, SCREAMING variants (emitter adds `OPENXCVR_<TYPE>_` prefix),
/// snake type suffix (emitter adds `openxcvr_` prefix / `_t`).
pub struct CNaming;
impl NamingPolicy for CNaming {
    fn field_name(&self, id: &str) -> String {
        id.to_string()
    }
    fn enum_variant(&self, id: &str) -> String {
        id.to_ascii_uppercase()
    }
    fn type_name(&self, id: &str) -> String {
        camel_to_snake(id)
    }
    fn module_name(&self, id: &str) -> String {
        id.to_string()
    }
}

/// PHP — camelCase properties, PascalCase enum cases, class = schema id as-is.
pub struct PhpNaming;
impl NamingPolicy for PhpNaming {
    fn field_name(&self, id: &str) -> String {
        camel_lower(id)
    }
    fn enum_variant(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn type_name(&self, id: &str) -> String {
        id.to_string()
    }
    fn module_name(&self, id: &str) -> String {
        id.to_string()
    }
}

/// Python — snake everything; the canonical `id` is already idiomatic.
pub struct PythonNaming;
impl NamingPolicy for PythonNaming {
    fn field_name(&self, id: &str) -> String {
        id.to_string()
    }
    fn enum_variant(&self, id: &str) -> String {
        id.to_ascii_uppercase()
    }
    fn type_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn module_name(&self, id: &str) -> String {
        id.to_string()
    }
}

/// TypeScript — camelCase fields, PascalCase variants/types (scoped `enum`).
pub struct TypeScriptNaming;
impl NamingPolicy for TypeScriptNaming {
    fn field_name(&self, id: &str) -> String {
        camel_lower(id)
    }
    fn enum_variant(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn type_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn module_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
}

/// Java — camelCase fields, SCREAMING_SNAKE variants, PascalCase types.
pub struct JavaNaming;
impl NamingPolicy for JavaNaming {
    fn field_name(&self, id: &str) -> String {
        camel_lower(id)
    }
    fn enum_variant(&self, id: &str) -> String {
        id.to_ascii_uppercase()
    }
    fn type_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn module_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
}

/// C# — PascalCase fields/variants/types.
pub struct CSharpNaming;
impl NamingPolicy for CSharpNaming {
    fn field_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn enum_variant(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn type_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
    fn module_name(&self, id: &str) -> String {
        pascal_segments(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- primitives reproduce the former per-emitter helpers verbatim ---

    #[test]
    fn pascal_segments_matches_former_snake_to_camel() {
        assert_eq!(pascal_segments("module_ready"), "ModuleReady");
        assert_eq!(pascal_segments("reserved_0"), "Reserved0");
        assert_eq!(pascal_segments("module_low_pwr"), "ModuleLowPwr");
        assert_eq!(pascal_segments("undefined"), "Undefined");
    }

    #[test]
    fn camel_lower_matches_former_snake_to_camel_lc() {
        assert_eq!(camel_lower("sff8024_identifier"), "sff8024Identifier");
        assert_eq!(camel_lower("temp_mon_value"), "tempMonValue");
        assert_eq!(camel_lower("module_state"), "moduleState");
    }

    #[test]
    fn camel_to_snake_and_screaming_match_former_c_helpers() {
        assert_eq!(camel_to_snake("CmisModuleState"), "cmis_module_state");
        assert_eq!(screaming_snake("CmisModuleState"), "CMIS_MODULE_STATE");
    }

    // --- translation table ---

    #[test]
    fn field_name_table() {
        assert_eq!(RustNaming.field_name("tx_bias_ma"), "tx_bias_ma");
        assert_eq!(GoNaming.field_name("tx_bias_ma"), "TxBiasMa");
        assert_eq!(TypeScriptNaming.field_name("tx_bias_ma"), "txBiasMa");
        assert_eq!(JavaNaming.field_name("tx_bias_ma"), "txBiasMa");
        assert_eq!(CSharpNaming.field_name("tx_bias_ma"), "TxBiasMa");
        assert_eq!(CNaming.field_name("tx_bias_ma"), "tx_bias_ma");
        assert_eq!(PhpNaming.field_name("tx_bias_ma"), "txBiasMa");
        assert_eq!(PythonNaming.field_name("tx_bias_ma"), "tx_bias_ma");
    }

    #[test]
    fn enum_variant_table() {
        let id = "laser_age_media_lane";
        assert_eq!(RustNaming.enum_variant(id), "LaserAgeMediaLane");
        assert_eq!(GoNaming.enum_variant(id), "LaserAgeMediaLane");
        assert_eq!(TypeScriptNaming.enum_variant(id), "LaserAgeMediaLane");
        assert_eq!(CSharpNaming.enum_variant(id), "LaserAgeMediaLane");
        assert_eq!(PhpNaming.enum_variant(id), "LaserAgeMediaLane");
        assert_eq!(JavaNaming.enum_variant(id), "LASER_AGE_MEDIA_LANE");
        assert_eq!(CNaming.enum_variant(id), "LASER_AGE_MEDIA_LANE");
    }

    #[test]
    fn type_name_table() {
        let id = "CmisModuleState"; // schema ids are PascalCase
        assert_eq!(RustNaming.type_name(id), "CmisModuleState");
        assert_eq!(GoNaming.type_name(id), "CmisModuleState");
        assert_eq!(TypeScriptNaming.type_name(id), "CmisModuleState");
        assert_eq!(JavaNaming.type_name(id), "CmisModuleState");
        assert_eq!(CSharpNaming.type_name(id), "CmisModuleState");
        // C composes `openxcvr_<type_name>_t` around the snake form:
        assert_eq!(CNaming.type_name(id), "cmis_module_state");
    }

    #[test]
    fn module_name_table() {
        assert_eq!(RustNaming.module_name("cmis"), "cmis");
        assert_eq!(GoNaming.module_name("cmis"), "Cmis");
        assert_eq!(TypeScriptNaming.module_name("cmis"), "Cmis");
        assert_eq!(JavaNaming.module_name("cmis"), "Cmis");
        assert_eq!(CSharpNaming.module_name("cmis"), "Cmis");
        assert_eq!(CNaming.module_name("cmis"), "cmis");
    }

    // --- resolved question 3: reserved / custom ranges survive identically ---

    #[test]
    fn reserved_and_custom_ranges() {
        assert_eq!(GoNaming.enum_variant("reserved_35_76"), "Reserved3576");
        assert_eq!(CNaming.enum_variant("reserved_35_76"), "RESERVED_35_76");
        assert_eq!(
            PhpNaming.enum_variant("custom_observables_100_127"),
            "CustomObservables100127"
        );
        assert_eq!(
            JavaNaming.enum_variant("custom_observables_100_127"),
            "CUSTOM_OBSERVABLES_100_127"
        );
    }
}
