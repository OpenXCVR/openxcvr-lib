# SFF-8472 v12.5a fixtures

End-to-end decode verification for `openxcvr.page.v1` schemas under `schemas/sff-8472/`.

All fixtures are real EEPROM dumps redistributed by [opencomputeproject/oom](https://github.com/opencomputeproject/oom) (MIT). Per-fixture `.md` files document module identity, source, and pages covered.

## File layout

Each fixture writes two device-address slices:

- `<stem>.a0h.bin` — A0h base ID memory. OOM provides 128 bytes (A0h:0–127). The A0h vendor-specific upper region (A0h:128–255) is not in the OOM corpus.
- `<stem>.a2h.bin` — A2h diagnostics + user EEPROM, full 256 bytes. Decodes against `schemas/sff-8472/page-a2h-diagnostics-base.yaml`.

The A2h paged regions (Pages 00h/01h/02h/03h, accessed via byte 127 page-select on A2h) are not covered by the OOM dumps — these are the target of synthetic fixtures next batch.

## Index

| Stem | Vendor | PN | S/N | Date | Type | Notable schema interactions |
|------|--------|----|-----|------|------|------------------------------|
| `finisar-ftlx8571d3bcl-mup0wb0` | Finisar | FTLX8571D3BCL | MUP0WB0 | 2016-01-07 | 10GBASE-SR SFP+ | Internal calibration (DDM 0x68 = bits 6,5,3); exercises non-variant decode path of the calibration variants. Temperature thresholds: high alarm +78°C, low alarm −13°C. |
| `finisar-ftlx8571d3bcl-muq1bzb` | Finisar | FTLX8571D3BCL | MUQ1BZB | 2016-01-07 | 10GBASE-SR SFP+ | Same model as MUP0WB0 with a different S/N — cross-check that A0h identity bytes are identical and only S/N (68–83) + CC_BASE differ. |
| `synthetic-extcal-sfp10g-sr` | OPENXCVR (synthetic) | SYN-SFP10G-SR | SYNTH000001 | 2026-05-26 | 10GBASE-SR SFP+ (external calibration) | Exercises external calibration path: dmt=0x58 (DDM+ext_cal+int_cal), Rx power poly c0=1.0 identity, linear slope=0x0100 (1.0 in 8.8fp). CC_DMI covers A2h:0–94. |
| `synthetic-dac-sfp10g-passive-cu` | OPENXCVR (synthetic) | SYN-SFP10G-DAC | SYNTH000002 | 2026-05-26 | 10G passive copper DAC | No DDM (dmt=0x00), connector=0x21 (copper pigtail). CC_DMI=0x00 (all-zero A2h). Exercises the no-monitoring decode path. |
| `synthetic-sfp28-25g-sr` | OPENXCVR (synthetic) | SYN-SFP28-25G-SR | SYNTH000003 | 2026-05-26 | 25GBASE-SR SFP28 | Bit-rate overflow: A0h:12=0xFF, actual rate at A0h:66=103 (25750 Mb/s). ext_spec_compliance=0x1C (25GBASE-SR/LR). |

## Future expansions

Real dumps to acquire (deferred — see corpus README):

- BiDi SFP+ (single-fiber wavelength asymmetry)
- DWDM tunable SFP+ (Page 02h tunability via SFF-8690 supplement)
