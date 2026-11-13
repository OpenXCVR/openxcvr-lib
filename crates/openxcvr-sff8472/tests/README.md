# SFF-8472 integration tests

`property_generated_surface.rs` — mechanically derived from
`../src/generated/` — holds one property module per generated page decoder
family (union view, flat pages, and every revision-specialised
`generated::v9_3`…`v12_5` variant; 28 families). Properties, bounded case
counts and the reconciliation recipe are documented in
`../../openxcvr-cmis/tests/README.md`; the shape is identical.

This family carries the recorded `decode_options_supplied` single-index
coverage gap (indexed supplied decoders accept exactly one in-range index
for 16-element array fields; fails closed with `InvalidIndex`) — see the
"Recorded finding" section of `../../openxcvr-cmis/tests/README.md`.

Run: `cargo test -p openxcvr-sff8472 --test property_generated_surface`.
