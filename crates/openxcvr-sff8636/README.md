# openxcvr-sff8636

The SFF-8636 (QSFP+/QSFP28) family adapter: generated page decoders plus `Sff8636Module`. It depends only on `openxcvr-core`.

Most consumers depend on the [`openxcvr`](../openxcvr/README.md) crate, which re-exports this one; depend on this crate directly only when you need it alone.

Part of the openxcvr-lib workspace.

## Quick start

`Sff8636MmapTransport` loads a lower-memory image plus Pages 00h through 03h from
128-byte files. `Sff8636Module::new` wraps that transport and implements the shared
`openxcvr_core::module::Module` trait.

```rust
use openxcvr_core::module::Module;
use openxcvr_sff8636::{
    adapter::Sff8636Module,
    mmap::Sff8636MmapTransport,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = Sff8636MmapTransport::from_files(
        "tests/fixtures/sff-8636/example.lower.bin",
        "tests/fixtures/sff-8636/example.page-00h.bin",
        "tests/fixtures/sff-8636/example.page-01h.bin",
        "tests/fixtures/sff-8636/example.page-02h.bin",
        "tests/fixtures/sff-8636/example.page-03h.bin",
    )?;

    let module = Sff8636Module::new(transport);
    let vendor = module.vendor_info()?;
    let lanes = module.lanes()?;

    println!("vendor: {}", vendor.name);
    println!("lanes: {}", lanes.len());
    Ok(())
}
```

This adapter reports four lanes and exposes identity, monitoring, lane, flag, and checksum
data through the `Module` trait.

## See also

- [`openxcvr` crate](../openxcvr/README.md): the facade most consumers use
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)

## License

MIT; see [LICENSE](../../LICENSE).
