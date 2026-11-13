//! Field-level presence classification.
//!
//! Generation classifies every FIELD-level condition from the parsed
//! schemas into exactly one of:
//!
//! - [`EmittedPresence::Unconditional`] — no condition declared;
//! - [`EmittedPresence::Equals`] — the supported equality subset: a map
//!   `{field, equals}` or a string `"<path> == <literal>"`, where the path
//!   is a same-page field, a page-qualified `page_id.field`, or an
//!   instance selected through the referenced field's own `enum_inline`
//!   value (`field[<enum_inline name>]` — no guessed bit positions). The
//!   referenced location, bank policy, scalar kind and bit footprint are
//!   resolved during generation and travel with the condition;
//! - [`EmittedPresence::Unsupported`] — well-formed syntax outside the
//!   subset (other operators, compound expressions, numeric instance
//!   indices). Emitted verbatim; observation yields `UnsupportedField`,
//!   never a value, and never silently unconditional.
//!
//! Malformed inputs (explicit YAML null, neither-map-nor-string forms, bad
//! literals) and unresolvable references (missing, ambiguous, cyclic, or
//! not applicable at the dependent's revision) are GENERATION ERRORS
//! naming the schema file, the field and the reference.
//!
//! Page-level `present_when` is not classified here: `PageSchema` retains
//! it raw, and promoting page conditions is out of scope for this stage.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub use crate::emitters::rust::catalog::EmittedLoc;
use crate::emitters::rust::catalog::{
    bit_width_of, field_kind, field_offset, page_locations, CatalogPageInfo, PageLocation,
    SpecFamily,
};
use crate::expr::compute;
use crate::ir::pages::{ContentSwitchDesc, PageField, PageSchema};
use crate::ir::version::{applicable_revs, Rev};

/// How a content switch settles, per the revision the decode runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settling {
    /// CMIS 4.0 § 8.11.1 (printed p. 179): the selected data is updated
    /// within the advertised tNACK time after the selector has been
    /// written.
    TnackAdvertised,
    /// CMIS 5.0+ § 8.13.1 (printed p. 244) and Table 10-5 (printed p. 353):
    /// pure timing, no completion confirmation; the host waits a tDDCS
    /// guard period (at most 10 ms from WRITE STOP to READ START).
    Tddcs,
}

/// One revision-qualified settling bound, emitted for consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmittedSwitchBound {
    /// `(major, minor)` the bound applies from, inclusive.
    pub since: Option<(u8, u8)>,
    pub settling: Settling,
}

/// Emitted content-switch metadata carried by a selector prerequisite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedContentSwitch {
    /// The multiplexed data window (window offsets, `len` bytes).
    pub data: EmittedLoc,
    /// Settling bounds, ascending by `since`.
    pub bound: Vec<EmittedSwitchBound>,
}

impl EmittedContentSwitch {
    /// The settling bound in force at `rev` (the latest bound whose `since`
    /// is at or below it), or `None` below the first bound.
    pub fn settling_at(&self, rev: Rev) -> Option<Settling> {
        self.bound
            .iter()
            .rev()
            .find(|bound| match bound.since {
                None => true,
                Some((major, minor)) => Rev::parse(&format!("{major}.{minor}"))
                    .map(|since| rev >= since)
                    .unwrap_or(false),
            })
            .map(|bound| bound.settling)
    }
}

/// A resolved equality prerequisite, as emitted into the catalogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedEquals {
    /// Schema field path as written (for example
    /// `page_00h_upper_administrative.option_control_or_flag_support[rx_squelch_disable]`).
    pub field: String,
    /// Location of the referenced field (or its selected instance).
    pub loc: EmittedLoc,
    /// The referenced field's page is banked.
    pub banked: bool,
    /// FieldKind name of the referenced field, for raw-code extraction.
    pub kind: &'static str,
    pub bit_offset: u32,
    pub bit_width: u32,
    /// The raw code the prerequisite must carry (true → 1, false → 0).
    pub expected: u64,
    /// Content-switch metadata declared by the referenced field: this
    /// prerequisite selects the content of a multiplexed data window.
    pub content_switch: Option<EmittedContentSwitch>,
    /// Revisions (of the family's table) at which the referenced field
    /// applies; empty for single-revision families, where the span check
    /// does not apply.
    pub applicable: BTreeSet<Rev>,
}

