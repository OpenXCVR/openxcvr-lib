# finisar-ftlx8571d3bcl-muq1bzb

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (A0h:0) | `0x03` (SFP / SFP+ / SFP28) | SFF-8024 Table 4-1 |
| Vendor name (A0h:20–35) | `FINISAR CORP.` | A0h base ID |
| Vendor PN (A0h:40–55) | `FTLX8571D3BCL` | A0h base ID |
| Vendor S/N (A0h:68–83) | `MUQ1BZB` | A0h base ID |
| Date code (A0h:84–91) | `160107` (2016-01-07) | A0h base ID |
| Connector (A0h:2) | `0x07` (LC) | SFF-8024 Table 4-3 |
| Encoding (A0h:11) | `0x06` (64B/66B) | SFF-8472 Table 5-2 |
| Nominal bit rate (A0h:12) | `103` × 100 Mbps = 10.3 Gbps | A0h base ID |
| Module type | 10GBASE-SR SFP+ | Vendor PN lookup |

Same model + production date as `finisar-ftlx8571d3bcl-mup0wb0` — kept as a separate fixture for **identity-fields cross-check**: A0h bytes outside the S/N range (68–83) and the CC_BASE checksum (95) should be byte-identical between the two; only S/N + checksum differ. This is a useful regression test against any decoder that accidentally cross-contaminates S/N with surrounding fields.

## DDM capabilities

A0h:92 = `0x68` (same byte pattern as MUP0WB0 — DDM impl + internal cal + Rx average power, no external cal, no address change).

Like its pair, this module exercises the **internal-calibration** branch of the SFF-8472 calibration variants.

## Live captured real-time monitors (A2h:96–109)

- Temperature: **12.56 °C**
- Vcc: **3.2556 V**
- Tx bias: **7.32 mA**

Different live readings from MUP0WB0 — confirms these are independent captures, not duplicated test data.

## Pages included

- `finisar-ftlx8571d3bcl-muq1bzb.a0h.bin` (128 bytes, A0h:0–127)
- `finisar-ftlx8571d3bcl-muq1bzb.a2h.bin` (256 bytes, A2h:0–255)

## Source + license

- **URL**: https://github.com/opencomputeproject/oom/blob/master/oom/module_data/MUQ1BZB_EEPROM_20160108_192514.txt and `.../MUQ1BZB_FCCABY_20160108_192514.txt`
- **License**: MIT — Copyright (c) 2016 ocpnetworking-wip
- **Anonymization status**: none required.

## Regeneration

```bash
mkdir -p /tmp/oom && cd /tmp/oom
curl -O https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/MUQ1BZB_EEPROM_20160108_192514.txt
curl -O https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/MUQ1BZB_FCCABY_20160108_192514.txt
cd /path/to/openxcvr-lib
python3 tools/oom-to-bin.py /tmp/oom/MUQ1BZB_EEPROM_20160108_192514.txt tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-muq1bzb
python3 tools/oom-to-bin.py /tmp/oom/MUQ1BZB_FCCABY_20160108_192514.txt tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-muq1bzb
```
