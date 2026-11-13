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

pub(super) fn go_type(f: &PageField) -> Option<&'static str> {
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
        FieldType::U64Le | FieldType::U64Be => Some("uint64"),
        FieldType::Ascii => Some("string"),
        FieldType::Bytes | FieldType::Composite => Some("[]byte"),
    }
}

pub(super) fn needs_binary(f: &PageField) -> bool {
    matches!(
        f.ty,
        FieldType::U16Be
            | FieldType::I16Be
            | FieldType::U16Le
            | FieldType::U32Be
            | FieldType::I32Be
            | FieldType::U64Le
            | FieldType::U64Be
            | FieldType::F16Be
            | FieldType::X16Be
    ) || (matches!(f.ty, FieldType::Bitmap) && f.size_bytes.unwrap_or(1) > 1)
        // Two-byte enum_ref fields decode through encoding/binary (16-bit
        // BE enum value; see decode_scalar_go).
        || (f.ty == FieldType::EnumRef && f.size_bytes == Some(2))
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

pub(super) fn decode_scalar_go(f: &PageField, file_offset: i64) -> String {
    let o = file_offset as usize;
    let bit_offset = f.bit_offset.unwrap_or(0);
    let bit_width = bit_width(f);

    match &f.ty {
        FieldType::Bool => format!("(buf[{o}]>>{bit_offset})&0x1 != 0"),
        FieldType::U2
        | FieldType::U3
        | FieldType::U4
        | FieldType::U5
        | FieldType::U6
        | FieldType::U7 => {
            let mask = (1u32 << bit_width) - 1;
            if bit_offset == 0 {
                format!("int(buf[{o}] & 0x{mask:X})")
            } else {
                format!("int((buf[{o}] >> {bit_offset}) & 0x{mask:X})")
            }
        }
        FieldType::U8 => format!("int(buf[{o}])"),
        // Two-byte enum_ref fields (SFF-8636 v2.12 § 7 Table 7-1 / SFF-8472
        // v12.5a § 13 Table 13-1 "128-129 | 2 | Format ID | CA1Bh") carry a
        // 16-bit big-endian enum value; read the full width (mirrors the
        // Rust emitter's decode_at enum branch).
        FieldType::EnumRef if f.size_bytes == Some(2) => {
            format!("int(binary.BigEndian.Uint16(buf[{o}:{}]))", o + 2)
        }
        FieldType::EnumRef => format!("int(buf[{o}])"),
        FieldType::I8 => format!("int(int8(buf[{o}]))"),
        FieldType::U16Be => format!("int(binary.BigEndian.Uint16(buf[{o}:{}]))", o + 2),
        FieldType::I16Be => format!("int(int16(binary.BigEndian.Uint16(buf[{o}:{}])))", o + 2),
        FieldType::U16Le => format!("int(binary.LittleEndian.Uint16(buf[{o}:{}]))", o + 2),
        FieldType::U32Be => format!("int(binary.BigEndian.Uint32(buf[{o}:{}]))", o + 4),
        FieldType::I32Be => format!("int(int32(binary.BigEndian.Uint32(buf[{o}:{}])))", o + 4),
        FieldType::U64Le => format!("binary.LittleEndian.Uint64(buf[{o}:{}])", o + 8),
        FieldType::U64Be => format!("binary.BigEndian.Uint64(buf[{o}:{}])", o + 8),
        FieldType::F16Be | FieldType::X16Be => {
            format!("int(binary.BigEndian.Uint16(buf[{o}:{}]))", o + 2)
        }
        FieldType::Bitmap => {
            if f.bit_offset.is_some() || f.bit_width.is_some() {
                let mask = (1u64 << bit_width) - 1;
                if bit_offset == 0 {
                    format!("int(buf[{o}] & 0x{mask:X})")
                } else {
                    format!("int((buf[{o}] >> {bit_offset}) & 0x{mask:X})")
                }
            } else {
                match f.size_bytes.unwrap_or(1) {
                    1 => format!("int(buf[{o}])"),
                    2 => format!("int(binary.BigEndian.Uint16(buf[{o}:{}]))", o + 2),
                    4 => format!("int(binary.BigEndian.Uint32(buf[{o}:{}]))", o + 4),
                    8 => format!("int(binary.BigEndian.Uint64(buf[{o}:{}]))", o + 8),
                    n => be_int_expr(o, n as usize),
                }
            }
        }
        FieldType::Ascii => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("strings.TrimRight(string(buf[{o}:{end}]), \" \")")
        }
        FieldType::Bytes | FieldType::Composite => {
            let size = f.size_bytes.unwrap_or(1) as usize;
            let end = o + size;
            format!("append([]byte(nil), buf[{o}:{end}]...)")
        }
    }
}

fn be_int_expr(offset: usize, size: usize) -> String {
    let terms: Vec<String> = (0..size)
        .map(|i| {
            let shift = (size - i - 1) * 8;
            if shift == 0 {
                format!("int(buf[{}])", offset + i)
            } else {
                format!("int(buf[{}])<<{}", offset + i, shift)
            }
        })
        .collect();
    terms.join(" | ")
}
