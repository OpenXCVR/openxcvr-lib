# schemas/cmis-vcs-1.1

OIF-CMIS-VCS-01.1 (CMIS Versatile Control Set) schema files.

CMIS-VCS generalizes the CMIS base Control Sets: a module advertises a
module-specific list of Signal Integrity (SI) parameters (VCS Parameters),
their attributes and their placement inside the Control Set memory spaces
through CDB commands (VCS Descriptor, TLV encoded). Hosts that do not
support VCS keep seeing the CMIS base Control Set layout. This directory
contains the supplement schema namespace. OIF-CMIS-VCS-01.1's reference is
CMIS Rev 5.3; it also says backwards compatibility is within CMIS 5.x and
copies some signal-integrity controls from CMIS 5.2. The windowed `cmis/`
schemas are imported via the `imports:` chain in `_index.yaml`.

## What this namespace owns

The windowed CMIS schema reserves the VCS parameter spaces as
`SupplementDefined` regions (`schemas/cmis/page-18h-control-set-extension.yaml`
bytes 144-199 and 200-255, `schemas/cmis/page-19h-status-extension.yaml`
bytes 152-207) pointing at CMIS-VCS as the defining supplement. This
namespace defines those spaces:

| Page | File | Bytes | Description |
|---|---|---|---|
| 18h | page-18h-vcs-staged-overflow.yaml | 144-255 | Staged Control Set 0 and 1 VCS overflow spaces |
| 19h | page-19h-vcs-active-overflow.yaml | 152-207 | Active Control Set VCS overflow space (read-only parameters) |

The in-page Control Set VCS space (page 10h bytes 153-175 and 188-210,
page 11h bytes 214-234) is not re-modeled here: OIF-CMIS-VCS-01.1 § 4.5
requires modules to keep the CMIS base Control Set layout in this revision,
so those bytes are the CMIS base parameters already defined by the `cmis`
namespace (`schemas/cmis/page-10h-lane-control.yaml`,
`schemas/cmis/page-11h-lane-status.yaml`).

## Layout is module-advertised

VCS defines no fixed field map inside the VCS spaces: the host computes each
VCS Parameter's placement from the accumulated MemoryLength attributes of the
advertised VCS Descriptor (§ 4.4). The schema therefore records the spaces as
region fields whose descriptions carry the fixed placement rules (base space
filled first, host-interface parameters before media-interface parameters
per § 4.8.1, one parameter never fragmented across pages per § 4.4), and
records every VCS Parameter's register format (field type, data type, size in
bits per lane, access) in `enums/vcs-parameter-id.yaml`. Individual parameter
offsets inside the spaces are `not recorded` because they are runtime
advertisement data, not specification data.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 18h | page-18h-vcs-staged-overflow.yaml | SCS0 overflow (144-199) + SCS1 overflow (200-255) | Lane banked |
| 19h | page-19h-vcs-active-overflow.yaml | ACS overflow (152-207), read-only | Lane banked |

## Enums

| File | Enum ID | Description |
|---|---|---|
| enums/vcs-parameter-id.yaml | CmisVcsParameterId | VCS Parameter ID registry, OIF-CMIS-VCS-01.1 § 5 Table 5-1, with per-parameter register formats as tags |
| enums/vcs-tx-input-eq-code.yaml | CmisVcsTxInputEqCode | Tx host-controlled input equalization codes, Table 5-11 |
| enums/vcs-rx-output-eq-code.yaml | CmisVcsRxOutputEqCode | Rx output equalization pre/post-cursor codes, Table 5-18 |
| enums/vcs-rx-output-amplitude-code.yaml | CmisVcsRxOutputAmplitudeCode | Rx output amplitude codes, Table 5-21 |
| enums/vcs-rx-output-fine-amplitude-code.yaml | CmisVcsRxOutputFineAmplitudeCode | Rx output fine amplitude codes, Table 5-35 |
| enums/vcs-equalizer-target.yaml | CmisVcsEqualizerTarget | EqualizerTarget attribute values (GDC/GDC2), Tables 5-24 and 5-26 |
| enums/vcs-equalization-step-size.yaml | CmisVcsEqualizationStepSize | StepSize attribute encoding for numeric equalization targets, Tables 5-24 and 5-26 |
| enums/vcs-fir-tap-step-size.yaml | CmisVcsFirTapStepSize | StepSize attribute encoding for FIR tap weights, Tables 5-33 and 5-45 |

## INCONCLUSIVE — spec-internal inconsistencies recorded, not resolved

These are recorded as found in OIF-CMIS-VCS-01.1; the tables govern where
they conflict with prose.

- § 4.6 states the ExplicitControlPerParam parameter "is assigned the VCS
  Parameter ID 02h"; Table 5-1 and Table 5-4 both record 01h. The schema
  follows the tables (01h).
- § 4.6's ID claim aside, Table 5-4 does not list a PropertyFlags attribute
  for ExplicitControlPerParam, but Appendix B Table B-1 shows one; the
  attribute's presence for this parameter is not recorded as settled.
- Table 5-37 (OutputAmplitudeSettingRx register format) lists Data Type "U8"
  with Size "4 (bits per lane)"; Table 5-36 advertises a CodeValueMask with
  16 code values (Table 5-35). The 4-bits-per-lane size is recorded; the U8
  data type label is recorded as printed.
- Parsed Table 4-1 prints "Page 10h" for the Active Control Set row; Appendix
  B Table B-4 and the CMIS base place the Active Control Set on page 11h
  bytes 214-234. The schema follows page 11h (CMIS base layout).
- Appendix A: Table A-1's ninth advertised parameter is a media-interface
  OutputEqPrePostCursorTargetRx, while Table A-3 titles the resulting
  overflow region "MediaOutputEqPostCursorTargetTx". The appendix example's
  naming is inconsistent; no schema content depends on it.
- The Active Control Set placement of read-write VCS Parameters that
  overflow the staged overflow space is not recorded: § 4.4 states the
  read-write register offsets are the same for all Control Sets, but Table
  4-1 gives the Active overflow as page 19h bytes 152-207 while the staged
  overflow is page 18h bytes 144-199/200-255, and page 19h bytes 144-151
  are the CMIS base NAD block indices. Only the region bounds are recorded.

## Deferred parity

No `tests/fixtures/cmis-vcs/` fixtures exist. Golden JSON parity tests are
deferred. Schemas are authored from OIF-CMIS-VCS-01.1 only.

## Not in this namespace

- CDB commands 0045h (Externally Defined Features) and 4000h-4003h (VCS
  Overview, Read-Write/Read-Only/Custom VCS Descriptor) are CDB command
  message exchanges, not memory-map pages; they belong in
  `openxcvr.cdb_command.v1` documents once the VCS command group
  is registered. CMD 0045h extends the CMIS base CDB registry owned by the
  `cmis` namespace.
- The VCS Descriptor TLV stream itself (carried in CDB reply LPL) is
  advertisement data; its common attribute layout is documented in the
  parameter enum entries, not as page fields.
