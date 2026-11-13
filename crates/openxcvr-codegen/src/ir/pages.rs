use serde::{Deserialize, Deserializer};

/// Primitive wire types that can appear in the `type:` field of a PageField.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    U2,
    U3,
    U4,
    U5,
    U6,
    U7,
    U8,
    I8,
    U16Be,
    I16Be,
    U16Le,
    U32Be,
    I32Be,
    U64Le,
    U64Be,
    F16Be,
    X16Be, // polymorphic float/int union — dispatched via `variants:`
    Ascii,
    Bitmap,
    Bytes,
    Composite,
    EnumRef,
}

impl<'de> Deserialize<'de> for FieldType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        match s.as_str() {
            "bool" => Ok(FieldType::Bool),
            "u2" => Ok(FieldType::U2),
            "u3" => Ok(FieldType::U3),
            "u4" => Ok(FieldType::U4),
            "u5" => Ok(FieldType::U5),
            "u6" => Ok(FieldType::U6),
            "u7" => Ok(FieldType::U7),
            "u8" => Ok(FieldType::U8),
            "i8" => Ok(FieldType::I8),
            "u16be" => Ok(FieldType::U16Be),
            "i16be" => Ok(FieldType::I16Be),
            "u16le" => Ok(FieldType::U16Le),
            "u32be" => Ok(FieldType::U32Be),
            "i32be" => Ok(FieldType::I32Be),
            "u64le" => Ok(FieldType::U64Le),
            "u64be" => Ok(FieldType::U64Be),
            "f16be" => Ok(FieldType::F16Be),
            "x16be" => Ok(FieldType::X16Be),
            "ascii" => Ok(FieldType::Ascii),
            "bitmap" => Ok(FieldType::Bitmap),
            "bytes" => Ok(FieldType::Bytes),
            "composite" => Ok(FieldType::Composite),
            "enum_ref" => Ok(FieldType::EnumRef),
            other => Err(D::Error::unknown_variant(
                other,
                &[
                    "bool",
                    "u2",
                    "u3",
                    "u4",
                    "u5",
                    "u6",
                    "u7",
                    "u8",
                    "i8",
                    "u16be",
                    "i16be",
                    "u16le",
                    "u32be",
                    "i32be",
                    "u64le",
                    "u64be",
                    "f16be",
                    "x16be",
                    "ascii",
                    "bitmap",
                    "bytes",
                    "composite",
                    "enum_ref",
                ],
            )),
        }
    }
}

impl FieldType {
    /// Canonical schema spelling of this wire type (the inverse of the serde
    /// variant mapping), used by documentation emitters.
    pub fn wire_name(&self) -> &'static str {
        match self {
            FieldType::Bool => "bool",
            FieldType::U2 => "u2",
            FieldType::U3 => "u3",
            FieldType::U4 => "u4",
            FieldType::U5 => "u5",
            FieldType::U6 => "u6",
            FieldType::U7 => "u7",
            FieldType::U8 => "u8",
            FieldType::I8 => "i8",
            FieldType::U16Be => "u16be",
            FieldType::I16Be => "i16be",
            FieldType::U16Le => "u16le",
            FieldType::U32Be => "u32be",
            FieldType::I32Be => "i32be",
            FieldType::U64Le => "u64le",
            FieldType::U64Be => "u64be",
            FieldType::F16Be => "f16be",
            FieldType::X16Be => "x16be",
            FieldType::Ascii => "ascii",
            FieldType::Bitmap => "bitmap",
            FieldType::Bytes => "bytes",
            FieldType::Composite => "composite",
            FieldType::EnumRef => "enum_ref",
        }
    }

    /// Returns the Rust type string for simple scalar types, or None for
    /// composite / variable-length / polymorphic types that need special handling.
    pub fn rust_type(&self, enum_ref: Option<&str>) -> Option<String> {
        match self {
            FieldType::Bool => Some("bool".into()),
            FieldType::U2
            | FieldType::U3
            | FieldType::U4
            | FieldType::U5
            | FieldType::U6
            | FieldType::U7
            | FieldType::U8 => Some("u8".into()),
            FieldType::I8 => Some("i8".into()),
            FieldType::U16Be | FieldType::U16Le => Some("u16".into()),
            FieldType::I16Be => Some("i16".into()),
            FieldType::U32Be => Some("u32".into()),
            FieldType::I32Be => Some("i32".into()),
            FieldType::U64Le | FieldType::U64Be => Some("u64".into()),
            FieldType::F16Be => Some("u16".into()), // raw mantissa/exponent
            FieldType::X16Be => Some("u16".into()), // raw — caller dispatches
            FieldType::EnumRef => enum_ref.map(|e| e.to_string()),
            FieldType::Ascii | FieldType::Bytes => None, // variable length
            FieldType::Bitmap | FieldType::Composite => None,
        }
    }
}

