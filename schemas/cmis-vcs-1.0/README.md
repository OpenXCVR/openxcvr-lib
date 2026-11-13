# schemas/cmis-vcs-1.0

OIF-CMIS-VCS-01.0 (CMIS Versatile Control Set) schema files.

CMIS-VCS generalizes the CMIS base Control Sets: a module advertises a
module-specific list of Signal Integrity (SI) parameters (VCS Parameters),
their attributes and their placement inside the Control Set memory spaces
through CDB commands (VCS Descriptor, TLV encoded). Hosts that do not
support VCS keep seeing the CMIS base Control Set layout. This directory
contains the supplement schema namespace.

## CMIS base revision

OIF-CMIS-VCS-01.0 § 7.1 source [1] names Common Management Interface
Specification (CMIS) Rev 5.3 (OIF-CMIS-05.3, 2024) as its normative base.
Section 6 repeats the same citation ("In CMIS 5.3 [1]"). The `imports:`
chain in `_index.yaml` supplies the windowed `cmis` namespace, not a
CMIS 5.3-only copy of those pages.

## What this namespace owns

The CMIS 5.3 base schema reserves the VCS parameter spaces as
`SupplementDefined` regions (`schemas/cmis/page-18h-control-set-extension.yaml`
bytes 144-199 and 200-255, `schemas/cmis/page-19h-status-extension.yaml`
bytes 152-207) pointing at CMIS-VCS as the defining supplement. This
namespace defines those spaces:

| Page | File | Bytes | Description |
|---|---|---|---|
| 18h | page-18h-vcs-staged-overflow.yaml | 144-255 | Staged Control Set 0 and 1 VCS overflow spaces |
| 19h | page-19h-vcs-active-overflow.yaml | 152-207 | Active Control Set VCS overflow space (read-only parameters) |

The in-page Control Set bytes are not re-modeled here. § 4.5 requires
modules, in this revision, to keep the Control Set space defined by CMIS
base Rev 5.3, so those bytes are the CMIS base parameters already defined
by the `cmis` namespace (`schemas/cmis/page-10h-lane-control.yaml`,
`schemas/cmis/page-11h-lane-status.yaml`). Table 4-1 prints different
in-page ranges; that disagreement is recorded below and on the page fields,
and it does not add a second in-page map.

## Layout is module-advertised

VCS defines no fixed field map inside the VCS spaces: the host computes each
VCS Parameter's placement from the accumulated MemoryLength attributes of the
advertised VCS Descriptor (§ 4.4). The schema therefore records the spaces as
region fields whose descriptions carry the fixed placement rules (base space
filled first, one parameter never fragmented across pages per § 4.4), and
records every VCS Parameter's register format (field type, data type, size in
bits per lane, access) in `enums/vcs-parameter-id.yaml`. Individual parameter
offsets inside the spaces are `not recorded` because they are runtime
advertisement data, not specification data. Advertisement order is the only
ordering rule this revision states (§ 4.4).

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 18h | page-18h-vcs-staged-overflow.yaml | SCS0 overflow (144-199) + SCS1 overflow (200-255) | Lane banked |
| 19h | page-19h-vcs-active-overflow.yaml | ACS overflow (152-207), read-only | Lane banked |

## Enums

| File | Enum ID | Description |
|---|---|---|
| enums/vcs-parameter-id.yaml | CmisVcs10ParameterId | VCS Parameter ID registry, OIF-CMIS-VCS-01.0 § 5 Table 5-1, with per-parameter register formats as tags |
| enums/vcs-tx-input-eq-code.yaml | CmisVcs10TxInputEqCode | Tx host-controlled input equalization codes, Table 5-11 |
| enums/vcs-rx-output-eq-code.yaml | CmisVcs10RxOutputEqCode | Rx output equalization pre/post-cursor codes, Table 5-18 |
| enums/vcs-rx-output-amplitude-code.yaml | CmisVcs10RxOutputAmplitudeCode | Rx output amplitude codes, Table 5-21 |
| enums/vcs-rx-output-fine-amplitude-code.yaml | CmisVcs10RxOutputFineAmplitudeCode | Rx output fine amplitude codes, Table 5-35 |
| enums/vcs-equalization-step-size.yaml | CmisVcs10EqualizationStepSize | StepSize attribute encoding for numeric equalization targets, Tables 5-24 and 5-26 |
| enums/vcs-fir-tap-step-size.yaml | CmisVcs10FirTapStepSize | StepSize attribute encoding for FIR tap weights, Tables 5-33 and 5-45 |

