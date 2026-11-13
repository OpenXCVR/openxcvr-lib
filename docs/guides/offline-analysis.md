# Offline analysis

Decode captured transceiver memory — capture files or bytes you already
hold — with no hardware and no bus, starting from the
[documentation map](../README.md).

## When this is your situation

- You have per-page `.bin` captures (from a field capture tool, a fixture
  corpus, or a colleague) and want identity, telemetry and field values
  out of them.
- You have a dense dump in memory — sysfs, a debugger, a vendor tool's
  export — and want the same decode.
- You are triaging a capture: which family is it, which revision does it
  report, which fields exist at that revision.

## Shape of the solution

1. Get canonical per-page files. The converters under `tools/` turn
   third-party formats into the naming this library reads: `oom-to-bin.py`
   (OCP OOM text dumps), `fboss-to-bin.py` (fboss fake-transceiver
   arrays), `optoe-to-bin.py` (reads a live OPTOE-driven Linux host).
   A dense image — any file whose length is a multiple of 128 — needs no
   conversion. `distill-capture.py` scrubs a private capture of a real
   module into a commit-ready synthetic fixture.
2. Open the capture. `UnifiedModule::open_auto_mmap` probes bytes 0 and 1
   of the lower capture, routes the family, and opens only that family's
   files; it takes nine positional paths so one call serves every family.
   The per-family constructors are `open_sff8472_mmap(a0h, a2h)`,
   `open_sff8636_mmap(lower, 00h, 01h, 02h, 03h)` and
   `open_cmis_mmap(lower, 00h, 01h, 02h, 03h, 04h, 05h)`.
   For bytes in memory, `CmisModule::from_bytes` takes a dense image.
3. Read through the `Module` trait (`vendor_info`, `lanes`, `flags`,
   `temperature_celsius`, `checksums`). Every read decodes only the bytes
   the capture supplies; a missing page is an error or a `NotSupplied`
   outcome, never zero-filled.
4. Ask the revision report when a field surprises you:
   `revision_report()` lists every catalog field as available or gated at
   the revision detection proved.

Bytes already in memory need no files at all. Lower memory is bytes
0–127 of the image; upper pages follow in selector order:

```rust
use openxcvr::prelude::*;
use openxcvr::CmisModule;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Dense dump: lower memory, then upper pages in selector order.
    // 256 bytes = lower memory plus Page 00h.
    let mut dump = vec![0u8; 256];
    dump[0] = 0x18; // SFF-8024 identifier: QSFP-DD
    dump[1] = 0x52; // lower byte 1: CMIS 5.2
    dump[14..16].copy_from_slice(&0x1000u16.to_be_bytes()); // 16.0 C, 1/256 C lsb
    dump[129..145].copy_from_slice(b"ACME PHOTONICS  "); // Page 00h VendorName
    dump[148..164].copy_from_slice(b"QSFP-DD-400G-SR4"); // Page 00h VendorPN
    dump[164..166].copy_from_slice(b"01"); // VendorRev
    dump[166..182].copy_from_slice(b"SERIAL0000000001"); // VendorSN

    let module = CmisModule::from_bytes(&dump)?;
    assert_eq!(module.detected_rev(), Rev::CMIS_5_2);
    let vendor = module.vendor_info()?;
    println!("vendor {}", vendor.name);
    println!("part {}", vendor.part_number);
    println!("serial {}", vendor.serial_number);
    println!("temp {:?}", module.temperature_celsius());
    Ok(())
}
```

Per-page files open through the mmap constructors. Paths are relative to
the repository root; the blocks below are compiled but not executed,
because doctests run from `crates/openxcvr`:

```rust,no_run
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a0h = "tests/fixtures/sff-8472/synthetic-sfp28-25g-sr.a0h.bin";
    let a2h = "tests/fixtures/sff-8472/synthetic-sfp28-25g-sr.a2h.bin";
    // Auto-open: byte 0 of `lower` routes the family; the unused paths
    // are never opened. For an SFP image, pass the A0h file as `lower`.
    let module = UnifiedModule::open_auto_mmap(
        a0h, a0h, a0h, a0h, a0h, a0h, a0h, a0h, a2h,
    )?;
    assert_eq!(module.detected_rev(), Some(Rev::SFF8472_10_4));
    println!("vendor {}", module.vendor_info()?.name);
    // The revision report lists available and gated fields at that rev.
    let report = module.revision_report()?;
    println!("{report}");
    assert!(report.field("vendor_name").unwrap().is_available());
    Ok(())
}
```

Which fields a revision has is a pure catalog question; no module bytes
are needed to answer it:

```rust
use openxcvr::catalog::family_catalog;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = family_catalog(Family::Sff8636)
        .ok_or_else(|| std::io::Error::other("no catalog"))?;
    let at_2_12 = field_availability(catalog, Rev::SFF8636_2_12)?;
    assert!(at_2_12.field("vendor_name").unwrap().is_available());
    // A field the schema gates out at this revision stays listed, gated.
    let report = field_availability(catalog, Rev::SFF8636_2_12)?;
    println!("{report}");
    Ok(())
}
```

To poke at a capture interactively, run the `explore-module` example
(`cargo run -p openxcvr --example explore-module`): it lists supplied
pages, hex-dumps a page, and decodes one field at a time, refusing an
unknown revision rather than guessing.

## What the library does not do for you

- It does not capture anything. Reading a live module's memory is the
  [live monitoring](live-monitoring.md) path.
- It does not fetch, pad or trim files. A missing page is an I/O error
  from the mmap constructors; a short capture fails rather than decoding
  zeros.
- It does not verify provenance or scrub vendor identity. `distill-capture.py`
  is the tool for turning a private capture into a publishable synthetic
  fixture; the library decodes whatever bytes it is handed.
- It does not guess. An unknown identifier, an unknown revision byte or
  an unspecified (`0x00`) compliance code is an error, and no later
  revision is substituted.

## Limits today

- `open_cmis_mmap` requires lower memory plus Pages 00h–05h. The
  committed synthetic CMIS captures do not include 03h–05h (the 5.2 set
  carries lower plus 00h, 01h, 02h, 10h and 11h), so they open through
  `CmisModule::from_bytes` (or
  `explore-module`), not through `open_cmis_mmap`.
- `open_sff8472_mmap` and `open_sff8636_mmap` are rev-agnostic: they
  decode through the union of every generated field and report no
  revision. The committed synthetic SFF-8636 captures carry compliance
  byte `0x00`, which the detect-once constructors refuse.
- C-CMIS, ELSFP, CMIS-LT and CMIS-VCS appear in the generated reference,
  but the unified handle does not open them and no catalog answers for
  them.
- A dense image in `explore-module` has zeros between sparse pages; those
  gaps are not decoded, and vendor identity needs Page 00h to have been
  supplied.

## Next

- Recipes: [identify family and revision](../cookbook/identify-family-and-revision.md),
  [read vendor and serial](../cookbook/read-vendor-and-serial.md),
  [explain a revision-gated field](../cookbook/explain-a-revision-gated-field.md).
- [API guide](api-guide.md) for the constructor surface and the test kit;
  [error handling](error-handling.md) for the failure modes above.
- Capture provenance: [fixture corpus](../../tests/fixtures/README.md).
