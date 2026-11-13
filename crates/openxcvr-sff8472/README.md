# openxcvr-sff8472

The SFF-8472 (SFP/SFP+/SFP28) family adapter: generated decoders plus `Sff8472Module`. It depends only on `openxcvr-core`.

Most consumers depend on the [`openxcvr`](../openxcvr/README.md) crate, which re-exports this one; depend on this crate directly only when you need it alone.

Part of the openxcvr-lib workspace.

## Quick start

`Sff8472MmapTransport` loads the SFP A0h and A2h address spaces from files.
`Sff8472Module::new` wraps that transport and implements the shared
`openxcvr_core::module::Module` trait.

```rust
use openxcvr_core::module::Module;
use openxcvr_sff8472::{
    adapter::Sff8472Module,
    mmap::Sff8472MmapTransport,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = Sff8472MmapTransport::from_files(
        "tests/fixtures/sff-8472/example.a0h.bin",
        "tests/fixtures/sff-8472/example.a2h.bin",
    )?;

    let module = Sff8472Module::new(transport);
    let vendor = module.vendor_info()?;
    let flags = module.flags()?;

    println!("vendor: {}", vendor.name);
    println!("temperature high alarm: {}", flags.temp_high_alarm);
    Ok(())
}
```

This adapter reports one lane and exposes identity, monitoring, lane, flag, and checksum
data through the `Module` trait.

## See also

- [`openxcvr` crate](../openxcvr/README.md): the facade most consumers use
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)

## License

MIT; see [LICENSE](../../LICENSE).
