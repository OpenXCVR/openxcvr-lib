# Test without hardware

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Exercise code that takes a [`Module`](../../crates/openxcvr-core/src/module.rs)
without a bus. [`MockModule`](../../crates/openxcvr-core/src/testkit.rs)
returns only stored values. An accessor left unset is
`ModuleError::Unsupported`, not a zero reading.

```rust
use openxcvr::core::testkit::{
    assert_close, assert_lanes, LaneExpectation, MockModule, PageBuilder,
};
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = MockModule::new()
        .with_vendor_info(VendorInfo {
            name: "SYNTH".to_string(),
            serial_number: "SN1".to_string(),
            ..VendorInfo::default()
        })
        .with_lanes(vec![LaneStatus {
            index: 1, // one-based, like a real module; see the lane recipe
            rx_los: Some(true),
            tx_fault: Some(false),
            ..LaneStatus::default()
        }]);
    let info = module.vendor_info()?;
    println!("mock vendor {} serial {}", info.name, info.serial_number);
    assert_lanes(
        &module.lanes()?,
        &[LaneExpectation {
            index: 1,
            rx_los: Some(true),
            tx_fault: Some(false),
            ..LaneExpectation::default()
        }],
        0.0,
    )?;
    assert_close("temperature_c", 25.0, 25.05, 0.1)?;
    println!("asserts ok");
    println!("unset {}", MockModule::new().lanes().expect_err("unset"));

    let mut page = PageBuilder::new(4);
    page.set_u8(0, 0x18)?;
    page.set_be_u16(1, 0x5200)?;
    println!("page {:02x?}", page.as_bytes());
    Ok(())
}
```

Expected output:

```text
mock vendor SYNTH serial SN1
asserts ok
unset not exposed by this module family/revision; see detected_rev()
page [18, 52, 00, 00]
```

**Notes.** The page line is hex (`{:02x?}`), so the first byte is
`0x18`. `assert_lanes` compares only the fields you set on
`LaneExpectation`; a negative tolerance is an error. `PageBuilder` does
not grow: a write past the allocated length is `TestKitError::PageWrite`.
`assert_close` rejects NaN and infinite values. The bindings do not wrap
this test kit; build a page map and call the facade instead.
