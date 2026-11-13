# Golden JSON corpus

End-to-end decode contracts for every fixture in [`tests/fixtures/`](../fixtures/). One `.json` per `.bin` fixture, paired by name. These goldens are the **cross-binding parity contract** — the Rust reference decoder, the Phase 2 Python emitter, and any future binding must produce byte-identical output for every fixture.

Goldens complement schema-side `examples:` / `assertions:`. Examples are spec-derived unit tests inside the schema YAML; goldens here are hardware-captured (or hardware-shaped) integration tests.

## Schema kind

Every golden file declares `"_metadata.schema_version": "openxcvr.golden.v1"`.

The shape is fixed (alphabetically ordered keys for stable diffs). Optional sections may be omitted when the underlying memory pages are absent (flatmem fixtures) or when the source data is synthetic / stale (fboss CMIS fixtures with placeholder vendor strings or zeroed monitoring).

## File layout

```
tests/golden/
├── cmis/         <- paired with tests/fixtures/cmis/*.bin
│   ├── fboss-cmis-{rev}-{form}-{rate}.json
│   └── synthetic-cmis-{rev}-qsfp-dd.json   <- limited synthetic parity goldens (builder-emitted)
├── sff-8472/     <- paired with tests/fixtures/sff-8472/*.{a0h,a2h}.bin
│   └── {vendor}-{pn}-{sn}.json
└── sff-8636/     <- paired with tests/fixtures/sff-8636/*.{lower,page-NNh}.bin
    └── {vendor}-{pn}-{rate}.json
```

The golden file's basename matches the fixture stem (drop `.lower.bin` / `.a0h.bin` / `.page-NNh.bin` etc.).

Three CMIS goldens are **builder-emitted rather than hand-authored**: `synthetic-cmis-3-0-qsfp-dd.json`, `synthetic-cmis-3-0-qsfp-dd-dp-reserved.json` and `synthetic-cmis-5-2-qsfp-dd.json` are regenerated, byte-for-byte deterministically, by `tools/build-cmis-3-0-synthetic.py` (both 3.0 stems in one run) / `tools/build-cmis-5-2-synthetic.py` alongside their `.bin` fixtures (`tools/test_build_cmis_synthetics.py` pins the reproduction). Their expected semantics are authored from the fixture bytes plus the CMIS 3.0 / CMIS 5.2 spec tables — never by running a decoder and capturing its output — so they are independent contracts for the decoders under test, not records of decoder behavior. They are **limited synthetic parity goldens, NOT hardware oracles**; see the synthetic row in the edge-case table below and the `synthetic_note` metadata field.

## Top-level shape

```json
{
  "_metadata": {
    "authored_at": "YYYY-MM-DD",
    "authored_by": "<name + tooling>",
    "cmis_version": "5.3" | null,            <- CMIS only
    "fixture": "<basename matching tests/fixtures/>",
    "monitoring_stale": false | true,         <- true ⇒ live monitors are zeroed/placeholder
    "schema_version": "openxcvr.golden.v1",
    "spec": "cmis-5.3" | "sff-8472" | "sff-8636",
    "spec_revision": "5.3" | "12.5a" | "2.11",
    "vendor_strings_synthetic": false | true  <- true ⇒ vendor name/PN/SN are placeholders
  },
  "decode_status": "ok" | "rejected",
  "rejection": {                              <- only when decode_status == "rejected"
    "reason": "<short machine-readable reason>",
    "detail": "<human-readable explanation>"
  },

  "applications": [ <ApplicationDescriptor>… ] | null,    <- CMIS Page 01h app-descriptor table
  "checksums": <Checksums> | null,
  "compliance": <ComplianceCodes> | null,                  <- SFF-8472 / SFF-8636 compliance-bit decode
  "flags": <FlagSummary> | null,
  "identity": <Identity>,
  "lanes": [ <Lane>… ] | null,
  "module": <Module>,
  "monitoring": <Monitoring> | null
}
```

