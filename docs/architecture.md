# Architecture

How openxcvr-lib is put together: what is authored, what is generated, how the
crates depend on each other, and which rules the design enforces. For the
consumer's view of the same model read [concepts](guides/concepts.md) first;
terms are defined in the [glossary](glossary.md).

## One source, many surfaces

```text
schemas/*.yaml                      authored, reviewed against the standard
      │
      ▼
openxcvr-codegen                    parser → IR → validator → emitters
      │
      ├─► crates/*/src/generated    Rust decoders, catalogs, state machines
      ├─► bindings/<lang>/generated C, C#, Go, Java, OpenAPI, PHP, Python, TypeScript
      └─► docs/reference/           per-spec, per-page, per-enum Markdown
```

The [schemas](../schemas/README.md) are the only place a field's offset, type,
revision window or meaning is written down. Every emitter consumes the same
intermediate representation, the same include and template expansion, and the
same revision resolver, so a Rust decoder, a Python decoder and a reference
page cannot be given different answers by construction.

Generated output is committed. `tools/check-regen-equality.sh` regenerates it
and fails when the committed copy differs, which is how "the generated surface
never disagrees with its schema" is enforced rather than hoped for.

## Crate layering

```text
openxcvr                 front door: unified handle, prelude, catalogs, planning
├── openxcvr-cmis        CMIS adapter: pages, CDB, VDM, state machines
├── openxcvr-sff8636     SFF-8636 adapter
├── openxcvr-sff8472     SFF-8472 adapter
├── openxcvr-transports  bus contract, paged windows, HID, i2c-dev, classification
└── openxcvr-core        traits, revisions, errors, catalogs, FSM runtime, test kit

openxcvr-codegen         build-time generator; depends on openxcvr-core only
bindings/c/rich-ffi      Rust FFI crate behind the C binding's rich variant
```

Dependencies point downward only. Each family adapter depends on
`openxcvr-core` and nothing else in the workspace, so a family can be used
alone. `openxcvr` re-exports what a consumer needs
(`openxcvr::core`, `openxcvr::classify`, the three adapter types), which is why one
path dependency is enough.

| Crate | Owns |
| --- | --- |
| [`openxcvr-core`](../crates/openxcvr-core/README.md) | The `Module` and `Transport` traits, `Rev`, the error surface, the schema-derived field catalog, the revision report, threshold evaluation, the state-machine runtime, and the test kit. |
| [`openxcvr-cmis`](../crates/openxcvr-cmis/README.md), [`openxcvr-sff8636`](../crates/openxcvr-sff8636/README.md), [`openxcvr-sff8472`](../crates/openxcvr-sff8472/README.md) | One family each: generated decoders plus the hand-written adapter that implements `Module` over a `Transport`. |
| [`openxcvr-transports`](../crates/openxcvr-transports/README.md) | Synchronous transport building blocks and transport error classification. Device protocols stay in the adapters. |
| [`openxcvr`](../crates/openxcvr/README.md) | Detection, the spec-agnostic `UnifiedModule`, the prelude, and the supplied-input planning modules. |
| [`openxcvr-codegen`](../crates/openxcvr-codegen/README.md) | The generator binary and its emitters. Never linked into a consumer. |

Inside `openxcvr`, each module has one job:

| Module | Job |
| --- | --- |
| `unified` | Identifier routing, revision detection, and the `UnifiedModule` handle. |
| `prelude` | Re-exports for consumers. Adds no behaviour. |
| `catalog` | One facade over the per-family generated field catalogs. |
| `live` | Layout discovery a caller directs, with a pure supplied-input companion. |
| `observation` | Basic observations decoded from a byte snapshot the caller supplies. |
| `rich` | Rich records built from supplied data. |
| `raw`, `raw_write` | Pure preflight and execution plans for raw access. No I/O. |
| `programming` | Pure preflight for vendor-password admission. |

## Runtime model

Two traits carry the design. A [`Transport`](../crates/openxcvr-core/src/transport.rs)
reads and writes module memory; the caller supplies it.
A [`Module`](../crates/openxcvr-core/src/module.rs) answers spec-agnostic
questions (identity, lanes, flags) and is implemented once per family.

A decode proceeds in three steps:

1. **Route.** `dispatch_identifier` maps the SFF-8024 identifier byte to a
   family. Routing is not revision admission.
2. **Detect.** `detect` reads the family's revision byte and returns
   `Detected { kind, rev }`, or an error. No revision is assumed.
3. **Open and read.** `UnifiedModule` wraps the family adapter. Its methods
   read through the transport when called and decode with the generated
   tables for the detected revision.

A second path serves callers that own acquisition entirely, such as a daemon
with its own scheduler. The planning functions return the byte ranges a
decode needs; the caller reads them however it likes; the supplied-input
functions decode exactly those bytes. No transport is involved.

## Rules the design enforces

These are checked, not aspirational.

- **Schemas are the source.** Generated files are never edited by hand;
  regeneration equality is a test.
- **Fail closed.** An unknown identifier, an unknown revision or an
  unspecified compliance code is an error. There is no assume-latest path.
- **Parse and decide, never act.** No routine opens a device, retries, sleeps,
  keeps a resident cache or schedules I/O on its own initiative. Live module
  authority belongs to the caller. The bundled transports are caller-directed
  mechanisms: the `linux-i2c` transport opens the device path it is given, and
  its page-select hold and post-write acknowledge poll sleep for a bounded
  interval, only when the caller constructs and calls it.
- **A golden is never bent to pass a test.** Golden outputs under
  [`tests/golden/`](../tests/golden/README.md) are the evidence of decode
  fidelity. A failing comparison is investigated, not regenerated away.
- **Documentation is checked.** Pages are validated and their Rust examples
  compiled; see [contributing](contributing.md).

## Known seams

Stated so that a reader does not mistake them for intent:

- Linux I2C access exists in two crates: a page-blind transport in
  `openxcvr-core` behind the `linux-i2c` feature, and the i2c-dev bus in
  `openxcvr-transports`.
- Observation decoding is split between `openxcvr-core` and `openxcvr`, with
  near-identical module descriptions.
- `docs/` holds user documentation, the generated reference and project
  records side by side; the [documentation map](README.md) separates them.
