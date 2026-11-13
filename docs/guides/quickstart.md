# Quickstart — zero-setup decode

Decode a module image with no fixture checkout path and no hardware. The
`zero-setup` and `explore-module` examples under
[`crates/openxcvr/examples/`](../../crates/openxcvr/examples/zero-setup.rs)
embed a synthetic sample, so they run from any working directory.

## Run the embedded sample

From the repository:

```sh
cargo run -p openxcvr --example zero-setup
```

From anywhere else, point Cargo at this repo's manifest:

```sh
cargo run --manifest-path /path/to/openxcvr-lib/Cargo.toml -p openxcvr --example zero-setup
```

The example prints the vendor strings and the spec revision proven from
lower-memory byte 1. [`CmisModule::from_bytes`](../../crates/openxcvr-cmis/src/module.rs)
fails closed on a revision byte this library does not know; the example does
not substitute a later revision.

## What the sample is

The bytes are the committed output of
[`tools/build-cmis-5-2-synthetic.py`](../../tools/build-cmis-5-2-synthetic.py):
a hand-authored CMIS 5.2 QSFP-DD image (lower memory plus pages 00h, 01h,
02h, 10h and 11h). Vendor strings are placeholders (`CMIS_5_2_SYNTH`), not a
module that was captured from hardware. Provenance:
[`tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.md`](../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.md).

`tests/fixtures/cmis/ATTRIBUTIONS.md` describes other files in that directory
that were derived from fboss. Those files are not embedded.

[`examples/quickstart.rs`](../../crates/openxcvr/examples/quickstart.rs) and
[`examples/read-vendor-info.rs`](../../crates/openxcvr/examples/read-vendor-info.rs)
still open fixture paths relative to the repository root. Use `zero-setup`
when you do not want that precondition.

## Explore pages and fields

[`explore-module`](../../crates/openxcvr/examples/explore-module.rs) lists the
pages that were supplied and decodes one page or field at a time. It is the
same embedded sample unless you pass a capture.

```sh
cargo run -p openxcvr --example explore-module
cargo run -p openxcvr --example explore-module -- --tour
cargo run -p openxcvr --example explore-module -- tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.lower.bin
```

A terminal starts a command loop: `vendor`, `pages`, `page <label>`,
`field [page] <name>`, `quit`. A pipe runs the commands it is given and
exits. With no commands (and when `--tour` is set) it prints a short tour:
revision, vendor, and a few decoded fields.

A path is one of:

- a `*.lower.bin` file — sibling `<stem>.page-XXh.bin` files in the same
  directory are loaded with it;
- a directory that contains exactly one `*.lower.bin`;
- a dense image whose length is a multiple of 128 bytes (first window is
  lower memory, then upper pages `00h`, `01h`, …).

Each named page file must be exactly 128 bytes. A missing page is reported
as missing. The dense transport image has zeros in gaps between sparse
pages; the explorer does not decode those gaps, and it will not read vendor
identity unless page 00h was supplied.

Decoding reads only the supplied bytes. It does not open an I2C device.

## Where next

The rest of the usage spine starts at the [consumer guide](../consumer-guide.md):
[install](installation.md), [concepts](concepts.md), this quickstart, the
[API guide](api-guide.md), then [error handling](error-handling.md).

For task-shaped recipes — decode, detect, thresholds, lane status, error
triage — open the [cookbook](../cookbook.md). For consuming the library from
another language, the per-binding guides are indexed in
[bindings/README.md](../../bindings/README.md).
