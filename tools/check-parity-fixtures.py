#!/usr/bin/env python3
"""Closed-world structural checker for kernel/userland parity fixtures.

Reads tests/fixtures/parity/_shape.yaml as the contract and validates:

- the shape itself (known keys and predicates, the nine kinds, an adapter registry equal
  to the adapter set the ledger checker accepts, the legacy kmod_function values);
- --inventory: task 1's donor_inventory.yaml through the ledger checker's own parser and
  partition rule, plus the shape's pinned closed world (every matrix row and inventory row
  with its one owner, disposition and adapter) and the adapter mapping table;
- --kind K: K's case file (case shape, typed expected values, citations separating
  normative standard authority from pinned donor host policy, donor dispositions whose
  sources must match inventory identity exactly, adapter mapping, per-kind constraints and
  rules, every required category and every coverage combination);
- no option: everything above for all nine kinds, differences.yaml, and the inventory's
  fixture-id closure (every required fixture id is an actual case that cites its row, with
  the row's executable adapter for donated logic).

A kind's case file that has not been authored yet is an EXPLICIT DELIVERY GAP in the
no-option run (printed, never silently skipped): the fixture kinds are owned by their
tasks and land one at a time, so the full run passes while reporting exactly which kinds
remain unauthored — and every closure check that kind would carry (required categories,
coverage, donor-fixture execution, difference-list case references) re-engages as a hard
error the moment the kind's file lands. An explicitly requested --kind whose file is
missing is still a hard failure: a named input never fails open. Structural pass is not
parity: no adapter executes here and execution receipts remain task 52/53 delivery gates.

Metadata files (_shape, differences, donor_inventory) are never read as cases. Any other
*.yaml in the directory must be a kind's case file. Every unreadable or unknown input is an
error, never skipped. A pass is structural only: it executes no adapter and claims no parity.
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple

try:
    import yaml
except ImportError:  # pragma: no cover - environment guard
    yaml = None

TOOLS_DIR = Path(__file__).resolve().parent
REPO_ROOT = TOOLS_DIR.parent
DEFAULT_DIR = REPO_ROOT / "tests" / "fixtures" / "parity"

KINDS: Tuple[str, ...] = (
    "module_state",
    "readiness",
    "classifier",
    "thermal",
    "selector",
    "dynamic_data",
    "lifecycle",
    "scheduling",
    "operations",
)
LEGACY_KMOD_FUNCTIONS = ("cmis_thermal_decide", "xcvr_page_class_of", "cmis_module_walk_to_ready", "none")


class ParityFixtureError(Exception):
    """A parity fixture, shape, inventory or difference-list violation."""


def _load_provenance_module():
    """Reuse the ledger checker (task 1) rather than a second inventory parser."""
    path = TOOLS_DIR / ("check-" + "provenance" + ".py")
    spec = importlib.util.spec_from_file_location("check_provenance_for_parity", path)
    if spec is None or spec.loader is None:
        raise ParityFixtureError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


provenance = _load_provenance_module()


# --------------------------------------------------------------------------- YAML loading


if yaml is not None:

    _BaseLoader = getattr(yaml, "CSafeLoader", yaml.SafeLoader)

    class _UniqueKeyLoader(_BaseLoader):
        """SafeLoader that refuses duplicate mapping keys instead of keeping the last one."""

    def _construct_unique_mapping(loader, node, deep=False):
        if not isinstance(node, yaml.MappingNode):
            raise yaml.constructor.ConstructorError(None, None, "expected a mapping", node.start_mark)
        seen: Set[Any] = set()
        for key_node, _ in node.value:
            key = loader.construct_object(key_node, deep=True)
            if key in seen:
                raise yaml.constructor.ConstructorError(
                    None, None, f"duplicate key {key!r}", key_node.start_mark
                )
            seen.add(key)
        return yaml.constructor.SafeConstructor.construct_mapping(loader, node, deep=deep)

    _UniqueKeyLoader.add_constructor(
        yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_unique_mapping
    )


def load_yaml(path: Path) -> Any:
    if yaml is None:
        raise ParityFixtureError("PyYAML is required")
    if not path.is_file():
        raise ParityFixtureError(f"{path}: file not found")
    try:
        with open(path, "r", encoding="utf-8") as f:
            return yaml.load(f, Loader=_UniqueKeyLoader)
    except yaml.YAMLError as e:
        raise ParityFixtureError(f"{path}: unreadable YAML: {e}")


# --------------------------------------------------------------------------- predicates

_MISSING = object()
_INT_SEG = re.compile(r"^-?\d+$")
PREDICATE_NAMES = {"present", "absent", "nonempty"}
PREDICATE_OPS = {"equals", "one_of", "not_one_of", "contains", "contains_all", "lacks"}


def resolve(obj: Any, path: str) -> List[Any]:
    """Resolve a dotted path; '*' fans out, integer segments index lists ('-1' = last)."""
    current = [obj]
    for seg in str(path).split("."):
        nxt: List[Any] = []
        for value in current:
            if seg == "*":
                if isinstance(value, list):
                    nxt.extend(value)
                elif isinstance(value, dict):
                    nxt.extend(value.values())
            elif isinstance(value, list) and _INT_SEG.match(seg):
                idx = int(seg)
                if -len(value) <= idx < len(value):
                    nxt.append(value[idx])
            elif isinstance(value, dict):
                if seg in value:
                    nxt.append(value[seg])
                elif _INT_SEG.match(seg) and int(seg) in value:
                    nxt.append(value[int(seg)])
        current = nxt
    return current


def strict_eq(a: Any, b: Any) -> bool:
    """Type-strict equality: True is not 1 and "3.0" is not 3.0."""
    if type(a) is not type(b):
        return False
    if isinstance(a, list):
        return len(a) == len(b) and all(strict_eq(x, y) for x, y in zip(a, b))
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(strict_eq(a[k], b[k]) for k in a)
    return a == b


def _member(value: Any, options: List[Any]) -> bool:
    return any(strict_eq(value, o) for o in options)


def validate_predicate(pred: Any, where: str) -> None:
    if isinstance(pred, str):
        if pred not in PREDICATE_NAMES:
            raise ParityFixtureError(f"shape: unknown predicate '{pred}' at {where}")
        return
    if isinstance(pred, dict) and len(pred) == 1:
        op, arg = next(iter(pred.items()))
        if op not in PREDICATE_OPS:
            raise ParityFixtureError(f"shape: unknown predicate '{op}' at {where}")
        if op in ("one_of", "not_one_of", "contains_all") and (not isinstance(arg, list) or not arg):
            raise ParityFixtureError(f"shape: predicate '{op}' at {where} needs a non-empty list")
        return
    raise ParityFixtureError(f"shape: malformed predicate {pred!r} at {where}")


def holds(obj: Any, path: str, pred: Any) -> bool:
    values = resolve(obj, path)
    if pred == "present":
        return bool(values) and all(v is not None for v in values)
    if pred == "absent":
        return all(v is None for v in values)
    if pred == "nonempty":
        return bool(values) and all(isinstance(v, (list, dict, str)) and len(v) > 0 for v in values)
    op, arg = next(iter(pred.items()))
    if op == "equals":
        return bool(values) and all(strict_eq(v, arg) for v in values)
    if op == "one_of":
        return bool(values) and all(_member(v, arg) for v in values)
    if op == "not_one_of":
        return bool(values) and all(v is not None and not _member(v, arg) for v in values)
    if op == "contains":
        return _member(arg, values)
    if op == "contains_all":
        return all(_member(a, values) for a in arg)
    if op == "lacks":
        return not _member(arg, values)
    raise ParityFixtureError(f"unknown predicate {pred!r}")  # unreachable after validation


def failed_predicates(obj: Any, preds: Dict[str, Any]) -> List[str]:
    return [f"{path} {pred}" for path, pred in preds.items() if not holds(obj, path, pred)]


# --------------------------------------------------------------------------- shape

SHAPE_KEYS = {
    "schema",
    "kind",
    "metadata_files",
    "case_file",
    "donor_commit",
    "cmis_supported_revisions",
    "families",
    "bit_effects",
    "planned_operations",
    "requesters",
    "case_fields",
    "citation",
    "host_policy_fields",
    "donor",
    "adapters",
    "differences",
    "inventory",
    "kinds",
}
ADAPTER_KEYS = {"legacy", "kinds", "families", "donor_paths", "requires"}
KIND_KEYS = {
    "file",
    "required_inputs",
    "required_facets",
    "expected_types",
    "constraints",
    "rules",
    "categories",
    "coverage",
}
CATEGORY_KEYS = {"required", "unsupported_revision", "requires"}
INVENTORY_PIN_FIELDS = ("matrix_row", "disposition", "routine_owner", "adapter_id")


def _require_list(value: Any, where: str, nonempty: bool = True) -> List[Any]:
    if not isinstance(value, list) or (nonempty and not value):
        raise ParityFixtureError(f"{where} must be a {'non-empty ' if nonempty else ''}list")
    return value


def _require_map(value: Any, where: str, nonempty: bool = True) -> Dict[str, Any]:
    if not isinstance(value, dict) or (nonempty and not value):
        raise ParityFixtureError(f"{where} must be a {'non-empty ' if nonempty else ''}mapping")
    return value


def _check_preds(preds: Any, where: str) -> None:
    for path, pred in _require_map(preds, where).items():
        validate_predicate(pred, f"{where}.{path}")


def validate_shape(shape: Any) -> Dict[str, Any]:
    """Validate _shape.yaml itself; an unknown key, predicate, kind or adapter is an error."""
    shape = _require_map(shape, "shape")
    unknown = set(shape) - SHAPE_KEYS
    missing = SHAPE_KEYS - set(shape)
    if unknown or missing:
        raise ParityFixtureError(f"shape: unknown keys {sorted(unknown)}, missing keys {sorted(missing)}")
    if shape["schema"] != "openxcvr.parity.shape.v1" or shape["kind"] != "parity_fixture_shape":
        raise ParityFixtureError("shape: wrong schema/kind header")
    for key in ("metadata_files", "cmis_supported_revisions", "families", "bit_effects",
                "planned_operations", "requesters", "host_policy_fields"):
        _require_list(shape[key], f"shape.{key}")

    kinds = _require_map(shape["kinds"], "shape.kinds")
    if tuple(kinds) != KINDS:
        raise ParityFixtureError(f"shape: kinds must be exactly {list(KINDS)}, got {list(kinds)}")

    adapters = _require_map(shape["adapters"], "shape.adapters")
    registry = set(adapters)
    if registry != set(provenance.VALID_ADAPTERS):
        raise ParityFixtureError(
            "shape: adapter registry differs from the ledger checker VALID_ADAPTERS: "
            f"missing {sorted(set(provenance.VALID_ADAPTERS) - registry)}, "
            f"extra {sorted(registry - set(provenance.VALID_ADAPTERS))}"
        )
    for legacy in LEGACY_KMOD_FUNCTIONS:
        if legacy not in adapters or adapters[legacy].get("legacy") is not True:
            raise ParityFixtureError(f"shape: legacy kmod_function '{legacy}' must be registered as legacy")
    for name, ad in adapters.items():
        ad = _require_map(ad, f"shape.adapters.{name}")
        if set(ad) - ADAPTER_KEYS:
            raise ParityFixtureError(f"shape.adapters.{name}: unknown keys {sorted(set(ad) - ADAPTER_KEYS)}")
        for k in _require_list(ad.get("kinds"), f"shape.adapters.{name}.kinds"):
            if k not in kinds:
                raise ParityFixtureError(f"shape.adapters.{name}: unknown kind '{k}'")
        for fam in _require_list(ad.get("families"), f"shape.adapters.{name}.families"):
            if fam not in shape["families"]:
                raise ParityFixtureError(f"shape.adapters.{name}: unknown family '{fam}'")
        paths = _require_list(ad.get("donor_paths"), f"shape.adapters.{name}.donor_paths", nonempty=name != "none")
        if name == "none" and paths:
            raise ParityFixtureError("shape.adapters.none must map to no donor path")
        if "requires" in ad:
            _check_preds(ad["requires"], f"shape.adapters.{name}.requires")

    donor = _require_map(shape["donor"], "shape.donor")
    disps = _require_map(donor.get("dispositions"), "shape.donor.dispositions")
    if set(disps) != {"executable", "source_only", "none"}:
        raise ParityFixtureError("shape.donor.dispositions must be exactly executable, source_only, none")
    for key in ("source_fields", "passes", "optional"):
        _require_list(donor.get(key), f"shape.donor.{key}")
    _require_map(donor.get("citable_rows"), "shape.donor.citable_rows")

    cit = _require_map(shape["citation"], "shape.citation")
    if set(cit) != {"standard", "no_standard"}:
        raise ParityFixtureError("shape.citation must define exactly standard and no_standard")

    diff = _require_map(shape["differences"], "shape.differences")
    for key in ("required", "required_ids", "comparisons"):
        _require_list(diff.get(key), f"shape.differences.{key}")

    inv = _require_map(shape["inventory"], "shape.inventory")
    _require_map(inv.get("matrix_rows"), "shape.inventory.matrix_rows")
    for rid, pin in _require_map(inv.get("rows"), "shape.inventory.rows").items():
        if set(_require_map(pin, f"shape.inventory.rows.{rid}")) != set(INVENTORY_PIN_FIELDS):
            raise ParityFixtureError(f"shape.inventory.rows.{rid} must pin exactly {list(INVENTORY_PIN_FIELDS)}")

    for kname, spec in kinds.items():
        where = f"shape.kinds.{kname}"
        spec = _require_map(spec, where)
        if set(spec) - KIND_KEYS:
            raise ParityFixtureError(f"{where}: unknown keys {sorted(set(spec) - KIND_KEYS)}")
        if spec.get("file") != f"{kname}.yaml":
            raise ParityFixtureError(f"{where}.file must be {kname}.yaml")
        _require_list(spec.get("required_inputs"), f"{where}.required_inputs", nonempty=False)
        for facet in _require_list(spec.get("required_facets"), f"{where}.required_facets", nonempty=False):
            if facet not in shape["case_fields"]["optional"]:
                raise ParityFixtureError(f"{where}: required facet '{facet}' is not a case field")
        for t, keys in _require_map(spec.get("expected_types"), f"{where}.expected_types").items():
            _require_list(keys, f"{where}.expected_types.{t}")
        if "constraints" in spec:
            _check_preds(spec["constraints"], f"{where}.constraints")
        for i, rule in enumerate(spec.get("rules") or []):
            rule = _require_map(rule, f"{where}.rules[{i}]")
            if set(rule) != {"when", "then"}:
                raise ParityFixtureError(f"{where}.rules[{i}] must have exactly when and then")
            _check_preds(rule["when"], f"{where}.rules[{i}].when")
            _check_preds(rule["then"], f"{where}.rules[{i}].then")
        cats = _require_map(spec.get("categories"), f"{where}.categories")
        if not any(c.get("required") is True for c in cats.values()):
            raise ParityFixtureError(f"{where}: no required category")
        for cname, cat in cats.items():
            cat = _require_map(cat, f"{where}.categories.{cname}")
            if set(cat) - CATEGORY_KEYS or not isinstance(cat.get("required"), bool):
                raise ParityFixtureError(f"{where}.categories.{cname}: needs boolean required and known keys only")
            _check_preds(cat.get("requires"), f"{where}.categories.{cname}.requires")
        for j, cov in enumerate(spec.get("coverage") or []):
            cov = _require_map(cov, f"{where}.coverage[{j}]")
            for c in _require_list(cov.get("categories"), f"{where}.coverage[{j}].categories"):
                if c not in cats:
                    raise ParityFixtureError(f"{where}.coverage[{j}]: unknown category '{c}'")
            for axis in _require_list(cov.get("axes"), f"{where}.coverage[{j}].axes"):
                _require_list(_require_map(axis, f"{where}.coverage[{j}].axis").get("values"), f"{where}.coverage[{j}].values")
    return shape


# --------------------------------------------------------------------------- inventory


def split_symbols(value: Any) -> List[str]:
    return [s.strip() for s in str(value).split(",") if s.strip()]


def load_inventory(path: Path) -> List[Dict[str, Any]]:
    """Parse through the ledger checker's own parser and partition rule (task 1)."""
    if not path.is_file():
        raise ParityFixtureError(f"{path}: file not found")
    try:
        rows = provenance.parse_inventory(str(path))
        provenance.check_partition(rows)
    except provenance.ProvenanceValidationError as e:
        raise ParityFixtureError(f"inventory: {e}")
    return rows