/// collapsed-array descriptor: one field entry expands to N instances.
/// Unknown keys are rejected: a typo here would silently drop an instance
/// dimension.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstancesDesc {
    pub index: String, // loop variable name (e.g. "bank", "app", "lane")
    pub start: i64,
    pub end: i64,
}

/// Canonical permission vocabulary; legacy spellings are transitional aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Ro,
    #[serde(
        alias = "rw_nv",
        alias = "rw_triggered",
        alias = "rw_v",
        alias = "rw_with_state_guard"
    )]
    Rw,
    Rww,
    Wo,
    WoSc,
    #[serde(alias = "cor")]
    RoCor,
    #[serde(alias = "implementation_defined")]
    Unknown,
}

impl Access {
    /// Canonical schema spelling of this access value (the inverse of the
    /// serde `snake_case` mapping), used by documentation emitters.
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Rw => "rw",
            Self::Rww => "rww",
            Self::Wo => "wo",
            Self::WoSc => "wo_sc",
            Self::RoCor => "ro_cor",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_readable(self) -> Option<bool> {
        match self {
            Self::Unknown => None,
            Self::Wo | Self::WoSc => Some(false),
            _ => Some(true),
        }
    }

    pub fn is_writable(self) -> Option<bool> {
        match self {
            Self::Unknown => None,
            Self::Ro | Self::RoCor => Some(false),
            _ => Some(true),
        }
    }

    /// Preserve today's setter policy: WO needs an explicit ever-writable flag.
    pub fn emits_setter(self) -> bool {
        matches!(self, Self::Rw | Self::Rww)
    }
}

/// Generator metadata only, not the public runtime ReadEffect type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadEffectMetadata {
    None,
    ClearOnRead,
    AcknowledgeStartupInterrupt,
    Unknown,
    WriteOnly,
}

