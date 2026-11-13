//! Classify a [`ModuleError`] whose cause came from this crate's transport
//! layer, so a consumer of the paged-memory surface can branch on the
//! actionable condition without re-implementing source-chain walking.
//!
//! The paged memory layer wraps every transport failure as
//! [`ModuleError::Io`] while preserving this crate's [`Error`] in the
//! `Error::source` chain. These helpers walk that chain and answer the two
//! questions a caller usually has:
//!
//! - [`is_timeout`] — did a transfer deadline expire?
//! - [`is_retryable`] — is resubmitting the identical operation safe?
//!
//! A [`ModuleError`] with no transport failure in its source chain answers
//! `false` to both: it is either not transport-caused or not classifiable
//! from here, and the caller should inspect it directly.

use crate::Error;
use openxcvr_core::error::ModuleError;

/// The transport-layer [`Error`] this `ModuleError` was caused by, if any.
///
/// Walks the whole `Error::source` chain (a `ModuleError::Detection` can
/// nest a transport-caused `ModuleError::Io` two levels down) and returns
/// the first transport-layer error found. `None` means no transport-layer
/// failure is visible in the chain — never guess a cause that is not there.
pub fn transport_failure(error: &ModuleError) -> Option<&Error> {
    let mut source: Option<&(dyn std::error::Error + 'static)> =
        Some(error as &(dyn std::error::Error + 'static));
    while let Some(current) = source {
        if let Some(transport) = current.downcast_ref::<Error>() {
            return Some(transport);
        }
        source = current.source();
    }
    None
}

/// True when the `ModuleError` was caused by a transport transfer deadline
/// expiring ([`Error::is_timeout`]). See [`transport_failure`] for how the
/// cause is located.
pub fn is_timeout(error: &ModuleError) -> bool {
    transport_failure(error).is_some_and(Error::is_timeout)
}

/// True when the `ModuleError` was caused by a transport failure that may
/// clear by itself, and resubmitting the identical operation is not
/// known-harmful ([`Error::is_retryable`]). A failure whose write may have
/// applied is never retryable. `false` also covers non-transport-caused
/// errors and opaque transport I/O the caller must inspect itself.
///
/// This says nothing about retry *policy* (delay, backoff, attempt budget) —
/// that stays with the caller — and it must not be used to replay reads of
/// clear-on-read locations: those are excluded from plans before any
/// transfer, and re-reading one is a plan-level decision, not a retry.
pub fn is_retryable(error: &ModuleError) -> bool {
    transport_failure(error).is_some_and(Error::is_retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, ErrorKind};
    use openxcvr_core::error::IoError;

    fn wrap(kind: ErrorKind, uncertain: bool) -> ModuleError {
        let mut transport = Error::new(kind, "test double failure");
        if uncertain {
            transport = transport.uncertain();
        }
        ModuleError::Io(IoError::with_source(transport.to_string(), transport))
    }

    #[test]
    fn timeout_is_classified_through_the_source_chain() {
        let error = wrap(ErrorKind::Timeout, false);
        assert!(is_timeout(&error));
        assert!(transport_failure(&error).is_some());
    }

    #[test]
    fn retryable_requires_uncertain_free_transient_kind() {
        assert!(is_retryable(&wrap(ErrorKind::Busy, false)));
        assert!(is_retryable(&wrap(ErrorKind::Nack, false)));
        assert!(is_retryable(&wrap(ErrorKind::Timeout, false)));
        // A write that may have applied is never safe to replay.
        assert!(!is_retryable(&wrap(ErrorKind::Timeout, true)));
        assert!(!is_retryable(&wrap(ErrorKind::Disconnected, false)));
        assert!(!is_retryable(&wrap(ErrorKind::PermissionDenied, false)));
        assert!(!is_retryable(&wrap(ErrorKind::Protocol, false)));
    }

    #[test]
    fn non_transport_errors_are_not_classified_as_transport_failures() {
        assert!(transport_failure(&ModuleError::Unsupported).is_none());
        assert!(!is_timeout(&ModuleError::Unsupported));
        let message_only = ModuleError::Io("read-only transport".into());
        assert!(transport_failure(&message_only).is_none());
    }

    #[test]
    fn detection_nested_transport_failure_is_found() {
        // DetectError::Transport wraps a ModuleError that wraps this crate's
        // Error: the chain is walked all the way down.
        let nested = openxcvr_core::DetectError::Transport(wrap(ErrorKind::Timeout, false));
        let error = ModuleError::Detection(Box::new(nested));
        assert!(is_timeout(&error));
    }
}
