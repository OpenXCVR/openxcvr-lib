# schemas/c-cmis-1.3

OIF-C-CMIS-01.3 (Coherent CMIS) schema files.

C-CMIS 01.3 extends CMIS for digital coherent optics modules, scoped to 400ZR.
The document's normative CMIS base is CMIS 5.2 (§ 8.1). This directory imports the
CMIS schema index the same way `c-cmis-1.4` does; it does not copy those pages.
Pages and registers that 01.3 does not define are absent.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 30h | page-30h-media-lane-configurable-thresholds.yaml | Rx power + FDD/FED thresholds | Media lane |
| 31h | page-31h-media-lane-provisioning.yaml | Tx filter and LF-on-LD enable | Media lane |
| 32h | page-32h-media-lane-flag-masks.yaml | Media alarm masks, including RxLOF and RxLOM | Media lane |
| 33h | page-33h-media-lane-flags-and-status.yaml | Media alarm latches, including RxLOF and RxLOM | Media lane |
| 34h | page-34h-media-lane-fec-pm.yaml | Media FEC performance monitoring | Media lane |
| 35h | page-35h-media-lane-link-pm.yaml | Link PM through Q margin | Media lane |
| 38h | page-38h-data-path-host-if-config.yaml | Host FDD/FED thresholds and enables | Data path |
| 3Ah | page-3ah-data-path-host-if-pm.yaml | Host FEC counters, including corrected frames | Data path |
| 3Bh | page-3bh-data-path-host-if-flags-and-masks.yaml | Host, PCS and FlexE masks and latches, including FlexE RPF | Data path |
| 40h | page-40h-applications-advertisement.yaml | C-CMIS revision advertisement | Unbanked |
| 41h | page-41h-rx-signal-power-advertisement.yaml | Power implementation bits, Rx LOS type and threshold ranges | Unbanked |
| 42h | page-42h-pm-advertisement.yaml | PM implementation bits through Q margin and MER | Unbanked |
| 43h | page-43h-media-lane-provisioning-advertisement.yaml | Tx filter and LF-on-LD implementation bits | Unbanked |
| 44h | page-44h-alarm-advertisement.yaml | Alarm implementation advertisement, including FlexE RPF | Unbanked |

Pages 45h–4Fh are reserved. Vendor custom bytes occupy 248–255 (and the mask-area custom
range on page 3Bh). VDM identifiers are 128–152 (Table 8): 142 is renamed EVM,
and 150–152 are SNR margin, Q factor and Q margin. CFO low granularity (153) is
not in this revision. Rx LOS type at 41h:129.2-0 is three bits.

## Deferred parity

No `tests/fixtures/c-cmis/` fixtures exist. Golden JSON parity tests are deferred.
Schemas are authored from OIF-C-CMIS-01.3 only.