impl ReadEffectMetadata {
    /// Canonical schema spelling (the inverse of the serde mapping), used by
    /// documentation emitters.
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ClearOnRead => "clear_on_read",
            Self::AcknowledgeStartupInterrupt => "acknowledge_startup_interrupt",
            Self::Unknown => "unknown",
            Self::WriteOnly => "write_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadEffectScope {
    Field,
    Byte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSourceMetadata {
    Static,
    Dynamic,
    Unknown,
}

// Explicit null clears an inherited authority rather than meaning "omitted".
fn present_value<'de, D: Deserializer<'de>>(d: D) -> Result<Option<serde_norway::Value>, D::Error> {
    serde_norway::Value::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FieldMetadata {
    /// Value-surface policy only: false suppresses generated decoded members,
    /// getters/setters and serialization properties. Omitted/true preserves
    /// legacy emission. Never changes catalog discovery or physical authority.
    #[serde(default)]
    pub emit_value: Option<bool>,
    #[serde(default)]
    pub read_effect: Option<ReadEffectMetadata>,
    #[serde(default)]
    pub read_effect_scope: Option<ReadEffectScope>,
    #[serde(default, deserialize_with = "present_value")]
    pub read_effect_provenance: Option<serde_norway::Value>,
    #[serde(default)]
    pub change_source: Option<ChangeSourceMetadata>,
    #[serde(default, deserialize_with = "present_value")]
    pub authority: Option<serde_norway::Value>,
}

impl FieldMetadata {
    pub(crate) fn inherit(&mut self, later: &Self) {
        if later.emit_value.is_some() {
            self.emit_value = later.emit_value;
        }
        if later.read_effect.is_some() {
            self.read_effect = later.read_effect;
        }
        if later.read_effect_scope.is_some() {
            self.read_effect_scope = later.read_effect_scope;
        }
        if later.read_effect_provenance.is_some() {
            self.read_effect_provenance
                .clone_from(&later.read_effect_provenance);
        }
        if later.change_source.is_some() {
            self.change_source = later.change_source;
        }
        if later.authority.is_some() {
            self.authority.clone_from(&later.authority);
        }
    }
}

/// Physical effects and constituent coverage stay separate, even for byte effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadEffectMasks {
    pub offset: u32,
    pub covered: u8,
    pub known: Vec<(ReadEffectMetadata, u8)>,
    pub unknown: u8,
    pub uncovered: u8,
    pub write_only: u8,
}

/// The flags block on each field. Unknown keys are
/// rejected: a typo here would silently drop a permission flag.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldFlags {
    #[serde(default)]
    pub is_required: Option<bool>,
    #[serde(default)]
    pub is_readable: Option<bool>,
    #[serde(default)]
    pub is_writable: Option<bool>,
    #[serde(default)]
    pub is_ever_writable: Option<bool>,
    #[serde(default)]
    pub is_dynamic: Option<bool>,
}

/// A single field in a page schema. Unknown keys are rejected — including
/// typos of the flattened `FieldMetadata` keys: `deny_unknown_fields` and
/// `#[serde(flatten)]` coexist in this serde version (proven by the
/// version-entry types), so a key neither this struct nor `FieldMetadata`
/// declares fails loud instead of silently dropping a field override.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageField {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,

    // Addressing — either a static offset or an expression (for instances)
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub offset_expr: Option<String>,

    // Bit-packed fields
    #[serde(default)]
    pub bit_offset: Option<u32>,
    #[serde(default)]
    pub bit_offset_expr: Option<String>,
    #[serde(default)]
    pub bit_width: Option<u32>,

    #[serde(default)]
    pub size_bytes: Option<u32>,

    #[serde(rename = "type")]
    pub ty: FieldType,

    #[serde(default)]
    pub enum_ref: Option<String>,

    #[serde(default)]
    pub composite_ref: Option<String>,

    // encoding can be a plain string ("bcd") or a structured map
    // ({kind: scaled_signed, lsb_units: ...}) — keep as raw YAML Value
    #[serde(default)]
    pub encoding: Option<serde_norway::Value>,

    // Field-level physical units. Schemas record this under two spellings:
    // `unit:` (cmis, sff families) and `units:` (elsfp-1.0). The alias keeps
    // the IR from silently dropping the plural spelling; the markdown
    // reference renders the recorded string verbatim — never a derived unit.
    #[serde(default, alias = "units")]
    pub unit: Option<String>,

    #[serde(default)]
    pub access: Option<Access>,

    #[serde(flatten)]
    pub metadata: FieldMetadata,
    // Transitional emission policy captured before template metadata inheritance.
    #[serde(skip)]
    pub setter_eligible: Option<bool>,
    // Original field assertion, before legacy/template metadata expansion.
    #[serde(skip)]
    pub explicit_access: Option<Access>,

    // field category (Informational, CapabilityFlag, etc.)
    #[serde(rename = "field_type", default)]
    pub field_category: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub flags: Option<FieldFlags>,

    #[serde(default)]
    pub description: Option<String>,

    // collapsed arrays
    #[serde(default)]
    pub instances: Option<InstancesDesc>,

    // polymorphic variants — kept as raw YAML for now
    #[serde(default)]
    pub variants: Option<serde_norway::Value>,

    // conditional presence. An explicit YAML null is preserved
    // (Some(Value::Null)) — it is a malformed condition, not an omitted
    // one, and presence classification must fail loud on it.
    #[serde(default, deserialize_with = "present_value")]
    pub present_when: Option<serde_norway::Value>,

    // Inline enum definitions for bool / small-int fields
    #[serde(default)]
    pub enum_inline: Option<serde_norway::Value>,

    // Remaining metadata — kept as raw YAML
    #[serde(default)]
    pub provenance: Option<serde_norway::Value>,
    #[serde(default)]
    pub scope: Option<serde_norway::Value>,
    #[serde(default)]
    pub behavior: Option<serde_norway::Value>,
    #[serde(default)]
    pub constraints: Option<serde_norway::Value>,
    #[serde(default, rename = "default")]
    pub default_value: Option<serde_norway::Value>,
    #[serde(default)]
    pub template: Option<String>,
    // supplement can be a string or a structured map — keep as raw Value
    #[serde(default)]
    pub supplement: Option<serde_norway::Value>,

    // version metadata
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub changed_in: Option<Vec<ChangedInEntry>>,
    #[serde(default)]
    pub renamed_from: Option<RenamedFromEntry>,
    #[serde(default)]
    pub aliased_to: Option<Vec<AliasedToEntry>>,
    /// Content-switch declaration (B2): this field multiplexes the content
    /// of a data window in the same page, settling per the revision-qualified
    /// bound. Carried by selector fields (for example Page 14h
    /// `diagnostics_selector`).
    #[serde(default)]
    pub content_switch: Option<ContentSwitchDesc>,
}

