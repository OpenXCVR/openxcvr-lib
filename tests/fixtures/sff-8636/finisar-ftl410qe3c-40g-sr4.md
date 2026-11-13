# finisar-ftl410qe3c-40g-sr4

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x0D` (QSFP+) | SFF-8024 Table 4-1 |
| Vendor name (Page 00h:20–35) | `FINISAR CORP` | Upper Page 00h |
| Vendor PN (Page 00h:40–55) | `FTL410QE3C` | Upper Page 00h |
| Vendor S/N (Page 00h:68–83) | `ETG09FZ` | Upper Page 00h |
| Date code (Page 00h:84–91) | `150513` (2015-05-13) | Upper Page 00h |
| Connector (Page 00h:2) | `0x0C` (MPO-12) | SFF-8024 Table 4-3 |
| Compliance byte 3 | `0x04` (40GBASE-SR4) | SFF-8636 Table 6-17 |
| Encoding (Page 00h:11) | `0x05` (64B/66B) | SFF-8636 Table 6-16 |
| Nominal bit rate (Page 00h:12) | `103` × 100 Mbps = 10.3 Gbps per lane | Upper Page 00h |
| OM1 reach (Page 00h:18) | 75 m | Upper Page 00h |
| Device technology (Page 00h:27) | `0x20` (850 nm VCSEL) | SFF-8636 Table 6-19 |
| Module type | 40GBASE-SR4 QSFP+ | Vendor PN lookup |

## Pages included

All 5 pages of the QSFP+ paged memory model (per SFF-8636 v2.11 § 6.1):

| File | Schema target |
|------|---------------|
| `finisar-ftl410qe3c-40g-sr4.lower.bin` (128 bytes) | `schemas/sff-8636/lower-memory.yaml` |
| `finisar-ftl410qe3c-40g-sr4.page-00h.bin` (128 bytes) | `schemas/sff-8636/page-00h-upper-administrative.yaml` |
| `finisar-ftl410qe3c-40g-sr4.page-01h.bin` (128 bytes) | (deprecated stub in SFF-8636 v2.11) |
| `finisar-ftl410qe3c-40g-sr4.page-02h.bin` (128 bytes) | `schemas/sff-8636/page-02h-user-eeprom.yaml` |
| `finisar-ftl410qe3c-40g-sr4.page-03h.bin` (128 bytes) | `schemas/sff-8636/page-03h-thresholds-and-controls.yaml` |

PAM4 monitoring pages (20h–21h) are not present — this is an NRZ 40G/4×10G module, the PAM4 spec extension applies only to 100G+ per-lane modules.

## Notable schema interactions

- 4-lane QSFP — exercises collapsed-array per-lane DOM fields (`rx_power_lane`, `tx_bias_lane`, `tx_power_lane` indexed by lane 1–4) on the Lower page.
- `device_technology = 0x20` (upper nibble 0x2 = 850 nm VCSEL) routes through the `device_technology` enum in `schemas/sff-8636/enums/device-technology.yaml`.
- Bit rate field `0x67` = legacy 8-bit form; no extended bit-rate field needed (Page 00h:222 is unused for 40G/Lane modules).

## Source + license

- **URL**: https://github.com/opencomputeproject/oom/blob/master/oom/module_data/qsfpdatafile.txt
- **License**: MIT — Copyright (c) 2016 ocpnetworking-wip
- **Anonymization status**: none required.

## Regeneration

```bash
curl -o /tmp/qsfpdatafile.txt https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/qsfpdatafile.txt
python3 tools/oom-to-bin.py /tmp/qsfpdatafile.txt tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4
```