def validate_inventory(rows: List[Dict[str, Any]], shape: Dict[str, Any]) -> Dict[str, Dict[str, Any]]:
    """Closed-world inventory coverage against the shape's pin and adapter mapping table."""
    pin = shape["inventory"]
    pinned_rows: Dict[str, Dict[str, Any]] = pin["rows"]
    matrix_rows: Dict[Any, Any] = pin["matrix_rows"]
    by_id: Dict[str, Dict[str, Any]] = {}
    for row in rows:
        rid = row["id"]
        by_id[rid] = row
        owner = row["routine_owner"]
        if not isinstance(owner, str) or len(owner.split()) != 1 or "," in owner:
            raise ParityFixtureError(f"inventory row {rid}: routine_owner must be exactly one owner, got {owner!r}")
        if not isinstance(row["disposition"], str):
            raise ParityFixtureError(f"inventory row {rid}: disposition must be exactly one value")
        if not isinstance(row["adapter_id"], str):
            raise ParityFixtureError(f"inventory row {rid}: adapter_id must be exactly one value")
        if rid not in pinned_rows:
            raise ParityFixtureError(
                f"inventory row {rid} is not in the shape's pinned closed world (shape.inventory.rows)"
            )
        for field in INVENTORY_PIN_FIELDS:
            if not strict_eq(row[field], pinned_rows[rid][field]):
                raise ParityFixtureError(
                    f"inventory row {rid}: {field} {row[field]!r} differs from the pinned "
                    f"{pinned_rows[rid][field]!r}"
                )
        mrow = row["matrix_row"]
        if mrow not in matrix_rows:
            raise ParityFixtureError(f"inventory row {rid}: matrix_row {mrow!r} is not a pinned matrix row")
        if str(row.get("matrix_title", "")).strip() != str(matrix_rows[mrow]).strip():
            raise ParityFixtureError(
                f"inventory row {rid}: matrix_title {row.get('matrix_title')!r} differs from pinned "
                f"matrix row {mrow} title {matrix_rows[mrow]!r}"
            )
        adapter = row["adapter_id"]
        if adapter != "none" and row["source_path"] not in shape["adapters"][adapter]["donor_paths"]:
            raise ParityFixtureError(
                f"inventory row {rid}: adapter {adapter} does not map {row['source_path']} "
                f"(shape.adapters.{adapter}.donor_paths)"
            )
    removed = sorted(set(pinned_rows) - set(by_id))
    if removed:
        raise ParityFixtureError(f"inventory: pinned donor rows missing from donor_inventory.yaml: {removed}")
    uncovered = sorted(set(matrix_rows) - {r["matrix_row"] for r in rows}, key=str)
    if uncovered:
        raise ParityFixtureError(f"inventory: matrix rows with no inventory row: {uncovered}")

    # Every extracted donated routine has exactly one owner.
    owners: Dict[Tuple[str, str], Set[str]] = defaultdict(set)
    for row in rows:
        if provenance.normalize_disposition(row["disposition"]) == "donated_logic":
            for sym in split_symbols(row["source_symbol"]):
                owners[(row["source_path"], sym)].add(row["routine_owner"])
    for (path, sym), who in sorted(owners.items()):
        if len(who) > 1:
            raise ParityFixtureError(f"inventory: donated routine {path}:{sym} has several owners {sorted(who)}")
    return by_id