/// One revision-qualified settling bound of a content switch.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentSwitchBound {
    /// First family revision the bound applies from (a valid revision
    /// string; validated at parse time).
    pub since: String,
    /// How the module signals the switch has settled:
    /// `tnack_advertised` (CMIS 4.0 § 8.11.1) or `tddcs` (CMIS 5.0+
    /// § 8.13.1 / Table 10-5).
    pub settling: String,
    /// Citation for the bound, kept verbatim.
    #[serde(default)]
    pub provenance: Option<String>,
}

/// Schema-declared content-switch metadata on a selector field (B2): the
/// inclusive `[start, end]` byte range within the page window whose content
/// this selector multiplexes, and the revision-qualified settling bound.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentSwitchDesc {
    /// Inclusive `[start, end]` window byte range (for example
    /// `[192, 255]` for the Page 14h Diagnostics Data window).
    pub data_range: [u32; 2],
    /// Settling bounds, ascending by `since` (validated at parse time).
    pub bound: Vec<ContentSwitchBound>,
}

/// A `changed_in` entry. Unknown keys are rejected (`deny_unknown_fields`,
/// also on `renamed_from`/`aliased_to` entries): the resolver applies only the
/// keys declared here, so accepting anything else would silently drop a
/// schema author's override. The flattened `FieldMetadata` keys (value-surface
/// policy and read-effect authority) and the `note` annotation remain
/// accepted.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangedInEntry {
    pub rev: String,
    /// Revision-specific sub-byte width (e.g. CMIS 4.0 TWI speed bits 3:2).
    #[serde(default)]
    pub bit_width: Option<u32>,
    /// Revision-specific meanings of the raw small-integer codes.
    #[serde(default)]
    pub enum_inline: Option<serde_norway::Value>,
    /// Legacy emitter policy captured before access alias normalization.
    #[serde(skip)]
    pub setter_eligible: Option<bool>,
    #[serde(default)]
    pub access: Option<Access>,
    #[serde(default)]
    pub flags: Option<FieldFlags>,
    #[serde(default, deserialize_with = "present_value")]
    pub provenance: Option<serde_norway::Value>,
    #[serde(default)]
    pub behavior: Option<serde_norway::Value>,
    #[serde(flatten)]
    pub metadata: FieldMetadata,
    #[serde(default)]
    pub enum_ref: Option<String>,
    #[serde(rename = "type", default)]
    pub ty: Option<FieldType>,
    /// Human annotation per : every other `changed_in` key is applied
    /// — `rev`, `bit_width`, `enum_inline`, `access`, `flags`, `provenance`,
    /// `behavior`, `enum_ref`, `type`, plus the flattened `FieldMetadata` keys
    /// `emit_value`, `read_effect`, `read_effect_scope`,
    /// `read_effect_provenance`, `change_source`, `authority` (fifteen in all) —
    /// while `note` alone is documentary: accepted for schema author clarity
    /// and inherited across partial overrides, but never applied to generated
    /// code.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenamedFromEntry {
    pub rev: String,
    pub old_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AliasedToEntry {
    pub rev: String,
    pub new_name: String,
}

