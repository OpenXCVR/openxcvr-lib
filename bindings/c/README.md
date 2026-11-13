# openxcvr C binding

## Overview

C99 library `libopenxcvr_c` for CMIS, SFF-8636, and SFF-8472. Public headers
live in `include/openxcvr/`. `generated/` is codegen. `src/openxcvr.c` is the
handwritten JSON aggregate layer.

The aggregate decoders are **not** the canonical page-map facade. There is no
lane-flag entry point, no threshold-evaluation entry point, and no canonical
key map. Python, Go, and TypeScript own that contract; see
[../README.md](../README.md#consumer-facade-contract). Callers pass loaded
`openxcvr_cmis_pages` / `openxcvr_sff8636_pages` / `openxcvr_sff8472_pages`
structs and receive JSON plus an `openxcvr_status`.

Do not edit `generated/`.

## Installation

CMake >= 3.16 and a C99 toolchain. From the repository root:

```sh
cmake -S bindings/c -B build/openxcvr-c -DCMAKE_BUILD_TYPE=Release
cmake --build build/openxcvr-c
cmake --install build/openxcvr-c --prefix /path/to/prefix
```

On Linux this builds `libopenxcvr_c.a` and `libopenxcvr_c.so` (macOS uses
`.dylib`). The installed `openxcvr.pc.in` becomes `lib/pkgconfig/openxcvr-c.pc`:

```sh
pkg-config --cflags --libs openxcvr-c
pkg-config --static --libs openxcvr-c    # adds Libs.private, including -lm when required
```

Against the build tree (no install), compile with
`-I bindings/c/include -I bindings/c/generated` and link `libopenxcvr_c`.
Dynamic consumers do not repeat `-lm`; static consumers need it because
`openxcvr_cmis_f16_to_double` uses `fma()`. `src/observation.c` refuses to
compile under `__FAST_MATH__` or a nonzero `FLT_EVAL_METHOD`.

The rich application-records component is off unless `-DOPENXCVR_C_RICH=ON`.
Building it from source needs Cargo and rustc. Installed shared consumers of
that library do not need a Rust toolchain. See Usage.

```sh
ctest --test-dir build/openxcvr-c --output-on-failure
```

### Windows source build and package checks

The Windows CI SDK build uses LLVM's **GNU-style** `clang` / `clang++`
drivers (not `clang-cl` or `cl`), Ninja, and an initialized x64 Visual Studio
developer environment with the Windows SDK and dynamic MSVC CRT. From that
environment, with LLVM, Ninja, and Python available, PowerShell commands are:

```powershell
# Select one installation in PATH order, even when multiple LLVMs are present.
$clang = (Get-Command clang.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$clangxx = (Get-Command clang++.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
cmake -S bindings/c -B build/openxcvr-c -G Ninja -DCMAKE_BUILD_TYPE=Release `
  "-DCMAKE_C_COMPILER=$clang" "-DCMAKE_CXX_COMPILER=$clangxx" `
  -DCMAKE_C_COMPILER_TARGET=x86_64-pc-windows-msvc `
  -DCMAKE_CXX_COMPILER_TARGET=x86_64-pc-windows-msvc `
  -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL `
  -DCMAKE_WINDOWS_EXPORT_ALL_SYMBOLS=ON
cmake --build build/openxcvr-c
ctest --test-dir build/openxcvr-c --output-on-failure -E "openxcvr_c_installed_shared_link|openxcvr_c_installed_f16_consumer|openxcvr_c_checked_page_portability|openxcvr_c_installed_checked_consumer"
cmake --install build/openxcvr-c --prefix C:/path/to/prefix
```

The static target installs `lib/openxcvr_c_static.lib`; the DLL/import pair
is `bin/openxcvr_c.dll` + `lib/openxcvr_c.lib`. The dynamic MSVC CRT setting
is `/MD`-equivalent; CI separately compiles and runs actual `cl /W4 /MD`
static and shared consumers from an unpacked package and verifies it with
`dumpbin`. The four excluded CTests require Unix-oriented pkg-config/link
or kernel-style tooling and remain covered on POSIX. This is distinct from
claiming a `cl` source build or native Windows 11 runtime verification. The
workflow attaches the package to the run and does not publish it; see
[versioning](../../docs/versioning.md). Export-all-symbols
preserves the existing function-export DLL surface; public data such as
`openxcvr_cmis_vdm_page_tags` is available through the static archive.

### Parity test entry points

Tests use the CMake-selected Python interpreter (`Python3_EXECUTABLE` can
select it); `-DBUILD_TESTING=OFF` does not require Python for a library-only
build. `openxcvr_c_parity` with no arguments still runs all three golden
families and the lane-decode, latch, checksum, alarm, and read-error witnesses.
`openxcvr_c_parity --self-test` retains its scaffold check. The small Python
driver handles discovery, JSON comparison (the existing `compare_json.py`
semantics), subprocesses, and isolated temporary files; C retains the SDK
calls and status/size/padding checks. Each family must execute every discovered
non-underscore `*.json` golden and must be nonempty. Read-error directories
must yield `OPENXCVR_ERR_IO`: Linux exercises `fread`/`ferror` on a directory,
whereas Windows may reject it at `fopen`. Both retain the 100-byte short-page
zero-padding oracle; they do not prove identical internal branch coverage.

## Quickstart

Synthetic bytes only. The JSON writer reads the vendor name from page `00h`
bytes 1–16 and strips trailing spaces. Fill the raw arrays, then run the
generated page decoders into the struct the way `openxcvr_cmis_pages_load`
does, then the two-pass JSON call. A sizing pass with a null buffer returns
`OPENXCVR_ERR_SHORT_BUFFER` (or `OPENXCVR_OK` if the buffer already fits);
`*written` includes the terminating NUL.

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "openxcvr/openxcvr.h"

int main(void) {
    openxcvr_cmis_pages pages;
    memset(&pages, 0, sizeof pages);
    pages.lower_raw[1] = 0x50; /* CMIS 5.0 */
    memset(pages.page00h_raw + 1, ' ', 16);
    memcpy(pages.page00h_raw + 1, "SYNTHETIC", 9);

    openxcvr_status st = openxcvr_decode_cmis_lower_memory(
        pages.lower_raw, sizeof pages.lower_raw, &pages.lower);
    if (st != OPENXCVR_OK) return 1;
    st = openxcvr_decode_cmis_page00h_administrative(
        pages.page00h_raw, sizeof pages.page00h_raw, &pages.page00h);
    if (st != OPENXCVR_OK) return 1;

    size_t needed = 0;
    st = openxcvr_decode_cmis_json(&pages, NULL, 0, &needed);
    if (st != OPENXCVR_OK && st != OPENXCVR_ERR_SHORT_BUFFER) return 1;
    char *json = malloc(needed);
    if (json == NULL) return 1;
    st = openxcvr_decode_cmis_json(&pages, json, needed, &needed);
    if (st != OPENXCVR_OK) {
        free(json);
        return 1;
    }
    if (strstr(json, "\"decode_status\":\"ok\"") == NULL ||
        strstr(json, "\"vendor_name\":\"SYNTHETIC\"") == NULL) {
        fprintf(stderr, "%s\n", json);
        free(json);
        return 1;
    }
    puts(json);
    free(json);
    return 0;
}
```

```sh
cc -std=c99 -Wall -Wextra -I bindings/c/include -I bindings/c/generated \
    quickstart.c build/openxcvr-c/libopenxcvr_c.a -lm -o quickstart
./quickstart
```

`-lopenxcvr_c` also works. If the shared library is selected, run it with
`LD_LIBRARY_PATH` pointing at the build directory, or install and use
pkg-config. The static archive above does not need that.

## Usage

### Aggregate JSON

```c
openxcvr_status openxcvr_cmis_pages_load(const char *fixture_dir, const char *stem, openxcvr_cmis_pages *out);
openxcvr_status openxcvr_decode_cmis_json(const openxcvr_cmis_pages *pages, char *buf, size_t len, size_t *written);
openxcvr_status openxcvr_sff8636_pages_load(const char *fixture_dir, const char *stem, openxcvr_sff8636_pages *out);
openxcvr_status openxcvr_decode_sff8636_json(const openxcvr_sff8636_pages *pages, char *buf, size_t len, size_t *written);
openxcvr_status openxcvr_sff8472_pages_load(const char *fixture_dir, const char *stem, openxcvr_sff8472_pages *out);
openxcvr_status openxcvr_decode_sff8472_json(const openxcvr_sff8472_pages *pages, char *buf, size_t len, size_t *written);
```

`openxcvr_cmis_pages` holds 128-byte `lower_raw`, `page00h_raw`, `page01h_raw`,
`page11h_raw`, `has_page01h`, `has_page11h`, the 15-slot VDM set (`vdm_raw`,
`has_vdm_page`; tags in `openxcvr_cmis_vdm_page_tags`, `2eh` excluded), and the
generated decoded structs. `openxcvr_cmis_pages_set_vdm` copies one 128-byte
page by tag. `openxcvr_cmis_pages_vdm_complete` is non-zero only for the full
set; a partial set emits no `vdm` object.

The `*_pages_load` helpers read fixture files (`<stem>.lower.bin`,
`<stem>.page-00h.bin`, and for CMIS the optional page files). SFF-8472 load
zero-pads a short file to 256 bytes. In-memory callers copy bytes and call
`openxcvr_decode_cmis_lower_memory`, `openxcvr_decode_cmis_page00h_administrative`,
`openxcvr_decode_sff8636_lower_memory`,
`openxcvr_decode_sff8636_page00h_upper_administrative`,
`openxcvr_decode_page_a0h_base_memory`, and
`openxcvr_decode_page_a2h_diagnostics_base` themselves. The JSON writers read
identity and checksums from those structs and from the raw buffers; leaving
the structs zeroed drops every field that comes from the generated decode.

An unknown CMIS major (lower raw byte 1 not 3, 4, or 5) is JSON
`decode_status` `"rejected"` with `rejection.reason`
`unknown_major_revision: 0xNN`, returned as `OPENXCVR_OK` when the buffer
fits. It is not `OPENXCVR_ERR_UNSUPPORTED_REVISION`. SFF decoders are ok-only.
SFF-8472 emits `flags`, `monitoring`, and `lanes` as null when A0h byte 92
bit 6 is clear. SFF-8472 telemetry in this JSON is uncalibrated.

Page `11h` with `has_page11h` set and `has_page01h` clear returns
`OPENXCVR_ERR_INVALID_ARGUMENT` on a paged module (Tx-bias scaling is the
generated `page01h.tx_bias_current_scaling_factor`).

### Other public helpers

These are not the cross-language facade. Contracts are the headers.

- DPSM, declared in `include/openxcvr/openxcvr.h`:
  `openxcvr_cmis_dpsm_is_terminal_for_rev` and
  `openxcvr_cmis_dpsm_expected_active_terminal_for_rev`. Admitted revision
  bytes are `0x30`, `0x40`, `0x50`, `0x51`, `0x52`, `0x53`, and `0x54`. Any
  other byte is `OPENXCVR_ERR_UNSUPPORTED_REVISION`. A null output, or a
  `raw_state` above `0x0f` on the terminal predicate, is
  `OPENXCVR_ERR_INVALID_ARGUMENT`. Outputs are left unchanged on error.
  They are pure: no I/O.
- Revision-checked page decodes (generated):
  `openxcvr_decode_cmis_page01h_advertising_for_revision` and
  `openxcvr_decode_cmis_page11h_lane_status_for_revision`. Same seven revision
  bytes. Error order is null argument, then unsupported revision, then
  `len < 128`. Failures leave `*out` unchanged. Read `out.present` before
  `out.values`. Raw enum storage is not remapped; use
  `openxcvr_cmis_data_path_state_for_revision` (NULL when that raw is reserved
  at that revision).
- Rich application records (`include/openxcvr/application_records.h`), only
  when the rich library is built. `openxcvr_application_records` covers CMIS
  4.0 and 5.0–5.3 (not 5.4). Set `size` / `version` to `sizeof(request)` and
  `OPENXCVR_APPLICATION_RECORDS_VERSION`, then
  `openxcvr_application_records_free` the result. Do not free members.
  `openxcvr_application_records_footprint` and
  `openxcvr_application_records_from_ranges` are the sparse pair: the
  footprint names the bytes a record needs, and `from_ranges` decodes only
  the bytes the caller supplied.
  Default `openxcvr-c.pc` does not advertise these symbols.
  `pkg-config --cflags --libs openxcvr-c-rich` is the shared consumer;
  `--static` is the static consumer. Rich configure is native GNU/Linux only
  (GCC or Clang, 64-bit little-endian `x86_64` or `aarch64`). x86_64 is the
  verified host; aarch64 is not. macOS, Windows, cross compilation, and
  multi-config generators are rejected at configure time.
- `openxcvr_cmis_f16_to_double` (`include/openxcvr/observation.h`) converts a
  CMIS / SFF-8636 F16 code (`mantissa * 10^(scaled_exponent - 24)`, not IEEE
  binary16) to binary64, round-to-nearest ties-to-even.
- `openxcvr_field_read_prelude` and `openxcvr_field_read_remaining` plan in
  automatic storage (on the order of tens of KB, compiler-dependent). They
  are for ordinary user-space threads, not kernel or small fixed stacks.
  `openxcvr_observe_field` admits CMIS rows only when the snapshot revision
  byte matches and presence prerequisites hold. Page 14h selector-multiplexed
  rows need `openxcvr_observe_field_ctx`; the context-free call behaves as an
  empty context and returns not-supplied for those rows.

## Error handling

`include/openxcvr/status.h`:

| Code | Value | Typical cause on this surface |
| --- | --- | --- |
| `OPENXCVR_OK` | 0 | Success, including a CMIS `rejected` JSON document that fit |
| `OPENXCVR_ERR_INVALID_ARGUMENT` | 1 | NULL `pages` or NULL `written`; page 11h without page 01h; bad rich request |
| `OPENXCVR_ERR_SHORT_BUFFER` | 2 | JSON buffer shorter than `*written` (normal sizing pass); page `len < 128` |
| `OPENXCVR_ERR_IO` | 3 | `*_pages_load` could not read a fixture |
| `OPENXCVR_ERR_INSUFFICIENT_INPUT` | 4 | supplied-field reader past the span |
| `OPENXCVR_ERR_UNSUPPORTED_REVISION` | 5 | DPSM or checked-page revision byte outside the seven catalog bytes |
| `OPENXCVR_ERR_NO_MATCH` | 6 | catalog / footprint resolution |
| `OPENXCVR_ERR_AMBIGUOUS` | 7 | catalog resolution |
| `OPENXCVR_ERR_OUT_OF_RANGE` | 8 | catalog resolution |

`openxcvr_status_string` returns the name. Unknown CMIS major inside
`openxcvr_decode_cmis_json` is not status 5; it is JSON. There is no errno
and no allocated error object.

## Regeneration

From the repository root. Do not edit `generated/`. There is no
`tools/regen-c.sh`; `tools/regen-all.sh` invokes the codegen CLI:

```sh
tools/regen-all.sh cmis
cargo run -p openxcvr-codegen -- generate --spec cmis --lang c --out bindings/c/generated
```

`src/openxcvr.c`, `src/observation.c`, `src/catalog.c`,
`src/application_records.c`, and `include/openxcvr/*.h` are handwritten.
`bindings/c/rich-ffi` is the Rust bridge for the rich component, not a
regenerated page surface.

## See also

- [Bindings overview](../README.md): the contract every binding follows
- [Consumer guide](../../docs/consumer-guide.md)
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
