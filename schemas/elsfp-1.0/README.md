# schemas/elsfp-1.0

OIF-ELSFP-CMIS-01.0 (External Laser SFP CMIS) schema files.

ELSFP is the partner management agreement for External Laser Small Form-Factor
Pluggable (ELSFP) modules — external-cavity tunable lasers in an SFP shell.
OIF-ELSFP-CMIS-01.0's normative CMIS reference is Revision 5.3; its firmware
section also cites OIF-CMIS-05.2 Table 9-14 for the CDB firmware-download
commands. ELSFP modules operate as "resource modules" (MSM only, no DPSM).
This directory contains 2 new pages. The windowed `cmis/` schemas are imported
via the `imports:` chain in `_index.yaml`, not a CMIS 5.3-only copy.

The namespace is revision-suffixed. Field meanings are in the schema files.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 1Ah | page-1ah-lane-mgmt.yaml | Lane management: advertisements, thresholds, flags, masks, enable/state, fiber check, freq | Lane-banked (8/bank); bytes 128-185 module-wide |
| 1Bh | page-1bh-lane-controls-monitors.yaml | Lane controls: bias + optical power setpoints and monitors, voltage monitor, Vcc current | Lane-banked (8/bank) |

## Banking model

Both pages use `per_bank_dimension: lane` with `bank_select_field: lower_memory.bank_select`.
8 lanes are accessible per bank read. Page 1Ah has a mixed structure: bytes 128-185 are
module-wide (unbanked header, same in all banks) while bytes 186-255 are per-bank indexed.
The global fault/warning flag arrays at bytes 166-169 and 174-177 report all 32 absolute
lanes regardless of bank selection.

## Deferred parity

No `tests/fixtures/elsfp/` fixtures exist. Golden JSON parity tests are deferred.
Schemas are authored from OIF-ELSFP-CMIS-01.0 PDF only.
