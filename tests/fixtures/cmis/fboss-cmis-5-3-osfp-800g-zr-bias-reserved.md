# fboss-cmis-5-3-osfp-800g-zr-bias-reserved

> **derived: true** (one byte changed from a real fboss capture — NOT a from-scratch synthetic)
> **vendor_strings_synthetic: false** (real fboss-supplied vendor strings, copied verbatim)
> Source: byte-for-byte copy of [`fboss-cmis-5-3-osfp-800g-zr`](fboss-cmis-5-3-osfp-800g-zr.lower.bin) (fboss `kCmis800GZr`, BSD-3-Clause — see [`ATTRIBUTIONS.md`](ATTRIBUTIONS.md)) with **exactly one byte changed** to drive the reserved Tx-bias-scaling decode path.

## The one-byte delta

| File | Offset | Source value | This fixture | Effect |
|------|--------|--------------|--------------|--------|
| `…page-01h.bin` | byte 32 (page-relative 32 = flat **Page 01h:160**) | `0x17` | `0x1F` | bits 4:3 `0b10`→`0b11` |

Page 01h:160 bits 4:3 are the **LaserBiasTx current scaling code** (CMIS 5.3 § 8.4.5 /
Table 8-52). The two-bit code selects the multiplier applied to every per-lane raw
`LaserBiasTx` value on Page 11h:

| code | multiplier |
|------|------------|
| `0b00` | ×1 |
| `0b01` | ×2 |
| `0b10` | ×4 |
| `0b11` | **reserved** |

The source capture carries `0b10` (×4): with raw `28825`, lane 1 decodes to
`28825 × 0.002 × 4 = 230.6 mA`. Setting **one bit** (bit 3: `0x17`→`0x1F`) moves the code to
the reserved `0b11`, for which the multiplier is undefined.

Because the scaling code is a **single module-wide field** — not per-lane — the reserved value
makes the Tx-bias current *unrepresentable for all eight lanes at once*. Every decoder must
render `tx_bias_ma: null` on **every** lane while leaving `rx_power_mw`, `tx_power_mw`,
`dp_state`, and all per-lane flags intact. Never abort the whole lane block; never panic.

Every other byte across all five pages is identical to the source capture.

## What this fixture exercises

**The reserved Tx-bias-scaling abort (Site 1 of the reserved-value bug area, issues #198 / #200).**
Before the fix, all five bindings treated the reserved `0b11` scaling code as a hard error and
**dropped the entire per-lane `lanes`/`flags` block** (Rust returned `Err`; C/Go/Python/PHP
raised/threw). That is a denial-of-decode on untrusted EEPROM: one reserved nibble erased every
lane's optical power and data-path state, not just the one unrepresentable field. The fix narrows
the blast radius to exactly the affected field — the multiplier is undefined, so `tx_bias_ma`
becomes `null`, and nothing else changes.

The cross-binding parity contract is the whole point — **all five bindings (Rust / C / Go /
Python / PHP) must independently emit `tx_bias_ma: null` on all eight lanes** while every other
decoded value matches the source golden, proving the recovery path did not diverge any binding
from the others.

## What this fixture does NOT exercise

- **New field coverage.** Identity, applications, monitoring, flags, `dp_state`, and the optical
  power values are byte-identical to `fboss-cmis-5-3-osfp-800g-zr`; their golden values are
  unchanged. The only golden delta vs the source is `lanes[*].tx_bias_ma` (now `null` on all
  eight lanes).
- **A real-world reserved value.** No shipping module is known to advertise the reserved `0b11`
  Tx-bias scaling code; this is a deliberately-injected adversarial byte. That is exactly the
  point — the decoder must survive bytes no compliant module would send.

## Pages included

Five pages — the union of what every binding's CMIS fixture loader reads:

- `…lower.bin` — Lower memory (identity dispatch, module state).
- `…page-00h.bin` — Administrative (vendor identity).
- `…page-01h.bin` — Advertising (application descriptors **and** the modified Tx-bias scaling byte).
- `…page-02h.bin` — Module/Lane Thresholds (required by the Rust `open_fixture` loader; not read
  by the C/Go/Python/PHP loaders, which stop at page-01h + page-11h).
- `…page-11h.bin` — Banked Lane Status (carries the per-lane `LaserBiasTx` raw values scaled by
  the code above).

The source capture has 19 pages; only these five are needed to drive the lane-status decode, so
the derived fixture ships the minimal set.

## Reproducibility

```bash
SRC=tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr
DST=tests/fixtures/cmis/fboss-cmis-5-3-osfp-800g-zr-bias-reserved
for p in lower page-00h page-01h page-02h page-11h; do cp "$SRC.$p.bin" "$DST.$p.bin"; done
# flip page-01h byte 32 (Page 01h:160) bit 3: 0x17 -> 0x1F (bits 4:3 0b10 -> 0b11 reserved)
python3 -c "import sys; d=bytearray(open('$DST.page-01h.bin','rb').read()); assert d[32]==0x17; d[32]=0x1F; open('$DST.page-01h.bin','wb').write(d)"
```

## License

The byte sequences (save the one injected byte) are derived from `facebook/fboss`
(`fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp`, `kCmis800GZr`) under BSD-3-Clause.
The `fboss-cmis-*` prefix keeps this fixture under the umbrella attribution in
[`ATTRIBUTIONS.md`](ATTRIBUTIONS.md). The single injected byte (`0x1F`) is an openxcvr test
artifact, not third-party data.

## Issue tracker

- Regression target: the reserved Tx-bias-scaling abort — Site 1 of the reserved-value bug area
  (issues #198 / #200). The decode-recovery fix spans all five bindings
  (`crates/openxcvr-cmis/src/module.rs` `tx_bias_multiplier`; `bindings/{go,python,php,c}`); the
  parity guard is `crates/openxcvr-cmis/tests/golden.rs`
  (`cmis_lanes_flags_match_golden_bias_reserved`) plus the auto-discovered C/Go/Python/PHP parity
  runners. Site 2 (reserved data-path-state nibble) is covered by
  `fboss-cmis-5-3-osfp-800g-zr-dp-reserved`.
