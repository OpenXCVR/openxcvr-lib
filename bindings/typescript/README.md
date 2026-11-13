# openxcvr TypeScript bindings

TypeScript binding: generated page modules emitted from the same schemas as the Rust crates, with a hand-written facade in `src/` on top.

## Overview

TypeScript facade for CMIS, SFF-8636, and SFF-8472, package
`@openxcvr/transceivers`. It implements the cross-language contract in
[../README.md](../README.md#consumer-facade-contract): typed page objects,
canonical-map adapters, aggregate decode, SFF lane-signal flags, and SFF
threshold decisions.

`generated/` is codegen (one module per page, `enums/<name>.ts`, and
`generated/openapi/cmis.ts`). `index.ts` re-exports that surface plus
`src/cmis.ts`, `src/sff8636.ts`, `src/sff8472.ts`, `src/sff_signals.ts`, and
`src/canonical.ts`. Do not edit `generated/`.

## Installation

The package is `"private": true` and is not published to the public npm
registry. Do not `npm install @openxcvr/transceivers` against public npm.
Build it, then depend on the directory:

```sh
cd bindings/typescript
npm install
npm run build    # tsc → dist/ (the export map points at dist/)
```

In the consumer `package.json`:

```json
{
  "dependencies": {
    "@openxcvr/transceivers": "file:../openxcvr-lib/bindings/typescript"
  }
}
```

The package is ESM only (`"type": "module"`). `require()` fails with
`ERR_PACKAGE_PATH_NOT_EXPORTED`. Node ESM or an ESM bundler is required.
`devDependencies` pin `typescript` ^5.4 and `openapi-typescript` 7.13.0.

## Quickstart

Synthetic bytes only. Lower byte 1 `0x50` is CMIS 5.0. Page `00h` bytes 1–16
are the vendor name.

```ts
import { cmisPagesFromCanonical, decodeCmis } from "@openxcvr/transceivers";

const lower = new Uint8Array(128);
lower[1] = 0x50;
const page00h = new Uint8Array(128);
page00h.fill(0x20, 1, 17);
page00h.set(new TextEncoder().encode("SYNTHETIC"), 1);

const result = decodeCmis(cmisPagesFromCanonical({ lower, "00h": page00h }));
if (result.decode_status !== "ok") {
  throw new Error(result.rejection.reason);
}
console.log(result.identity.vendor_name, result.module.cmis_revision);
```

That prints `SYNTHETIC 5.0`.

## Usage

| Function | Input | Result |
| --- | --- | --- |
| `cmisPagesFromCanonical` / `sff8636PagesFromCanonical` / `sff8472PagesFromCanonical` | `CanonicalPages` (`Record<string, Uint8Array \| undefined>`) | `CmisPages`, `Sff8636Pages`, `Sff8472Pages` |
| `decodeCmis(pages: CmisPages): CmisDecodeResult` | `lower`, `page00h`; optional `page01h`, `page11h`, `page20h`–`page2fh` (no `page2eh`) | `decode_status` `"ok"` or `"rejected"` |
| `decodeSff8636(pages: Sff8636Pages): Sff8636DecodeResult` | `lower`, `page00h`; `page03h` is unread | ok-only |
| `decodeSff8472(pages: Sff8472Pages): Sff8472DecodeResult` | `a0h`, `a2h` | ok-only. Short pages are zero-padded to 256 |
| `decodeSff8636LaneSignalFlags` / `decodeSff8472LaneSignalFlags` | the same page objects | lane records, `index` 1-based |
| `evaluateSff8636Thresholds` / `evaluateSff8472Thresholds` | `page03h` must be a `Uint8Array` of at least 128 bytes for SFF-8636 | `{high_alarm, low_alarm, high_warning, low_warning}` per monitor |

`decodeCmis` checks length before the revision test: a `lower` or `page00h`
shorter than 128 throws `RangeError` and never returns `rejected`. An unknown
major on a full page returns `rejection.reason` `unknown_major_revision: 0xNN`
and does not throw. `lanes` / `flags` need `page11h` on a paged module. The
`vdm` block needs the complete 15-page set.

SFF-8636 lane records include `tx_los`, `tx_cdr_lol`, and `rx_cdr_lol`.
`tx_adaptive_eq_fault` is omitted for compliance bytes `0x03`–`0x05`.
SFF-8472 omits `tx_los` and both CDR-LOL fields; `tx_adaptive_eq_fault` is
omitted unless A0h byte 94 is `0x0A` or a compliance byte this family does not
admit. A short `lower` passed to `decodeSff8636LaneSignalFlags` is zero-padded
to 128 (`checkPagePadded`); `decodeSff8636` itself is fail-closed under 128
bytes. Aggregate `decodeSff8472` telemetry is uncalibrated (see
`calibration.externally_calibrated`). `evaluateSff8472Thresholds` applies
internal or external calibration. An all-zero threshold set does not trip.
When A0h byte 92 bit 6 is clear, `flags`, `monitoring`, and `lanes` are null.

```ts
import {
  decodeSff8636LaneSignalFlags,
  evaluateSff8636Thresholds,
  sff8636PagesFromCanonical,
} from "@openxcvr/transceivers";

const lower = new Uint8Array(128);
lower[1] = 0x07;
lower[3] = 0x11;
lower[86] = 0x01;
lower[22] = 0x02;
const page03h = new Uint8Array(128);
page03h.set([0x01, 0x00, 0xff, 0x00, 0x00, 0x80, 0xff, 0x80]);

const pages = sff8636PagesFromCanonical({
  lower,
  "00h": new Uint8Array(128),
  "03h": page03h,
});
const lanes = decodeSff8636LaneSignalFlags(pages);
const evaluation = evaluateSff8636Thresholds(pages);
// lanes[0].index === 1, lanes[0].rx_los === true
// evaluation.temperature.high_alarm === true
```

`Sff8636DecodeResult` and `Sff8472DecodeResult` alias OpenAPI components.
`CmisDecodeResult` is the generated component refined in `src/types.ts`.
`npm test` typechecks that contract with `tsc -p tsconfig.test.json` before
the Node tests.

## Error handling

Canonical adapters throw `TypeError`:

- `cmis pages must be an object` (also `sff-8636` / `sff-8472`)
- `cmis pages: missing required key "lower"`
- `cmis pages: unknown key "2eh"`
- `<key> must be a Uint8Array`

`decodeCmis` / `decodeSff8636` throw `TypeError` when a required page is
missing or not a `Uint8Array`, and `RangeError` when it is shorter than 128
(`lower must be at least 128 bytes, got N`). `evaluateSff8636Thresholds`
throws `RangeError` `sff-8636 page03h: expected >=128 bytes, got N` and does
not zero-fill. `evaluateSff8472Thresholds` throws `RangeError`
`sff-8472 A0h:92: neither internal nor external calibration set` when A0h byte 92
sets neither calibration bit. None of those are a successful document.

## Regeneration

From the repository root. Do not edit `generated/`.

```sh
tools/regen-typescript.sh          # all specs, then OpenAPI → generated/openapi/cmis.ts
tools/regen-typescript.sh cmis     # one spec; still refreshes generated/openapi/cmis.ts
tools/regen-all.sh                 # includes that OpenAPI → TypeScript step
```

The script runs
`cargo run -p openxcvr-codegen -- generate --spec <spec> --lang typescript --out bindings/typescript/generated`,
then `npm run gen:types` (`openapi-typescript` 7.13.0) to rewrite
`generated/openapi/cmis.ts` from `bindings/openapi/openxcvr.openapi.yaml`.
If the pinned binary is missing it runs `npm install` here; if `npm` itself
is missing, regeneration fails instead of skipping the file. `src/` and
`index.ts` are handwritten.

```sh
npm test    # build + tsc -p tsconfig.test.json + node --test tests/*.test.mjs
```

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
