# schemas/c-cmis-1.4/enums/

Enum schemas for the C-CMIS 1.4 supplement (OIF-C-CMIS-01.4).

## Contents

| File | Enum ID | Description |
|---|---|---|
| `vdm-observable-type.yaml` | `CCmisVdmObservableType` | 26 coherent VDM observable type identifiers (0x80–0x99), from OIF-C-CMIS-01.4 § 7.3.1 Table 8 |

These enums sit beside the windowed CMIS registry. OIF-C-CMIS-01.4's normative
CMIS reference is Revision 5.3. The c-cmis-1.4 `_index.yaml` imports
`../cmis/_index.yaml`, making both `CmisVdmObservableType` and `CCmisVdmObservableType`
available to any decoder loading the c-cmis-1.4 spec.
Observable types are a namespace of their own, not a reuse of the CMIS VDM enum.
