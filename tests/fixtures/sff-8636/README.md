# SFF-8636 v2.11 fixtures

End-to-end decode verification for `openxcvr.page.v1` schemas under `schemas/sff-8636/`.

All fixtures are real EEPROM dumps redistributed by [opencomputeproject/oom](https://github.com/opencomputeproject/oom) (MIT). Per-fixture `.md` files document module identity, source, and pages covered.

## File layout

Each fixture is a multi-page module — one `.bin` per page slot, mirroring the schema topology:

- `<stem>.lower.bin` — Lower memory (128 bytes, always present). Decodes against `schemas/sff-8636/lower-memory.yaml`.
- `<stem>.page-00h.bin` — Upper Page 00h Administrative.
- `<stem>.page-01h.bin` — Upper Page 01h (deprecated stub in SFF-8636 v2.11).
- `<stem>.page-02h.bin` — Upper Page 02h User EEPROM.
- `<stem>.page-03h.bin` — Upper Page 03h Thresholds + Optional Channel Controls.

Pages 20h–21h (PAM4 monitoring) are not present in the OOM dumps — they apply only to PAM4-capable modules (typically 100G+ per lane), not to the older NRZ 40G/100G SR4 modules in this corpus.

## Index

| Stem | Vendor | PN | S/N | Date | Type | Module ID | Notable schema interactions |
|------|--------|----|-----|------|------|-----------|------------------------------|
| `finisar-ftl410qe3c-40g-sr4` | Finisar | FTL410QE3C | ETG09FZ | 2015-05-13 | 40GBASE-SR4 QSFP+ | 0x0D | Legacy QSFP+ identifier. Exercises 4-lane telemetry collapsed arrays. |
| `finisar-ftlc9551repm-100g-sr4` | Finisar | FTLC9551REPM | XUB0AAQ | 2015-09-26 | 100GBASE-SR4 QSFP28 | 0x11 | QSFP28 identifier. 4-lane 25G NRZ; exercises threshold sets on Page 03h. |
| `synthetic-qsfp-dac-40g-passive-cu` | OPENXCVR (synthetic) | SYN-QSFP-DAC | SYNTH000004 | 2026-05-26 | 40G QSFP+ passive copper DAC | 0x0D | No DDM: all-zero telemetry, connector=0x21. Exercises the no-monitoring lower-memory path for QSFP+. |
| `synthetic-qsfp28-bidi-100g-cwdm4` | OPENXCVR (synthetic) | SYN-QSFP28-BIDI | SYNTH000005 | 2026-05-26 | 100G CWDM4 QSFP28 (BiDi-style 2km SMF) | 0x11 | device_tech=0x4 (1550nm DFB), ext_spec=0x18 (100G CWDM4). 4-lane monitoring: Rx=0.5mW, Tx=0.4mW, bias=30mA. CC_BASE=0xEE, CC_EXT=0xD0. |
| `synthetic-qsfp28-aoc-100g-sr4` | OPENXCVR (synthetic) | SYN-QSFP28-AOC | SYNTH000006 | 2026-05-26 | 100GBASE-SR4 QSFP28 AOC (5m) | 0x11 | connector=0x01 (SC), device_tech=0x0 (850nm VCSEL), length_om4_m=5. 4-lane monitoring: Rx=0.6mW, Tx=0.5mW, bias=35mA. |

## Future expansions

Deferred to later batches:

- BiDi QSFP+ (asymmetric wavelength, older 40G form factor)
- 100GBASE-LR4 (longer-reach optical, LAN-WDM 4×25G)