/// Partial behavior maps inherit omitted keys. A present null-valued key
/// overrides that key; whole-map null deserializes as absence at the caller.
/// Non-map values retain the existing replacement semantics.
pub(crate) fn inherit_behavior(
    base: &mut Option<serde_norway::Value>,
    later: &serde_norway::Value,
) {
    if let (Some(base), Some(overrides)) = (
        base.as_mut().and_then(|b| b.as_mapping_mut()),
        later.as_mapping(),
    ) {
        base.extend(overrides.clone());
    } else {
        *base = Some(later.clone());
    }
}

impl ChangedInEntry {
    /// Equal-revision field keys override template keys; absent keys inherit.
    pub(crate) fn inherit(&mut self, later: &Self) {
        if later.access.is_some() {
            self.access = later.access;
            self.setter_eligible = later.setter_eligible;
        }
        if let Some(flags) = &later.flags {
            self.flags
                .get_or_insert_with(Default::default)
                .inherit(flags);
        }
        if later.provenance.is_some() {
            self.provenance.clone_from(&later.provenance);
        }
        if let Some(behavior) = &later.behavior {
            inherit_behavior(&mut self.behavior, behavior);
        }
        self.metadata.inherit(&later.metadata);
        if later.enum_ref.is_some() {
            self.enum_ref.clone_from(&later.enum_ref);
        }
        if later.ty.is_some() {
            self.ty.clone_from(&later.ty);
        }
        if later.bit_width.is_some() {
            self.bit_width = later.bit_width;
        }
        if later.enum_inline.is_some() {
            self.enum_inline.clone_from(&later.enum_inline);
        }
        if later.note.is_some() {
            self.note.clone_from(&later.note);
        }
    }
}

impl FieldFlags {
    pub(crate) fn inherit(&mut self, later: &Self) {
        if later.is_required.is_some() {
            self.is_required = later.is_required;
        }
        if later.is_readable.is_some() {
            self.is_readable = later.is_readable;
        }
        if later.is_writable.is_some() {
            self.is_writable = later.is_writable;
        }
        if later.is_ever_writable.is_some() {
            self.is_ever_writable = later.is_ever_writable;
        }
        if later.is_dynamic.is_some() {
            self.is_dynamic = later.is_dynamic;
        }
    }
}

impl PageField {
    pub fn emits_value(&self) -> bool {
        self.metadata.emit_value.unwrap_or(true)
    }

    pub(crate) fn normalize_legacy_access(&mut self) {
        match self
            .behavior
            .as_ref()
            .and_then(|b| b.get("latching"))
            .and_then(|v| v.as_str())
        {
            Some("clear_on_read" | "clear_on_read_when_condition_clears") => {
                self.access = Some(Access::RoCor)
            }
            Some("implementation_defined") => self.access = Some(Access::Unknown),
            _ => {}
        }
    }

