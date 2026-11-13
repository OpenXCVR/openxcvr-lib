# API guide

The Rust calls a consumer actually makes. Every `rust` block below is a
complete program that `cargo test --doc -p openxcvr` compiles; the blocks
that open fixture files are compiled only, and the rest also run. Fixture
paths are relative to the repository root. Types come from
[`prelude`](../../crates/openxcvr/src/prelude.rs) unless the snippet
imports them. Narrative of the model is [concepts](concepts.md). Failures
are [error handling](error-handling.md).

## Prelude

`use openxcvr::prelude::*;` pulls in `UnifiedModule`, `detect`,
`dispatch_identifier`, `Detected`, `SpecKind`, `Module`, `ModuleError`,
`Rev`, `Family`, the revision-report types, `evaluate` / `Thresholds`,
and the identity and lane value types. It re-exports; it adds no behaviour.
`evaluate` here is the alarm/warning decision, not thermal or state-machine
evaluation.

```rust
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(dispatch_identifier(0x03, 0x00)?, SpecKind::Sff8472);
    assert_eq!(dispatch_identifier(0x11, 0x00)?, SpecKind::Sff8636);
    assert_eq!(dispatch_identifier(0x18, 0x50)?, SpecKind::Cmis);
    assert_eq!(dispatch_identifier(0x18, 0x00)?, SpecKind::Sff8636);
    let decision = evaluate(
        25.0,
        &Thresholds {
            high_alarm: Some(80.0),
            low_alarm: Some(-10.0),
            high_warning: Some(70.0),
            low_warning: Some(0.0),
        },
    );
    assert!(decision.is_within_bounds());
    let at_bound = evaluate(
        80.0,
        &Thresholds {
            high_alarm: Some(80.0),
            ..Thresholds::default()
        },
    );
    assert!(at_bound.high_alarm);
    Ok(())
}
```

`dispatch_identifier(byte0, byte1)` routes the SFF-8024 identifier.
Byte 1 matters only for QSFP-DD (`0x18`) and OSFP (`0x19`): `0x30` or
`≥ 0x40` is CMIS, every other byte 1 is the legacy SFF-8636 map.
`0x02`/`0x03` are SFF-8472. `0x0C`/`0x0D`/`0x11`/`0x17` are SFF-8636.
`0x1B` and `0x1E`–`0x25` are CMIS regardless of byte 1. Anything else is
`ModuleError::UnknownIdentifier`. Routing is not revision admission.

## Detect

