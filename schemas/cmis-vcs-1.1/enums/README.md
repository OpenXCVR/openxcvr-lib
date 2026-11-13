# schemas/cmis-vcs-1.1/enums/

Enum schemas for the CMIS-VCS 1.1 supplement (OIF-CMIS-VCS-01.1).

## Contents

| File | Enum ID | Description |
|---|---|---|
| `vcs-parameter-id.yaml` | `CmisVcsParameterId` | VCS Parameter ID registry (0x00-0x12), OIF-CMIS-VCS-01.1 § 5 Table 5-1, with each parameter's register format and descriptor attributes recorded in tags and descriptions |
| `vcs-tx-input-eq-code.yaml` | `CmisVcsTxInputEqCode` | Tx host-controlled input equalization codes, Table 5-11 (§ 5.6) |
| `vcs-rx-output-eq-code.yaml` | `CmisVcsRxOutputEqCode` | Rx output equalization pre/post-cursor codes, Table 5-18 (§ 5.9) |
| `vcs-rx-output-amplitude-code.yaml` | `CmisVcsRxOutputAmplitudeCode` | Rx output amplitude codes, Table 5-21 (§ 5.10) |
| `vcs-rx-output-fine-amplitude-code.yaml` | `CmisVcsRxOutputFineAmplitudeCode` | Rx output fine amplitude codes, Table 5-35 (§ 5.16) |
| `vcs-equalizer-target.yaml` | `CmisVcsEqualizerTarget` | EqualizerTarget attribute values (GDC, GDC2), Tables 5-24 and 5-26 |
| `vcs-equalization-step-size.yaml` | `CmisVcsEqualizationStepSize` | StepSize attribute encoding for numeric equalization targets, Tables 5-24 and 5-26 |
| `vcs-fir-tap-step-size.yaml` | `CmisVcsFirTapStepSize` | StepSize attribute encoding for FIR tap weights, Tables 5-33 and 5-45 |

These enums sit beside the windowed CMIS registries. OIF-CMIS-VCS-01.1's
reference is CMIS Rev 5.3. The cmis-vcs-1.1 `_index.yaml` imports
`../cmis/_index.yaml`, so both the base and the supplement enums are
available to any decoder loading the cmis-vcs-1.1 spec.
