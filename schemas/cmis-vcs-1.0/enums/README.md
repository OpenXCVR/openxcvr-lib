# schemas/cmis-vcs-1.0/enums/

Enum schemas for the CMIS-VCS 1.0 supplement (OIF-CMIS-VCS-01.0).

## Contents

| File | Enum ID | Description |
|---|---|---|
| `vcs-parameter-id.yaml` | `CmisVcs10ParameterId` | VCS Parameter ID registry (0x00-0x12), OIF-CMIS-VCS-01.0 § 5 Table 5-1, with each parameter's register format and descriptor attributes recorded in tags and descriptions |
| `vcs-tx-input-eq-code.yaml` | `CmisVcs10TxInputEqCode` | Tx host-controlled input equalization codes, Table 5-11 (§ 5.6) |
| `vcs-rx-output-eq-code.yaml` | `CmisVcs10RxOutputEqCode` | Rx output equalization pre/post-cursor codes, Table 5-18 (§ 5.9) |
| `vcs-rx-output-amplitude-code.yaml` | `CmisVcs10RxOutputAmplitudeCode` | Rx output amplitude codes, Table 5-21 (§ 5.10) |
| `vcs-rx-output-fine-amplitude-code.yaml` | `CmisVcs10RxOutputFineAmplitudeCode` | Rx output fine amplitude codes, Table 5-35 (§ 5.16) |
| `vcs-equalization-step-size.yaml` | `CmisVcs10EqualizationStepSize` | StepSize attribute encoding for numeric equalization targets, Tables 5-24 and 5-26 |
| `vcs-fir-tap-step-size.yaml` | `CmisVcs10FirTapStepSize` | StepSize attribute encoding for FIR tap weights, Tables 5-33 and 5-45 |

These enums extend the CMIS 5.3 registries. OIF-CMIS-VCS-01.0 § 7.1 names
CMIS Rev 5.3 as its base. The cmis-vcs-1.0 `_index.yaml` imports
`../cmis/_index.yaml`, so both the base and the supplement enums are
available to any decoder loading the cmis-vcs-1.0 spec.

Tables 5-24 and 5-26 list Min, Max and StepSize and no other
equalization-target attribute, so there is no separate target enum.
