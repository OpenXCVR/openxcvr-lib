# fboss-cmis-5-3-osfp-800g-zr-dp-reserved

> **derived: true** (one byte changed from a real fboss capture — NOT a from-scratch synthetic)
> **vendor_strings_synthetic: false** (real fboss-supplied vendor strings, copied verbatim)
> Source: byte-for-byte copy of [`fboss-cmis-5-3-osfp-800g-zr`](fboss-cmis-5-3-osfp-800g-zr.lower.bin) (fboss `kCmis800GZr`, BSD-3-Clause — see [`ATTRIBUTIONS.md`](ATTRIBUTIONS.md)) with **exactly one byte changed** to drive the reserved-nibble decode path.

## The one-byte delta

| File | Offset | Source value | This fixture | Effect |
|------|--------|--------------|--------------|--------|
| `…page-11h.bin` | byte 0 (page-relative 0 = flat 128) | `0x44` | `0x48` | low nibble `0x4`→`0x8` |

Page 11h `dp_state_host_lane` packs two lanes per byte (`offset_expr "128 + (lane - 1) / 2"`,
`bit_offset_expr "((lane - 1) % 2) * 4"`, `bit_width 4`). Byte 0 holds **lane 1 in the low
nibble** and **lane 2 in the high nibble**. Changing `0x44`→`0x48`:

- **Lane 1** (low nibble): `0x4` (DPActivated) → `0x8`, which falls in the **`reserved_8_f`
  range** of `CmisDataPathState` (CMIS 5.3 § 8.10.1 / Table 8-84). All decoders must render
  this as the canonical unmapped string **`"Unknown8"`** — never panic.
- **Lane 2** (high nibble): `0x4` unchanged → still `"DpActivated"`.

Every other byte across all five pages is identical to the source capture.

## What this fixture exercises

**The reserved-nibble decode panic (the Page 11h / Page 16h DoS).** Before the fix, the Rust
page emitter was the only binding that honored `enum_ref` on instanced fields, and its generated
`TryFrom` `other => Err(other)` arm caused `decode()` to **panic** on any unmapped value — a
denial-of-service on untrusted EEPROM. The fix makes the generated enum *total*: a payload
`Unknown(repr)` catch-all variant, an infallible `From<repr>`, and `repr()`/`name()` accessors
where `name()` pins unmapped values to `"Unknown{n}"`. This fixture is the end-to-end proof: a
real-shape module that just happens to carry a reserved data-path-state nibble on one lane.

The cross-binding parity contract is the whole point — **all five bindings (Rust / C / Go /
Python / PHP) must independently emit the identical string `"Unknown8"`** for `lanes[0].dp_state`,
proving the totality fix did not diverge any binding from the others.

## What this fixture does NOT exercise

- **New field coverage.** Identity, applications, monitoring, flags, and the other seven lanes
  are byte-identical to `fboss-cmis-5-3-osfp-800g-zr`; their golden values are unchanged. The
  only golden delta vs the source is `lanes[0].dp_state`.
- **A real-world reserved value.** No shipping module is known to advertise `dp_state == 0x8`;
  this is a deliberately-injected adversarial byte. That is exactly the point — the decoder must
  survive bytes no compliant module would send.

## Pages included

Five pages — the union of what every binding's CMIS fixture loader reads:

- `…lower.bin` — Lower memory (identity dispatch, module state).
- `…page-00h.bin` — Administrative (vendor identity).
- `…page-01h.bin` — Advertising (application descriptors).
- `…page-02h.bin` — Module/Lane Thresholds (required by the Rust `open_fixture` loader; not read
  by the C/Go/Python/PHP loaders, which stop at page-01h + page-11h).
- `…page-11h.bin` — Banked Lane Status (carries the modified `dp_state_host_lane` byte).

The source capture has 19 pages; only these five are needed to drive the lane-status decode, so
the derived fixture ships the minimal set.

## Reproducibility

```bash
SRC=tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr
DST=tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr-dp-reserved
for p in lower page-00h page-01h page-02h page-11h; do cp "$SRC.$p.bin" "$DST.$p.bin"; done
# flip page-11h byte 0: 0x44 -> 0x48
python3 -c "import sys; d=bytearray(open('$DST.page-11h.bin','rb').read()); assert d[0]==0x44; d[0]=0x48; open('$DST.page-11h.bin','wb').write(d)"
```

## License

The byte sequences (save the one injected byte) are derived from `facebook/fboss`
(`fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp`, `kCmis800GZr`) under BSD-3-Clause.
The `fboss-cmis-*` prefix keeps this fixture under the umbrella attribution in
[`ATTRIBUTIONS.md`](ATTRIBUTIONS.md). The single injected byte (`0x48`) is an openxcvr test
artifact, not third-party data.

## Issue tracker

- Regression target: the Page 11h / Page 16h reserved data-path-state decode panic (untrusted
  EEPROM DoS). The Rust totality fix lives in
  `crates/openxcvr-codegen/src/emitters/rust/enums.rs`; the parity guard is
  `crates/openxcvr-cmis/tests/golden.rs`
  (`cmis_lanes_flags_match_golden_dp_state_reserved`) plus the auto-discovered C/Go/Python/PHP
  parity runners.
