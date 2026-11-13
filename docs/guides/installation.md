# Installation

How to put the library on a crate path, and where each language binding
documents its own install. The concepts those crates implement are
[concepts](concepts.md). The zero-setup run needs no install beyond a
Rust toolchain: [quickstart](quickstart.md).

## Rust

The consumer crate is `openxcvr` (`crates/openxcvr`). Workspace version
is `0.2.0`, edition 2021. MSRV is `rust-version = "1.82"` in the workspace
`Cargo.toml`: that is the version CI typechecks, not a suggestion.

The workspace crates are `publish = false`. They are not on crates.io.
Depend on a checkout:

```toml
[dependencies]
openxcvr = { path = "path/to/openxcvr-lib/crates/openxcvr" }
```

The optional `linux-i2c` feature forwards `openxcvr-core`'s Linux i2c-dev
transport. It is meaningful on Linux; the runnable example is
[`crates/openxcvr/examples/linux-i2c.rs`](../../crates/openxcvr/examples/linux-i2c.rs)
(`cargo run -p openxcvr --features linux-i2c --example linux-i2c`).

```toml
openxcvr = { path = "path/to/openxcvr-lib/crates/openxcvr", features = ["linux-i2c"] }
```

The crates are not published. See [versioning](../versioning.md). Do not
`cargo add openxcvr` against a registry and expect this tree.

`use openxcvr::prelude::*;` is the import the rest of these guides use.
The [API guide](api-guide.md) starts there.

## Language bindings

Bindings are generated surfaces in this repo, not registry packages
unless that binding's own page says so. OpenAPI is a schema description,
not a runtime library. PHP's install recipe (Composer package
`petabitscale/openxcvr`, PHP ^8.2, path-repository consumption) is the
Installation section of its README, linked below. C's build and
`cmake --install` steps are
the opening paragraphs of its README, above the first heading.

| Binding | Where install is documented |
| --- | --- |
| C | [bindings/c/README.md](../../bindings/c/README.md) (opening: cmake build, `cmake --install`, `pkg-config`) |
| C# | [Build](../../bindings/csharp/README.md#installation) |
| Go | [Module path](../../bindings/go/README.md#installation) |
| Java | [Build](../../bindings/java/README.md#installation) |
| OpenAPI | [bindings/openapi/README.md](../../bindings/openapi/README.md) (no install; schema and golden validation) |
| PHP | [Installation](../../bindings/php/README.md#installation) |
| Python | [Installation](../../bindings/python/README.md#installation) |
| TypeScript | [Local distribution and installation](../../bindings/typescript/README.md#installation) |

The index of surfaces is [bindings/README.md](../../bindings/README.md).
A local path or build command on the page above is not a public package.
See [versioning](../versioning.md).

## Where next

[Concepts](concepts.md), then the [quickstart](quickstart.md).
