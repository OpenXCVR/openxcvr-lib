# Changelog

All notable changes to `openxcvr-lib` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project uses
Cargo semver, so while the workspace is pre-1.0 every breaking change bumps
the minor version.

`0.2.0` is the first versioned release. The repository has no earlier release
tag, so this entry covers the full history since the library split from
`openxcvr-meta` (`014d217`, 2026-06-10). Commit hashes are cited for every
breaking item so each claim can be traced to its landed change.

## [Unreleased]

### Changed

- OpenAPI `info.version` is `0.4.0`. `CmisLaneStatus` requires `rx_los` and
  `tx_fault`; a required-key addition is breaking, and the v0.2/v0.3 scope
  narrative had not recorded it.
- Regeneration equality now refreshes
  `bindings/typescript/generated/openapi/cmis.ts` through `openapi-typescript`
  (`tools/regen-typescript.sh`, via `tools/regen-all.sh`). A stale artifact
  fails the gate. `npm test` already failed on that drift
  (`bindings/typescript/tests/openapi_types.test.mjs`).
- Changelog MSRV wording corrected to the live gate: blocking
  `rust-version = 1.82` (`.github/workflows/publish-check.yml` job `msrv`),
  not a non-blocking 1.75 job. See `docs/versioning.md`.

### Added

- **Consumer guide** (`docs/consumer-guide.md`, registered as a required
  documentation entry point): architecture walkthrough (detect → decode →
  decide → planned effects; execution ownership stays with the daemon),
  per-family quickstarts pointing at the runnable examples, revision handling
  (fail-closed, never assume-latest), and the error-handling guide.
- **Error catalog** (`docs/error-catalog.md`): every consumer-facing error
  type and variant mapped to meaning and retry guidance.
- **Error predicates**: `ModuleError::is_unsupported_revision()`,
  `DetectError::is_unsupported_revision()`,
  `EncodeError::is_unsupported_in_revision()`,
  `MgmtError::is_unsupported_in_revision()`,
  `openxcvr_transports::Error::is_timeout()` / `is_retryable()`, and
  `openxcvr_transports::classify` (`transport_failure` / `is_timeout` /
  `is_retryable` over `ModuleError`, re-exported as `openxcvr::classify`).
- **Capture distillation** (`tools/distill-capture.py` +
  `tools/test_distill_capture.py`): scrubs a private raw module capture into
  synthetic-fixture builder inputs (vendor identity replaced, layout
  preserved, checksums recomputed) so real-module evidence can grow the
  golden corpus without vendor-derived captures landing in the repository;
  workflow documented in the tool docstring and `tests/fixtures/README.md`.

### Fixed

- C-CMIS 64-bit PM/FEC counters were decoded little-endian; they now decode
  MSB-first as the specification states.
- **BREAKING** The SFF-8472 revision domain now matches Table 8-8 of the
  specification (checked against the Rev 12.3, Rev 12.4 and Rev 12.5a
  documents): compliance byte `0x08` decodes to Rev **12.3** (previously
  "12.0", a label none of the cached documents gives any code; Linux
  `include/linux/sfp.h` still names 0x08 `SFP_SFF8472_COMPLIANCE_REV12_0`) and
  `0x09` to Rev **12.4**
  (previously "12.3"). The public constant `Rev::SFF8472_12_0` is removed
  with no alias, and `Rev::SFF8472_12_4` is added; a module reporting 12.3
  or 12.4 compliance previously decoded to the wrong revision label.
  SFF-8472 field windows were re-qualified against the cached documents:
  the equalization controls (A2h 114-115) move from `12.0` to `12.3`, the
  byte-62 64GFC register, the A2h 118.4/118.2 bits and the Section 9.6
  enhanced-feature fields move from `12.5` to `12.4`, and the RDT value
  (A2h page 02h byte 131) moves from `12.5` to `11.4`. The generated
  per-revision modules rename accordingly (`v12_0` disappears, `v12_4`
  appears) on every binding surface.

## [0.2.0] — 2026-11-11

### Specification coverage

Seven specification namespaces landed or advanced, integrated as
`integration/specs-rollout` (`4939653`):

- **CMIS 5.4** — CDB command deltas CDB-01..12 (`e8a6b3e`), VDM deltas with
  repaired VDM since windows VDM-01..03 (`0109e2e`), VDM enumeration gated on
  advertised groups (`6b9d786`), synthetic VDM/threshold/mask fixtures and
  golden (`4741e95`).
- **C-CMIS 1.5** — new namespace for OIF-C-CMIS-01.5 (`a51d2e8`), registered
  as a companion spec in codegen (`67da12e`), markdown reference surface
  (`a8762d9`).
- **CMIS VCS 1.1** — new namespace for OIF-CMIS-VCS-01.1 (`8a3fb86`), codegen
  registration (`98a56ad`), reference render (`f14f546`).
- **CMIS FF 1.0** — new namespace for OIF-CMIS-FF-01.0 page 05h (`9f80672`),
  codegen registration (`1788e23`).
- **CMIS LT 1.0** — new namespace for OIF-CMIS-LT-01.0 pages 08h, 50h-53h
  (`edefebf`), tools registration (`9b30830`), bindings and markdown surfaces
  (`f0c7ea0`).
- **SFF-8024 4.14** — registry advanced to Rev 4.14 (`cf71f72`), emitter
  expectations pinned (`d668bc0`), all surfaces regenerated (`2fd3d9b`).
- **SFF-8636 2.12** — namespace advanced to Rev 2.12 with optional page 22h
  (High Accuracy Timing) (`a56d095`), revision domain registration
  (`4ef2921`).

### Any-version unified interface

