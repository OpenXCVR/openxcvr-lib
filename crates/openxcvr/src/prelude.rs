//! Consumer front door.
//!
//! `use openxcvr::prelude::*;` is the single import for the unified module,
//! detection, the error types, the identity and lane values, revisions, and
//! threshold evaluation. Everything here is a re-export; the module adds no
//! behaviour.
//!
//! [`evaluate`] is the alarm/warning threshold decision
//! ([`openxcvr_core::threshold::evaluate`]), not access, thermal, or
//! state-machine evaluation.
//!
//! ```
//! use openxcvr::prelude::*;
//!
//! # fn main() -> Result<(), ModuleError> {
//! let kind = dispatch_identifier(0x03, 0x00)?;
//! assert_eq!(kind, SpecKind::Sff8472);
//! let decision = evaluate(
//!     25.0,
//!     &Thresholds {
//!         high_alarm: Some(80.0),
//!         ..Thresholds::default()
//!     },
//! );
//! assert!(decision.is_within_bounds());
//! # Ok(())
//! # }
//! ```

pub use crate::unified::{detect, dispatch_identifier, Detected, SpecKind, UnifiedModule};
pub use openxcvr_core::error::{DecodeError, FieldSite, IoError, ModuleError};
pub use openxcvr_core::module::Module;
pub use openxcvr_core::rev::Family;
pub use openxcvr_core::revision_report::{
    field_availability, FieldAvailability, FieldStatus, RevisionReport, RevisionReportError,
    RevisionWindow,
};
pub use openxcvr_core::threshold::{evaluate, ThresholdEvaluation, Thresholds};
pub use openxcvr_core::types::{
    Application, ChecksumStatus, LaneFlags, LaneStatus, ModuleFlags, ModuleIdentifier, ModuleState,
    VendorInfo,
};
pub use openxcvr_core::Rev;
