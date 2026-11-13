# openxcvr-codegen

The build-time generator: it parses the YAML schemas, builds and validates the intermediate representation, and emits the Rust adapters, the language bindings and the Markdown reference. It is never a runtime dependency.

Part of the openxcvr-lib workspace.

**This crate is not published to crates.io.** Install from source via `cargo install --path crates/openxcvr-codegen`.

## Quick start

Install the workspace-local CLI from source, then run the verified `generate` subcommand.
The binary name is `openxcvr-codegen`.

```sh
cargo install --path crates/openxcvr-codegen
openxcvr-codegen generate --schemas schemas/ --spec cmis --out generated/
# Or run the workspace tool without installing:
cargo run -p openxcvr-codegen -- generate --lang markdown --spec cmis --out docs/reference/
```

The CLI accepts these `generate` flags:

| Flag | Meaning | Default |
|------|---------|---------|
| `--schemas <PATH>` | Schema root to scan | `schemas` |
| `--spec <SPEC>` | Optional spec filter | none |
| `--out <PATH>` | Output directory | `generated` |
| `--lang <LANG>` | rust, python, go, php, c, typescript, java, csharp, openapi, markdown | `rust` |

Generation validates enum and page schemas before emission. Markdown uses the same
IR, template/include expansion, width calculation and revision resolver as bindings.
It writes a root index, owning-spec indexes, and per-page/per-enum files. Imported
SFF-8024 enums retain their own namespace. Indexes inventory unsupported schema kinds;
missing metadata stays `not recorded` and resolved CMIS 3.0–5.3 views are IR projections,
not independent historical-spec verification. No introduction dates are inferred.

Regenerate the committed [reference](../../docs/reference/index.md) with
`tools/regen-markdown.sh` (also included by `tools/regen-all.sh`). With no spec
names, every namespace that has `schemas/<name>/_index.yaml` is rendered.
An explicit spec must have that index. A generator error fails the run; it is
not skipped.
Run generator self-tests with `python3 -m unittest tools.test_gen_docs -v`.
Regenerate the reference with `tools/regen-markdown.sh` and check it with `./tools/check-regen-equality.sh`.

## See also

- [Contributing](../../docs/contributing.md): schema changes and regeneration
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)

## License

MIT; see [LICENSE](../../LICENSE).
