use crate::error::ModuleError;
use crate::rev::Rev;

/// Failures of revision detection: the transport read that probes the
/// revision byte, or a revision byte that does not map to a revision this
/// library knows. Detection always fails closed — never an assumed latest.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum DetectError {
    /// Transport-layer read failure while probing MajorRevision.
    /// The `#[from]` derive lets `transport.read(...)?` propagate
    /// `ModuleError` through `DetectError` without an explicit
    /// `.map_err(...)` call site — required by Task B8 Step 4's
    /// `CmisModule::new(mut transport)` constructor, which performs
    /// the detect-once read and must surface I/O failures distinctly
    /// from rev-byte parse failures.
    #[error("transport read failed: {0}")]
    Transport(#[from] ModuleError),
    /// The revision byte does not map to a revision this library knows
    /// (including the family's unspecified code). Carries the detected byte
    /// and the family's known-revision domain, so the caller can report
    /// both instead of guessing.
    #[error(
        "unsupported or unknown rev byte: 0x{detected_byte:02x}; not mapped by this library — see known_revs and do not assume a later revision"
    )]
    UnsupportedRev {
        /// The byte the family's revision location actually held.
        detected_byte: u8,
        /// Every revision this library knows for the family.
        known_revs: &'static [Rev],
    },
}

impl DetectError {
    /// True when the failure is the family's revision byte itself: it holds a
    /// value this library does not map to any revision it knows (including the
    /// family's unspecified code). Retrying the same probe on the same module
    /// cannot fix this; the module's reported revision and this library's
    /// known-revision domain disagree. Upgrading to a library that knows the
    /// revision, or reading the module with its vendor's tooling, are the
    /// remedies — never assuming a revision it does not report.
    pub fn is_unsupported_revision(&self) -> bool {
        matches!(self, DetectError::UnsupportedRev { .. })
    }
}

/// Failures of revision-aware field encoding: the field does not exist as a
/// writable field at the revision being encoded.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum EncodeError {
    /// The field is not writable at the detected revision.
    #[error(
        "field {field} not writable in rev {detected:?} (available since {available_since:?}); encode for a revision the field exists at, or leave the field"
    )]
    UnsupportedInRev {
        /// The field's schema name.
        field: &'static str,
        /// The revision the field first became writable in, when the schema
        /// states one.
        available_since: Option<Rev>,
        /// The revision the caller is encoding for.
        detected: Rev,
    },
}

impl EncodeError {
    /// True when the failure is the revision in force: the field is not
    /// writable there (it may not exist at all, or exist only in later
    /// revisions). Retrying at the same revision cannot fix this; encode for
    /// a revision the field exists at, or leave the field alone.
    pub fn is_unsupported_in_revision(&self) -> bool {
        matches!(self, EncodeError::UnsupportedInRev { .. })
    }
}

/// Failures of revision-aware management operations: the operation does not
/// exist at the revision in force.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum MgmtError {
    /// The management operation is not supported at the detected revision.
    #[error(
        "op {op} not supported in rev {detected:?}; not exposed by this revision — use a revision where the operation exists, or skip it"
    )]
    NotSupportedInRev {
        /// The operation's name.
        op: &'static str,
        /// The revision the caller is operating at.
        detected: Rev,
    },
}

impl MgmtError {
    /// True when the failure is the revision in force: the management
    /// operation does not exist there. Retrying at the same revision cannot
    /// fix this; use a revision where the operation exists, or skip it.
    pub fn is_unsupported_in_revision(&self) -> bool {
        matches!(self, MgmtError::NotSupportedInRev { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ModuleError;

    #[test]
    fn module_error_converts_into_detect_error_via_from() {
        let io_err = ModuleError::Io("simulated bus failure".into());
        let detect: DetectError = io_err.into();
        assert!(matches!(detect, DetectError::Transport(_)));
        assert!(detect.to_string().starts_with("transport read failed:"));
        // A transport failure is not an unsupported-revision failure.
        assert!(!detect.is_unsupported_revision());
    }

    #[test]
    fn unsupported_rev_predicate_names_only_the_rev_byte_failure() {
        let detect = DetectError::UnsupportedRev {
            detected_byte: 0x7f,
            known_revs: &[],
        };
        assert!(detect.is_unsupported_revision());
    }

    #[test]
    fn encode_error_predicate_names_only_the_revision_gate() {
        let err = EncodeError::UnsupportedInRev {
            field: "some_field",
            available_since: None,
            detected: Rev::CMIS_5_2,
        };
        assert!(err.is_unsupported_in_revision());
    }

    #[test]
    fn mgmt_error_predicate_names_only_the_revision_gate() {
        let err = MgmtError::NotSupportedInRev {
            op: "some_op",
            detected: Rev::CMIS_5_2,
        };
        assert!(err.is_unsupported_in_revision());
    }

    #[test]
    fn unsupported_rev_display_names_the_next_step() {
        let detect = DetectError::UnsupportedRev {
            detected_byte: 0x7f,
            known_revs: &[],
        };
        let text = detect.to_string();
        assert!(text.contains("0x7f"), "{text}");
        assert!(text.contains("do not assume a later revision"), "{text}");
        assert!(text.contains("known_revs"), "{text}");
    }

    #[test]
    fn encode_and_mgmt_displays_name_the_next_step() {
        let encode = EncodeError::UnsupportedInRev {
            field: "temp_mon_value",
            available_since: Some(Rev::CMIS_5_0),
            detected: Rev::CMIS_4_0,
        };
        let text = encode.to_string();
        assert!(text.contains("temp_mon_value"), "{text}");
        assert!(
            text.contains("encode for a revision the field exists at"),
            "{text}"
        );
        let mgmt = MgmtError::NotSupportedInRev {
            op: "some_op",
            detected: Rev::CMIS_4_0,
        };
        let text = mgmt.to_string();
        assert!(text.contains("some_op"), "{text}");
        assert!(text.contains("not exposed by this revision"), "{text}");
    }
}
