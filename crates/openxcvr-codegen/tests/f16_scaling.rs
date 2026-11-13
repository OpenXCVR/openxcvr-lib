//! F16 scaling derivation tests (Stage 2 D2): schema-owned unit provenance
//! for f16be fields.
//!
//! - Fail-closed: an f16be field without `unit:`, with a unit string
//!   outside the closed table, or with an `encoding:` alongside is a
//!   generation error naming the field and the accepted units — no unit is
//!   ever inferred from field names, tags or the kind alone.
//! - The four real Page 14h BER rows derive `F16 { Ratio }`.
//!
//! Synthetic fixtures below are labelled synthetic in each test's comment;
//! the real-row test reads the repository schemas.
use openxcvr_codegen::emitters::rust::catalog::{
    derive_rows, page_catalog_info, DerivedRow, EmittedScaling,
};
use openxcvr_codegen::parser::index::IndexLoader;
use openxcvr_codegen::parser::pages::load_page_schema;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn spec_rows(spec: &str) -> Vec<DerivedRow> {
    let schemas = repo_root().join("schemas");
    let paths = IndexLoader::collect_page_schemas(&schemas, Some(spec)).expect("collect pages");
    let loaded: Vec<_> = paths
        .iter()
        .map(|p| load_page_schema(p).expect("load page"))
        .collect();
    let info = page_catalog_info(&paths).expect("page info");
    derive_rows(&loaded, spec, &info).expect("derive rows")
}

/// Writes a synthetic CMIS spec dir and derives its rows. Synthetic
/// fixture: none of these schemas exist in the repo. The fixture dir is
/// removed on every exit path — success, `Err` return, or panic — on drop.
fn synthetic_rows(page_yaml: &str) -> Result<Vec<DerivedRow>, String> {
    let dir = tempfile::tempdir().unwrap();
    let page_path = dir.path().join("page.yaml");
    std::fs::write(&page_path, page_yaml).unwrap();
    let loaded: Vec<_> = vec![load_page_schema(&page_path).map_err(|e| format!("{e:#}"))?];
    let info = page_catalog_info(&[page_path]).unwrap();
    derive_rows(&loaded, "cmis", &info)
}

const SYNTH_PAGE_HEAD: &str = r#"
schema: openxcvr.page.v1
kind: page
id: SynthPage
title: T
page_address: "01h"
fields:
"#;

/// One f16be field with the given extra declaration lines (unit, encoding).
fn synth_f16_field(extra: &str) -> String {
    format!(
        r#"  - name: probe
    offset: 200
    size_bytes: 2
    type: f16be
{extra}
"#
    )
}

#[test]
fn f16be_without_unit_is_a_generation_error() {
    // Synthetic: after the real schemas declare their units, no f16be
    // field exists without one.
    let page = format!("{}{}", SYNTH_PAGE_HEAD, synth_f16_field(""));
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("probe") && err.contains("ratio"),
        "error must name field and accepted units: {err}"
    );
}

#[test]
fn f16be_with_unknown_unit_is_a_generation_error() {
    // Synthetic: "bananas" is not in the closed f16 unit table.
    let page = format!(
        "{}{}",
        SYNTH_PAGE_HEAD,
        synth_f16_field("    unit: \"bananas\"")
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("probe") && err.contains("bananas") && err.contains("ratio"),
        "error must name field, unit and accepted units: {err}"
    );
}

#[test]
fn f16be_with_encoding_is_a_generation_error() {
    // Synthetic: an `encoding:` alongside f16be is a conflict, not a
    // second unit source.
    let page = format!(
        "{}{}",
        SYNTH_PAGE_HEAD,
        synth_f16_field(
            "    unit: \"ratio\"\n    encoding: { kind: scaled_unsigned, lsb_units: \"1/256 degC\" }"
        )
    );
    let err = synthetic_rows(&page).expect_err("must fail");
    assert!(
        err.contains("probe") && err.contains("encoding"),
        "error must name field and the encoding conflict: {err}"
    );
}

#[test]
fn the_four_page14h_ber_rows_emit_f16_ratio() {
    let rows = spec_rows("cmis");
    for name in [
        "host_side_ber",
        "media_side_ber",
        "gated_host_side_ber",
        "gated_media_side_ber",
    ] {
        let row = rows
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} row"));
        assert_eq!(
            row.scaling,
            EmittedScaling::F16 { unit: "Ratio" },
            "{name}: {:?}",
            row.scaling
        );
    }
}
