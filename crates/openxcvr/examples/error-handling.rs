//! Tour the `ModuleError` variants a consumer is likely to see.
//!
//! Run: `cargo run --example error-handling`
//!
//! Every fallible call on the `Module` trait returns `Result<T, ModuleError>`.
//! The error enum has five variants:
//! - `Io(IoError)`       — transport-layer failure (file missing, mmap denied, …);
//!   the underlying typed cause stays reachable through `Error::source`
//! - `Unsupported`       — field defined by the trait but absent on this spec
//! - `Decode(DecodeError)` — bytes read but did not match the expected encoding
//! - `UnknownIdentifier` — `dispatch_identifier` did not recognize byte 0
//! - `Detection(DetectError)` — revision detection failed; ask
//!   `error.is_unsupported_revision()` whether the module reported a revision
//!   this library does not know (fail-closed, never assume-latest).
//!
//! Retry guidance per variant lives in docs/error-catalog.md; transport-caused
//! failures classify with openxcvr::classify (is_timeout / is_retryable).

use openxcvr::{dispatch_identifier, ModuleError, UnifiedModule};

fn main() {
    // 1) Io error: open a file that does not exist.
    match UnifiedModule::open_sff8472_mmap("does-not-exist.a0h.bin", "does-not-exist.a2h.bin") {
        Err(ModuleError::Io(io)) => println!("Io          → {io}"),
        Err(other) => println!("Io          → unexpected error: {other}"),
        Ok(_) => println!("Io          → unexpected success (fixture should not exist)"),
    }

    // 2) UnknownIdentifier: SFF-8024 byte-0 = 0xFF is unassigned.
    match dispatch_identifier(0xFF, 0x00) {
        Err(ModuleError::UnknownIdentifier(b)) => println!("Unknown ID  → 0x{b:02X}"),
        Err(other) => println!("Unknown ID  → unexpected error: {other}"),
        Ok(spec) => println!("Unknown ID  → unexpected success: {spec:?}"),
    }

    // 3) Unsupported / Decode are surfaced from per-spec adapters when a field
    //    isn't representable for the active module type. Treat them as
    //    "field is optional on this hardware" rather than fatal — most callers
    //    flatten them with `.ok().flatten()` or `match`-match the variant.
    println!("Unsupported → returned by adapters when a field has no spec equivalent");
    println!("Decode      → returned by adapters when bytes failed checksum or range checks");
}
