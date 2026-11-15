# openxcvr-lib — intent

openxcvr-lib is the OpenXCVR library implementation and validation-tooling repository: the schemas,
the Rust crates and code generators, the generated bindings, the fixtures and golden outputs, and
the documentation validator the rest of the workspace is judged by.

## Purpose

This repository makes the schema the single source of truth for transceiver memory maps and turns it
into everything downstream: Rust crates, generated bindings for other languages, fixtures, golden
outputs, and emitted documentation. It also owns the cross-repo documentation validator, so the
standard other repositories are held to is implemented here rather than argued about in review.
Project coordination, decision records, and status history belong to the `openxcvr` repository; this
one points at them and does not duplicate them.

Its purpose includes reusable runtime management logic: generated revision-aware CMIS state
machines; pure lifecycle, readiness, scheduling/fencing, operation and thermal decisions over
supplied observations/history/time/capabilities/results; and schema-driven read classification,
observation planning and cache algorithms over caller-owned entries. The library parses and
decides and never acts on a module: pure routines return decisions/state/effects;
the OpenXCVR daemon owns authoritative live instances, history, cache storage, epochs, clocks,
scheduling, admission and execution. Existing caller-directed transport helpers remain mechanisms,
not autonomous state owners.

## Top invariant

Three things must never happen, in the order they outrank one another.

A generated surface never disagrees with its schema. Crates, bindings, fixtures, goldens, and
emitted documentation are all output, and a parity failure between an output and its schema is a
defect in the generator or in the schema — never something to patch in the output.

The documentation validator never passes a false claim. Other repositories rely on its verdict, so a
document naming a missing file, a stale inventory, or a broken cross-repo link is an error rather
than a warning, and an input it cannot read is inconclusive rather than clean.

A golden is never bent to make a test pass. Fixtures and goldens are canonical evidence: editing an
expected output to clear a failure is a defect unless a schema decision says the meaning really
changed.

## Non-goals

This is not the project's record. Work logs, repository maps, decision records, status trees, and
coordination documents live in the `openxcvr` repository.

This is not a kernel ABI authority.
openxcvr-lib may generate internal behaviour tables that the OpenXCVR kernel module compiles in; the kernel UAPI and ABI remain owned by the kernel module.

This is not a home for commercial or private material. No licensing, control-plane, or enforcement
code, and no vendor-derived captures, land in this open repository.

## Audience

Schema and code-generation authors, who work in the schema language rather than in generated code.
Consumers of the generated bindings — the kernel module, the userland library, the packaging. Every
repository whose documentation this repository's validator judges. Agents dispatched into schemas or
generators, which must not hand-edit what a generator owns.

## Core bet

Generation can be trusted enough to be the only source. One schema language with a canonical Rust
emitter is worth its standing cost because every additional specification family, page layout, or
binding language is then schema work instead of new hand-written decoders — and because one
generator is a single place to be correct, where a hundred hand-written parsers are a hundred
places to be subtly wrong.

## Direction

The code-generation migration is being finished, so generated surfaces replace hand-written parsers
and bindings across every schema namespace. Coverage is widening — C-CMIS, ELSFP, and the CMIS
version-compatibility framework — so new module families land as schemas. Parity and golden testing
is hardening, with every namespace decoded from real fixtures and compared against canonical
goldens. The documentation validator's reach is extending across more of the workspace's documents
and more classes of drift.

## Posture

Generated over hand-written: if a generator can emit it, nobody types it. Evidence over assertion:
fixtures and goldens decide, not a reading of the code. Fail closed: an unreadable input is reported
as inconclusive, never as clean. The schema is the argument of record — a disagreement about a field
is settled by changing the schema, not by patching around it.

## Parent intent

parent_intent: ../openxcvr/INTENT.md https://github.com/OpenXCVR/openxcvr/blob/main/INTENT.md precedence=narrows

This artifact sits under the OpenXCVR stack intent in the `openxcvr` repository, named on the line
above. Those invariants and non-goals bind here: this file may add to them and narrow them, but it
may not contradict them. Nothing in git enforces that — the repositories are independent clones —
so it holds as stated policy: an agent that finds this artifact, or a change proposed here, in
conflict with the parent stops and puts the conflict to the owner instead of choosing a side.
