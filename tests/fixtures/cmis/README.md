# CMIS fixtures

End-to-end decode verification for `openxcvr.page.v1` schemas under `schemas/cmis/`. The corpus is keyed by **CMIS specification revision** (Lower memory byte 1, MajorRevision) so that the decoder's rev-byte dispatch is verifiable for every supported branch.

## Source

Thirteen fixtures are extracted directly from [facebook/fboss](https://github.com/facebook/fboss) `fboss/qsfp_service/module/tests/FakeTransceiverImpl.cpp` (BSD-3-Clause; see `ATTRIBUTIONS.md`); a fourteenth and fifteenth are **derived** from one of them by flipping a single byte each (see the extractor note below). fboss publishes static C++ byte-array constants for the test harness it uses to validate its own QSFP service against transceiver fake implementations. Each fixture has:

- a 128-byte Lower memory page,
- one or more 128-byte Upper page slices keyed by hex page ID (Page 00h Administrative, Page 01h Advertising, Page 02h Thresholds, banked Pages 10h–14h, VDM Pages 20h–2Fh, etc.).

The fboss test data is intentionally labeled `kCmis*` and the helper struct is named `Fake*` because **vendor name / vendor PN / S/N / date code are placeholder strings** ("FACETEST", null bytes, or short test patterns) — only the *byte layouts and field placements* are spec-grounded. That's the right shape for a backwards-compat decoder dispatch corpus: we're verifying that MajorRevision-byte branches route correctly, not catching real-vendor quirks.

The `tools/fboss-to-bin.py` extractor walks `FakeTransceiverImpl.cpp`, pairs each `kCmis<NAME>LowerA0` declaration with its proximity-matched `kCmis<MAYBE-REORDERED>UpperPagesA0` map, and emits canonical `<stem>.lower.bin` + `<stem>.page-NNh.bin` slices that match the openxcvr schema topology.

One additional fixture — `fboss-cmis-5-3-osfp-800g-zr-dp-reserved` — is **derived** rather than extracted: it is a byte-for-byte copy of five pages of `fboss-cmis-5-3-osfp-800g-zr` with a single injected byte (`page-11h` byte 0 `0x44`→`0x48`) that drives lane 1's `dp_state` into the reserved `0x8` nibble. It is the regression guard for the Page 11h / Page 16h reserved-nibble decode panic (untrusted-EEPROM DoS); see `fboss-cmis-5-3-osfp-800g-zr-dp-reserved.md`. It stays under the fboss BSD-3-Clause umbrella — the one injected byte is an openxcvr test artifact, not third-party data. A second derived fixture — `fboss-cmis-5-3-osfp-800g-zr-bias-reserved` — flips `page-01h` byte 32 (Page 01h:160) `0x17`→`0x1F` to drive the reserved `0b11` Tx-bias scaling code; see `fboss-cmis-5-3-osfp-800g-zr-bias-reserved.md`.

## Fixture table

| Stem | Source ID | CMIS rev | Form factor | Mem model | Notable | Pages |
|------|-----------|----------|-------------|-----------|---------|-------|
| `fboss-cmis-4-0-qsfp112-200g-fr4` | `kCmis200GFr4` | 4.0 | QSFP112 (0x1E) | paged | 200G FR4 — non-QSFP-DD form factor at 4.0 era | 6 (Lower + 00h, 01h, 02h, 10h, 11h, 13h) |
| `fboss-cmis-4-0-osfp-2x400g-dr4` | `kCmis400G2xDr4` | 4.0 | OSFP (0x19) | paged | Dual-port 2×400G DR4 (8-lane data-path-mode) | 13 |
| `fboss-cmis-4-0-qsfpdd-400g-dr4` | `kCmis400GDr4` | 4.0 | QSFP-DD (0x18) | paged | Vanilla 400G DR4 | 13 |
| `fboss-cmis-4-0-qsfpdd-400g-fr4-multiport` | `kCmis400GFr4MultiPort` | 4.0 | QSFP-DD | paged | MultiPort advertisement (Page 01h ApplicationDescriptor table) | 13 |
| `fboss-cmis-4-0-qsfpdd-400g-lr4` | `kCmis400GLr4` | 4.0 | QSFP-DD | paged | LR4 media interface (alternate VDM page set) | 13 |
| `fboss-cmis-4-0-qsfpdd-flatmem` | `kCmisFlatMem` | 4.0 | QSFP-DD | **flat** | FlatMem=1 path (Lower:2 bit 7 set) — important regression target | 2 (Lower + 00h only) |
| `fboss-cmis-5-0-osfp-2x400g-fr4` | `kCmis400G2xFr4` | 5.0 | OSFP | paged | First CMIS 5.0 fixture; introduces VDM Page 22h and Pages 26h/2Ch | 16 |
| `fboss-cmis-5-1-osfp-2x400g-fr4-lite` | `kCmis400G2xFr4Lite` | 5.1 | OSFP | paged | "Lite" variant; date_code=250112 (real-looking) | 16 |
| `fboss-cmis-5-1-osfp-2x400g-fr4-lpo` | `kCmis400G2xFr4Lpo` | 5.1 | OSFP | paged | **LPO (linear-pluggable optic, no DSP)** — vendor=FACETESTLPO, OUI=70:EE:A3, S/N=UL244280002, date=240301. Exercises LPO tags. | 16 |
| `fboss-cmis-5-1-osfp-2x800g-dr4` | `kCmis800G2xDr4` | 5.1 | OSFP | paged | 2×800G DR4 dual-rate | 13 |
| `fboss-cmis-5-1-osfp-credo-800g-aec` | `kCmisCredo800AEC` | 5.1 | OSFP | paged | **AEC (active electrical cable)** — vendor=Credo (OUI 9A:AD:CA), PN=CAC815321A1AC1HW, date=240424. Exercises copper-media reach kind. | 14 |
| `fboss-cmis-5-3-osfp-800g-zr` | `kCmis800GZr` | **5.3** | OSFP | paged | **800G ZR coherent** — vendor=METAZR (Meta-branded, OUI 7C:B2:5C), PN=DP08SFP8-ZRB-29B, S/N=251356640, date=250331. **22 pages** including VDM groups 20h–27h, 2Ch flags, 2Fh advertisement, plus C-CMIS pages 34h/35h/38h/45h. **Highest-fidelity fixture in the corpus.** | 19 |
| `fboss-cmis-5-3-osfp-800g-zr-dp-reserved` | `kCmis800GZr` +1B | **5.3** | OSFP | paged | **Reserved dp_state-nibble regression** — `page-11h` byte 0 `0x44`→`0x48` forces lane 1 `dp_state` into the `reserved_8_f` range (CMIS 5.3 § 8.10.1 / Table 8-84). All five bindings must render `"Unknown8"` (never panic) — the Page 11h/16h untrusted-EEPROM DoS guard. Derived from the ZR fixture. | 5 (Lower + 00h/01h/02h/11h) |
| `fboss-cmis-5-3-osfp-800g-zr-bias-reserved` | `kCmis800GZr` +1B | **5.3** | OSFP | paged | **Reserved Tx-bias-scaling regression** — `page-01h` byte 32 (Page 01h:160) `0x17`→`0x1F` moves the LaserBiasTx multiplier code to the reserved `0b11` (CMIS 5.3 § 8.4.5 / Table 8-52). All five bindings must render `tx_bias_ma: null` on every lane while the rest of the lane block stays intact. Derived from the ZR fixture. | 5 (Lower + 00h/01h/02h/11h) |
| `fboss-cmis-anomalous-qsfpdd-400g-cr8` | `kCmis400GCr8` | **0.5 (anomalous)** | QSFP-DD | flat | Rev byte `0x05` does not correspond to any released CMIS revision. Useful for "decoder rejects garbage rev" regression test. **Do not use as a baseline rev-4.0 fixture.** | 2 |
| `synthetic-cmis-3-0-qsfp-dd` | (hand-authored) | **3.0** (synthetic) | QSFP-DD (0x18) | paged | Hand-authored from the QSFP-DD MSA "CMIS Rev 3.0" PDF (2018-09-18). Lower:1=`0x30`. Vendor strings are explicit synthetic markers (`CMIS_3_0_SYNTH`). Carries a limited synthetic parity golden (`tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd.json`) — see `synthetic-cmis-3-0-qsfp-dd.md`. | 6 (Lower + 00h/01h/02h/10h/11h) |
| `synthetic-cmis-3-0-qsfp-dd-dp-reserved` | (hand-authored) | **3.0** (synthetic) | QSFP-DD (0x18) | paged | **Reserved-DPSM variant of `synthetic-cmis-3-0-qsfp-dd`**: Page 11h packs the literal CMIS 3.0 Table 66 lane vector `0h,5h,6h,7h,3h,4h,0h,3h` (`50 76 43 30`); every other page is byte-identical to the baseline. A revision-correct 3.0 decode renders lanes 2/3/4 (raw 5h/6h/7h, Reserved in 3.0) as `Unknown5`/`Unknown6`/`Unknown7` while the 0h/3h/4h controls stay `Reserved0`/`DpDeinit`/`DpActivated` — the guard against borrowing the unversioned CMIS 4.0+ Table 8-58 names. Carries a limited synthetic parity golden (`tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd-dp-reserved.json`) — see `synthetic-cmis-3-0-qsfp-dd-dp-reserved.md`. | 6 (Lower + 00h/01h/02h/10h/11h) |
| `synthetic-cmis-5-4-qsfp-dd-vdm` | (hand-authored) | **5.4** (synthetic) | QSFP-DD (0x18) | paged | Hand-authored from the OIF-CMIS-05.4 PDF (2025). Lower:1=`0x54`. Vendor strings are explicit synthetic markers (`CMIS_5_4_VDM_SYNTH`). **First fixture carrying the complete VDM page set**: all four descriptor/sample groups (20h–27h), the supervision threshold quads 28h–2Bh, TC flags 2Ch, supervision masks 2Dh (none of which existed anywhere in the corpus before) and the 2Fh advertisement incl. the 5.4 duty-cycle registers. Carries a synthetic VDM parity golden (`tests/golden/cmis/synthetic-cmis-5-4-qsfp-dd-vdm.json`) — see `synthetic-cmis-5-4-qsfp-dd-vdm.md`. | 21 (Lower + 00h/01h/02h/10h/11h + 20h–2Bh/2Ch/2Dh/2Fh) |
| `synthetic-cmis-5-2-qsfp-dd` | (hand-authored) | **5.2** (synthetic) | QSFP-DD (0x18) | paged | Hand-authored from the OIF-CMIS-05.2 PDF (2022-04-27). Lower:1=`0x52`. Vendor strings are explicit synthetic markers (`CMIS_5_2_SYNTH`). Carries a limited synthetic parity golden (`tests/golden/cmis/synthetic-cmis-5-2-qsfp-dd.json`) — see `synthetic-cmis-5-2-qsfp-dd.md`. | 6 (Lower + 00h/01h/02h/10h/11h) |

Total: 19 fixtures, 206 `.bin` files (inventory count, re-derive with `ls tests/fixtures/cmis/*.bin | wc -l`). Coverage: CMIS 3.0 (2 synthetic), 4.0 (6 fboss), 5.0 (1 fboss), 5.1 (4 fboss), 5.2 (1 synthetic), 5.3 (1 fboss + 2 fboss-derived), 5.4 (1 synthetic), anomalous (1 fboss). All 19 fixtures have paired goldens under `tests/golden/cmis/` (18 decode-form + 1 rejection-form).

## Anonymization status

fboss intentionally uses synthetic vendor identity strings to avoid leaking real-customer S/N and date codes. The handful of fixtures with non-placeholder identity strings (Lpo, Credo AEC, ZR) appear to be vendor-supplied test patterns — Facebook either obtained explicit permission or these are synthesized-but-realistic strings the vendors agreed to ship. No further anonymization required for downstream redistribution under BSD-3-Clause.

## Caveats

- **Vendor strings are synthetic** for most fixtures (FACETEST or null bytes). Don't use these to validate vendor-PN-driven dispatch logic — that needs real-vendor fixtures (deferred to vendor-permission and SONiC/OPTOE field collection campaigns).
- **Live monitor values are stale or zeroed** for many fixtures. Real-time DOM verification needs hardware-captured fixtures.
- **VDM threshold values** in the fboss fixtures are placeholder patterns; `synthetic-cmis-5-4-qsfp-dd-vdm` now carries the spec-authored threshold/mask/flag pages (28h–2Bh, 2Ch, 2Dh) and a VDM parity golden, but hardware-captured fixtures remain the real correctness oracle.
- **kCmis400GCr8 has rev byte `0x05`** — not a real CMIS rev. Inspect, but don't use as baseline.

## Synthetic fixtures (CMIS 3.0, CMIS 5.2, CMIS 5.4)

The two `synthetic-cmis-*-qsfp-dd` fixtures fill rev-byte branches that no permissively-licensed real-vendor source could supply. The project's fixture-license boundary forbids byte-copying from EPL-1.0 sources, which rules out the only known third-party CMIS 3.0 fixture (`floodlight/bigcode` `modules/sff/module/src/sff_db.c`); CMIS 5.2 has no known public fixture source at all. Hand-authoring from the upstream spec PDFs sidesteps both gaps cleanly.

**Build provenance.** Each synthetic fixture — **and its golden** — is emitted by a dedicated builder script with inline spec citations per byte:

- `tools/build-cmis-3-0-synthetic.py` → `synthetic-cmis-3-0-qsfp-dd.{lower,page-NNh}.bin` + `tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd.json` (CMIS 3.0 § 1.7.2 / § 1.7.3 sourced), and the reserved-DPSM variant `synthetic-cmis-3-0-qsfp-dd-dp-reserved.{lower,page-NNh}.bin` + `tests/golden/cmis/synthetic-cmis-3-0-qsfp-dd-dp-reserved.json` in the same run — its Page 11h carries the literal Table 66 lane vector `0h,5h,6h,7h,3h,4h,0h,3h` (`50 76 43 30`) so a 3.0 decode must render `Unknown5`/`Unknown6`/`Unknown7` on lanes 2/3/4 (5h–7h Reserved in 3.0) with the 0h/3h/4h controls named.
- `tools/build-cmis-5-2-synthetic.py` → `synthetic-cmis-5-2-qsfp-dd.{lower,page-NNh}.bin` + `tests/golden/cmis/synthetic-cmis-5-2-qsfp-dd.json` (CMIS 5.2 § 8.2 / § 8.3 sourced).
- `tools/build-cmis-5-4-synthetic.py` → `synthetic-cmis-5-4-qsfp-dd-vdm.{lower,page-NNh}.bin` + `tests/golden/cmis/synthetic-cmis-5-4-qsfp-dd-vdm.json` (CMIS 5.4 § 8.2 / § 8.3 / § 8.27 sourced) — the complete VDM page set with the U16/S16/F16 sample dispatch, threshold quads and TC flag/mask nibble packing.

Run either script to regenerate the corresponding `.bin` files and goldens; the run is deterministic (pinned authorship stamps — each stem carries its own), and `tools/test_build_cmis_synthetics.py` pins byte-for-byte reproduction of all of them. Each `.md` provenance file flags `synthetic: true` + `vendor_strings_synthetic: true`, and each golden's `_metadata` carries `synthetic: true`, `monitoring_stale: true`, `vendor_strings_synthetic: true` plus a `synthetic_note` classifying it as a limited synthetic parity golden, not a hardware oracle.

**What they're useful for.** Decoder rev-byte dispatch tests — the decoder must observe `0x30` / `0x52` in `Lower:1` and route into the corresponding spec branch. Page 00h carries spec-grounded identity-string layout + a correctly-computed Page Checksum. Pages 01h/02h/10h/11h are all-zero. The goldens additionally pin the cross-binding decode of those zero bytes: `dp_state: "Reserved0"` on every lane (0h is Reserved in CMIS 3.0 Table 66 and CMIS 5.2 Table 8-77 — **not** Deactivated, which is 1h), `module_state: "Reserved0"` (000b Reserved), `interrupt_asserted: true` (byte 3 bit 0 = 0b asserted), `applications: null` (no AppSel advertised anywhere), `0.0` monitors, and all-clear lane flags.

**What they're NOT useful for.** Field-round-trip-stability tests, vendor-PN-driven dispatch tests, real-time monitor decode parity, hardware-oracle comparisons. Vendor strings are the explicit synthetic markers `CMIS_3_0_SYNTH` / `CMIS_5_2_SYNTH` so a hex-dump audit immediately classifies these as non-real captures. A strict decoder may consider the all-zero advertisement underspecified (both specs require at least one Application) — the golden records the decode of the bytes as authored, and the classification lives in `_metadata`.

## Open backwards-compat gaps

Known gaps, and how each was closed:

- ~~**CMIS 3.0** — exists in `floodlight/bigcode` but EPL-1.0 (incompatible with the project's fixture-license boundary). Synthetic fallback queued.~~ → **Closed by `synthetic-cmis-3-0-qsfp-dd`** (commit `0bdc232`).
- ~~**CMIS 5.2** — no public byte fixtures found anywhere. Synthetic fallback queued.~~ → **Closed by `synthetic-cmis-5-2-qsfp-dd`** (commit `a9af59d`).
- **CMIS 5.3 with full real-vendor identity** — the METAZR ZR fixture is the closest we have; complementary real-vendor 5.3 dumps (DR4, FR4, LR4) need a vendor-permission or SONiC-field-capture campaign.
- ~~**Decoder dispatch test wiring** — `crates/openxcvr-cmis/tests/dispatch_by_rev.rs` cannot land on `main` until PR #189 merges.~~ → **Closed**: PR #189 merged; `dispatch_by_rev.rs` ships 6 tests covering CMIS 3.0/4.0/5.0/5.1/5.2/5.3.

## Index

(See the fixture table above. Per-fixture provenance is consolidated here for fboss-derived fixtures because fboss's BSD-3-Clause source is uniform across those 13 entries; the two synthetic fixtures and the derived `fboss-cmis-5-3-osfp-800g-zr-dp-reserved` fixture have dedicated `.md` provenance files alongside their `.bin` slices. See `ATTRIBUTIONS.md` for the full fboss BSD-3-Clause notice.)
