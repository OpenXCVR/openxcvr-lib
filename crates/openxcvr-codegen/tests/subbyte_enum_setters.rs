//! Writable sub-byte enum setters — real compile-and-execute proof (ADR review
//! disposition B5).
//!
//! `encode_at`'s contract says sub-byte writes are read-modify-write so
//! neighbouring bits survive. These tests do not stop at substring assertions:
//! they take the *actual* RustEmitter output (enum definition + setter),
//! compile it with rustc, and execute an exhaustive check — every background
//! byte (all 256 patterns, covering 0x00 / 0xff / mixed), every valid enum
//! value, for Bool and u2..u7 at valid nonzero bit offsets, plus the
//! instanced nibble-pair shape that CMIS page 13h ships today.
//!
//! Controls pin the unchanged full-width surfaces: the full-byte (u8-backed)
//! enum setter stays a whole-byte write and the two-byte enum setter stays a
//! big-endian u16 `copy_from_slice`, exactly as before this repair.

use openxcvr_codegen::emitters::rust::{pages::emit_page, RustEmitter};
use openxcvr_codegen::emitters::Emitter;
use openxcvr_codegen::ir::enums::{EntryValue, EnumEntry, EnumSchema};
use openxcvr_codegen::parser::pages::parse_page_schema;

fn page(fields: &str) -> openxcvr_codegen::ir::pages::PageSchema {
    parse_page_schema(&format!(
        "schema: openxcvr.page.v2\nkind: page\nid: Synthetic\ntitle: Synthetic\npage_address: lower\nsize_bytes: 16\nfields:\n{fields}"
    ))
    .unwrap()
}

/// Bit width of a field type name as written in the synthetic YAML.
fn width_of(ty: &str) -> u32 {
    match ty {
        "bool" => 1,
        other => other[1..].parse().unwrap(),
    }
}

/// An enum whose single entries cover every value in `0..=max` (all values
/// valid at the field's width), so the exhaustive loops below exercise every
/// representable value, not a sample.
fn full_enum(id: &str, max: u64) -> EnumSchema {
    let entries = (0..=max)
        .map(|v| EnumEntry {
            name: format!("e{v}"),
            value: EntryValue::Single(v),
            description: None,
            aliases: vec![],
            tags: vec![],
            signaling: None,
            reach: None,
            bit_rate_gbps: None,
            vendor_specific: None,
            since: None,
            until: None,
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
            provenance: None,
        })
        .collect();
    EnumSchema {
        spec: None,
        provenance: None,
        description: None,
        schema: "openxcvr.enum.v2".to_string(),
        kind: "enum".to_string(),
        id: id.to_string(),
        title: id.to_string(),
        entry_includes: vec![],
        entries,
    }
}

fn extra_entry(name: &str, value: u64) -> EnumEntry {
    EnumEntry {
        name: name.to_string(),
        value: EntryValue::Single(value),
        description: None,
        aliases: vec![],
        tags: vec![],
        signaling: None,
        reach: None,
        bit_rate_gbps: None,
        vendor_specific: None,
        since: None,
        until: None,
        changed_in: None,
        renamed_from: None,
        aliased_to: None,
        provenance: None,
    }
}