`detect` reads the identifier and the family's revision byte through any
[`Transport`](../../crates/openxcvr-core/src/transport.rs).
[`BufTransport`](../../crates/openxcvr-core/src/transport.rs) is the
in-tree buffer transport (lower memory at offset 0, 128 bytes per page).
A hardware backend implements the same `read` / `write` / `probe` methods;
this example does not open a device.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = vec![0u8; 128];
    image[0] = 0x18; // QSFP-DD
    image[1] = 0x52; // CMIS 5.2
    let mut transport = BufTransport::from_slice(&image);
    let detected = detect(&mut transport)?;
    assert_eq!(detected.kind, SpecKind::Cmis);
    assert_eq!(detected.rev, Rev::CMIS_5_2);

    image[0] = 0x1B; // DSFP, always CMIS
    image[1] = 0x00; // not a known CMIS revision
    let mut transport = BufTransport::from_slice(&image);
    let err = match detect(&mut transport) {
        Ok(_) => return Err(std::io::Error::other("0x00 must not decode as a revision").into()),
        Err(err) => err,
    };
    assert!(err.is_unsupported_revision());
    Ok(())
}
```

`ModuleError::Detection` displays as `module detection error`. The
`DetectError` (byte and known revisions) is `Error::source`, not the
`Display` text. See [error handling](error-handling.md).

## Memory-map constructors

Paths below are the synthetic fixtures committed in this repo (vendor
string `OPENXCVR`), not hardware captures. Run from the repository root
so the relative paths resolve.

`open_sff8472_mmap(a0h, a2h)` and `open_sff8636_mmap(lower, 00h, 01h, 02h, 03h)`
do not detect a revision. `open_auto_mmap` probes byte 0 of `lower`, then
opens only that family's files. For an SFP image, pass the A0h file as
`lower` as well; the unused page arguments are not opened.
`open_cmis_mmap(lower, 00h, 01h, 02h, 03h, 04h, 05h)` detects the CMIS
revision and requires every one of those files.

```rust,no_run
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a0h = "tests/fixtures/sff-8472/synthetic-sfp28-25g-sr.a0h.bin";
    let a2h = "tests/fixtures/sff-8472/synthetic-sfp28-25g-sr.a2h.bin";
    let agnostic = UnifiedModule::open_sff8472_mmap(a0h, a2h)?;
    assert!(agnostic.detected_rev().is_none());
    assert!(matches!(
        agnostic.revision_report(),
        Err(RevisionReportError::RevisionNotDetected)
    ));
    assert_eq!(agnostic.vendor_info()?.name, "OPENXCVR");

    let module = UnifiedModule::open_auto_mmap(a0h, a0h, a0h, a0h, a0h, a0h, a0h, a0h, a2h)?;
    assert_eq!(module.detected_rev(), Some(Rev::SFF8472_10_4));
    assert_eq!(module.vendor_info()?.name, "OPENXCVR");
    assert_eq!(module.flags()?.lanes.len(), 1);
    Ok(())
}
```

A0h byte 94 in that synthetic image is `0x04`, which
`Rev::from_compliance_byte_sff8472` maps to `Rev::SFF8472_10_4`. The
fixture's prose header is not what detection returns.

```rust,no_run
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stem = "tests/fixtures/sff-8636/synthetic-qsfp28-aoc-100g-sr4";
    let module = UnifiedModule::open_sff8636_mmap(
        format!("{stem}.lower.bin"),
        format!("{stem}.page-00h.bin"),
        format!("{stem}.page-01h.bin"),
        format!("{stem}.page-02h.bin"),
        format!("{stem}.page-03h.bin"),
    )?;
    assert!(module.detected_rev().is_none());
    assert_eq!(module.vendor_info()?.name, "OPENXCVR");
    assert_eq!(module.lanes()?.len(), 4);

    // Lower byte 1 is 0x00 (unspecified). Auto-open must not assume a revision.
    let opened = UnifiedModule::open_auto_mmap(
        format!("{stem}.lower.bin"),
        format!("{stem}.page-00h.bin"),
        format!("{stem}.page-01h.bin"),
        format!("{stem}.page-02h.bin"),
        format!("{stem}.page-03h.bin"),
        format!("{stem}.lower.bin"),
        format!("{stem}.lower.bin"),
        format!("{stem}.lower.bin"),
        format!("{stem}.lower.bin"),
    );
    let Err(err) = opened else {
        return Err(std::io::Error::other("unspecified compliance must fail closed").into());
    };
    assert!(err.is_unsupported_revision());
    Ok(())
}
```

The committed synthetic CMIS 5.2 image has lower memory and pages 00h, 01h,
02h, 10h, and 11h. It does not have 03h–05h, so `open_cmis_mmap` cannot
open it. The [quickstart](quickstart.md) uses `CmisModule::from_bytes` on
an embedded image instead. Pointing `open_cmis_mmap` at the missing page
is an I/O error, not a guessed module:

```rust,no_run
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stem = "tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd";
    let opened = UnifiedModule::open_cmis_mmap(
        format!("{stem}.lower.bin"),
        format!("{stem}.page-00h.bin"),
        format!("{stem}.page-01h.bin"),
        format!("{stem}.page-02h.bin"),
        format!("{stem}.page-03h.bin"),
        format!("{stem}.page-04h.bin"),
        format!("{stem}.page-05h.bin"),
    );
    match opened {
        Err(ModuleError::Io(_)) => Ok(()),
        Err(other) => Err(other.into()),
        Ok(_) => Err(std::io::Error::other("03h-05h are not in this fixture").into()),
    }
}
```

## Custom transport

Build the family adapter over your `Transport`, then box it.
`Sff8472Module::open` detects A0h byte 94. `open_custom` dispatches
`Module` reads and drops the revision context:

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = vec![0u8; 128];
    image[94] = 0x04;
    let adapter = openxcvr::Sff8472Module::open(BufTransport::from_slice(&image))?;
    assert_eq!(adapter.detected_rev(), Some(Rev::SFF8472_10_4));
    let unified = UnifiedModule::open_custom(adapter);
    assert!(matches!(
        unified.revision_report(),
        Err(RevisionReportError::RevisionNotDetected)
    ));
    Ok(())
}
```

Per-spec methods beyond `Module` stay on the concrete adapter. Call them
before `open_custom`, or keep the adapter and do not rely on the unified
handle to recover them.

## Revision and field availability

`revision_report` needs `detected_rev()`. Otherwise call
`field_availability` with the catalog and the `Rev` you already hold.
Windows are schema `since` (inclusive) / `until` (exclusive). A gated name
stays in the report. `None` from `field` means the schema has no such name.

