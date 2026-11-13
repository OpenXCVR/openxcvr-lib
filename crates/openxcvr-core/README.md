# openxcvr-core

The shared foundation of the library: the `Module` and `Transport` traits, the `Rev` revision type, the error surface, the schema-derived field catalog and revision report, threshold evaluation, the generic state-machine runtime and the test kit. It performs no device I/O unless the optional `linux-i2c` feature is enabled.

Most consumers depend on the [`openxcvr`](../openxcvr/README.md) crate, which re-exports this one; depend on this crate directly only when you need it alone.

Part of the openxcvr-lib workspace.

## Quick start

Implement the `Module` trait for an adapter, then return the shared public types from
`openxcvr_core::types`.

```rust
use openxcvr_core::{
    error::ModuleError,
    module::Module,
    types::{ChecksumStatus, LaneStatus, ModuleFlags, VendorInfo},
};

pub struct ExampleModule;

impl Module for ExampleModule {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        Ok(VendorInfo::default())
    }

    fn identifier(&self) -> Result<u8, ModuleError> {
        Ok(0x03)
    }

    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        Ok(Some(31.5))
    }

    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        Ok(Some(3.3))
    }

    fn lane_count(&self) -> Result<u8, ModuleError> {
        Ok(1)
    }

    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        Ok(vec![LaneStatus {
            index: 0,
            rx_power_mw: Some(0.42),
            ..LaneStatus::default()
        }])
    }

    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        Ok(ModuleFlags::default())
    }

    fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
        Ok(ChecksumStatus::default())
    }
}
```

`ModuleFlags` is the shared module-level flag struct, and each `LaneStatus` carries
per-lane optical telemetry and fault state.

## See also

- [`openxcvr` crate](../openxcvr/README.md): the facade most consumers use
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)

## License

MIT; see [LICENSE](../../LICENSE).
