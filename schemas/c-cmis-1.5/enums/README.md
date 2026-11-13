# schemas/c-cmis-1.5/enums/

Enum schemas for the C-CMIS 1.5 supplement (OIF-C-CMIS-01.5).

## Contents

| File | Enum ID | Description |
|---|---|---|
| `vdm-observable-type.yaml` | `CCmis15VdmObservableType` | 26 coherent VDM observable type identifiers (0x80–0x99) with their pseudo-NA values, from OIF-C-CMIS-01.5 § 7.3.1 Table 8 |

The identifier set is unchanged from C-CMIS 1.4; revision 01.5 adds the Pseudo-NA
value column, recorded here as an `na_value:` tag on every entry (Table 8 Note 3),
and the 0xF060 low-power reporting note for the three power monitors
(Table 8 Note 4). These enums sit beside the windowed CMIS registry.
OIF-C-CMIS-01.5's normative CMIS reference is Revision 5.4. The c-cmis-1.5
`_index.yaml` imports `../cmis/_index.yaml`, making both `CmisVdmObservableType`
and `CCmis15VdmObservableType` available to any decoder loading the c-cmis-1.5
spec. Observable types are a namespace of their own, not a reuse of the CMIS VDM enum.
