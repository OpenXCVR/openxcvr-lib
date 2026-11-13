# Handle a decode or unsupported error

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Branch on the error variant. Messages are for logs.

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Page 11h byte 147 is los_flag_rx. Page 00h byte 222 is page_checksum.
    let unsupported = ModuleError::unsupported_field(FieldSite::new("11h", 0x50, 147, "los_flag_rx"));
    println!("{unsupported}");
    println!(
        "not_exposed {} unsupported_rev {}",
        unsupported.is_not_exposed(),
        unsupported.is_unsupported_revision()
    );

    let decode = ModuleError::decode_at(
        FieldSite::new("00h", 0x50, 222, "page_checksum"),
        "stored checksum does not match",
    );
    println!("{decode}");
    println!("site {}", decode.site().expect("located decode").field);

    let unknown = dispatch_identifier(0x00, 0x00).expect_err("identifier 0");
    println!("{unknown}");

    let mut bad = [0u8; 128];
    bad[0] = 0x18;
    bad[1] = 0x55;
    let err = detect(&mut BufTransport::from_slice(&bad)).expect_err("CMIS 5.5");
    println!("predicate {}", err.is_unsupported_revision());
    println!("source {}", std::error::Error::source(&err).expect("detect source"));
    Ok(())
}
```

Expected output:

```text
page 11h address 0x50 offset 147 field los_flag_rx: not exposed by this module family/revision; see detected_rev()
not_exposed true unsupported_rev false
decode error: page 00h address 0x50 offset 222 field page_checksum: stored checksum does not match
site page_checksum
unknown module identifier: 0x00
predicate true
source unsupported or unknown rev byte: 0x55; not mapped by this library — see known_revs and do not assume a later revision
```

**Notes.** `is_not_exposed` is "this family or revision has no such
field". `is_unsupported_revision` is "the module's revision byte is not
one this library maps". They are different failures. Walk
`Error::source` for the detect cause; do not parse `Display`.
The next step for a gated name is the revision report above, not a retry.
The catalog of which failures may be resubmitted is the
[error catalog](../error-catalog.md).

Facade failures are language errors, not a `decode_status`. An unknown
canonical key is Python `ValueError: cmis pages: unknown key 'nope'`,
Go `error` `cmis pages: unknown key "nope"`, and TypeScript
`TypeError: cmis pages: unknown key "nope"`. A missing SFF-8636 `00h`
is Python `KeyError: '00h'`. The short Page `03h` case is in the
threshold recipe.