`_metadata` may carry additional classification keys: `derived_from` + `derived_note` for byte-flip-derived fixtures (see `fboss-cmis-5-3-osfp-800g-zr-dp-reserved.json`), `synthetic` + `synthetic_note` for the hand-authored synthetic fixtures, and `corrected_at` + `correction_note` for goldens corrected on spec evidence. `cmis_version` is the **device-reported** revision (decoded from Lower:1); `spec` / `spec_revision` name the **canonical schema namespace** the decoders are emitted from. For the fboss fixtures the two coincide; for the synthetic CMIS 3.0 / 5.2 fixtures they legitimately differ (device 3.0 / 5.2 vs canonical `cmis-5.3`) — the `synthetic_note` states which fields were verified to agree between the device revision's spec tables and the canonical schema, so the split is explicit rather than silent.

`applications` and `compliance` are mutually exclusive: CMIS modules use `applications` only; SFF-8472 / SFF-8636 modules use `compliance` only. They model overlapping concepts (link-type capability advertising) but the byte layouts and structural assumptions differ enough that a single shape would either lose information or carry unused fields. Cross-binding parity is enforced separately within each spec; the per-spec sections decode the spec-specific bytes.

**Per-spec compliance shape divergence** (intentional, not a bug):

- **SFF-8472** `compliance` carries only `raw_bytes_a0h_3_10` (the 8 raw bytes), `reach`, and `wavelength_nm`. There is no `bits_set` array because the SFF-8472 schema models the entire region as a single collapsed-array field `transceiver_compliance` indexed by bit-position 0–63, with no per-bit field names.
- **SFF-8636** `compliance` additionally carries `bits_set` (per-bit schema field names like `compliance_40gbase_sr4`, `compliance_extended`), `device_technology`, and (when bit 7 of byte 3 is set) `ext_spec_byte_192`. The SFF-8636 schema names each compliance bit as its own field, so the `bits_set` array is well-defined.

### Enum-name source rule

**Every `*.name` value in a golden must be byte-identical to the corresponding entry's `name` field in `schemas/<spec>/[enums/]*.yaml`.** Author-invented prose (e.g., `"LC"`, `"10GBASE-SR"`, `"850 nm VCSEL"`) breaks cross-binding parity by construction — a fresh emitter pulling from the schema enum produces the schema's canonical name (`"lc"`, `"sixty_four_b_sixty_six_b_or_legacy_manchester"`, `"nm850_vcsel"`), not the prose. When in doubt, drop `name` and keep only `raw`; a decoder can always look up the name at decode time.

Exception: `module.module_type` is a vendor-PN-derived label (not a byte-decode), and `_metadata` / `vendor_*` fields are metadata. These are not subject to the enum-name source rule.