/// Classified field-level presence, as emitted into the catalogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmittedPresence {
    Unconditional,
    Equals(EmittedEquals),
    Unsupported { expr: String },
}

/// The error context every generation error carries: the schema (file path
/// when known, else the page id), the dependent field, and the reference
/// when one is involved.
fn schema_label(page_id: &str, path: Option<&str>) -> String {
    path.map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .unwrap_or_else(|| page_id.to_string())
}

// Labels vary between CLI diagnostics and generated Markdown. Reference
// matching always reads the original catalog paths, never their display form.
struct PresenceContext<'a> {
    info: &'a BTreeMap<String, CatalogPageInfo>,
    render_label: &'a dyn Fn(&str, Option<&str>) -> String,
}

impl PresenceContext<'_> {
    fn label(&self, page_id: &str) -> String {
        (self.render_label)(
            page_id,
            self.info.get(page_id).and_then(|info| info.path.as_deref()),
        )
    }
}

fn err(
    context: &PresenceContext<'_>,
    page: &PageLocation,
    field: &str,
    reference: Option<&str>,
    reason: &str,
) -> String {
    match reference {
        Some(reference) => format!(
            "{}: field {}: reference {reference:?}: {reason}",
            context.label(&page.schema.id),
            field
        ),
        None => format!(
            "{}: field {}: {reason}",
            context.label(&page.schema.id),
            field
        ),
    }
}

// ─── literal and path parsing ──────────────────────────────────────────────

/// Parse an equality literal: YAML bool, YAML integer (hex `0x…` included),
/// or a string holding `true`/`false`/decimal/`0x`-hex. Everything else is
/// a malformed literal.
fn parse_literal(value: &serde_norway::Value) -> Result<u64, ()> {
    match value {
        serde_norway::Value::Bool(true) => Ok(1),
        serde_norway::Value::Bool(false) => Ok(0),
        serde_norway::Value::Number(number) => number.as_u64().ok_or(()),
        serde_norway::Value::String(text) => parse_literal_str(text.trim()),
        _ => Err(()),
    }
}

fn parse_literal_str(text: &str) -> Result<u64, ()> {
    match text {
        "true" => Ok(1),
        "false" => Ok(0),
        _ => {
            if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(());
                }
                u64::from_str_radix(hex, 16).map_err(|_| ())
            } else if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                Err(())
            } else {
                text.parse::<u64>().map_err(|_| ())
            }
        }
    }
}

/// A parsed reference path: optional page qualifier, field name, optional
/// bracket selector.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefPath {
    qualifier: Option<String>,
    name: String,
    selector: Option<String>,
}

fn valid_ident(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn parse_ref_path(text: &str) -> Result<RefPath, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty reference path".to_string());
    }
    let (base, selector) = match text.find('[') {
        Some(open) => {
            if !text.ends_with(']') || text[open + 1..].contains('[') {
                return Err(format!("malformed instance selector in {text:?}"));
            }
            let selector = &text[open + 1..text.len() - 1];
            if selector.is_empty() || selector.contains(']') {
                return Err(format!("malformed instance selector in {text:?}"));
            }
            (&text[..open], Some(selector.to_string()))
        }
        None => (text, None),
    };
    let parts: Vec<&str> = base.split('.').collect();
    if parts.len() > 2 || parts.iter().any(|part| !valid_ident(part)) {
        return Err(format!("malformed reference path {text:?}"));
    }
    Ok(RefPath {
        qualifier: (parts.len() == 2).then(|| parts[0].to_string()),
        name: parts[parts.len() - 1].to_string(),
        selector,
    })
}

/// Whether a reference path selects an instance by a bare numeric index
/// (`field[3]`): well-formed reference syntax outside the supported
/// resolution subset (enum_inline names only — no guessed bit positions).
fn has_numeric_selector(path: &str) -> bool {
    match parse_ref_path(path) {
        Ok(RefPath {
            selector: Some(selector),
            ..
        }) => !selector.is_empty() && selector.bytes().all(|b| b.is_ascii_digit()),
        _ => false,
    }
}

// ─── condition syntax classification ───────────────────────────────────────

/// The well-formed operators this classifier recognizes; anything but `==`
/// (with a clean path and literal) is Unsupported, never a decode.
const OPERATORS: &[&str] = &["==", "!=", ">=", "<=", "&&", "||", ">", "<"];

