# Read lane flags

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Read per-lane Rx LOS and Tx fault. This recipe uses SFF-8636
lower memory, which is the family the facade exposes as its own lane-flag
call. CMIS lane flags are `CmisModule::lanes` (Page 11h) and, on the
facade, the `lanes` array inside `decode_cmis` when key `11h` is present.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;
use openxcvr::Sff8636Module;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lower = [0u8; 128];
    lower[0] = 0x11; // QSFP28, SFF-8636
    lower[1] = 0x07; // compliance 2.5, so adaptive-EQ is modeled
    lower[3] = 0x11; // lane 1 Rx LOS (bit 0) and Tx LOS (bit 4)
    lower[4] = 0x01; // lane 1 Tx fault
    let module = Sff8636Module::open(BufTransport::from_slice(&lower))?;
    let lanes = module.lanes()?;
    let lane = &lanes[0];
    println!(
        "index {} rx_los {:?} tx_fault {:?} tx_los {:?} lanes {}",
        lane.index, lane.rx_los, lane.tx_fault, lane.tx_los, lanes.len()
    );
    Ok(())
}
```

Expected output:

```text
index 1 rx_los Some(true) tx_fault Some(true) tx_los Some(true) lanes 4
```

**Notes.** `Sff8636Module::lanes` and `CmisModule::lanes` store a
**1-based** index (`1` is the first lane), and
[`LaneStatus`](../../crates/openxcvr-core/src/types.rs) documents the field
as one-based. Binding JSON `index` is also 1-based — the adapters, the
goldens, the Rust struct, and the facade all use the same lane basis.
[`MockModule`](../../crates/openxcvr-core/src/testkit.rs) stores whatever
index the test writes, so write one-based indexes there too. Flags the
revision does not model are `None` in Rust and omitted in the facade,
not `false`. SFF-8472 has no `tx_los` and no CDR-LOL fields.

```python
from openxcvr import decode_sff8636_lane_signal_flags

lower = bytearray(128)
lower[0] = 0x11
lower[1] = 0x07
lower[3] = 0x11
lower[4] = 0x01
lanes = decode_sff8636_lane_signal_flags({"lower": bytes(lower), "00h": bytes(128)})
print(lanes[0]["index"], lanes[0]["rx_los"], lanes[0]["tx_fault"], lanes[0]["tx_los"])
```

```go
lanes, err := openxcvr.DecodeSff8636LaneSignalFlags(sff)
```

```typescript
const lanes = decodeSff8636LaneSignalFlags(
  sff8636PagesFromCanonical({ lower, "00h": new Uint8Array(128) }),
);
```

Expected first-lane values (all three): `index` 1, `rx_los` true,
`tx_fault` true, `tx_los` true, `tx_adaptive_eq_fault` false.
Lane 2 is `rx_los` false and `tx_fault` false.
