# SFF-8636 integration tests

`property_generated_surface.rs` — mechanically derived from
`../src/generated/` — holds one property module per generated page decoder
family (union view, flat pages, and every revision-specialised
`generated::v1_3`…`v2_12` variant; 36 families). Properties, bounded case
counts and the reconciliation recipe are documented in
`../../openxcvr-cmis/tests/README.md`; the shape is identical.

Run: `cargo test -p openxcvr-sff8636 --test property_generated_surface`.
