# Decode VDM

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** List advertised VDM instances, decode one sample by observable
type, and read the threshold quad for that instance. Group count comes
from Page 2Fh byte 128 bits 1:0 (`0` means group 1 only). An inactive
descriptor slot is type `0` and is skipped.

Rust samples go through
[`openxcvr_cmis::ops::vdm`](../../crates/openxcvr-cmis/src/ops/vdm.rs).
Threshold pages are not part of that module; decode them with the
revision-specialised generated reader for the revision `detect` returned.
This image is 5.2, so the reader is `generated::v5_2`. `generated::union`
is the rev-agnostic union and is a different, explicit choice.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr_cmis::generated::v5_2::{
    CmisPage20hVdmDescriptors1To64, CmisPage28hVdmThresholds1To16,
};
use openxcvr_cmis::ops::vdm::{list_observables, read_observable, VdmSampleValue};

fn place(image: &mut [u8], page: u8, bytes: &[u8]) {
    let start = 128 + page as usize * 128;
    image[start..start + bytes.len()].copy_from_slice(bytes);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = vec![0u8; 128 * (0x2F + 2)];
    image[0] = 0x18;
    image[1] = 0x52;
    let mut page20 = [0u8; 128];
    page20[0] = 0x10; // local threshold-set id 1 in the high nibble
    page20[1] = 0x01; // laser age
    let mut page24 = [0u8; 128];
    page24[1] = 0x2A; // U16 sample 42
    let mut page28 = [0u8; 128];
    page28[0] = 0x12;
    page28[1] = 0x34; // high alarm of threshold set 1
    let page2f = [0u8; 128]; // vdm_support code 0
    place(&mut image, 0x20, &page20);
    place(&mut image, 0x24, &page24);
    place(&mut image, 0x28, &page28);
    place(&mut image, 0x2F, &page2f);

    let mut transport = BufTransport::from_slice(&image);
    let ids = list_observables(&mut transport)?;
    let sample = read_observable(&mut transport, ids[0], 1)?;
    assert!(matches!(sample.value, VdmSampleValue::U16(42)));
    println!(
        "ids {ids:?} type {} raw 0x{:04X} value {:?}",
        sample.observable_type, sample.raw_value, sample.value
    );

    let descriptors = CmisPage20hVdmDescriptors1To64::decode(&page20);
    let thresholds = CmisPage28hVdmThresholds1To16::decode(&page28);
    println!(
        "descriptor {:?} set {} high_alarm 0x{:04X}",
        descriptors.observable_type[0],
        descriptors.local_threshold_set_id[0],
        thresholds.high_alarm_threshold[0]
    );
    Ok(())
}
```

Expected output:

```text
ids [0] type 1 raw 0x002A value U16(42)
descriptor LaserAgeMediaLane set 1 high_alarm 0x1234
```

**Notes.** Rust observable ids are the 0-based slot index (`0` is the
first instance of group 1). `read_observable`'s `lane` argument is stored
on the result; the descriptor's monitored-resource nibble is separate
(`0` here). A group the advertisement does not list returns
`VdmError::GroupNotAdvertised` and is not read.

The facade emits a `vdm` block only when the caller supplies the whole
set `20h`–`27h`, `28h`–`2bh`, `2ch`, `2dh`, and `2fh` (`2eh` is not a
key). A missing key is an absent page, not a zero page. Present zero
pages are decoded. Facade `instance` is 1-based, so Rust id `0` is JSON
instance `1`. `global_threshold_set_id` for local set `1` in group 1 is
`2` (`group * 16 + local_set + 1` with a 0-based group).

```python
from openxcvr import decode_cmis

pages = {key: bytes(128) for key in (
    "lower", "00h", "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
    "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh")}
pages["lower"] = bytearray(pages["lower"]); pages["lower"][0] = 0x18; pages["lower"][1] = 0x52
pages["lower"] = bytes(pages["lower"])
page20 = bytearray(128); page20[0] = 0x10; page20[1] = 0x01
page24 = bytearray(128); page24[1] = 0x2A
page28 = bytearray(128); page28[0] = 0x12; page28[1] = 0x34
pages["20h"], pages["24h"], pages["28h"] = bytes(page20), bytes(page24), bytes(page28)
block = decode_cmis(pages)["vdm"]
print(block["support"]["vdm_support"], block["support"]["groups_advertised"],
      block["support"]["vdm_support_name"])
print(block["observables"][0]["instance"], block["observables"][0]["sample"])
print(block["threshold_quads"][0])
```

```go
cmis, err := openxcvr.CmisPagesFromCanonical(vdm) // same canonical keys as Python
doc, err := openxcvr.DecodeCmis(cmis)
block := doc["vdm"].(map[string]interface{})
```

```typescript
const doc = decodeCmis(cmisPagesFromCanonical(pages)); // same keys; Uint8Array values
doc.vdm.support.vdm_support_name; // "group_1"
doc.vdm.observables[0].sample;    // { storage: "u16", value: 42 }
```

Actual output of the Python `print` statements above:

```text
0 1 group_1
1 {'storage': 'u16', 'value': 42}
{'page': '28h', 'set': 1, 'high_alarm': '0x1234', 'low_alarm': '0x0000', 'high_warning': '0x0000', 'low_warning': '0x0000'}
```

The Go and TypeScript snippets access the same fields without printing:
`observables[0].instance` is `1` and `observables[0].sample` is
`{ storage: "u16", value: 42 }` in TypeScript (Go yields the equivalent
`map[string]interface{}`).
