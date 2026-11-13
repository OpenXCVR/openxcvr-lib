# Identify family and revision

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Route the SFF-8024 identifier and read the family's revision
byte, without assuming a later revision.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // QSFP-DD (byte 0 = 0x18) whose byte 1 is the CMIS 5.2 major byte.
    let kind = dispatch_identifier(0x18, 0x52)?;
    let rev = Rev::from_major_byte_cmis(0x52).expect("CMIS 5.2 is a known revision");
    println!("dispatch: {kind:?} family {:?} rev {}", rev.family, rev.as_str());

    let mut image = vec![0u8; 128];
    image[0] = 0x18;
    image[1] = 0x52;
    let detected = detect(&mut BufTransport::from_slice(&image))?;
    println!(
        "detect: {:?} family {:?} rev {}",
        detected.kind, detected.rev.family, detected.rev.as_str()
    );
    Ok(())
}
```

Expected output:

```text
dispatch: Cmis family Cmis rev 5.2
detect: Cmis family Cmis rev 5.2
```

**Notes.** `dispatch_identifier` only routes the family.
[`detect`](../../crates/openxcvr/src/unified.rs) then maps the family's
revision byte and returns [`ModuleError::Detection`] for a byte outside
the known domain. Byte `0x55` (which would be CMIS 5.5) is refused;
`err.is_unsupported_revision()` is true. See the error recipe.

Python, Go, and TypeScript do not have `detect`. `decode_cmis` reports
the major/minor of lower-memory byte 1 when the major is 3, 4, or 5, and
returns `decode_status: "rejected"` for any other major. It does not
throw, and it does not apply Rust's full revision-domain gate: byte
`0x55` is major 5, so the facade returns `ok` and `cmis_revision` `5.5`.
Match on `reason`, not on the diagnostic `detail` string.

```python
from openxcvr import decode_cmis

lower = bytearray(128)
lower[0] = 0x18
lower[1] = 0x52
doc = decode_cmis({"lower": bytes(lower), "00h": bytes(128)})
print(doc["decode_status"], doc["module"]["cmis_revision"], doc["module"]["identifier"])

bad = bytearray(128)
bad[0] = 0x18
bad[1] = 0x20  # major 2, not a CMIS major this facade accepts
rejected = decode_cmis({"lower": bytes(bad), "00h": bytes(128)})
print(rejected["decode_status"], rejected["rejection"]["reason"])
```

```go
pages, err := openxcvr.CmisPagesFromCanonical(map[string][]byte{
    "lower": lower, // lower[0]=0x18, lower[1]=0x52, len 128
    "00h":   make([]byte, 128),
})
doc, err := openxcvr.DecodeCmis(pages)
// doc["decode_status"] == "ok"
// doc["module"].(map[string]interface{})["cmis_revision"] == "5.2"
```

```typescript
const doc = decodeCmis(cmisPagesFromCanonical({ lower, "00h": new Uint8Array(128) }));
// doc.decode_status === "ok" && doc.module.cmis_revision === "5.2"
```

Expected facade output for the Python block (Go and TypeScript match):

```text
ok 5.2 {'name': 'qsfp_dd', 'raw': '0x18'}
rejected unknown_major_revision: 0x20
```

The Go and TypeScript imports are
`github.com/OpenXCVR/openxcvr-lib/bindings/go` and
`@openxcvr/transceivers` (`cmisPagesFromCanonical`, `decodeCmis` from
`bindings/typescript`). Build the TypeScript package before importing it.