/// Slice one `pub fn set_<name>` out of an emitted page, brace-matched, so the
/// compiled harness runs the real emitted setter body verbatim.
fn setter_fn(page_src: &str, name: &str) -> String {
    let sig = format!("pub fn set_{name}(");
    let start = page_src
        .find(&sig)
        .unwrap_or_else(|| panic!("setter set_{name} missing from emitted page\n---\n{page_src}"));
    let body_start = start + page_src[start..].find('{').unwrap();
    let mut depth = 0i32;
    for (i, c) in page_src[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return page_src[start..body_start + i + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("setter set_{name} has no closing brace\n---\n{page_src}");
}

/// Compile a self-contained snippet (enum defs + setters + `fn main`) and run
/// it. Any wrong byte makes the binary panic; its stderr is surfaced here.
fn compile_and_run(case: &str, source: String) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join(format!("{case}.rs"));
    let bin = dir.path().join(case);
    std::fs::write(&src, source).unwrap();
    let compiled = std::process::Command::new("rustc")
        .arg("--edition=2021")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{case}: generated code failed to compile\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let executed = std::process::Command::new(&bin).output().unwrap();
    assert!(
        executed.status.success(),
        "{case}: generated setter misbehaved\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&executed.stdout),
        String::from_utf8_lossy(&executed.stderr)
    );
}

/// Exhaustive masked-RMW checker compiled into every harness: for all 256
/// background bytes × all field values, the setter must leave the byte equal
/// to `(bg & !(mask << bo)) | ((value & mask) << bo)` — neighbours preserved
/// *and* the value lands at exactly the field's bit span.
const CHECKER: &str = r#"
fn check<const BO: u8, const MASK: u8>(field: &str, byte: usize, set: impl Fn(&mut [u8; 16], u8)) {
    for bg in 0u8..=255 {
        for v in 0u8..=MASK {
            let mut buf = [0u8; 16];
            buf[byte] = bg;
            set(&mut buf, v);
            let expect = (bg & !(MASK << BO)) | ((v & MASK) << BO);
            assert_eq!(
                buf[byte], expect,
                "{field}: bg={bg:#04x} value={v} wrote {:#04x}, RMW expected {expect:#04x} (neighbour bits clobbered or wrong bit span)",
                buf[byte]
            );
        }
    }
}
"#;

/// (field, type, bit_offset, enum max value) — one field per sub-byte width,
/// each at a valid nonzero bit offset, each on its own byte.
/// Group A uses the maximum valid nonzero offset for each width.
const GROUP_A: &[(&str, &str, u32, u64)] = &[
    ("f_w1", "bool", 7, 1),
    ("f_w2", "u2", 6, 3),
    ("f_w3", "u3", 5, 7),
    ("f_w4", "u4", 4, 15),
    ("f_w5", "u5", 3, 31),
    ("f_w6", "u6", 2, 63),
    ("f_w7", "u7", 1, 127),
];

/// Group B: different valid nonzero offsets (u4 and u7 each have only one).
const GROUP_B: &[(&str, &str, u32, u64)] = &[
    ("g_w1", "bool", 2, 1),
    ("g_w2", "u2", 3, 3),
    ("g_w3", "u3", 3, 7),
    ("g_w4", "u4", 4, 15),
    ("g_w5", "u5", 2, 31),
    ("g_w6", "u6", 1, 63),
    ("g_w7", "u7", 1, 127),
];

fn group_case(case: &str, group: &[(&str, &str, u32, u64)]) {
    let yaml_fields: String = group
        .iter()
        .enumerate()
        .map(|(i, (name, ty, bo, _))| {
            format!(
                "  - {{ name: {name}, offset: {i}, bit_offset: {bo}, bit_width: {w}, type: {ty}, enum_ref: {case}_{name}, access: rw }}\n",
                w = width_of(ty)
            )
        })
        .collect();
    let schema = page(&yaml_fields);
    let page_src = emit_page(&schema);

    let mut source = String::new();
    for (name, _, _, max) in group {
        source.push_str(&RustEmitter.emit_enum(&full_enum(&format!("{case}_{name}"), *max)));
        source.push('\n');
        source.push_str(&setter_fn(&page_src, name));
        source.push('\n');
    }
    source.push_str(CHECKER);
    source.push_str("fn main() {\n");
    for (i, (name, ty, bo, _)) in group.iter().enumerate() {
        source.push_str(&format!(
            "    check::<{bo}, {mask}>(\"{name}\", {i}, |b, v| set_{name}(b, {case}_{name}::from(v)));\n",
            mask = (1u16 << width_of(ty)) - 1
        ));
    }
    source.push_str("}\n");
    compile_and_run(case, source);
}

#[test]
fn subbyte_enum_setters_preserve_neighbours_at_max_offsets() {
    group_case("group_a", GROUP_A);
}

#[test]
fn subbyte_enum_setters_preserve_neighbours_at_varied_offsets() {
    group_case("group_b", GROUP_B);
}

/// The exact corpus shape: CMIS page 13h pattern-select fields are *instanced*
/// u4 enums where lane pairs share a byte (`offset_expr`/`bit_offset_expr`),
/// `access: rw`. Odd lanes sit in the high nibble — the shipped setter wrote
/// the whole byte and destroyed the even-lane nibble (and vice versa).
#[test]
fn instanced_u4_enum_setter_preserves_paired_nibble() {
    let schema = page(
        "  - name: pattern\n    instances: { index: lane, start: 1, end: 8 }\n    offset_expr: \"(lane - 1) / 2\"\n    bit_offset_expr: \"((lane - 1) % 2) * 4\"\n    bit_width: 4\n    type: u4\n    enum_ref: PairPattern\n    access: rw\n",
    );
    let page_src = emit_page(&schema);
    // Settled mutator contract: the buffer parameter is the page's fixed-size
    // `&mut [u8; 16]`, and the instanced setter validates its index and
    // returns a typed error instead of silently ignoring it.
    let setter = setter_fn(&page_src, "pattern");
    assert!(
        setter.starts_with("pub fn set_pattern(buf: &mut [u8; 16], index: usize, value: PairPattern) -> Result<(), openxcvr_core::supplied::SpanError>"),
        "instanced setter contract changed\n---\n{setter}"
    );
    assert!(
        setter.contains("_ => Err(openxcvr_core::supplied::SpanError::InvalidIndex(index as i64))"),
        "instanced setter must refuse out-of-range indices with a typed error\n---\n{setter}"
    );
    let mut source = String::new();
    source.push_str(&RustEmitter.emit_enum(&full_enum("PairPattern", 15)));
    source.push('\n');
    // Harness-only shim for the crate the generated path names: the setter is
    // compiled verbatim, so `openxcvr_core::supplied::SpanError` must resolve
    // in the standalone rustc invocation like it does inside the crate.
    source.push_str(
        r#"
mod openxcvr_core {
    pub mod supplied {
        #[derive(Debug)]
        pub enum SpanError {
            InvalidIndex(i64),
        }
    }
}
"#,
    );
    source.push('\n');
    source.push_str(&setter);
    source.push_str(
        r#"
fn main() {
    // index 0 = lane 1 (low nibble), index 1 = lane 2 (high nibble), same byte.
    for bg in 0u8..=255 {
        for v in 0u8..=15 {
            let mut buf = [0u8; 16];
            buf[0] = bg;
            set_pattern(&mut buf, 1, PairPattern::from(v)).unwrap();
            assert_eq!(buf[0], (bg & 0x0f) | (v << 4),
                "high-nibble lane clobbered low neighbour: bg={bg:#04x} v={v} -> {:#04x}", buf[0]);
            let mut buf = [0u8; 16];
            buf[0] = bg;
            set_pattern(&mut buf, 0, PairPattern::from(v)).unwrap();
            assert_eq!(buf[0], (bg & 0xf0) | v,
                "low-nibble lane clobbered high neighbour: bg={bg:#04x} v={v} -> {:#04x}", buf[0]);
            // Out-of-range index: typed error, no write, no panic.
            let mut buf = [0u8; 16];
            buf[0] = bg;
            assert!(set_pattern(&mut buf, 8, PairPattern::from(v)).is_err());
            assert_eq!(buf[0], bg, "refused index must not touch the buffer");
        }
    }
}
"#,
    );
    compile_and_run("instanced_pair", source);
}

/// Raw-setter overflow semantics must stay consistent with non-enum raw
/// setters: an out-of-range repr is masked into the field width, never
/// refused and never spilled into neighbouring bits.
#[test]
fn subbyte_enum_setter_masks_overflow_like_nonenum_raw_setters() {
    let schema = page(
        "  - { name: of_w2, offset: 0, bit_offset: 4, bit_width: 2, type: u2, enum_ref: OvfEnum, access: rw }\n  - { name: of_n2, offset: 1, bit_offset: 4, bit_width: 2, type: u2, access: rw }\n",
    );
    let page_src = emit_page(&schema);
    let mut source = String::new();
    source.push_str(&RustEmitter.emit_enum(&full_enum("OvfEnum", 3)));
    source.push('\n');
    source.push_str(&setter_fn(&page_src, "of_w2"));
    source.push_str(&setter_fn(&page_src, "of_n2"));
    source.push_str(
        r#"
fn main() {
    for bg in 0u8..=255 {
        // repr beyond a u2 field (catch-all variant): masked to the width,
        // exactly like the non-enum raw u2 setter does with value 0xFF.
        let mut ebuf = [0u8; 16];
        ebuf[0] = bg;
        set_of_w2(&mut ebuf, OvfEnum::from(0xff));
        let mut nbuf = [0u8; 16];
        nbuf[1] = bg;
        set_of_n2(&mut nbuf, 0xff);
        assert_eq!(ebuf[0], nbuf[1],
            "enum overflow repr {:#04x} diverged from non-enum raw mask: enum={:#04x} nonenum={:#04x} (bg={bg:#04x})",
            0xffu8, ebuf[0], nbuf[1]);
    }
}
"#,
    );
    compile_and_run("overflow_mask", source);
}

/// Control: the full-byte (u8-backed) enum setter remains a whole-byte write —
/// neighbouring bits are *replaced*, the byte equals repr exactly. Unmapped
/// reprs (catch-all variants) are written raw, not refused.
#[test]
fn full_byte_enum_setter_is_whole_byte_write_unchanged() {
    let schema =
        page("  - { name: ctl_u8, offset: 0, size_bytes: 1, type: u8, enum_ref: FullEnum, access: rw }\n");
    let page_src = emit_page(&schema);
    let setter = setter_fn(&page_src, "ctl_u8");
    assert!(
        setter.contains("buf[0] = value.repr() as u8;"),
        "full-byte enum setter form changed\n---\n{setter}"
    );
    let mut source = String::new();
    let mut en = full_enum("FullEnum", 3);
    en.entries.push(extra_entry("mid", 0xab));
    source.push_str(&RustEmitter.emit_enum(&en));
    source.push('\n');
    source.push_str(&setter);
    source.push_str(
        r#"
fn main() {
    for bg in 0u8..=255 {
        for v in 0u8..=255 {
            let mut buf = [0u8; 16];
            buf[0] = bg;
            set_ctl_u8(&mut buf, FullEnum::from(v));
            assert_eq!(buf[0], v,
                "full-byte enum setter must write the whole byte (bg={bg:#04x} v={v} -> {:#04x})", buf[0]);
        }
    }
}
"#,
    );
    compile_and_run("full_byte_control", source);
}

/// Control: the two-byte enum setter remains a big-endian u16
/// `copy_from_slice` across both bytes, unchanged by the sub-byte repair.
#[test]
fn two_byte_enum_setter_is_big_endian_u16_unchanged() {
    let schema = page(
        "  - { name: ctl_u16, offset: 0, size_bytes: 2, type: u16be, enum_ref: WideEnum, access: rw }\n",
    );
    let page_src = emit_page(&schema);
    let setter = setter_fn(&page_src, "ctl_u16");
    assert!(
        setter.contains("buf[0..2].copy_from_slice(&(value.repr() as u16).to_be_bytes());"),
        "two-byte enum setter form changed\n---\n{setter}"
    );
    let mut source = String::new();
    let mut en = full_enum("WideEnum", 3);
    en.entries.push(extra_entry("wide", 0xbeef));
    source.push_str(&RustEmitter.emit_enum(&en));
    source.push('\n');
    source.push_str(&setter);
    source.push_str(
        r#"
fn main() {
    for v in [0x0000u16, 0x0003, 0xbeef, 0xffff] {
        let mut buf = [0u8; 16];
        buf[0] = 0xaa;
        buf[1] = 0x55;
        set_ctl_u16(&mut buf, WideEnum::from(v));
        assert_eq!(&buf[..2], &[(v >> 8) as u8, v as u8][..],
            "two-byte enum setter must be big-endian u16 (v={v:#06x} -> {:02x?})", &buf[..2]);
    }
}
"#,
    );
    compile_and_run("two_byte_control", source);
}
