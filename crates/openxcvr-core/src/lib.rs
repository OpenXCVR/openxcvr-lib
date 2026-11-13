//! Core library for transceiver management: the unified module vocabulary,
//! revision identity, transport contract, and the pure decision routines
//! (observation decoding, threshold evaluation, scheduling/fencing, thermal)
//! that callers feed with supplied bytes and values.
//!
//! The library parses and decides and never acts on a module on its own
//! initiative: routines here return decisions, states, or planned effects;
//! authoritative live module instances, history, cache storage, epochs,
//! clocks, scheduling, admission and execution belong to the daemon.
#![warn(missing_docs)]
pub mod access;
pub mod cmis_f16;
/// The error types shared across the unified surface.
pub mod error;
pub mod field_catalog;
pub mod fsm;
/// The canonical L2 module trait and its vocabulary types.
pub mod module;
pub mod observation;
pub mod rev;
/// Schema-driven revision and field-availability reports.
pub mod revision_report;
pub mod selector;
pub mod supplied;
/// In-memory module, synthetic pages, and assertion helpers for consumers
/// testing without hardware.
pub mod testkit;
pub mod threshold;
/// The byte-oriented transport contract family adapters are generic over.
pub mod transport;
/// The shared value types of the unified module vocabulary.
pub mod types;
/// Version-detection and management error types shared across the families.
pub mod version_error;

/// Backend transport building blocks (Linux i2c-dev, feature-gated).
#[cfg(all(feature = "linux-i2c", target_os = "linux"))]
pub mod transports;

pub use cmis_f16::CmisF16;
pub use rev::{Family, Rev};
pub use version_error::{DetectError, EncodeError, MgmtError};
