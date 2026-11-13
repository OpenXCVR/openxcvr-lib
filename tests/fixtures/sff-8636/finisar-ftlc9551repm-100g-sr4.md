# finisar-ftlc9551repm-100g-sr4

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x11` (QSFP28) | SFF-8024 Table 4-1 |
| Vendor name (Page 00h:20–35) | `FINISAR CORP` | Upper Page 00h |
| Vendor PN (Page 00h:40–55) | `FTLC9551REPM` | Upper Page 00h |
| Vendor S/N (Page 00h:68–83) | `XUB0AAQ` | Upper Page 00h |
| Date code (Page 00h:84–91) | `150926` (2015-09-26) | Upper Page 00h |
| Connector (Page 00h:2) | `0x0C` (MPO-12) | SFF-8024 Table 4-3 |
| Extended identifier (Page 00h:1) | `0xCC` (power class encoded) | SFF-8636 Table 6-15 |
| Compliance byte 3 | `0x80` (extended specification — see SFF-8024 Table 4-4) | SFF-8636 Table 6-17 |
| Encoding (Page 00h:11) | `0x07` | SFF-8636 Table 6-16 |
| Nominal bit rate (Page 00h:12) | `0xFF` (defer to extended bit rate Page 00h:222) | SFF-8636 |
| OM1 reach (Page 00h:18) | 50 m | Upper Page 00h |
| Device technology (Page 00h:27) | `0x20` (850 nm VCSEL) | SFF-8636 Table 6-19 |
| Module type | 100GBASE-SR4 QSFP28 | Vendor PN lookup |

## Pages included

All 5 pages of the QSFP28 paged memory model:

| File | Schema target |
|------|---------------|
| `finisar-ftlc9551repm-100g-sr4.lower.bin` (128 bytes) | `schemas/sff-8636/lower-memory.yaml` |
| `finisar-ftlc9551repm-100g-sr4.page-00h.bin` (128 bytes) | `schemas/sff-8636/page-00h-upper-administrative.yaml` |
| `finisar-ftlc9551repm-100g-sr4.page-01h.bin` (128 bytes) | (deprecated stub) |
| `finisar-ftlc9551repm-100g-sr4.page-02h.bin` (128 bytes) | `schemas/sff-8636/page-02h-user-eeprom.yaml` |
| `finisar-ftlc9551repm-100g-sr4.page-03h.bin` (128 bytes) | `schemas/sff-8636/page-03h-thresholds-and-controls.yaml` |

PAM4 monitoring pages (20h–21h) are not present — this is a 25G NRZ × 4 module, not a 50G PAM4 × 2 module.

## Notable schema interactions

- 4-lane QSFP28 — same collapsed-array per-lane DOM shape as the 40G QSFP+ but at 25 Gbps per lane.
- Bit rate `0xFF` is the SFF-8636 sentinel meaning "see extended bit rate at Page 00h byte 222" — useful for testing the bit-rate dispatch path.
- Compliance byte 3 bit 7 = `1` indicates an extended-specification code lives at Page 00h byte 192. Decoding the 100GBASE-SR4 application requires reading both bytes — useful for testing cross-byte compliance dispatch.
- Threshold sets on Page 03h cover temperature, voltage, Tx bias, Tx power, Rx power with high alarm / low alarm / high warning / low warning ordering — same shape as SFF-8472 and CMIS, validating the three-way convergence.

## Source + license

- **URL**: https://github.com/opencomputeproject/oom/blob/master/oom/module_data/qsfp28datafile.txt
- **License**: MIT — Copyright (c) 2016 ocpnetworking-wip
- **Anonymization status**: none required.

## Regeneration

```bash
curl -o /tmp/qsfp28datafile.txt https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/qsfp28datafile.txt
python3 tools/oom-to-bin.py /tmp/qsfp28datafile.txt tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4
```
