# openxcvr-lib documentation

The map of this repository's documentation, organized by what you came to do.
If you have not run the library yet, start with the
[repository README](../README.md): it has a one-command first run.

## Learn the library

Read these in order. The [consumer guide](consumer-guide.md) is the same path
with the invariants the guides rely on.

1. [Installation](guides/installation.md) — toolchain, path dependency, binding installs.
2. [Concepts](guides/concepts.md) — spec families, detection, fail-closed revisions, and the parse-and-decide boundary.
3. [Quickstart](guides/quickstart.md) — decode an embedded sample with no hardware.
4. [API guide](guides/api-guide.md) — constructors, the revision report, thresholds, lane flags, and the test kit.
5. [Error handling](guides/error-handling.md) — which variant means what, and what to do about it.

## Do a task

- Journey guides — one complete integration path each, from zero to a
  working program: [offline analysis](guides/offline-analysis.md)
  (decode captured memory, no hardware),
  [live monitoring](guides/live-monitoring.md) (poll a live module
  through your own transport) and
  [managed operations](guides/managed-operations.md) (a daemon that
  plans reads, state transitions and commands and executes them itself).
- [Task cookbook](cookbook.md) — one page per job: identify a module, read
  vendor data, decode VDM, evaluate thresholds, plan a state transition, issue
  a CDB command, test without hardware.
- [Crate overview](../crates/openxcvr/README.md) — the fixture-backed
  quickstart and the supplied-input planning surface for callers that own
  their own I/O.

## Look something up

- [Generated reference](reference/index.md) — every schema field, page and
  enum, per specification and revision. Generated; never edited by hand.
- [Error catalog](error-catalog.md) — per-variant meaning and retry guidance.
- [Glossary](glossary.md) — the terms these pages use.
- [Fixture corpus](../tests/fixtures/README.md) and
  [golden outputs](../tests/golden/README.md) — the test data and where it
  came from.

## Use another language

[bindings/README.md](../bindings/README.md) indexes the C, C#, Go, Java,
OpenAPI, PHP, Python and TypeScript surfaces and states the facade contract
they share.

## Understand and change the library

- [Architecture](architecture.md) — schemas, the generator, crate layering,
  and the rules the design enforces.
- [Contributing](contributing.md) — the check set, regeneration, schema edits,
  and how to write a page that passes validation.
- [Testing](testing.md) — focused regression loops and what each layer proves.
- [Versioning](versioning.md) — how versions are named, and what is not published.
- [Schema authoring](../schemas/README.md) — layout and authoring rules.
- [Connector locking design](connector-interfaces/locking-design.md) — the
  cooperative lock the transports are intended to provide. It is a design,
  not a claim that the code already does all of it.

## How these pages stay true

- `cargo test --doc -p openxcvr` compiles the Rust examples in the guides, the
  cookbook and the READMEs, and runs those that need no fixture file.
- The reference under `reference/` is regenerated from the schemas, and a
  check fails when the committed copy differs from what the generator emits.

[Contributing](contributing.md) has the commands.
