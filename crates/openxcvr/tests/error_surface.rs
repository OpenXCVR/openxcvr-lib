//! Public error-surface usability: the refusal/error types returned by the
//! programming, CDB and readiness APIs implement `Display` and
//! `std::error::Error` with preserved nested sources, so consumers can use
//! `?` against `Box<dyn Error>`, format them, and walk `source()` chains.

use std::error::Error;

use openxcvr::programming::{
    plan_named_vendor_programming, FieldValue, ProgrammingContinuationRefusal, ProgrammingRefusal,
};
use openxcvr_cmis::managed_cdb::CdbInputError;
use openxcvr_cmis::readiness::Cmis4ReadinessError;
use openxcvr_cmis::vendor_programming::VendorProgrammingRefusal;
use openxcvr_core::error::ModuleError;
use openxcvr_core::supplied::SpanError;
use openxcvr_core::types::management::Range;
use openxcvr_core::version_error::DetectError;
use openxcvr_core::Rev;

/// Every public error in this surface formats and converts into `Box<dyn Error>`.
#[test]
fn public_errors_display_and_convert() {
    fn boxed<E: Error + 'static>(error: E) -> Box<dyn Error> {
        Box::new(error)
    }
    let cases: Vec<Box<dyn Error>> = vec![
        boxed(ProgrammingRefusal::InvalidCurrentLength),
        boxed(ProgrammingContinuationRefusal::ScopeLost),
        boxed(VendorProgrammingRefusal::MissingPassword),
        boxed(CdbInputError::WrongStep),
        boxed(Cmis4ReadinessError::InvalidPolicy),
    ];
    for error in cases {
        // Display produced a nonempty message, never an empty or Debug leak.
        let text = error.to_string();
        assert!(!text.is_empty(), "empty Display for {error:?}");
        assert!(
            !text.contains("Variant"),
            "Display leaked Debug form: {text}"
        );
    }
}

/// `?` works against `Box<dyn Error>` for the named preflight entry point.
#[test]
fn question_mark_conversion_through_named_preflight() {
    fn refused() -> Result<(), Box<dyn Error>> {
        // A 7-byte "current page" cannot be a 128-byte upper-page image.
        plan_named_vendor_programming(
            Rev::CMIS_4_0,
            Some(&[0; 4]),
            &[0; 7],
            &[("vendor_oui", FieldValue::Bytes(&[0; 3]))],
        )?;
        Ok(())
    }
    let error = refused().unwrap_err();
    assert!(
        error.to_string().contains("128 bytes"),
        "unexpected Display: {error}"
    );
}

/// The nested sources survive: continuation -> proposal -> vendor refusal.
#[test]
fn nested_sources_are_preserved() {
    let refusal = plan_named_vendor_programming(
        Rev::CMIS_4_0,
        None, // no password: the vendor planner refuses
        &[0xa5; 128],
        &[("vendor_oui", FieldValue::Bytes(&[0; 3]))],
    )
    .unwrap_err();
    let vendor: VendorProgrammingRefusal = match &refusal {
        ProgrammingRefusal::Vendor(vendor) => *vendor,
        other => panic!("expected nested vendor refusal, got {other:?}"),
    };
    assert_eq!(vendor, VendorProgrammingRefusal::MissingPassword);

    let continuation = ProgrammingContinuationRefusal::Proposal(refusal);
    let mut chain: Vec<String> = Vec::new();
    let mut current: Option<&dyn Error> = Some(&continuation);
    while let Some(error) = current {
        chain.push(error.to_string());
        current = error.source();
    }
    assert_eq!(chain.len(), 3, "source chain broken: {chain:?}");
    assert!(
        chain[2].contains("vendor password required"),
        "vendor refusal lost from chain: {chain:?}"
    );
}

/// `ModuleError::Detection` keeps its nested detect error as `source()`.
#[test]
fn module_error_detection_source_preserved() {
    let detect = DetectError::Transport(ModuleError::Io("probe failed".into()));
    let module_error = ModuleError::from(detect);
    let source = module_error
        .source()
        .expect("Detection must retain its nested detect error");
    assert_eq!(
        source.to_string(),
        "transport read failed: transport I/O error: probe failed"
    );
}

/// `Cmis4ReadinessError::Span` keeps the span error as `source()`.
#[test]
fn readiness_span_source_preserved() {
    let error = Cmis4ReadinessError::Span(SpanError::InvalidSpan { offset: 0, len: 0 });
    assert!(error.source().is_some(), "Span lost its source: {error:?}");
    let text = error.to_string();
    assert!(!text.contains('{'), "Display leaked Debug form: {text}");
}

/// `#[non_exhaustive]` public error enums stay matchable on named variants
/// from a downstream crate with an explicit catch-all arm.
#[test]
fn non_exhaustive_errors_still_match_named_variants() {
    let error = CdbInputError::BackwardTime;
    let described = match error {
        CdbInputError::BackwardTime => "backward time",
        CdbInputError::WrongStep => "wrong step",
        _ => "other",
    };
    assert_eq!(described, "backward time");
}

/// Range payloads are rendered in Display, not swallowed into a unit message.
#[test]
fn range_payloads_display_without_debug_leak() {
    let error = VendorProgrammingRefusal::FieldNotAdmitted(Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 1,
        len: 1,
    });
    let text = error.to_string();
    assert!(text.contains("Range"), "range omitted from Display: {text}");
}