# --------------------------------------------------------------------------- cases

_PATHLIKE = re.compile(r"(/|\.(c|h|rs|py|php)$)")


def _nonempty_str(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def check_source_identity(
    src: Any, where: str, inventory: Dict[str, Dict[str, Any]], shape: Dict[str, Any], need_row: bool = True
) -> Dict[str, Any]:
    """A donor/host-policy source must match its task-1 inventory row byte for byte."""
    fields = list(shape["donor"]["source_fields"]) if need_row else list(shape["host_policy_fields"])
    src = _require_map(src, where)
    allowed = set(fields)
    if set(src) != allowed:
        raise ParityFixtureError(f"{where}: fields must be exactly {sorted(allowed)}, got {sorted(src)}")
    if str(src["source_commit"]) != str(shape["donor_commit"]):
        raise ParityFixtureError(
            f"{where}: forged source identity: source_commit {src['source_commit']!r} is not the "
            f"pinned donor commit {shape['donor_commit']}"
        )
    if need_row:
        candidates = [inventory[src["inventory_row"]]] if src["inventory_row"] in inventory else []
        if not candidates:
            raise ParityFixtureError(
                f"{where}: forged source identity: unknown inventory_row {src['inventory_row']!r}"
            )
    else:
        candidates = [r for r in inventory.values() if r["source_path"] == src["source_path"]]
        if not _nonempty_str(src.get("policy")):
            raise ParityFixtureError(f"{where}: host policy needs a non-empty policy statement")
    try:
        ranges = provenance.parse_ranges(src["source_range"])
    except provenance.ProvenanceValidationError as e:
        raise ParityFixtureError(f"{where}: {e}")
    symbols = split_symbols(src["source_symbol"])
    if not symbols:
        raise ParityFixtureError(f"{where}: empty source_symbol")
    for row in candidates:
        row_ranges = provenance.parse_ranges(row["source_range"])
        inside = all(any(lo >= a and hi <= b for a, b in row_ranges) for lo, hi in ranges)
        if (
            row["source_path"] == src["source_path"]
            and str(row["source_commit"]) == str(src["source_commit"])
            and set(symbols) <= set(split_symbols(row["source_symbol"]))
            and inside
        ):
            return row
    target = src.get("inventory_row", src["source_path"])
    raise ParityFixtureError(
        f"{where}: forged source identity: path/commit/symbol/range do not match inventory {target}"
    )


def check_footprint(fp: Any, where: str, shape: Dict[str, Any]) -> None:
    """Every byte names all eight bits exactly once, each with a known effect."""
    for i, byte in enumerate(_require_list(fp, where)):
        byte = _require_map(byte, f"{where}[{i}]")
        if not _is_int(byte.get("offset")):
            raise ParityFixtureError(f"{where}[{i}]: offset must be an integer")
        bits = _require_list(byte.get("bits"), f"{where}[{i}].bits")
        seen = []
        for b in bits:
            b = _require_map(b, f"{where}[{i}].bits")
            if set(b) != {"bit", "effect"}:
                raise ParityFixtureError(f"{where}[{i}]: each bit marking is exactly {{bit, effect}}")
            if b["effect"] not in shape["bit_effects"]:
                raise ParityFixtureError(f"{where}[{i}]: unknown bit effect {b['effect']!r}")
            seen.append(b["bit"])
        if sorted(seen, key=str) != list(range(8)) or not all(_is_int(x) for x in seen):
            raise ParityFixtureError(
                f"{where}[{i}]: physical footprint must mark bits 0-7 exactly once, got {seen}"
            )


def check_citation(cit: Any, where: str, shape: Dict[str, Any]) -> None:
    cit = _require_map(cit, where)
    authority = cit.get("authority")
    if authority not in shape["citation"]:
        raise ParityFixtureError(f"{where}: authority must be one of {sorted(shape['citation'])}, got {authority!r}")
    fields = shape["citation"][authority]
    if set(cit) != {"authority", *fields}:
        raise ParityFixtureError(
            f"{where}: {authority} citation must carry exactly {['authority', *fields]}, got {sorted(cit)}"
            + (" (no_standard never carries a quote)" if authority == "no_standard" else "")
        )
    for f in fields:
        if not _nonempty_str(cit[f]):
            raise ParityFixtureError(f"{where}: {f} must be a non-empty string")
    if authority == "standard" and _PATHLIKE.search(cit["document"].strip()):
        raise ParityFixtureError(
            f"{where}: document {cit['document']!r} is a source path, not a standard; donor host "
            f"policy belongs in host_policy with authority no_standard"
        )


def check_donor(case: Dict[str, Any], where: str, kind: str, shape: Dict[str, Any],
                inventory: Dict[str, Dict[str, Any]]) -> List[Dict[str, Any]]:
    donor = _require_map(case["donor"], f"{where}.donor")
    disp = donor.get("disposition")
    spec = shape["donor"]["dispositions"].get(disp)
    if spec is None:
        raise ParityFixtureError(f"{where}.donor: unknown disposition {disp!r}")
    allowed = {"disposition", *spec["required"], *shape["donor"]["optional"]}
    if set(donor) - allowed:
        raise ParityFixtureError(f"{where}.donor: unknown keys {sorted(set(donor) - allowed)}")
    for f in spec["required"]:
        if donor.get(f) in (None, "", [], {}):
            raise ParityFixtureError(f"{where}.donor: {disp} disposition requires {f}")
    kf = case["kmod_function"]
    if spec["kmod_function"] == "none" and kf != "none":
        raise ParityFixtureError(f"{where}: a {disp} case has kmod_function none, got {kf!r}")
    if spec["kmod_function"] == "adapter" and kf == "none":
        raise ParityFixtureError(f"{where}: an executable case names its test adapter in kmod_function")
    rows: List[Dict[str, Any]] = []
    if "sources" in spec["required"]:
        citable = shape["donor"]["citable_rows"][disp]
        for i, src in enumerate(_require_list(donor["sources"], f"{where}.donor.sources")):
            row = check_source_identity(src, f"{where}.donor.sources[{i}]", inventory, shape)
            if provenance.normalize_disposition(row["disposition"]) not in citable:
                raise ParityFixtureError(
                    f"{where}: {disp} case cites inventory row {row['id']} of disposition "
                    f"{row['disposition']} (allowed: {citable})"
                )
            if disp == "executable" and row["adapter_id"] != kf:
                raise ParityFixtureError(
                    f"{where}: kmod_function {kf} does not map row {row['id']} (its adapter is {row['adapter_id']})"
                )
            rows.append(row)
    if disp == "executable":
        if not _nonempty_str(donor["carrier"]):
            raise ParityFixtureError(f"{where}.donor: carrier must name the real donor carrier")
        if donor["pass"] not in shape["donor"]["passes"]:
            raise ParityFixtureError(f"{where}.donor: unknown pass {donor['pass']!r}")
    if "reason" in donor and not _nonempty_str(donor["reason"]):
        raise ParityFixtureError(f"{where}.donor: reason must be a non-empty string")
    return rows


def validate_case(case: Any, where: str, kind: str, shape: Dict[str, Any],
                  inventory: Dict[str, Dict[str, Any]]) -> Dict[str, Any]:
    case = _require_map(case, where)
    kspec = shape["kinds"][kind]
    cf = shape["case_fields"]
    missing = [f for f in cf["required"] if f not in case]
    unknown = sorted(set(case) - set(cf["required"]) - set(cf["optional"]))
    if missing or unknown:
        raise ParityFixtureError(f"{where}: missing required fields {missing}, unknown fields {unknown}")
    if not _nonempty_str(case["id"]):
        raise ParityFixtureError(f"{where}: id must be a non-empty string")
    where = f"{where} ({case['id']})"
    if case["category"] not in kspec["categories"]:
        raise ParityFixtureError(f"{where}: unknown {kind} category {case['category']!r}")
    category = kspec["categories"][case["category"]]
    if case["family"] not in shape["families"]:
        raise ParityFixtureError(f"{where}: unknown family {case['family']!r}")
    if not _nonempty_str(case["revision"]):
        raise ParityFixtureError(f"{where}: revision must be a non-empty string")
    if case["family"] == "cmis":
        supported = case["revision"] in shape["cmis_supported_revisions"]
        if supported == bool(category.get("unsupported_revision")):
            raise ParityFixtureError(
                f"{where}: CMIS revision {case['revision']!r} "
                + ("is supported but the category tests refusal" if supported
                   else "is outside ADR 0029's set; only an unsupported_revision category may use it")
            )
    if not _nonempty_str(case["routine"]):
        raise ParityFixtureError(f"{where}: routine must name the exact routine")

    kf = case["kmod_function"]
    adapter = shape["adapters"].get(kf) if isinstance(kf, str) else None
    if adapter is None:
        raise ParityFixtureError(f"{where}: unsupported kmod_function/adapter tag {kf!r}")
    if kind not in adapter["kinds"]:
        raise ParityFixtureError(f"{where}: adapter {kf} does not serve kind {kind}")
    if case["family"] not in adapter["families"]:
        raise ParityFixtureError(f"{where}: adapter {kf} cannot express family {case['family']}")

    inputs = case["inputs"]
    if not isinstance(inputs, dict) or not inputs:
        raise ParityFixtureError(f"{where}: inputs must be a non-empty mapping")
    for k in kspec["required_inputs"]:
        if k not in inputs:
            raise ParityFixtureError(f"{where}: inputs missing required key {k!r}")
    expected = _require_map(case["expected"], f"{where}.expected")
    etype = expected.get("type")
    if etype not in kspec["expected_types"]:
        raise ParityFixtureError(f"{where}: expected.type {etype!r} is not one of {sorted(kspec['expected_types'])}")
    for k in kspec["expected_types"][etype]:
        if k not in expected:
            raise ParityFixtureError(f"{where}: expected ({etype}) missing {k!r}")
    for facet in kspec["required_facets"]:
        if case.get(facet) is None:
            raise ParityFixtureError(f"{where}: {kind} cases require {facet}")

    check_citation(case["citation"], f"{where}.citation", shape)
    if "host_policy" in case:
        for i, hp in enumerate(_require_list(case["host_policy"], f"{where}.host_policy")):
            check_source_identity(hp, f"{where}.host_policy[{i}]", inventory, shape, need_row=False)
    if "time_anchors" in case:
        for k, v in _require_map(case["time_anchors"], f"{where}.time_anchors").items():
            if not _is_int(v) or v < 0:
                raise ParityFixtureError(f"{where}.time_anchors.{k} must be a non-negative integer (ms)")
    for facet in ("generation", "prior_state"):
        if facet in case:
            _require_map(case[facet], f"{where}.{facet}", nonempty=facet == "generation")
    if "supplied_results" in case:
        last = None
        for i, r in enumerate(_require_list(case["supplied_results"], f"{where}.supplied_results")):
            r = _require_map(r, f"{where}.supplied_results[{i}]")
            if not _is_int(r.get("seq")) or (last is not None and r["seq"] <= last):
                raise ParityFixtureError(f"{where}.supplied_results[{i}]: seq must be a strictly increasing integer")
            last = r["seq"]
    if "planned_effects" in case:
        for i, eff in enumerate(_require_list(case["planned_effects"], f"{where}.planned_effects", nonempty=False)):
            eff = _require_map(eff, f"{where}.planned_effects[{i}]")
            if eff.get("op") not in shape["planned_operations"]:
                raise ParityFixtureError(f"{where}.planned_effects[{i}]: unknown op {eff.get('op')!r}")
            if eff.get("requester") not in shape["requesters"]:
                raise ParityFixtureError(f"{where}.planned_effects[{i}]: unknown requester {eff.get('requester')!r}")
            check_footprint(eff.get("footprint"), f"{where}.planned_effects[{i}].footprint", shape)
    if "physical_footprint" in case:
        check_footprint(case["physical_footprint"], f"{where}.physical_footprint", shape)
    if "differences" in case:
        for d in _require_list(case["differences"], f"{where}.differences"):
            if not _nonempty_str(d):
                raise ParityFixtureError(f"{where}.differences: ids must be non-empty strings")

    rows = check_donor(case, where, kind, shape, inventory)
    if "requires" in adapter:
        bad = failed_predicates(case, adapter["requires"])
        if bad:
            raise ParityFixtureError(f"{where}: adapter {kf} cannot express this case: {bad}")
    bad = failed_predicates(case, kspec.get("constraints") or {})
    if bad:
        raise ParityFixtureError(f"{where}: {kind} constraints violated: {bad}")
    for j, rule in enumerate(kspec.get("rules") or []):
        if not failed_predicates(case, rule["when"]):
            bad = failed_predicates(case, rule["then"])
            if bad:
                raise ParityFixtureError(f"{where}: {kind} rule {j} violated: {bad}")
    bad = failed_predicates(case, category["requires"])
    if bad:
        raise ParityFixtureError(f"{where}: does not meet category {case['category']}: {bad}")
    case["_rows"] = [r["id"] for r in rows]
    return case


def _axis_value(case: Dict[str, Any], axis: Dict[str, Any]) -> Any:
    values = resolve(case, axis["path"])
    if len(values) != 1:
        return _MISSING
    value = values[0]
    if "bits" in axis:
        if not _is_int(value):
            return _MISSING
        hi, lo = axis["bits"]
        return (value >> lo) & ((1 << (hi - lo + 1)) - 1)
    return value


def validate_kind_file(path: Path, kind: str, shape: Dict[str, Any],
                       inventory: Dict[str, Dict[str, Any]]) -> List[Dict[str, Any]]:
    data = load_yaml(path)
    head = shape["case_file"]
    if not isinstance(data, dict) or set(data) != set(head["required"]):
        raise ParityFixtureError(f"{path.name}: a case file is exactly {head['required']}")
    if data["schema"] != head["schema"] or data["kind"] != kind:
        raise ParityFixtureError(f"{path.name}: header must be schema {head['schema']}, kind {kind}")
    if data["synthetic"] is not True:
        raise ParityFixtureError(f"{path.name}: parity fixtures are synthetic only (synthetic: true)")
    cases_raw = data["cases"]
    if not isinstance(cases_raw, list) or not cases_raw:
        raise ParityFixtureError(f"{path.name}: no cases (an empty case file fails closed)")
    cases = [validate_case(c, f"{path.name}[{i}]", kind, shape, inventory) for i, c in enumerate(cases_raw)]
    dup = sorted(k for k, n in Counter(c["id"] for c in cases).items() if n > 1)
    if dup:
        raise ParityFixtureError(f"{path.name}: duplicate case ids {dup}")

    kspec = shape["kinds"][kind]
    present = {c["category"] for c in cases}
    absent = [n for n, c in kspec["categories"].items() if c["required"] and n not in present]
    if absent:
        raise ParityFixtureError(f"{path.name}: required {kind} categories have no case: {absent}")
    for cov in kspec.get("coverage") or []:
        axes = cov["axes"]
        have = {
            tuple(repr(_axis_value(c, a)) for a in axes)
            for c in cases
            if c["category"] in cov["categories"]
        }
        combos = [()]
        for a in axes:
            combos = [x + (repr(v),) for x in combos for v in a["values"]]
        gaps = [x for x in combos if x not in have]
        if gaps:
            raise ParityFixtureError(f"{path.name}: coverage {cov['name']} missing combinations {gaps[:8]}"
                                     + (f" (+{len(gaps) - 8} more)" if len(gaps) > 8 else ""))
    return cases


# --------------------------------------------------------------------------- differences


def validate_differences(data: Any, shape: Dict[str, Any],
                         inventory: Dict[str, Dict[str, Any]]) -> Dict[str, Dict[str, Any]]:
    spec = shape["differences"]
    if not isinstance(data, dict) or set(data) != {"differences"}:
        raise ParityFixtureError("differences.yaml must be exactly {differences: [entry, ...]}")
    entries = _require_list(data["differences"], "differences")
    out: Dict[str, Dict[str, Any]] = {}
    for i, e in enumerate(entries):
        where = f"differences[{i}]"
        e = _require_map(e, where)
        missing = [f for f in spec["required"] if f not in e]
        unknown = sorted(set(e) - set(spec["required"]) - set(spec["optional"]))
        if missing or unknown:
            raise ParityFixtureError(f"{where}: missing fields {missing}, unknown fields {unknown}")
        if not _nonempty_str(e["id"]) or e["id"] in out:
            raise ParityFixtureError(f"{where}: id must be a unique non-empty string")
        where = f"differences {e['id']}"
        cases = _require_list(e["cases"], f"{where}.cases")
        if not all(_nonempty_str(c) for c in cases):
            raise ParityFixtureError(f"{where}: cases must be case id strings")
        if e["kmod_result"] in (None, "", [], {}):
            raise ParityFixtureError(f"{where}: kmod_result (donor result for the named cases) is required")
        if not _nonempty_str(e["adoption_item"]):
            raise ParityFixtureError(f"{where}: adoption_item must name the kmod-generated-tables item")
        if not isinstance(e["library_less_strict"], bool):
            raise ParityFixtureError(f"{where}: library_less_strict must be true or false")
        std = _require_map(e["standard"], f"{where}.standard")
        authority = std.get("authority")
        if authority not in spec["standard"]:
            raise ParityFixtureError(f"{where}.standard: authority must be standard or no_standard")
        fields = spec["standard"][authority]
        if set(std) != {"authority", *fields} or not all(_nonempty_str(std[f]) for f in fields):
            raise ParityFixtureError(f"{where}.standard: {authority} requires exactly non-empty {fields}")
        if authority == "standard" and _PATHLIKE.search(std["document"].strip()):
            raise ParityFixtureError(f"{where}.standard: document {std['document']!r} is a source path, not a standard")
        if authority == "no_standard" and e["library_less_strict"]:
            raise ParityFixtureError(
                f"{where}: the library is less strict than kmod with no standard; a less-strict "
                f"difference must cite the standard"
            )
        if "comparison" in e and e["comparison"] not in spec["comparisons"]:
            raise ParityFixtureError(f"{where}: comparison must be one of {spec['comparisons']}")
        for j, src in enumerate(e.get("donor_sources") or []):
            check_source_identity(src, f"{where}.donor_sources[{j}]", inventory, shape)
        out[e["id"]] = e
    missing_ids = sorted(set(spec["required_ids"]) - set(out))
    if missing_ids:
        raise ParityFixtureError(f"differences.yaml: required entries missing: {missing_ids}")
    return out


# --------------------------------------------------------------------------- directory


def scan_directory(directory: Path, shape: Dict[str, Any]) -> None:
    """Every YAML file is metadata or a kind's case file; nothing else is silently skipped."""
    if not directory.is_dir():
        raise ParityFixtureError(f"--dir {directory}: parity fixture directory not found")
    known = set(shape["metadata_files"]) | {k["file"] for k in shape["kinds"].values()}
    for p in sorted(directory.iterdir()):
        if p.suffix in (".yaml", ".yml") and p.name not in known:
            raise ParityFixtureError(
                f"{p.name}: unrecognized YAML in the parity directory; it is neither metadata nor a "
                f"case file of the nine kinds"
            )


class Result:
    def __init__(self) -> None:
        self.cases: Dict[str, List[Dict[str, Any]]] = {}
        self.gaps: List[str] = []
        self.notes: List[str] = []


def check(directory: Path = DEFAULT_DIR, kinds: Optional[List[str]] = None,
          inventory_only: bool = False, shape_path: Optional[Path] = None,
          inventory_path: Optional[Path] = None) -> Result:
    """Run the checker. kinds=None and inventory_only=False is the full closed-world run."""
    result = Result()
    shape = validate_shape(load_yaml(shape_path or directory / "_shape.yaml"))
    scan_directory(directory, shape)
    inventory = validate_inventory(load_inventory(inventory_path or directory / "donor_inventory.yaml"), shape)
    if inventory_only:
        return result

    full = kinds is None
    selected = list(KINDS) if full else kinds
    unauthored: List[str] = []
    for k in selected:
        if k not in shape["kinds"]:
            raise ParityFixtureError(f"unknown kind {k!r}; kinds are {list(KINDS)}")
        path = directory / shape["kinds"][k]["file"]
        if full and not path.is_file():
            # The fixture kinds land one task at a time: a kind whose case
            # file does not exist yet is an explicit delivery gap, printed
            # and never silently skipped. Its closure checks re-engage as
            # hard errors the moment the file lands; an explicitly
            # requested --kind below still fails closed on a missing file.
            unauthored.append(k)
            result.gaps.append(
                f"{k}: case file {shape['kinds'][k]['file']} not authored yet — structural "
                f"coverage of this kind is an explicit delivery gap"
            )
            continue
        result.cases[k] = validate_kind_file(path, k, shape, inventory)

    def kinds_served_by(adapter_id: str) -> List[str]:
        return list(shape["adapters"][adapter_id]["kinds"])

    def gating_kinds_authored(adapter_id: str) -> bool:
        """Every kind the adapter serves has an authored case file."""
        return all(k in result.cases for k in kinds_served_by(adapter_id))

    all_cases = [c for cs in result.cases.values() for c in cs]
    dup = sorted(i for i, n in Counter(c["id"] for c in all_cases).items() if n > 1)
    if dup:
        raise ParityFixtureError(f"duplicate case ids across case files: {dup}")
    by_id = {c["id"]: c for c in all_cases}

    diff_path = directory / "differences.yaml"
    diffs: Optional[Dict[str, Dict[str, Any]]] = None
    if diff_path.exists():
        diffs = validate_differences(load_yaml(diff_path), shape, inventory)
    elif full:
        raise ParityFixtureError("differences.yaml: missing; the full run requires the difference list")
    cited = sorted({d for c in all_cases for d in c.get("differences") or []})
    if diffs is None:
        if cited:
            result.notes.append(
                f"difference ids {cited} are unresolved: differences.yaml does not exist yet; the full "
                f"run fails until they resolve"
            )
    else:
        unknown = [d for d in cited if d not in diffs]
        if unknown:
            raise ParityFixtureError(f"cases cite unknown difference entries {unknown}")
        for c in all_cases:
            for d in c.get("differences") or []:
                if c["id"] not in diffs[d]["cases"]:
                    raise ParityFixtureError(f"case {c['id']} cites {d}, but {d} does not name it")
        if full:
            for d, e in diffs.items():
                for cid in e["cases"]:
                    if cid not in by_id:
                        if unauthored:
                            # The entry names a case in a kind that has not
                            # landed yet; once every kind is authored this
                            # is a hard error again.
                            result.gaps.append(
                                f"differences {d}: case {cid} is not authored yet (unauthored "
                                f"kinds: {', '.join(sorted(unauthored))})"
                            )
                            continue
                        raise ParityFixtureError(f"differences {d} names {cid}, which is not a case")
                    if d not in (by_id[cid].get("differences") or []):
                        raise ParityFixtureError(f"differences {d} names {cid}, which does not cite it")

    if full:
        # Inventory fixture-id closure: every donated routine maps to actual cases executed
        # through its adapter; a non-donated row's missing case is reported as its explicit gap.
        for rid, row in inventory.items():
            donated = provenance.normalize_disposition(row["disposition"]) == "donated_logic"
            for fid in row["required_fixture_ids"]:
                case = by_id.get(fid)
                if case is None:
                    if donated and gating_kinds_authored(row["adapter_id"]):
                        raise ParityFixtureError(
                            f"inventory row {rid}: donated routine's required fixture {fid} is not a case"
                        )
                    reason = (
                        f"fixture not authored yet (adapter {row['adapter_id']} serves unauthored "
                        f"kinds {', '.join(kinds_served_by(row['adapter_id']))})"
                        if donated
                        else row["source_only_gap"]
                    )
                    result.gaps.append(f"{rid}: {fid} absent ({reason})")
                    continue
                if donated and (case["donor"]["disposition"] != "executable"
                                or case["kmod_function"] != row["adapter_id"]):
                    raise ParityFixtureError(
                        f"inventory row {rid}: fixture {fid} must execute through adapter {row['adapter_id']}"
                    )
                if rid not in case["_rows"]:
                    raise ParityFixtureError(f"inventory row {rid}: fixture {fid} does not cite the row")
    return result


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Structural closed-world parity fixture checker (D6)")
    parser.add_argument("--dir", type=Path, default=DEFAULT_DIR, help="parity fixture directory")
    parser.add_argument("--kind", action="append", choices=KINDS,
                        help="validate only this kind's case file (repeatable)")
    parser.add_argument("--inventory", nargs="?", const="", default=None,
                        help="validate only the shape and donor inventory (optional inventory path)")
    parser.add_argument("--authorship", type=Path, help="Retained history JSON (default: --dir/authorship.json)")
    args = parser.parse_args(argv)
    if not args.dir.is_dir():
        parser.error(f"--dir: not a directory: {args.dir}")
    if args.kind and args.inventory is not None:
        parser.error("--kind and --inventory are separate modes")
    inv_path = Path(args.inventory) if args.inventory else None
    try:
        provenance.load_authorship(args.authorship or args.dir / "authorship.json")
        result = check(args.dir, kinds=args.kind, inventory_only=args.inventory is not None,
                       inventory_path=inv_path)
    except (ParityFixtureError, provenance.ProvenanceValidationError, OSError) as e:
        print(f"PARITY FIXTURE CHECK FAILED (--dir/--inventory/--authorship): {e}", file=sys.stderr)
        return 1
    mode = "inventory" if args.inventory is not None else ("kind " + ",".join(args.kind) if args.kind else "full")
    print(f"OpenXCVR parity fixtures ({mode}): structurally valid")
    for k, cases in result.cases.items():
        disp = Counter(c["donor"]["disposition"] for c in cases)
        print(f"  {k}: {len(cases)} cases (executable {disp['executable']}, "
              f"source_only {disp['source_only']}, none {disp['none']})")
    for g in result.gaps:
        print(f"  gap: {g}")
    for n in result.notes:
        print(f"  note: {n}")
    print("  Structural only: no adapter was executed and no parity is claimed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
