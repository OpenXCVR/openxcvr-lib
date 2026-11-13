#![doc(html_root_url = "https://docs.rs/openxcvr/0.2.0")]
#![warn(missing_docs)]
//! Unified Rust API for SFF-8636, SFF-8472, and CMIS pluggable optical and copper network
//! transceivers.
//!
//! Network transceivers are managed through register reads and writes against a paged
//! byte-addressable memory map. Three governing standards describe overlapping memory models
//! with different vocabularies for the same concepts:
//!
//! - **CMIS 5.3** — QSFP-DD, OSFP, DSFP, COBO. Banked + paged upper memory. (v0.1: unbanked
//!   Pages 00h–05h supported, plus typed CDB commands for the qualified subset; banked pages
//!   deferred to v0.2+.)
//! - **SFF-8636 v2.12** — QSFP+, QSFP28. Paged upper memory.
//! - **SFF-8472 v12.5a** — SFP, SFP+, SFP28. Two-address (A0h + A2h) map with paged regions.
//!
//! All three families share a large fragment of vocabulary (vendor identity strings, telemetry
//! encodings, threshold ordering, SFF-8024 enum tables) but diverge in others. The [`Module`]
//! trait is the canonical L2 surface; per-spec adapters translate from this CMIS-named vocabulary
//! down to each spec's native L1 location.
//!
//! # Quick start
//!
//! [`prelude`] is the single import for the types a consumer touches:
//! `use openxcvr::prelude::*;`. The example below uses the crate-root
//! re-exports, which remain available.
//!
//! ```no_run
//! use openxcvr::{Module, UnifiedModule};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let module = UnifiedModule::open_sff8636_mmap(
//!     "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin",
//!     "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin",
//!     "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin",
//!     "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin",
//!     "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin",
//! )?;
//! let info = module.vendor_info()?;
//! println!("vendor: {}", info.name);
//! # Ok(()) }
//! ```
//!
//! See the repository quickstart (`docs/guides/quickstart.md`) for the CMIS and
//! SFF-8472 paths, and `crates/openxcvr/examples/` for runnable examples.
//!
//! # Dispatch
//!
//! Construct a [`UnifiedModule`] via the spec-specific `open_*_mmap` constructors —
//! [`UnifiedModule::open_sff8472_mmap`], [`UnifiedModule::open_sff8636_mmap`], or
//! [`UnifiedModule::open_cmis_mmap`]. Power users can downcast to the spec-native
//! module type via the [`UnifiedModule`] enum's variants.
//!
//! [`dispatch_identifier`] takes byte 0 (the SFF-8024 Identifier) plus byte 1 (CMIS
//! revision tie-break for QSFP-DD/OSFP) and returns a [`SpecKind`] for callers who need
//! to inspect the spec-family decision without opening a module.

pub mod catalog;
pub mod live;
pub mod observation;
/// Consumer front door. `use openxcvr::prelude::*;` imports the unified
/// module, detection, errors, identity and lane types, revisions, and
/// threshold evaluation. See [`prelude`].
pub mod prelude;
pub mod programming;
pub mod raw;
pub mod raw_write;
pub mod rich;
pub mod unified;

#[cfg(doctest)]
mod doc_snippets;

// Top-level entry points for the unified API.
pub use unified::{dispatch_identifier, SpecKind, UnifiedModule};

// Curated re-exports: the most consumer-facing types from openxcvr-core promoted to crate root.
// Power users can still reach everything via `openxcvr::core::*`.
pub use openxcvr_core::access;
pub use openxcvr_core::error::ModuleError;
pub use openxcvr_core::module::Module;
pub use openxcvr_core::types::{
    ChecksumEntry, ChecksumStatus, LaneFlags, LaneStatus, ModuleFlags, ModuleIdentifier, VendorInfo,
};

// Per-spec adapter types — promoted so consumers can match on `UnifiedModule` variants
// or instantiate adapters directly without a `use openxcvr_<spec>::` line.
pub use openxcvr_cmis::{
    capability_meanings, firmware_info, firmware_management_features, managed_cdb, CmisModule,
};
pub use openxcvr_sff8472::adapter::Sff8472Module;
pub use openxcvr_sff8636::adapter::Sff8636Module;

// Transport-caused ModuleError classification (timeout / retryable) lives
// next to the transport layer that produces the causes; re-exported here so
// consumers of the unified surface need no extra dependency.
pub use openxcvr_transports::classify;

// Power-user access to the full openxcvr-core surface.
pub use openxcvr_core as core;
