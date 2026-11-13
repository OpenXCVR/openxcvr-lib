# Concepts

What openxcvr-lib does, which spec families the unified handle covers, and
what it refuses to guess. Install first ([installation](installation.md)),
then run the zero-setup decode ([quickstart](quickstart.md)). Signatures
and copyable examples are the [API guide](api-guide.md).

## What the library does

The library parses transceiver memory maps and answers questions about
them. A call such as `vendor_info` or [`detect`](../../crates/openxcvr/src/unified.rs)
reads bytes through a transport the caller already holds. Nothing in the
library opens a device, retries a transfer, arms a timer, or keeps a
resident cache on its own initiative.

Pure routines return decisions: a revision, a decoded value, a threshold
evaluation, a field-availability report, or a plan of which ranges to read.
Acting on a live module — admission, clocks, scheduling, cache storage,
epochs — belongs to the caller. For a managed module that caller is
the process that owns the modules.

The schemas under [`schemas/`](../../schemas/README.md) are the source of
those decisions. Generated Rust, bindings, and the
[schema reference](../reference/index.md) are output. Do not hand-edit
generated pages to change a field's meaning.

## Spec families

Three families share one [`UnifiedModule`](../../crates/openxcvr/src/unified.rs)
surface. SFF-8024 is the identifier registry used to route them; it is not
itself a module variant.

| Family | Modules the dispatcher routes here | Revision byte the library trusts |
| --- | --- | --- |
| CMIS | QSFP-DD / OSFP when byte 1 is 0x30 or ≥ 0x40, plus pure-CMIS identifiers (DSFP 0x1B and 0x1E–0x25) | Lower memory byte 1, packed BCD (`0x52` is 5.2). Known: `0x30`, `0x40`, `0x50`–`0x54`. |
| SFF-8636 | QSFP+ / QSFP28 (0x0C, 0x0D, 0x11, 0x17) and legacy QSFP-DD/OSFP maps (byte 1 below 0x40, except 0x30) | Lower memory byte 1, SFF-8636 Table 6-4. `0x00` (unspecified) is refused. |
| SFF-8472 | SFP / SFP+ / SFP28 (identifier 0x02 or 0x03), addresses A0h and A2h | A0h byte 94, Table 8-8. `0x00` (unspecified) is refused. |

`dispatch_identifier` only chooses the family. It does not admit the
revision. A QSFP-DD whose byte 1 is `0x00` is routed to SFF-8636, and
revision detection then still fails closed because `0x00` is not a known
SFF-8636 compliance code.

The generated reference also renders namespaces the unified handle does
not open as their own family: C-CMIS, ELSFP, CMIS-LT, CMIS-VCS.
[`family_catalog`](../../crates/openxcvr/src/catalog.rs) returns `None`
for `Family::CCmis` and `Family::Elsfp`. There is no field-availability
report for them, and the library will not invent one.

## Detect, then open

[`detect`](../../crates/openxcvr/src/unified.rs) reads byte 0 (SFF-8024
identifier) and the family's revision byte, and returns `Detected { kind, rev }`.
It holds no state; the transport stays yours.

[`UnifiedModule`](../../crates/openxcvr/src/unified.rs) is then built in
one of three ways:

- **Detect-once.** `open_auto_mmap`, CMIS `open_cmis_mmap` / `CmisModule::new`,
  or `Sff8472Module::open` / `Sff8636Module::open`. Later reads use the
  revision those constructors proved. `detected_rev()` is `Some`.
- **Rev-agnostic memory map.** `open_sff8472_mmap` and `open_sff8636_mmap`
  call the family's `new`, not `open`. They decode through the union of
  generated fields. `detected_rev()` is `None`.
- **Custom.** `open_custom` boxes any [`Module`](../../crates/openxcvr-core/src/module.rs)
  you built over your own transport. The unified handle does not copy the
  inner revision. `detected_rev()` is `None` even when the adapter you
  boxed had already detected one. Keep that `Rev` yourself.

## Fail closed

An unknown identifier is `ModuleError::UnknownIdentifier`. An unknown or
unspecified revision byte is `ModuleError::Detection` carrying
`DetectError::UnsupportedRev` (the byte, and the revisions this library
knows for that family). `is_unsupported_revision()` is true only for that
case.

There is no path that substitutes the newest revision the crate was built
with. `revision_report` on a handle with no detected revision returns
`RevisionReportError::RevisionNotDetected` instead of filling the report
from a guess. `field_availability` refuses a revision the catalog does not
list, including a revision of another family.

## Decisions, not actions

[`evaluate`](../../crates/openxcvr-core/src/threshold.rs) turns one
measurement and a threshold set into alarm/warning booleans. It does not
read a module and it does not clear a latched flag.
[`field_availability`](../../crates/openxcvr-core/src/revision_report.rs)
classifies schema field names at a revision you pass; it does not open a
bus. Read plans in `openxcvr::observation` (see the
[crate README](../../crates/openxcvr/README.md)) list ranges and refusals.
The caller performs the I/O. The C binding mirrors the basic-observation
plan pair as `openxcvr_basic_read_prelude` and
`openxcvr_basic_read_remaining`
([C binding](../../bindings/c/README.md)).

Transport backends are mechanisms. The bus contract forbids them from
retrying internally, because a retried write could be applied twice.
[`openxcvr::classify`](../../crates/openxcvr-transports/src/classify.rs)
only says whether resubmitting is safe.

## Where the schema reference fits

[docs/reference/index.md](../reference/index.md) is generated from the
same schemas as the decoders. Use it for page layout, enum labels, and
which revision a field applies to. Use `revision_report` or
`field_availability` when the program needs that answer for the revision
it just detected. A name the schema never declared is absent from the
report (`field` returns `None`). A name that exists only at other
revisions stays in the report as gated.

## Where next

[Quickstart](quickstart.md), then the [API guide](api-guide.md). Failures
and remedies: [error handling](error-handling.md) and the
[error catalog](../error-catalog.md).
