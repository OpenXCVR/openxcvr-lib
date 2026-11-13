# openxcvr C# bindings

C# binding: generated page records emitted from the same schemas as the Rust crates, with a hand-written `OpenXcvr.Decoder` facade on top.

## Overview

C# facade for CMIS, SFF-8636, and SFF-8472. `OpenXcvr.Decoder`
(`Decoder.cs`) is the handwritten aggregate decoder over generated page
records in namespace `OpenXcvr.Generated`. It does **not** implement the
canonical page map, lane-flag functions, or threshold evaluators. Those
remain Python, Go, and TypeScript only; see
[../README.md](../README.md#consumer-facade-contract).

Do not edit `generated/`.

## Installation

`OpenXcvr.Transceivers.csproj` targets `netstandard2.1` (C# 9, nullable).
Package id `OpenXcvr.Transceivers`, version `0.2.0`. No NuGet runtime
dependencies.

Build or pack from the repository (this is not a nuget.org package):

```sh
dotnet build bindings/csharp/OpenXcvr.Transceivers.csproj
dotnet pack bindings/csharp/OpenXcvr.Transceivers.csproj -c Release -o /path/to/feed
```

A consumer then uses a local feed only, as
`tests/consumer-check/nuget.config` does:

```xml
<PackageReference Include="OpenXcvr.Transceivers" Version="0.2.0" />
```

`tests/consumer-check/run-consumer-check.sh` packs that nupkg and runs a
net8.0 consumer against it. `.github/workflows/parity.yml` runs `dotnet build`
and `dotnet test` on CI with the .NET 8 SDK. The quickstart below was checked
against `Decoder.cs` on a host without `dotnet`; it was not executed here.

## Quickstart

Synthetic bytes only. Lower byte 1 `0x50` is CMIS 5.0. Page `00h` bytes 1–16
are the vendor name. Omit optional pages (they default to null).

```csharp
using System.Text;
using OpenXcvr;

var lower = new byte[128];
lower[1] = 0x50;
var page00h = new byte[128];
Array.Fill(page00h, (byte)' ', 1, 16);
Encoding.ASCII.GetBytes("SYNTHETIC").CopyTo(page00h.AsSpan(1));

var result = Decoder.DecodeCmis(lower, page00h);
if (!Equals(result["decode_status"], "ok"))
{
    throw new InvalidOperationException(result["rejection"]?.ToString());
}
var identity = (Dictionary<string, object?>)result["identity"]!;
var module = (Dictionary<string, object?>)result["module"]!;
if (!Equals(identity["vendor_name"], "SYNTHETIC") || !Equals(module["cmis_revision"], "5.0"))
{
    throw new InvalidOperationException("synthetic CMIS decode mismatch");
}
Console.WriteLine($"{identity["vendor_name"]} {module["cmis_revision"]}");
```

Verified by inspection: `DecodeCmis` copies vendor name from
`CmisPage00hAdministrative.VendorName` (page `00h` bytes 1–16, trailing spaces
trimmed by the generated record). CI runs `dotnet test`.

## Usage

```csharp
Dictionary<string, object?> Decoder.DecodeCmis(
    byte[] lower, byte[] page00h,
    byte[]? page01h = null, byte[]? page11h = null,
    Dictionary<string, byte[]>? vdmPages = null);
Dictionary<string, object?> Decoder.DecodeSff8636(byte[] lower, byte[] page00h);
Dictionary<string, object?> Decoder.DecodeSff8472(byte[] a0h, byte[] a2h);
```

Results are `Dictionary<string, object?>` trees with `List<object?>` for
arrays. They serialize to the shared golden document.

CMIS and SFF-8636 pages must be at least 128 bytes; longer buffers are
truncated to 128. SFF-8472 pages of any length, including empty, are
zero-padded to 256 and every caller byte is kept. An unknown CMIS major
(not 3, 4, or 5) returns `decode_status` `"rejected"` and `rejection.reason`
`unknown_major_revision: 0xNN` and does not throw. Page `11h` without page
`01h` throws. `vdmPages` uses keys `"20h"`..`"2fh"` excluding `"2eh"`; the
`vdm` entry exists only for the complete set.

`DecodeSff8472` telemetry is uncalibrated. Check
`calibration.externally_calibrated`. There is no C# lane-flag or
threshold-evaluation method. Lane objects inside the SFF documents use
1-based `index`. When A0h byte 92 bit 6 is clear, `flags`, `monitoring`, and
`lanes` are null.

These decoders admit CMIS by major revision and decode flat-union page fields.
They are not a revision-qualified facade.

## Error handling

Input-shape failures throw `ArgumentException` (not a successful document):

- null page: `lower is required`, `page00h is required`, `a0h is required`,
  `a2h is required`, or `<name> must not be null` for a non-null-typed check
- CMIS or SFF-8636 page shorter than 128: `<name> must be at least 128 bytes, got N`
  (this runs before the CMIS major check, so a short `lower` does not return
  `rejected`)
- page `11h` present and page `01h` absent

`DecodeSff8472` does not throw for a short buffer. Unknown CMIS major on a
full `lower` page does not throw.

## Regeneration

From the repository root. Do not edit `generated/`.

```sh
tools/regen-csharp.sh              # cmis, sff-8636, sff-8472
tools/regen-csharp.sh cmis
tools/regen-all.sh
```

`tools/regen-csharp.sh` runs
`cargo run -p openxcvr-codegen -- generate --spec <spec> --lang csharp --out bindings/csharp/generated`.
`Decoder.cs` is handwritten.

```sh
dotnet test tests/OpenXcvr.Transceivers.Tests/OpenXcvr.Transceivers.Tests.csproj
```

Run that from `bindings/csharp`. The test project targets `net8.0`.

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