- All sections except `_metadata`, `decode_status`, `identity`, and `module` are nullable. A `null` (or omitted) section usually means "the underlying memory page is not present in this fixture" (e.g., flatmem CMIS modules have only Lower + Page 00h, so `applications`, `lanes`, `flags` are all `null"). For `applications` there is a second null case: a **paged module whose advertisement is empty** (every AppSel descriptor slot unadvertised) — see Step 4.
- `decode_status: "rejected"` golden form: only `_metadata` + `decode_status` + `rejection` are present. Used for fixtures whose raison d'être is the rejection path (e.g., `fboss-cmis-anomalous-qsfpdd-400g-cr8` has rev byte `0x05`, not a real CMIS revision — golden encodes the "decoder rejects garbage rev" contract).

### `identity` shape

```json
{
  "date_code": "YYMMDD",            <- ASCII 6-byte string from spec table
  "vendor_name": "<trimmed ASCII>", <- trailing spaces stripped
  "vendor_oui": "AA:BB:CC",         <- canonical hex with colon separators
  "vendor_pn": "<trimmed ASCII>",
  "vendor_sn": "<trimmed ASCII>"
}
```

### `module` shape

Logical-fact summary of the module's static character. Always present.

```json
{
  "cmis_revision": "5.3",                 <- CMIS only
  "connector": { "name": "LC", "raw": "0x07" },
  "encoding": { "name": "64B/66B", "raw": "0x06" },
  "host_interface": "<media interface code name>",   <- when known
  "identifier": { "name": "OSFP", "raw": "0x19" },
  "media_interface": "<host interface code name>",   <- when known
  "memory_model": "paged" | "flat",       <- CMIS / SFF-8636
  "module_state": "ModuleReady" | …,      <- CMIS only; SFF-8472/8636 omit
  "module_type": "<vendor-PN-derived label>",
  "nominal_bit_rate_gbps": <number>
}
```

### `monitoring` shape

Live module-level real-time monitors. Per-lane monitors live on `lanes[]`. Units are encoded in the value path:

```json
{
  "module_temperature_c": 32.5,
  "module_voltage_v": 3.31,
  "tec_current_ma": null,            <- omit (or null) for non-applicable monitors
  …
}
```

Convention: every monitor field name ends with the unit suffix (`_c`, `_v`, `_ma`, `_dbm`, `_uw`). This makes the unit explicit without a sibling `unit:` key.

### `lanes` shape

Per-lane status array. Index is **1-based** (matches CMIS lane numbering and collapsed-array `instances:`).

```json
[
  { "index": 1, "tx_bias_ma": 50.2, "tx_power_dbm": -2.1, "rx_power_dbm": -3.5,
    "tx_state": "TxStateActive", "rx_los": false, "tx_fault": false },
  …
]
```

### `applications` shape

AppSel 1-8 from Lower 86-117 then AppSel 9-15 from Page 01h 223-250, additive in every CMIS revision, with app_code = AppSel (or SFF-8636 compliance bytes). Each entry decodes one advertised application:

```json
[
  { "app_code": 1,
    "host_interface_id": 33, "host_interface_name": "800GAUI-8 C2M (Annex 168E)",
    "host_lane_count": 8,
    "media_interface_id": 224, "media_interface_name": "800ZR",
    "media_lane_count": 8,
    "host_lane_assignment_options": "0xFF" }
]
```

### `flags` shape

Top-level flag/alarm summary. Sub-objects group related flags by source page:

```json
{
  "module_alarm_summary": {
    "temp_high_alarm": false, "temp_low_alarm": false,
    "vcc_high_alarm": false, "vcc_low_alarm": false
  },
  "data_path_state_changed": false
}
```

### `checksums` shape

For SFF-8472 and SFF-8636 only (CMIS has no identity-byte checksums per spec). Each entry decodes the stored byte and the computed value to flag tampering or corruption:

```json
{
  "cc_base":  { "stored": "0xFE", "computed": "0xFE", "valid": true },
  "cc_ext":   { "stored": "0x88", "computed": "0x88", "valid": true },
  "cc_dmi":   { "stored": "0x4E", "computed": "0x4E", "valid": true },  <- SFF-8472 only
  "cc_calib": { "stored": null,   "computed": null,   "valid": null }   <- SFF-8472 Page 03h, optional
}
```

CC_BASE covers the base-ID region and CC_EXT covers the extended region (vendor serial and date code). SFF-8472 stores CC_BASE at A0h byte 63 (sum of bytes 0–62) and CC_EXT at byte 95. SFF-8636 stores CC_BASE at page 00h byte 63 (sum of bytes 0–62). The checksum field in the schema is the range to recompute; do not invent a second range.

## Authoring workflow

Hand-decode budget is real — the corpus is 28 goldens (18 CMIS + 5 SFF-8472 + 5 SFF-8636; re-derive with `ls tests/golden/<spec>/*.json | wc -l` rather than trusting this number), of which the three synthetic CMIS goldens are builder-emitted instead. Follow this workflow to avoid transcription drift.

### Step 1 — pull formulas from `schemas/`, NOT from a fresh PDF read

Every decode formula is already in the schema YAML (offset, type, encoding, scaling formula, enum reference). Treat `schemas/<spec>/*.yaml` as the single source of truth. The PDF is for resolving ambiguities in the schema, not for the primary decode path. This eliminates the class of bug where two goldens decode the same field differently because two authors transcribed the spec independently.

### Step 2 — start with existing provenance `.md` if present

For SFF-8472 + SFF-8636 fixtures (OCP OOM bootstrap), the `.md` provenance files in `tests/fixtures/<spec>/` already hand-decode the universal facts (identifier, vendor strings, connector, encoding, bit rate, calibration variants, real-time monitors). Convert those tables to the JSON shape; they cover ~80% of the golden's content.

For fboss CMIS fixtures, only consolidated provenance exists in `tests/fixtures/cmis/README.md`. Per-fixture facts (form factor, real-vendor identity for ZR / Credo / LPO, page coverage) come from that table.

### Step 3 — checksum verification

For SFF-8472 and SFF-8636, recompute CC_BASE / CC_EXT (and CC_DMI / CC_CALIB where applicable) over the byte range the schema's checksum field names, and compare that sum to the stored byte. Do not invent a second range.

### Step 4 — handle the edge-case categories

| Category | Fixtures | Golden treatment |
|---|---|---|
| Synthetic vendor strings | most fboss CMIS | `_metadata.vendor_strings_synthetic: true`. Decode the strings as-is (`"FACETEST"`, etc.) — the contract is "what does the decoder produce," not "what does the module say it is." |
| Stale/zeroed monitoring | most fboss CMIS | `_metadata.monitoring_stale: true`. Decode the values as-is (mostly `0.0`) — exercises the formula path, not the value semantics. |
| Flatmem (only Lower + Page 00h) | `fboss-cmis-4-0-qsfpdd-flatmem` | Set `module.memory_model: "flat"`. Set `applications`, `lanes`, `flags` to `null`. Keep `identity`, `module`, `monitoring` populated from the two pages that ARE present. |
| Anomalous rev byte | `fboss-cmis-anomalous-qsfpdd-400g-cr8` | `decode_status: "rejected"`. Only `_metadata`, `decode_status`, `rejection` present. `rejection.reason: "unknown_major_revision: 0x05"`. |
| Hand-authored synthetic (CMIS 3.0 / 5.2) | `synthetic-cmis-3-0-qsfp-dd`, `synthetic-cmis-5-2-qsfp-dd` | `_metadata.synthetic: true` + `synthetic_note` (classification + device-revision-vs-canonical-schema statement) + `monitoring_stale: true` + `vendor_strings_synthetic: true`. Builder-emitted (see above); zero pages decode honestly: `dp_state: "Reserved0"` on every lane (0h is Reserved in CMIS 3.0 Table 66 and CMIS 5.2 Table 8-77, not Deactivated), `module_state: "Reserved0"`, `interrupt_asserted: true`, `applications: null`. NOT hardware oracles. |

**Paged module with an empty application advertisement** (first exercised by the synthetic fixtures): when every AppSel descriptor slot is unadvertised — Lower 86-117 all zero (CMIS 3.0 Table 26 / CMIS 5.2 Table 8-20) and Page 01h 223-250 all zero (Table 48 / Table 8-53) — `applications` renders `null`, the cross-binding rendering of "no entries" (every aggregator emits null for an empty advertisement; never an empty array, never an invented descriptor). This is distinct from the flatmem case: the pages ARE present, they just advertise nothing.

### Step 5 — alphabetic key ordering

Keys at every nesting level sort alphabetically. This is a stability rule for diff-based tests — adding a new field never re-sorts the entire file, and CI parity diffs stay readable.

### Step 6 — numeric format

| Kind | Format |
|---|---|
| Real-valued telemetry / thresholds | Decimal (`32.5`, not `0x4080`) |
| Opcodes / register bytes / OUIs | Hex with `0x` prefix or canonical colon-separated (`"0x07"`, `"7C:B2:5C"`) |
| Lane indices / app codes | Decimal integers (`1`, not `"01"`) |
| Module state / enum names | UpperCamelCase ASCII strings (`"ModuleReady"`, `"TxStateActive"`) |

### Step 7 — numeric precision: write to LSB granularity, no FP noise

Every real-valued field encodes to the **exact LSB-granularity result** of the spec formula. The convention:

1. Compute `raw_int × LSB_decimal` where `LSB_decimal` is the spec's LSB factor written as a decimal (e.g., `0.0001` for 100 µV voltage, `0.002` for 2 µA bias).
2. Write the result with **trailing zeros up to the LSB's decimal precision**. If LSB = 0.0001, write 4 decimal places; if LSB = 1/256 (= 0.00390625), write the exact 8-place result.
3. **Avoid binary-float-roundoff noise.** Python computes `33162 × 0.0001 = 3.3162000000000003` because 0.0001 isn't exactly representable in IEEE-754; in the JSON, write `3.3162`, not `3.3162000000000003`. Use integer-arithmetic with explicit decimal point placement, OR `f"{val:.4f}"`-style formatting.

Examples:
- s8.8 temperature `0x0A 0x1A` → `2586 / 256` → `10.1015625` (LSB = 1/256 → exact-result decimal length). Write `10.1015625`.
- 0.1 µW Rx power `0x16D6` → `5846 × 0.0001` → write `0.5846` (4 dp matching LSB), **not** `0.5846000000000001`.
- s8.8 `0x4200` → `16896 / 256` → `66.0`. Write `66.0` (the trailing `.0` marks the field as float, not int).

For derived values (dBm = 10·log₁₀(mW), %used etc.): these depend on language-specific log10 implementations. Goldens should encode the **raw LSB-granularity primary unit** (mW, mA, V); dBm and other derivations are computed at display time and **not stored in the golden** to avoid cross-binding-parity divergence on the last few digits of `log10`.

## Validation

`tools/validate-golden-schema.py` validates every `tests/golden/cmis/*.json` (skipping `_`-prefixed flag files) against the `CmisDecodeResult` schema in `bindings/openapi/openxcvr.openapi.yaml`, after popping `_metadata`. `jq -e . tests/golden/<spec>/*.json` remains the cheap JSON-syntax smoke test. The three builder-emitted synthetic goldens are additionally pinned byte-for-byte by `python3 -m unittest tools.test_build_cmis_synthetics`, which fails on any drift between the committed artifacts and the builders' deterministic output. Still forthcoming: a checker for `.bin` ↔ `.json` pairing and edge-case bookkeeping consistency across the whole corpus.

## Cross-binding parity

The parity runners that make these goldens load-bearing (each auto-discovers goldens or fixtures, so a new golden takes effect the moment it lands):

- **Go** — `bindings/go/parity_test.go` enumerates `tests/golden/<spec>/*.json` (skipping `_`-prefixed files), decodes the paired fixture with `DecodeCmis`/`DecodeSff8636`/`DecodeSff8472`, and diffs against the golden (skipping `_metadata` and `module_type` for CMIS).
- **C** — `bindings/c/tests/parity.c` enumerates `tests/fixtures/cmis/*.lower.bin`, decodes each stem that has a golden, and compares via `bindings/c/tests/compare_json.py` (fixtures without a golden are skipped — a golden therefore *activates* a fixture here).
- **Python** — `bindings/python/tests/test_parity.py` (cases collected in `tests/conftest.py` from the golden dirs) decodes with `openxcvr.decode_cmis` / `decode_sff8472` / `decode_sff8636`.
- **PHP** — `bindings/php/tests/ParityTest.php` globs the golden dirs per spec.
- **Rust** — the reference decoder is asserted at field level in `crates/openxcvr-cmis/tests/golden.rs` (`lanes`/`flags` against the golden JSON, per-fixture test functions), complementing the JSON-comparing runners.
- **TypeScript** — golden-fixture parity is not wired yet (`bindings/typescript/tests/decode.test.mjs` decodes a single hardcoded fixture); it lands in a later wave.

Every runner fails on any divergence. This is the contract that makes goldens load-bearing rather than just illustrative.

## Cross-references

- [`tests/fixtures/`](../fixtures/) — paired `.bin` + `.md` source notes
- Schema checksum fields — the byte range a golden's `cc_*` object must recompute