    pub fn emits_setter(&self) -> bool {
        self.setter_eligible.unwrap_or_else(|| {
            self.flags
                .as_ref()
                .is_some_and(|f| f.is_ever_writable == Some(true))
                || self.access.is_some_and(Access::emits_setter)
        })
    }

    pub fn readable(&self) -> Option<bool> {
        // Unknown is unresolved, not an inherited readable=true permission or
        // a known write-only prohibition. Match writable()'s tri-state policy.
        if self.access == Some(Access::Unknown) {
            return None;
        }
        if matches!(self.access, Some(Access::Wo | Access::WoSc)) {
            return Some(false);
        }
        self.flags
            .as_ref()
            .and_then(|f| f.is_readable)
            .or_else(|| self.access.and_then(Access::is_readable))
    }

    pub fn writable(&self) -> Option<bool> {
        if matches!(self.access, Some(Access::Ro | Access::RoCor)) {
            return Some(false);
        }
        if self.access == Some(Access::Unknown) {
            return None;
        }
        self.flags
            .as_ref()
            .and_then(|f| f.is_writable)
            .or_else(|| self.access.and_then(Access::is_writable))
    }

    pub fn change_source(&self) -> Option<ChangeSourceMetadata> {
        self.metadata.change_source.or_else(|| {
            self.flags.as_ref()?.is_dynamic.map(|dynamic| {
                if dynamic {
                    ChangeSourceMetadata::Dynamic
                } else {
                    ChangeSourceMetadata::Static
                }
            })
        })
    }

    pub fn read_effects(&self) -> Vec<ReadEffectMetadata> {
        use ReadEffectMetadata::*;
        let implied = match self.access {
            Some(Access::RoCor) => Some(ClearOnRead),
            Some(Access::Wo | Access::WoSc) => Some(WriteOnly),
            Some(Access::Unknown) => Some(Unknown),
            _ => Option::None,
        };
        let mut effects: Vec<_> = self.metadata.read_effect.into_iter().collect();
        if let Some(effect) = implied {
            if !effects.contains(&effect) {
                effects.push(effect);
            }
        }
        effects
    }

    /// Effective bit width of one instance of this field: the recorded
    /// `bit_width` when present, else the width implied by the wire type,
    /// else `size_bytes * 8`. This is the single width defaulting rule —
    /// `bit_coverage` and the documentation emitter share it.
    pub fn effective_bit_width(&self) -> u32 {
        self.bit_width.unwrap_or_else(|| match self.ty {
            FieldType::Bool => 1,
            FieldType::U2 => 2,
            FieldType::U3 => 3,
            FieldType::U4 => 4,
            FieldType::U5 => 5,
            FieldType::U6 => 6,
            FieldType::U7 => 7,
            FieldType::U16Be
            | FieldType::I16Be
            | FieldType::U16Le
            | FieldType::F16Be
            | FieldType::X16Be => 16,
            FieldType::U32Be | FieldType::I32Be => 32,
            FieldType::U64Le | FieldType::U64Be => 64,
            _ => self.size_bytes.unwrap_or(1).saturating_mul(8),
        })
    }

    /// Expand addressing using the same expression evaluator as page emitters.
    pub fn bit_coverage(&self) -> Result<std::collections::BTreeMap<u32, u8>, String> {
        let mut coverage = std::collections::BTreeMap::new();
        let (var, start, end) = self
            .instances
            .as_ref()
            .map(|i| (i.index.as_str(), i.start, i.end))
            .unwrap_or(("", 0, 0));
        for index in start..=end {
            let offset = match (&self.offset_expr, self.offset) {
                (Some(expr), _) => crate::expr::compute(expr, var, index)?,
                (_, Some(offset)) => i64::from(offset),
                _ => return Err(format!("{}: missing offset", self.name)),
            };
            let bit = match &self.bit_offset_expr {
                Some(expr) => crate::expr::compute(expr, var, index)?,
                None => i64::from(self.bit_offset.unwrap_or(0)),
            };
            let width = self.effective_bit_width();
            if offset < 0 || !(0..8).contains(&bit) || width == 0 {
                return Err(format!("{}: invalid bit footprint", self.name));
            }
            for constituent in 0..i64::from(width) {
                let byte = u32::try_from(offset + (bit + constituent) / 8)
                    .map_err(|_| format!("{}: offset overflow", self.name))?;
                *coverage.entry(byte).or_default() |= 1 << ((bit + constituent) % 8);
            }
        }
        Ok(coverage)
    }

