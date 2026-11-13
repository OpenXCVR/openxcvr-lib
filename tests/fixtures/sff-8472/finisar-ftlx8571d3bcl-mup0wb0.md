# finisar-ftlx8571d3bcl-mup0wb0

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (A0h:0) | `0x03` (SFP / SFP+ / SFP28) | SFF-8024 Table 4-1 |
| Vendor name (A0h:20–35) | `FINISAR CORP.` | A0h base ID |
| Vendor PN (A0h:40–55) | `FTLX8571D3BCL` | A0h base ID |
| Vendor S/N (A0h:68–83) | `MUP0WB0` | A0h base ID |
| Date code (A0h:84–91) | `160107` (2016-01-07) | A0h base ID |
| Connector (A0h:2) | `0x07` (LC) | SFF-8024 Table 4-3 |
| Encoding (A0h:11) | `0x06` (64B/66B) | SFF-8472 Table 5-2 |
| Nominal bit rate (A0h:12) | `103` × 100 Mbps = 10.3 Gbps | A0h base ID |
| Module type | 10GBASE-SR SFP+ | Vendor PN lookup |

## DDM capabilities

A0h:92 = `0x68` = `01101000`:

- bit 6 (DDM implemented): **1**
- bit 5 (internally calibrated): **1**
- bit 4 (externally calibrated): **0**
- bit 3 (Rx power measurement): **1** (average power, not OMA)
- bit 2 (address change required): **0**

This module exercises the **internal-calibration** branch of the SFF-8472 calibration variants in `schemas/sff-8472/page-a2h-diagnostics-base.yaml` — the discriminator path `page_a0h_base_memory.externally_calibrated` resolves to `false`, so all real-time monitors decode through `scaled_signed` / `scaled_unsigned` with fixed LSB units (no slope/offset application, no `rx_power` polynomial).

## Live captured real-time monitors (A2h:96–109)

- Temperature: **10.10 °C**
- Vcc: **3.3162 V**
- Tx bias: **7.18 mA**

These are real readings captured at FCC test time, not zeroed defaults — useful for end-to-end decode parity tests.

## Pages included

- `finisar-ftlx8571d3bcl-mup0wb0.a0h.bin` (128 bytes, A0h:0–127 — base ID + DDM type byte). The A0h vendor-specific upper region (A0h:128–255) is not in the OOM dump.
- `finisar-ftlx8571d3bcl-mup0wb0.a2h.bin` (256 bytes, A2h:0–255 — full diagnostics + user EEPROM). The A2h paged regions (Pages 00h/01h/02h/03h via byte 127 page-select) are not in the dump.

## Source + license

- **URL**: https://github.com/opencomputeproject/oom/blob/master/oom/module_data/MUP0WB0_EEPROM_20160108_192637.txt and `.../MUP0WB0_FCCABY_20160108_192637.txt`
- **License**: MIT — Copyright (c) 2016 ocpnetworking-wip
- **Anonymization status**: none required. OCP redistributed with explicit MIT permission; vendor S/N + date code preserved as captured.

## Regeneration

```bash
mkdir -p /tmp/oom && cd /tmp/oom
curl -O https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/MUP0WB0_EEPROM_20160108_192637.txt
curl -O https://raw.githubusercontent.com/opencomputeproject/oom/master/oom/module_data/MUP0WB0_FCCABY_20160108_192637.txt
cd /path/to/openxcvr-lib
python3 tools/oom-to-bin.py /tmp/oom/MUP0WB0_EEPROM_20160108_192637.txt tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0
python3 tools/oom-to-bin.py /tmp/oom/MUP0WB0_FCCABY_20160108_192637.txt tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0
```
