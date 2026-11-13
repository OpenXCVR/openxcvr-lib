# synthetic-cmis-3-0-qsfp-dd-dp-reserved

> **synthetic: true**
> **vendor_strings_synthetic: true**
> Source: hand-authored from CMIS 3.0 spec (`docs/stds/cmis/CMIS3p0_Third_Party_Spec.pdf`, QSFP-DD MSA, "Common Management Specification Rev 3.0", September 18, 2018). NO bytes copied from any third-party source. This fixture is the reserved-DPSM variant of `synthetic-cmis-3-0-qsfp-dd`: every page except Page 11h is byte-identical to the baseline fixture.

## Module identification

Identical to `synthetic-cmis-3-0-qsfp-dd` (Lower/Page 00h bytes are shared): Lower:0 `0x18` (QSFP-DD), Lower:1 `0x30` (CMIS 3.0), Lower:2 `0x00` (paged memory), synthetic vendor strings, Page 00h checksum `0x5D`.

## What this fixture exercises

**CMIS 3.0 Table 66 reserved Data Path State encodings** — the distinct, independently authored purpose of this variant. Page 11h's first four bytes pack the literal lane vector (lanes 1..8) `0h, 5h, 6h, 7h, 3h, 4h, 0h, 3h` per Table 65 (odd lanes bits 3-0, even lanes bits 7-4) as `50 76 43 30`; the remaining 124 bytes are zero.

CMIS 3.0 § 1.7.7.1 Table 66 defines: 0h **Reserved**, 1h DataPathDeactivated, 2h DataPathInit, 3h DataPathDeinit, 4h DataPathActivated, **5h–Fh Reserved**. The DPTxTurnOn/DPTxTurnOff/DPInitialized meanings for 5h–7h only exist from CMIS 4.0 (Table 8-58) on. A revision-correct CMIS 3.0 decode therefore renders the lane vector as:

| Lane | Raw nibble | Table 66 meaning | Expected `dp_state` |
|------|-----------|------------------|---------------------|
| 1 | 0h | Reserved | `Reserved0` |
| 2 | 5h | Reserved (3.0) | `Unknown5` |
| 3 | 6h | Reserved (3.0) | `Unknown6` |
| 4 | 7h | Reserved (3.0) | `Unknown7` |
| 5 | 3h | DataPathDeinit | `DpDeinit` |
| 6 | 4h | DataPathActivated | `DpActivated` |
| 7 | 0h | Reserved | `Reserved0` |
| 8 | 3h | DataPathDeinit | `DpDeinit` |

Lanes 2/3/4 (raw 5/6/7) are the repair discriminators: a decoder that borrows the unversioned CMIS 4.0+ names renders `DpTxTurnOn`/`DpTxTurnOff`/`DpInitialized` there and is wrong at exactly those lanes; lanes 1/5/6/7/8 (raw 0/3/4) are controls that must stay correct. All lane flags (Tables 67-69) and lane monitors (Table 70) are zero.

The golden `tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd-dp-reserved.json` is authored from the fixture bytes plus the CMIS 3.0 spec tables (never by running a decoder and capturing its output); `tools/build-cmis-3-0-synthetic.py` regenerates both the `.bin` pages and the golden deterministically (`expected_golden(..., reserved_variant=True)`), and `tools/test_build_cmis_synthetics.py` pins that reproduction and the Table 66 label literals. It is a **limited synthetic parity golden, NOT a hardware oracle** — see the `synthetic_note` metadata field for the full classification.

**Nonconformance disclosure (Application advertisement).** The zeroed Application advertisement is an intentional nonconforming fixture, exactly as the baseline: CMIS 3.0 § 1.7.2.7 requires every module to advertise at least one Application and requires FFh in the Host Electrical Code of the first unused Table 26 entry; this fixture does neither. The authored bytes are preserved as-is. The golden's `applications: null` and `decode_status: "ok"` are the structural decode of those bytes — `"ok"` is decode-success, not conformance.

## What this fixture does NOT exercise

- Field-round-trip-stability tests (vendor strings are synthetic placeholders; Pages 01h/02h/10h are all zeros).
- Non-zero lane flags or monitors (Page 11h bytes 4–127 are zero).
- Decode parity against a real CMIS 3.0 module.

## Pages included

User-chosen coverage: **Lower + Pages 00h, 01h, 02h, 10h, 11h** — 6 × 128 bytes = 768 total bytes.

- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.lower.bin` (128 bytes; identical to baseline).
- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.page-00h.bin` (128 bytes; identical to baseline).
- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.page-01h.bin` (128 bytes, all-zero; identical to baseline).
- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.page-02h.bin` (128 bytes, all-zero; identical to baseline).
- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.page-10h.bin` (128 bytes, all-zero; identical to baseline).
- `synthetic-cmis-3-0-qsfp-dd-dp-reserved.page-11h.bin` (128 bytes; `50 76 43 30` + 124 zero bytes).

## Reproducibility

The `.bin` files **and the golden JSON** are emitted by `tools/build-cmis-3-0-synthetic.py` (both stems in one run). To regenerate:

```bash
python3 tools/build-cmis-3-0-synthetic.py
python3 -m unittest tools.test_build_cmis_synthetics -v
```

Authorship stamp: `authored_at: 2026-10-22` (pinned actual authoring date; never the regeneration clock).