```rust
use openxcvr::catalog::family_catalog;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = match family_catalog(Family::Cmis) {
        Some(catalog) => catalog,
        None => return Err(std::io::Error::other("CMIS catalog missing").into()),
    };
    let at_4_0 = field_availability(catalog, Rev::CMIS_4_0)?;
    assert!(at_4_0.field("mci_max_speed").unwrap().is_gated());
    assert!(at_4_0.field("twi_max_speed_cmis4").unwrap().is_available());
    assert!(at_4_0.field("not_a_schema_field").is_none());

    let at_5_2 = field_availability(catalog, Rev::CMIS_5_2)?;
    assert!(at_5_2.field("temp_mon_value").unwrap().is_available());
    assert!(at_5_2.field("vendor_name").unwrap().is_available());

    match field_availability(catalog, Rev::SFF8472_10_4) {
        Err(RevisionReportError::UnsupportedRevision { .. }) => {}
        Ok(_) => return Err(std::io::Error::other("cross-family rev must be refused").into()),
        Err(other) => return Err(other.into()),
    }
    assert!(family_catalog(Family::CCmis).is_none());
    assert!(family_catalog(Family::Elsfp).is_none());
    Ok(())
}
```

On the detect-once SFF-8472 handle from the mmap section,
`revision_report()?.field("vendor_name")` is available. The report's
`Display` is the revision label plus available and gated counts. It does
not re-read the module.

## Thresholds

`evaluate(measurement, &Thresholds)` is inclusive: a value equal to a
bound trips it. `None` disables that bound. This is not the latched flag
bits in the memory map; those come from `flags()`. Clearing or acting on
an alarm is the caller's decision.

The prelude snippet above is the call. `ThresholdEvaluation::is_within_bounds`
is false when any of `high_alarm`, `low_alarm`, `high_warning`, or
`low_warning` is set.

## Lane flags

`lanes()` returns [`LaneStatus`](../../crates/openxcvr-core/src/types.rs):
optical power, bias, and signal flags (`rx_los`, `tx_fault`, …). Absent
monitors are `None`, not zero. Index is one-based: `1` is the first lane.

`flags()` returns [`ModuleFlags`](../../crates/openxcvr-core/src/types.rs).
`temp_high_alarm` and the other module booleans are the latched
alarm/warning bits. `flags.lanes` is `Vec<LaneFlags>` (Rx/Tx power and
bias alarm/warning), positionally aligned with `lanes()`: entry `i` is the
lane whose `LaneStatus::index` is `i + 1`. A `false` flag
means the bound was not met, not that the monitor exists.

The SFF-8472 mmap snippet reads `flags()?.lanes` (one lane on that
image). The QSFP28 mmap snippet reads `lanes()?.len()` (four). Setting
bits without a module is the test kit below.

## Test kit

[`openxcvr::core::testkit`](../../crates/openxcvr-core/src/testkit.rs) is
an in-memory `Module`, a fixed-length `PageBuilder`, and assertion
helpers. Unset accessors return `ModuleError::Unsupported`. A stored
`None` temperature is a present monitor with no sample, not the same
error. No I/O and no clock.

```rust
use openxcvr::core::testkit::{assert_close, assert_lanes, LaneExpectation, MockModule, PageBuilder};
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = MockModule::new()
        .with_identifier(0x11)
        .with_lane_count(1)
        .with_lanes(vec![LaneStatus {
            index: 1,
            rx_los: Some(true),
            rx_power_mw: Some(0.5),
            ..LaneStatus::default()
        }])
        .with_flags(ModuleFlags {
            lanes: vec![LaneFlags {
                rx_power_high_alarm: true,
                ..LaneFlags::default()
            }],
            ..ModuleFlags::default()
        });
    let unified = UnifiedModule::open_custom(module);
    assert_eq!(unified.identifier()?, 0x11);
    assert!(unified.flags()?.lanes[0].rx_power_high_alarm);
    assert_lanes(
        &unified.lanes()?,
        &[LaneExpectation {
            index: 1,
            rx_los: Some(true),
            rx_power_mw: Some(0.5),
            ..LaneExpectation::default()
        }],
        0.0,
    )?;
    assert_close("rx_power_mw", 0.5, 0.5, 0.0)?;
    let mut page = PageBuilder::new(4);
    page.set_u8(1, 0x0a)?;
    assert_eq!(page.as_bytes()[1], 0x0a);
    assert!(matches!(
        MockModule::new().identifier(),
        Err(ModuleError::Unsupported)
    ));
    Ok(())
}
```

`assert_lanes` compares the set of one-based indexes. A `None` field on
`LaneExpectation` is not compared. Power uses an absolute tolerance; NaN
never matches. `PageBuilder` does not grow: a write past `len` is
`TestKitError::PageWrite`.

Binding JSON lane records use the same one-based `index` as
`LaneStatus::index`.

## Where next

[Error handling](error-handling.md) and the [error catalog](../error-catalog.md).
Runnable programs, from the repository root: `cargo run -p openxcvr --example zero-setup`,
`explore-module`, `quickstart`, `read-vendor-info`, `dispatch-across-specs`,
`error-handling`.
