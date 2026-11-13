#!/usr/bin/env python3
"""Unittest for tools/validate-golden-schema.py (design R2 A2/A3, amended by R3 A1).

CMIS probe set (design-r2 A2 as extended by design-r3 A1): in-memory copies of
the real goldens in tests/golden/cmis, mutated per probe. tests/golden is never
written. OLD_CMIS_VERDICTS below was recorded once, while authoring, by running
the same probes against fdbfb85's schema:

    git show fdbfb85:bindings/openapi/openxcvr.openapi.yaml > /tmp/old.openapi.yaml

The committed (restructured) schema must reproduce every recorded verdict;
design-r3 A1 requires zero differences (probe set and table stay in sync).

SFF probes: each golden validates against its family's result schema; dropping
any required top-level key fails; decode_status "rejected" fails (SFF results
are ok-only).
"""

import copy
import importlib.util
import json
import sys
import unittest
from pathlib import Path

_TOOLS = Path(__file__).resolve().parent

_SPEC = importlib.util.spec_from_file_location(
    "validate_golden_schema", _TOOLS / "validate-golden-schema.py"
)
vgs = importlib.util.module_from_spec(_SPEC)
sys.modules["validate_golden_schema"] = vgs
_SPEC.loader.exec_module(vgs)

ROOT = _TOOLS.parent
GOLDEN_ROOT = ROOT / "tests" / "golden"

CMIS_OK_REQUIRED = [
    "decode_status",
    "identity",
    "module",
    "monitoring",
    "applications",
    "checksums",
    "lanes",
    "flags",
]

SFF_RESULT_SCHEMAS = {
    "sff-8636": "Sff8636DecodeResult",
    "sff-8472": "Sff8472DecodeResult",
}

SFF_REQUIRED_KEYS = [
    "decode_status",
    "identity",
    "module",
    "monitoring",
    "compliance",
    "flags",
    "lanes",
    "checksums",
]


def golden_paths(family):
    return sorted(
        p for p in (GOLDEN_ROOT / family).glob("*.json") if not p.name.startswith("_")
    )


def load_golden(family, path):
    doc = json.loads(path.read_text())
    doc.pop("_metadata", None)
    return doc


def ok_base():
    """First CMIS golden (alphabetical) whose decode_status is ok; mutation base."""
    for path in golden_paths("cmis"):
        doc = load_golden("cmis", path)
        if doc.get("decode_status") == "ok":
            return doc
    raise AssertionError("no ok CMIS golden found")


def rejected_base():
    doc = ok_base()
    doc["decode_status"] = "rejected"
    for key in CMIS_OK_REQUIRED:
        if key != "decode_status":
            doc.pop(key)
    doc["rejection"] = {
        "reason": "unknown_major_revision: 0x05",
        "detail": "test rejection",
    }
    return doc


def build_cmis_probes():
    probes = {}

    for path in golden_paths("cmis"):
        probes[f"golden:{path.stem}"] = load_golden("cmis", path)

    base = ok_base()
    for key in CMIS_OK_REQUIRED[1:]:
        doc = copy.deepcopy(base)
        doc.pop(key)
        probes[f"ok_missing:{key}"] = doc

    rejected = rejected_base()
    probes["rejected_valid"] = copy.deepcopy(rejected)

    doc = copy.deepcopy(rejected)
    doc.pop("rejection")
    probes["rejected_missing_rejection"] = doc
    doc = copy.deepcopy(rejected)
    doc["rejection"].pop("detail")
    probes["rejected_missing_rejection_detail"] = doc
    doc = copy.deepcopy(rejected)
    doc["rejection"].pop("reason")
    probes["rejected_missing_rejection_reason"] = doc

    doc = copy.deepcopy(base)
    doc["decode_status"] = "unknown"
    probes["unknown_discriminant"] = doc
    doc = copy.deepcopy(base)
    doc.pop("decode_status")
    probes["decode_status_missing"] = doc
    doc = copy.deepcopy(base)
    doc["decode_status"] = None
    probes["decode_status_null"] = doc
    doc = copy.deepcopy(base)
    doc["decode_status"] = 5
    probes["decode_status_non_string"] = doc

    for key in ("applications", "lanes", "flags"):
        doc = copy.deepcopy(base)
        doc[key] = None
        probes[f"null_in:{key}"] = doc

    for key, wrong in (
        ("identity", 7),
        ("module", "x"),
        ("monitoring", []),
        ("applications", 3),
        ("checksums", 5),
        ("lanes", "no"),
        ("flags", 2),
    ):
        doc = copy.deepcopy(base)
        doc[key] = wrong
        probes[f"wrong_type:{key}"] = doc

    doc = copy.deepcopy(base)
    doc["rejection"] = {"reason": "r", "detail": "d"}
    probes["ok_carrying_valid_rejection"] = doc
    doc = copy.deepcopy(base)
    doc["rejection"] = 7
    probes["ok_carrying_malformed_rejection"] = doc
    doc = copy.deepcopy(rejected)
    doc["identity"] = copy.deepcopy(base["identity"])
    probes["rejected_carrying_valid_identity"] = doc
    doc = copy.deepcopy(rejected)
    doc["identity"] = 7
    probes["rejected_carrying_malformed_identity"] = doc

    doc = copy.deepcopy(base)
    doc["surprise"] = 1
    probes["extra_top_level"] = doc
    doc = copy.deepcopy(base)
    doc["identity"]["surprise"] = 1
    probes["extra_nested"] = doc

    return probes


