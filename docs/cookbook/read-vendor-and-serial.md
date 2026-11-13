# Read vendor and serial

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Read the vendor name, part, and serial from page 00h. The
spec revision stays on the module handle; `VendorInfo.rev` is the
vendor's hardware-revision string, not the spec revision.

```rust
use openxcvr::prelude::*;
use openxcvr_cmis::CmisModule;

fn identity_image() -> Vec<u8> {
    // BufTransport: bytes 0..128 are lower memory. Page 00h byte N is at
    // flat offset 128+N. Strings are space-padded to the spec width.
    let mut image = vec![0u8; 256];
    image[0] = 0x18;
    image[1] = 0x52;
    image[128] = 0x18;
    image[129..145].copy_from_slice(b"CMIS_5_2_SYNTH  ");
    image[148..164].copy_from_slice(b"FIXTURE-SYNTH-V1");
    image[164..166].copy_from_slice(b"00");
    image[166..182].copy_from_slice(b"SYNTHETIC0000001");
    image[182..190].copy_from_slice(b"22042700");
    image
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = CmisModule::from_bytes(&identity_image())?;
    let info = module.vendor_info()?;
    println!("vendor: {}", info.name);
    println!("part: {}", info.part_number);
    println!("serial: {}", info.serial_number);
    println!("vendor rev: {}", info.rev);
    println!("date: {}", info.date_code);
    println!(
        "spec revision: CMIS {} (lower byte 1 = 0x{:02X})",
        module.detected_rev().as_str(),
        module.cmis_revision()?
    );
    Ok(())
}
```

Expected output:

```text
vendor: CMIS_5_2_SYNTH
part: FIXTURE-SYNTH-V1
serial: SYNTHETIC0000001
vendor rev: 00
date: 22042700
spec revision: CMIS 5.2 (lower byte 1 = 0x52)
```

**Notes.** `from_bytes` fails closed when lower-memory byte 1 is not a
known CMIS revision. Trailing spaces in the spec fields are trimmed.
`VendorInfo.spec_rev` stays unset on this path; use `detected_rev`.

The facade puts the same strings on `identity` and the spec revision on
`module.cmis_revision`. Page 00h offsets are 1..17, 20..36, 36..38,
38..54, and 54..62 of the 128-byte page (not the flat image).

```python
from openxcvr import decode_cmis

lower = bytearray(128)
lower[0] = 0x18
lower[1] = 0x52
page00 = bytearray(128)
page00[1:17] = b"CMIS_5_2_SYNTH  "
page00[20:36] = b"FIXTURE-SYNTH-V1"
page00[38:54] = b"SYNTHETIC0000001"
doc = decode_cmis({"lower": bytes(lower), "00h": bytes(page00)})
print(doc["identity"]["vendor_name"], doc["identity"]["vendor_pn"], doc["identity"]["vendor_sn"])
```

```go
ident := doc["identity"].(map[string]interface{})
fmt.Println(ident["vendor_name"], ident["vendor_pn"], ident["vendor_sn"])
```

```typescript
console.log(doc.identity.vendor_name, doc.identity.vendor_pn, doc.identity.vendor_sn);
```

Expected identity line (all three languages):

```text
CMIS_5_2_SYNTH FIXTURE-SYNTH-V1 SYNTHETIC0000001
```

Python also prints `vendor_rev` `00`. Go and TypeScript return the same
fields (`vendor_rev` included).