/// The raw condition, split into its syntactic class.
enum Syntax {
    /// `{field, equals}` map form.
    Map {
        field: String,
        equals: serde_norway::Value,
    },
    /// A string expression.
    Expression(String),
    /// A map that names a field but uses a different shape (operator keys,
    /// extra keys) — outside the subset.
    MapUnsupported,
    /// Not a recognizable condition at all.
    Malformed(String),
}

fn classify_syntax(value: &serde_norway::Value) -> Syntax {
    match value {
        serde_norway::Value::Mapping(map) => {
            let keys: Vec<String> = map
                .keys()
                .map(|key| match key {
                    serde_norway::Value::String(text) => text.clone(),
                    other => format!("{other:?}"),
                })
                .collect();
            let has_field = keys.iter().any(|key| key == "field");
            let has_equals = keys.iter().any(|key| key == "equals");
            if has_field && has_equals && keys.len() == 2 {
                let field = map
                    .get("field")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let equals = map
                    .get("equals")
                    .cloned()
                    .unwrap_or(serde_norway::Value::Null);
                Syntax::Map { field, equals }
            } else if has_field {
                Syntax::MapUnsupported
            } else {
                Syntax::Malformed(format!(
                    "present_when mapping without a field key: {value:?}"
                ))
            }
        }
        serde_norway::Value::String(text) => Syntax::Expression(text.clone()),
        other => Syntax::Malformed(format!(
            "present_when is neither a mapping nor a string: {other:?}"
        )),
    }
}

/// Render a raw condition verbatim for `Unsupported { expr }`.
fn render_condition(value: &serde_norway::Value) -> String {
    match value {
        serde_norway::Value::String(text) => text.clone(),
        other => format!("{other:?}"),
    }
}

// ─── reference resolution ──────────────────────────────────────────────────