# Verdicts under fdbfb85's schema (bool: document validated). Recorded while
# authoring by running build_cmis_probes() against
# `git show fdbfb85:bindings/openapi/openxcvr.openapi.yaml`.
# New fixture probes must be recorded against that same historical schema.
OLD_CMIS_VERDICTS: dict = {
    "decode_status_missing": False,
    "decode_status_non_string": False,
    "decode_status_null": False,
    "extra_nested": True,
    "extra_top_level": True,
    "golden:fboss-cmis-4-0-osfp-2x400g-dr4": True,
    "golden:fboss-cmis-4-0-qsfp112-200g-fr4": True,
    "golden:fboss-cmis-4-0-qsfpdd-400g-dr4": True,
    "golden:fboss-cmis-4-0-qsfpdd-400g-fr4-multiport": True,
    "golden:fboss-cmis-4-0-qsfpdd-400g-lr4": True,
    "golden:fboss-cmis-4-0-qsfpdd-flatmem": True,
    "golden:fboss-cmis-5-0-osfp-2x400g-fr4": True,
    "golden:fboss-cmis-5-1-osfp-2x400g-fr4-lite": True,
    "golden:fboss-cmis-5-1-osfp-2x400g-fr4-lpo": True,
    "golden:fboss-cmis-5-1-osfp-2x800g-dr4": True,
    "golden:fboss-cmis-5-1-osfp-credo-800g-aec": True,
    "golden:fboss-cmis-5-3-osfp-800g-zr": True,
    "golden:fboss-cmis-5-3-osfp-800g-zr-bias-reserved": True,
    "golden:fboss-cmis-5-3-osfp-800g-zr-dp-reserved": True,
    "golden:fboss-cmis-anomalous-qsfpdd-400g-cr8": True,
    "golden:synthetic-cmis-3-0-qsfp-dd": True,
    "golden:synthetic-cmis-3-0-qsfp-dd-dp-reserved": True,
    "golden:synthetic-cmis-5-2-qsfp-dd": True,
    "golden:synthetic-cmis-5-4-qsfp-dd-vdm": True,
    "null_in:applications": True,
    "null_in:flags": True,
    "null_in:lanes": True,
    "ok_carrying_malformed_rejection": False,
    "ok_carrying_valid_rejection": True,
    "ok_missing:applications": False,
    "ok_missing:checksums": False,
    "ok_missing:flags": False,
    "ok_missing:identity": False,
    "ok_missing:lanes": False,
    "ok_missing:module": False,
    "ok_missing:monitoring": False,
    "rejected_carrying_malformed_identity": False,
    "rejected_carrying_valid_identity": True,
    "rejected_missing_rejection": False,
    "rejected_missing_rejection_detail": False,
    "rejected_missing_rejection_reason": False,
    "rejected_valid": True,
    "unknown_discriminant": False,
    "wrong_type:applications": False,
    "wrong_type:checksums": False,
    "wrong_type:flags": False,
    "wrong_type:identity": False,
    "wrong_type:lanes": False,
    "wrong_type:module": False,
    "wrong_type:monitoring": False,
}


class CmisVerdictEquivalenceTest(unittest.TestCase):
    def test_verdicts_match_old_schema_table(self):
        spec = vgs.load_spec()
        validator = vgs.make_validator(spec, "CmisDecodeResult")
        probes = build_cmis_probes()
        self.assertEqual(
            set(probes),
            set(OLD_CMIS_VERDICTS),
            "probe set changed: re-record OLD_CMIS_VERDICTS against fdbfb85",
        )
        differences = []
        for name in sorted(probes):
            errors = list(validator.iter_errors(probes[name]))
            valid = not errors
            expected = OLD_CMIS_VERDICTS[name]
            if valid != expected:
                differences.append(
                    f"{name}: old={expected} new={valid} "
                    f"(new errors: {['; '.join(str(e.message) for e in errors)]})"
                )
        self.assertEqual(
            differences,
            [],
            "verdict differences vs fdbfb85 (design-r3 A1 requires none)",
        )


class SffResultSchemaTest(unittest.TestCase):
    def test_goldens_pass_dropped_keys_and_rejected_fail(self):
        spec = vgs.load_spec()
        for family, schema_name in SFF_RESULT_SCHEMAS.items():
            validator = vgs.make_validator(spec, schema_name)
            paths = golden_paths(family)
            self.assertGreater(len(paths), 0, f"{family} must have goldens")

            for path in paths:
                doc = load_golden(family, path)
                with self.subTest(family=family, golden=path.name):
                    errors = list(validator.iter_errors(doc))
                    self.assertEqual(
                        errors,
                        [],
                        f"{path.name} must validate against {schema_name}: "
                        + "; ".join(e.message for e in errors),
                    )

            base = load_golden(family, paths[0])
            for key in SFF_REQUIRED_KEYS:
                doc = copy.deepcopy(base)
                doc.pop(key)
                with self.subTest(family=family, dropped=key):
                    self.assertFalse(
                        validator.is_valid(doc),
                        f"dropping required key {key} must invalidate {schema_name}",
                    )
            doc = copy.deepcopy(base)
            doc["decode_status"] = "rejected"
            with self.subTest(family=family, rejected=True):
                self.assertFalse(
                    validator.is_valid(doc),
                    "SFF results are ok-only; decode_status 'rejected' must fail",
                )

    def test_family_map_covers_all_golden_directories(self):
        families = vgs.FAMILIES
        for family in families:
            self.assertTrue((GOLDEN_ROOT / family).is_dir(), family)
        for entry in GOLDEN_ROOT.iterdir():
            if entry.is_dir():
                self.assertIn(entry.name, families, f"unmapped golden directory {entry.name}")


if __name__ == "__main__":
    unittest.main()
