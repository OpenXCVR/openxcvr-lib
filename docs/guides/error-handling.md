# Error handling

How to branch on failures from the unified surface, and what to do about
the common ones. The per-variant tables (meaning and retry guidance for
`ModuleError`, `DetectError`, transport `ErrorKind`, observation, and
`RevisionReportError`) stay in the [error catalog](../error-catalog.md).
This page is the control flow. Examples assume the [API guide](api-guide.md)
setup: the `openxcvr` crate, plus `openxcvr-transports` when a snippet
constructs a transport `Error` directly.

```toml
[dependencies]
openxcvr = { path = "path/to/openxcvr-lib/crates/openxcvr" }
openxcvr-transports = { path = "path/to/openxcvr-lib/crates/openxcvr-transports" }
```

Production code that only classifies a `ModuleError` it already holds
needs `openxcvr` alone: `openxcvr::classify` is re-exported.

## Match the variant

`ModuleError` is `#[non_exhaustive]`. Match the variants you handle and
keep a wildcard. The `Display` text is for a log. `is_unsupported_revision`
and `is_not_exposed` are the predicates; do not search the message.

| Variant | What happened | What to do |
| --- | --- | --- |
| `UnknownIdentifier(byte)` | Byte 0 is outside the routing table. | Re-read the base/lower identifier. Do not pick a family for it. |
| `Detection(DetectError::UnsupportedRev)` | The revision byte is not one this library maps. `is_unsupported_revision()` is true. | Upgrade the library or use the vendor's tooling. Not a retry. Do not pass a different `Rev` to `field_availability` to paper over it. |
| `Detection(DetectError::Transport)` | The probe read failed. | `is_unsupported_revision()` is false. Classify the source. A probe is a read; resubmit only if the transport error says that is safe. |
| `Unsupported` / `UnsupportedField` | This family or revision has no such field, or a test double left the accessor unset. `is_not_exposed()` is true. `is_unsupported_revision()` stays false. | `revision_report` or `field_availability`. A gated field is absent at this revision. On `MockModule`, store the value or treat unset as unsupported. |
| `Decode` | Bytes were read and did not decode. `site()` is set when the decoder knew the page and field. | Same bytes fail the same way. Re-read only if the capture itself may be truncated. Stable EEPROM will not change on retry. |
| `Io` | The transport or a filesystem open failed. The typed cause is the source chain. | Walk `Error::source`. Classify with `openxcvr::classify` when the cause is a transport `Error`. |

`site()` is `Some` for `UnsupportedField` and for a `DecodeError` built
with `DecodeError::at`. If it is `None`, the construction site did not
know the page. Do not parse `Display` to recover one.

## Source chains

`IoError::with_source` and `DecodeError::with_source` keep the original
error. `ModuleError::Detection` does the same for `DetectError`.

```rust
use std::error::Error;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let io = std::io::Error::other("bus stuck");
    let error = ModuleError::Io(IoError::with_source(format!("read failed: {io}"), io));
    let cause = error.source().and_then(Error::source);
    match cause {
        Some(cause) => assert_eq!(cause.to_string(), "bus stuck"),
        None => return Err(std::io::Error::other("source chain dropped the cause").into()),
    }
    // A std::io error is not a transport Error. The classifier must not guess.
    assert!(!openxcvr::classify::is_retryable(&error));
    assert!(!openxcvr::classify::is_timeout(&error));
    assert!(!error.is_unsupported_revision());
    Ok(())
}
```

## Unsupported revision is not "field missing"

`is_unsupported_revision()` is true only when detection wrapped
`DetectError::UnsupportedRev`. A field the schema gates out at a known
revision is `Unsupported` / `UnsupportedField`, and the predicate stays
false. The DSFP byte below is the revision case (`0x1B` routes to CMIS;
`0x00` is not a known CMIS revision byte):

