//! Module error surface with preserved typed causes.
//!
//! The [`ModuleError`] variants wrap their underlying failures instead of
//! flattening them to strings: a transport or decode failure carries the
//! original error object in its `source` chain (see [`IoError`] and
//! [`DecodeError`]), so callers can walk `Error::source` down to the real
//! cause (an I/O error, an nix errno, …) instead of parsing a message.

use std::error::Error;
use std::fmt;

/// Transport-layer I/O failure detail: a human-readable message plus, when
/// one exists, the underlying typed cause it was derived from.
///
/// Construct a message-only failure with `"...".into()` (a read-only
/// transport refusing a write, an absent capture, a synthetic test
/// failure). Wrap an underlying error with [`IoError::with_source`], which
/// keeps the original cause reachable through `Error::source` while the
/// message stays the human-readable context.
#[derive(Debug)]
pub struct IoError {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl IoError {
    /// A transport failure whose `message` is the full context and whose
    /// typed cause is preserved as the error source.
    ///
    /// The message should embed the cause's display (for example
    /// `format!("linux-i2c set_slave_address: {e}")`) so the failure reads
    /// standalone; the original error object is attached as the source.
    pub fn with_source(
        message: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(source.into()),
        }
    }

    /// The human-readable context message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<String> for IoError {
    fn from(message: String) -> Self {
        Self {
            message,
            source: None,
        }
    }
}

impl From<&str> for IoError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
            source: None,
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for IoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}

/// Where a field lives, when a failure knows the page and the field.
///
/// `page` is the schema page label (`00h`, `lower`, `A0h`, …), `address` is
/// the seven-bit I2C address of that window, `offset` is the flat byte
/// offset inside the window, and `field` is the schema field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSite {
    /// Schema page label, for example `00h` or `lower`.
    pub page: String,
    /// Seven-bit I2C address of the page window.
    pub address: u8,
    /// Flat byte offset of the field inside the window.
    pub offset: u16,
    /// Schema field name.
    pub field: String,
}

impl FieldSite {
    /// A field site from owned or borrowed page and field names.
    pub fn new(
        page: impl Into<String>,
        address: u8,
        offset: u16,
        field: impl Into<String>,
    ) -> Self {
        Self {
            page: page.into(),
            address,
            offset,
            field: field.into(),
        }
    }
}

impl fmt::Display for FieldSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "page {} address 0x{:02x} offset {} field {}",
            self.page, self.address, self.offset, self.field
        )
    }
}

/// Decode failure detail: bytes were read but did not decode into the
/// requested field. A message describes what failed to decode; when the
/// failure was derived from an underlying typed error, that cause is kept in
/// the source chain.
///
/// Construct with `"...".into()` for a plain decode refusal,
/// [`DecodeError::with_source`] when wrapping an underlying error, or
/// [`DecodeError::at`] / [`DecodeError::at_with_source`] when the page and
/// field are known. A located failure names that site in its `Display` and
/// keeps any typed cause in the source chain.
#[derive(Debug)]
pub struct DecodeError {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
    site: Option<FieldSite>,
}

impl DecodeError {
    /// A decode failure whose `message` is the full context and whose typed
    /// cause is preserved as the error source.
    pub fn with_source(
        message: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(source.into()),
            site: None,
        }
    }

    /// A decode failure at a known page and field. `detail` is what failed
    /// (checksum, range, encoding); the site is included in the `Display`.
    pub fn at(site: FieldSite, detail: impl Into<String>) -> Self {
        Self {
            message: detail.into(),
            source: None,
            site: Some(site),
        }
    }

    /// [`Self::at`] plus a typed cause preserved as the error source.
    pub fn at_with_source(
        site: FieldSite,
        detail: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            message: detail.into(),
            source: Some(source.into()),
            site: Some(site),
        }
    }

    /// The human-readable context message, without the page/field prefix.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The page and field this failure names, when the caller knew them.
    pub fn site(&self) -> Option<&FieldSite> {
        self.site.as_ref()
    }
}

