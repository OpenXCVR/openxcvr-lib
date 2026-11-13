#!/usr/bin/env python3
"""Validate golden fixtures against openxcvr.openapi.yaml decode-result schemas.

Maps each golden family directory under tests/golden/ to its result schema,
validates every discovered golden (skipping `_`-prefixed files), and fails if a
family directory holds none. Prints per-family counts and the total.

Usage:
    python3 tools/validate-golden-schema.py

Requires: pip install "jsonschema[format-filter]>=4.18" pyyaml
"""
import json
import pathlib
import sys

try:
    import yaml
except ImportError:
    sys.exit("PyYAML not installed: pip install pyyaml")

try:
    from jsonschema import Draft202012Validator, FormatChecker
except ImportError:
    sys.exit("jsonschema not installed: pip install 'jsonschema[format-filter]>=4.18'")

ROOT = pathlib.Path(__file__).parent.parent
SPEC_PATH = ROOT / "bindings" / "openapi" / "openxcvr.openapi.yaml"
GOLDEN_ROOT = ROOT / "tests" / "golden"

# Golden family directory -> result schema in components.schemas.
FAMILIES = {
    "cmis": "CmisDecodeResult",
    "sff-8636": "Sff8636DecodeResult",
    "sff-8472": "Sff8472DecodeResult",
}


def load_spec(path=SPEC_PATH):
    path = pathlib.Path(path)
    if not path.exists():
        sys.exit(f"Schema not found: {path}")
    return yaml.safe_load(path.read_text())


def make_validator(spec, schema_name):
    """Build a Draft 2020-12 validator for one component schema.

    Inlines the other component schemas into a local $defs registry and rewrites
    '#/components/schemas/...' refs to '#/$defs/...' so jsonschema resolves
    them without the surrounding OpenAPI document.
    """
    schema = spec["components"]["schemas"][schema_name]
    full_schema = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": spec["components"]["schemas"],
        **schema,
    }
    full_schema = json.loads(
        json.dumps(full_schema).replace("#/components/schemas/", "#/$defs/")
    )
    return Draft202012Validator(full_schema, format_checker=FormatChecker())


def discover_goldens(family):
    """Sorted golden paths for one family, skipping `_`-prefixed and non-JSON."""
    directory = GOLDEN_ROOT / family
    if not directory.is_dir():
        sys.exit(f"Golden directory not found: {directory}")
    return sorted(p for p in directory.glob("*.json") if not p.name.startswith("_"))


def load_golden(path):
    data = json.loads(path.read_text())
    data.pop("_metadata", None)
    return data


def validate_family(spec, family):
    """Validate one family's goldens; prints per-file lines and the family count."""
    paths = discover_goldens(family)
    if not paths:
        sys.exit(f"No golden fixtures found in {GOLDEN_ROOT / family}/")
    validator = make_validator(spec, FAMILIES[family])
    passed = failed = 0
    for path in paths:
        errors = list(validator.iter_errors(load_golden(path)))
        if errors:
            print(f"  FAIL  {family}/{path.name}")
            for err in errors:
                loc = " -> ".join(str(p) for p in err.absolute_path) or "(root)"
                print(f"        [{loc}] {err.message}")
            failed += 1
        else:
            print(f"  PASS  {family}/{path.name}")
            passed += 1
    print(f"  {family}: {passed} passed; {failed} failed")
    return passed, failed


def main():
    spec = load_spec()
    total_passed = total_failed = 0
    for family in FAMILIES:
        passed, failed = validate_family(spec, family)
        total_passed += passed
        total_failed += failed
    print(f"\nOpenAPI parity: {total_passed} passed; {total_failed} failed")
    return 1 if total_failed else 0


if __name__ == "__main__":
    sys.exit(main())
