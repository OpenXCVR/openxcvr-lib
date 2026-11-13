# synthetic-cmis-5-2-qsfp-dd

> **synthetic: true**
> **vendor_strings_synthetic: true**
> Source: hand-authored from CMIS 5.2 spec (`docs/stds/cmis/OIF-CMIS-05.2.pdf`, OIF, April 27, 2022). NO bytes copied from any third-party source. The byte layout is a tested-shape skeleton, not a real-vendor capture.

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x18` (QSFP-DD) | SFF-8024 4.13.2 § 4.1 Table 4-1 |
| CmisRevision (Lower:1) | `0x52` (5.2) | CMIS 5.2 § 8.2.1 Table 8-5 |
| MemoryModel (Lower:2[7]) | `0` (paged) | CMIS 5.2 § 8.2.1 Table 8-5 |
| Identifier copy (Page 00h:0 = flat 128) | `0x18` (mirror) | CMIS 5.2 § 8.3.1 Table 8-24 |
| Vendor name (Page 00h:1–16) | `CMIS_5_2_SYNTH ` | Synthetic placeholder |
| Vendor OUI (Page 00h:17–19) | `00 00 00` | Synthetic placeholder |
| Vendor PN (Page 00h:20–35) | `FIXTURE-SYNTH-V1` | Synthetic placeholder |
| Vendor rev (Page 00h:36–37) | `00` | Synthetic placeholder |
| Vendor S/N (Page 00h:38–53) | `SYNTHETIC0000001` | Synthetic placeholder |
| Date code (Page 00h:54–61) | `22042700` (CMIS 5.2 publish 2022-04-27 + 2 lot chars) | Synthetic placeholder |
| CLEI code (Page 00h:62–71) | 10 ASCII spaces | Synthetic placeholder |
| Page 00h checksum (Page 00h:94) | `0x79` (sum of bytes 0–93 & 0xFF) | CMIS 5.2 § 8.3.11 |

## What this fixture exercises

**Decoder dispatch test** — the MajorRevision-byte branch for CMIS 5.2 (`Lower:1 == 0x52`). The decoder under test must observe `0x52` in `Lower:1` and route this fixture into the CMIS 5.2 decode path (which, on `main`, will gracefully fall back to the CMIS 5.3 schemas for any field not changed between 5.2 and 5.3 — the rev branch primarily exists to catch cases where a 5.2-specific layout differs).

**Limited synthetic parity golden** — `tests/golden/cmis/synthetic-cmis-5-2-qsfp-dd.json` pins the structural decode of the authored bytes: identity strings, the zero-value module/monitoring paths, the all-Reserved Page 11h data-path states, and the empty application advertisement. The golden's expected semantics are authored from the fixture bytes plus the CMIS 5.2 spec tables (never by running a decoder and capturing its output); `tools/build-cmis-5-2-synthetic.py` regenerates both the `.bin` pages and the golden deterministically, and `tools/test_build_cmis_synthetics.py` pins that reproduction. It is a **limited synthetic parity golden, NOT a hardware oracle** — see the `synthetic_note` metadata field for the full classification.

Spec-grounded decode of the zeroed pages (all citations from OIF-CMIS-05.2): Page 11h's all-zero DPStateHostLane nibbles are 0h = **Reserved** per § 8.10.1 Table 8-77 — **not** DPDeactivated, which is 1h ("DPDeactivated (or unused lane)") — so the golden renders `dp_state: "Reserved0"` on every lane; ModuleState 000b = Reserved (Tables 8-6/8-7); byte 3 bit 0 InterruptDeasserted = 0b means Interrupt asserted (Table 8-6); monitors read 0.0 (Table 8-10); module flags are clear (Table 8-9); no AppSel code is advertised — Lower 86-117 (Table 8-20) and Page 01h 223-250 (Table 8-53) are all zero — so `applications` renders null; and the TxBiasCurrentScalingFactor at Page 01h:160 bits 4-3 (Table 8-47) decodes as 00b = ×1.

**Nonconformance disclosure (Application advertisement).** The zeroed Application advertisement is an intentional nonconforming fixture: CMIS 5.2 § 8.2.11 requires every module to advertise at least one Application and requires FFh in the HostInterfaceID of the first unused Application Descriptor; this fixture does neither (Lower 86-117 and the Page 01h descriptor slots are all 0x00, so no FFh terminator is authored either). The authored bytes are preserved as-is. The golden's `applications: null` and `decode_status: "ok"` are the structural decode of those bytes — `"ok"` is decode-success, not conformance.

## What this fixture does NOT exercise

- **Field-round-trip-stability tests.** The vendor strings are explicit synthetic placeholders, not real-vendor identity bytes. Pages 01h, 02h, 10h, 11h are all zeros (no Applications advertised, no thresholds, no lane state) — a strict decoder may consider this an underspecified module, but the dispatch test does not read those pages.
- **Decode parity against a real CMIS 5.2 module.** No real-vendor CMIS 5.2 EEPROM exists in our corpus (per `2026-05-06-cmis-3-and-5-2-fixtures-design.md`); all vendor outreach paths are pending.

## Pages included

The user-chosen coverage for this fixture is **Lower + Pages 00h, 01h, 02h, 10h, 11h** — 6 × 128 bytes = 768 total bytes.

- `synthetic-cmis-5-2-qsfp-dd.lower.bin` (128 bytes, addresses 00h–7Fh).
- `synthetic-cmis-5-2-qsfp-dd.page-00h.bin` (128 bytes, page-relative 0–127 = flat 128–255).
- `synthetic-cmis-5-2-qsfp-dd.page-01h.bin` (128 bytes, all-zero — Advertising).
- `synthetic-cmis-5-2-qsfp-dd.page-02h.bin` (128 bytes, all-zero — Module/Lane Thresholds).
- `synthetic-cmis-5-2-qsfp-dd.page-10h.bin` (128 bytes, all-zero — Banked Lane Control).
- `synthetic-cmis-5-2-qsfp-dd.page-11h.bin` (128 bytes, all-zero — Banked Lane Status).

## Reproducibility

The `.bin` files **and the golden JSON** are emitted by `tools/build-cmis-5-2-synthetic.py`. To regenerate:

```bash
python3 tools/build-cmis-5-2-synthetic.py
python3 -m unittest tools.test_build_cmis_synthetics
```

Each byte's value is documented inline in that script with the corresponding CMIS 5.2 spec section, and the golden's expected semantics are computed from those bytes plus the 5.2 tables (Tables 8-5..8-10, 8-20, 8-30, 8-47, 8-53, 8-76..8-82). The run is deterministic — the authorship stamp is pinned in the script — so re-running reproduces the committed `.bin` pages and `.json` golden byte-for-byte; the unittest pins that reproduction and fails on any drift. If a future revision of CMIS changes a critical-byte location (e.g. PageChecksum scope), update both the script and this provenance file.

## License

Hand-authored by the openxcvr project. No third-party byte sequences. Distributable under the project's license terms.

## Issue tracker

- Plan: _(archived — the `docs/superpowers/` tree was removed)_ task 5.
- Written because no redistributable CMIS 5.2 capture was available.
- Background design doc: _(archived — removed with the `docs/superpowers/` tree)_.
