# Consumer guide — using openxcvr-lib from your code

Guided walkthrough for code that reads a pluggable transceiver, or plans
those reads. You consume this library's generated output; you do not edit
schemas or the generated reference. This page is the order to read. The
linked guides hold the detail.

## Read in this order

1. [Installation](guides/installation.md) — the Rust crate, MSRV 1.82, and each binding's install section.
2. [Concepts](guides/concepts.md) — spec families, `detect` → `Detected` → `UnifiedModule`, fail-closed revisions, and the pure-decision boundary.
3. [Quickstart](guides/quickstart.md) — zero-setup decode, no fixture path and no hardware: `cargo run -p openxcvr --example zero-setup`.
4. [API guide](guides/api-guide.md) — `use openxcvr::prelude::*;`, constructors, the revision report, thresholds, lane flags, and the test kit.
5. [Error handling](guides/error-handling.md) — variants, source chains, and remedies. The tables are the [error catalog](error-catalog.md).

Task-shaped recipes are in the [cookbook](cookbook.md). Whole integration
paths — offline capture analysis, live monitoring through your own
transport, or a managed daemon — are the three
[journey guides](README.md#do-a-task). Consuming the
generated surfaces from another language starts at the per-binding guides
indexed in [bindings/README.md](../bindings/README.md).

## What you are calling

`use openxcvr::prelude::*;` is the Rust front door
([`prelude`](../crates/openxcvr/src/prelude.rs)).
[`detect`](../crates/openxcvr/src/unified.rs) reads the identifier and the
family's revision byte from a transport you own and returns `Detected`.
An unknown identifier or an unknown revision is an error. The library does
not substitute a later revision.

[`UnifiedModule`](../crates/openxcvr/src/unified.rs) is the spec-agnostic
handle. Its methods read through that transport when you call them. They
do not open a device on their own, retry, or schedule I/O. Live admission,
clocks, and execution belong to the caller. See [concepts](guides/concepts.md).

Field meanings at a revision are the generated
[schema reference](reference/index.md). Do not hand-edit those pages.
`revision_report` is the runtime view of which schema fields apply at the
revision detection proved.

## Invariants the guides rely on

- Detection reads the module's own revision byte. Unknown and unspecified codes fail. There is no assume-latest path.
- `open_sff8472_mmap` and `open_sff8636_mmap` are rev-agnostic. `revision_report` returns `RevisionNotDetected` until you open through detection (`open_auto_mmap`, or the family's `open` / CMIS `new`).
- `UnifiedModule::open_custom` does not keep the inner handle's revision. Save the `Rev` from `detect` and pass it to `field_availability`.
- Match error variants, or ask `is_unsupported_revision` / `openxcvr::classify::is_retryable`. Do not parse `Display`.
- A write that may have reached the device is not replayed. Re-read, then decide. The [error catalog](error-catalog.md) is the full table.

## Also from here

- Crate overview and supplied-input planning: [`crates/openxcvr/README.md`](../crates/openxcvr/README.md).
- Fixture corpus: [`tests/fixtures/README.md`](../tests/fixtures/README.md).
- Publication (workspace crates are `publish = false`): [versioning](versioning.md).