    pub fn read_effect_masks(&self) -> Result<Vec<ReadEffectMasks>, String> {
        let effects = self.read_effects();
        Ok(self
            .bit_coverage()?
            .into_iter()
            .map(|(offset, covered)| {
                let physical = if self.metadata.read_effect_scope == Some(ReadEffectScope::Byte) {
                    u8::MAX
                } else {
                    covered
                };
                ReadEffectMasks {
                    offset,
                    covered,
                    known: effects
                        .iter()
                        .filter(|e| {
                            !matches!(
                                e,
                                ReadEffectMetadata::Unknown | ReadEffectMetadata::WriteOnly
                            )
                        })
                        // None qualifies only explicitly covered constituents.
                        .map(|e| {
                            let mask = if *e == ReadEffectMetadata::None {
                                covered
                            } else {
                                physical
                            };
                            (*e, mask)
                        })
                        .collect(),
                    unknown: if self.access.is_none()
                        || effects.is_empty()
                        || effects.contains(&ReadEffectMetadata::Unknown)
                    {
                        covered
                    } else {
                        0
                    },
                    uncovered: !covered,
                    write_only: if effects.contains(&ReadEffectMetadata::WriteOnly) {
                        covered
                    } else {
                        0
                    },
                }
            })
            .collect())
    }

    /// If this field carries a `sum8` checksum encoding, returns the flat
    /// `(start, end)` byte range it covers (inclusive on both ends).
    /// Returns `None` for any other encoding or when the encoding key is absent.
    pub fn checksum_region(&self) -> Option<(u32, u32)> {
        let enc = self.encoding.as_ref()?;
        let map = enc.as_mapping()?;
        let kind = map.get("kind")?.as_str()?;
        if kind != "checksum" {
            return None;
        }
        let algo = map.get("algorithm")?.as_str()?;
        if algo != "sum8" {
            return None;
        }
        let range = map.get("covers_range")?.as_sequence()?;
        // exactly [start, end] — a 1- or 3-element list is a malformed schema,
        // not a non-checksum field; refuse it rather than silently truncating.
        if range.len() != 2 {
            return None;
        }
        let start = u32::try_from(range.first()?.as_u64()?).ok()?;
        let end = u32::try_from(range.get(1)?.as_u64()?).ok()?;
        Some((start, end))
    }

    /// True if this field can be emitted as a simple Rust struct member
    /// (fixed offset, scalar or enum type, no instancing or conditional presence).
    pub fn is_simple_scalar(&self) -> bool {
        if self.offset.is_none()
            || self.offset_expr.is_some()
            || self.instances.is_some()
            || self.variants.is_some()
        {
            return false;
        }
        // Either the type itself resolves to a Rust scalar, or there is an
        // enum_ref that lets us treat any sub-byte numeric type as that enum.
        self.enum_ref.is_some() || self.ty.rust_type(None).is_some()
    }
}

