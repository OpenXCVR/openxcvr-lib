# openxcvr Python bindings

Python binding: generated page decoders emitted from the same schemas as the Rust crates, with a hand-written facade in the public `openxcvr` module on top.

## Overview

Python decoding facade for CMIS, SFF-8636, and SFF-8472. The public module is
`openxcvr`. It implements the cross-language facade in
[../README.md](../README.md#consumer-facade-contract): a canonical page map in,
a JSON-shaped dict out, plus SFF lane-signal flags and threshold decisions.

Modules under `openxcvr/_generated/` are private codegen outputs. Do not import
`openxcvr._generated` and do not edit those files by hand.

## Installation

The package is `openxcvr` (`pyproject.toml`, hatchling, Python >= 3.10, no
runtime dependencies). From the repository root:

```sh
pip install ./bindings/python
# local development:
pip install -e ./bindings/python
```

## Quickstart

Synthetic bytes only. CMIS lower memory byte 1 is the revision (`0x50` is
CMIS 5.0). Page `00h` bytes 1–16 are the vendor name; the facade strips
trailing spaces.

```python
from openxcvr import decode_cmis

lower = bytearray(128)
lower[1] = 0x50
page00h = bytearray(128)
page00h[1:17] = b"SYNTHETIC".ljust(16)

result = decode_cmis({"lower": bytes(lower), "00h": bytes(page00h)})
assert result["decode_status"] == "ok"
assert result["identity"]["vendor_name"] == "SYNTHETIC"
assert result["module"]["cmis_revision"] == "5.0"
```

## Usage

```python
from openxcvr import (
    decode_cmis,
    decode_sff8636,
    decode_sff8472,
    decode_sff8636_lane_signal_flags,
    decode_sff8472_lane_signal_flags,
    evaluate_sff8636_thresholds,
    evaluate_sff8472_thresholds,
)
```

Each decode takes `dict[str, bytes]` and returns a `dict`. Lane-flag functions
return `list[dict]`. Threshold functions return `dict`.

| Function | Required keys | Optional keys |
| --- | --- | --- |
| `decode_cmis` | `lower`, `00h` (>= 128 bytes once the major is admitted) | `01h`, `11h`, VDM `20h`–`27h`, `28h`–`2dh`, `2fh` |
| `decode_sff8636` | `lower`, `00h` | `03h` is accepted and unread |
| `decode_sff8472` | `a0h`, `a2h` (shorter buffers are zero-padded to 256) | none |
| `decode_sff8636_lane_signal_flags` | `lower` (short buffers are zero-padded to 128) | other SFF-8636 keys are allowed and unread |
| `decode_sff8472_lane_signal_flags` | `a0h`, `a2h` | none |
| `evaluate_sff8636_thresholds` | `lower`, `03h` (>= 128; never fabricated) | `00h` is allowed and unread |
| `evaluate_sff8472_thresholds` | `a0h`, `a2h` | none |

`decode_cmis` emits `lanes` and `flags` only when `11h` is present on a paged
module. Page `11h` without page `01h` raises `ValueError` (the Tx-bias
multiplier is on page `01h`). The `vdm` block is emitted only when the full
15-page set is supplied (`2eh` is not a key). An unknown CMIS major (not 3, 4,
or 5) returns `decode_status: "rejected"` with `rejection.reason`
`unknown_major_revision: 0xNN` and does not raise.

SFF-8636 lane records use 1-based `index` and include `rx_los`, `tx_disable`,
`tx_fault`, `tx_los`, `tx_cdr_lol`, `rx_cdr_lol`. `tx_adaptive_eq_fault` is
absent for compliance bytes `0x03`–`0x05` (revs 1.3–1.5). SFF-8472 has one
lane and no `tx_los` or CDR-LOL keys; `tx_adaptive_eq_fault` is absent unless
A0h byte 94 is `0x0A` (rev 12.5) or a compliance byte this family does not
admit. When A0h byte 92 bit 6 is clear, `decode_sff8472` sets `flags`,
`monitoring`, and `lanes` to `None`. Aggregate SFF-8472 telemetry is
uncalibrated; `evaluate_sff8472_thresholds` applies internal or external
calibration.

```python
lower = bytearray(128)
lower[1] = 0x07  # compliance 2.5: adaptive-EQ is modeled
lower[3] = 0x11  # lane 1 Rx LOS and Tx LOS
lower[86] = 0x01  # lane 1 Tx disable
lower[22:24] = b"\x02\x00"  # 2.0 C

page03h = bytearray(128)
page03h[0:8] = bytes([0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80])

lanes = decode_sff8636_lane_signal_flags({"lower": bytes(lower)})
assert lanes[0]["index"] == 1 and lanes[0]["rx_los"] is True

evaluation = evaluate_sff8636_thresholds({"lower": bytes(lower), "03h": bytes(page03h)})
assert evaluation["temperature"]["high_alarm"] is True
assert evaluation["temperature"]["high_warning"] is True
```

`evaluate_sff8636_thresholds` returns `temperature`, `supply_voltage`, and
per-lane lists `rx_power`, `tx_bias`, `tx_power`. Each value has `high_alarm`,
`low_alarm`, `high_warning`, `low_warning`. An all-zero threshold set does not
trip. `evaluate_sff8472_thresholds` returns the five A2h base monitors
(temperature, supply voltage, TX bias, TX power, RX power).

## Error handling

- Unknown canonical key: `ValueError` naming the family and key
  (`cmis pages: unknown key '2eh'`).
- Missing required key on decode or on the SFF-8472 helpers: `KeyError`.
- Admitted CMIS page shorter than 128 bytes, or SFF-8636 `lower`/`00h` shorter
  than 128: `ValueError` from the generated decoder (`expected >=128 bytes`).
  A short `lower` whose byte 1 is still an unknown major returns `rejected`
  instead of raising, because the revision check runs first.
- `evaluate_sff8636_thresholds` with missing or short `03h`: `ValueError`
  `sff-8636 page03h: expected >=128 bytes, got N`.
- SFF-8472 A0h byte 92 with neither calibration bit set, on threshold
  evaluation only: `ValueError`
  `sff-8472 A0h:92: neither internal nor external calibration set`.

## Regeneration

Contributors regenerate generated pages from the repository root. Do not edit
`openxcvr/_generated/`.

```sh
tools/regen-python.sh              # cmis, sff-8636, sff-8472
tools/regen-python.sh cmis         # one family
tools/regen-all.sh                 # every language, including this one
```

`tools/regen-python.sh` runs
`cargo run -p openxcvr-codegen -- generate --spec <spec> --lang python --out bindings/python/openxcvr/_generated`.
The handwritten facade (`openxcvr/__init__.py`, `_cmis.py`, `_sff8636.py`,
`_sff8472.py`, `_common.py`) is not generated.

```sh
pytest bindings/python/tests
```

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
