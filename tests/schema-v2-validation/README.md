# `tests/schema-v2-validation/`

Fixture corpus that exercises the `openxcvr.page.v2` checks in
[`tools/validate-schemas.py`](../../tools/validate-schemas.py) (`validate_page_v2`,
 fail-loud contract). No real schema uses `page.v2` yet, so these
hand-crafted pages are the only coverage for the version-metadata lint.

Each case is its own directory so it can be driven as a standalone schema root
(`validate-schemas.py` takes an argv root, defaulting to `schemas/`). Mirrors the
Rust side's tempfile-based `crates/openxcvr-codegen/tests/` pattern.

| Case | Expectation | Exercises |
|---|---|---|
| `ok/` | **PASS** | all five version-metadata keys (`since`/`until`/`changed_in`/`renamed_from`/`aliased_to`) with valid revs, resolving `enum_ref`s, and a non-colliding `old_name` |
| `bad-rev/` | **FAIL** | invalid rev string in a scalar key (`since: "v4.0"`) — must agree with `Rev::parse` in `version.rs` |
| `bad-aliased-rev/` | **FAIL** | invalid rev string in a list-item key (`aliased_to[].rev: "v5.2"`) — exercises the `- ` list-item strip path; mirrors the Rust `rejects_invalid_aliased_to_rev` test |
| `bad-enum-ref/` | **FAIL** | `changed_in[].enum_ref` pointing at an undeclared enum (caught by the global `enum_ref` pass) |
| `bad-rename/` | **FAIL** | `renamed_from.old_name` colliding with a current field `name:` in the same page |
| `ok-comments-flow/` | **PASS** | inline `#` comments on `since`/`until`/`changed_in[].rev` (must be stripped, not parsed as part of the value); a flow-style field mapping with a valid `since`; and a non-version `rev:` outside any version-metadata block (must be ignored, not rev-checked) |
| `bad-flow-rev/` | **FAIL** | invalid `since` rev inside a flow-style field mapping (`- { …, since: "v4.0" }`) — the old start-of-line scanner silently passed this; the regex-based scan now reaches rev keys inside `{ }` |

## Run

```sh
# Valid cases — must exit 0:
python3 tools/validate-schemas.py tests/schema-v2-validation/ok
python3 tools/validate-schemas.py tests/schema-v2-validation/ok-comments-flow

# Negative cases — each must exit 1 with a targeted FAIL line:
python3 tools/validate-schemas.py tests/schema-v2-validation/bad-rev
python3 tools/validate-schemas.py tests/schema-v2-validation/bad-aliased-rev
python3 tools/validate-schemas.py tests/schema-v2-validation/bad-enum-ref
python3 tools/validate-schemas.py tests/schema-v2-validation/bad-rename
python3 tools/validate-schemas.py tests/schema-v2-validation/bad-flow-rev
```

These directories live under `tests/`, never `schemas/`, so the default
`validate-schemas.py` run (root `schemas/`) does not scan the intentionally
invalid pages.