## INCONCLUSIVE — spec-internal inconsistencies recorded, not resolved

These are recorded as found in OIF-CMIS-VCS-01.0; the tables govern where
they conflict with prose, except Table 4-1's in-page columns, which conflict
with later tables in the same document and are not used as a second map.

- § 4.6 states the ExplicitControlPerParam parameter "is assigned the VCS
  Parameter ID 02h"; Table 5-1 and Table 5-4 both record 01h. The schema
  follows the tables (01h).
- Table 4-1 prints Staged Control Set 0 as page 10h bytes 188-195 and
  Staged Control Set 1 as page 10h bytes 196-210. § 4.5 requires the CMIS
  base Rev 5.3 Control Set layout, § 5.2 places AdaptiveInputEqRecallTx at
  page 10h bytes 154-155, and Appendix A Table A-2 and Appendix B Table B-3
  start the staged registers at byte 153. The overflow columns of Table 4-1
  (page 18h bytes 144-199 and 200-255, page 19h bytes 152-207) agree with
  Appendix B Table B-5, which starts at page 19h byte 152, and those are
  the regions this namespace models.
- Table 5-37 (OutputAmplitudeSettingRx register format) lists Data Type "U8"
  with Size "4 (bits per lane)"; Table 5-35 has 16 code values. The
  4-bits-per-lane size is recorded; the U8 data type label is recorded as
  printed. § 5.16's prose names the field OutputAmplitudeTargetRx; Table
  5-36 names the descriptor OutputAmplitudeSettingRx and Table 5-1 names
  the parameter OutputFineAmplitudeSettingRx. The parameter id follows
  Table 5-1 and the register name follows Table 5-37.
- A note under Appendix B Table B-4 says the Active Control Set memory
  location in CMIS base is page 10h. Table 4-1 and Table B-4's own header
  place it on page 11h bytes 214-234. The schema follows page 11h.
- Appendix B Table B-1's third parameter is HostControlledInputEqTargetNumericTx
  (ID 09h) with Length 04h and a CodeValueMask. Table 5-24 gives that
  parameter Min, Max and StepSize, and no code mask. The example is not
  recorded as the parameter format; Table 5-24 is.
- Appendix B Table B-1 says the explicit-control mask applies to VCS
  Parameter ID 04h. § 5.3 and Table 5-5 say bit 0 is the first advertised
  VCS Parameter, independent of its ID. The schema follows Table 5-5.
- § 4.4 cites the read-only VCS Descriptor CDB command as section 6.4.
  Section 6.4 is CMD 4001h (read-write) and section 6.5 is CMD 4002h
  (read-only). Table 5-1 (PDF pages 20-21) cites no section. § 5's prose
  immediately before that table (PDF page 20) references section 6.5.
  The schema follows that prose.
- § 5.18's prose says the HostChannelLossRx value is ignored when the host
  owns Tx input equalization. Table 5-41 names HostChannelLossTx. The
  schema follows the table.
- § 5.15's opening sentence says the Rx FIR taps are in dB, while Table 5-33
  prints StepSize without a unit and Table 5-32's example is a dimensionless
  scale normalized to magnitude 1. Table 5-45 prints the same step encoding
  as a step in dB. One enum records both printings.
- The Active Control Set placement of read-write VCS Parameters that
  overflow the staged overflow space is not recorded: § 4.4 states the
  read-write register offsets are the same for all Control Sets, but Table
  4-1 gives the Active overflow as page 19h bytes 152-207 while the staged
  overflow is page 18h bytes 144-199/200-255, and page 19h bytes 144-151
  are the CMIS base NAD block indices. Only the region bounds are recorded.

## Deferred parity

No `tests/fixtures/cmis-vcs/` fixtures exist. Golden JSON parity tests are
deferred. Schemas are authored from OIF-CMIS-VCS-01.0 only.

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
