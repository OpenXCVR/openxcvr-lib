# openxcvr-lib

A schema-first library for pluggable-transceiver memory maps. It decodes the
management memory of SFP, QSFP and QSFP-DD/OSFP class modules — SFF-8472,
SFF-8636 and CMIS — into typed values, and answers planning questions about a
module: which revision it reports, which fields exist at that revision, and
which byte ranges a read needs.

One set of YAML schemas is the source. A code generator turns it into the Rust
crates, eight language bindings and a browsable field reference, so every
surface describes the same bytes the same way.

The library parses and decides. It does not open a device, retry a transfer or
schedule I/O on its own initiative: the caller owns the bus and hands the
library a transport, or the bytes it has already read.

New here? Run the example below, then follow the
[documentation map](docs/README.md).

## Try it

No hardware and no fixture files are needed. With Rust 1.82 or newer, from any
directory of a checkout:

```sh
cargo run -p openxcvr --example zero-setup
```

```text
source: embedded synthetic CMIS 5.2 QSFP-DD (tools/build-cmis-5-2-synthetic.py)
vendor: CMIS_5_2_SYNTH
part: FIXTURE-SYNTH-V1
serial: SYNTHETIC0000001
vendor rev: 00
spec revision: CMIS 5.2 (lower byte 1 = 0x52)
identifier: 0x18
```

Every decode starts the same way: route the identifier byte to a family and
read the revision the module itself reports. In your own program:

```rust
use openxcvr::core::transport::BufTransport;
use openxcvr::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lower = vec![0u8; 128]; // lower memory, as read from a module
    lower[0] = 0x18; // SFF-8024 identifier: QSFP-DD
    lower[1] = 0x52; // revision byte: CMIS 5.2

    let mut transport = BufTransport::from_slice(&lower);
    let detected = detect(&mut transport)?;
    assert_eq!(detected.kind, SpecKind::Cmis);
    assert_eq!(detected.rev, Rev::CMIS_5_2);
    println!("{:?} revision {}", detected.kind, detected.rev.as_str());
    Ok(())
}
```

`BufTransport` stands in for a bus here; a hardware backend implements the same
`Transport` trait. The crates are not published to a registry, so depend on
them by path ([installation](docs/guides/installation.md)). The
[quickstart](docs/guides/quickstart.md) explains the example run above.

## What it supports

| Family | Modules | Revision handling |
| --- | --- | --- |
| CMIS | QSFP-DD, OSFP, DSFP and the other CMIS form factors | Detects CMIS 3.0, 4.0 and 5.0–5.4 from lower-memory byte 1. |
| SFF-8636 | QSFP+, QSFP28, and QSFP-DD/OSFP modules that carry the legacy map | Detects the compliance code in lower-memory byte 1. |
| SFF-8472 | SFP, SFP+, SFP28 (addresses A0h and A2h) | Detects the compliance code in A0h byte 94. |

Detection fails closed. An unknown identifier, an unknown revision, or an
unspecified (`0x00`) compliance code is an error; the library never substitutes
a later revision. The exact routing and revision rules are in
[concepts](docs/guides/concepts.md).

The schemas also describe C-CMIS, ELSFP, CMIS-LT and CMIS-VCS. Those appear in
the [generated reference](docs/reference/index.md), but the unified module
handle does not open them.

Bindings generated from the same schemas: C, C#, Go, Java, OpenAPI, PHP,
Python and TypeScript, indexed in [bindings/README.md](bindings/README.md).

## Where to go next

| You want to | Start at |
| --- | --- |
| Decode a module from Rust | [Quickstart](docs/guides/quickstart.md), then the [API guide](docs/guides/api-guide.md) |
| Follow the guides in order | [Consumer guide](docs/consumer-guide.md) |
| Solve one specific task | [Task cookbook](docs/cookbook.md) |
| Use another language | [Bindings](bindings/README.md) |
| Look up a field, page or enum | [Generated reference](docs/reference/index.md) |
| Understand an error | [Error handling](docs/guides/error-handling.md) and the [error catalog](docs/error-catalog.md) |
| Understand how the library is built | [Architecture](docs/architecture.md) |
| Change a schema, the generator or a page | [Contributing](docs/contributing.md) |
| See every page | [Documentation map](docs/README.md) |

## Repository layout

| Path | Contents |
| --- | --- |
| [`schemas/`](schemas/README.md) | Authored YAML schemas and enum registries: the source of every generated surface. |
| [`crates/`](crates/openxcvr/README.md) | The Rust workspace: core types, one adapter per family, transports, the code generator, and the `openxcvr` front-door crate. |
| [`bindings/`](bindings/README.md) | Generated binding surfaces and their hand-written facades. |
| [`docs/`](docs/README.md) | Guides, cookbook, architecture, and the generated reference. |
| [`tests/fixtures/`](tests/fixtures/README.md) | EEPROM fixture corpus with source notes. |
| [`tests/golden/`](tests/golden/README.md) | Decode-fidelity golden outputs. |
| [`tools/`](docs/contributing.md) | Schema validation, regeneration and fixture conversion. |

## Verify a checkout

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/validate-schemas.py
```

`cargo test --workspace` also compiles and runs the Rust examples in these
documents. The full check set, including regeneration and the C binding build,
is in [contributing](docs/contributing.md); what each test layer proves is in
[testing](docs/testing.md).

## Status and boundaries

Version 0.2.0, MIT licensed, minimum supported Rust 1.82. Workspace crates are
`publish = false`; [versioning](docs/versioning.md) says how versions are
named. Support for a revision means the library decodes it from the schemas
— it is not a qualification of any particular hardware. The library parses
and decides; it does not drive a module. A kernel module, userland tools and
packaging are separate components and are not part of this tree.
