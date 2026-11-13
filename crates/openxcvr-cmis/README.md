# openxcvr-cmis

The CMIS family adapter: generated page decoders plus `CmisModule`, CDB commands, VDM, and the generated module and data-path state machines. It depends only on `openxcvr-core`.

Most consumers depend on the [`openxcvr`](../openxcvr/README.md) crate, which re-exports this one; depend on this crate directly only when you need it alone.

Part of the `openxcvr-lib` workspace. See the [library README](../../README.md) for context on the four overlapping standards (CMIS, SFF-8636, SFF-8472, SFF-8024).

## Coverage (v0.1)

| Page | Description | Coverage |
|------|-------------|----------|
| Lower memory | Module identity, state, monitors | full |
| 00h | Administrative (vendor info) | full |
| 01h | Advertising (capabilities) | curated subset |
| 02h | Thresholds | full |
| 03h | User EEPROM | raw bytes |
| 04h | Laser capabilities | full |
| 05h | Form-factor signals | raw bytes |

Banked pages (10h–16h, 18h–19h, 1Ch–1Dh, 20h–2Fh) and CDB commands (page 9Fh + A0h–AFh) are deferred to v0.2+.

## Pure daemon planning

`read_planning::CmisReadPlanner::for_revision` binds the delivered CMIS 4.0 or
5.3 passive footprint. `vendor_programming::plan_vendor_programming` provides a
narrow CMIS 4.0 password → identity → checksum → verify plan, not general RO-field
write permission. A refused step is a typed error; the caller decides whether to
retry. Page 00h bytes 128–222 and bootstrap byte 2 are the qualified 4.0 read;
bytes 223–255 are not admitted, and 5.3 read authority is unchanged.

The bounded pure `readiness::Cmis4Readiness` helper adds only CMIS 4.0 byte-3
readiness, with mandatory caller policy. It is not full Readiness/Power and it
does not wait for the module.

## Quick start

> **API preview:** `CmisModule` and `CmisMmapTransport` are planned types (T27); this example shows the intended interface.

```rust
use openxcvr_cmis::{CmisModule, CmisMmapTransport};
use openxcvr_core::Module;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = CmisMmapTransport::from_files(
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.lower.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-00h.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-01h.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-02h.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-03h.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-04h.bin",
        "tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr.page-05h.bin",
    )?;
    let module = CmisModule::new(transport)?; // detects + freezes the CMIS rev (Lower:1)
    println!("vendor: {}", module.vendor_name()?);
    Ok(())
}
```

Replace `fboss-cmis-5-3-osfp-800g-zr` with one of the CMIS fixtures — run `ls tests/fixtures/cmis/` to list them.

## See also

- [`openxcvr` crate](../openxcvr/README.md): the facade most consumers use
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)

## License

MIT; see [LICENSE](../../LICENSE).
