# schemas/c-cmis-1.5

OIF-C-CMIS-01.5 (Coherent CMIS) schema files.

C-CMIS extends CMIS for digital coherent optics (DCO) modules, scoped to 400ZR.
This directory contains the 15 C-CMIS pages at revision 01.5. OIF-C-CMIS-01.5
names CMIS Revision 5.4 as its normative CMIS reference. The windowed `cmis/`
schemas are imported via the `imports:` chain in `_index.yaml`. The namespace
follows the c-cmis-1.4 companion convention: a revision-suffixed memory-map
namespace with its own page and enum identifiers (`CCmis15…`), no Rust crate,
and a rendered Markdown reference only.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 30h | page-30h-media-lane-configurable-thresholds.yaml | Rx power + Tx offset thresholds + FDD/FED thresholds | Media lane |
| 31h | page-31h-media-lane-provisioning.yaml | Tx filter, Rx LOS type, CD compensation, payload type | Media lane |
| 32h | page-32h-media-lane-flag-masks.yaml | Alarm masks | Media lane |
| 33h | page-33h-media-lane-flags-and-status.yaml | Alarm latches + CD/Rx LOS/Tx filter status | Media lane |
| 34h | page-34h-media-lane-fec-pm.yaml | FEC performance monitoring counters (pseudo-NA values) | Media lane |
| 35h | page-35h-media-lane-link-pm.yaml | Link PM: CD, DGD, SOPMD, OSNR, EVM, etc. | Media lane |
| 38h | page-38h-data-path-host-if-config.yaml | FDD/FED thresholds + consequent actions | Data path |
| 3Ah | page-3ah-data-path-host-if-pm.yaml | Host interface FEC performance monitoring (pseudo-NA values) | Data path |
| 3Bh | page-3bh-data-path-host-if-flags-and-masks.yaml | Host alarm masks + flag latches | Data path |
| 40h | page-40h-applications-advertisement.yaml | C-CMIS revision advertisement | Unbanked |
| 41h | page-41h-rx-signal-power-advertisement.yaml | Power impl bits + Rx LOS type + threshold ranges | Unbanked |
| 42h | page-42h-pm-advertisement.yaml | Media and host PM implementation advertisement | Unbanked |
| 43h | page-43h-media-lane-provisioning-advertisement.yaml | Media provisioning impl bits + Rx LOS control | Unbanked |
| 44h | page-44h-alarm-advertisement.yaml | Alarm implementation advertisement | Unbanked |
| 45h | page-45h-host-lane-provisioning-advertisement.yaml | Host provisioning impl bits | Unbanked |

## 01.4 → 01.5 deltas (OIF-C-CMIS-01.5 § 4, Rev 1.5 change list)

- Page 30h: offset-based Tx power alarm provisioning — `txPwrUseRelativeThresh`
  (30h:144.2) and the four `txPowerVDM{High,Low}{Alarm,Warn}Offset` nibbles at 30h:145-146.
- Page 31h: `rxLosTypeConfig` (31h:131.3-0), `CDCompensationControl` (31h:136.6-4),
  `CDSearchRangeLow/High` (31h:141-144); `txFilterType`/`txFilterRollOff` gain the
  DPSM/NPSM application note.
- Page 33h: CD and Rx LOS status readback — `CDTargetSetValue` (33h:140-141),
  `CDSearchRangeLowValue/HighValue` (33h:142-145), `rxLosTypeSelection` (33h:146.3-0),
  `txFilterTypeValue` (33h:147), `txFilterRollOffValue` (33h:148).
- Pages 34h/3Ah: pseudo-NA values recorded for the PM counters that carry them
  (§ 7.4 NA Values for PM Samples); the min/max sub-interval counters carry no
  NA value in the spec and are recorded without one.
- Page 41h: `RxLosType` widened from 3 to 4 bits (values 0-5, 6-11 reserved,
  12-15 vendor defined).
- Page 42h: host PM advertisement from page 3Ah added at 42h:188-189.
- Page 43h: advertisement for the new provisioning — `cDSearchRangeValueImpl`
  (128.7), `txPwrUseRelativeThreshImpl`/`totalPwrUseCfgThreshImpl`/`sigPwrUseCfgThreshImpl`
  (129.2-0), `rxLosTypeControlImpl` (130.0), `rxLosTypeSupportedValues` (131-132).
- VDM Table 8: identifier set unchanged (128-153); the new Pseudo-NA value column
  is recorded per entry as an `na_value:` tag, with the 0xF060 low-power note on
  the three power monitors.
- § 7.5 RX LOS Type Configuration is new; the 31h/33h/41h/43h Rx LOS fields cite it.

Items in the task brief that describe earlier revisions (400ZR single-400G-DCO
scoping, SOPMD type 137 split, alarm direction normalized to module, CMIS VDM
freeze mechanism, clock recovery loop, removed multi-TEC note) are Rev 1.2
changes per the document's own revision history and are already carried by the
1.4 base; they needed no further edit here.

## Deferred parity

No `tests/fixtures/c-cmis/` fixtures exist. Golden JSON parity tests are deferred.
Schemas are authored from OIF-C-CMIS-01.5 only (PDF and its docling parse).
