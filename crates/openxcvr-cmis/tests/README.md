# CMIS integration tests

## Property suites (hostile-input totality)

`property_generated_surface.rs` — mechanically derived from
`../src/generated/` — holds one property module per generated page decoder
family (union view, flat pages, and every revision-specialised
`generated::v3_0`…`v5_4` variant; 214 families). Per family it proves:

* `decode(&[u8; N])` accepts arbitrary bytes of the declared page size
  without panicking and is deterministic (two calls render identically).
* every fixed-size mutator accepts any value (derived from an arbitrary
  seed; includes `i16`/`i8` negatives and `From<u16>` enums) without
  panicking, and the page still decodes afterwards.
* every indexed setter accepts in-range lane indexes, refuses every
  out-of-range index (through `usize::MAX`) with
  `openxcvr_core::supplied::SpanError::InvalidIndex`, and leaves the buffer
  byte-identical on refusal.
* every `decode_*_supplied` returns `Ok` for each field when the full page
  is supplied at its base offset, at least one in-range index decodes for
  indexed variants, negative indexes are `InvalidIndex`, and arbitrary
  `(bytes, offset, index)` triples never panic.

`property_cdb_reply.rs` proves the CDB reply parser end to end through
`Cdb::send` over a scripted transport: arbitrary reply bytes never panic
(every outcome is a response or a typed `CdbError`); busy status yields
`Timeout`, failure statuses `Failed`, completion statuses proceed; reply
checksum follows CMIS 5.3 Table 8-179 (one's complement of the body sum,
only verified when a body is present); reply length bounds are enforced
(`LplTooLong` above 120, `TruncatedReply` below the command's fixed
layout); and `decode_reply` is total for five representative commands.

Case counts are bounded (24–64 per property) so `cargo test` stays fast;
longer campaigns belong to `../../fuzz/` (cargo-fuzz targets).

### Keeping the table current

The family table lists every `pub fn decode(buf: &[u8; N]) -> Self` surface.
Reconcile after a regen with:

```sh
grep -rl "pub fn decode(buf: &\[u8;" ../src/generated | grep -v catalog.rs | wc -l
# modules in the test file: grep -c "^mod p_" property_generated_surface.rs
```

Both numbers must match; add one `mod p_*` per new family following the
existing shape (the union/flat/rev path is the module path the generator
re-exports it at).

### Recorded finding (not a defect fix)

`decode_options_supplied` on the SFF-8472 A0h page (all revisions) accepts
exactly one in-range index (2) while `decode` yields all 16 `options`
elements; the generated indexed-supplied surface under-covers array fields.
It fails closed (`InvalidIndex`) rather than mis-decoding, so the property
suite asserts the fail-closed contract (≥1 valid index, typed errors), not
full index coverage. Widening the indexed-supplied emitter is a generator
change to schedule separately (see `openxcvr-codegen`
`emitters/rust/pages.rs`, the `decode_{name}_supplied(... index ...)` arm
builder).
