# CMIS golden audit flags

- Audit date: 2026-04-29
- Scope: 11 decodeable CMIS goldens plus anomalous rejection-form golden from commit cce9e7d task.
- Offset rule: locked schema YAML was authoritative. Page 01h additional ApplicationDescriptor host IDs were decoded from `schemas/cmis-5.3/page-01h-advertising.yaml` offset expression `223 + 4 * (app - 1)`, i.e. page-relative 95, despite the prompt text also mentioning page-relative 96.
- Application rule used here: Lower memory contributes AppSel codes 1-8; Page 01h contributes additional AppSel codes 9-15 when both host and media IDs are populated and not `0xFF`.

## Raw byte repairs
- None. Every existing `*.raw` byte matched the schema-correct fixture byte before the audit.

## Enum-name repairs
- None. Every audited `*.name` value already matched the corresponding schema enum `name`.

## Scalar repairs beyond raw/name pairs
- Module-state audit was checked against `schemas/cmis-5.3/enums/module-state.yaml`; no module-state repairs remain after using the enum's actual value mapping.
- Some fixtures had stale rounded temperature values or empty strings where Page 00h bytes are NUL-padded. Those were repaired from schema offsets Lower:14-15 and Page 00h:20-59.

## Module type normalization
- `module.module_type` is not a byte-decode and the golden contract says it must be vendor-PN-derived. The audited files had invented form-factor labels in this field despite NUL or non-decoding PN bytes, so those labels were normalized to `null` to preserve sibling scalar-path parity.

## ApplicationDescriptor repairs
### fboss-cmis-4-0-osfp-2x400g-dr4
- applications: 3 entries -> 10 entries
- missing app_code 9: host=0x4F media=0x1C lanes=4/4 assign=0x11
- missing app_code 10: host=0x4E media=0x1C lanes=2/2 assign=0x55
- missing app_code 11: host=0x4D media=0x1C lanes=2/2 assign=0x55
- missing app_code 12: host=0x4C media=0x14 lanes=1/1 assign=0xFF
- missing app_code 13: host=0x4B media=0x14 lanes=1/1 assign=0xFF
- missing app_code 14: host=0x11 media=0x17 lanes=8/8 assign=0x01
- missing app_code 15: host=0x0F media=0x17 lanes=4/4 assign=0x11
### fboss-cmis-4-0-qsfpdd-400g-lr4
- applications: 0 entries -> 4 entries
- missing app_code 1: host=0x11 media=0x1E lanes=8/4 assign=0x01
- missing app_code 2: host=0x0F media=0x18 lanes=4/4 assign=0x01
- missing app_code 3: host=0x11 media=0xC0 lanes=8/4 assign=0x01
- missing app_code 4: host=0x11 media=0xC1 lanes=8/4 assign=0x01
### fboss-cmis-5-0-osfp-2x400g-fr4
- applications: 0 entries -> 7 entries
- missing app_code 1: host=0x50 media=0x1D lanes=4/4 assign=0x11
- missing app_code 2: host=0x0F media=0x18 lanes=4/4 assign=0x11
- missing app_code 3: host=0x4C media=0x15 lanes=1/1 assign=0xFF
- missing app_code 4: host=0x42 media=0x10 lanes=4/4 assign=0x11
- missing app_code 5: host=0x4E media=0xC0 lanes=2/2 assign=0x55
- missing app_code 7: host=0x4F media=0x1D lanes=4/4 assign=0x11
- missing app_code 8: host=0x4B media=0x15 lanes=1/1 assign=0xFF
### fboss-cmis-5-1-osfp-2x400g-fr4-lite
- applications: 0 entries -> 6 entries
- missing app_code 1: host=0x50 media=0x1D lanes=4/4 assign=0x11
- missing app_code 2: host=0x0F media=0x18 lanes=4/4 assign=0x11
- missing app_code 3: host=0x4C media=0x15 lanes=1/1 assign=0xFF
- missing app_code 4: host=0x42 media=0x10 lanes=4/4 assign=0x11
- missing app_code 5: host=0x4E media=0xC0 lanes=2/2 assign=0x55
- missing app_code 6: host=0x52 media=0xC1 lanes=8/8 assign=0x01
### fboss-cmis-5-1-osfp-2x400g-fr4-lpo
- applications: 0 entries -> 3 entries
- missing app_code 1: host=0x20 media=0x15 lanes=1/1 assign=0xFF
- missing app_code 2: host=0x22 media=0x1D lanes=4/4 assign=0x11
- missing app_code 3: host=0x23 media=0xC1 lanes=8/8 assign=0x01
### fboss-cmis-5-1-osfp-2x800g-dr4
- applications: 0 entries -> 5 entries
- missing app_code 1: host=0x82 media=0x77 lanes=4/4 assign=0x11
- missing app_code 2: host=0x81 media=0x75 lanes=2/2 assign=0x55
- missing app_code 3: host=0x80 media=0x73 lanes=1/1 assign=0xFF
- missing app_code 4: host=0x50 media=0x1C lanes=4/4 assign=0x11
- missing app_code 5: host=0x4C media=0x14 lanes=1/1 assign=0xFF
### fboss-cmis-5-1-osfp-credo-800g-aec
- applications: 0 entries -> 8 entries
- missing app_code 1: host=0x4B media=0x04 lanes=1/1 assign=0xFF
- missing app_code 2: host=0x4C media=0x04 lanes=1/1 assign=0xFF
- missing app_code 3: host=0x4D media=0x04 lanes=2/2 assign=0x55
- missing app_code 4: host=0x4E media=0x04 lanes=2/2 assign=0x55
- missing app_code 5: host=0x4F media=0x04 lanes=4/4 assign=0x11
- missing app_code 6: host=0x50 media=0x04 lanes=4/4 assign=0x11
- missing app_code 7: host=0x51 media=0x04 lanes=8/8 assign=0x01
- missing app_code 8: host=0x52 media=0x04 lanes=8/8 assign=0x01

## Missing enum codes
- None.

## Page 01h empty descriptor tables
- fboss-cmis-4-0-qsfp112-200g-fr4: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-4-0-qsfpdd-400g-dr4: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-4-0-qsfpdd-400g-fr4-multiport: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-4-0-qsfpdd-400g-lr4: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-5-0-osfp-2x400g-fr4: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: app9=51 00 88 01, app10=ff 00 00 00
- fboss-cmis-5-1-osfp-2x400g-fr4-lite: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-5-1-osfp-2x400g-fr4-lpo: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-5-1-osfp-2x800g-dr4: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: none
- fboss-cmis-5-1-osfp-credo-800g-aec: Page 01h present but no populated descriptors at schema offset page-relative 95; nonzero slots: app9=ff 00 00 00

## Anomalous fixture
- fboss-cmis-anomalous-qsfpdd-400g-cr8: top-level keys were ['_metadata', 'decode_status', 'rejection']. Expected ['_metadata', 'decode_status', 'rejection']. No modification made.

## Unchanged decodeable fixtures
- None.

