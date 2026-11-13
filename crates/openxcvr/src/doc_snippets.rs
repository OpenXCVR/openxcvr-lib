//! Compiles the Rust blocks of the handwritten documentation as doctests.
//!
//! Built only under `cargo test --doc`. A page listed here cannot drift from
//! the API it shows: a block that stops compiling, or whose assertions stop
//! holding, fails the workspace tests. Blocks tagged `no_run` open fixture
//! files by repository-relative path and are compiled but not executed,
//! because doctests run from the package directory.
//!
//! Add a page here when it gains its first `rust` block. Every fence in a
//! listed page needs a language tag; rustdoc compiles an untagged fence.

#[doc = include_str!("../../../README.md")]
mod repository_readme {}
#[doc = include_str!("../README.md")]
mod crate_readme {}

#[doc = include_str!("../../../docs/guides/api-guide.md")]
mod api_guide {}
#[doc = include_str!("../../../docs/guides/error-handling.md")]
mod error_handling {}
#[doc = include_str!("../../../docs/guides/offline-analysis.md")]
mod offline_analysis {}
#[doc = include_str!("../../../docs/guides/live-monitoring.md")]
mod live_monitoring {}
#[doc = include_str!("../../../docs/guides/managed-operations.md")]
mod managed_operations {}

mod cookbook {
    #[doc = include_str!("../../../docs/cookbook/identify-family-and-revision.md")]
    mod identify_family_and_revision {}
    #[doc = include_str!("../../../docs/cookbook/read-vendor-and-serial.md")]
    mod read_vendor_and_serial {}
    #[doc = include_str!("../../../docs/cookbook/decode-vdm.md")]
    mod decode_vdm {}
    #[doc = include_str!("../../../docs/cookbook/evaluate-a-threshold.md")]
    mod evaluate_a_threshold {}
    #[doc = include_str!("../../../docs/cookbook/read-lane-flags.md")]
    mod read_lane_flags {}
    #[doc = include_str!("../../../docs/cookbook/plan-a-state-transition.md")]
    mod plan_a_state_transition {}
    #[doc = include_str!("../../../docs/cookbook/issue-a-typed-cdb-command.md")]
    mod issue_a_typed_cdb_command {}
    #[doc = include_str!("../../../docs/cookbook/explain-a-revision-gated-field.md")]
    mod explain_a_revision_gated_field {}
    #[doc = include_str!("../../../docs/cookbook/handle-a-decode-or-unsupported-error.md")]
    mod handle_a_decode_or_unsupported_error {}
    #[doc = include_str!("../../../docs/cookbook/test-without-hardware.md")]
    mod test_without_hardware {}
}
