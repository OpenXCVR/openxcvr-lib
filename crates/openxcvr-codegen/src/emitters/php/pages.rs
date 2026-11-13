use crate::expr::compute;
use crate::ir::naming::{NamingPolicy, PhpNaming};
use crate::ir::pages::{page_base, FieldType, InstancesDesc, PageField, PageSchema};

#[path = "page_decode.rs"]
mod page_decode;
use page_decode::{decode_scalar_php, php_type, uses_bit_offset};

struct FieldEmit {
    name: String,
    ty: String,
    init: String,
}

fn emit_scalar_field(f: &PageField, base: i64) -> Option<FieldEmit> {
    // Skip fields without static offset or with instancing / variants
    if f.offset.is_none() || f.offset_expr.is_some() || f.instances.is_some() {
        return None;
    }
    // Raw byte-like and text fields need a known extent.
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }

    let php = php_type(f)?;
    let flat = f.offset.unwrap() as i64;
    let file_off = flat - base;
    if file_off < 0 {
        return None;
    }

    let name = PhpNaming.field_name(&f.name);
    let init = decode_scalar_php(f, file_off);

    Some(FieldEmit {
        name,
        ty: php.to_string(),
        init,
    })
}

fn resolve_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.offset_expr {
        if expr.is_empty() {
            return f.offset.map(|o| o as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        f.offset.map(|o| o as i64)
    }
}

fn resolve_bit_offset(f: &PageField, inst: &InstancesDesc, i: i64) -> Option<i64> {
    if let Some(expr) = &f.bit_offset_expr {
        if expr.is_empty() {
            return Some(f.bit_offset.unwrap_or(0) as i64);
        }
        compute(expr, &inst.index, i).ok()
    } else {
        Some(f.bit_offset.unwrap_or(0) as i64)
    }
}

fn emit_instanced_field(f: &PageField, inst: &InstancesDesc, base: i64) -> Option<FieldEmit> {
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    php_type(f)?;

    let count = (inst.end - inst.start + 1) as usize;
    let mut elems: Vec<String> = Vec::with_capacity(count);
    for i in inst.start..=inst.end {
        let flat = resolve_offset(f, inst, i)?;
        let bit_offset = resolve_bit_offset(f, inst, i)?;
        if bit_offset < 0 {
            return None;
        }
        let file_off = flat - base;
        if file_off < 0 {
            return None;
        }
        let mut element = f.clone();
        element.bit_offset = if uses_bit_offset(f) {
            Some(bit_offset as u32)
        } else {
            None
        };
        element.bit_offset_expr = None;
        elems.push(decode_scalar_php(&element, file_off));
    }

    Some(FieldEmit {
        name: PhpNaming.field_name(&f.name),
        ty: "array".to_string(),
        init: format!("[{}]", elems.join(", ")),
    })
}

fn is_writable(f: &PageField) -> bool {
    f.emits_setter()
}

fn enc_bit_width(f: &PageField) -> u32 {
    f.bit_width.unwrap_or(match f.ty {
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    })
}

/// Decimal-string u64 to exactly 8 bytes, no bcmath/gmp. Rejects a non-digit
/// string and any value above 18446744073709551615.
fn encode_u64_php(o: usize, le: bool) -> String {
    let reverse = if le {
        "\n        $raw = strrev($raw);"
    } else {
        ""
    };
    format!(
        r#"if (!preg_match('/^[0-9]+$/', $value)) {{
        throw new \InvalidArgumentException('u64 must be a decimal digit string');
        }}
        $digits = ltrim($value, '0');
        if ($digits === '') {{ $digits = '0'; }}
        if (strlen($digits) > 20 || (strlen($digits) === 20 && strcmp($digits, '18446744073709551615') > 0)) {{
        throw new \InvalidArgumentException('u64 out of range: ' . $value);
        }}
        $be = array_fill(0, 8, 0);
        for ($i = 7; $i >= 0; $i--) {{
        $rem = 0;
        $next = '';
        $len = strlen($digits);
        for ($j = 0; $j < $len; $j++) {{
        $acc = $rem * 10 + (ord($digits[$j]) - 48);
        $q = intdiv($acc, 256);
        $rem = $acc % 256;
        if ($next !== '' || $q !== 0) {{ $next .= chr(48 + $q); }}
        }}
        $be[$i] = $rem;
        $digits = ($next === '') ? '0' : $next;
        }}
        if ($digits !== '0') {{
        throw new \InvalidArgumentException('u64 out of range: ' . $value);
        }}
        $raw = chr($be[0]) . chr($be[1]) . chr($be[2]) . chr($be[3]) . chr($be[4]) . chr($be[5]) . chr($be[6]) . chr($be[7]);{reverse}
        $buf = substr_replace($buf, $raw, {o}, 8);"#
    )
}

