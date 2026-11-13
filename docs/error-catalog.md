# Error catalog — meaning and retry guidance per public error type

This catalog maps the operational Rust error types — module I/O and decode,
revision detection, transport classification, supplied-data observation, and
the revision/field-availability report — to their meaning and retry guidance:
which failures are deterministic (a retry of the identical call cannot change
the outcome), which may clear on their own, and which must never be replayed.
Retry *policy* — delay, backoff, attempt budget — always stays with the caller;
this catalog is what the caller decides against.

The control flow (source chains, `is_unsupported_revision`, retryability, and
remedies) is [error handling](guides/error-handling.md). The architecture is
the [consumer guide](consumer-guide.md). A runnable tour is
[`crates/openxcvr/examples/error-handling.rs`](../crates/openxcvr/examples/error-handling.rs).
`openxcvr_core::testkit::TestKitError` is the page-builder and assertion
failure used by tests; it is not a module I/O error. See the
[API guide](guides/api-guide.md#test-kit).

Ground rules:

- Variants decide control flow; messages are for logs. Typed causes are kept in
  `Error::source` chains and should be walked, never parsed.
- The library never retries internally (the transport bus contract forbids
  backends from retrying a possibly applied write). Every retry is a caller
  decision, so every retryable condition is named here.
- A write that may have reached the device (`write_may_have_applied`) is never
  replayed implicitly. Re-read the affected bytes first; that rule has no
  exception.

## `openxcvr_core::error::ModuleError` — the unified module surface

| Variant | Meaning | Retry guidance |
| --- | --- | --- |
| `Io(IoError)` | Transport-layer read/write failure; the typed cause stays in the source chain. | Classify via [`openxcvr_transports::classify`](#openxcvr_transports--the-transport-layer) when the cause is a transport error: `is_retryable`/`is_timeout`. Otherwise treat as opaque: retrying a **read** may succeed once the bus recovers; never blindly replay a failed write (see the write rule above). |
| `Unsupported` | The field or capability is not exposed by this module family or revision. The message points at `detected_rev()`. | Deterministic on this module. Not a retry. `is_not_exposed()` is true. Use [`revision_report`](../crates/openxcvr/src/unified.rs) / [`field_availability`](../crates/openxcvr-core/src/revision_report.rs) to see whether a field is revision-gated or absent from the schema. |
| `UnsupportedField { site }` | The named page, I2C address, offset and field are not exposed. The message includes that site and points at `detected_rev()`. `ModuleError::site()` returns the site; `is_not_exposed()` is true. | Deterministic. Not a retry. Same revision-report remedy. Not an unknown revision (`is_unsupported_revision()` stays false). |
| `Decode(DecodeError)` | Bytes were read but did not decode into the requested field (checksum, range, encoding). When the decoder knew the site, `DecodeError::at` / `ModuleError::decode_at` put the page, I2C address, offset and field in the message and in `site()`; the typed source chain is unchanged. | Deterministic on the same bytes. A re-read can only help if the bytes themselves were transiently wrong; for stable EEPROM evidence, fix the input. |
| `UnknownIdentifier(u8)` | The SFF-8024 identifier byte does not name a known module family. | Deterministic on the same byte. Not a retry condition; check the address/bytes read. |
| `Detection(DetectError)` | Revision detection failed; see [`DetectError`](#openxcvr_coreversion_error--revision-gates). | Follow the inner `DetectError`: unsupported-revision is deterministic; transport-caused follows the transport guidance. |

**Predicate:** `is_unsupported_revision()` — true when the module's reported
revision byte is not one this library knows. Remedies: a library that knows the
revision, or the vendor's tooling. Never a retry, never an assumed revision.

## `openxcvr_core::version_error` — revision gates

| Type / variant | Meaning | Retry guidance |
| --- | --- | --- |
| `DetectError::Transport(ModuleError)` | The transport read that probes the revision byte failed. | Transport guidance (see [`ModuleError::Io`](#openxcvr_coreerror-module_error--the-unified-module-surface)). The probe is a plain read; retrying it is safe once the transport condition clears. |
| `DetectError::UnsupportedRev { detected_byte, known_revs }` | The revision byte does not map to a revision this library knows (including the family's unspecified code); carries the byte and the known domain. | Deterministic on this module. Never assume a revision it does not report (fail-closed). Predicate: `is_unsupported_revision()`. |
| `EncodeError::UnsupportedInRev { field, available_since, detected }` | The field is not writable at the revision being encoded (it may exist only in later revisions). | Deterministic at that revision; encode for a revision where the field exists, or skip it. Predicate: `is_unsupported_in_revision()`. |
| `MgmtError::NotSupportedInRev { op, detected }` | The management operation does not exist at the revision in force. | Deterministic at that revision. Predicate: `is_unsupported_in_revision()`. |

## `openxcvr_core::revision_report::RevisionReportError` — field availability refused

[`RevisionReportError`](../crates/openxcvr-core/src/revision_report.rs) is
returned by `UnifiedModule::revision_report` and by `field_availability`.
The report is not filled with a guess. None of these is a retry.

| Variant | Meaning | Retry guidance |
| --- | --- | --- |
| `RevisionNotDetected` | `detected_rev()` is `None`: a rev-agnostic mmap constructor (`open_sff8472_mmap`, `open_sff8636_mmap`) or `open_custom`. | Detect first (`detect`, `open_auto_mmap`, the family `open` / CMIS `new`) and keep that `Rev`. Do not substitute the newest `Rev` constant. |
| `NoCatalog { family }` | The family has no generated schema catalog (C-CMIS and ELSFP: `family_catalog` returns `None`). | Not a retry. Field availability is not guessed. |
| `UnsupportedRevision { rev }` | `rev` is outside the catalog's supported set, or it belongs to another family than the catalog. | Not a retry. Pass a revision from `Rev::known_for` for that family. |

## `openxcvr_transports` — the transport layer

[`Error`](../crates/openxcvr-transports/src/lib.rs) is the one error type at the
transport boundary: `kind` decides control flow, `detail` is for logs, and
`write_may_have_applied` marks a failure at or after a write boundary.

| `ErrorKind` | Meaning | Retry guidance |
| --- | --- | --- |
| `Invalid` | A caller-supplied argument is out of range or self-contradictory; nothing was sent. | Deterministic: fix the argument. |
| `Unsupported` | The address, page, bank or operation does not exist for the supplied layout. | Deterministic: fix the layout/address. |
| `Busy` | The device/node is owned by someone else (i2c-dev `EBUSY`, a bound kernel driver, another process's lock, an unpowered module). | May clear by itself once the owner releases; resubmission is safe (`is_retryable` = true). |
| `PermissionDenied` | The device exists but this process may not open or address it. | Not a retry; needs rights. |
| `Disconnected` | No such device, or it was removed or lost power during the transfer. | Not a blind retry; re-enumerate and re-open, then resubmit. |
| `Timeout` | The transfer deadline expired before completion. | Resubmission is safe and may succeed (possibly with a longer deadline). |
| `Nack` | The slave did not acknowledge (`ENXIO`/`EREMOTEIO`) — often an unpowered or momentarily unready module. | Resubmission is safe; a persistent Nack behaves like `Disconnected`. |
| `Protocol` | The device answered but the bytes violate the protocol. | Not a retry; the device or the path is wrong. |
| `ShortTransfer` | Fewer bytes moved than requested. | Not a blind retry; investigate the bus before resubmitting. |
| `Io` | Any other backend failure, as text in `detail`. | Not classifiable here; inspect `detail`/the backend cause. |

**Predicates on `Error`:** `is_timeout()` (deadline expired);
`is_retryable()` — true only for `Busy`, `Timeout` and `Nack` **and** only when
`write_may_have_applied` is unset (replaying a possibly-applied write could
double-apply it; re-read the affected bytes instead).

**Classification of transport-caused `ModuleError`s**
([`classify`](../crates/openxcvr-transports/src/classify.rs), re-exported as
`openxcvr::classify`): `transport_failure(&ModuleError)` walks the source chain
to the transport error; `is_timeout(&ModuleError)` and
`is_retryable(&ModuleError)` answer through it. A `ModuleError` with no
transport failure in its chain answers `false` to both — inspect it directly.
These predicates say resubmission is safe, never that it is wise: clear-on-read
locations are excluded from read plans before any transfer, and re-reading one
is a plan-level decision, not a retry.

## `openxcvr::observation` — supplied-data observation and planning

`ObservationError`, `BasicPlanError` and `FieldPlanError` share the same shape:
family/revision gates, catalog-lookup failures, and typed input problems. All of
these are deterministic on the same inputs — none is fixed by retrying:

| Variant family | Meaning | Guidance |
| --- | --- | --- |
| `UnsupportedFamily` / `UnsupportedRevision` | The family (or the snapshot's reported revision) has no catalog the planner/decoder can use. | Not a retry. For `UnsupportedRevision`, the snapshot's rev byte is known but undescribed — upgrade the library or use vendor tooling. |
| `RevisionMismatch` / `EntryRevisionMismatch` | The requested revision disagrees with the snapshot's (or an entry was resolved at another revision). | Caller error: plan/decode at the revision the snapshot reports. |
| `MissingRevisionByte` / `PreludeMissing` | The byte that proves the revision (or a prerequisite byte) is absent from the supplied view. | Supply the byte; a retry with the same view fails identically. |
| `ComplianceRefused` | The SFF compliance byte does not admit a decode at the schema revision. | Deterministic on the same bytes. |
| `MonitorRowAbsent` / `IdentifierRowAbsent` | The generated catalog lacks the row for a field the schema names at that revision. | A library/schema defect: report it, do not work around it. |
| `TooManyEntries` / `TooManyRanges` | The request exceeds the planner's documented limits. | Split the request. |
| `Input(InputError)` | Invalid supplied view: bad range/length, missing bytes, stale generation/epoch, overlapping or reversed observations, time overflow, unsupported revision. | Caller error: fix the supplied view. See `openxcvr_core::types::management::InputError`. |

`FieldPlanRefusalReason` (`ClearOnRead`, `AcknowledgeOnRead`, `WriteOnly`) and
`FieldAdmissionReason` are **not** failures to retry: they are the planner
refusing, before any transfer, to read bytes whose read has an effect (latched
flags, startup-interrupt acknowledgment), or that are write-only, or whose
admission prerequisites (gate byte, compliance, selector view, presence) are
not satisfied. Consuming such a field is a deliberate plan-level decision with
its own read-effect handling — the refusal is the library keeping you honest.

## `openxcvr::live` — identity and layout discovery

| Type / variant | Meaning | Guidance |
| --- | --- | --- |
| `IdentifierError::UnsupportedFamily` / `UnsupportedRevision` / `IdentifierRowAbsent` / `Input` | Same meanings as the observation gates above. | Same guidance. |
| `LayoutDiscoveryError::Input` | The observation view was stale, absent or from another generation/epoch. | Fix the supplied view. |
| `LayoutDiscoveryError::Span(SpanError)` | A supplied byte span was too short or out of range for its field (`openxcvr_core::supplied::SpanError`). | Caller error: fix the span. |
| `LayoutDiscoveryError::UnknownIdentifier(u8)` | Byte 0 names a family outside the SFF-8024 routing table. | Deterministic on the same bytes. |
| `LayoutDiscoveryError::UnsupportedRevision(u8)` | The family's revision byte names a revision outside its known domain — detected, never assumed. | Deterministic: same guidance as [`DetectError::UnsupportedRev`](#openxcvr_coreversion_error--revision-gates). |

## Summary table — the retry question

| Question | Answer |
| --- | --- |
| Which transport failures may I resubmit? | `Busy`, `Timeout`, `Nack` — unless the write may have applied (`classify::is_retryable` encodes exactly this). |
| May I replay a failed write? | Only if `write_may_have_applied` is unset; otherwise re-read the affected bytes and recompute what to send. |
| Which failures never change on retry? | Unsupported family/field/revision, unknown identifier, decode failures on stable bytes, planner gates, all caller-input errors. |
| What do I do on an unsupported revision? | Nothing retryable: the module reported a revision the library does not describe. Upgrade the library or use the vendor's tooling — never assume a revision. |
| Where are read-effect fields? | Excluded from plans with typed refusals before any I/O; reading them is a deliberate decision, not a retry. |