```rust
use std::error::Error;
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = vec![0u8; 128];
    image[0] = 0x1B;
    image[1] = 0x00;
    let mut transport = BufTransport::from_slice(&image);
    let err = match detect(&mut transport) {
        Err(err) => err,
        Ok(_) => return Err(std::io::Error::other("unspecified CMIS rev was accepted").into()),
    };
    assert!(err.is_unsupported_revision());
    assert!(!err.is_not_exposed());
    let detail = match err.source() {
        Some(detail) => detail.to_string(),
        None => return Err(std::io::Error::other("DetectError missing from source").into()),
    };
    assert!(detail.contains("0x00"));
    Ok(())
}
```

The source string names the detected byte and points at `known_revs`.
Retrying `detect` on the same bytes cannot succeed.

`RevisionNotDetected` is the other closed failure: the handle was opened
without detection (`open_sff8472_mmap`, `open_sff8636_mmap`, or
`open_custom`). Remedy: `open_auto_mmap`, the family's `open` / CMIS
`new`, or `field_availability` with the `Rev` you saved from `detect`.
Do not fill the gap with `Rev::CMIS_5_4` because it is the newest
constant in the crate.

## Retryability

`openxcvr::classify::is_retryable` walks the source chain for
`openxcvr_transports::Error`. It is true only for `Busy`, `Timeout`, and
`Nack`, and only when `write_may_have_applied` is unset. `is_timeout` is
the deadline case. Both are false when the chain has no transport error.

Retry policy (delay, budget) stays with the caller. The library never
retries internally. A write that may have reached the device is not
replayed: re-read the affected bytes, then decide what to send.
Clear-on-read bytes are a plan-level refusal, not a retry.

Constructing the transport error is a test double. Live code classifies
the `ModuleError` a read or write already returned.

```rust
use openxcvr::prelude::*;
use openxcvr_transports::{Error, ErrorKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let timeout = Error::new(ErrorKind::Timeout, "deadline");
    let retryable = ModuleError::Io(IoError::with_source(timeout.to_string(), timeout));
    assert!(openxcvr::classify::is_timeout(&retryable));
    assert!(openxcvr::classify::is_retryable(&retryable));

    let uncertain = Error::new(ErrorKind::Timeout, "deadline").uncertain();
    let not_replayed = ModuleError::Io(IoError::with_source(uncertain.to_string(), uncertain));
    assert!(openxcvr::classify::is_timeout(&not_replayed));
    assert!(!openxcvr::classify::is_retryable(&not_replayed));
    Ok(())
}
```

`Error::uncertain` sets `write_may_have_applied`. The deadline still
expired; resubmitting that write is not safe.

## Common failures

**Missing mmap file.** `open_cmis_mmap` on
`tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd` returns `ModuleError::Io`
because pages 03h–05h are not in that set (`No such file or directory`).
Remedy: pass the files that constructor reads, or use the quickstart's
`CmisModule::from_bytes` path for the embedded sample. Do not delete the
error by skipping detection.

**Unspecified SFF-8636 compliance.** `open_auto_mmap` on
`tests/fixtures/sff-8636/synthetic-qsfp28-aoc-100g-sr4` returns
`Detection` and `is_unsupported_revision()` is true (lower byte 1 is
`0x00`). `open_sff8636_mmap` still opens that image, with
`detected_rev() == None`. Remedy: use the rev-agnostic constructor when
you accept union-view decode, or obtain a capture whose compliance byte
maps. Do not pass `Rev::SFF8636_2_12` in its place.

**Wrong-family catalog.** `field_availability` with a CMIS catalog and
`Rev::SFF8472_10_4` is `RevisionReportError::UnsupportedRevision`.
`family_catalog(Family::CCmis)` and `Family::Elsfp` are `None`
(`RevisionReportError::NoCatalog` if a handle's family has no catalog).
Remedy: stop. Availability is not guessed.

**Decode site.** `ModuleError::decode_at(FieldSite::new(page, address, offset, field), detail)`
puts the site in `Display` and in `site()`. Remedy: inspect those bytes.
The catalog's `Decode` row is the retry rule.

## Where next

[Error catalog](../error-catalog.md) for every operational variant.
[API guide](api-guide.md) for the calls that produce these errors.
Runnable tour: `cargo run -p openxcvr --example error-handling`.
