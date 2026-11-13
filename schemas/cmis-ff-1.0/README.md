# schemas/cmis-ff-1.0

OIF-CMIS-FF-01.0 (Formfactor Specific Hardware Management, "CMIS-FF") schema files.

CMIS-FF is a supplement to CMIS: it defines the content of CMIS upper page 05h —
the page CMIS reserves for form-factor-specific management of hardware signals
(TxDis/TxDisable, RxLOS, TxFault, IntL/LPMode dual-function pins, and the SFP-DD
hardware rate selects) for SFP112, SFP-DD/SFP-DD112, QSFP112 and
QSFP-DD/QSFP-DD800/QSFP-DD1600 modules. OIF-CMIS-FF-01.0's normative references
are CMIS 5.2 and a CMIS 5.3 editor draft. The document says a module advertises
CMIS-FF support in bit 01h:142.3 (CMIS Table 8-41) and cites CMIS 5.3 for the
SFP112 LowPwrRequestHW behavior. This directory contains the one CMIS-FF page
(05h). The windowed `cmis/` schemas are imported via the `imports:` chain in
`_index.yaml`; that import is not a CMIS 5.3-only tree.

Source: `docs/stds/cmis/OIF-CMIS-FF-01.0.pdf` (parsed form:
`docs/stds/.parsed/markdown/cmis__OIF-CMIS-FF-01.0.md` in the sibling `openxcvr`
repository). Every field cites its spec section, table and PDF page.

## Page inventory

| Page | File | Description | Banking |
|---|---|---|---|
| 05h | page-05h-form-factor-hardware-management.yaml | Dual-mode hardware support advertising and pin control, RxLOS advertisement/control, TxFault advertisement/controls, TxDisable advertisement/controls, FC rate select, active firmware version, custom bytes, page checksum | None (unbanked, module-wide) |

## Relation to the `cmis` namespace

CMIS 5.3 § 8.8 introduced the page 05h reservation (form-factor-specific
management signals) without defining the CMIS-FF layout; CMIS 5.4 § 8.8 keeps
that reservation. The `schemas/cmis/` namespace carries its own page 05h
(`CmisPage05hFormFactorSignals`, field category `SupplementDefined`) modeling
the same registers as a supplement expansion of that reserved page. This
namespace (`CmisFfPage05hFormFactorHardwareManagement`) is the standalone
OIF-CMIS-FF-01.0 view of the same physical page, owned by the CMIS-FF spec
itself. The two pages intentionally coexist: generation is per-`--spec` and
page identity is by schema `id`, not by page address. Whether the `cmis`
namespace's page 05h supplement expansion should eventually be slimmed back to
a reserved full-page placeholder (leaving field detail to this namespace) is an
open ownership question for the project ADR record, not settled here.

## Recorded spec-text ambiguities

The parsed OIF-CMIS-FF-01.0 text contains internal inconsistencies. None were
resolved by guessing; each is recorded where it is modeled:

1. **Table 9 vs Tables 16/17 (byte map).** The Table 9 overview marks bytes
   134-254 as "Reserved [121]" and byte 255 as the page checksum, but Table 16
   defines bytes 135-136 (firmware version) and Table 17 marks bytes 245-255
   for custom use. Resolution (matching the existing `cmis` namespace page 05h
   model): detailed field tables win — bytes 135-136 are the firmware version,
   bytes 137-244 are reserved, bytes 245-254 are custom, and byte 255 is the
   page checksum per Table 9.
2. **Table 11 bit 4 (`SelectTxFaultDD`).** The field is named
   `SelectTxFaultDD` but its parsed description says "change the default IntL
   to TxFault". Section 6.2.6 defines the selectable function as TxFaultDD
   (fault on media lane 2, equivalent to CMIS `FailureFlagTx2`). The name and
   the section 6.2.6 semantics are recorded; the table's description text is
   quoted verbatim in provenance.
3. **Table 14 bit 2 (`MaskHWTxDisableDD`).** The parsed field description
   repeats the media-lane-1/`OutputDisableTx1` wording of bit 3. Section 5.8
   defines hardware TxDisableDD as 'OR'd with CMIS `OutputDisableTx2`; the
   field is recorded from section 5.8 and the discrepancy is noted in the
   field description.
4. **Table 15 `HWStatus1DD`.** The lane 2 receive CDR rate status is named
   `HWStatus1DD` as printed (presumably `HWStatusSpeed1DD`); the printed name
   is recorded.

## Deferred parity

No `tests/fixtures/cmis-ff/` fixtures exist. Golden JSON parity tests are
deferred. Schemas are authored from the OIF-CMIS-FF-01.0 PDF only.
