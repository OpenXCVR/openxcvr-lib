//! Bounds checking for schema-generated supplied-span accessors.
//! Missing bytes are errors, including when their would-be value is zero.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
/// Why a supplied-span field access failed.
pub enum SpanError {
    /// The supplied span itself is invalid (offset or length outside the
    /// 256-byte window).
    #[error("invalid supplied span at {offset} with length {len}")]
    InvalidSpan {
        /// The supplied span's offset.
        offset: usize,
        /// The supplied span's byte length.
        len: usize,
    },
    /// The field's bytes are not fully inside the supplied span — missing
    /// bytes are an error even when their would-be value is zero.
    #[error("field at {offset} with length {len} is not fully supplied")]
    InsufficientInput {
        /// The field's offset.
        offset: usize,
        /// The field's byte length.
        len: usize,
    },
    /// The field's instance index is invalid.
    #[error("invalid field instance {0}")]
    InvalidIndex(i64),
}

/// Extract one field's bytes from a supplied page-window span, refusing
/// invalid spans and missing bytes (never zero-filling).
pub fn field_bytes(
    bytes: &[u8],
    supplied_offset: usize,
    field_offset: usize,
    field_len: usize,
) -> Result<&[u8], SpanError> {
    if supplied_offset > 256 || bytes.len() > 256 - supplied_offset {
        return Err(SpanError::InvalidSpan {
            offset: supplied_offset,
            len: bytes.len(),
        });
    }
    let missing = || SpanError::InsufficientInput {
        offset: field_offset,
        len: field_len,
    };
    let start = field_offset
        .checked_sub(supplied_offset)
        .ok_or_else(missing)?;
    let end = start.checked_add(field_len).ok_or_else(missing)?;
    bytes.get(start..end).ok_or_else(missing)
}
