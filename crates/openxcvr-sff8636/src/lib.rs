#![warn(missing_docs)]
//! SFF-8636 v2.12 adapter: the QSFP+/QSFP28 paged lower/upper memory map,
//! decoded through generated revision-specialised readers.
//!
//! [`adapter::Sff8636Module`] is the module surface: construct it over any
//! [`openxcvr_core::transport::Transport`] with [`adapter::Sff8636Module::new`]
//! (rev-agnostic union decoding) or [`adapter::Sff8636Module::open`]
//! (detect-once: reads Lower:1 and freezes the proven revision — unknown or
//! legacy compliance codes fail closed, never an assumed latest).
//! [`mmap::Sff8636MmapTransport`] adapts captured lower/page files for tests
//! and offline decoding.

pub mod adapter;
pub mod ctx;
// Generated revision-specialised decoders. Item docs are owned by codegen;
// the lint is scoped here rather than silenced crate-wide.
#[allow(missing_docs)]
pub mod generated;
pub mod mmap;
/// Pure monitor-data readiness over caller-supplied observations (the
/// SFF-8636 Rev 2.12 § 6.2.2 Data_Not_Ready startup semantics and byte-2
/// acknowledge effect, as a time-bounded poll plan).
pub mod readiness;
