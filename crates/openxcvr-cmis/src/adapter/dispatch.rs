//! Rev-aware adapter dispatch.
//!
//! Resolves a detected [`Ctx`] to the matching per-rev generated module set
//! (`generated/v{rev}/`). Phase B implementations live as functions per
//! accessor (see `adapter/dpsm.rs`, `adapter/cdb.rs` in Task B9) rather than
//! as one type-erased trait object — Rust's monomorphization gives us free
//! dispatch.

use crate::ctx::Ctx;
use openxcvr_core::Rev;

/// Mark trait for per-rev generated module collections. Each
/// `generated/v{rev}/mod.rs` will implement this; today the body is empty
/// because Phase B2–B7 just authored the schemas — the codegen run that
/// produces these modules is the same `cargo run -p openxcvr-codegen` we
/// already invoked in B2 Step 3.
pub trait CmisRevModules {}

/// Resolve a [`Ctx`] to the matching `generated/v{rev}/` module label.
pub fn rev_dispatch_label(ctx: &Ctx) -> &'static str {
    use Rev as R;
    match ctx.detected_rev {
        r if r == R::CMIS_3_0 => "v3_0",
        r if r == R::CMIS_4_0 => "v4_0",
        r if r == R::CMIS_5_0 => "v5_0",
        r if r == R::CMIS_5_1 => "v5_1",
        r if r == R::CMIS_5_2 => "v5_2",
        r if r == R::CMIS_5_3 => "v5_3",
        r if r == R::CMIS_5_4 => "v5_4",
        _ => "unknown",
    }
}