/// Spec identity block recorded on page and enum schemas (`spec:` key):
/// which specification namespace and revision a schema documents, plus any
/// errata it already incorporates.
/// Unknown keys are rejected: a typo in a spec identity block would
/// silently mis-cite the document a schema claims to follow.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecRef {
    pub id: String,
    #[serde(default)]
    pub revision: Option<String>,
    /// Errata entries: either plain source strings (index form) or
    /// `{ revision, source }` maps (schema form). Kept raw — the markdown
    /// emitter cites them verbatim.
    #[serde(default)]
    pub errata: Vec<serde_norway::Value>,
    /// Source document cited by validation-only families that read a working
    /// PDF rather than a published spec (for example
    /// `docs/stds/cmis/OIF-C-CMIS-01.4_IA.pdf`). Retained verbatim; no IR
    /// consumer yet — declared so the key is vocabulary, not a silent drop.
    #[serde(default)]
    pub source: Option<String>,
    /// annotation: parity work for this family is deferred, so
    /// it stays out of `tools/regen-all.sh` emission. Retained verbatim; no
    /// IR consumer yet — declared so the key is vocabulary, not a silent drop.
    #[serde(default)]
    pub parity_deferred: Option<bool>,
}

/// A parsed openxcvr.page.v1 schema. Unknown keys are rejected: every key
/// the tree uses is declared, so a new or misspelled page-level key fails
/// loud instead of being silently dropped.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSchema {
    pub schema: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    pub page_address: String,
    #[serde(default)]
    pub size_bytes: Option<u32>,
    #[serde(default)]
    pub field_includes: Vec<String>,
    #[serde(default)]
    pub fields: Vec<PageField>,
    /// Spec identity (`spec:` block) recorded by the schema, if any.
    #[serde(default)]
    pub spec: Option<SpecRef>,
    /// Page-level provenance (`provenance:` block) recorded by the schema.
    #[serde(default)]
    pub provenance: Option<serde_norway::Value>,
    /// Page-level conditional presence. Not modelled — the
    /// presence classifier documents page-level conditions as out of scope —
    /// but retained raw so the key is declared vocabulary instead of a
    /// silently dropped one.
    #[serde(default)]
    pub present_when: Option<serde_norway::Value>,
    /// Banked-page declaration. Consumed from
    /// the raw document by `emitters::rust::catalog::page_catalog_info`;
    /// declared here so the IR does not silently drop it.
    #[serde(default)]
    pub banking: Option<serde_norway::Value>,
    /// SFF-8472 A2h upper-page selector value (`00h`…`03h`). Consumed from
    /// the raw document by `emitters::rust::catalog::page_catalog_info`;
    /// declared here so the IR does not silently drop it.
    #[serde(default)]
    pub page_select: Option<serde_norway::Value>,
    /// Field path holding the A2h page selector value. No consumer in the
    /// repo; retained raw so the key is declared vocabulary.
    #[serde(default)]
    pub page_select_field: Option<serde_norway::Value>,
}

/// Convert flat schema offsets to offsets within the page buffer.
///
/// Lower memory and full 256-byte address-space pages (such as SFF-8472 A0h
/// and A2h) begin at byte 0. Upper-half-only 128-byte pages begin at byte 128.
pub(crate) fn page_base(schema: &PageSchema) -> i64 {
    if schema.page_address.eq_ignore_ascii_case("lower") || schema.size_bytes.unwrap_or(128) > 128 {
        0
    } else {
        128
    }
}

impl PageSchema {
    /// Value emitters use this view; catalogs and read classifiers must use
    /// the complete field set, including metadata-only fields.
    pub fn value_fields(&self) -> impl Iterator<Item = &PageField> {
        self.fields.iter().filter(|field| field.emits_value())
    }

    /// True when any field carries version metadata
    /// (`since` / `until` / `changed_in` / `renamed_from` / `aliased_to`).
    /// Such pages get per-revision resolved emission; unversioned pages
    /// have a single revision-independent field set.
    pub fn is_versioned(&self) -> bool {
        self.fields.iter().any(|f| {
            f.since.is_some()
                || f.until.is_some()
                || f.changed_in.is_some()
                || f.renamed_from.is_some()
                || f.aliased_to.is_some()
        })
    }
}
