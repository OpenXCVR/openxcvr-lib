# schemas/c-cmis-1.2

OIF-C-CMIS-01.2 (Coherent CMIS) schema files.

C-CMIS 01.2 extends CMIS for digital coherent optics modules, scoped to 400ZR.
The document's normative CMIS base is CMIS 4.0 (§ 8.1). This directory imports the
CMIS schema index the same way `c-cmis-1.4` does; it does not copy those pages.
Pages and registers that 01.2 does not define are absent.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 30h | page-30h-media-lane-configurable-thresholds.yaml | Rx power + FDD/FED thresholds | Media lane |
| 31h | page-31h-media-lane-provisioning.yaml | Tx filter and LF-on-LD enable | Media lane |
| 32h | page-32h-media-lane-flag-masks.yaml | Media alarm masks, including RxLOF and RxLOM | Media lane |
| 33h | page-33h-media-lane-flags-and-status.yaml | Media alarm latches, including RxLOF and RxLOM | Media lane |
| 34h | page-34h-media-lane-fec-pm.yaml | Media FEC performance monitoring | Media lane |
| 35h | page-35h-media-lane-link-pm.yaml | Link PM, high/low SOPMD and clock recovery | Media lane |
| 38h | page-38h-data-path-host-if-config.yaml | Host FDD/FED thresholds and enables | Data path |
| 3Ah | page-3ah-data-path-host-if-pm.yaml | Host FEC counters, named `tx*` | Data path |
| 3Bh | page-3bh-data-path-host-if-flags-and-masks.yaml | Host, PCS and FlexE masks and latches | Data path |
| 41h | page-41h-rx-signal-power-advertisement.yaml | Power implementation bits and threshold ranges | Unbanked |
| 42h | page-42h-pm-advertisement.yaml | PM implementation bits, including clock recovery and both SOPMD granularities | Unbanked |
| 43h | page-43h-media-lane-provisioning-advertisement.yaml | Tx filter and LF-on-LD implementation bits | Unbanked |
| 44h | page-44h-alarm-advertisement.yaml | Alarm implementation advertisement | Unbanked |

Page 40h is reserved for applications advertisement. Pages 45h–4Fh are reserved.
Vendor custom bytes occupy 248–255 (and 175–191 on page 3Bh).
VDM identifiers are 128–149 (Table 8). Identifier 137 is SOPMD high granularity,
142 is still EVM_modem, 148 is the clock recovery loop and 149 is SOPMD low granularity.

## Deferred parity

No `tests/fixtures/c-cmis/` fixtures exist. Golden JSON parity tests are deferred.
Schemas are authored from OIF-C-CMIS-01.2 only.
