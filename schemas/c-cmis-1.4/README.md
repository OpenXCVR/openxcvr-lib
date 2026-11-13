# schemas/c-cmis-1.4

OIF-C-CMIS-01.4 (Coherent CMIS) schema files.

C-CMIS manages digital coherent optics (DCO) modules, initially scoped to 400ZR.
OIF-C-CMIS-01.4 names CMIS Revision 5.3 (OIF-CMIS-05.3) as its normative CMIS
reference. This directory is the 01.4 companion: 15 pages of its own. The
windowed `cmis/` schemas are imported via the `imports:` chain in `_index.yaml`,
not copied, and are not a CMIS 5.3-only tree.

The namespace is revision-suffixed and has its own pages and enums.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 30h | page-30h-media-lane-configurable-thresholds.yaml | Rx power + FDD/FED thresholds | Media lane |
| 31h | page-31h-media-lane-provisioning.yaml | Tx filter, CD target, payload type | Media lane |
| 32h | page-32h-media-lane-flag-masks.yaml | Alarm masks | Media lane |
| 33h | page-33h-media-lane-flags-and-status.yaml | Alarm latches + Rx payload type status | Media lane |
| 34h | page-34h-media-lane-fec-pm.yaml | FEC performance monitoring counters | Media lane |
| 35h | page-35h-media-lane-link-pm.yaml | Link PM: CD, DGD, SOPMD, OSNR, EVM, etc. | Media lane |
| 38h | page-38h-data-path-host-if-config.yaml | FDD/FED thresholds + consequent actions | Data path |
| 3Ah | page-3ah-data-path-host-if-pm.yaml | Host interface FEC performance monitoring | Data path |
| 3Bh | page-3bh-data-path-host-if-flags-and-masks.yaml | Host alarm masks + flag latches | Data path |
| 40h | page-40h-applications-advertisement.yaml | C-CMIS revision advertisement | Unbanked |
| 41h | page-41h-rx-signal-power-advertisement.yaml | Power impl bits + threshold ranges | Unbanked |
| 42h | page-42h-pm-advertisement.yaml | PM implementation advertisement (per-metric bits) | Unbanked |
| 43h | page-43h-media-lane-provisioning-advertisement.yaml | Media provisioning impl bits | Unbanked |
| 44h | page-44h-alarm-advertisement.yaml | Alarm implementation advertisement | Unbanked |
| 45h | page-45h-host-lane-provisioning-advertisement.yaml | Host provisioning impl bits | Unbanked |

## Deferred parity

No `tests/fixtures/c-cmis/` fixtures exist. Golden JSON parity tests are deferred.
Schemas are authored from OIF-C-CMIS-01.4 PDF only.