/// PHP encode statement — inverse of `decode_scalar_php` — mutating `$buf`
/// (passed by reference). Sub-byte fields use read-modify-write.
fn encode_scalar_php(f: &PageField, offset: i64) -> String {
    let o = offset as usize;
    let bo = f.bit_offset.unwrap_or(0);
    match &f.ty {
        FieldType::Bool => format!(
            "$buf[{o}] = chr(((ord($buf[{o}]) & ~(1 << {bo})) | (((int) $value) << {bo})) & 0xFF);"
        ),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << enc_bit_width(f)) - 1;
            format!(
                "$buf[{o}] = chr(((ord($buf[{o}]) & ~(0x{mask:X} << {bo})) | (($value & 0x{mask:X}) << {bo})) & 0xFF);"
            )
        }
        FieldType::U8 | FieldType::I8 => {
            format!("$buf[{o}] = chr($value & 0xFF);")
        }
        // Two-byte enum_ref values encode big-endian at the field width —
        // the exact inverse of decode_scalar_php's enum branch.
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("$buf = substr_replace($buf, pack('n', $value & 0xFFFF), {o}, 2);")
        }
        FieldType::EnumRef => {
            format!("$buf[{o}] = chr($value & 0xFF);")
        }
        FieldType::U16Be => {
            format!("$buf = substr_replace($buf, pack('n', $value & 0xFFFF), {o}, 2);")
        }
        FieldType::I16Be => {
            format!("$buf = substr_replace($buf, pack('n', $value & 0xFFFF), {o}, 2);")
        }
        FieldType::U16Le => {
            format!("$buf = substr_replace($buf, pack('v', $value & 0xFFFF), {o}, 2);")
        }
        FieldType::U32Be => {
            format!("$buf = substr_replace($buf, pack('N', $value & 0xFFFFFFFF), {o}, 4);")
        }
        FieldType::I32Be => {
            format!("$buf = substr_replace($buf, pack('N', $value & 0xFFFFFFFF), {o}, 4);")
        }
        FieldType::U64Le => encode_u64_php(o, true),
        FieldType::U64Be => encode_u64_php(o, false),
        FieldType::F16Be | FieldType::X16Be => {
            format!("$buf = substr_replace($buf, pack('n', $value & 0xFFFF), {o}, 2);")
        }
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << enc_bit_width(f)) - 1;
                format!(
                    "$buf[{o}] = chr(((ord($buf[{o}]) & ~(0x{mask:X} << {bo})) | (($value & 0x{mask:X}) << {bo})) & 0xFF);"
                )
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("$buf[{o}] = chr($value & 0xFF);"),
                    2 => {
                        format!("$buf = substr_replace($buf, pack('n', $value & 0xFFFF), {o}, 2);")
                    }
                    4 => format!(
                        "$buf = substr_replace($buf, pack('N', $value & 0xFFFFFFFF), {o}, 4);"
                    ),
                    _ => format!("$buf = substr_replace($buf, pack('J', $value), {o}, 8);"),
                }
            }
        }
        FieldType::Ascii => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("$buf = substr_replace($buf, str_pad(substr($value, 0, {n}), {n}), {o}, {n});")
        }
        FieldType::Bytes | FieldType::Composite => {
            let n = f.size_bytes.unwrap_or(1) as usize;
            format!("$buf = substr_replace($buf, str_pad(substr($value, 0, {n}), {n}, \"\\0\"), {o}, {n});")
        }
    }
}

/// Emit a `set<Field>` static method for a writable scalar field. Instanced PHP
/// setters are deferred (follow-up).
fn emit_setter_php(f: &PageField, base: i64) -> Option<String> {
    if !is_writable(f) {
        return None;
    }
    if matches!(
        f.ty,
        FieldType::Ascii | FieldType::Bytes | FieldType::Composite
    ) && f.size_bytes.is_none()
    {
        return None;
    }
    let php = php_type(f)?;
    let method = PhpNaming.enum_variant(&f.name); // PascalCase

    // Scalar.
    if f.offset_expr.is_none() && f.instances.is_none() {
        let offset = f.offset? as i64 - base;
        if offset < 0 {
            return None;
        }
        let body = encode_scalar_php(f, offset);
        return Some(format!(
            "\n    public static function set{method}(string &$buf, {php} $value): void\n    {{\n        {body}\n    }}\n"
        ));
    }

    // Instanced: switch on $index.
    if let Some(inst) = &f.instances {
        let mut arms = String::new();
        for (n, i) in (inst.start..=inst.end).enumerate() {
            let offset = resolve_offset(f, inst, i)? - base;
            let bit_offset = resolve_bit_offset(f, inst, i)?;
            if offset < 0 || bit_offset < 0 {
                return None;
            }
            let mut element = f.clone();
            element.bit_offset = if uses_bit_offset(f) {
                Some(bit_offset as u32)
            } else {
                None
            };
            element.bit_offset_expr = None;
            arms.push_str(&format!(
                "            case {n}:\n                {}\n                break;\n",
                encode_scalar_php(&element, offset)
            ));
        }
        return Some(format!(
            "\n    public static function set{method}(string &$buf, int $index, {php} $value): void\n    {{\n        switch ($index) {{\n{arms}        }}\n    }}\n"
        ));
    }
    None
}

