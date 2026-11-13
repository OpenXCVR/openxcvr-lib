# openxcvr

Universal transceiver EEPROM decoder and library — supports SFF-8472, SFF-8636, and CMIS transceiver families

Part of the openxcvr-lib workspace.

## Quick start

Use `UnifiedModule` when the caller wants one L2 `Module` surface across the supported spec
families. The crate root re-exports `Module`, `UnifiedModule`, `SpecKind`, and
`dispatch_identifier`.

### Zero-setup first run

No fixture checkout path and no hardware: the [`zero-setup`
example](examples/zero-setup.rs) embeds a synthetic sample, so it runs from
any working directory:

```sh
cargo run -p openxcvr --example zero-setup
```

The zero-setup walkthrough is
[`docs/guides/quickstart.md`](../../docs/guides/quickstart.md).

### Fixture-backed alternative

#### Repository root precondition

The fixture paths used below (`tests/fixtures/...`) are relative to the **repository root**
(`openxcvr-lib/`). When running code or tests from subdirectories (such as `crates/openxcvr/`),
the relative fixture paths will fail to resolve unless adjusted or passed as explicit arguments.

#### Running the example

A runnable example is provided in [`examples/quickstart.rs`](examples/quickstart.rs).

From the **repository root**, run with default fixture paths:

```sh
cargo run -p openxcvr --example quickstart
```

From any directory, pass fixture paths explicitly via command-line arguments:

```sh
cargo run -p openxcvr --example quickstart -- <path/to/a0h.bin> <path/to/a2h.bin>
```

#### Quickstart code

```rust,no_run
use openxcvr::{dispatch_identifier, Module, SpecKind, UnifiedModule};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Note: paths are relative to the repository root
    let sff8472 = UnifiedModule::open_sff8472_mmap(
        "tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin",
        "tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin",
    )?;

    let sff8636 = UnifiedModule::open_sff8636_mmap(
        "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin",
        "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin",
        "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin",
        "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin",
        "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin",
    )?;

    for module in [&sff8472, &sff8636] {
        let vendor = module.vendor_info()?;
        println!("vendor: {}", vendor.name);
    }

    assert_eq!(dispatch_identifier(0x03, 0x00)?, SpecKind::Sff8472);
    assert_eq!(dispatch_identifier(0x11, 0x00)?, SpecKind::Sff8636);
    assert_eq!(dispatch_identifier(0x18, 0x50)?, SpecKind::Cmis);
    Ok(())
}
```

`UnifiedModule` also exposes spec-specific enum variants for callers that need to match on
the concrete adapter after dispatch. CMIS is supported through `open_cmis_mmap`; the committed
CMIS captures do not currently include the complete Pages 00h–05h set required by that offline
constructor, so this runnable fixture example intentionally demonstrates the two complete SFF
fixture sets instead.

## Supplied-input runtime support

Callers that own acquisition themselves — a daemon, a cache owner, a rig
that batches reads — use the supplied-input surface: pure functions that
plan byte ranges, and decodes that take supplied bytes.
`observation::observe_basic` with `basic_read_prelude` /
`basic_read_remaining` serves the basic monitor set; `catalog::resolve`
with `field_read_plan` / `field_read_prelude` / `field_read_remaining`
and `observe_fields` serves arbitrary catalog-resolved fields;
`live::cmis_identity_layout` / `decode_cmis_identity_supplied` /
`decode_identifier_supplied` serves identity, CMIS 3.0–5.3 (evidence:
[CMIS 4 runtime support](docs/cmis4-runtime-support.md); supplied-byte
decodes qualify no hardware).

Each integration path is a journey guide, with that path's limits
stated on the page:
[offline analysis](../../docs/guides/offline-analysis.md),
[live monitoring](../../docs/guides/live-monitoring.md) or
[managed operations](../../docs/guides/managed-operations.md).

## CMIS 5.3 capability meanings

`capability_meanings::cmis53_capability_meaning` is a pure optional projection
for exactly `banks_supported` and `tx_input_clocking_capabilities`. Supply the
catalog-resolved `FieldEntry`, its `FieldObservation` and the extracted raw
code; only exact CMIS 5.3, Valid, agreeing two-bit raw observations receive a
schema-generated name, revision and library applicability note. The returned
view borrows the unchanged source and retains the entry/range. It neither
acquires bytes nor authorizes selectors. Existing numeric page fields and
CMIS 4.0 ModuleState behavior are unchanged. The projection is a name for
those two advertisements at exactly CMIS 5.3; it does not acquire bytes and
it does not authorize a selector.

## License

MIT; see [LICENSE](../../LICENSE).
