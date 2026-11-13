"""OpenAPI binding parity runner (Wave 4, openxcvr-enhancements).

Validates every generated JSON Schema under ``bindings/openapi/generated/``:

1. **Meta-schema validity** — each file is a well-formed JSON Schema (Draft
   2020-12), so a malformed emitter output is caught.
2. **Structural invariants** — `$id`, `$schema`, `title`, `type` present; for
   page schemas, every `required` name appears in `properties`.

The per-page OpenAPI schemas describe the raw page-decode shape (the same
contract the Rust/Go/TS/Java/C# per-page decoders produce). Golden fixtures are
organised by L2 semantic section, not per page, so cross-binding golden parity
for OpenAPI is covered by the unified harness in Wave 5; this runner proves the
emitted schemas are themselves valid and self-consistent.

Run: ``python3 -m pytest bindings/openapi/tests`` (or ``python3 <thisfile>``).
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from jsonschema.validators import Draft202012Validator

GENERATED = Path(__file__).resolve().parents[1] / "generated"
SCHEMA_FILES = sorted(GENERATED.glob("**/*.schema.json"))


def _load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def test_schemas_exist():
    assert SCHEMA_FILES, f"no generated schemas under {GENERATED}"


@pytest.mark.parametrize("path", SCHEMA_FILES, ids=lambda p: p.name)
def test_schema_is_valid_and_consistent(path: Path):
    doc = _load(path)

    # 1. Well-formed JSON Schema (raises SchemaError on malformed output).
    Draft202012Validator.check_schema(doc)

    # 2. Structural invariants.
    for key in ("$id", "$schema", "title", "type"):
        assert key in doc, f"{path.name}: missing top-level '{key}'"
    assert doc["$schema"].endswith("2020-12/schema"), f"{path.name}: unexpected draft"

    # 3. Page schemas: required ⊆ properties.
    if "properties" in doc:
        props = set(doc["properties"])
        missing = set(doc.get("required", [])) - props
        assert not missing, f"{path.name}: required not in properties: {sorted(missing)}"


def _main() -> int:
    bad = 0
    for path in SCHEMA_FILES:
        try:
            test_schema_is_valid_and_consistent(path)
        except AssertionError as exc:
            bad += 1
            print(f"FAIL {exc}")
    print(f"{len(SCHEMA_FILES)} schema(s) checked, {bad} invalid.")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(_main())
