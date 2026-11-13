# Versioning

How versions are named in this repository. Nothing here is uploaded to a
package registry by the tree as it stands: the workspace crates are
`publish = false`.

## Workspace version

One version lives in the root `Cargo.toml` `[workspace.package]` block, and
every crate inherits it. While the project is pre-1.0, a breaking change
bumps the minor version. [`CHANGELOG.md`](../CHANGELOG.md) records what
shipped.

The minimum supported Rust is 1.82 (`rust-version` in the workspace
manifest). That is the version CI typechecks.

## Bindings

These manifests carry the same version as the workspace:

- C: `project(openxcvr_c VERSION …)` in `bindings/c/CMakeLists.txt`
- C#: `OpenXcvr.Transceivers.csproj`, `openxcvr.nuspec`, and the
  consumer-check project
- Java: `build.gradle` and `pom.xml` (the jar name embeds the version)
- Python: `pyproject.toml`
- TypeScript: `package.json`

Deliberately versionless, because the version comes from a VCS tag:

- PHP `composer.json`
- Go module `github.com/OpenXCVR/openxcvr-lib/bindings/go`, tagged
  `bindings/go/vX.Y.Z`

The fuzz crate stays `0.0.0`. It is a test target, not a distributable.

## What a checkout is

Depend on the Rust crates by path. The language bindings are built from
this tree. The C SDK workflow attaches build artifacts to the workflow
run and does not upload them anywhere.
