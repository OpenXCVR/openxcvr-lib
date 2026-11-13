//! CMIS 5.3 adapter for unbanked Pages 00h-05h plus the CDB framing wrapper.
#![warn(missing_docs)]

/// Spec-native adapter surfaces (CDB framing, module/data-path/network-path
/// state machines, rev-aware dispatch).
pub mod adapter;
/// Supplied-data application advertising decode (Tables 8-59 et al).
pub mod application_records;
/// Optional schema-generated interpretations of capability bit fields.
pub mod capability_meanings;
/// Detect-once revision context frozen at module construction.
pub mod ctx;
/// Shared decode helpers over generated pages.
pub mod decode;
/// The crate's unified error type.
pub mod error;
/// Pure 0100h firmware-information projection.
pub mod firmware_info;
/// Pure 0041h firmware-management feature projection.
pub mod firmware_management_features;
// Generated revision-specialised decoders. Item docs are owned by codegen;
// the lint is scoped here rather than silenced crate-wide.
#[allow(missing_docs)]
pub mod generated;
/// Externally driven, singleton managed CDB 0040h/0041h/0100h operations.
pub mod managed_cdb;
/// The revision-frozen module handle over any CmisTransport.
pub mod module;
/// Transport-level typed CDB operations (attestation, firmware, loopback,
/// network paths, RMON, tunable laser, VDM).
pub mod ops;
/// Pure CMIS 4.0 vendor-password entry plans.
pub mod password_entry;
/// Prerequisite-read receipts for plan/observation contracts.
pub mod prerequisite_receipt;
/// Read planning over the schema footprint.
pub mod read_planning;
/// Pure CMIS 4.0/5.3 management-readiness poll walks.
pub mod readiness;
/// File-backed test/offline transport for captured pages.
pub mod transport;
/// Pure CMIS 4.0 vendor-password programming plans.
pub mod vendor_programming;

pub use adapter::{Cdb, CdbError, CdbOp};
pub use error::CmisError;
pub use module::CmisModule;
pub use openxcvr_core::Rev;
pub use transport::CmisMmapTransport;
