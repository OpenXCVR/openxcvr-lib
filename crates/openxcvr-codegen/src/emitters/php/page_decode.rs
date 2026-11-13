use crate::ir::pages::{FieldType, PageField};

fn bit_width(f: &PageField) -> u32 {
    f.bit_width.unwrap_or(match f.ty {
        FieldType::Bool => 1,
        FieldType::U2 => 2,
        FieldType::U3 => 3,
        FieldType::U4 => 4,
        FieldType::U5 => 5,
        FieldType::U6 => 6,
        FieldType::U7 => 7,
        _ => 8,
    })
}

pub(super) fn php_type(f: &PageField) -> Option<&'static str> {
    match &f.ty {
        FieldType::Bool => Some("bool"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7
        | FieldType::U8
        | FieldType::I8
        | FieldType::EnumRef
        | FieldType::U16Be
        | FieldType::I16Be
        | FieldType::U16Le
        | FieldType::U32Be
        | FieldType::I32Be
        | FieldType::F16Be
        | FieldType::X16Be
        | FieldType::Bitmap => Some("int"),
        // PHP has no unsigned 64-bit integer and no core big-integer type.
        // A decimal string carries every value in [0, 2^64-1] exactly.
        FieldType::U64Le | FieldType::U64Be => Some("string"),
        FieldType::Ascii => Some("string"),
        FieldType::Bytes | FieldType::Composite => Some("string"),
    }
}

pub(super) fn uses_bit_offset(f: &PageField) -> bool {
    matches!(
        f.ty,
        FieldType::Bool
            | FieldType::U2
            | FieldType::U3
            | FieldType::U4
            | FieldType::U5
            | FieldType::U6
            | FieldType::U7
    ) || f.bit_offset.is_some()
        || f.bit_offset_expr.is_some()
        || f.bit_width.is_some()
}

pub(super) fn decode_scalar_php(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bit_offset = f.bit_offset.unwrap_or(0);
    let bit_width = bit_width(f);

    match &f.ty {
        FieldType::Bool => {
            format!("(bool) ((ord($buf[{o}]) >> {bit_offset}) & 0x1)")
        }
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width) - 1;
            if bit_offset == 0 {
                format!("ord($buf[{o}]) & 0x{mask:X}")
            } else {
                format!("(ord($buf[{o}]) >> {bit_offset}) & 0x{mask:X}")
            }
        }
        FieldType::U8 => {
            format!("ord($buf[{o}])")
        }
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the
        // Rust emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("unpack('n', substr($buf, {o}, 2))[1]")
        }
        FieldType::EnumRef => {
            format!("ord($buf[{o}])")
        }
        FieldType::I8 => {
            format!("(ord($buf[{o}]) >= 128 ? ord($buf[{o}]) - 256 : ord($buf[{o}]))")
        }
        FieldType::U16Be => {
            format!("unpack('n', substr($buf, {o}, 2))[1]")
        }
        FieldType::I16Be => {
            format!(
                "($tmp{o} = unpack('n', substr($buf, {o}, 2))[1]) >= 32768 ? $tmp{o} - 65536 : $tmp{o}"
            )
        }
        FieldType::U16Le => {
            format!("unpack('v', substr($buf, {o}, 2))[1]")
        }
        FieldType::U32Be => {
            format!("unpack('N', substr($buf, {o}, 4))[1]")
        }
        FieldType::I32Be => {
            format!(
                "($tmp{o} = unpack('N', substr($buf, {o}, 4))[1]) >= 2147483648 ? $tmp{o} - 4294967296 : $tmp{o}"
            )
        }
        // On 64-bit PHP, unpack of a value >= 2^63 yields a negative int.
        // sprintf('%u') prints that bit pattern as an unsigned decimal string.
        FieldType::U64Le => {
            format!("sprintf('%u', unpack('P', substr($buf, {o}, 8))[1])")
        }
        FieldType::U64Be => {
            format!("sprintf('%u', unpack('J', substr($buf, {o}, 8))[1])")
        }
        FieldType::F16Be | FieldType::X16Be => {
            format!("unpack('n', substr($buf, {o}, 2))[1]")
        }
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bit_width) - 1;
                if bit_offset == 0 {
                    format!("ord($buf[{o}]) & 0x{mask:X}")
                } else {
                    format!("(ord($buf[{o}]) >> {bit_offset}) & 0x{mask:X}")
                }
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("ord($buf[{o}])"),
                    2 => format!("unpack('n', substr($buf, {o}, 2))[1]"),
                    4 => format!("unpack('N', substr($buf, {o}, 4))[1]"),
                    8 => format!("unpack('J', substr($buf, {o}, 8))[1]"),
                    n => be_int_expr(o, n as usize),
                }
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!("rtrim(substr($buf, {o}, {size}), \" \")")
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            format!("substr($buf, {o}, {size})")
        }
    }
}

fn be_int_expr(offset: usize, size: usize) -> String {
    let terms: Vec<String> = (0..size)
        .map(|i| {
            let shift = (size - i - 1) * 8;
            if shift == 0 {
                format!("ord($buf[{}])", offset + i)
            } else {
                format!("ord($buf[{}]) << {}", offset + i, shift)
            }
        })
        .collect();
    terms.join(" | ")
}