/// Whether a page answers a reference qualifier: the schema id, or the
/// normalized root file stem (SFF-8636 and CMIS page schemas qualify
/// references by snake-cased file stem, SFF-8472 by its snake schema ids).
fn page_matches_qualifier(
    page: &PageLocation,
    info: &BTreeMap<String, CatalogPageInfo>,
    qualifier: &str,
) -> bool {
    if page.schema.id == qualifier {
        return true;
    }
    info.get(&page.schema.id)
        .and_then(|page_info| page_info.path.as_ref())
        .map(|path| {
            std::path::Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.replace('-', "_") == qualifier)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Resolve one prerequisite reference to its concrete location shape.
struct ResolvedRef {
    page_index: usize,
    field: PageField,
    loc: EmittedLoc,
    bit_offset: u32,
    bit_width: u32,
}

fn resolve_reference(
    pages: &[PageLocation],
    dependent: &PageLocation,
    info: &BTreeMap<String, CatalogPageInfo>,
    reference: &str,
) -> Result<ResolvedRef, String> {
    let path = parse_ref_path(reference)?;
    let page_index = match &path.qualifier {
        // The unqualified branch selects the dependent's own page by exact
        // schema id; the validator rejects duplicate page ids before
        // classification runs, so at most one page can match here.
        None => pages
            .iter()
            .position(|p| p.schema.id == dependent.schema.id)
            .ok_or_else(|| format!("unknown page {:?}", dependent.schema.id))?,
        // A qualifier can be answered by more than one page — one page's
        // schema id can equal another page's snake-cased file stem — so
        // collect every match: zero is an unknown page, more than one an
        // ambiguous reference, never a silent first-match pick.
        Some(qualifier) => {
            let matching: Vec<usize> = pages
                .iter()
                .enumerate()
                .filter(|(_, page)| page_matches_qualifier(page, info, qualifier))
                .map(|(index, _)| index)
                .collect();
            match matching.as_slice() {
                [] => return Err(format!("unknown page {qualifier:?}")),
                [page_index] => *page_index,
                _ => {
                    let ids: Vec<&str> = matching
                        .iter()
                        .map(|&index| pages[index].schema.id.as_str())
                        .collect();
                    return Err(format!(
                        "ambiguous page qualifier {qualifier:?}: matches pages {}",
                        ids.join(", ")
                    ));
                }
            }
        }
    };
    let page = &pages[page_index];
    let field = page
        .schema
        .fields
        .iter()
        .find(|f| f.name == path.name)
        .ok_or_else(|| format!("unknown field reference {reference:?}"))?;
    let (offset, bit_offset) = match &path.selector {
        None => {
            if field.instances.is_some() {
                return Err(format!(
                    "selects no instance of the instanced field {:?}",
                    field.name
                ));
            }
            (
                field_offset(field, None).map_err(|e| e.to_string())?,
                field.bit_offset.unwrap_or(0),
            )
        }
        Some(selector) => {
            let instances = field.instances.as_ref().ok_or_else(|| {
                format!(
                    "selects an instance, but field {:?} has no instances",
                    field.name
                )
            })?;
            // Resolve through the field's own enum_inline: name → value →
            // instance, never a guessed bit position.
            let entries = field
                .enum_inline
                .as_ref()
                .and_then(|v| v.as_sequence())
                .ok_or_else(|| {
                    format!(
                        "reference {reference:?} selects an instance, but field {:?} has no enum_inline",
                        field.name
                    )
                })?;
            let matching: Vec<i64> = entries
                .iter()
                .filter_map(|entry| {
                    let map = entry.as_mapping()?;
                    if map.get("name").and_then(|v| v.as_str()) == Some(selector.as_str()) {
                        map.get("value").and_then(|v| v.as_i64())
                    } else {
                        None
                    }
                })
                .collect();
            match matching.as_slice() {
                [] => {
                    return Err(format!(
                        "enum_inline name {selector:?} not found on field {:?}",
                        field.name
                    ))
                }
                [value] => {
                    let value = *value;
                    if value < instances.start || value > instances.end {
                        return Err(format!(
                            "enum_inline name {selector:?} (value {value}) is outside the \
                             instances {}..={} of field {:?}",
                            instances.start, instances.end, field.name
                        ));
                    }
                    let offset = match &field.offset_expr {
                        Some(expr) => {
                            compute(expr, &instances.index, value).map_err(|e| e.to_string())?
                        }
                        None => field_offset(field, None).map_err(|e| e.to_string())?,
                    };
                    let bit = match &field.bit_offset_expr {
                        Some(expr) => {
                            compute(expr, &instances.index, value).map_err(|e| e.to_string())?
                        }
                        None => i64::from(field.bit_offset.unwrap_or(0)),
                    };
                    (
                        offset,
                        u32::try_from(bit.max(0)).map_err(|_| "bit offset overflow".to_string())?,
                    )
                }
                _ => {
                    return Err(format!(
                        "enum_inline name {selector:?} is ambiguous on field {:?}",
                        field.name
                    ))
                }
            }
        }
    };
    let len = u16::try_from(field.size_bytes.unwrap_or(1)).unwrap_or(u16::MAX);
    let loc = EmittedLoc {
        address: page.address,
        page: page.page,
        offset: u16::try_from(offset)
            .map_err(|_| format!("offset overflow in reference {reference:?}"))?,
        len,
    };
    let bit_width = u32::try_from(bit_width_of(field))
        .map_err(|_| format!("bit width overflow in reference {reference:?}"))?;
    Ok(ResolvedRef {
        page_index,
        field: field.clone(),
        loc,
        bit_offset,
        bit_width,
    })
}

/// Resolve a field's content_switch declaration onto its page location.
fn resolve_content_switch(
    page: &PageLocation,
    desc: &ContentSwitchDesc,
) -> Result<EmittedContentSwitch, String> {
    let [start, end] = desc.data_range;
    if end < start || end > 255 {
        return Err(format!(
            "content_switch data_range [{start}, {end}] is malformed"
        ));
    }
    let mut bound = Vec::with_capacity(desc.bound.len());
    for entry in &desc.bound {
        let rev = Rev::parse(&entry.since)
            .map_err(|_| format!("invalid bound revision {:?}", entry.since))?;
        let settling = match entry.settling.as_str() {
            "tnack_advertised" => Settling::TnackAdvertised,
            "tddcs" => Settling::Tddcs,
            other => {
                return Err(format!(
                    "unknown settling {other:?}; expected tnack_advertised or tddcs"
                ))
            }
        };
        bound.push(EmittedSwitchBound {
            since: Some((rev_major(rev), rev_minor(rev))),
            settling,
        });
    }
    Ok(EmittedContentSwitch {
        data: EmittedLoc {
            address: page.address,
            page: page.page,
            offset: u16::try_from(start).map_err(|_| "data_range start overflow".to_string())?,
            len: u16::try_from(end - start + 1)
                .map_err(|_| "data_range length overflow".to_string())?,
        },
        bound,
    })
}

fn rev_major(rev: Rev) -> u8 {
    rev.as_str().split_once('.').unwrap().0.parse().unwrap()
}

fn rev_minor(rev: Rev) -> u8 {
    rev.as_str().split_once('.').unwrap().1.parse().unwrap()
}

// ─── classification ────────────────────────────────────────────────────────

/// Classify every field-level condition of every page. The returned map is
/// keyed by (page schema id, field name); fields without a condition are
/// absent from it (Unconditional by omission).
pub(crate) fn classify_presence(
    pages: &[PageLocation],
    family: &SpecFamily,
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<BTreeMap<(String, String), EmittedPresence>, String> {
    classify_presence_with_context(
        pages,
        family,
        &PresenceContext {
            info,
            render_label: &schema_label,
        },
    )
}

fn classify_presence_with_context(
    pages: &[PageLocation],
    family: &SpecFamily,
    context: &PresenceContext<'_>,
) -> Result<BTreeMap<(String, String), EmittedPresence>, String> {
    let info = context.info;
    // Pass 1 — syntactic classification and reference resolution. Edges
    // record the Equals reference graph for cycle detection.
    let mut classified: BTreeMap<(String, String), EmittedPresence> = BTreeMap::new();
    let mut edges: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    for (page_index, page) in pages.iter().enumerate() {
        for field in &page.schema.fields {
            let Some(raw) = &field.present_when else {
                continue;
            };
            if matches!(raw, serde_norway::Value::Null) {
                return Err(err(
                    context,
                    page,
                    &field.name,
                    None,
                    "present_when is explicitly null; declare a condition or omit the key",
                ));
            }
            let syntax = classify_syntax(raw);
            let presence = match syntax {
                Syntax::Malformed(reason) => {
                    return Err(err(context, page, &field.name, None, &reason));
                }
                Syntax::MapUnsupported => EmittedPresence::Unsupported {
                    expr: render_condition(raw),
                },
                Syntax::Expression(text) => {
                    let has_other_operator = OPERATORS
                        .iter()
                        .filter(|op| **op != "==")
                        .any(|op| text.contains(op));
                    if has_other_operator || !text.contains("==") {
                        EmittedPresence::Unsupported { expr: text }
                    } else {
                        // Exactly one `==` (the first); a second one makes
                        // the literal unparseable and fails below.
                        let (path, literal) = text
                            .split_once("==")
                            .map(|(path, literal)| (path.trim(), literal.trim()))
                            .unwrap_or((text.as_str(), ""));
                        let expected =
                            parse_literal(&serde_norway::Value::String(literal.to_string()))
                                .map_err(|_| {
                                    err(
                                        context,
                                        page,
                                        &field.name,
                                        Some(path),
                                        &format!(
                                            "literal {literal:?} is not true/false/decimal/0x-hex"
                                        ),
                                    )
                                })?;
                        if has_numeric_selector(path) {
                            EmittedPresence::Unsupported { expr: text }
                        } else {
                            let resolved =
                                resolve_reference(pages, page, info, path).map_err(|reason| {
                                    err(context, page, &field.name, Some(path), &reason)
                                })?;
                            let target_page = resolved.page_index;
                            let presence = finish_equals(
                                pages, page_index, family, context, field, path, expected, resolved,
                            )?;
                            record_edge(&mut edges, page, field, pages, target_page, path);
                            presence
                        }
                    }
                }
                Syntax::Map {
                    field: path,
                    equals,
                } => {
                    let expected = parse_literal(&equals).map_err(|_| {
                        err(
                            context,
                            page,
                            &field.name,
                            Some(&path),
                            &format!("literal {equals:?} is not true/false/decimal/0x-hex"),
                        )
                    })?;
                    if has_numeric_selector(&path) {
                        EmittedPresence::Unsupported {
                            expr: render_condition(raw),
                        }
                    } else {
                        let resolved =
                            resolve_reference(pages, page, info, &path).map_err(|reason| {
                                err(context, page, &field.name, Some(&path), &reason)
                            })?;
                        let target_page = resolved.page_index;
                        let presence = finish_equals(
                            pages, page_index, family, context, field, &path, expected, resolved,
                        )?;
                        record_edge(&mut edges, page, field, pages, target_page, &path);
                        presence
                    }
                }
            };
            classified.insert((page.schema.id.clone(), field.name.clone()), presence);
        }
    }

    // Pass 2 — cycle detection over the Equals reference graph. A cyclic
    // condition graph is nonsense schema (A present iff B == x while B is
    // present iff A == y); fail loud naming the cycle.
    for start in edges.keys() {
        let mut path = vec![start.clone()];
        let mut on_stack = BTreeSet::from([start.clone()]);
        if let Some(cycle) = walk_edges(&edges, start, &mut path, &mut on_stack) {
            let rendered: Vec<String> = cycle
                .iter()
                .map(|(page, field)| format!("{page}::{field}"))
                .collect();
            return Err(format!(
                "present_when references form a cycle: {}",
                rendered.join(" -> ")
            ));
        }
    }

    Ok(classified)
}

/// Record one Equals reference edge: dependent (page, field) → referenced
/// (page, bare field name).
fn record_edge(
    edges: &mut BTreeMap<(String, String), (String, String)>,
    page: &PageLocation,
    field: &PageField,
    pages: &[PageLocation],
    target_page: usize,
    path: &str,
) {
    edges.insert(
        (page.schema.id.clone(), field.name.clone()),
        (
            pages[target_page].schema.id.clone(),
            referenced_field_name(path),
        ),
    );
}

/// DFS over the reference edges from `node`; `Some(cycle)` when a chain
/// leads back to a node on the current stack.
fn walk_edges(
    edges: &BTreeMap<(String, String), (String, String)>,
    node: &(String, String),
    path: &mut Vec<(String, String)>,
    on_stack: &mut BTreeSet<(String, String)>,
) -> Option<Vec<(String, String)>> {
    let target = edges.get(node)?;
    if on_stack.contains(target) {
        path.push(target.clone());
        return Some(path.clone());
    }
    // Only Equals-classified fields continue the chain; a target that is
    // terminal (unconditional or unsupported) cannot cycle.
    if !edges.contains_key(target) {
        return None;
    }
    path.push(target.clone());
    on_stack.insert(target.clone());
    let found = walk_edges(edges, target, path, on_stack);
    on_stack.remove(target);
    if found.is_none() {
        path.pop();
    }
    found
}

/// The bare field name of a reference path (qualifier and selector stripped).
fn referenced_field_name(reference: &str) -> String {
    let base = reference.split('[').next().unwrap_or(reference);
    base.rsplit('.').next().unwrap_or(base).to_string()
}

/// Assemble the Equals presence for a resolved reference, validating the
/// literal against the prerequisite's width and carrying the referenced
/// field's content-switch metadata and applicability.
#[allow(clippy::too_many_arguments)]
fn finish_equals(
    pages: &[PageLocation],
    page_index: usize,
    family: &SpecFamily,
    context: &PresenceContext<'_>,
    field: &PageField,
    path: &str,
    expected: u64,
    resolved: ResolvedRef,
) -> Result<EmittedPresence, String> {
    let page = &pages[page_index];
    if resolved.bit_width < 64 && expected >> resolved.bit_width != 0 {
        return Err(err(
            context,
            page,
            &field.name,
            Some(path),
            &format!(
                "expected value {expected} does not fit the prerequisite's {} bits",
                resolved.bit_width
            ),
        ));
    }
    let referenced_page = &pages[resolved.page_index];
    let content_switch = resolved
        .field
        .content_switch
        .as_ref()
        .map(|desc| {
            resolve_content_switch(referenced_page, desc).map_err(|reason| {
                err(
                    context,
                    page,
                    &field.name,
                    Some(path),
                    &format!("referenced field carries {reason}"),
                )
            })
        })
        .transpose()?;
    // Applicability of the prerequisite across the family's revision table
    // (multi-revision families); single-revision families keep an empty set
    // and skip the per-span check, mirroring derive_field_rows' span model.
    let applicable = if family.single_rev() {
        BTreeSet::new()
    } else {
        applicable_revs(&resolved.field, &family.rev_domain())
    };
    Ok(EmittedPresence::Equals(EmittedEquals {
        field: path.to_string(),
        loc: resolved.loc,
        banked: referenced_page.banked,
        kind: field_kind(&resolved.field.ty),
        bit_offset: resolved.bit_offset,
        bit_width: resolved.bit_width,
        expected,
        content_switch,
        applicable,
    }))
}

/// The owning namespace of a schema file: the first path component below
/// the schemas root (the spec directory that owns the file). The single
/// owner of this rule; the Markdown tree emitter's `spec_of` grouping
/// delegates to it so schema ownership can never drift between the docs
/// tree and validation. `None` when the file is not under the root.
pub fn namespace_of(schemas_root: &Path, path: &Path) -> Option<String> {
    let norm_root = crate::parser::index::normalize_path(schemas_root);
    let norm_path = crate::parser::index::normalize_path(path);
    norm_path
        .strip_prefix(&norm_root)
        .ok()
        .and_then(|rel| rel.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
}

/// Classify one family spec's schemas (locate → classify). The per-namespace
/// entry point below calls this once per owning namespace that has a
/// [`SpecFamily`], so every `--lang` run rejects malformed or unresolvable
/// conditions, not only the catalog emitters.
pub fn validate_presence(
    schemas: &[PageSchema],
    spec: &str,
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<(), String> {
    validate_presence_with_context(
        schemas,
        spec,
        &PresenceContext {
            info,
            render_label: &schema_label,
        },
    )
}

fn validate_presence_with_context(
    schemas: &[PageSchema],
    spec: &str,
    context: &PresenceContext<'_>,
) -> Result<(), String> {
    let family = crate::emitters::rust::catalog::spec_family(spec)
        .ok_or_else(|| format!("presence classification does not know spec {spec:?}"))?;
    let pages = page_locations(schemas, spec, context.info)?;
    classify_presence_with_context(&pages, family, context)?;
    Ok(())
}

/// Validate field-level presence conditions for every loaded page schema,
/// grouped by OWNING namespace — the spec directory that owns the file, not
/// the requested `--spec`. A supplement run (e.g. `--spec c-cmis-1.4`)
/// imports `../cmis/_index.yaml`; those pages classify under their own
/// family, with their own namespace's pages as the resolution context —
/// the same input set a `--spec cmis` run classifies. A namespace without a
/// [`SpecFamily`] has no revision model to classify against, so its pages
/// may not declare `present_when` at all; a declaration is an explicit
/// error naming the namespace, page and field. Runs with `--spec` omitted
/// validate exactly the same way.
pub fn validate_presence_by_namespace(
    schemas_root: &Path,
    page_pairs: &[(std::path::PathBuf, PageSchema)],
    info: &BTreeMap<String, CatalogPageInfo>,
) -> Result<(), String> {
    validate_presence_by_namespace_with_labels(schemas_root, page_pairs, info, &schema_label)
}

pub(crate) fn validate_presence_by_namespace_with_labels(
    schemas_root: &Path,
    page_pairs: &[(std::path::PathBuf, PageSchema)],
    info: &BTreeMap<String, CatalogPageInfo>,
    render_label: &dyn Fn(&str, Option<&str>) -> String,
) -> Result<(), String> {
    let context = PresenceContext { info, render_label };
    let mut by_namespace: BTreeMap<String, Vec<&PageSchema>> = BTreeMap::new();
    for (path, schema) in page_pairs {
        let namespace = namespace_of(schemas_root, path).unwrap_or_else(|| "ungrouped".to_string());
        by_namespace.entry(namespace).or_default().push(schema);
    }

    for (namespace, schemas) in &by_namespace {
        if crate::emitters::rust::catalog::spec_family(namespace).is_some() {
            let owned: Vec<PageSchema> = schemas.iter().map(|s| (*s).clone()).collect();
            validate_presence_with_context(&owned, namespace, &context)?;
            continue;
        }
        for schema in schemas {
            for field in &schema.fields {
                if field.present_when.is_some() {
                    let label = context.label(&schema.id);
                    return Err(format!(
                        "namespace {namespace:?} has no presence family (SpecFamily), so \
                         presence conditions are not supported for this namespace, but page \
                         {} ({label}) field {:?} declares present_when",
                        schema.id, field.name
                    ));
                }
            }
        }
    }
    Ok(())
}