impl From<String> for DecodeError {
    fn from(message: String) -> Self {
        Self {
            message,
            source: None,
            site: None,
        }
    }
}

impl From<&str> for DecodeError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
            source: None,
            site: None,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(site) = &self.site {
            write!(f, "{site}: {}", self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}

/// The error type of the unified module surface: transport I/O failures,
/// unsupported capabilities, and detection failures. Callers match named
/// variants; the enum is non-exhaustive so new failure modes are additive.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ModuleError {
    /// Transport-layer read or write failure. The [`IoError`] payload keeps
    /// the underlying typed cause (when one exists) in the source chain.
    #[error("transport I/O error: {0}")]
    Io(#[from] IoError),
    /// The field or capability is not exposed by this module family or
    /// revision. The message points at `detected_rev()`. When the page and
    /// field are known, [`Self::unsupported_field`] names them.
    #[error("not exposed by this module family/revision; see detected_rev()")]
    Unsupported,
    /// A named field is not exposed by this module family or revision.
    /// The message names the page address and field and points at
    /// `detected_rev()`. Ask the revision report whether the field is
    /// revision-gated or absent from the schema.
    #[error("{site}: not exposed by this module family/revision; see detected_rev()")]
    UnsupportedField {
        /// Page address and schema field that this module does not expose.
        site: FieldSite,
    },
    /// The stored bytes could not be decoded into the requested field. The
    /// [`DecodeError`] payload preserves any underlying typed cause.
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
    /// The SFF-8024 identifier byte does not name a known module family.
    #[error("unknown module identifier: 0x{0:02x}")]
    UnknownIdentifier(u8),
    /// Revision detection failed; the original error is preserved as the source.
    #[error("module detection error")]
    Detection(#[source] Box<crate::version_error::DetectError>),
}

impl From<crate::version_error::DetectError> for ModuleError {
    fn from(error: crate::version_error::DetectError) -> Self {
        Self::Detection(Box::new(error))
    }
}

impl ModuleError {
    /// True when the failure is the module's reported revision: the family's
    /// revision byte holds a value this library does not map to any revision
    /// it knows. Retry at the same point cannot fix this — the library never
    /// assumes a revision the module does not report (fail-closed), so the
    /// remedies are a library that knows the revision or the module vendor's
    /// own tooling.
    pub fn is_unsupported_revision(&self) -> bool {
        matches!(self, ModuleError::Detection(inner) if inner.is_unsupported_revision())
    }

    /// True when the family or revision does not expose the requested field
    /// or capability ([`Self::Unsupported`] or [`Self::UnsupportedField`]).
    /// An unknown revision is a different failure — see
    /// [`Self::is_unsupported_revision`].
    pub fn is_not_exposed(&self) -> bool {
        matches!(
            self,
            ModuleError::Unsupported | ModuleError::UnsupportedField { .. }
        )
    }

    /// The page and field this failure names. Set for [`Self::UnsupportedField`]
    /// and for a [`DecodeError`] built with [`DecodeError::at`] or
    /// [`DecodeError::at_with_source`]. Absent when the construction site did
    /// not know them — do not parse `Display` to recover a site.
    pub fn site(&self) -> Option<&FieldSite> {
        match self {
            Self::UnsupportedField { site } => Some(site),
            Self::Decode(error) => error.site(),
            _ => None,
        }
    }

    /// [`Self::UnsupportedField`] for a field whose page address is known.
    pub fn unsupported_field(site: FieldSite) -> Self {
        Self::UnsupportedField { site }
    }

    /// [`Self::Decode`] naming the page and field that failed to decode.
    pub fn decode_at(site: FieldSite, detail: impl Into<String>) -> Self {
        Self::Decode(DecodeError::at(site, detail))
    }

    /// [`Self::decode_at`] with the typed cause preserved as the source.
    pub fn decode_at_with_source(
        site: FieldSite,
        detail: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self::Decode(DecodeError::at_with_source(site, detail, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_source_chain_reaches_the_original_error() {
        let io = std::io::Error::other("bus stuck");
        let wrapped = IoError::with_source(format!("read failed: {io}"), io);
        let error = ModuleError::Io(wrapped);
        assert_eq!(
            error.to_string(),
            "transport I/O error: read failed: bus stuck"
        );
        let source = error.source().expect("IoError must be the source");
        assert_eq!(source.to_string(), "read failed: bus stuck");
        let cause = source.source().expect("underlying cause must be preserved");
        assert_eq!(cause.to_string(), "bus stuck");
    }

    #[test]
    fn message_only_io_construction_stays_compatible() {
        let error: ModuleError = ModuleError::Io("read-only transport".into());
        assert_eq!(
            error.to_string(),
            "transport I/O error: read-only transport"
        );
        // No underlying cause: the chain ends at the message.
        assert!(error.source().and_then(|s| s.source()).is_none());
    }

    #[test]
    fn unsupported_revision_predicate_matches_only_detection_gate() {
        use crate::version_error::DetectError;
        let error: ModuleError = DetectError::UnsupportedRev {
            detected_byte: 0x0a,
            known_revs: &[],
        }
        .into();
        assert!(error.is_unsupported_revision());
        assert!(!ModuleError::Unsupported.is_unsupported_revision());
        let io: ModuleError = ModuleError::Io("bus gone".into());
        assert!(!io.is_unsupported_revision());
    }

    #[test]
    fn decode_source_chain_reaches_the_original_error() {
        #[derive(Debug)]
        struct Checksum;
        impl fmt::Display for Checksum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("checksum mismatch")
            }
        }
        impl Error for Checksum {}
        let error = ModuleError::Decode(DecodeError::with_source("page 00h decode", Checksum));
        assert_eq!(error.to_string(), "decode error: page 00h decode");
        let cause = error
            .source()
            .and_then(|s| s.source())
            .expect("underlying cause must be preserved");
        assert_eq!(cause.to_string(), "checksum mismatch");
    }

    fn mon_site() -> FieldSite {
        FieldSite::new("00h", 0x50, 14, "temp_mon_value")
    }

    #[test]
    fn unsupported_display_points_at_detected_rev() {
        let error = ModuleError::Unsupported;
        let text = error.to_string();
        assert!(
            text.contains("not exposed by this module family/revision"),
            "{text}"
        );
        assert!(text.contains("detected_rev()"), "{text}");
        assert!(error.site().is_none());
        assert!(error.is_not_exposed());
        assert!(!error.is_unsupported_revision());
    }

    #[test]
    fn unsupported_field_names_page_address_and_field() {
        let error = ModuleError::unsupported_field(mon_site());
        let text = error.to_string();
        assert!(text.contains("page 00h"), "{text}");
        assert!(text.contains("address 0x50"), "{text}");
        assert!(text.contains("offset 14"), "{text}");
        assert!(text.contains("field temp_mon_value"), "{text}");
        assert!(text.contains("detected_rev()"), "{text}");
        assert!(error.is_not_exposed());
        assert!(!error.is_unsupported_revision());
        assert_eq!(
            error.site().map(|site| site.field.as_str()),
            Some("temp_mon_value")
        );
    }

    #[test]
    fn decode_at_names_the_site_and_keeps_the_source() {
        #[derive(Debug)]
        struct Checksum;
        impl fmt::Display for Checksum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("checksum mismatch")
            }
        }
        impl Error for Checksum {}
        let error = ModuleError::decode_at_with_source(mon_site(), "checksum mismatch", Checksum);
        let text = error.to_string();
        assert!(
            text.starts_with("decode error: page 00h address 0x50 offset 14 field temp_mon_value:"),
            "{text}"
        );
        assert!(text.contains("checksum mismatch"), "{text}");
        let cause = error
            .source()
            .and_then(|source| source.source())
            .expect("underlying cause must be preserved");
        assert_eq!(cause.to_string(), "checksum mismatch");
        assert!(!error.is_not_exposed());
    }
}
