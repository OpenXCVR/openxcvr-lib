#![warn(missing_docs)]
//! SFF-8472 v12.5a adapter: the two-address (A0h + A2h) SFP/SFP+/SFP28 memory
//! map, decoded through generated revision-specialised readers.
//!
//! [`adapter::Sff8472Module`] is the module surface: construct it over any
//! [`openxcvr_core::transport::Transport`] with [`adapter::Sff8472Module::new`]
//! (rev-agnostic union decoding) or [`adapter::Sff8472Module::open`]
//! (detect-once: reads A0h byte 94 and freezes the proven revision — unknown
//! or unspecified compliance codes fail closed, never an assumed latest).
//! [`mmap::Sff8472MmapTransport`] adapts captured A0h/A2h files for tests and
//! offline decoding.

pub mod adapter;
pub mod ctx;
// Generated revision-specialised decoders. Item docs are owned by codegen;
// the lint is scoped here rather than silenced crate-wide.
#[allow(missing_docs)]
pub mod generated;
pub mod mmap;
/// Pure A2h monitor-data readiness over caller-supplied observations (the
/// SFF-8472 v12.5a § 9.8 Data_Not_Ready semantics and the Table 8-7 t_data
/// bound, as a time-bounded poll plan).
pub mod readiness;
