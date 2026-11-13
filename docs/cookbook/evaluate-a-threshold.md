# Evaluate a threshold

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Turn one measurement and one threshold set into alarm/warning
booleans. Bounds are inclusive. A `None` / nil bound does not trip.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;
use openxcvr::Sff8636Module;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let decision = evaluate(
        85.0,
        &Thresholds {
            high_alarm: Some(80.0),
            low_alarm: Some(-5.0),
            high_warning: Some(75.0),
            low_warning: Some(0.0),
        },
    );
    println!(
        "pure high_alarm={} high_warning={} within={}",
        decision.high_alarm, decision.high_warning, decision.is_within_bounds()
    );

    // SFF-8636: caller supplies Page 03h. The library does not select the page.
    // Lower byte 22..24 is case temperature, 1/256 °C. Here 0x0200 = 2.0 °C.
    // Page 03h bytes 0..8 are high alarm, low alarm, high warning, low warning
    // (1.0 °C, -1.0 °C, 0.5 °C, -0.5 °C).
    let mut lower = [0u8; 128];
    lower[1] = 0x07; // compliance byte for SFF-8636 2.5
    lower[22] = 0x02;
    let mut page03 = [0u8; 128];
    page03[..8].copy_from_slice(&[0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80]);
    let module = Sff8636Module::open(BufTransport::from_slice(&lower))?;
    let evaluated = module.evaluate_thresholds(&page03)?;
    println!("temperature high_alarm={}", evaluated.temperature.high_alarm);
    Ok(())
}
```

Expected output:

```text
pure high_alarm=true high_warning=true within=false
temperature high_alarm=true
```

**Notes.** An all-zero threshold set does not trip. There is no CMIS
threshold-evaluation entry point; CMIS Page 02h monitors are ordinary
decoded fields. The facade counterparts are `evaluate_sff8636_thresholds`
and `evaluate_sff8472_thresholds`. Page `03h` is required by the SFF-8636
evaluation and is never zero-filled. A missing or short page is
`ValueError` (Python), a Go `error`, or `RangeError` (TypeScript), all
with the text `sff-8636 page03h: expected >=128 bytes, got 0`.

```python
from openxcvr import evaluate_sff8636_thresholds

lower = bytearray(128)
lower[1] = 0x07
lower[22] = 0x02
page03 = bytearray(128)
page03[0:8] = bytes([0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80])
pages = {"lower": bytes(lower), "00h": bytes(128), "03h": bytes(page03)}
print(evaluate_sff8636_thresholds(pages)["temperature"])
```

```go
sff, err := openxcvr.Sff8636PagesFromCanonical(map[string][]byte{
    "lower": lower, "00h": make([]byte, 128), "03h": page03,
})
evaluated, err := openxcvr.EvaluateSff8636Thresholds(sff)
```

```typescript
const evaluated = evaluateSff8636Thresholds(
  sff8636PagesFromCanonical({ lower, "00h": new Uint8Array(128), "03h": page03 }),
);
evaluated.temperature.high_alarm; // true
```

Expected temperature object:

```text
{"high_alarm": true, "low_alarm": false, "high_warning": true, "low_warning": false}
```
