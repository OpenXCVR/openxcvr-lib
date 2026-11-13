# Bindings

Generated and hand-maintained binding surfaces emitted from the OpenXCVR schema
model. See [../README.md](../README.md) for library-wide validation commands.

Each language guide uses the same sections: Overview, Installation, Quickstart,
Usage, Error handling, Regeneration. The guides document the facade that is
actually implemented. They do not invent a canonical page map, lane-flag
entry point, or threshold evaluator for a language that does not have one.

## Regeneration

From the repository root:

```sh
tools/regen-all.sh                         # cmis sff-8636 sff-8472
tools/regen-all.sh sff-8472                 # one family
tools/regen-all.sh cmis sff-8636            # selected families, in this order
```

[`tools/regen-all.sh`](../tools/regen-all.sh) accepts `cmis`, `sff-8636`,
`sff-8472`, and `cmis-lt-1.0`; no arguments selects `cmis`, `sff-8636`, and
`sff-8472` in that order. It validates the entire argument list before running
any generator. An unknown argument (even after a valid one) prints an error
listing `cmis | sff-8636 | sff-8472` to stderr and exits nonzero without
regeneration. Any child failure stops the script immediately with a nonzero
exit; earlier children may already have written output, so this is not an
atomic operation.

The script runs the existing Rust, Python, TypeScript, Java, C#, OpenAPI, and
Markdown regeneration scripts, then the codegen CLI for C, Go, and PHP per
spec. Those last three outputs go to `bindings/c/generated`,
`bindings/go/generated`, and `bindings/php/src/Generated`. Existing formatting
is preserved, including Rust's per-crate `cargo fmt`, which can also format
handwritten source. Paths are resolved from the script location, so invoking
it by absolute path from outside the repository works too. Cargo and the
existing Rust toolchain are required, as for the individual regeneration
scripts.

This is a development regeneration convenience, not a qualification or
capability-admission receipt. Run it in a clean, isolated worktree and review
the complete working-state diff, including handwritten files; do not hand-edit
generated output or update goldens to hide unexpected drift. It does not replace
the project's separate qualification, native lifecycle or delivery gates.

## Surfaces

- [c/README.md](c/README.md) — C binding.
- [csharp/README.md](csharp/README.md) — C# binding.
- [go/README.md](go/README.md) — Go binding.
- [java/README.md](java/README.md) — Java binding.
- [openapi/README.md](openapi/README.md) — OpenAPI description (not a language facade).
- [php/README.md](php/README.md) — PHP binding.
- [python/README.md](python/README.md) — Python binding.
- [typescript/README.md](typescript/README.md) — TypeScript binding.

## Consumer facade contract

One logical facade, spelled in each language's idiom. Python, Go, and
TypeScript implement it. C, C#, Java, and PHP do not yet (see Remainder).
Generated page decoders stay underneath this facade; do not call them as the
cross-language API.

### Operations

| Operation | Python | Go | TypeScript |
| --- | --- | --- | --- |
| Decode a family | `decode_cmis`, `decode_sff8636`, `decode_sff8472` | `DecodeCmis`, `DecodeSff8636`, `DecodeSff8472` | `decodeCmis`, `decodeSff8636`, `decodeSff8472` |
| Lane signal flags | `decode_sff8636_lane_signal_flags`, `decode_sff8472_lane_signal_flags` | `DecodeSff8636LaneSignalFlags`, `DecodeSff8472LaneSignalFlags` | `decodeSff8636LaneSignalFlags`, `decodeSff8472LaneSignalFlags` |
| Threshold decision | `evaluate_sff8636_thresholds`, `evaluate_sff8472_thresholds` | `EvaluateSff8636Thresholds`, `EvaluateSff8472Thresholds` | `evaluateSff8636Thresholds`, `evaluateSff8472Thresholds` |

CMIS lane status and VDM threshold quads stay inside `decode` when the
optional pages are supplied. There is no separate CMIS lane-flag or
threshold-evaluation entry point yet.

### Input shape

One page map. Canonical keys are the wire tags the Python facade already
uses. A missing key is an absent page. A present buffer, including an empty
one, is supplied and is never rewritten into absence. An unknown key fails
closed.

