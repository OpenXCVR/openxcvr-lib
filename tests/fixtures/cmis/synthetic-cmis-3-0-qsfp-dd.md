# synthetic-cmis-3-0-qsfp-dd

> **synthetic: true**
> **vendor_strings_synthetic: true**
> Source: hand-authored from CMIS 3.0 spec (`docs/stds/cmis/CMIS3p0_Third_Party_Spec.pdf`, QSFP-DD MSA, "Common Management Specification Rev 3.0", September 18, 2018 — the file previously cited here as `QSFP-DD-mgmt-rev3p0-final-9-18-18.pdf` is the same document under its pre-re-host name). NO bytes copied from any third-party source — in particular, NO bytes copied from `floodlight/bigcode` (EPL-1.0; incompatible with the project's fixture-license boundary, see the design doc cited below).

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x18` (QSFP-DD) | SFF-8024 4.13.2 § 4.1 Table 4-1 |
| Revision Compliance Identifier (Lower:1) | `0x30` (CMIS 3.0; upper nibble=3, lower=0) | CMIS 3.0 § 1.7.2.1 Table 18 |
| Flat_mem (Lower:2[7]) | `0` (paged) | CMIS 3.0 § 1.7.2.1 Table 18 |
| CLEI present (Lower:2[6]) | `0` (no CLEI) | CMIS 3.0 § 1.7.2.1 Table 18 |
| Identifier copy (Page 00h:0 = flat 128) | `0x18` (mirror) | CMIS 3.0 § 1.7.3.1 Table 28 |
| Vendor name (Page 00h:1–16) | `CMIS_3_0_SYNTH ` | Synthetic placeholder |
| Vendor OUI (Page 00h:17–19) | `00 00 00` | Synthetic placeholder |
| Vendor PN (Page 00h:20–35) | `FIXTURE-30-SYNTH` | Synthetic placeholder |
| Vendor rev (Page 00h:36–37) | `00` | Synthetic placeholder |
| Vendor S/N (Page 00h:38–53) | `SYNTHETIC0000003` | Synthetic placeholder |
| Date code (Page 00h:54–61) | `18091800` (CMIS 3.0 publish 2018-09-18 + 2 lot chars) | Synthetic placeholder |
| CLEI code (Page 00h:62–71) | 10 ASCII spaces | Synthetic placeholder (consistent with Lower:2[6]=0) |
| Page 00h checksum (Page 00h:94) | `0x5D` (sum of bytes 0–93 & 0xFF) | CMIS 3.0 § 1.7.3.15 |

## What this fixture exercises

**Decoder dispatch test** — the MajorRevision-byte branch for CMIS 3.0 (`Lower:1 == 0x30`). The decoder under test must observe `0x30` in `Lower:1` and route this fixture into the CMIS 3.0 decode path.

**Limited synthetic parity golden** — `tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd.json` pins the structural decode of the authored bytes: identity strings, the zero-value module/monitoring paths, the all-Reserved Page 11h data-path states, and the empty application advertisement. The golden's expected semantics are authored from the fixture bytes plus the CMIS 3.0 spec tables (never by running a decoder and capturing its output); `tools/build-cmis-3-0-synthetic.py` regenerates both the `.bin` pages and the golden deterministically, and `tools/test_build_cmis_synthetics.py` pins that reproduction. It is a **limited synthetic parity golden, NOT a hardware oracle** — see the `synthetic_note` metadata field for the full classification.

Two areas the design doc flagged as test-significant for CMIS 3.0, with the spec-grounded facts now verified directly against the 3.0 PDF:

1. **DPSM state encoding** (Page 11h Data Path State Indicator, § 1.7.7.1 Tables 65/66): 0h = **Reserved**, 1h = DataPathDeactivated, 2h = DataPathInit, 3h = DataPathDeinit, 4h = DataPathActivated, 5h-Fh = Reserved. These values are **stable** into CMIS 4.0 (Table 8-58) and 5.2/5.3 — 4.0 only *added* the 5h-7h codes (DataPathTxTurnOn/TxTurnOff/Initialized) that 3.0 leaves Reserved; no value was re-encoded. This fixture's all-zero Page 11h therefore decodes `dp_state: "Reserved0"` on every lane (0h is Reserved in 3.0's own table — it is **not** Deactivated; Deactivated is 1h). A 3.0-specific field-content test would still be needed to exercise non-zero nibbles.
2. **Page 01h advertising layout** — several bits were repurposed in 4.0. The two regions this fixture's golden reads are stable across revisions: the additional Application Descriptor slots at bytes 223-250 (Table 48; 5.2's Table 8-53 is byte-identical) and the Tx-bias multiplier at byte 160 bits 4-3 (Table 45; same 00b=x1/01b=x2/10b=x4/11b=reserved encoding as 5.2's Table 8-47).

Pages 01h + 11h in this fixture are zero-filled; a synthetic-zero is acceptable for the dispatch test and the zero-value golden paths, but a future field-round-trip test for CMIS 3.0 specifics would need the 3.0-specific bit layout authored.

**Nonconformance disclosure (Application advertisement).** The zeroed Application advertisement is an intentional nonconforming fixture: CMIS 3.0 § 1.7.2.7 requires every module to advertise at least one Application and requires FFh in the Host Electrical Code of the first unused Table 26 entry; this fixture does neither (Lower 86-117 and the Page 01h descriptor slots are all 0x00, so no FFh terminator is authored either). The authored bytes are preserved as-is. The golden's `applications: null` and `decode_status: "ok"` are the structural decode of those bytes — `"ok"` is decode-success, not conformance.

## What this fixture does NOT exercise

- **Field-round-trip-stability tests.** Vendor strings are explicit synthetic placeholders. Pages 01h, 02h, 10h, 11h are all zeros (no Applications advertised, no thresholds, no lane state, no DPSM activity).
- **Decode parity against a real CMIS 3.0 module.** No real-vendor CMIS 3.0 EEPROM exists in the corpus. The only known 3.0 fixture in the wild is in `floodlight/bigcode` (`modules/sff/module/src/sff_db.c` lines 1844–1892, EPL-1.0) which the project's fixture-license boundary forbids importing.

## Pages included

User-chosen coverage: **Lower + Pages 00h, 01h, 02h, 10h, 11h** — 6 × 128 bytes = 768 total bytes.

- `synthetic-cmis-3-0-qsfp-dd.lower.bin` (128 bytes, addresses 00h–7Fh).
- `synthetic-cmis-3-0-qsfp-dd.page-00h.bin` (128 bytes; identity strings + checksum).
- `synthetic-cmis-3-0-qsfp-dd.page-01h.bin` (128 bytes, all-zero — Application Advertising).
- `synthetic-cmis-3-0-qsfp-dd.page-02h.bin` (128 bytes, all-zero — Module/Lane Thresholds).
- `synthetic-cmis-3-0-qsfp-dd.page-10h.bin` (128 bytes, all-zero — Lane Control).
- `synthetic-cmis-3-0-qsfp-dd.page-11h.bin` (128 bytes, all-zero — Lane Status + DPSM).

## Reproducibility

The `.bin` files **and the golden JSON** are emitted by `tools/build-cmis-3-0-synthetic.py`. To regenerate:

```bash
python3 tools/build-cmis-3-0-synthetic.py
python3 -m unittest tools.test_build_cmis_synthetics
```

Each byte's value is documented inline in that script with the corresponding CMIS 3.0 spec section reference, and the golden's expected semantics are computed from those bytes plus the 3.0 tables (Tables 18/19/21/22/25/26/45/48/65/66/70). The run is deterministic — the authorship stamp is pinned in the script — so re-running reproduces the committed `.bin` pages and `.json` golden byte-for-byte; the unittest pins that reproduction and fails on any drift. Companion script for CMIS 5.2: `tools/build-cmis-5-2-synthetic.py`.

## Differences vs. CMIS 5.2

The following bytes differ from `synthetic-cmis-5-2-qsfp-dd` to reflect the spec evolution:

- `Lower:1` = `0x30` (was `0x52` in 5.2).
- `Lower:2[6]` is "CLEI present" in 3.0 (renamed "SteppedConfigOnly" in 5.x). Value `0` is benign in both interpretations.
- `Page 00h:54–61` date code reflects CMIS 3.0's 2018-09-18 publish date.
- Vendor strings carry the `CMIS_3_0_SYNTH` / `FIXTURE-30-SYNTH` / `SYNTHETIC0000003` markers.

Page 00h checksum differs accordingly: `0x5D` for 3.0 vs `0x79` for 5.2 (different byte content → different sum).

Identity-string layouts (vendor name / OUI / PN / rev / S/N positions in Page 00h) are byte-for-byte identical between 3.0 and 5.2 — verified against CMIS 3.0 § 1.7.3.2–§ 1.7.3.7 and CMIS 5.2 § 8.3.2.

## License

Hand-authored by the openxcvr project. No third-party byte sequences. Distributable under the project's license terms.

## Issue tracker

- Plan: _(archived — the `docs/superpowers/` tree was removed)_ task 4.
- Written because no redistributable CMIS 3.0 capture was available.
- Background design doc: _(archived — removed with the `docs/superpowers/` tree)_.
