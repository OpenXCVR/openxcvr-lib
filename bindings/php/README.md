# openxcvr PHP bindings

PHP binding: generated page classes emitted from the same schemas as the Rust crates, with a hand-written `Openxcvr\OpenxcvrDecoder` facade on top.

## Overview

PHP 8.2 facade for CMIS, SFF-8636, and SFF-8472. The class is
`Openxcvr\OpenxcvrDecoder` (`src/OpenxcvrDecoder.php`), which pulls in
`OpenxcvrCmisDecoder`, `OpenxcvrSff8636Decoder`, and `OpenxcvrSff8472Decoder`.
It does **not** implement the canonical page map, lane-flag functions, or
threshold evaluators. Those remain Python, Go, and TypeScript only; see
[../README.md](../README.md#consumer-facade-contract).

`src/Generated/` is codegen (namespace `Openxcvr\Generated`). Do not edit it
and do not call generated page classes as the cross-language API.

## Installation

Composer package name `petabitscale/openxcvr` (`composer.json`). This
repository does not publish it; consume the checkout. Require PHP `^8.2`.
There are no runtime Composer dependencies; `phpunit/phpunit` ^11 is
`require-dev` only.

From a consumer project, use a path repository:

```json
{
  "repositories": [
    { "type": "path", "url": "../openxcvr-lib/bindings/php", "options": { "symlink": false } }
  ],
  "require": { "petabitscale/openxcvr": "*" }
}
```

For development inside this directory: `composer install`.

PSR-4 maps `Openxcvr\` to `src/`.

## Quickstart

Synthetic bytes only. Lower byte 1 `0x50` is CMIS 5.0. Page `00h` bytes 1–16
are the vendor name; trailing spaces are stripped. Empty strings for the
optional pages mean those pages are absent.

```php
<?php
declare(strict_types=1);

require __DIR__ . '/vendor/autoload.php';

use Openxcvr\OpenxcvrDecoder;

$lower = str_repeat("\0", 128);
$lower[1] = "\x50";
$page00h = str_repeat("\0", 128);
$page00h = substr_replace($page00h, str_pad('SYNTHETIC', 16), 1, 16);

$result = OpenxcvrDecoder::decodeCmis($lower, $page00h);
assert($result['decode_status'] === 'ok');
assert($result['identity']['vendor_name'] === 'SYNTHETIC');
assert($result['module']['cmis_revision'] === '5.0');
```

Run that file with `php` from `bindings/php` after `composer install` (or
`composer dump-autoload`).

## Usage

All three entry points are static and return an `array` shaped like the shared
JSON document.

```php
OpenxcvrDecoder::decodeCmis(
    string $lower,
    string $page00h,
    string $page01h = '',
    string $page11h = '',
    array $vdmPages = []
): array;

OpenxcvrDecoder::decodeSff8636(string $lower, string $page00h): array;
OpenxcvrDecoder::decodeSff8472(string $a0h, string $a2h): array;
```

`decodeCmis` requires `lower` of at least 128 bytes. `page01h` and `page11h`
shorter than 128, including `''`, are absent. `lanes` and `flags` are decoded
only when `page11h` is at least 128 bytes and the module is not flat memory.
`$vdmPages` is keyed `'20h'`..`'2fh'` excluding `'2eh'`; the `vdm` element is
present only when every page of that set is supplied. An unknown CMIS major
(not 3, 4, or 5) returns `decode_status` `'rejected'` and `rejection.reason`
`unknown_major_revision: 0xNN` without throwing.

`decodeSff8636` reads lower memory and upper page `00h` (each at least 128
bytes via the generated decoders) and always returns `decode_status` `'ok'`.
`decodeSff8472` zero-pads `a0h` and `a2h` to 256 bytes. When A0h byte 92 bit 6
is clear, `flags`, `monitoring`, and `lanes` are `null`. Aggregate SFF-8472
telemetry is uncalibrated: external calibration constants are not applied.
There is no PHP threshold-evaluation function.

There is no separate lane-flag API. Lane objects inside `decodeSff8636` and
`decodeSff8472` use 1-based `index`.

## Error handling

- `lower` shorter than 128 in `decodeCmis`: `InvalidArgumentException`
  `lower page too short: N bytes`, before the revision check.
- An admitted `page00h` / SFF page shorter than the generated decoder width:
  `InvalidArgumentException` from `Openxcvr\Generated\...::decode`
  (`requires 128 bytes, got N`).
- Page `11h` of at least 128 bytes with `page01h` shorter than 128:
  `RuntimeException` `page 11h present but page 01h missing (Tx-bias multiplier)`.
- Unknown CMIS major, once `lower` is at least 128 bytes: not an exception.
  `rejection.reason` is `unknown_major_revision: 0xNN`.
- `decodeSff8472` does not throw for a short page; it zero-pads.

Unknown page-map keys are not a PHP concern: the facade takes positional
strings, not a map.

## Regeneration

From the repository root. Do not edit `src/Generated/`.

```sh
tools/regen-all.sh cmis
cargo run -p openxcvr-codegen -- generate --spec cmis --lang php \
    --out bindings/php/src/Generated
```

There is no `tools/regen-php.sh`. `OpenxcvrDecoder.php` and the three decoder
traits are handwritten.

CI runs `cd bindings/php && ./vendor/bin/phpunit` (see
`.github/workflows/parity.yml`).

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
