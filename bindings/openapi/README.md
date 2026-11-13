# bindings/openapi — OpenAPI 3.1 / JSON Schema Binding

This binding produces two artifacts:

| Artifact | Description |
|---|---|
| `openxcvr.openapi.yaml` | Hand-authored OpenAPI 3.1 document describing the JSON output of the aggregate decode functions, listed per family by the entry points that exist on this tree: `decode_cmis()` (Python, Go, PHP, C, TypeScript, Java, C#) → `CmisDecodeResult`; `decode_sff8636()` / `decode_sff8472()` (Python, Go, PHP, C, TypeScript, Java, C#) → `Sff8636DecodeResult` / `Sff8472DecodeResult`. |
| `generated/` | Machine-generated JSON Schema files (one per page, one per enum, across CMIS, SFF-8636, and SFF-8472), emitted by `openxcvr-codegen --lang openapi`. |

## Regenerate the generated schemas

```sh
tools/regen-openapi.sh             # all specs (cmis, sff-8636, sff-8472)
tools/regen-openapi.sh cmis        # one spec
```

The script runs `cargo run -p openxcvr-codegen -- generate --spec <spec> --lang openapi --out bindings/openapi/generated` per spec.

Expected after a full regen: one schema per page in `generated/pages/` and one per enum in `generated/enums/`, across CMIS, SFF-8636, and SFF-8472 (SFF-8024 enums are shared into each family that imports them). The generated directories themselves are the current inventory — no total count is pinned here, so it cannot drift as schemas are added.

## Validate golden fixtures against the decode output schema

```sh
python3 tools/validate-golden-schema.py
```

This validates every discovered golden fixture in `tests/golden/cmis/`, `tests/golden/sff-8636/` and `tests/golden/sff-8472/` (currently 18 + 5 + 5) against the `CmisDecodeResult`, `Sff8636DecodeResult` and `Sff8472DecodeResult` components in `openxcvr.openapi.yaml`, skipping `_`-prefixed files. Its probe-set self-test is `python3 -m unittest tools.test_validate_golden_schema`.

Dependencies: `pip install "jsonschema[format-filter]>=4.18" pyyaml`

## Schema structure

### Decode output schema (`openxcvr.openapi.yaml`)

```
CmisDecodeResult                      (discriminated union on decode_status)
  ├── CmisDecodeOk                    (decode_status: const "ok")
  │   ├── identity (CmisIdentity)
  │   │   └── vendor_name, vendor_oui, vendor_pn, vendor_sn, vendor_rev, date_code
  │   ├── module (CmisModule)
  │   │   └── cmis_revision, identifier, connector, memory_model, module_state, module_type
  │   ├── monitoring (CmisMonitoring)
  │   │   └── module_temperature_c, module_voltage_v
  │   ├── applications: null | [ CmisApplication, ... ]
  │   └── checksums, lanes, flags: null (v0.1)
  └── CmisDecodeRejected              (decode_status: const "rejected")
      └── rejection (CmisRejection)
          └── reason, detail

Sff8636DecodeResult                   (ok-only: const "ok")
  ├── identity (Sff8636Identity)      ├── flags (Sff8636Flags)
  ├── module (Sff8636Module)          ├── lanes: [ Sff8636Lane ]
  ├── monitoring (Sff8636Monitoring)  └── checksums: cc_base, cc_ext
  └── compliance (Sff8636Compliance)

Sff8472DecodeResult                   (ok-only: const "ok")
  ├── identity (Sff8472Identity)      ├── flags: null | Sff8472Flags
  ├── module (Sff8472Module)          ├── lanes: null | [ Sff8472Lane ]
  ├── monitoring: null | Sff8472Monitoring
  └── compliance (Sff8472Compliance)  └── checksums: cc_base, cc_dmi, cc_ext
```

SFF results are ok-only: every aggregate SFF decoder emits `decode_status
"ok"`; there is no rejected document. The CMIS and SFF-8636 paths are
fail-closed on length: a missing page, a page of the wrong type, or a page
under 128 bytes raises an input error. SFF-8472 instead zero-pads an A0h or
A2h page shorter than 256 bytes, exactly like the Python `_pad_page` helper;
only a missing or wrong-type page raises an input error there.

### Generated page schemas (`generated/pages/`)

One JSON Schema file per CMIS page schema. Describes the raw decoded field
layout of each page (field names, types, and ranges). Scalar types are fully
typed; complex types (`composite`, `variants:`, `present_when:`) are noted
with `x-openxcvr-note` extensions.

### Generated enum schemas (`generated/enums/`)

One JSON Schema file per CMIS enum. Uses `oneOf` + `const` + `title` to
enumerate valid values while preserving symbolic names. Range entries from
the DSL are omitted (they cannot be expressed as single integer constants).

## Versioning

`openxcvr.openapi.yaml` is versioned at `info.version` (currently `"0.4.0"`) and must be
updated manually when the decode output shape changes. A required-key addition is
breaking: 0.4.0 records `rx_los` and `tx_fault` as required on `CmisLaneStatus`.
Populating a previously null field (`lanes`, `flags`, or `checksums`) is also a
shape change and needs a bump.

Generated schemas are regenerated from CMIS schemas on every codegen run;
the committed files in `generated/` are the canonical generated output.

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