| Family | Required keys | Optional keys |
| --- | --- | --- |
| CMIS | `lower`, `00h` | `01h`, `11h`, `20h`–`27h`, `28h`–`2dh`, `2fh` (`2eh` is not a facade key) |
| SFF-8636 | `lower`, `00h` | `03h` (required by threshold evaluation, unread by decode) |
| SFF-8472 | `a0h`, `a2h` | none |

Go and TypeScript keep their typed page structs (`Pages`, `Sff8636Pages`,
`Sff8472Pages`, `CmisPages`). Build those structs from a canonical map with
`CmisPagesFromCanonical` / `Sff8636PagesFromCanonical` /
`Sff8472PagesFromCanonical` (Go) and `cmisPagesFromCanonical` /
`sff8636PagesFromCanonical` / `sff8472PagesFromCanonical` (TypeScript).
Python takes the map directly.

Optional-page rule: the facade does not invent a missing optional page.
SFF-8636 page `03h` is never zero-padded; a missing or short buffer is an
error, because a zero page would silently disable every threshold.
SFF-8636 `lower` and SFF-8472 `a0h`/`a2h` that are present but short are
still zero-padded to the decoder width, matching the existing Python and Go
decoders. A missing required key is an error from the canonical adapter
before that pad can run.

### Result and error shape

- Decode returns the shared JSON document (`decode_status` `ok` or, for an
  unsupported CMIS major revision, `rejected` plus `rejection`). It does not
  throw for an unknown revision.
- Lane flags return a list of records. `index` is **1-based**. Flags the
  revision does not model are absent, not `false`. SFF-8472 has no `tx_los`
  and no CDR-LOL fields. Rust [`LaneStatus::index`](../crates/openxcvr-core/src/types.rs)
  is **1-based** too, matching the binding JSON — the adapters, the goldens,
  and the facade all use the same lane basis.
- Threshold evaluation returns `high_alarm`, `low_alarm`, `high_warning`,
  `low_warning` booleans. An all-zero threshold set does not trip. Bounds are
  inclusive.
- A missing required page, a short page `03h`, an unknown canonical key, or
  SFF-8472 with neither calibration bit set is a language error that names
  the page or key (Python `ValueError`/`KeyError`, Go `error`, TypeScript
  `TypeError`/`RangeError`). It is not a successful document.

Runnable synthetic examples (no vendor captures):
`bindings/python/tests/test_consumer_synthetic_example.py`,
`bindings/go/consumer_synthetic_example_test.go`,
`bindings/typescript/tests/facade_signals.test.mjs`.

The Rust consumer test kit (`openxcvr_core::testkit`: `MockModule`,
`PageBuilder`, `assert_lanes`, `assert_close`) is for callers testing their
own `Module` code without hardware. It returns stored values only.

### Remainder

Not aligned to this contract yet:

- **C** — `openxcvr_decode_cmis_json`, `openxcvr_decode_sff8636_json`,
  `openxcvr_decode_sff8472_json` take loaded structs and a status code plus
  an output buffer. No lane-flag or threshold-evaluation entry points, and
  no canonical page map.
- **C#** — `OpenXcvr.Decoder.DecodeCmis` / `DecodeSff8636` / `DecodeSff8472`
  take positional `byte[]` arguments. No lane-flag or threshold entry points.
- **Java** — `com.openxcvr.Openxcvr.decodeCmis` / `decodeSff8636` /
  `decodeSff8472` take positional `byte[]` arguments. No lane-flag or
  threshold entry points.
- **PHP** — `Openxcvr\OpenxcvrDecoder::decodeCmis(string $lower, string $page00h, string $page01h = '', string $page11h = '', array $vdmPages = [])`,
  `decodeSff8636(string $lower, string $page00h)`,
  `decodeSff8472(string $a0h, string $a2h)`. An empty `page01h` or `page11h`
  means that page is absent. No lane-flag or threshold entry points, and no
  canonical page map.
- **CMIS** — dedicated lane-flag and threshold-evaluation functions, in
  every language, including Python, Go, and TypeScript.

## See also

- [Consumer guide](../docs/consumer-guide.md)
- [Documentation map](../docs/README.md)
- [Glossary](../docs/glossary.md)