One entry point that detects the module family and revision instead of the
caller pre-picking a typed API:

- `openxcvr::unified::detect(transport)`: pure two-byte family routing plus
  the family's revision byte, failing closed (`DetectError::UnsupportedRev`)
  on unknown identifiers or unspecified revisions — never an assume-latest
  fallback (`967a03d`).
- `open_auto` any-version factory over every family (`967a03d`).
- Any-version SFF layout detection through the detect machinery (`edd3bca`,
  merge `c07c403`).
- SFF adapters: detect-once `Ctx`, per-revision compiled dispatch, canonical
  projection (`8f19d52`).
- Schema revision-lineage windows (`since:`) with per-revision generated
  decoders on every surface (`cfc5da0`).
- Family-aware revision domain (SFF-8636 + SFF-8472) with CMIS parity
  preserved (`f87d876`); SFF field-catalog per-rev spans, family-domain
  checked pages, tier-2 legacy SFF-8079 application-select windows
  (`1d8ef1d`, `a97adb0`).

### Stateful runtime (generated FSMs and pillars)

The library's runtime-management surface: pure decisions over caller-supplied
observations, generated from the schema, with no autonomous I/O.

- Pure generic FSM runtime types and evaluators (`7469bba`).
- CMIS adapters consume the generated FSMs: dedup, revision gating, planners
  (`2053d7b`).
- FSM v2: Resetting/Reset/MgmtInit modelled as unreported states (schema v2 +
  machine-space emission) (`ae6b3e4`), `step_transition` and family readiness
  (`9911dc3`).
- Runtime-management pillar slices: thermal, lifecycle, cache, retry, CDB
  fencing (`a93be13`), deepened scheduling/cache/operations contracts
  (`6f33646`), scheduling/admission remainder, operations/cache plans,
  MgmtInit→ModuleReady edge, readiness poll facade (`ef0a7cf`).
- SFF readiness: sff-8636/sff-8472 pure monitor-data readiness walks (DNR
  predicate + poll plan) (`e4330f1`), byte-2 startup-interrupt acknowledge
  read effect (`b18c017`), sff8636 Page 03h threshold evaluation
  (`c4ed9ce`), LaneStatus per-lane signal flags and sff8472 adaptive-EQ
  (`a157a43`).
- C behaviour-table emitter for the generated state machines
 with runtime compile gate (`b8a4806`).

### Error/mutator hardening — BREAKING

- Generated mutators now take the page's fixed-size `&mut [u8; N]` (N =
  declared page size) instead of raw slices: a short buffer is a compile
  error, never a runtime panic; instanced/indexed setters validate the index
  and return `Result` (`54ca395`).
- Typed error causes preserved through `ModuleError` and the CMIS error
  enums (`d4625c0`); integration adaptations (`4eb0be1`), merge (`6e4a24f`).

### API ergonomics — BREAKING

- Removed the placeholder `Module::cdb_run` / `Module::data_path_states`
  defaults and the empty `DataPathState`/`CdbCommand`/`CdbReply` stub types:
  they advertised a capability nothing implements; the typed CDB API is the
  `CmisModule` `cdb_send` path and data-path state is exposed via `lanes()`
  and the DPSM (`80a8b61`).
- Error types made usable and extensible: programming refusal errors carry
  nested sources and a current CDB claim (`1da1e9d`), public refusal types
  implement `Display`/`std::error::Error` (`d903a2a`), nested error sources
  preserved and extensible public errors marked `#[non_exhaustive]`
  (`0b2c692`).
- Non-breaking additions from the same wave: `UnifiedModule::Custom` boxed
  `dyn-Module` route for user transports (`4495fc1`), `missing_docs`
  completed and enabled across core/sff/transports (`a5d3b4e`, `0beb4db`),
  `must_use` on pure decisions (`4688fd1`); merge (`89c366e`).

### Bindings VDM parity

VDM facades over CMIS Pages 20h-2Fh with golden parity in every binding
(merge `f24f2bc`):

- C (`89fd977`), C# (`530f874`), Java (`1b1fa69`), PHP (`443ddd9`),
  TypeScript (`92e5867`), Python (`11f50ce`), Go (`a364d48`); earlier
  Python/Go VDM parity wave merge (`fdd6401`).

### Onboarding

Quickstart and per-language onboarding docs (merge `d18ed1b`): runnable
`openxcvr` example and repo-root precondition (`a3356cc`), Python public
facade/install route/page dictionary keys (`0634c16`), TypeScript
`decodeCmis` quickstart and local distribution route (`da43018`), Go README
with input struct conventions (`93ede71`).

### CI and fuzzing

- Supply-chain gates: cargo-deny (licenses/advisories/bans/sources, `deny.toml`)
  and cargo-semver-checks armed on the first release tag (`5037f42`).
- MSRV job for `rust-version = 1.75` with MSRV-aware lockfile resolution
  (`9e53833`). Non-blocking when that commit landed; it is not the live gate.
  The live job is blocking `rust-version = 1.82` (`publish-check.yml` job
  `msrv`); see `docs/versioning.md`.
- Standalone docs reference-scope validation job (`8ec603c`); `go vet` in
  the parity matrix (`73d18fe`).
- Property/fuzz: proptest suites for every generated page decoder family and
  the CMIS CDB reply parser (`c5c83eb`, `7e418b2`), cargo-fuzz targets for
  the hottest decode entry points (`56d30c8`); merge (`0b34364`).
- Regeneration equality gate, clean-by-default and fail-closed snapshotting
  (`4410e69`, `7ed5e6c`); registered CTest suite for the C binding
  (`e38f738`); workflow token permissions limited to read (`c9c2902`).