pub fn emit_page(schema: &PageSchema) -> String {
    let base = page_base(schema);
    let type_name = &schema.id;
    let min_size = schema.size_bytes.unwrap_or(128);

    let mut fields: Vec<FieldEmit> = Vec::new();

    for f in schema.value_fields() {
        let emit = if let Some(inst) = &f.instances {
            emit_instanced_field(f, inst, base)
        } else {
            emit_scalar_field(f, base)
        };
        if let Some(emit) = emit {
            fields.push(emit);
        } else {
            panic!("{}: cannot emit field {}", schema.id, f.name);
        }
    }

    // Build the decode() body assignments
    let decode_args: String = fields
        .iter()
        .map(|fe| format!("            {}: {},\n", fe.name, fe.init))
        .collect();

    // Build the __construct() parameters
    let construct_params: String = if fields.is_empty() {
        "        // no fields decoded at current scope\n".to_string()
    } else {
        fields
            .iter()
            .map(|fe| format!("        public readonly {} ${},\n", fe.ty, fe.name))
            .collect()
    };

    // encode: per-field setters for writable fields (RMW for bit-fields).
    let setters: String = schema
        .value_fields()
        .filter_map(|f| emit_setter_php(f, base))
        .collect();

    // checksum recompute: emit recomputeChecksums() for sum8 checksum fields.
    let chksum_fields: Vec<_> = schema
        .value_fields()
        .filter_map(|f| {
            let offset = f.offset?;
            let (flat_start, flat_end) = f.checksum_region()?;
            Some((offset, flat_start, flat_end))
        })
        .collect();

    let recompute_checksums = if chksum_fields.is_empty() {
        String::new()
    } else {
        let size = schema.size_bytes.unwrap_or(128) as i64;
        let mut body = String::new();
        for (flat_offset, flat_start, flat_end) in &chksum_fields {
            let chk_idx = *flat_offset as i64 - base;
            let cov_start = *flat_start as i64 - base;
            let cov_end = *flat_end as i64 - base;
            assert!(
                chk_idx >= 0
                    && chk_idx < size
                    && cov_start >= 0
                    && cov_start <= cov_end
                    && cov_end < size,
                "{}: checksum at flat {flat_offset} covering flat [{flat_start},{flat_end}] \
                 out of bounds for page_base {base}, size {size}",
                schema.id
            );
            body.push_str(&format!(
                "        $sum = 0;\n        for ($i = {cov_start}; $i <= {cov_end}; $i++) {{ $sum = ($sum + ord($buf[$i])) & 0xFF; }}\n        $buf[{chk_idx}] = chr($sum);\n",
            ));
        }
        format!(
            "\n    public static function recomputeChecksums(string &$buf): void\n    {{\n{body}    }}\n",
        )
    };

    format!(
        concat!(
            "<?php\n",
            "// Generated by openxcvr-codegen. DO NOT EDIT.\n",
            "declare(strict_types=1);\n",
            "namespace Openxcvr\\Generated;\n",
            "\n",
            "readonly class {type_name}\n",
            "{{\n",
            "    public static function decode(string $buf): self\n",
            "    {{\n",
            "        if (strlen($buf) < {min_size}) {{\n",
            "            throw new \\InvalidArgumentException(\n",
            "                '{type_name}::decode requires {min_size} bytes, got ' . strlen($buf)\n",
            "            );\n",
            "        }}\n",
            "        return new self(\n",
            "{decode_args}",
            "        );\n",
            "    }}\n",
            "{setters}",
            "{recompute_checksums}",
            "\n",
            "    public function __construct(\n",
            "{construct_params}",
            "    ) {{}}\n",
            "}}\n",
        ),
        type_name = type_name,
        min_size = min_size,
        decode_args = decode_args,
        setters = setters,
        recompute_checksums = recompute_checksums,
        construct_params = construct_params,
    )
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod pages_tests;
