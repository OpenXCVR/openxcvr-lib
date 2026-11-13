# synthetic-cmis-5-4-qsfp-dd-vdm

> **synthetic: true**
> **vendor_strings_synthetic: true**
> Source: hand-authored from the CMIS 5.4 spec (`docs/stds/cmis/OIF-CMIS-05.4.pdf`, OIF, 2025). NO bytes copied from any third-party source. The byte layout is a tested-shape skeleton, not a real-vendor capture.

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x18` (QSFP-DD) | SFF-8024 § 4.1 Table 4-1 |
| CmisRevision (Lower:1) | `0x54` (5.4) | CMIS 5.4 § 8.2.1 Table 8-5 |
| MemoryModel (Lower:2[7]) | `0` (paged) | CMIS 5.4 § 8.2.1 Table 8-5 |
| Identifier copy (Page 00h:0 = flat 128) | `0x18` (mirror) | CMIS 5.4 § 8.3.1 |
| Vendor name (Page 00h:1–16) | `CMIS_5_4_VDM_SYNTH ` | Synthetic placeholder |
| Vendor OUI (Page 00h:17–19) | `00 00 00` | Synthetic placeholder |
| Vendor PN (Page 00h:20–35) | `FIXTURE-SYNTH-V1` | Synthetic placeholder |
| Vendor rev (Page 00h:36–37) | `00` | Synthetic placeholder |
| Vendor S/N (Page 00h:38–53) | `SYNTHETIC0000002` | Synthetic placeholder |
| Date code (Page 00h:54–61) | `25111100` | Synthetic placeholder |
| CLEI code (Page 00h:62–71) | 10 ASCII spaces | Synthetic placeholder |
| Page 00h checksum (Page 00h:94) | sum of bytes 0–93 & 0xFF | CMIS 5.4 § 8.3.11 |
| VDMPagesSupported (01h:142.6) | `1` | CMIS 5.4 § 8.4.4 Table 8-47 |
| BanksSupported (01h:142.1-0) | `00b` (bank 0, 8 lanes) | CMIS 5.4 § 8.4.4 Table 8-47 |

## What this fixture exercises

**The complete VDM page set of a four-group module** — the first fixture in
the corpus to carry supervision-threshold Pages 28h-2Bh and supervision-mask
Page 2Dh at all, plus descriptor/samples pages for all four groups and the
full 2Fh control set including the CMIS 5.4 duty-cycle registers:

- Page 2Fh (Table 8-186): VDMSupport = 3 (groups 1-4), DutyCycleSupport = 1,
  PowerSavingSupport = 1, FineIntervalLength = 100 (10.0 ms),
  MonitoringDutyCycle k = 5, FreezeDone/UnfreezeDone = 0.
- Pages 20h-23h descriptors (Table 8-178/8-179) with the global threshold-set
  ID arithmetic (1/17/33/49 + local): laser age (type 1, U16), laser
  temperature deviation (type 4, S16 — the 5.4 redefinition), pre-FEC BER
  minimum (type 9, F16), Vcc2p6 monitor (type 0x4D, U16, module), FERC total
  accumulated (type 0x1A, F16), ELS input power (type 0x54, S16), SEWmax
  minimum (type 0x1B, U16, group 4).
- Pages 24h-27h samples with hand-computed raw values exercising the 
  polymorphic dispatch: 0x0064 (100), 0x0100 (+256 = +1.0 degC in 1/256 degC),
  0x98FA (F16 2.5e-3), 0x0A28 (2600), 0xC001 (F16 1.0), 0xFFCE (-50 = -0.50
  dBm in 0.01 dBm), 0x0007.
- Pages 28h-2Bh threshold quads (Table 8-182): one authored quad per page
  (sets 1, 17, 33, 49 by the global ID arithmetic) as raw X16 carriers.
- Pages 2Ch/2Dh (Tables 8-183..8-185): threshold-crossing flags and masks in
  the two-instances-per-byte nibble packing, including instances 1, 2, 129
  and 255 so both nibble halves and all group quarters carry authored bits.

**Synthetic VDM parity golden** — `tests/golden/cmis/synthetic-cmis-5-4-qsfp-dd-vdm.json`
pins the expected VDM semantics of the authored bytes: descriptor layout and
global threshold-set arithmetic, the per-observable-type sample
decode (U16/S16/F16), the threshold-quad layout, the TC flag/mask nibble
packing and the 2Fh control decode. The golden's expected semantics are
authored from the fixture bytes plus the CMIS 5.4 spec tables (never by
running a decoder and capturing its output); `tools/build-cmis-5-4-synthetic.py`
regenerates both the `.bin` slices and the golden deterministically, and
`tools/test_build_cmis_synthetics.py` (Cmis54VdmBuilderTests) pins that
reproduction. The `vdm` block is a fixture-scoped extension admitted by the
permissive CmisDecodeResult OpenAPI schema; no generated decoder output is
compared against it today. It is a **synthetic parity golden, NOT a hardware
oracle** — see the `synthetic_note` metadata field.

Spec-grounded decode of the zeroed pages matches the CMIS 5.2 synthetic
fixture conventions: Page 11h all-zero DPStateHostLane nibbles are 0h =
**Reserved** ("Reserved0"), ModuleState 000b = Reserved, byte 3 bit 0
InterruptDeasserted = 0b means Interrupt asserted, monitors read 0.0, module
flags clear, no Applications advertised (`applications` renders null).

## Regen

```sh
python3 tools/build-cmis-5-4-synthetic.py
```
