# openxcvr Go bindings

Go binding: generated page decoders emitted from the same schemas as the Rust crates, with a hand-written facade on top.

## Overview

Go facade for CMIS, SFF-8636, and SFF-8472. The module implements the
cross-language contract in [../README.md](../README.md#consumer-facade-contract):
typed page structs, canonical-map adapters, aggregate decode, SFF lane-signal
flags, and SFF threshold decisions.

`generated/` is codegen output (`package generated`). Do not edit it and do
not treat those page decoders as the cross-language API.

## Installation

`go.mod` declares:

```
module github.com/OpenXCVR/openxcvr-lib/bindings/go
```

```go
import "github.com/OpenXCVR/openxcvr-lib/bindings/go"
```

This module is consumed from the repository, not a published version tag.
Point a consumer at a checkout with a `replace` directive:

```
replace github.com/OpenXCVR/openxcvr-lib/bindings/go => /path/to/openxcvr-lib/bindings/go
```

Go 1.21 or newer. The module has no third-party dependencies.

## Quickstart

Synthetic bytes only. `0x50` in lower memory byte 1 is CMIS 5.0. Page `00h`
bytes 1–16 are the vendor name.

```go
package main

import (
	"fmt"

	openxcvr "github.com/OpenXCVR/openxcvr-lib/bindings/go"
)

func main() {
	lower := make([]byte, 128)
	lower[1] = 0x50
	page00h := make([]byte, 128)
	for i := 1; i < 17; i++ {
		page00h[i] = ' '
	}
	copy(page00h[1:], []byte("SYNTHETIC"))

	pages, err := openxcvr.CmisPagesFromCanonical(map[string][]byte{
		"lower": lower,
		"00h":   page00h,
	})
	if err != nil {
		panic(err)
	}
	doc, err := openxcvr.DecodeCmis(pages)
	if err != nil {
		panic(err)
	}
	if doc["decode_status"] != "ok" {
		panic(doc["rejection"])
	}
	identity := doc["identity"].(map[string]interface{})
	module := doc["module"].(map[string]interface{})
	fmt.Println(identity["vendor_name"], module["cmis_revision"])
}
```

That program prints `SYNTHETIC 5.0`.

## Usage

Canonical adapters (`canonical.go`) fail closed on a nil map, an unknown key,
or a missing required key. A present key whose slice is nil stays present
(empty), distinct from an omitted optional page.

```go
pages, err := openxcvr.CmisPagesFromCanonical(map[string][]byte{ /* ... */ })
sff8636, err := openxcvr.Sff8636PagesFromCanonical(map[string][]byte{ /* ... */ })
sff8472, err := openxcvr.Sff8472PagesFromCanonical(map[string][]byte{ /* ... */ })
```

| Function | Input | Result |
| --- | --- | --- |
| `DecodeCmis(Pages) (map[string]interface{}, error)` | `Lower` and `Page00h` required; `Page01h`, `Page11h`, `Page20h`–`Page2Fh` except `Page2Eh` optional | JSON-shaped map. `vdm` only when every VDM page is non-nil |
| `DecodeSff8636(Sff8636Pages) (map[string]interface{}, error)` | `Lower`, `Page00h`. `Page03h` is unread | ok-only document |
| `DecodeSff8472(Sff8472Pages) (map[string]interface{}, error)` | `A0h`, `A2h`, zero-padded to 256 when short | ok-only document. `flags` / `monitoring` / `lanes` are nil when A0h byte 92 bit 6 is clear |
| `DecodeSff8636LaneSignalFlags(Sff8636Pages) ([]map[string]interface{}, error)` | `Lower` (short slices are padded to 128) | four lanes, `index` 1-based |
| `DecodeSff8472LaneSignalFlags(Sff8472Pages) ([]map[string]interface{}, error)` | `A0h`, `A2h` | one lane. No `tx_los` and no CDR-LOL keys |
| `EvaluateSff8636Thresholds(Sff8636Pages) (map[string]interface{}, error)` | `Lower` plus caller-supplied `Page03h` of at least 128 bytes | `temperature`, `supply_voltage`, `rx_power`, `tx_bias`, `tx_power` |
| `EvaluateSff8472Thresholds(Sff8472Pages) (map[string]interface{}, error)` | `A0h`, `A2h` | five A2h monitors, calibration applied |

`DecodeCmis` returns `(doc, nil)` with `decode_status` `"rejected"` and
`rejection.reason` `unknown_major_revision: 0xNN` when lower byte 1 is not
major 3, 4, or 5. That is not an `error`. Page `11h` without page `01h` is an
error. Aggregate SFF-8472 telemetry in `DecodeSff8472` is uncalibrated;
`EvaluateSff8472Thresholds` applies the internal or external calibration terms.

Lane-flag and threshold shapes match the Python facade, including absent
`tx_adaptive_eq_fault` below the revision that models it (SFF-8636 compliance
`0x03`–`0x05`, SFF-8472 compliance other than `0x0A` when the byte is
admitted). An all-zero threshold set does not trip.

```go
lower := make([]byte, 128)
lower[1] = 0x07
lower[3] = 0x11
lower[86] = 0x01
lower[22], lower[23] = 0x02, 0x00
page03h := make([]byte, 128)
copy(page03h[:8], []byte{0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80})

typed, err := openxcvr.Sff8636PagesFromCanonical(map[string][]byte{
	"lower": lower,
	"00h":   make([]byte, 128),
	"03h":   page03h,
})
lanes, err := openxcvr.DecodeSff8636LaneSignalFlags(typed)
eval, err := openxcvr.EvaluateSff8636Thresholds(typed)
_ = lanes[0]["index"] // 1
_ = eval["temperature"].(map[string]interface{})["high_alarm"]
```

## Error handling

Adapter errors are plain `error` values:

- `cmis pages: map is nil` (also `sff-8636` / `sff-8472`)
- `cmis pages: unknown key "2eh"`
- `cmis pages: missing required key "lower"`

`DecodeCmis` returns an error for `lower` shorter than 128 (`lower page too short: N bytes`) before the revision check, and for a short optional `Page01h` or `Page11h`. A short `Page00h` is `page00h: expected >=128 bytes, got N`. `DecodeSff8636` wraps the same generated length error as `sff-8636 lower:` / `sff-8636 page00h:`. `EvaluateSff8636Thresholds` returns `sff-8636 page03h:` when `Page03h` is nil or shorter than 128; it does not zero-fill. `EvaluateSff8472Thresholds` returns an error when A0h byte 92 sets neither calibration bit.

## Regeneration

From the repository root. Do not edit `generated/`.

```sh
tools/regen-all.sh cmis          # includes Go; there is no regen-go.sh
cargo run -p openxcvr-codegen -- generate --spec cmis --lang go --out bindings/go/generated
gofmt -w bindings/go/generated
```

`openxcvr.go`, `cmis.go`, `sff8636.go`, `sff8472.go`, `canonical.go`,
`sff_lane_flags.go`, and `sff_thresholds.go` are handwritten.

```sh
go test ./bindings/go/...
```

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
