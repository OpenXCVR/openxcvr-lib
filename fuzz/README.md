# Fuzzing

cargo-fuzz targets for the hottest decode entry points. The bounded
property suites in `crates/*/tests/property_generated_surface.rs` and
`crates/openxcvr-cmis/tests/property_cdb_reply.rs` run inside plain
`cargo test` (no nightly, no libFuzzer); the targets here hand the same
surfaces arbitrary unbounded byte streams under coverage-guided fuzzing.

## Targets

| Target | Surface |
| --- | --- |
| `cmis_page_decode` | all 52 union-view CMIS page decoders (`generated::union::*` plus the flat-compatibility pages), 9Fh CDB message page and the A0h–AFh EPL segments included |
| `sff8636_page_decode` | all 7 SFF-8636 page decoders (lower memory, upper pages, PAM4 monitoring, high-accuracy timing) |
| `sff8472_page_decode` | all 6 SFF-8472 page decoders (A0h base memory, A2h diagnostics, user EEPROM, tunability, timing) |
| `cdb_reply_parse` | `Cdb::send` for CMIS CDB 0x0040 over a scripted transport (status / reply length / reply checksum / reply LPL all attacker-controlled) plus `decode_reply` for five representative commands |

Revision-specialised pages (`generated::v3_0` … `v5_4`, `v1_3` … `v2_12`,
`v9_3` … `v12_5`) share the union decoders' shapes and are exercised by the
in-tree property suites instead; extend a target's selector table if you want
them under libFuzzer too.

## Running

```sh
cargo install cargo-fuzz          # once; needs a nightly toolchain
cd fuzz
cargo fuzz run cmis_page_decode -- -max_total_time=60
cargo fuzz run sff8636_page_decode -- -max_total_time=60
cargo fuzz run sff8472_page_decode -- -max_total_time=60
cargo fuzz run cdb_reply_parse -- -max_total_time=60
```

`cargo fuzz` builds with `-Zsanitizer=address` implicitly; a nightly
toolchain must be installed (`rustup toolchain install nightly`).

Compile-checking the targets without cargo-fuzz (stable toolchain is fine —
`libfuzzer-sys` builds its runtime with `cc`, present on this host):

```sh
cd fuzz && cargo check --bins
```

## What counts as a finding

A target finding is a panic, abort, or hang in a decode or CDB reply path.
Non-panic outcomes (`Ok` or typed `Err` such as `SpanError::InvalidIndex`,
`CdbError::ChecksumMismatch`) are by design. Fix defects at the root cause
(generated code comes from `openxcvr-codegen`; schemas are the source of
truth) — never in a target.

## Keeping tables current

The `*_page_decode` selector tables are mechanically derived from the
generated surface: one arm per family re-exported at
`crates/<crate>/src/generated` (union view and flat pages). Reconcile with:

```sh
grep -rl "pub fn decode(buf: &\[u8;" crates/openxcvr-cmis/src/generated \
  | grep -v "/v" | wc -l   # = arms in cmis_page_decode
```
