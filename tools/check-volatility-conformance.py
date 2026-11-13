#!/usr/bin/env python3
"""Volatility conformance tool.

Compares each family's schema volatility markings with the applicable parsed
standard under docs/stds/.parsed/markdown of the checkout named by
OPENXCVR_ROOT (that cache is not part of this repository, so this comparison
is a local tool; CI re-verifies the committed receipts instead):

- CMIS 3.0, 4.0, 5.0, 5.1, 5.2 and 5.3 against each revision's field-table
  Type entries, access-type definitions, reserved-location conventions and
  latched-flag prose (the "L-" convention and "Clear on Read" descriptions in
  3.0/4.0, the RO/COR access type in 5.x).
- SFF-8636 against the revision declared by its schemas, using its writable
  memory blocks (RO/RW/WO), the byte-2 startup-interrupt prose and the bytes
  3-21 alarm-latch prose.
- SFF-8472 Rev 12.5a against its per-bit Status/Control definitions
  (Table 9-16), the RO/RW prose (S:8.10) and the latched-flag text (S:9.9).

Every marked schema field and range is accounted for: it is compared against
the applicable standard content, or recorded with an explicit error kind.  A
missing or unparseable marking, an unresolved input, a skipped field/range, an
empty comparison, a missing applicable standard file, a marking with no
standard authority, borrowing another revision's authority, claiming a 2.11
comparison from 2.12-only input, acknowledging the startup interrupt while
hiding an Unknown constituent, or an omitted byte-scoped effect is an error,
never a zero-mismatch success.

A comparison's status is set only after account_completeness, one step that
checks five invariants against the whole inventory rather than at individual
skip sites: (1) every field the hashed schema closure declares is loaded or
reported and at least one is compared; (2) a field is excluded at a revision
only by its own since/until, and every other (field, revision) cell is
compared or unresolved_at_applicable_revision; (3) a known read effect cites a
standard source, and "none" for a byte needs eight established constituents;
(4) conflicting authority (rows, writable blocks, byte effects, overlapping
fields, templates) is an error, never resolved by order; (5) whole-byte
qualification needs eight compared, conformant constituents with established
access and effect and no mismatch on the byte.

Expected values are read from the parsed standard text and from the completed
authority reports in the directory passed as --authority-dir (maintainer
evidence, not part of this repository): each
convention passage used by the comparison must be present in the parsed
standard text and is looked up in the authority report for its citation, and
the receipt records where it was read.  Interpretations (e.g. the CMIS 4.0-5.3
reserved-location reading - reserved locations are accessible,
zero-initialized and carry no other module obligations, supporting a
side-effect-free read) are recorded as inference, separately from primary
authority, and are applied only to their own revision: never borrowed for
CMIS 3.0, whose byte-scoped reserved statement leaves bits 4-7 Unknown.

An honestly Unknown marking can conform wherever the standard leaves the read
effect undefined or the effect rests on interpretation, but it never
establishes safe readiness: receipts distinguish access permission, change
source, known read effects, Unknown/uncovered masks and whether a whole
physical byte is qualified for a host read.  SFF-8636 byte 2 carries
AcknowledgeStartupInterrupt, distinct from the alarm-latch effects in bytes
3-21; the receipt does not require the byte-2 read effect to be None and does
not assert that other pending interrupts are cleared.

`--write` emits one receipt per family recording the checked revisions,
standard-file hashes, the deterministic path-and-content hash of the complete
schema-input closure used to resolve the checked markings (page and field
files, family indexes, inherited templates, includes and revision metadata;
compute_schema_closure is the canonical closure hash reused by
tools/validate-schemas.py), the compared field/byte inventory and every
mismatch; it exits non-zero when any written receipt fails.  `--check` needs
no parsed standards: it recomputes closure membership and content hashes
(added, removed or changed relevant inputs make the receipt stale),
re-validates the receipt rules, re-runs the completeness accounting against
the receipt's inventory (recheck_accounting) and fails on any stale receipt,
rule violation, accounting error the receipt does not record, or recorded
mismatch.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - _load_yaml then raises; nothing is compared silently
    yaml = None  # type: ignore[assignment]

# ---------------------------------------------------------------------------
# Canonical D1 vocabulary (shared with task 18's validator gate)
# ---------------------------------------------------------------------------

ALLOWED_ACCESS = frozenset({"ro", "rw", "rww", "wo", "wo_sc", "ro_cor", "unknown"})
# Access spellings retired by the D1 migration.
RETIRED_ACCESS_SPELLINGS = frozenset({"cor", "rw_with_state_guard"})
# behavior.latching leaves the vocabulary; its values are retired spellings.
RETIRED_LATCHING_SPELLINGS = frozenset(
    {"clear_on_read", "clear_on_read_when_condition_clears", "implementation_defined"}
)

SUPPORTED_FAMILIES = ("cmis", "sff-8636", "sff-8472")

CMIS_REVISIONS = ("3.0", "4.0", "5.0", "5.1", "5.2", "5.3")
SFF8472_REVISIONS = ("12.5a",)

# The applicable parsed standard file per (family, revision).  This table is
# the applicability map: a receipt may only cite the file of its own revision,
# and a missing applicable file is an error, never a silent skip.
STANDARD_FILENAMES = {
    ("cmis", "3.0"): "cmis__CMIS3p0_Third_Party_Spec.md",
    ("cmis", "4.0"): "cmis__CMIS4p0_Third_Party_Spec.md",
    ("cmis", "5.0"): "cmis__CMIS5p0_Third_Party_Spec.md",
    ("cmis", "5.1"): "cmis__CMIS5p1_Third_Party_Spec.md",
    ("cmis", "5.2"): "cmis__OIF-CMIS-05.2.md",
    ("cmis", "5.3"): "cmis__OIF-CMIS-05.3.md",
    ("sff-8636", "2.11"): "sff-8636__SFF-8636-Rev2.11.md",
    ("sff-8636", "2.12"): "sff-8636__SFF-8636-Rev2.12.md",
    ("sff-8472", "12.5a"): "sff-8472__SFF-8472-Rev12.5a.md",
}
FILE_TO_REVISIONS: dict[str, list[tuple[str, str]]] = {}
for _fam, _rev in STANDARD_FILENAMES:
    FILE_TO_REVISIONS.setdefault(STANDARD_FILENAMES[(_fam, _rev)], []).append((_fam, _rev))

# Completed authority reports whose research this tool reuses (fail closed if
# absent when writing receipts).
AUTHORITY_REPORT_FILES = {
    "cmis": "cmis-byte3-authority.json",
    "sff-8636": "sff8636-byte2-authority.json",
    "sff-8472": "sff8472-byte110-authority.json",
}

# Byte-scoped effects recorded in receipts.
EFFECT_ACK_STARTUP = "acknowledge_startup_interrupt"
EFFECT_CLEAR_ON_READ = "clear_on_read"
EFFECT_ALARM_LATCH = "interrupt_flag_clear_on_read"

# Mismatch kinds (each is an error, never a zero-mismatch pass).
KIND_MISSING_MARKING = "missing_marking"
KIND_UNPARSEABLE_MARKING = "unparseable_marking"
KIND_RETIRED_MARKING = "retired_marking_spelling"
KIND_UNRESOLVED_INPUT = "unresolved_input"
KIND_SKIPPED_FIELD = "skipped_field_or_range"
KIND_EMPTY_COMPARISON = "empty_comparison"
KIND_MISSING_STANDARD = "missing_applicable_standard_comparison"
KIND_NO_STANDARD_AUTHORITY = "no_standard_authority"
KIND_ACCESS_MISMATCH = "access_mismatch"
KIND_MISSING_RESERVED_BIT = "missing_reserved_bit_comparison"
KIND_OMITTED_BYTE_EFFECT = "omitted_byte_scoped_effect"
KIND_BORROWING = "borrowing_another_revision_authority"
KIND_CLAIM_211 = "claiming_211_comparison_from_212_only_input"
KIND_HIDDEN_UNKNOWN = "acknowledging_startup_while_hiding_unknown_constituent"
KIND_MIXED_ACCESS = "mixed_access_not_expanded"
KIND_MISSING_AUTHORITY_REPORT = "missing_authority_report"
KIND_PASSAGE_NOT_FOUND = "authority_passage_not_found_in_standard"
# A row of a parsed standard table that cannot be placed (unparseable span,
# unknown operation spelling, no page context) is recorded, never dropped.
KIND_UNPARSEABLE_ROW = "unparseable_standard_row"
# Equally specific standard rows that disagree about one bit.
KIND_CONFLICTING_ROWS = "conflicting_standard_rows"
# Overlapping schema fields that cover one bit with disagreeing markings.
KIND_OVERLAPPING_FIELDS = "overlapping_fields_disagree"
# Completeness accounting (account_completeness): each is an error that
# blocks a passing receipt.
# A field the schema closure declares that was neither loaded nor reported,
# or a closure file / reference form the loader cannot resolve.
KIND_UNACCOUNTED_INPUT = "unaccounted_schema_input"
# A field applicable at a revision (no since/until excludes it) whose bits
# no typed standard row compared there: unresolved authority, never a pass.
KIND_UNRESOLVED_AT_REVISION = "unresolved_at_applicable_revision"
# A recorded access or read effect with no standard authority behind it.
KIND_UNESTABLISHED_AUTHORITY = "unestablished_authority"
# Conflicting authority sources (writable-block rows, template definitions)
# that would otherwise be resolved by order.
KIND_CONFLICTING_AUTHORITY = "conflicting_authority"
# A whole-byte qualification not backed by eight compared, established,
# conformant constituents.
KIND_UNJUSTIFIED_QUALIFICATION = "unjustified_whole_byte_qualification"


class InputError(Exception):
    """An input reference failed to resolve (missing file, unparseable YAML)."""


# ---------------------------------------------------------------------------
# Text helpers
# ---------------------------------------------------------------------------

_MD_ESCAPE_RE = re.compile(r"\\([_#*<>|\[\]()`])")
_WS_RE = re.compile(r"\s+")


def norm_text(s: str | None) -> str:
    """Normalize markdown text for comparison (unescape, collapse space)."""
    if not s:
        return ""
    s = _MD_ESCAPE_RE.sub(r"\1", s)
    return _WS_RE.sub(" ", s).strip()


_SPACE_BEFORE_PUNCT_RE = re.compile(r"\s+([,.;:)])")


def passage_norm(s: str | None) -> str:
    """Normalize text for passage search: PDF extraction inserts stray spaces
    before punctuation (e.g. "accessible , albeit"), which must not hide a
    passage that is present."""
    return _SPACE_BEFORE_PUNCT_RE.sub(r"\1", norm_text(s))


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# ---------------------------------------------------------------------------
# Schema input resolution (fail closed: no silent drops)
# ---------------------------------------------------------------------------


def _load_yaml(path: Path) -> Any:
    if yaml is None:
        raise RuntimeError("PyYAML is required for schema resolution")
    try:
        with open(path, "r", encoding="utf-8") as f:
            return yaml.safe_load(f)
    except Exception as e:  # noqa: BLE001 - reported as unresolved input
        raise InputError(f"Failed to parse YAML file {path}: {e}") from e


def _iter_reference_keys() -> tuple[str, ...]:
    return ("imports", "includes", "field_includes", "cdb_commands", "state_machines", "tom_messages")


# Closure-file kinds that declare no memory-map fields of the family's pages:
# their own payload (enum entries, CDB/TOM message fields, state machines) is
# not a page volatility marking.  Any other file in the closure that declares
# fields must be loaded by load_family_fields or reported.
_NON_MEMORY_MAP_KINDS = frozenset(
    {"enum", "enum_registry", "cdb_command", "state_machine", "tom_message"}
)


def _decl_path(path: Path, schemas_dir: Path) -> str:
    """Stable identity of a schema file for input accounting."""
    try:
        return path.resolve().relative_to(schemas_dir.resolve()).as_posix()
    except ValueError:
        return path.resolve().as_posix()  # outside schemas_dir: still identified


def schema_declarations(family: str, schemas_dir: Path) -> dict[str, Any]:
    """Independent inventory of the memory-map fields the closure declares.

    Walks every file ``compute_schema_closure`` hashes and records each
    field entry it declares as ``file#index`` (or ``file#*`` when the file's
    field payload has a shape no entry can be read from), with the page file
    through which the loader reaches it.  It does not apply the loader's
    rules; ``account_completeness`` requires every declared entry to be
    loaded or reported by an error.  A file that declares fields but is not
    a page included by the family index, nor a ``field_includes`` fragment
    of one (a nested reference, an include of an include), is unsupported:
    it is returned as an error instead of being hashed and never compared.
    """
    schemas_dir = Path(schemas_dir)
    index_path = (schemas_dir / family / "_index.yaml").resolve()
    files = find_schema_closure_files(family, schemas_dir)
    data = {p: _load_yaml(p) for p in files}

    index_data = data.get(index_path)
    index_refs = index_data if isinstance(index_data, dict) else {}
    imported = {(index_path.parent / str(r)).resolve() for r in index_refs.get("imports") or []}
    included = {(index_path.parent / str(r)).resolve() for r in index_refs.get("includes") or []}
    # fragment path -> the page files that reference it
    fragments: dict[Path, list[Path]] = {}
    for p in included:
        d = data.get(p)
        if isinstance(d, dict) and d.get("kind") == "page":
            for r in d.get("field_includes") or []:
                fragments.setdefault((p.parent / str(r)).resolve(), []).append(p)

    declared: dict[str, dict[str, Any]] = {}
    errors: list[dict[str, Any]] = []

    def _declare(path: Path, payload: Any, pages: list[Path]) -> None:
        rel = _decl_path(path, schemas_dir)
        page_rels = [_decl_path(pg, schemas_dir) for pg in pages]
        if isinstance(payload, list):
            for idx, raw in enumerate(payload):
                declared[f"{rel}#{idx}"] = {
                    "file": rel, "pages": page_rels,
                    "name": raw.get("name") if isinstance(raw, dict) else None,
                }
        elif payload:
            declared[f"{rel}#*"] = {"file": rel, "pages": page_rels, "name": None}

    def _carries_fields(d: Any) -> bool:
        # Any memory-map field payload form: a top-level list of entries, a
        # fields list, or field_includes naming more of them.
        return isinstance(d, list) or (
            isinstance(d, dict) and bool(d.get("fields") or d.get("field_includes"))
        )

    # Each closure file lands in exactly one branch below (first match wins,
    # in the order the loader reads them); ``classification`` records it.
    classification: dict[str, str] = {}
    for p in files:
        d = data[p]
        rel = _decl_path(p, schemas_dir)
        kind = d.get("kind") if isinstance(d, dict) else None
        if p in fragments:
            # Before the kind skip: load_family_fields loads a field_includes
            # fragment's fields whatever its kind, so they are declared too.
            classification[rel] = "field_includes_fragment"
            _declare(p, d if isinstance(d, list) else (d.get("fields") if isinstance(d, dict) else d),
                     fragments[p])
        elif isinstance(d, dict) and kind in _NON_MEMORY_MAP_KINDS:
            # Its payload is not a page volatility marking (see
            # _NON_MEMORY_MAP_KINDS); a non-page include still meets the
            # loader's non-page-include check.
            classification[rel] = "non_memory_map"
        elif p in included:
            classification[rel] = "page_include" if (
                isinstance(d, dict) and kind == "page") else "non_page_include"
            if isinstance(d, dict) and kind == "page":
                _declare(p, d.get("fields"), [p])
            else:
                # A non-page include: whatever field payload it carries is
                # declared, so the loader must report it.
                _declare(p, d if isinstance(d, list) else (
                    (d.get("fields") or d.get("field_includes")) if isinstance(d, dict) else None
                ), [p])
        elif p == index_path or p in imported:
            # load_family_fields reads only the index's includes (and
            # load_family_templates only templates from index and imports):
            # any field payload form here is never loaded.
            classification[rel] = "index_or_import"
            if _carries_fields(d):
                errors.append({
                    "kind": KIND_UNACCOUNTED_INPUT, "decl_file": rel,
                    "message": (
                        f"index or imported file {rel!r} declares fields (a top-level list, "
                        "fields or field_includes) outside any page included by the family "
                        "index; load_family_fields never loads them, so they are never compared"
                    ),
                })
        elif _carries_fields(d):
            classification[rel] = "unsupported_nested_reference"
            errors.append({
                "kind": KIND_UNACCOUNTED_INPUT, "decl_file": rel,
                "message": (
                    f"closure file {rel!r} declares fields but is reached only through a "
                    "nested reference (not a page included by the family index nor a "
                    "field_includes fragment of one); this schema form is unsupported and "
                    "its fields would be hashed but never compared"
                ),
            })
        else:
            classification[rel] = "no_field_payload"
    return {"declared": declared, "errors": errors, "classification": classification}


def template_definitions(family: str, schemas_dir: Path) -> dict[str, list[dict[str, Any]]]:
    """Every definition of each template name in the family index and its
    imports, with its source: two differing definitions of one name are an
    order-dependent resolution (account_completeness records it)."""
    index_file = schemas_dir / family / "_index.yaml"
    data = _load_yaml(index_file)
    out: dict[str, list[dict[str, Any]]] = {}
    if not isinstance(data, dict):
        return out  # load_family_templates raises InputError for this index
    sources: list[tuple[Path, Any]] = []
    for imp in data.get("imports") or []:
        imp_path = (index_file.parent / str(imp)).resolve()
        sources.append((imp_path, _load_yaml(imp_path) if imp_path.is_file() else None))
    sources.append((index_file.resolve(), data))
    for path, d in sources:
        for name, body in ((d or {}).get("templates") or {}).items() if isinstance(d, dict) else []:
            out.setdefault(name, []).append({"source": _decl_path(path, schemas_dir), "body": body})
    return out


def find_schema_closure_files(family: str, schemas_dir: Path) -> list[Path]:
    """Find all files in the dependency closure of a schema family.

    Follows _index.yaml imports, includes, field_includes and command/state
    machine references recursively.  Raises InputError on any missing or
    unparseable input: the conformance closure is never silently narrowed.
    """
    family_dir = schemas_dir / family
    index_file = family_dir / "_index.yaml"
    if not index_file.is_file():
        raise InputError(f"Missing _index.yaml for family '{family}' at {index_file}")

    closure: set[Path] = set()
    queue: list[Path] = [index_file.resolve()]

    while queue:
        current = queue.pop(0)
        if current in closure:
            continue  # already hashed once: a duplicate reference adds nothing
        if not current.is_file():
            raise InputError(f"Referenced schema file does not exist: {current}")
        closure.add(current)

        data = _load_yaml(current)
        if not isinstance(data, dict):
            continue  # hashed above; a list/scalar file carries no reference keys to follow

        parent = current.parent
        for key in _iter_reference_keys():
            for ref in data.get(key) or []:
                ref_path = (parent / str(ref)).resolve()
                if ref_path not in closure:
                    queue.append(ref_path)

    return sorted(closure)


def compute_schema_closure(
    family: str, schemas_dir: Path | str, repo_root: Path | str | None = None
) -> tuple[str, list[dict[str, str]]]:
    """Compute the deterministic path-and-content closure hash and file list.

    This is the canonical closure hash implementation; tools/validate-schemas.py
    reuses this exact function so a receipt can only be refreshed together with
    the validator's view of the schemas.
    """
    schemas_path = Path(schemas_dir).resolve()
    root_path = Path(repo_root).resolve() if repo_root else schemas_path.parent

    files = find_schema_closure_files(family, schemas_path)
    file_records: list[dict[str, str]] = []
    hasher = hashlib.sha256()

    for fpath in files:
        try:
            rel_path = fpath.relative_to(root_path).as_posix()
        except ValueError:
            rel_path = fpath.as_posix()  # outside repo_root: still hashed, under its absolute path
        file_records.append({"path": rel_path, "sha256": sha256_file(fpath)})

    file_records.sort(key=lambda x: x["path"])
    for rec in file_records:
        hasher.update(f"{rec['path']}:{rec['sha256']}\n".encode("utf-8"))

    return hasher.hexdigest(), file_records


def load_family_templates(family: str, schemas_dir: Path) -> dict[str, dict[str, Any]]:
    """Load templates from the family index and imported indexes (fail closed)."""
    family_dir = schemas_dir / family
    index_file = family_dir / "_index.yaml"
    if not index_file.is_file():
        raise InputError(f"Missing _index.yaml for family '{family}' at {index_file}")

    data = _load_yaml(index_file)
    if not isinstance(data, dict):
        raise InputError(f"Family index is not a mapping: {index_file}")

    templates: dict[str, dict[str, Any]] = {}
    # Inherited templates first; family templates override.
    for imp in data.get("imports") or []:
        imp_path = (index_file.parent / str(imp)).resolve()
        if not imp_path.is_file():
            raise InputError(f"Imported index does not exist: {imp_path}")
        imp_data = _load_yaml(imp_path)
        if not isinstance(imp_data, dict):
            raise InputError(f"Imported index is not a mapping: {imp_path}")
        templates.update(imp_data.get("templates") or {})
    templates.update(data.get("templates") or {})
    return templates


class SchemaField:
    """A resolved schema field occurrence with its volatility marking."""

    __slots__ = (
        "name", "page_id", "page_address", "page_select", "origin",
        "byte_lo", "byte_hi", "bit_lo", "bit_hi", "access", "is_dynamic",
        "marking_source", "template", "retired", "since", "until", "present_when",
        # decl: the declaring (file#index) entry; uid: this occurrence;
        # overrides: per-revision override keys this tool does not resolve.
        "decl", "uid", "overrides",
    )

    def __init__(self, **kw: Any) -> None:
        for slot in self.__slots__:
            setattr(self, slot, kw.get(slot))

    @property
    def bit_mask(self) -> int:
        if self.bit_lo is None:
            return 0xFF
        mask = 0
        for b in range(self.bit_lo, (self.bit_hi if self.bit_hi is not None else self.bit_lo) + 1):
            mask |= 1 << b
        return mask

    def to_record(self) -> dict[str, Any]:
        return {
            "field_name": self.name,
            "field_id": self.uid,
            "page": self.page_id,
            "page_address": self.page_address,
            "page_select": self.page_select,
            "origin": self.origin,
            "byte_span": [self.byte_lo, self.byte_hi],
            "bits": None if self.bit_lo is None else [self.bit_lo, self.bit_hi],
            "access": self.access,
            "is_dynamic": self.is_dynamic,
            "change_source": (
                None if self.is_dynamic is None
                else ("module" if self.is_dynamic else "host_or_neither")
            ),
            "marking_source": self.marking_source,
            "template": self.template,
            "retired_spellings": sorted(self.retired),
            # Revision applicability is a compared marking: recorded (and so
            # hashed) with the field.
            "since": None if self.since is None else str(self.since),
            "until": None if self.until is None else str(self.until),
        }


def _eval_int_expr(expr: str, env: dict[str, int]) -> int:
    """Safely evaluate a simple integer expression over instance indices."""
    try:
        tree = ast.parse(expr.strip(), mode="eval")
    except SyntaxError as e:
        raise InputError(f"Unparseable offset expression {expr!r}: {e}") from e

    def ev(node: ast.AST) -> int:
        if isinstance(node, ast.Expression):
            return ev(node.body)
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.Name):
            if node.id not in env:
                raise InputError(f"Unknown name {node.id!r} in expression {expr!r}")
            return env[node.id]
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd)):
            val = ev(node.operand)
            return -val if isinstance(node.op, ast.USub) else val
        if isinstance(node, ast.BinOp):
            left, right = ev(node.left), ev(node.right)
            if isinstance(node.op, ast.Add):
                return left + right
            if isinstance(node.op, ast.Sub):
                return left - right
            if isinstance(node.op, ast.Mult):
                return left * right
            if isinstance(node.op, (ast.Div, ast.FloorDiv)):
                if right == 0:
                    raise InputError(f"Division by zero in expression {expr!r}")
                # Schema expressions use '/' with integer semantics.
                return left // right
            if isinstance(node.op, ast.Mod):
                return left % right
        raise InputError(f"Unsupported construct in expression {expr!r}")

    return ev(tree)


def _resolve_field_marking(
    field: dict[str, Any], templates: dict[str, dict[str, Any]], origin: str
) -> tuple[Any, Any, str, str | None, list[str]]:
    """Resolve (access, is_dynamic, marking_source, template, retired) for a field.

    The retired spellings keep their semantic reading so the comparison can
    still report what the marking claims, but every retired spelling is an
    error: the D1 vocabulary has one spelling.
    """
    retired: list[str] = []
    for key in ("flags", "behavior"):
        if field.get(key) is not None and not isinstance(field.get(key), dict):
            # A non-mapping flags/behavior would hide is_dynamic or latching.
            raise InputError(
                f"Field {field.get('name')!r} {key} is not a mapping ({origin}); "
                "its volatility marking cannot be read"
            )
    access = field.get("access")
    is_dynamic = (field.get("flags") or {}).get("is_dynamic")
    marking_source = "field" if access is not None or is_dynamic is not None else "none"
    template_name = field.get("template")

    if template_name is not None:
        if template_name not in templates:
            raise InputError(
                f"Field {field.get('name')!r} references unknown template {template_name!r} ({origin})"
            )
        tmpl = templates[template_name] or {}
        if not isinstance(tmpl, dict) or not isinstance(tmpl.get("behavior") or {}, dict) \
                or not isinstance(tmpl.get("flags") or {}, dict):
            # A non-mapping template (or its flags/behavior) would hide the
            # inherited marking: an unresolved input, never a silent default.
            raise InputError(
                f"Template {template_name!r} (or its flags/behavior) is not a mapping ({origin})"
            )
        if access is None and "access" in tmpl:
            access = tmpl.get("access")
        if is_dynamic is None:
            is_dynamic = (tmpl.get("flags") or {}).get("is_dynamic")
        if marking_source == "none" and (
            "access" in tmpl or (tmpl.get("flags") or {}).get("is_dynamic") is not None
        ):
            marking_source = f"template:{template_name}"

    def _apply_latching(latching: Any, prefix: str) -> None:
        nonlocal access
        if latching is None:
            return  # no behavior.latching key: no retired spelling to record
        retired.append(f"{prefix}={latching}")
        if latching in ("clear_on_read", "clear_on_read_when_condition_clears"):
            if access in (None, "ro"):
                access = "ro_cor"
        elif latching == "implementation_defined":
            if access in (None, "ro"):
                access = "unknown"

    behavior = field.get("behavior")
    if isinstance(behavior, dict):  # non-mapping behavior raised above
        _apply_latching(behavior.get("latching"), "behavior.latching")
    if template_name is not None and template_name in templates:
        tmpl_behavior = (templates[template_name] or {}).get("behavior") or {}
        if isinstance(tmpl_behavior, dict):
            _apply_latching(tmpl_behavior.get("latching"), "template.behavior.latching")

    if isinstance(access, str) and access in RETIRED_ACCESS_SPELLINGS:
        retired.append(f"access={access}")
        if access == "cor":
            access = "ro_cor"
        elif access == "rw_with_state_guard":
            access = "rw"

    return access, is_dynamic, marking_source, template_name, retired


def _field_extent(
    raw: dict[str, Any], off: int, bit_lo_v: Any
) -> tuple[int, int | None, int | None]:
    """(byte_hi, bit_lo, bit_hi) of one field occurrence.

    Raises InputError instead of defaulting: a bit field without bit_width,
    a byte field without a positive size_bytes, or a bit span leaving the
    byte would otherwise be silently narrowed or widened.
    """
    if bit_lo_v is not None:
        if raw.get("bit_width") is None:
            raise InputError("bit field carries no bit_width; its bit span would default to one bit")
        bit_lo = int(bit_lo_v)
        bit_hi = bit_lo + int(raw["bit_width"]) - 1
        if not (0 <= bit_lo <= bit_hi <= 7):
            raise InputError(f"bit span {bit_lo}-{bit_hi} does not fit inside one byte")
        return off, bit_lo, bit_hi
    size = raw.get("size_bytes")
    if size is None or int(size) < 1:
        raise InputError(
            f"byte field carries no positive size_bytes ({size!r}); its extent would default to one byte"
        )
    return off + int(size) - 1, None, None


def load_family_fields(
    family: str, schemas_dir: Path
) -> tuple[list[SchemaField], list[dict[str, Any]]]:
    """Load every page field of a family with resolved volatility markings.

    Raises InputError on unresolved inputs; marking and location problems are
    returned as per-field error dicts so the receipt records them.
    """
    templates = load_family_templates(family, schemas_dir)
    family_dir = schemas_dir / family
    index_data = _load_yaml(family_dir / "_index.yaml")

    fields: list[SchemaField] = []
    errors: list[dict[str, Any]] = []

    includes = list((index_data or {}).get("includes") or [])
    for inc in includes:
        inc_path = (family_dir / str(inc)).resolve()
        if not inc_path.is_file():
            raise InputError(f"Included page file does not exist: {inc_path}")
        inc_decl = _decl_path(inc_path, schemas_dir)
        page_data = _load_yaml(inc_path)
        if not isinstance(page_data, dict) or page_data.get("kind") != "page":
            # A non-page include contributes no memory-map fields when it
            # carries none (e.g. kind: enum registries, which the closure
            # still hashes); an include that DOES carry fields (or
            # field_includes, or is itself a list of field mappings) would
            # silently drop them: an error.
            if isinstance(page_data, list) or (isinstance(page_data, dict) and (
                page_data.get("fields") or page_data.get("field_includes")
            )):
                errors.append({
                    "kind": KIND_SKIPPED_FIELD,
                    "include": str(inc),
                    "decl_file": inc_decl,
                    "page": None,
                    "message": (
                        f"Included file {inc!r} carries fields but is not kind: page "
                        f"(kind: {page_data.get('kind') if isinstance(page_data, dict) else 'list'!r}); "
                        "its fields would be silently dropped"
                    ),
                })
            continue  # no fields declared: account_completeness checks this against the closure
        page_id = page_data.get("id") or inc_path.stem
        if not page_data.get("page_address"):
            # A page without page_address would silently default to lower
            # memory and be compared against the wrong page: an error.
            errors.append({
                "kind": KIND_UNRESOLVED_INPUT,
                "include": str(inc),
                "decl_file": inc_decl,
                "page": page_id,
                "message": f"Page file {inc!r} declares no page_address; its fields cannot be placed",
            })
            continue
        page_address = str(page_data["page_address"])
        page_select = page_data.get("page_select")
        page_select = str(page_select).lower() if page_select is not None else None
        if not isinstance(page_data.get("fields") or [], list):
            errors.append({
                "kind": KIND_SKIPPED_FIELD,
                "include": str(inc),
                "decl_file": inc_decl,
                "page": page_id,
                "message": f"Page file {inc!r} fields is not a list; its fields would be silently dropped",
            })
            continue
        # (raw entry, declaring file, index within that file's fields list)
        page_fields: list[tuple[Any, str, int]] = [
            (raw, inc_decl, i) for i, raw in enumerate(page_data.get("fields") or [])
        ]

        for fi in page_data.get("field_includes") or []:
            fi_path = (inc_path.parent / str(fi)).resolve()
            if not fi_path.is_file():
                raise InputError(f"field_includes file does not exist: {fi_path}")
            fi_decl = _decl_path(fi_path, schemas_dir)
            fi_data = _load_yaml(fi_path)
            if isinstance(fi_data, dict) and isinstance(fi_data.get("fields"), list):
                page_fields.extend((raw, fi_decl, i) for i, raw in enumerate(fi_data["fields"]))
            elif isinstance(fi_data, list):
                page_fields.extend((raw, fi_decl, i) for i, raw in enumerate(fi_data))
            else:
                # A field_includes payload that is neither a mapping with a
                # fields list nor a list contributes nothing and hides its
                # shape: an error.
                errors.append({
                    "kind": KIND_SKIPPED_FIELD,
                    "include": str(fi),
                    "decl_file": fi_decl,
                    "page": page_id,
                    "message": (
                        f"field_includes file {fi!r} is neither a mapping with a fields "
                        "list nor a list; its fields would be silently dropped"
                    ),
                })

        for raw, decl_file, decl_idx in page_fields:
            decl = f"{decl_file}#{decl_idx}"
            # One fragment may be included by several pages: the occurrence
            # id names the page too.
            occ = decl if decl_file == inc_decl else f"{inc_decl}>{decl}"
            if not isinstance(raw, dict):
                # A non-mapping entry in a fields list is dropped silently
                # otherwise: an error, never a skip.
                errors.append({
                    "kind": KIND_SKIPPED_FIELD,
                    "field": "<non-mapping entry>",
                    "decl": decl,
                    "page": page_id,
                    "message": (
                        f"fields list in {decl_file!r} contains a non-mapping entry "
                        f"({raw!r}); it would be silently dropped"
                    ),
                })
                continue
            name = raw.get("name") or "<unnamed>"
            origin = inc_path.name
            try:
                access, is_dynamic, marking_source, template_name, retired = (
                    _resolve_field_marking(raw, templates, origin)
                )
            except InputError as e:
                errors.append({
                    "kind": KIND_UNRESOLVED_INPUT,
                    "field": name, "decl": decl,
                    "page": page_id,
                    "message": str(e),
                })
                continue

            base = {
                "name": name,
                "page_id": page_id,
                "page_address": page_address,
                "page_select": page_select,
                "origin": origin,
                "access": access,
                "is_dynamic": is_dynamic,
                "marking_source": marking_source,
                "template": template_name,
                "retired": retired,
                "since": raw.get("since"),
                "until": raw.get("until"),
                "present_when": raw.get("present_when"),
                "decl": decl,
                # Per-revision marking overrides (codegen resolve_field_for_rev
                # applies changed_in access/flags/behavior): not resolved here,
                # so account_completeness fails them explicitly.
                "overrides": [k for k in ("changed_in",) if raw.get(k)],
            }

            offset_expr = raw.get("offset_expr")
            bit_expr = raw.get("bit_offset_expr")
            instances = raw.get("instances")
            if instances is not None and offset_expr is None and bit_expr is None:
                # instances expand offset expressions; with a literal offset
                # the instance range would silently collapse to one
                # occurrence: an error, never a silent narrowing.
                errors.append({
                    "kind": KIND_SKIPPED_FIELD,
                    "field": name, "decl": decl,
                    "page": page_id,
                    "message": (
                        f"Field {name!r} carries instances without offset_expr/"
                        "bit_offset_expr; the instance range would collapse to a "
                        "single occurrence"
                    ),
                })
                continue
            if offset_expr is not None or bit_expr is not None:
                if not isinstance(instances, dict) or "index" not in instances:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": (
                            f"Field {name!r} uses offset expressions without instances; "
                            "cannot be located in the memory map"
                        ),
                    })
                    continue
                idx_name = str(instances["index"])
                # start and the range end (end, or count as end = start +
                # count - 1) must be explicit: a missing key or an empty range
                # silently yields zero or one occurrence.
                if "start" not in instances:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": (
                            f"Field {name!r} instances carry no explicit start; "
                            "the first occurrence would silently default"
                        ),
                    })
                    continue
                if "end" in instances and "count" in instances \
                        and int(instances["end"]) != int(instances["start"]) + int(instances["count"]) - 1:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": (
                            f"Field {name!r} instances carry disagreeing end "
                            f"({instances['end']!r}) and count ({instances['count']!r}); "
                            "the instance range is ambiguous"
                        ),
                    })
                    continue
                if "end" in instances:
                    start = int(instances["start"])
                    end = int(instances["end"])
                elif "count" in instances:
                    start = int(instances["start"])
                    end = start + int(instances["count"]) - 1
                else:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": (
                            f"Field {name!r} instances carry neither end nor count; "
                            "the range would silently default to one occurrence"
                        ),
                    })
                    continue
                if end < start:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": (
                            f"Field {name!r} instances range end {end} < start {start}; "
                            "the field would silently expand to zero occurrences"
                        ),
                    })
                    continue
                for i in range(start, end + 1):
                    env = {idx_name: i}
                    try:
                        if offset_expr is not None:
                            off = _eval_int_expr(str(offset_expr), env)
                        else:
                            off = int(raw["offset"])
                        bit_lo_v = (
                            _eval_int_expr(str(bit_expr), env) if bit_expr is not None
                            else raw.get("bit_offset")
                        )
                        byte_hi, bit_lo, bit_hi = _field_extent(raw, off, bit_lo_v)
                    except (InputError, KeyError, ValueError, TypeError) as e:
                        errors.append({
                            "kind": KIND_SKIPPED_FIELD,
                            "field": name, "decl": decl,
                            "page": page_id,
                            "message": f"Field {name!r} instance {i} could not be located: {e}",
                        })
                        continue
                    fields.append(SchemaField(
                        byte_lo=off, byte_hi=byte_hi, bit_lo=bit_lo, bit_hi=bit_hi,
                        uid=f"{occ}@{i}", **base
                    ))
            else:
                off = raw.get("offset")
                if off is None:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": f"Field {name!r} has no offset; cannot be located",
                    })
                    continue
                try:
                    off = int(off)
                    byte_hi, bit_lo, bit_hi = _field_extent(raw, off, raw.get("bit_offset"))
                except (InputError, ValueError, TypeError) as e:
                    errors.append({
                        "kind": KIND_SKIPPED_FIELD,
                        "field": name, "decl": decl,
                        "page": page_id,
                        "message": f"Field {name!r} could not be located: {e}",
                    })
                    continue
                fields.append(SchemaField(
                    byte_lo=off, byte_hi=byte_hi, bit_lo=bit_lo, bit_hi=bit_hi,
                    uid=occ, **base
                ))

    return fields, errors


# ---------------------------------------------------------------------------
# Authority reports (research reuse, fail closed)
# ---------------------------------------------------------------------------


def load_authority_report(authority_dir: Path | None, family: str) -> dict[str, Any]:
    """Load the completed authority report for a family.

    Returns {"path", "sha256", "passages"} where passages are every
    {document, clause_or_table, exact_passage} row found anywhere in the
    report.  Raises InputError when the required report is missing or
    unparseable: the tool never invents the research it reuses.
    """
    filename = AUTHORITY_REPORT_FILES[family]
    if authority_dir is None:
        raise InputError(f"Authority report directory not given; {filename} is required")
    report_path = authority_dir / filename
    if not report_path.is_file():
        raise InputError(f"Required authority report not found: {report_path}")

    try:
        with open(report_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception as e:  # noqa: BLE001
        raise InputError(f"Authority report {report_path} is not parseable JSON: {e}") from e

    passages: list[dict[str, str]] = []

    def walk(node: Any, doc: str | None = None) -> None:
        if isinstance(node, dict):
            doc = node.get("document") or doc
            if isinstance(node.get("exact_passage"), str):
                passages.append({
                    "document": str(doc or node.get("document") or ""),
                    "clause_or_table": str(node.get("clause_or_table") or ""),
                    "exact_passage": str(node["exact_passage"]),
                })
            for v in node.values():
                walk(v, doc)
        elif isinstance(node, list):
            for v in node:
                walk(v, doc)

    result = data.get("result")
    if isinstance(result, (dict, list)):
        walk(result)
    elif isinstance(result, str):
        # The completed reports embed their evidence as JSON inside the
        # result string, sometimes fenced as ```json blocks, sometimes raw.
        walked = False
        for m in re.finditer(r"```json\s*(\{.*?\})\s*```", result, re.DOTALL):
            try:
                block = json.loads(m.group(1))
            except Exception as e:  # noqa: BLE001 - a lost research block is an error
                raise InputError(
                    f"Authority report {report_path} carries an unparseable JSON evidence "
                    f"block; its research would be silently dropped: {e}"
                ) from e
            walk(block)
            walked = True
        if not walked:
            for candidate in (result.strip(), result[result.find("{"):result.rfind("}") + 1]):
                try:
                    walk(json.loads(candidate))
                    break
                except Exception:  # noqa: BLE001
                    continue  # next candidate; none parsing leaves passages empty -> InputError below
    else:
        walk(data)
    if not passages:
        # A required research input that contributed nothing is an error, not
        # a silent pass: the citations would all fall back while the receipt
        # still names the report.
        raise InputError(
            f"Authority report {report_path} yielded no reusable passages; the "
            "completed research could not be extracted"
        )
    return {
        "path": report_path.name,
        "sha256": sha256_file(report_path),
        "passages": passages,
    }


def find_authority_passage(
    report: dict[str, Any] | None, fragment: str
) -> dict[str, str] | None:
    """Find the report row whose exact_passage contains the fragment."""
    if not report:
        return None  # no report loaded: recorded as missing_authority_report by build_receipt
    frag = passage_norm(fragment)
    for row in report["passages"]:
        if frag and frag in passage_norm(row["exact_passage"]):
            return row
    return None  # caller cites the parsed text and records authority_report_note


# ---------------------------------------------------------------------------
# Parsed standard handling
# ---------------------------------------------------------------------------


class StdRow:
    __slots__ = ("page_ctx", "page_range", "byte_lo", "byte_hi", "bit_lo", "bit_hi",
                 "name", "type_text", "desc", "table_title", "line")

    def __init__(self, **kw: Any) -> None:
        for slot in self.__slots__:
            setattr(self, slot, kw.get(slot))

    @property
    def bit_mask(self) -> int:
        if self.bit_lo is None:
            return 0xFF
        mask = 0
        for b in range(self.bit_lo, (self.bit_hi if self.bit_hi is not None else self.bit_lo) + 1):
            mask |= 1 << b
        return mask

    def is_reserved_name(self) -> bool:
        n = norm_text(self.name).lower()
        d = norm_text(self.desc).lower()
        if n in ("reserved", "-"):
            return True
        if d.startswith("reserved") or "reserved[" in d:
            return True
        # An unnamed row is only reserved when it describes nothing; overview
        # memory-map rows carry their name in the description column.
        if not n and not d:
            return True
        return False

    def is_custom_name(self) -> bool:
        n = norm_text(self.name).lower()
        d = norm_text(self.desc).lower()
        return n == "custom" or d.startswith("custom") or "custom[" in d


class ParsedStandard:
    def __init__(self, filename: str, sha: str, text: str) -> None:
        self.filename = filename
        self.sha256 = sha
        self.text = text
        self.norm_lines = [passage_norm(l) for l in text.splitlines()]
        self.rows: list[StdRow] = []
        # (page_ctx, byte) -> "rw" | "wo"  (SFF-8636 writable memory blocks)
        self.writable_bytes: dict[tuple[str, int], str] = {}
        # (page_ctx, byte) -> line of the writable-block row that placed it.
        self.writable_lines: dict[tuple[str, int], int] = {}
        # Bytes two writable-block rows place with different operations: no
        # authority (never first- or last-row-wins); recorded as an error.
        self.writable_conflicts: set[tuple[str, int]] = set()
        # Rows that could not be ingested (unparseable span, unknown operation
        # spelling, no page context): recorded and surfaced as mismatches, so
        # standard content is never silently dropped.
        self.ingest_errors: list[dict[str, Any]] = []

    def find_passage_line(self, fragment: str) -> int | None:
        frag = passage_norm(fragment)
        if not frag:
            return None  # empty fragment: no passage to locate; caller records not-found
        for idx, line in enumerate(self.norm_lines):
            if frag in line:
                return idx + 1
        return None  # _convention_citation records authority_passage_not_found_in_standard


_TABLE_TITLE_RE = re.compile(r"^Table\s+\S+\s+\S")
_LOWER_CTX_RE = re.compile(r"\bLower\s+(?:Page|Memory)\b", re.I)
_UPPER_PAGE_RE = re.compile(r"Upper Page\s+(?P<page>[0-9A-Fa-f]{2})h", re.I)
_PAGE_PAREN_RE = re.compile(r"\(Page\s+(?P<page>[0-9A-Fa-f]{2})h", re.I)
_PAGE_RANGE_RE = re.compile(
    r"\bPages?\s+(?P<lo>[0-9A-Fa-f]{2})h\s*[-\u2013]\s*(?P<hi>[0-9A-Fa-f]{2})h\b", re.I
)
_PAGE_BARE_RE = re.compile(r"\bPages?\s+(?P<page>[0-9A-Fa-f]{2})h\b", re.I)
_ADDRESS_A_RE = re.compile(r"\[?Address\s+A(?P<page>0|2)h\b", re.I)
# CMIS memory-map field tables live under section 8 (5.x) or 1.7 (3.0/4.0);
# tables elsewhere (state-machine and conformance-rule tables) are not
# field-table Type entries.
_CMIS_MAP_SECTION_RE = re.compile(r"^(8\.|1\.7\b|1\.7\.)")


def _page_context(heading: str) -> str | None:
    h = heading or ""
    if _LOWER_CTX_RE.search(h):
        return "lower"
    m = _UPPER_PAGE_RE.search(h)
    if m:
        return m.group("page").lower() + "h"
    # A page range is checked before a single parenthesized page: a title like
    # "VDM Configuration (Page 20h-23h)" describes the whole range, and the
    # paren pattern would otherwise narrow it to its first page.
    m = _PAGE_RANGE_RE.search(h)
    if m:
        return f"{m.group('lo').lower()}h-{m.group('hi').lower()}h"
    m = _PAGE_PAREN_RE.search(h)
    if m:
        return m.group("page").lower() + "h"
    m = _ADDRESS_A_RE.search(h)
    if m:
        return f"a{m.group('page').lower()}h"
    m = _PAGE_BARE_RE.search(h)
    if m:
        return m.group("page").lower() + "h"
    return None


def _page_value(ctx: str) -> int | None:
    """Numeric page value of a context like 'a1h' or '10h' (None otherwise)."""
    m = re.fullmatch(r"([0-9a-f]{2})h", ctx or "")
    return int(m.group(1), 16) if m else None


def _parse_span(text: str) -> tuple[int, int] | None:
    """Parse '3', '7-4', '129-144' into a sorted (lo, hi); None when unparseable."""
    t = norm_text(text)
    if not t:
        return None
    m = re.fullmatch(r"(\d+)\s*[-\u2013]\s*(\d+)", t)
    if m:
        lo, hi = int(m.group(1)), int(m.group(2))
        return (lo, hi) if lo <= hi else (hi, lo)
    m = re.fullmatch(r"(\d+)", t)
    if m:
        return int(m.group(1)), int(m.group(1))
    return None


def _bits_cell_is_whole_byte(text: str) -> bool:
    t = norm_text(text).lower()
    return not t or t in ("all", "7-0", "0-7")


def _bits_cell_spans_all(text: str, nbytes: int) -> bool:
    """True when a Bits cell covers every bit of an n-byte field: a bit
    count of 8n, a single span of 8n bits ("31-0") or one "7-0" per byte
    ("7-0 7-0")."""
    t = norm_text(text).lower()
    if re.fullmatch(r"\d+", t) and int(t) == 8 * nbytes:
        # A bit COUNT (SFF-8472 "Bits #" = 8/16/32) equal to the field
        # width; never a bit index, since bit indices stop at 7.
        return True
    whole = _parse_span(t)
    if whole is not None and whole == (0, 8 * nbytes - 1):
        return True
    parts = t.split()
    return len(parts) == nbytes and all(_parse_span(x) == (0, 7) for x in parts)


def _parse_bits_cell(text: str) -> tuple[int, int] | None:
    """Parse a Bits cell; None means the whole byte or an unparseable cell
    (callers distinguish them with _bits_cell_is_whole_byte)."""
    t = norm_text(text).lower()
    if not t or t == "all":
        return None  # whole byte
    span = _parse_span(t)
    if span is not None and not (0 <= span[0] <= span[1] <= 7):
        return None  # not a bit span inside one byte: unparseable
    return span


_PAGE_LABEL_RE = re.compile(
    r"^(?:(?:Lower|Upper)\s+)?Pages?\s+(?P<lo>[0-9A-Fa-f]{2})h?"
    r"(?:\s*[-\u2013]\s*(?P<hi>[0-9A-Fa-f]{2})h)?\b",
    re.I,
)


def _page_label_ctx(cell: str) -> str | None:
    """Page context named by a label cell such as 'Page 02h' or 'Page 00-01h'."""
    t = norm_text(cell)
    if re.match(r"^Lower\s+(?:Page|Memory)\b", t, re.I):
        return "lower"
    m = _PAGE_LABEL_RE.match(t)
    if not m:
        return None
    lo = m.group("lo").lower()
    hi = m.group("hi")
    if hi:
        return f"{lo}h-{hi.lower()}h"
    return lo + "h"


_PAGE_FILLER_RE = re.compile(r"^(\d{1,4}|<!--\s*image\s*-->)$")


def _header_names_location(norm_header: list[str]) -> bool:
    """True when a table header carries a column the ingester places rows by."""
    return (
        any(h in ("byte", "byte number", "a2h", "a0h", "address") for h in norm_header)
        or any(h.startswith("byte") for h in norm_header)
        or ("from" in norm_header and "to" in norm_header)
    )


def _header_is_data_row(norm_header: list[str]) -> bool:
    """True when a 'header' line is shaped like a field-table data row: one
    of its first two cells is a byte or bit number (the header was lost at a
    page break)."""
    return any(
        re.fullmatch(r"\d{1,3}(\s*[-\u2013]\s*\d{1,3})?", h or "")
        for h in norm_header[:2]
    )


def _is_page_break_filler(line: str) -> bool:
    """A line a PDF page break leaves between two segments of one table:
    blank, a bare page/line number, an image marker, or a repeated
    un-numbered running heading (e.g. "## OIF-CMIS-05.3")."""
    t = line.strip()
    if not t or _PAGE_FILLER_RE.match(t):
        return True
    if t.startswith("#"):
        heading = t.lstrip("#").strip()
        return not heading[:1].isdigit() and not _TABLE_TITLE_RE.match(heading)
    return False


def parse_standard_file(path: Path, family: str) -> ParsedStandard:
    """Parse a parsed-standard markdown file into rows and byte access maps."""
    raw = path.read_text(encoding="utf-8", errors="replace")
    std = ParsedStandard(path.name, sha256_file(path), raw)
    lines = raw.splitlines()

    page_ctx: str | None = None
    table_title = ""
    in_map_section = family != "cmis"
    # A table split by a PDF page break continues as a new markdown table
    # with the same header, separated only by blank lines; its first rows
    # carry the previous segment's byte (e.g. SFF-8472 Table 9-17 byte 116
    # bits 5-0).  The carry passes only across such a continuation.
    prev_table: dict[str, Any] | None = None
    idx = 0
    while idx < len(lines):
        line = lines[idx]
        stripped = line.strip()
        if line.startswith("#"):
            heading = line.lstrip("#").strip()
            ctx = _page_context(heading)
            if ctx:
                page_ctx = ctx
            if family == "cmis" and heading[:1].isdigit():
                # Only numbered headings open or close the memory-map
                # chapters; mid-document junk headings (e.g. a repeated
                # document title) leave the section state alone.
                in_map_section = bool(_CMIS_MAP_SECTION_RE.match(heading))
            if not _TABLE_TITLE_RE.match(heading):
                table_title = ""
            idx += 1
            continue
        if _TABLE_TITLE_RE.match(stripped):
            # Only a caption followed by its table is a title: a prose
            # sentence that merely starts with "Table N" (e.g. "Table 12-3
            # defines ... address A0h, byte 65") would otherwise refile every
            # later row under the page the sentence mentions.
            nxt = next((l for l in lines[idx + 1:idx + 6] if l.strip()), "")
            if nxt.startswith("|"):
                table_title = stripped
                ctx = _page_context(table_title)
                if ctx:
                    page_ctx = ctx
            idx += 1
            continue
        if line.startswith("|") and idx + 1 < len(lines) and re.match(r"^\|[\s:|-]+\|?\s*$", lines[idx + 1]):
            if not in_map_section:
                # CMIS tables outside the memory-map chapters (8.x / 1.7) are
                # state-machine, timing and conformance tables; measured on
                # the six parsed revisions, none with a Byte/Address column
                # carries an access Type, so no field-table content is lost.
                idx += 1
                # Advance past the table body.
                j = idx + 1
                while j < len(lines) and lines[j].startswith("|"):
                    j += 1
                idx = j
                continue
            header = [c.strip() for c in line.strip().strip("|").split("|")]
            rows: list[tuple[int, list[str]]] = []
            j = idx + 2
            while j < len(lines) and lines[j].startswith("|"):
                rows.append((j + 1, [c.strip() for c in lines[j].strip().strip("|").split("|")]))
                j += 1
            norm_hdr = [norm_text(h).lower() for h in header]
            carry_in: int | None = None
            contiguous = prev_table is not None and all(
                _is_page_break_filler(l) for l in lines[prev_table["end"]:idx]
            )
            if contiguous and prev_table["header"] == norm_hdr:
                carry_in = prev_table["carry_byte"]
                table_title = prev_table["title"]
            elif (
                contiguous
                and prev_table["located"]
                and not _header_names_location(norm_hdr)
                and _header_is_data_row(norm_hdr)
                and len(header) == len(prev_table["raw_header"])
            ):
                # A page break that lost the header: the "header" line is the
                # first data row of the previous table, continued.
                carry_in = prev_table["carry_byte"]
                table_title = prev_table["title"]
                rows = [(idx + 1, header)] + rows
                header = prev_table["raw_header"]
                norm_hdr = prev_table["header"]
            elif in_map_section and not _header_names_location(norm_hdr) \
                    and _header_is_data_row(norm_hdr):
                # A data-shaped header that cannot be joined to a located
                # table: its rows name locations that cannot be placed.
                for ln, cells in [(idx + 1, header)] + rows:
                    std.ingest_errors.append({
                        "kind": KIND_UNPARSEABLE_ROW,
                        "file": std.filename,
                        "table": table_title,
                        "line": ln,
                        "row": " | ".join(norm_text(c) for c in cells),
                        "message": (
                            "table header is a data row (header lost at a page break) "
                            "and no preceding located table continues; its rows cannot "
                            "be placed"
                        ),
                    })
                prev_table = None
                idx = j
                continue
            carry_out = _ingest_table(
                std, family, header, rows, page_ctx, table_title, carry_in
            )
            prev_table = {
                "header": norm_hdr, "raw_header": header, "end": j,
                "carry_byte": carry_out, "title": table_title,
                "located": _header_names_location(norm_hdr),
            }
            idx = j
            continue
        idx += 1
    return std


def _ingest_table(
    std: ParsedStandard,
    family: str,
    header: list[str],
    rows: list[tuple[int, list[str]]],
    page_ctx: str | None,
    table_title: str,
    carry_in: int | None = None,
) -> int | None:
    """Ingest one markdown table; returns the last carried byte (for a
    page-break continuation of the same table)."""
    norm_header = [norm_text(h).lower() for h in header]
    ncols = len(header)

    def col_exact(candidates: tuple[str, ...]) -> int | None:
        for cand in candidates:
            if cand in norm_header:
                return norm_header.index(cand)
        return None

    def col_contains(token: str) -> int | None:
        for k, h in enumerate(norm_header):
            if token in h:
                return k
        return None

    # SFF-8636 writable memory blocks: | Byte | # Bytes | Operation | Description |
    if family == "sff-8636" and "operation" in norm_header and col_exact(("byte",)) is not None:
        op_col = norm_header.index("operation")
        byte_col = norm_header.index("byte")
        count_col = col_exact(("# bytes", "no. of bytes", "number of bytes", "count"))
        page_label: str | None = None

        def _unplaceable(line_no: int, cells: list[str], reason: str) -> None:
            # A row that cannot be placed is recorded, never dropped: the
            # writable-block map is the authority for every writable byte.
            std.ingest_errors.append({
                "kind": KIND_UNPARSEABLE_ROW,
                "file": std.filename,
                "table": table_title,
                "line": line_no,
                "row": " | ".join(norm_text(c) for c in cells),
                "message": reason,
            })

        for line_no, cells in rows:
            if len(cells) <= max(byte_col, op_col):
                _unplaceable(line_no, cells, "writable-block row has no Byte or Operation cell")
                continue
            first = norm_text(cells[byte_col])
            m = re.match(r"Page\s+([0-9A-Fa-f]{2})h", first, re.I)
            if m:
                # "Page 00h" rows in the writable-blocks table are the lower page.
                page_val = m.group(1).lower()
                page_label = "lower" if page_val == "00" else page_val + "h"
                continue  # a page label row: it places later rows, names no block
            op = norm_text(cells[op_col]).lower()
            if "write-only" in op:
                access = "wo"
            elif "read/write" in op or op == "rw":
                access = "rw"
            else:
                _unplaceable(line_no, cells, f"unknown Operation spelling {op!r} in the writable-blocks table")
                continue
            span = _parse_span(first)
            if span is None:
                _unplaceable(line_no, cells, f"unparseable Byte cell {first!r} in the writable-blocks table")
                continue
            # The "# Bytes" column extends a single-byte cell to its block;
            # without it a "start byte + count" row collapses to one byte and
            # the rest of the block silently defaults to read-only.
            count: int | None = None
            if count_col is not None and count_col < len(cells):
                count_text = norm_text(cells[count_col])
                cm = re.fullmatch(r"\d+", count_text)
                count = int(cm.group(0)) if cm else None
                if count is None and count_text and span[0] == span[1]:
                    _unplaceable(line_no, cells, f"unparseable # Bytes cell {count_text!r}: the block span is unknown")
                    continue
            if count is not None:
                if count < 1:
                    _unplaceable(line_no, cells, f"# Bytes count {count} is not a positive span")
                    continue
                count_span = (span[0], span[0] + count - 1)
                if span[1] != span[0] and span != count_span:
                    _unplaceable(
                        line_no, cells,
                        f"Byte cell {first!r} disagrees with # Bytes {count}: "
                        "the block span is ambiguous",
                    )
                    continue
                span = count_span
            if page_label is None:
                _unplaceable(line_no, cells, "writable-block row appears before any Page label; its page is unknown")
                continue
            for b in range(span[0], span[1] + 1):
                key = (page_label, b)
                prior = std.writable_bytes.get(key)
                if prior is not None and prior != access:
                    # Two rows give one byte different operations: the
                    # conflict is an error and the byte keeps no authority.
                    std.writable_conflicts.add(key)
                    std.ingest_errors.append({
                        "kind": KIND_CONFLICTING_ROWS,
                        "file": std.filename,
                        "table": table_title,
                        "page": page_label,
                        "offset": b,
                        "lines": [std.writable_lines.get(key), line_no],
                        "operations": [prior, access],
                        "message": (
                            f"Writable-block rows at lines {std.writable_lines.get(key)} and "
                            f"{line_no} give {page_label} byte {b} different operations "
                            f"({prior} vs {access}); the conflict is never resolved by row order"
                        ),
                    })
                    continue
                std.writable_bytes.setdefault(key, access)
                std.writable_lines.setdefault(key, line_no)
        return None

    # "Address" is the byte column of the memory-map overview tables (CMIS
    # Lower Memory / Page 00h Overview, SFF-8472 Table 12-3): reading it
    # keeps their rows instead of dropping the whole table.
    byte_col = col_exact(("byte", "byte number", "a2h", "a0h", "address"))
    if byte_col is None:
        byte_col = next(
            (k for k, h in enumerate(norm_header) if h.startswith("byte")), None
        )
    # Memory-map overviews with From/To columns (SFF-8636 Table 6-1) give the
    # span as two cells.
    to_col = col_exact(("to",)) if byte_col is None else None
    if byte_col is None and to_col is not None:
        byte_col = col_exact(("from",))
        if byte_col is None:
            to_col = None
    if byte_col is None:
        # A table with no Byte/Address column names no memory-map location
        # (enumeration, timing and compliance-code tables): nothing in it can
        # be placed, so nothing comparison-relevant is lost here.
        return None
    bit_col = col_exact(("bit", "bits", "bit #", "bits #"))
    name_col = (
        col_exact(("name", "field name", "register name", "subject area", "subject"))
        or col_contains("name")
    )
    type_col = col_exact(("type",))
    desc_col = col_contains("description")
    # Some CMIS/SFF-8472 tables carry an explicit "# Bytes" column (e.g.
    # SFF-8472 Tables 4-2/4-3/9-5/9-6); it extends a single-byte cell to its
    # span, so a "start byte + count" row does not collapse to its first byte.
    gen_count_col = col_exact(
        ("# bytes", "no. of bytes", "number of bytes", "size", "size (bytes)")
    )
    # Some CMIS VDM tables carry an explicit Page column (e.g. Table 8-168
    # "VDM Configuration (Page 20h-23h)"); it overrides the title context.
    page_col = col_exact(("page",))

    def _unplaceable_row(line_no: int, cells: list[str], reason: str) -> None:
        # A row that names a location but cannot be placed is recorded,
        # never dropped: it may carry the only authority for that location.
        std.ingest_errors.append({
            "kind": KIND_UNPARSEABLE_ROW,
            "file": std.filename,
            "table": table_title,
            "line": line_no,
            "row": " | ".join(norm_text(c) for c in cells),
            "message": reason,
        })

    carry_byte: int | None = carry_in
    carry_type = ""  # last non-empty Type cell in this table
    # A "Page XXh" label row inside a table (e.g. SFF-8472 Table 4-4) files
    # the rows after it under that page, for the rest of the table.
    label_ctx: str | None = None
    for line_no, cells in rows:
        if len(cells) < 2:
            continue  # a one-cell line carries no Byte and description pair: not a row
        if len(set(cells)) == 1:
            # Repeated divider/label row (the PDF repeats one label across
            # every column); a page label sets the context of later rows.
            lc = _page_context(norm_text(cells[0])) or _page_label_ctx(norm_text(cells[0]))
            if lc:
                label_ctx = lc
                carry_byte = None
            continue
        while len(cells) < ncols:
            cells.append("")
        byte_cell = norm_text(cells[byte_col])
        if to_col is not None and byte_cell:
            to_cell = norm_text(cells[to_col])
            if re.fullmatch(r"\d+", byte_cell) and re.fullmatch(r"\d+", to_cell):
                byte_cell = f"{byte_cell}-{to_cell}"
        span = _parse_span(byte_cell) if byte_cell else None
        if span is None and byte_cell:
            lc = _page_label_ctx(byte_cell)
            if lc:
                label_ctx = lc
                carry_byte = None
                continue
            if re.search(r"\d", byte_cell):
                _unplaceable_row(
                    line_no, cells,
                    f"unparseable Byte cell {byte_cell!r}: the row names a location "
                    "that cannot be placed",
                )
                continue
            continue  # digit-free section label (e.g. "GENERAL USE FIELDS"): names no location
        if span is None:
            span = (carry_byte, carry_byte) if carry_byte is not None else None
        if span is None:
            # A continuation row before any byte: it cannot be placed.
            if any(norm_text(c) for k, c in enumerate(cells) if k != byte_col):
                _unplaceable_row(
                    line_no, cells,
                    "row has no Byte cell and no earlier row to carry one from",
                )
            continue
        # An explicit count column extends a single-byte cell to its span;
        # a range that disagrees with the count is ambiguous and recorded.
        if gen_count_col is not None and gen_count_col < len(cells):
            count_text = norm_text(cells[gen_count_col])
            cm = re.fullmatch(r"\d+", count_text)
            if not cm and count_text and byte_cell and span[1] == span[0]:
                _unplaceable_row(
                    line_no, cells,
                    f"unparseable count cell {count_text!r}: the row's byte span is unknown",
                )
                continue
            if cm:
                gcount = int(cm.group(0))
                gspan = (span[0], span[0] + gcount - 1)
                if span[1] != span[0] and span != gspan:
                    std.ingest_errors.append({
                        "kind": KIND_UNPARSEABLE_ROW,
                        "file": std.filename,
                        "table": table_title,
                        "line": line_no,
                        "row": " | ".join(norm_text(c) for c in cells),
                        "message": (
                            f"Byte cell {byte_cell!r} disagrees with the count column "
                            f"({gcount}): the row span is ambiguous"
                        ),
                    })
                    continue
                if gcount < 1:
                    _unplaceable_row(
                        line_no, cells, f"count column {gcount} is not a positive span"
                    )
                    continue
                span = gspan
        carry_byte = span[1]
        bits = _parse_bits_cell(cells[bit_col]) if bit_col is not None else None
        if bits is None and bit_col is not None and _bits_cell_spans_all(
            cells[bit_col], span[1] - span[0] + 1
        ):
            pass  # "31-0" over a 4-byte field, "7-0 7-0" over two bytes: whole bytes
        elif bits is None and bit_col is not None and not _bits_cell_is_whole_byte(cells[bit_col]):
            # An unparseable Bits cell (e.g. a PDF-merged "7-4 3-0") must
            # not silently widen the row to the whole byte: recorded.
            _unplaceable_row(
                line_no, cells,
                f"unparseable Bits cell {norm_text(cells[bit_col])!r}: the row's bit "
                "span cannot be placed",
            )
            continue
        name = norm_text(cells[name_col]) if name_col is not None and name_col < len(cells) else ""
        type_text = norm_text(cells[type_col]) if type_col is not None and type_col < len(cells) else ""
        desc = norm_text(cells[desc_col]) if desc_col is not None and desc_col < len(cells) else ""
        bits_text = norm_text(cells[bit_col]) if bit_col is not None else ""
        if not byte_cell and not name and not type_text and not bits_text:
            continue  # text-wrap continuation (no byte, bits, name or type): names no location

        # PDF extraction frequently drops the Type cell on rows after the
        # first of a group (e.g. DPDeinitLane7..1 keep only the first "RW").
        # Carry the last non-empty Type within the same table forward for
        # named (non-reserved) rows; reserved rows never inherit, because an
        # empty Type cell is their meaning.
        # Custom rows likewise carry no Type of their own (their access is
        # vendor-defined): inheriting the previous row's Type would invent
        # authority for them.
        reserved_named = (
            name.lower() in ("reserved", "-", "", "custom")
            or desc.lower().startswith(("reserved", "custom"))
            or "reserved[" in desc.lower()
            or "custom[" in desc.lower()
        )
        if type_text:
            carry_type = type_text
        elif not reserved_named and carry_type and type_col is not None:
            type_text = carry_type

        # "(Page 00h ...)" table titles in the SFF-8636 lower-page sections and
        # the CMIS/SFF upper pages share the "00h" context; disambiguate by
        # byte value (lower pages are 0-127, upper pages 128-255).
        ctx = label_ctx or page_ctx
        page_range = None
        range_m = re.fullmatch(r"([0-9a-f]{2})h-([0-9a-f]{2})h", ctx or "")
        if range_m:
            page_range = (int(range_m.group(1), 16), int(range_m.group(2), 16))
            ctx = None
        if ctx == "00h" and span[0] < 128:
            ctx = "lower"
        # An explicit Page column (e.g. 20h/21h/22h/23h in the CMIS VDM
        # configuration table) files the row under its own page.
        if page_col is not None and page_col < len(cells):
            pm = re.fullmatch(r"([0-9A-Fa-f]{2})h", norm_text(cells[page_col]), re.I)
            if pm:
                page_val = pm.group(1).lower()
                ctx = "lower" if page_val == "00" and span[0] < 128 else page_val + "h"
                page_range = None

        if ctx is None and page_range is None:
            # No heading, title, label or Page cell gives this row a page: it
            # could never match a schema field, so it is recorded.
            _unplaceable_row(line_no, cells, "row has no page context; it cannot be placed")
            continue
        std.rows.append(StdRow(
            page_ctx=ctx, page_range=page_range, byte_lo=span[0], byte_hi=span[1],
            bit_lo=bits[0] if bits else None, bit_hi=bits[1] if bits else None,
            name=name, type_text=type_text, desc=desc,
            table_title=table_title, line=line_no,
        ))
    return carry_byte


# --- CMIS type-cell parsing -------------------------------------------------

_CMIS_ACCESS_TOKENS = [
    ("ro/cor", "ro_cor"),
    ("wo/sc", "wo_sc"),
    ("rww", "rww"),
    ("rw", "rw"),
    ("ro", "ro"),
    ("wo", "wo"),
]


def parse_cmis_type(type_text: str, name: str = "", desc: str = "") -> dict[str, Any]:
    """Parse a CMIS Type cell (and row name/description) into a marking."""
    t = norm_text(type_text).lower()
    access = None
    rest = t
    for token, value in _CMIS_ACCESS_TOKENS:
        if re.search(rf"\b{re.escape(token)}\b", t):
            access = value
            rest = re.sub(rf"\b{re.escape(token)}\b", " ", t)
            break
    support = None
    m = re.search(r"\b(rqd\.?|adv\.?|opt\.?)\b", rest)
    if m:
        support = m.group(1)
    latched = bool(re.match(r"^l[-\u2013]", norm_text(name).lower()))
    clear_on_read_desc = "clear on read" in norm_text(desc).lower()
    return {
        "access": access,
        "support": support,
        "latched_name": latched,
        "clear_on_read_desc": clear_on_read_desc,
        "type_text": norm_text(type_text),
    }


# --- Conventions (verified against the parsed text + authority report) -------

# Distinctive fragments of the researched passages this tool relies on.  Each
# fragment must be present in the parsed standard text and is looked up in the
# completed authority report for its citation (reuse, not repetition).
CONVENTION_PASSAGES: dict[tuple[str, str], dict[str, Any]] = {
    ("cmis", "reserved_3.0"): {
        "fragment": "Reserved bytes are for future use and shall not be used and shall be set to 0",
        "applies": ("3.0",),
        "meaning": "byte-scoped reserved statement only; no bit-level read-effect authority",
    },
    ("cmis", "reserved_4.0"): {
        "fragment": "Reserved locations (bytes, fields, or bits) are for future use",
        "applies": ("4.0",),
        "meaning": "reserved locations accessible and zero-initialized with no other module obligations",
    },
    ("cmis", "reserved_5.x"): {
        "fragment": "such locations are accessible, albeit without associated functionality",
        "applies": ("5.0", "5.1", "5.2", "5.3"),
        "meaning": "reserved locations accessible and zero-initialized with no other module obligations",
    },
    ("cmis", "read_side_effect"): {
        "fragment": "Side effects of a READ",
        "applies": ("5.0", "5.1", "5.2", "5.3"),
        "meaning": "read side effects must be specified to exist; COR is byte-scoped",
    },
    ("cmis", "access_types"): {
        "fragment": "A read-only element with clear-on-read (clear-after-read) side effect",
        "applies": ("5.0", "5.1", "5.2", "5.3"),
        "meaning": "RO/COR definition: all bits in a RO/COR byte are cleared after the byte is read",
    },
    ("cmis", "latched_prose"): {
        "fragment": "until cleared by a read operation that includes the affected bit",
        "applies": ("3.0", "4.0"),
        "meaning": "latched flags clear on read (the L- convention and its prose)",
    },
    ("cmis", "flags_12_13_prose"): {
        "fragment": "Byte 12 is reserved for future Module level flags",
        "applies": ("3.0",),
        "meaning": "byte 12 is reserved and byte 13 is provided for Custom Module Level Flags",
    },
    ("sff-8636", "startup_ack"): {
        "fragment": "The IntL bit will remain asserted until a read is performed of the Data_Not_Ready bit",
        "applies": ("2.11", "2.12"),
        "meaning": "reading byte 2 acknowledges the startup interrupt; other pending interrupts are not asserted cleared",
    },
    ("sff-8636", "alarm_latch"): {
        "fragment": "remain set until the fixed-side performs a read operation of the bit",
        "applies": ("2.11", "2.12"),
        "meaning": "bytes 3-21 interrupt flags are latched and cleared by reading the bit",
    },
    ("sff-8636", "reserved_fill"): {
        "fragment": "Reserved memory locations are to be filled with logic zeros",
        "applies": ("2.11", "2.12"),
        "meaning": "write-side fill rule only; no read-effect contract for reserved bits",
    },
    ("sff-8472", "enhanced_options"): {
        "fragment": "remain readable and writable",
        "applies": ("12.5a",),
        "meaning": "byte 110 bits 3 and 6 are host controls (RW); all bits readable",
    },
    ("sff-8472", "latched_flags"): {
        "fragment": "may be latched or nonlatched",
        "applies": ("12.5a",),
        "meaning": "bytes 112-117 flag latching is implementation specific; read effect stays unknown",
    },
}


def _record_passage_error(
    mismatches: list[dict[str, Any]],
    family: str,
    key: str,
    std: ParsedStandard,
    rev: str | None,
    message: str,
) -> None:
    """Record a missing-convention-passage error once per file and key.

    The error reaches the receipt (so --write exits 1): a convention passage
    the comparison relies on must be present in the applicable parsed text.
    """
    for m in mismatches:
        if (
            m.get("kind") == KIND_PASSAGE_NOT_FOUND
            and m.get("convention") == f"{family}/{key}"
            and m.get("source_file") == std.filename
        ):
            return  # already recorded for this file and convention
    mismatches.append({
        "kind": KIND_PASSAGE_NOT_FOUND,
        "convention": f"{family}/{key}",
        "revision": rev,
        "source_file": std.filename,
        "message": message,
    })


def _convention_citation(
    family: str,
    key: str,
    std: ParsedStandard,
    report: dict[str, Any] | None,
    mismatches: list[dict[str, Any]] | None = None,
    rev: str | None = None,
) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    """Verify a convention passage in the parsed text; cite where it was read.

    Returns (citation, error).  The citation records the standard file and line
    plus the authority-report row that researched the passage.  An error is
    returned - and recorded into ``mismatches`` (deduplicated per standard
    file and convention key, so it reaches the receipt and ``--write`` exits 1)
    - when the passage is absent from the applicable standard text or the
    convention does not apply to the requested revision: no expectation may
    rest on authority the parsed text does not contain.
    """
    conv = CONVENTION_PASSAGES[(family, key)]
    if rev is not None and rev not in conv["applies"]:
        message = (
            f"Convention passage for {family}/{key} applies only to revisions "
            f"{', '.join(conv['applies'])}, not {rev}; it may not be borrowed "
            f"for {std.filename}"
        )
        if mismatches is not None:
            _record_passage_error(mismatches, family, key, std, rev, message)
        return None, {"kind": KIND_PASSAGE_NOT_FOUND, "message": message}
    line = std.find_passage_line(conv["fragment"])
    report_row = find_authority_passage(report, conv["fragment"])
    if line is None:
        message = (
            f"Convention passage for {family}/{key} not found in parsed standard "
            f"{std.filename}: {conv['fragment']!r}"
        )
        if mismatches is not None:
            _record_passage_error(mismatches, family, key, std, rev, message)
        return None, {"kind": KIND_PASSAGE_NOT_FOUND, "message": message}
    citation = {
        "document": report_row["document"] if report_row else std.filename,
        "clause": report_row["clause_or_table"] if report_row else f"line {line}",
        "passage": conv["fragment"],
        "source_file": std.filename,
        "source_line": line,
        # Only a report row that carries the passage is cited as reused
        # research; otherwise the citation rests on the parsed standard text
        # alone (primary authority, verified above) and says so.
        "authority_report": (report or {}).get("path") if report_row else None,
    }
    if report_row is None:
        citation["authority_report_note"] = (
            "no row of the completed authority report carries this passage; cited "
            "from the parsed standard text"
        )
    return citation, None


# ---------------------------------------------------------------------------
# Expected-marking derivation and shared comparison helpers
# ---------------------------------------------------------------------------


def _cmis_row_expectation(
    std: ParsedStandard,
    rev: str,
    row: StdRow,
    report: dict[str, Any] | None,
    mismatches: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Derive the expected marking for one CMIS standard row.

    A read effect may rest only on convention authority that is present in the
    parsed text: when the passage is missing, the effect it would establish is
    downgraded to "unknown" and the passage error is recorded into
    ``mismatches`` (never a silent "no read effect" claim).
    """
    parsed = parse_cmis_type(row.type_text, row.name, row.desc)
    access = parsed["access"]
    read_effect: str | None = None
    inference: str | None = None
    byte_effect: dict[str, Any] | None = None
    # True when the row's read effect rests on a convention passage that was
    # verified in the parsed text; the byte builder only derives a byte-scoped
    # effect from established per-bit effects.
    effect_established = False
    authority: dict[str, Any] = {
        "document": std.filename,
        "clause": f"{row.table_title or 'field table'} line {row.line}",
        "source_file": std.filename,
    }

    if access == "ro_cor":
        # The RO/COR spelling is the table's own statement of the access type
        # (the D1 vocabulary maps it to clear-on-read); the byte-scoped
        # semantics ("all bits in the byte are cleared") come from the
        # access-types definition, so the byte effect is only recorded when
        # that passage is present.
        read_effect = EFFECT_CLEAR_ON_READ
        if rev in CONVENTION_PASSAGES[("cmis", "latched_prose")]["applies"]:
            # CMIS 3.0/4.0 type some flags RO/COR ("COR: Clear on read" in
            # the 4.0 acronyms) but carry no access-types definition; their
            # own latched-flag prose establishes the clear, bit-scoped ("a
            # read operation that includes the affected bit").  The 5.x
            # byte-scoped definition is never borrowed.
            conv_key = "latched_prose"
        else:
            conv_key = "access_types"
        cit, err = _convention_citation("cmis", conv_key, std, report, mismatches, rev)
        if err is None and cit is not None and conv_key == "latched_prose":
            authority = cit
            effect_established = True
        elif err is None and cit is not None:
            authority = cit
            byte_effect = {"effect": EFFECT_CLEAR_ON_READ, "scope": "byte"}
            effect_established = True
        else:
            authority = {
                **authority,
                "note": (
                    f"{conv_key} passage not found in the parsed text; "
                    "the scope of the clear-on-read is not established"
                ),
            }
    elif access == "ro" and parsed["latched_name"] and rev in ("3.0", "4.0"):
        cit, err = _convention_citation("cmis", "latched_prose", std, report, mismatches, rev)
        if err is None and cit is not None:
            read_effect = EFFECT_CLEAR_ON_READ
            authority = cit
            effect_established = True
        else:
            # The Type cell says only "RO": without the latched-flag prose the
            # effect is not established - unknown, never a silent "no effect".
            read_effect = "unknown"
    elif access == "ro" and parsed["clear_on_read_desc"] and rev == "4.0":
        cit, err = _convention_citation("cmis", "latched_prose", std, report, mismatches, rev)
        if err is None and cit is not None:
            read_effect = EFFECT_CLEAR_ON_READ
            authority = cit
            effect_established = True
        else:
            read_effect = "unknown"
    elif access is None and row.is_reserved_name():
        # Reserved row: apply only its own revision's reserved-location
        # authority; never borrow 4.0-5.3 authority for 3.0.
        if rev == "3.0":
            access = "unknown"
            read_effect = "unknown"
            cit, err = _convention_citation("cmis", "reserved_3.0", std, report, mismatches, rev)
            if err is None and cit is not None:
                authority = cit
            else:
                authority = {
                    "document": std.filename, "clause": f"line {row.line}",
                    "source_file": std.filename,
                    "note": "reserved-location statement not found in the parsed text",
                }
        else:
            key = "reserved_4.0" if rev == "4.0" else "reserved_5.x"
            cit, err = _convention_citation("cmis", key, std, report, mismatches, rev)
            if err is None and cit is not None:
                authority = cit
                access = "ro"
                read_effect = "none"
                # Established on the cited passage, as an interpretation:
                # readiness then says qualified_with_inference.
                effect_established = True
                inference = (
                    "CMIS reserved-location interpretation: reserved locations are accessible, "
                    "zero-initialized and carry no other module obligations, supporting a "
                    "side-effect-free read (interpretation, not an explicit read-effect sentence)"
                )
            else:
                access = "unknown"
                read_effect = "unknown"
    elif access is None and row.is_custom_name():
        access = "unknown"
        read_effect = "unknown"
    elif access is not None:
        if rev in ("5.0", "5.1", "5.2", "5.3"):
            # The READ side-effect clause is the 5.x authority that side
            # effects must be specified to exist: only with it present is a
            # typed location's "no read effect" established.  Without it the
            # effect stays unknown (never a default "none"); the passage
            # error reaches the receipt.
            cit, err = _convention_citation("cmis", "read_side_effect", std, report, mismatches, rev)
            if err is None and cit is not None:
                authority = cit
                read_effect = "none"
                effect_established = True
            else:
                read_effect = "unknown"
        else:
            # CMIS 3.0/4.0 carry no read-effect passage of their own: the
            # "Side effects of a READ" clause first appears in 5.0 (the parsed
            # 3.0/4.0 texts carry no "side effect" passage at all; 4.0 defines
            # only "RO : Read-Only" and "COR : Clear on read" as acronyms),
            # and the 5.x clause is never borrowed.  A Type cell states the
            # access, not the absence of a read effect, so a typed row with no
            # latched-flag convention (handled above) has an unknown read
            # effect: never a default "none", never qualified.
            read_effect = "unknown"
    else:
        # Untyped and not named reserved/custom: no access authority.
        return {
            "access": None, "read_effect": None, "inference": None,
            "authority": authority, "byte_effect": None,
            "effect_established": False,
            "standard_type": parsed["type_text"] or None,
        }

    return {
        "access": access,
        "read_effect": read_effect,
        "inference": inference,
        "authority": authority,
        "byte_effect": byte_effect,
        "effect_established": effect_established,
        "standard_type": parsed["type_text"] or None,
    }


def _select_bit_row(
    rows: list[StdRow],
    bit: int,
    mismatches: list[dict[str, Any]] | None = None,
    where: dict[str, Any] | None = None,
) -> StdRow | None:
    """The most specific standard row covering a bit.

    Bit-scoped rows from the detailed field tables win over whole-byte rows
    from overview memory maps (which define the byte, not the bit); smaller
    spans win.  Equally specific rows that disagree (different name or Type)
    are a conflict in the standard content: recorded into ``mismatches``,
    never resolved silently by line order.
    """
    candidates = [
        r for r in rows
        if r.bit_lo is None or (r.bit_lo <= bit <= (r.bit_hi if r.bit_hi is not None else r.bit_lo))
    ]
    if not candidates:
        return None  # no row covers the bit: callers record or block qualification
    def rank(r: StdRow) -> tuple[int, int]:
        span = 8 if r.bit_lo is None else (r.bit_hi if r.bit_hi is not None else r.bit_lo) - r.bit_lo + 1
        return (0 if r.bit_lo is not None else 1, span)
    best = min(rank(r) for r in candidates)
    top = sorted((r for r in candidates if rank(r) == best), key=lambda r: -r.line)
    if mismatches is not None and len({(norm_text(r.name), norm_text(r.type_text)) for r in top}) > 1:
        mismatches.append({
            "kind": KIND_CONFLICTING_ROWS,
            **(where or {}),
            "bit": bit,
            "lines": [r.line for r in top],
            "message": (
                f"Equally specific standard rows disagree for bit {bit}: "
                + "; ".join(f"line {r.line} {r.name!r} {r.type_text!r}" for r in top)
            ),
        })
    return top[0]


def _page_ctx_of(field: SchemaField) -> str:
    """Effective standard page context of a field.

    SFF-8472 sub-pages share page_address A2h and are distinguished by
    page_select (e.g. 02h); the parsed standards file them under their own
    page context.
    """
    if field.page_select:
        addr = field.page_select.lower()
    else:
        # load_family_fields refuses a page without page_address, so the
        # fallback is never reached for a loaded field.
        addr = (field.page_address or "lower").lower()
    if addr in ("lower", "lower-memory", "lower_memory"):
        return "lower"
    if addr.endswith("h"):
        return addr[:-1].lower() + "h"
    return addr


def _match_rows(
    std: ParsedStandard, page_ctx: str, byte: int, bit_mask: int | None
) -> list[StdRow]:
    """Standard rows for a byte, optionally overlapping a bit mask."""
    field_page = _page_value(page_ctx)
    out = []
    for row in std.rows:
        # Filters only select the rows for this location; a field no row
        # matches is recorded by every caller (no_standard_authority /
        # missing_applicable_standard_comparison).
        if not (row.byte_lo <= byte <= row.byte_hi):
            continue
        if row.page_ctx == page_ctx:
            pass  # same page: matched
        elif row.page_range and field_page is not None:
            lo, hi = row.page_range
            if not (lo <= field_page <= hi):
                continue
        else:
            continue
        if bit_mask is None or row.bit_mask & bit_mask:
            out.append(row)
    return out


def _dedupe_rows(rows: list[StdRow]) -> list[StdRow]:
    seen: set[tuple[Any, ...]] = set()
    out = []
    for r in rows:
        key = (r.page_ctx, r.byte_lo, r.byte_hi, r.bit_lo, r.bit_hi, r.name, r.type_text, r.line)
        if key not in seen:
            seen.add(key)
            out.append(r)
    return out


def _allowed_schema_access(
    expected_access: str | None, expected_effect: str | None, inference: str | None
) -> list[str]:
    """The schema access spellings that conform at a location.

    - a typed location accepts its own type (plus ro_cor where the standard
      establishes clear-on-read);
    - an honestly Unknown marking conforms wherever the standard leaves the
      read effect undefined or the effect rests on interpretation;
    - a reserved location with only interpretive authority (CMIS 4.0-5.3)
      accepts both the interpretation (ro) and the conservative unknown.
    """
    if expected_access is None:
        return ["unknown"]
    allowed = {expected_access}
    if expected_effect == EFFECT_CLEAR_ON_READ:
        allowed.add("ro_cor")
    if expected_effect == "unknown" or (inference is not None and expected_effect == "none"):
        allowed.add("unknown")
    return sorted(allowed)


def _effect_axis_mismatch(schema_access: str | None, expected: dict[str, Any]) -> str | None:
    """Return a reason when the schema's effect assertion contradicts authority."""
    exp_effect = expected.get("read_effect")
    if schema_access == "ro_cor":
        if exp_effect != EFFECT_CLEAR_ON_READ:
            return "schema asserts clear-on-read; the standard does not establish it for this location"
        return None
    if schema_access in ("ro", "rw", "rww") and exp_effect == EFFECT_CLEAR_ON_READ:
        return "standard establishes clear-on-read; schema marks it without the ro_cor spelling"
    return None


def _rev_key(rev: Any) -> tuple[int, ...] | None:
    """Numeric ordering key of a revision string ('4.0', '5.3'); None when
    it is not a dotted number."""
    m = re.fullmatch(r"\s*(\d+(?:\.\d+)*)\s*", str(rev))
    return tuple(int(x) for x in m.group(1).split(".")) if m else None


def _applies_at(field: SchemaField, rev: str) -> bool:
    """True when the field's revision applicability (since/until) includes
    rev: ``since`` inclusive, ``until`` exclusive, the same rule as the
    codegen's ``applicable_revs`` (crates/openxcvr-codegen/src/ir/version.rs),
    so a marking is compared exactly where generated readers apply it.  An
    unparseable bound is recorded by _record_marking_errors and the field is
    then compared at every revision (never silently dropped)."""
    rk = _rev_key(rev)
    if rk is None:
        return True  # family revision is not dotted-numeric: since/until cannot narrow it
    lo = _rev_key(field.since) if field.since is not None else None
    hi = _rev_key(field.until) if field.until is not None else None
    if lo is not None and rk < lo:
        return False
    # Basis: version.rs documents "`until` (exclusive): field is absent at and
    # after this rev" and filters `*r < u` in applicable_revs.
    if hi is not None and rk >= hi:
        return False
    return True


def _covering_fields(
    schema_fields: list[SchemaField], offset: int, bit: int, rev: str
) -> list[SchemaField]:
    """Every schema field applicable at rev that covers one bit (all of
    them, never only the first)."""
    return [
        f for f in schema_fields
        if f.byte_lo <= offset <= f.byte_hi
        and (f.bit_lo is None or (f.bit_lo <= bit <= (f.bit_hi if f.bit_hi is not None else f.bit_lo)))
        and _applies_at(f, rev)
    ]


def _record_marking_errors(field: SchemaField, mismatches: list[dict[str, Any]]) -> bool:
    """Record missing/unparseable/retired marking errors for one field.

    Returns True when the marking is unusable for comparison.
    """
    unusable = False
    where = {"field": field.name, "page": field.page_id, "offset": field.byte_lo}
    if field.access is None:
        mismatches.append({
            **where, "kind": KIND_MISSING_MARKING, "axis": "access",
            "message": f"Field {field.name!r} carries no access marking (field or template)",
        })
        unusable = True
    elif field.access not in ALLOWED_ACCESS:
        mismatches.append({
            **where, "kind": KIND_UNPARSEABLE_MARKING, "axis": "access",
            "value": str(field.access),
            "message": f"Field {field.name!r} access {field.access!r} is not in the D1 vocabulary",
        })
        unusable = True
    if field.is_dynamic is None:
        mismatches.append({
            **where, "kind": KIND_MISSING_MARKING, "axis": "is_dynamic",
            "message": f"Field {field.name!r} carries no flags.is_dynamic marking",
        })
        unusable = True
    for axis in ("since", "until"):
        value = getattr(field, axis)
        if value is not None and _rev_key(value) is None:
            mismatches.append({
                **where, "kind": KIND_UNPARSEABLE_MARKING, "axis": axis,
                "value": str(value),
                "message": (
                    f"Field {field.name!r} {axis} {value!r} is not a revision number; "
                    "its revision applicability cannot be compared"
                ),
            })
            unusable = True
    for spelling in field.retired:
        mismatches.append({
            **where, "kind": KIND_RETIRED_MARKING, "spelling": spelling,
            "message": (
                f"Field {field.name!r} uses retired marking spelling {spelling!r}; "
                "the D1 vocabulary has one spelling"
            ),
        })
    return unusable


def _compare_bits_against_expected(
    schema_access: str | None,
    expected: dict[str, Any],
    where: dict[str, Any],
    mismatches: list[dict[str, Any]],
) -> bool:
    """Compare one marking against one expected location; True on mismatch."""
    if schema_access is None or schema_access not in ALLOWED_ACCESS:
        # The field-level marking error is recorded by _record_marking_errors;
        # this bit cannot conform.
        return True
    allowed = _allowed_schema_access(
        expected.get("access"), expected.get("read_effect"), expected.get("inference")
    )
    if schema_access not in allowed:
        mismatches.append({
            "kind": KIND_ACCESS_MISMATCH,
            "expected": expected.get("access"),
            "allowed": allowed,
            "actual": schema_access,
            "authority": expected.get("authority"),
            **where,
        })
        return True
    reason = _effect_axis_mismatch(schema_access, expected)
    if reason:
        mismatches.append({
            "kind": KIND_ACCESS_MISMATCH,
            "expected": "ro_cor",
            "actual": schema_access,
            "reason": reason,
            "authority": expected.get("authority"),
            **where,
        })
        return True
    return False


def _compare_all_covering(
    covering: list[SchemaField],
    expected: dict[str, Any],
    where: dict[str, Any],
    mismatches: list[dict[str, Any]],
    rev: str | None,
    offset: int,
    bit: int,
) -> str:
    """Compare every overlapping schema field covering one bit.

    Overlapping schema fields covering one bit are all compared; when they
    disagree the overlap itself is an error.  Returns the bit status.
    """
    bit_status = "conformant"
    names = [f.name for f in covering]
    markings = {(f.access, f.is_dynamic) for f in covering}
    if len(markings) > 1:
        mismatches.append({
            "kind": KIND_OVERLAPPING_FIELDS,
            "revision": rev, "offset": offset, "bit": bit,
            "fields": names,
            "markings": [
                f"{f.name}: access={f.access}, is_dynamic={f.is_dynamic}" for f in covering
            ],
            "message": (
                f"Schema fields {names} overlap at byte {offset} bit {bit} with "
                "disagreeing volatility markings; the overlap is an error"
            ),
        })
        bit_status = "mismatch"
    for f in covering:
        if _compare_bits_against_expected(f.access, expected, where, mismatches):
            bit_status = "mismatch"
    return bit_status


def _compare_field_entry(
    field: SchemaField,
    family: str,
    revisions: list[str],
    stds: dict[str, ParsedStandard | None],
    report: dict[str, Any] | None,
    expectation_fn: Any,
    mismatches: list[dict[str, Any]],
    marking_ok: bool,
) -> dict[str, Any]:
    """Compare one schema field against every revision's standard rows."""
    entry = {
        "kind": "field",
        **field.to_record(),
        "expected": {},
        "status": "marking_error" if not marking_ok else "not_compared",
    }
    page_ctx = _page_ctx_of(field)
    any_compared = False
    any_mismatch = not marking_ok
    matched_untyped = False

    for rev in revisions:
        std = stds.get(rev)
        if not _applies_at(field, rev):
            # The schema's own revision applicability (since/until) excludes
            # this revision: recorded per revision, compared nowhere else.
            entry["expected"][rev] = {
                "status": "not_applicable_at_revision",
                "since": field.since, "until": field.until,
            }
            continue
        if std is None:
            # The missing standard is recorded by _std_for_revision as
            # missing_applicable_standard_comparison.
            entry["expected"][rev] = {"status": "not_compared"}
            continue
        rows: list[StdRow] = []
        for b in range(field.byte_lo, field.byte_hi + 1):
            mask = field.bit_mask if field.bit_lo is not None else None
            rows.extend(_match_rows(std, page_ctx, b, mask))
        rows = _dedupe_rows(rows)
        if not rows:
            # This revision's tables define no row for the location although
            # the field applies here: unresolved authority, which
            # account_completeness counts as an error (never a pass).
            entry["expected"][rev] = {
                "status": "unresolved",
                "reason": "no standard row for this location at an applicable revision",
            }
            continue

        rev_mismatch = False
        per_rev: dict[str, Any] | None = None
        matched_untyped = False
        compared_rows: list[dict[str, Any]] = []
        typed_mask: dict[int, int] = {}
        for row in rows:
            expected = expectation_fn(std, rev, row, report, mismatches)
            if expected.get("access") is None and expected.get("read_effect") is None:
                # An untyped row carries no access authority; the bytes/bits it
                # alone covers are reported by the partial-coverage check below.
                matched_untyped = True
                continue
            any_compared = True
            for b in range(max(row.byte_lo, field.byte_lo), min(row.byte_hi, field.byte_hi) + 1):
                typed_mask[b] = typed_mask.get(b, 0) | (row.bit_mask & field.bit_mask)
            compared_rows.append({
                "line": row.line,
                "access": expected.get("access"),
                "read_effect": expected.get("read_effect"),
            })
            where = {
                "field": field.name, "page": field.page_id, "revision": rev,
                "offset": field.byte_lo,
                "bits": None if field.bit_lo is None else [field.bit_lo, field.bit_hi],
            }
            if _compare_bits_against_expected(field.access, expected, where, mismatches):
                rev_mismatch = True
                any_mismatch = True
            if per_rev is None:
                per_rev = {
                    "status": "conformant",
                    "access": expected.get("access"),
                    "read_effect": expected.get("read_effect"),
                    "inference": expected.get("inference"),
                    "authority": expected.get("authority"),
                    "standard_type": expected.get("standard_type"),
                    "allowed": _allowed_schema_access(
                        expected.get("access"), expected.get("read_effect"),
                        expected.get("inference"),
                    ),
                }
        if per_rev is None:
            # The revision's tables locate the field but type none of its rows:
            # unresolved authority at an applicable revision, counted by
            # account_completeness.
            entry["expected"][rev] = {
                "status": "unresolved",
                "reason": "only untyped standard rows at an applicable revision",
            }
        else:
            # Every typed row is compared (not only the first, which is the
            # one summarized above); a byte or bit of the field that no typed
            # row covers was compared by nothing: an error, never a partial
            # pass.
            uncovered = {
                b: field.bit_mask & ~typed_mask.get(b, 0)
                for b in range(field.byte_lo, field.byte_hi + 1)
                if field.bit_mask & ~typed_mask.get(b, 0)
            }
            if uncovered:
                rev_mismatch = True
                any_mismatch = True
                mismatches.append({
                    "kind": KIND_NO_STANDARD_AUTHORITY,
                    "field": field.name, "page": field.page_id, "revision": rev,
                    "offset": field.byte_lo,
                    "uncovered": {str(b): m for b, m in sorted(uncovered.items())},
                    "message": (
                        f"Marked field {field.name!r} is only partly covered by {rev} "
                        f"standard rows: bytes {sorted(uncovered)} have bits no typed "
                        "row defines"
                    ),
                })
            per_rev["compared_rows"] = compared_rows
            per_rev["status"] = "mismatch" if rev_mismatch else "conformant"
            entry["expected"][rev] = per_rev

    if not any_compared and not any_mismatch:
        # The marked field was never compared against any revision's applicable
        # content: an error, never a silent skip.
        if matched_untyped:
            detail = (
                "the applicable tables describe this location but carry no access "
                "or read-effect authority for it"
            )
        else:
            detail = "no field-table rows were found for this location"
        mismatches.append({
            "kind": KIND_NO_STANDARD_AUTHORITY,
            "field": field.name,
            "page": field.page_id,
            "offset": field.byte_lo,
            "bits": None if field.bit_lo is None else [field.bit_lo, field.bit_hi],
            "page_address": field.page_address,
            "message": (
                f"Marked field {field.name!r} at {field.page_address} byte(s) "
                f"{field.byte_lo}-{field.byte_hi} has no standard authority: {detail}"
            ),
        })
        entry["status"] = "no_authority"
    elif any_mismatch:
        entry["status"] = "mismatch"
    else:
        entry["status"] = "conformant"
    return entry


def _std_for_revision(
    family: str,
    rev: str,
    standards_dir: Path | None,
    standard_files: list[dict[str, str]],
    mismatches: list[dict[str, Any]],
) -> ParsedStandard | None:
    """Load and parse the applicable standard file for one revision."""
    filename = STANDARD_FILENAMES.get((family, rev))
    if filename is None:
        mismatches.append({
            "kind": KIND_MISSING_STANDARD,
            "revision": rev,
            "message": f"No applicable standard file mapping for {family} revision {rev}",
        })
        return None
    if standards_dir is None:
        mismatches.append({
            "kind": KIND_MISSING_STANDARD,
            "revision": rev,
            "file": filename,
            "message": f"Standards directory not provided; cannot compare {family} revision {rev}",
        })
        return None
    path = standards_dir / filename
    if not path.is_file():
        mismatches.append({
            "kind": KIND_MISSING_STANDARD,
            "revision": rev,
            "file": filename,
            "message": f"Applicable standard file is missing: {path}",
        })
        return None
    std = parse_standard_file(path, family)
    # Rows that could not be ingested are errors that reach the receipt:
    # standard content is never silently dropped.
    mismatches.extend(std.ingest_errors)
    standard_files.append({"revision": rev, "file": filename, "sha256": std.sha256})
    return std


# ---------------------------------------------------------------------------
# CMIS comparison
# ---------------------------------------------------------------------------

CMIS_EXPANDED_BYTES = (3, 8, 9, 10, 11, 12, 13)


def _cmis_expanded_byte_entry(
    std: ParsedStandard,
    rev: str,
    report: dict[str, Any] | None,
    offset: int,
    schema_fields: list[SchemaField],
    mismatches: list[dict[str, Any]],
) -> dict[str, Any] | None:
    """Build the per-revision bit-expanded inventory entry for one CMIS byte.

    Byte 3 is the detection/readiness byte; bytes 8-13 are the module-flag
    block (bytes 8-11 RO/COR, byte 12 Reserved, byte 13 Custom), whose
    byte-scoped clear-on-read effect the receipt must record.
    """
    rows = _dedupe_rows(_match_rows(std, "lower", offset, None))
    if not rows and offset in (12, 13) and rev == "3.0":
        # CMIS 3.0 types bytes 12/13 only in the module-flags prose (the
        # table itself carries no rows for them); byte 12 is reserved and
        # byte 13 is Custom, so their read effects stay unknown.  The prose
        # convention applies to 3.0 only: it is never borrowed for 4.0+.
        cit, err = _convention_citation(
            "cmis", "flags_12_13_prose", std, report, mismatches, rev
        )
        if err is None and cit is not None:
            rows = [StdRow(
                page_ctx="lower", byte_lo=offset, byte_hi=offset,
                bit_lo=0, bit_hi=7,
                name="Reserved" if offset == 12 else "Custom",
                type_text="", desc="Reserved" if offset == 12 else "Custom",
                table_title="module-flags prose", line=cit.get("source_line"),
            )]
    if not rows:
        mismatches.append({
            "kind": KIND_MISSING_STANDARD,
            "revision": rev,
            "offset": offset,
            "message": (
                f"CMIS {rev} lower memory byte {offset}: no field-table rows parsed; "
                "the applicable comparison content is missing"
            ),
        })
        return None

    bits: list[dict[str, Any]] = []
    unknown_constituents: list[int] = []
    unknown_mask = 0
    uncovered_mask = 0
    compared_mask = 0
    byte_effect: dict[str, Any] | None = None
    # Every byte-scoped effect a constituent row states; disagreeing
    # candidates are an order-dependent resolution, which
    # account_completeness records (never first-bit-wins).
    byte_effect_candidates: list[dict[str, Any]] = []

    for bit in range(8):
        row = _select_bit_row(
            rows, bit, mismatches, {"revision": rev, "offset": offset}
        )
        if row is None:
            # No standard row (not even a byte-level one) defines this bit.
            # A schema marking there would be compared by nothing: an error.
            covering = _covering_fields(schema_fields, offset, bit, rev)
            if covering:
                mismatches.append({
                    "kind": KIND_NO_STANDARD_AUTHORITY,
                    "revision": rev,
                    "offset": offset,
                    "bit": bit,
                    "fields": [f.name for f in covering],
                    "message": (
                        f"Schema marks CMIS {rev} lower byte {offset} bit {bit} but the "
                        "standard's tables define no row for it; the marking has no "
                        "standard authority"
                    ),
                })
            continue  # not appended: an uncompared bit blocks qualification
        # A bit is only "defined at bit level" when a bit-scoped row covers
        # it; whole-byte overview rows define the byte, not each bit, so an
        # uncovered bit under byte-level coverage is a byte gap (no schema
        # marking exists there to compare), not a missing-bit comparison
        # error - but it still blocks whole-byte qualification.
        bit_defined = any(
            r.bit_lo is not None
            and r.bit_lo <= bit <= (r.bit_hi if r.bit_hi is not None else r.bit_lo)
            for r in rows
        )
        expected = _cmis_row_expectation(std, rev, row, report, mismatches)
        covering = _covering_fields(schema_fields, offset, bit, rev)
        # covering[0] only labels the record: every covering field is
        # compared below and a disagreeing overlap is its own error.
        schema_access = covering[0].access if covering else "uncovered"
        schema_dyn = covering[0].is_dynamic if covering else None

        if expected.get("byte_effect"):
            byte_effect_candidates.append({
                **expected["byte_effect"], "bit": bit, "authority": expected.get("authority"),
            })
        if expected.get("read_effect") == "unknown":
            unknown_constituents.append(bit)

        bit_status = "conformant"
        if not covering and not bit_defined:
            continue  # byte-level gap only: no schema marking exists at this bit
        if not covering:
            uncovered_mask |= 1 << bit
            mismatches.append({
                "kind": KIND_MISSING_RESERVED_BIT,
                "revision": rev,
                "offset": offset,
                "bit": bit,
                "message": (
                    f"CMIS {rev} lower byte {offset} bit {bit} is defined by the standard "
                    "but uncovered by any schema field"
                ),
            })
            bit_status = "mismatch"
        else:
            where = {"revision": rev, "offset": offset, "bit": bit}
            bit_status = _compare_all_covering(
                covering, expected, where, mismatches, rev, offset, bit
            )
            if schema_access == "unknown":
                unknown_mask |= 1 << bit

        compared_mask |= 1 << bit
        bits.append({
            "bit": bit,
            "field_name": covering[0].name if covering else None,
            "covering_fields": [f.name for f in covering],
            "covering_field_ids": [f.uid for f in covering],
            "schema_access": schema_access,
            "schema_change_source": (
                ("module" if schema_dyn else "host_or_neither") if schema_dyn is not None else None
            ),
            "standard_type": expected.get("standard_type"),
            "access_permission": expected.get("access"),
            "read_effect": expected.get("read_effect"),
            "authority": expected.get("authority"),
            "inference": expected.get("inference"),
            "effect_established": bool(expected.get("effect_established")),
            "status": bit_status,
        })

    distinct_effects = {(c.get("effect"), c.get("scope")) for c in byte_effect_candidates}
    if len(distinct_effects) == 1:
        first = byte_effect_candidates[0]
        byte_effect = {"effect": first["effect"], "scope": first["scope"],
                       "authority": first["authority"]}
    elif distinct_effects:
        byte_effect = {
            "effect": "unknown", "scope": "byte",
            "note": "constituent rows state conflicting byte-scoped effects",
        }

    if byte_effect is None:
        # CMIS 3.0/4.0 express the latched-flag clear-on-read in prose (the
        # "L-" convention / "Clear on Read" descriptions): the effect clears
        # the affected bit, not the whole byte as RO/COR does in 5.x.  Only
        # bits whose effect rests on a verified passage establish it; a
        # missing access-types passage never silently narrows a byte-scoped
        # clear into a bit-scoped one.
        latched = [
            b for b in bits
            if b.get("read_effect") == EFFECT_CLEAR_ON_READ and b.get("effect_established")
        ]
        if latched:
            byte_effect = {
                "effect": EFFECT_CLEAR_ON_READ,
                "scope": "bit",
                "authority": latched[0].get("authority"),
            }

    standard_bit_mask = 0
    for r in rows:
        if r.bit_lo is not None:
            for b in range(r.bit_lo, (r.bit_hi if r.bit_hi is not None else r.bit_lo) + 1):
                standard_bit_mask |= 1 << b

    return {
        "kind": "byte",
        "revision": rev,
        "page": "lower-memory",
        "offset": offset,
        "standard_bit_mask": standard_bit_mask,
        "byte_scoped_effect": byte_effect,
        "byte_effect_candidates": byte_effect_candidates,
        "unknown_constituents_required": unknown_constituents,
        "unknown_mask": unknown_mask,
        "uncovered_mask": uncovered_mask,
        "compared_mask": compared_mask,
        # Set by account_completeness (_qualify_byte), the one place that
        # decides whole-byte qualification.
        "whole_byte_qualified": None,
        "qualification": None,
        "bits": bits,
    }


def compare_cmis(
    schemas_dir: Path,
    standards_dir: Path | None,
    authority_report: dict[str, Any] | None,
    specific_revisions: list[str] | None = None,
    fields: list[SchemaField] | None = None,
    load_errors: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Compare CMIS schema markings against all applicable revisions."""
    revisions = list(specific_revisions or CMIS_REVISIONS)
    if fields is None:
        fields, load_errors = load_family_fields("cmis", schemas_dir)
    load_errors = list(load_errors or [])

    standard_files: list[dict[str, str]] = []
    mismatches: list[dict[str, Any]] = list(load_errors)
    stds: dict[str, ParsedStandard | None] = {}
    for rev in revisions:
        stds[rev] = _std_for_revision("cmis", rev, standards_dir, standard_files, mismatches)

    inventory: list[dict[str, Any]] = []

    expanded_fields = [
        f for f in fields
        if _page_ctx_of(f) == "lower"
        and any(f.byte_lo <= b <= f.byte_hi for b in CMIS_EXPANDED_BYTES)
    ]
    # A field only partly inside the expanded bytes is also compared as a
    # field entry, so its bytes outside them are never compared by nothing.
    general_fields = [
        f for f in fields
        if f not in expanded_fields
        or any(b not in CMIS_EXPANDED_BYTES for b in range(f.byte_lo, f.byte_hi + 1))
    ]

    # Marking errors are recorded once per field (all fields, expanded or not).
    marking_ok: dict[int, bool] = {}
    for field in fields:
        marking_ok[id(field)] = not _record_marking_errors(field, mismatches)

    for rev in revisions:
        std = stds.get(rev)
        if std is None:
            continue  # recorded by _std_for_revision as missing_applicable_standard_comparison
        for offset in CMIS_EXPANDED_BYTES:
            entry = _cmis_expanded_byte_entry(
                std, rev, authority_report, offset, expanded_fields, mismatches
            )
            if entry is not None:
                inventory.append(entry)

    for field in general_fields:
        inventory.append(_compare_field_entry(
            field, "cmis", revisions, stds, authority_report,
            _cmis_row_expectation, mismatches, marking_ok[id(field)],
        ))

    if not inventory:
        mismatches.append({
            "kind": KIND_EMPTY_COMPARISON,
            "message": "No CMIS fields or bytes were compared",
        })

    return _finalize_comparison("cmis", schemas_dir, fields, load_errors, {
        "family": "cmis",
        "revisions_checked": revisions,
        "standard_files": standard_files,
        "compared_inventory": inventory,
        "mismatches": mismatches,
    })


# ---------------------------------------------------------------------------
# SFF-8636 comparison
# ---------------------------------------------------------------------------

SFF8636_EXPANDED_BYTES = tuple([2] + list(range(3, 22)))


def _sff8636_type_access(type_text: str | None) -> str | None:
    """Access stated by an SFF-8636 Type cell (None when it states none)."""
    t = norm_text(type_text).lower()
    if "write-only" in t:
        return "wo"
    if "read/write" in t:
        return "rw"
    if "read-only" in t:
        return "ro"
    return None


def _sff8636_row_expectation(
    std: ParsedStandard,
    rev: str,
    row: StdRow,
    report: dict[str, Any] | None,
    mismatches: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Derive the expected marking for one SFF-8636 standard row.

    A read effect may rest only on prose authority present in the parsed text:
    when the passage is missing the effect is downgraded to "unknown" and the
    passage error is recorded, never a silent "no read effect" claim.
    """
    # Byte-level access: the row's own Type cell; else the writable memory
    # blocks (Table 5-3) for every byte of the row; else the memory-map
    # overview rows (Tables 6-1/6-2) that state Read-Only/Read/Write for the
    # byte range.  A byte none of them types has no access authority: never
    # a default "ro" (the caller records no_standard_authority).
    access = _sff8636_type_access(row.type_text)
    no_access_reason: str | None = None
    if access is None:
        span = range(row.byte_lo, row.byte_hi + 1)
        writable = {std.writable_bytes.get((str(row.page_ctx), b)) for b in span}
        conflicted = [b for b in span if (str(row.page_ctx), b) in std.writable_conflicts]
        if conflicted:
            # Conflicting writable-block rows (recorded at ingest): no
            # authority, never a default or a row-order choice.
            no_access_reason = f"writable-block rows conflict for bytes {conflicted}"
        elif len(writable) == 1 and None not in writable:
            access = writable.pop()
        elif len(writable) > 1:
            no_access_reason = (
                f"writable-block map is mixed over bytes {row.byte_lo}-{row.byte_hi}"
            )
        else:
            overview = {
                _sff8636_type_access(r.type_text)
                for r in std.rows
                if r is not row and r.page_ctx == row.page_ctx
                and r.byte_lo <= row.byte_lo and row.byte_hi <= r.byte_hi
                and _sff8636_type_access(r.type_text) is not None
            }
            if len(overview) == 1:
                access = overview.pop()
            elif overview:
                no_access_reason = (
                    f"memory-map overview rows disagree ({sorted(overview)}) for bytes "
                    f"{row.byte_lo}-{row.byte_hi}"
                )
            else:
                no_access_reason = (
                    f"no Type cell, writable block or overview row types bytes "
                    f"{row.byte_lo}-{row.byte_hi}"
                )
    read_effect: str | None = None
    authority: dict[str, Any] = {
        "document": std.filename,
        "clause": f"{row.table_title or 'field table'} line {row.line}",
        "source_file": std.filename,
    }

    if row.is_reserved_name():
        # Reserved-fill is a write-side rule only: no read-effect contract, so
        # the read effect of reserved bits stays Unknown.
        access = "unknown"
        read_effect = "unknown"
        cit, err = _convention_citation("sff-8636", "reserved_fill", std, report, mismatches, rev)
        if err is None and cit is not None:
            authority = cit
        else:
            authority = {
                "document": std.filename,
                "clause": f"line {row.line}",
                "source_file": std.filename,
                "note": "reserved-fill note not found in the parsed text; read effect stays unknown",
            }
        return {
            "access": access, "read_effect": read_effect, "inference": None,
            "authority": authority, "standard_type": row.type_text or None,
        }

    if access is None:
        # Untyped location: no access authority, so nothing is expected; the
        # field comparison records no_standard_authority and the expanded
        # byte builder only accepts an honest unknown.
        return {
            "access": None, "read_effect": None, "inference": None,
            "authority": {**authority, "note": no_access_reason},
            "standard_type": row.type_text or None,
            "reason": no_access_reason,
        }

    if 3 <= row.byte_lo <= 21:
        # Bytes 3-21 interrupt flags: latched, cleared by reading the bit.
        # Without the alarm-latch prose the effect is not established:
        # unknown, never a silent "no read effect" claim.
        cit, err = _convention_citation("sff-8636", "alarm_latch", std, report, mismatches, rev)
        if err is None and cit is not None:
            read_effect = EFFECT_CLEAR_ON_READ
            authority = cit
        else:
            read_effect = "unknown"
    if row.byte_lo == 2 and row.bit_lo is not None and row.bit_lo <= 0 <= (
        row.bit_hi if row.bit_hi is not None else row.bit_lo
    ):
        # Data_Not_Ready: its read acknowledges the startup interrupt.  The
        # receipt does not require the byte-2 read effect to be None and does
        # not assert that other pending interrupts are cleared.
        cit, err = _convention_citation("sff-8636", "startup_ack", std, report, mismatches, rev)
        if err is None and cit is not None:
            read_effect = EFFECT_ACK_STARTUP
            authority = cit
        else:
            read_effect = "unknown"

    return {
        "access": access,
        "read_effect": read_effect,
        "inference": None,
        "authority": authority,
        "standard_type": row.type_text or None,
    }


def _sff8636_expanded_byte_entry(
    std: ParsedStandard,
    rev: str,
    report: dict[str, Any] | None,
    offset: int,
    schema_fields: list[SchemaField],
    mismatches: list[dict[str, Any]],
) -> dict[str, Any] | None:
    rows = _dedupe_rows(_match_rows(std, "lower", offset, None))
    if not rows:
        # A mandated expanded byte with no standard rows is an error, never a
        # silently dropped comparison: the schema fields touching it are
        # routed through the general field path by the caller.
        mismatches.append({
            "kind": KIND_NO_STANDARD_AUTHORITY,
            "revision": rev,
            "offset": offset,
            "message": (
                f"SFF-8636 byte {offset} is a mandated comparison location but the parsed "
                "standard yields no rows for it; its schema markings have no standard "
                "authority"
            ),
        })
        return None

    bits: list[dict[str, Any]] = []
    unknown_constituents: list[int] = []
    unknown_mask = 0
    uncovered_mask = 0
    compared_mask = 0
    byte_effect: dict[str, Any] | None = None

    if offset == 2:
        cit, err = _convention_citation(
            "sff-8636", "startup_ack", std, report, mismatches, rev
        )
        if err is not None or cit is None:
            mismatches.append({
                "kind": KIND_OMITTED_BYTE_EFFECT,
                "revision": rev,
                "offset": 2,
                "message": (
                    "SFF-8636 byte 2 must carry the AcknowledgeStartupInterrupt byte-scoped "
                    "effect; the startup-interrupt prose was not found in the parsed standard"
                ),
            })
        else:
            byte_effect = {
                "effect": EFFECT_ACK_STARTUP,
                "scope": "bit read of Data_Not_Ready (Byte 2)",
                "asserts_other_pending_interrupts_cleared": False,
                "authority": cit,
            }
    elif 3 <= offset <= 21:
        cit, err = _convention_citation(
            "sff-8636", "alarm_latch", std, report, mismatches, rev
        )
        if err is None and cit is not None:
            byte_effect = {
                "effect": EFFECT_ALARM_LATCH,
                "scope": "bit",
                "authority": cit,
            }
        else:
            # A missing alarm-latch passage is never converted into an
            # affirmative "no read effect": the byte-scoped effect stays
            # unknown, and the passage error reaches the receipt.
            byte_effect = {
                "effect": "unknown",
                "scope": "bit",
                "authority": {
                    "document": std.filename,
                    "source_file": std.filename,
                    "note": "alarm-latch passage not found in the parsed text",
                },
            }

    for bit in range(8):
        row = _select_bit_row(
            rows, bit, mismatches, {"revision": rev, "offset": offset}
        )
        if row is None:
            # No standard row defines this bit; a schema marking there would
            # be compared by nothing: an error.
            covering = _covering_fields(schema_fields, offset, bit, rev)
            if covering:
                mismatches.append({
                    "kind": KIND_NO_STANDARD_AUTHORITY,
                    "revision": rev, "offset": offset, "bit": bit,
                    "fields": [f.name for f in covering],
                    "message": (
                        f"Schema marks SFF-8636 byte {offset} bit {bit} but the standard's "
                        "tables define no row for it; the marking has no standard authority"
                    ),
                })
            continue  # not appended: an uncompared bit blocks qualification
        # A bit is only "defined at bit level" when a bit-scoped row covers
        # it; whole-byte overview rows define the byte, not each bit, so an
        # uncovered bit under byte-level coverage is a byte gap (no schema
        # marking exists there to compare), not a missing-bit comparison
        # error - but it still blocks whole-byte qualification.
        bit_defined = any(
            r.bit_lo is not None
            and r.bit_lo <= bit <= (r.bit_hi if r.bit_hi is not None else r.bit_lo)
            for r in rows
        )
        expected = _sff8636_row_expectation(std, rev, row, report, mismatches)
        covering = _covering_fields(schema_fields, offset, bit, rev)
        # covering[0] only labels the record: every covering field is
        # compared below and a disagreeing overlap is its own error.
        schema_access = covering[0].access if covering else "uncovered"
        schema_dyn = covering[0].is_dynamic if covering else None

        if expected.get("read_effect") == "unknown":
            unknown_constituents.append(bit)

        bit_status = "conformant"
        if not covering and not bit_defined:
            continue  # byte-level gap only: no schema marking exists at this bit
        if not covering:
            uncovered_mask |= 1 << bit
            mismatches.append({
                "kind": KIND_MISSING_RESERVED_BIT,
                "revision": rev, "offset": offset, "bit": bit,
                "message": (
                    f"SFF-8636 byte {offset} bit {bit} is defined by the standard "
                    "but uncovered by any schema field"
                ),
            })
            bit_status = "mismatch"
        else:
            where = {"revision": rev, "offset": offset, "bit": bit}
            bit_status = _compare_all_covering(
                covering, expected, where, mismatches, rev, offset, bit
            )
            if schema_access == "unknown":
                unknown_mask |= 1 << bit

        compared_mask |= 1 << bit
        bits.append({
            "bit": bit,
            "field_name": covering[0].name if covering else None,
            "covering_fields": [f.name for f in covering],
            "covering_field_ids": [f.uid for f in covering],
            "schema_access": schema_access,
            "schema_change_source": (
                ("module" if schema_dyn else "host_or_neither") if schema_dyn is not None else None
            ),
            "standard_type": row.type_text or None,
            "access_permission": expected.get("access"),
            "read_effect": expected.get("read_effect"),
            "authority": expected.get("authority"),
            "inference": expected.get("inference"),
            # A known effect in this family is only ever set from a verified
            # convention citation (see the row expectation).
            "effect_established": expected.get("read_effect") not in (None, "unknown"),
            "status": bit_status,
        })

    # Acknowledging the startup interrupt must not hide an Unknown constituent:
    # byte 2 bits 3-7 have no read-effect authority and stay Unknown.
    if byte_effect is not None and byte_effect.get("effect") == EFFECT_ACK_STARTUP:
        for bit in unknown_constituents:
            rec = next((b for b in bits if b["bit"] == bit), None)  # bit indices are unique
            if rec is not None and rec["schema_access"] not in ("unknown", "uncovered", None):
                mismatches.append({
                    "kind": KIND_HIDDEN_UNKNOWN,
                    "revision": rev, "offset": offset, "bit": bit,
                    "message": (
                        f"Byte {offset} carries {EFFECT_ACK_STARTUP} while constituent bit "
                        f"{bit} is marked known; the standard leaves its read effect Unknown"
                    ),
                })

    standard_bit_mask = 0
    for r in rows:
        if r.bit_lo is not None:
            for b in range(r.bit_lo, (r.bit_hi if r.bit_hi is not None else r.bit_lo) + 1):
                standard_bit_mask |= 1 << b

    return {
        "kind": "byte",
        "revision": rev,
        "page": "lower-memory",
        "offset": offset,
        "standard_bit_mask": standard_bit_mask,
        "byte_scoped_effect": byte_effect,
        "unknown_constituents_required": unknown_constituents,
        "unknown_mask": unknown_mask,
        "uncovered_mask": uncovered_mask,
        "compared_mask": compared_mask,
        # Set by account_completeness (_qualify_byte).
        "whole_byte_qualified": None,
        "qualification": None,
        "bits": bits,
    }


def compare_sff8636(
    schemas_dir: Path,
    standards_dir: Path | None,
    authority_report: dict[str, Any] | None,
    specific_revision: str | None = None,
    fields: list[SchemaField] | None = None,
    load_errors: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Compare SFF-8636 schema markings against the declared revision."""
    index_data = _load_yaml(schemas_dir / "sff-8636" / "_index.yaml") or {}
    schema_declared = str((index_data.get("spec") or {}).get("revision", "")).strip()
    if not schema_declared and not specific_revision:
        raise InputError(
            "sff-8636 schema index declares no revision; the applicable standard cannot be resolved"
        )
    declared_rev = specific_revision or schema_declared
    mismatches_pre: list[dict[str, Any]] = []
    if specific_revision and schema_declared and specific_revision != schema_declared:
        # A caller-supplied revision must not launder the schema's own
        # declaration: the override is recorded as an error, and the 2.11
        # refusal below is still keyed on the schema-declared revision.
        mismatches_pre.append({
            "kind": KIND_BORROWING,
            "revision": schema_declared,
            "message": (
                f"Caller-specified revision {specific_revision!r} overrides the "
                f"schema-declared revision {schema_declared!r}; an override is never "
                "an unrecorded replacement of the applicable standard"
            ),
        })

    if fields is None:
        fields, load_errors = load_family_fields("sff-8636", schemas_dir)
    load_errors = list(load_errors or [])

    standard_files: list[dict[str, str]] = []
    mismatches: list[dict[str, Any]] = list(load_errors) + mismatches_pre
    revisions = [declared_rev]

    std = _std_for_revision("sff-8636", declared_rev, standards_dir, standard_files, mismatches)

    # The cached 2.12 text does not establish schema-advertised 2.11
    # applicability: no receipt may claim that comparison complete without
    # actual 2.11 authority or an explicitly reviewed revision migration.
    # Keyed on the schema-declared revision, so a caller override cannot
    # bypass the refusal.
    refused_rev = schema_declared or declared_rev
    if refused_rev == "2.11" and (std is None or declared_rev != schema_declared) \
            and standards_dir is not None:
        rev212 = STANDARD_FILENAMES.get(("sff-8636", "2.12"))
        if rev212 and (standards_dir / rev212).is_file():
            mismatches.append({
                "kind": KIND_CLAIM_211,
                "revision": "2.11",
                "message": (
                    "Schema declares revision 2.11 but only the Rev 2.12 parsed text exists; "
                    "the cached 2.12 text does not establish 2.11 applicability and no "
                    "revision migration has been reviewed"
                ),
            })

    inventory: list[dict[str, Any]] = []
    stds = {declared_rev: std}

    if std is not None:
        # Expanded bytes the parsed standard has no rows for: each records a
        # no-standard-authority error, and the schema fields touching them
        # are routed through the general field path so their markings are
        # compared (never silently dropped with the byte entry).
        bytes_without_rows = {
            offset for offset in SFF8636_EXPANDED_BYTES
            if not _dedupe_rows(_match_rows(std, "lower", offset, None))
        }
        expanded_fields = [
            f for f in fields
            if _page_ctx_of(f) == "lower"
            and any(
                f.byte_lo <= b <= f.byte_hi
                for b in SFF8636_EXPANDED_BYTES if b not in bytes_without_rows
            )
        ]
        # A field only partly inside the bit-expanded bytes is also compared
        # as a field entry, so its other bytes are never compared by nothing.
        general_fields = [
            f for f in fields
            if f not in expanded_fields
            or any(
                b not in SFF8636_EXPANDED_BYTES or b in bytes_without_rows
                for b in range(f.byte_lo, f.byte_hi + 1)
            )
        ]

        for field in fields:
            _record_marking_errors(field, mismatches)

        for offset in SFF8636_EXPANDED_BYTES:
            entry = _sff8636_expanded_byte_entry(
                std, declared_rev, authority_report, offset, expanded_fields, mismatches
            )
            if entry is not None:
                inventory.append(entry)

        for field in general_fields:
            inventory.append(_compare_field_entry(
                field, "sff-8636", revisions, stds, authority_report,
                _sff8636_row_expectation, mismatches, True,
            ))

    if not inventory and not any(m.get("kind") in (KIND_CLAIM_211, KIND_BORROWING) for m in mismatches):
        mismatches.append({
            "kind": KIND_EMPTY_COMPARISON,
            "message": "No SFF-8636 fields or bytes were compared",
        })

    return _finalize_comparison("sff-8636", schemas_dir, fields, load_errors, {
        "family": "sff-8636",
        "revisions_checked": revisions,
        "declared_revision": schema_declared or declared_rev,
        "revision_override": specific_revision if (specific_revision and specific_revision != schema_declared) else None,
        "standard_files": standard_files,
        "compared_inventory": inventory,
        "mismatches": mismatches,
    })


# ---------------------------------------------------------------------------
# SFF-8472 comparison
# ---------------------------------------------------------------------------

SFF8472_EXPANDED_BYTES = (110, 111, 112, 113, 116, 117)


def _sff8472_row_expectation(
    std: ParsedStandard,
    rev: str,
    row: StdRow,
    report: dict[str, Any] | None,
    mismatches: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Derive the expected marking for one SFF-8472 standard row.

    Expectations rest only on passages present in the parsed text: when a
    convention passage is missing, the expectation it would establish is not
    fabricated, and the passage error is recorded into ``mismatches``.
    """
    desc = norm_text(row.desc).lower()
    authority: dict[str, Any] = {
        "document": std.filename,
        "clause": f"{row.table_title or 'field table'} line {row.line}",
        "source_file": std.filename,
    }

    if row.is_reserved_name():
        return {
            "access": "unknown", "read_effect": "unknown", "inference": None,
            "authority": authority, "standard_type": row.type_text or None,
        }
    if "read/write bit" in desc:
        # Per-bit RW controls (e.g. Soft TX Disable Select, Soft Rate_Select).
        # The access rests on the row's own "Read/write bit" text.  A read
        # effect is recorded only for byte 110, where the S:8.10 passage
        # ("the bits remain readable and writable") is about these very bits
        # (3 and 6) - the same recorded inference as byte 110's state bits.
        # Elsewhere the read effect stays unestablished (None): never a
        # default "none", and it blocks whole-byte qualification.
        cit, err = _convention_citation("sff-8472", "enhanced_options", std, report, mismatches, rev)
        if err is None and cit is not None:
            authority = cit
            if row.byte_lo == 110:
                return {
                    "access": "rw", "read_effect": "none",
                    "inference": (
                        "byte 110 control bits remain readable (S:8.10); no primary clause "
                        "states a read effect, so a side-effect-free read is an inference"
                    ),
                    "authority": authority, "standard_type": row.type_text or None,
                }
        return {
            "access": "rw", "read_effect": None, "inference": None,
            "authority": authority, "standard_type": row.type_text or None,
        }
    if row.byte_lo in (112, 113, 116, 117):
        # S:9.9: the flags may be latched or nonlatched, implementation
        # specific - the read effect stays unknown.
        cit, err = _convention_citation("sff-8472", "latched_flags", std, report, mismatches, rev)
        if err is None and cit is not None:
            authority = cit
            return {
                "access": "ro", "read_effect": "unknown", "inference": None,
                "authority": authority, "standard_type": row.type_text or None,
            }
        # Without the S:9.9 passage neither readability nor the read effect
        # is established for these bytes: no fabricated expectation.
        return {
            "access": None, "read_effect": None, "inference": None,
            "authority": authority, "standard_type": row.type_text or None,
            "reason": "latched-flags passage not found; no RO/RW or read-effect authority",
        }
    if row.byte_lo == 110:
        cit, err = _convention_citation("sff-8472", "enhanced_options", std, report, mismatches, rev)
        if err is None and cit is not None:
            return {
                "access": "ro", "read_effect": "none",
                "inference": (
                    "byte 110 bits are state/control bits that remain readable; no primary "
                    "clause states a read effect, so a side-effect-free read is an inference "
                    "from the per-bit definitions and the enhanced-options prose"
                ),
                "authority": cit, "standard_type": row.type_text or None,
            }
        return {
            "access": None, "read_effect": None, "inference": None,
            "authority": authority, "standard_type": row.type_text or None,
            "reason": "enhanced-options passage not found; no RO/RW or read-effect authority",
        }
    # No RO/RW or read-effect authority in the parsed tables for this location.
    return {
        "access": None, "read_effect": None, "inference": None,
        "authority": authority, "standard_type": row.type_text or None,
        "reason": "no RO/RW or read-effect authority in the parsed tables for this location",
    }


def _sff8472_expanded_byte_entry(
    std: ParsedStandard,
    rev: str,
    report: dict[str, Any] | None,
    offset: int,
    schema_fields: list[SchemaField],
    mismatches: list[dict[str, Any]],
) -> dict[str, Any] | None:
    rows = _dedupe_rows(_match_rows(std, "a2h", offset, None))
    if not rows:
        # A mandated expanded byte with no standard rows is an error, never a
        # silently dropped comparison: the schema fields touching it are
        # compared as field entries by the caller.
        mismatches.append({
            "kind": KIND_NO_STANDARD_AUTHORITY,
            "revision": rev,
            "offset": offset,
            "message": (
                f"SFF-8472 A2h byte {offset} is a mandated comparison location but the "
                "parsed standard yields no rows for it; its schema markings have no "
                "standard authority"
            ),
        })
        return None

    bits: list[dict[str, Any]] = []
    unknown_constituents: list[int] = []
    unknown_mask = 0
    uncovered_mask = 0
    compared_mask = 0

    for bit in range(8):
        row = _select_bit_row(
            rows, bit, mismatches, {"revision": rev, "offset": offset}
        )
        if row is None:
            # No standard row defines this bit; a schema marking there would
            # be compared by nothing: an error.
            covering = _covering_fields(schema_fields, offset, bit, rev)
            if covering:
                mismatches.append({
                    "kind": KIND_NO_STANDARD_AUTHORITY,
                    "revision": rev, "offset": offset, "bit": bit,
                    "fields": [f.name for f in covering],
                    "message": (
                        f"Schema marks SFF-8472 A2h byte {offset} bit {bit} but the "
                        "standard's tables define no row for it; the marking has no "
                        "standard authority"
                    ),
                })
            continue  # not appended: an uncompared bit blocks qualification
        # A bit is only "defined at bit level" when a bit-scoped row covers
        # it; whole-byte overview rows define the byte, not each bit, so an
        # uncovered bit under byte-level coverage is a byte gap (no schema
        # marking exists there to compare), not a missing-bit comparison
        # error - but it still blocks whole-byte qualification.
        bit_defined = any(
            r.bit_lo is not None
            and r.bit_lo <= bit <= (r.bit_hi if r.bit_hi is not None else r.bit_lo)
            for r in rows
        )
        expected = _sff8472_row_expectation(std, rev, row, report, mismatches)
        covering = _covering_fields(schema_fields, offset, bit, rev)
        # covering[0] only labels the record: every covering field is
        # compared below and a disagreeing overlap is its own error.
        schema_access = covering[0].access if covering else "uncovered"
        schema_dyn = covering[0].is_dynamic if covering else None

        if expected.get("read_effect") == "unknown":
            unknown_constituents.append(bit)

        bit_status = "conformant"
        if not covering and not bit_defined:
            continue  # byte-level gap only: no schema marking exists at this bit
        if not covering:
            uncovered_mask |= 1 << bit
            mismatches.append({
                "kind": KIND_MISSING_RESERVED_BIT,
                "revision": rev, "offset": offset, "bit": bit,
                "message": (
                    f"SFF-8472 A2h byte {offset} bit {bit} is defined by the standard "
                    "but uncovered by any schema field"
                ),
            })
            bit_status = "mismatch"
        else:
            where = {"revision": rev, "offset": offset, "bit": bit}
            bit_status = _compare_all_covering(
                covering, expected, where, mismatches, rev, offset, bit
            )
            if schema_access == "unknown":
                unknown_mask |= 1 << bit

        compared_mask |= 1 << bit
        bits.append({
            "bit": bit,
            "field_name": covering[0].name if covering else None,
            "covering_fields": [f.name for f in covering],
            "covering_field_ids": [f.uid for f in covering],
            "schema_access": schema_access,
            "schema_change_source": (
                ("module" if schema_dyn else "host_or_neither") if schema_dyn is not None else None
            ),
            "standard_type": row.type_text or None,
            "access_permission": expected.get("access"),
            "read_effect": expected.get("read_effect"),
            "authority": expected.get("authority"),
            "inference": expected.get("inference"),
            # A known effect in this family is only ever set from a verified
            # convention citation (see the row expectation).
            "effect_established": expected.get("read_effect") not in (None, "unknown"),
            "status": bit_status,
        })

    # Mixed-access expansion: any expanded byte that defines RW bits is not
    # generated bit coverage by a whole-byte schema field.
    covering_whole = [
        f for f in schema_fields
        if f.byte_lo <= offset <= f.byte_hi and f.bit_lo is None
    ]
    rw_bits = [b["bit"] for b in bits if b["access_permission"] == "rw"]
    if covering_whole and rw_bits:
        mismatches.append({
            "kind": KIND_MIXED_ACCESS,
            "revision": rev,
            "offset": offset,
            "message": (
                f"Schema models A2h byte {offset} as a whole-byte field but the standard "
                f"defines mixed access (RW bits {rw_bits}); per-bit expansion is required"
            ),
        })

    standard_bit_mask = 0
    for r in rows:
        if r.bit_lo is not None:
            for b in range(r.bit_lo, (r.bit_hi if r.bit_hi is not None else r.bit_lo) + 1):
                standard_bit_mask |= 1 << b

    # Byte-scoped effect: derived from the standard, never an unmeasured
    # literal.  The latched-flag bytes carry the S:9.9 implementation-
    # specific statement; byte 110's side-effect-free read is the recorded
    # enhanced-options inference; a byte whose constituents are unknown, or
    # whose bits were never compared, never asserts "no effect".
    if offset in (112, 113, 116, 117):
        cit, err = _convention_citation(
            "sff-8472", "latched_flags", std, report, mismatches, rev
        )
        if err is None and cit is not None:
            byte_effect = {
                "effect": "unknown", "scope": "byte", "authority": cit,
                "note": "S:9.9: flag latching is implementation specific; the "
                        "whole-byte read effect is unknown",
            }
        else:
            byte_effect = {
                "effect": "unknown", "scope": "byte",
                "authority": {
                    "document": std.filename, "source_file": std.filename,
                    "note": "latched-flags passage not found in the parsed text",
                },
            }
    elif len(bits) == 8 and all(
        b.get("read_effect") == "none"
        and isinstance(b.get("authority"), dict) and b["authority"].get("source_file")
        for b in bits
    ):
        # All eight constituent bits carry a "none" read effect, each with a
        # cited standard source: only then is "none" recorded for the byte.
        # The byte record cites the first; each bit keeps its own.
        byte_effect = {
            "effect": "none", "scope": "byte",
            "authority": bits[0]["authority"],
        }
        inference = next((b.get("inference") for b in bits if b.get("inference")), None)
        if inference:
            byte_effect["inference"] = inference
    else:
        # A missing, uncompared, Unknown or unestablished (None) constituent
        # read effect: the byte effect stays unknown, never a default "none".
        byte_effect = {
            "effect": "unknown", "scope": "byte",
            "note": (
                "no constituent bit was compared" if not bits else
                "not every constituent bit has an established read effect"
            ),
        }
        if bits:
            byte_effect["authority"] = next(
                (b["authority"] for b in bits if b.get("authority")), None
            )

    return {
        "kind": "byte",
        "revision": rev,
        "page": "page-a2h-diagnostics-base",
        "offset": offset,
        "standard_bit_mask": standard_bit_mask,
        "byte_scoped_effect": byte_effect,
        "unknown_constituents_required": unknown_constituents,
        "unknown_mask": unknown_mask,
        "uncovered_mask": uncovered_mask,
        "compared_mask": compared_mask,
        # Set by account_completeness (_qualify_byte).
        "whole_byte_qualified": None,
        "qualification": None,
        "bits": bits,
    }


def compare_sff8472(
    schemas_dir: Path,
    standards_dir: Path | None,
    authority_report: dict[str, Any] | None,
    fields: list[SchemaField] | None = None,
    load_errors: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Compare SFF-8472 schema markings against Rev 12.5a."""
    rev = "12.5a"
    if fields is None:
        fields, load_errors = load_family_fields("sff-8472", schemas_dir)
    load_errors = list(load_errors or [])

    standard_files: list[dict[str, str]] = []
    mismatches: list[dict[str, Any]] = list(load_errors)
    std = _std_for_revision("sff-8472", rev, standards_dir, standard_files, mismatches)
    stds = {rev: std}

    inventory: list[dict[str, Any]] = []

    if std is not None:
        expanded_fields = [
            f for f in fields
            if _page_ctx_of(f) == "a2h"
            and any(f.byte_lo <= b <= f.byte_hi for b in SFF8472_EXPANDED_BYTES)
        ]

        for field in fields:
            _record_marking_errors(field, mismatches)

        for offset in SFF8472_EXPANDED_BYTES:
            entry = _sff8472_expanded_byte_entry(
                std, rev, authority_report, offset, expanded_fields, mismatches
            )
            if entry is not None:
                inventory.append(entry)

        # Every field - expanded-byte or not - is also compared as a field
        # entry, so the expanded A2h fields' markings are compared on every
        # axis the other families compare (access and effect, per-revision
        # citations, authority) and the receipt validator's field rules apply
        # to them; the bit-expanded byte entries are the read-readiness view.
        for field in fields:
            inventory.append(_compare_field_entry(
                field, "sff-8472", [rev], stds, authority_report,
                _sff8472_row_expectation, mismatches, True,
            ))

    if not inventory:
        mismatches.append({
            "kind": KIND_EMPTY_COMPARISON,
            "message": "No SFF-8472 fields or bytes were compared",
        })

    return _finalize_comparison("sff-8472", schemas_dir, fields, load_errors, {
        "family": "sff-8472",
        "revisions_checked": [rev],
        "declared_revision": rev,
        "standard_files": standard_files,
        "compared_inventory": inventory,
        "mismatches": mismatches,
    })


# ---------------------------------------------------------------------------
# Completeness accounting: the one place a comparison becomes a passing result
# ---------------------------------------------------------------------------

# Per family: the page context and offsets the comparators bit-expand.
EXPANDED_BYTES = {
    "cmis": ("lower", CMIS_EXPANDED_BYTES),
    "sff-8636": ("lower", SFF8636_EXPANDED_BYTES),
    "sff-8472": ("a2h", SFF8472_EXPANDED_BYTES),
}

# Bit-record statuses that mean "compared" (a mismatch is already an error).
_COMPARED = ("conformant", "mismatch")


def byte_qualification_blockers(entry: dict[str, Any], mismatches: list[dict[str, Any]]) -> list[str]:
    """Why a byte entry may not be qualified for a whole-byte host read.

    Invariant 5: all eight constituent bits present and compared, each with
    an established access permission and an established, compatible read
    effect (none Unknown), every constituent conformant, no Unknown schema
    marking, no recorded mismatch on the byte, and a byte-scoped effect that
    is not the startup acknowledgment.  An empty list means qualified.
    """
    reasons: list[str] = []
    bits = [b for b in entry.get("bits") or [] if isinstance(b, dict)]
    seen = {b.get("bit") for b in bits}
    if seen != set(range(8)):
        reasons.append(f"uncompared constituent bits {sorted(set(range(8)) - seen)}")
    for b in bits:
        n = b.get("bit")
        if b.get("status") != "conformant":
            reasons.append(f"bit {n} comparison status is {b.get('status')!r}")
        if b.get("schema_access") in (None, "uncovered", "unknown"):
            reasons.append(f"bit {n} schema marking is {b.get('schema_access')!r}")
        if b.get("access_permission") in (None, "unknown"):
            reasons.append(f"bit {n} has no established access permission")
        if b.get("read_effect") in (None, "unknown") or not b.get("effect_established"):
            reasons.append(f"bit {n} has no established read effect")
        auth = b.get("authority")
        if not (isinstance(auth, dict) and auth.get("source_file")):
            reasons.append(f"bit {n} cites no standard source")
    effect = entry.get("byte_scoped_effect")
    if isinstance(effect, dict):
        if effect.get("effect") == EFFECT_ACK_STARTUP:
            reasons.append("the byte carries the startup acknowledgment")
        elif effect.get("effect") in (None, "unknown"):
            reasons.append("the byte-scoped read effect is unknown")
    if entry.get("uncovered_mask") or entry.get("unknown_mask"):
        reasons.append("the byte has uncovered or Unknown constituent masks")
    rev, off = entry.get("revision"), entry.get("offset")
    if any(m.get("revision") == rev and m.get("offset") == off for m in mismatches):
        reasons.append("a mismatch is recorded on the byte")
    return reasons


def _qualify_byte(entry: dict[str, Any], mismatches: list[dict[str, Any]]) -> None:
    """Set the whole-byte qualification of one byte entry (invariant 5)."""
    blockers = byte_qualification_blockers(entry, mismatches)
    qualified = not blockers
    inference = qualified and any(b.get("inference") for b in entry.get("bits") or [])
    entry["whole_byte_qualified"] = qualified
    entry["qualification"] = {
        "readiness": (
            "qualified_with_inference" if inference
            else "qualified" if qualified else "blocked"
        ),
        "requester": "host_read",
        "reason": (
            "all eight constituent bits compared and conformant, each with an "
            "established access permission and read effect"
            if qualified else
            "; ".join(dict.fromkeys(blockers)) + "; an honestly Unknown marking "
            "conforms but cannot establish safe readiness"
        ),
    }


def account_completeness(
    family: str,
    schemas_dir: Path,
    fields: list[SchemaField],
    load_errors: list[dict[str, Any]],
    res: dict[str, Any],
) -> list[dict[str, Any]]:
    """Completeness accounting (round 4): the receipt passes only if it holds.

    Every invariant is checked here, once, against the compared inventory -
    not at the individual skip sites - and every violation is returned as an
    error that the caller adds before the status is set:

    1. Input accounting: every field entry the hashed schema closure declares
       is loaded or reported by a load error, nothing is loaded that the
       closure does not declare, unsupported forms (nested references,
       per-revision overrides) fail explicitly, every loaded occurrence
       reaches the inventory, and at least one field is compared.
    2. Applicability: a field is excluded at a revision only by its own
       since/until; otherwise each (field, revision) cell must be compared,
       or it is unresolved_at_applicable_revision.
    3. No default authority: a known read effect (bit or byte) cites a
       standard source; a byte "none" effect needs eight established "none"
       constituents.
    4. No order-dependent resolution: conflicting template definitions,
       byte-scoped effects or overlapping schema markings are errors.
    5. Whole-byte qualification is decided here (_qualify_byte).
    """
    violations: list[dict[str, Any]] = []
    mismatches = res.get("mismatches") or []
    inventory = res.get("compared_inventory") or []
    revisions = list(res.get("revisions_checked") or [])

    # -- 1. input accounting ---------------------------------------------
    decls = schema_declarations(family, schemas_dir)
    violations.extend(decls["errors"])
    declared = decls["declared"]
    reported_decl = {e.get("decl") for e in load_errors if e.get("decl")}
    reported_file = {e.get("decl_file") for e in load_errors if e.get("decl_file")}
    loaded_decl = {f.decl for f in fields}
    for key, d in sorted(declared.items()):
        if key in loaded_decl or key in reported_decl or d["file"] in reported_file \
                or any(pg in reported_file for pg in d["pages"]):
            continue
        violations.append({
            "kind": KIND_UNACCOUNTED_INPUT, "decl": key, "field": d.get("name"),
            "message": (
                f"Schema entry {key} ({d.get('name')!r}) is declared in the hashed closure "
                "but was neither loaded nor reported; it would never be compared"
            ),
        })
    for key in sorted(loaded_decl - set(declared)):
        violations.append({
            "kind": KIND_UNACCOUNTED_INPUT, "decl": key,
            "message": f"Loaded field {key} is not declared by the hashed schema closure",
        })
    # Occurrence identity must be injective: a page listed twice in the
    # index includes (or a fragment twice in one page's field_includes)
    # loads the same occurrence twice under one uid, which the set-based
    # checks above cannot see.  A duplicated include is an error.
    uid_count: dict[str, int] = {}
    for f in fields:
        uid_count[f.uid] = uid_count.get(f.uid, 0) + 1
    for uid in sorted(u for u, n in uid_count.items() if n > 1):
        f = next(x for x in fields if x.uid == uid)
        violations.append({
            "kind": KIND_UNACCOUNTED_INPUT, "decl": f.decl, "field_id": uid,
            "field": f.name, "occurrences": uid_count[uid],
            "message": (
                f"Field occurrence {uid} ({f.name!r}) is loaded {uid_count[uid]} times: "
                "its page or fragment is included more than once; a duplicated include "
                "is an unsupported schema form"
            ),
        })
    for key in sorted({f.decl for f in fields if f.overrides}):
        f = next(x for x in fields if x.decl == key)
        violations.append({
            "kind": KIND_UNACCOUNTED_INPUT, "decl": key, "field": f.name,
            "message": (
                f"Field {f.name!r} carries per-revision overrides {f.overrides}; this tool "
                "does not resolve them (unsupported schema form), so its marking cannot be "
                "compared per revision"
            ),
        })

    # -- 4. order-dependent resolution: templates ---------------------------
    for name, defs in sorted(template_definitions(family, schemas_dir).items()):
        bodies = {json.dumps(d["body"], sort_keys=True, default=str) for d in defs}
        if len(bodies) > 1:
            violations.append({
                "kind": KIND_CONFLICTING_AUTHORITY, "template": name,
                "sources": [d["source"] for d in defs],
                "message": (
                    f"Template {name!r} is defined differently in {[d['source'] for d in defs]}; "
                    "the inherited marking would be resolved by import order"
                ),
            })

    # -- inventory indexes -------------------------------------------------
    field_entries = {e.get("field_id"): e for e in inventory if e.get("kind") == "field"}
    bytes_by_key = {
        (e.get("revision"), e.get("offset")): e for e in inventory if e.get("kind") == "byte"
    }
    exp_page, exp_offsets = EXPANDED_BYTES[family]
    # Revisions whose whole standard file is missing (not a single byte's
    # rows, which carry an offset): every cell there is already reported.
    missing_std_revs = {
        m.get("revision") for m in mismatches
        if m.get("kind") == KIND_MISSING_STANDARD and m.get("offset") is None
    }
    # SFF-8636 comparisons refused outright (2.11 from 2.12 text, an override
    # of the declared revision): every cell there is already reported.
    refused = family == "sff-8636" and any(
        m.get("kind") in (KIND_CLAIM_211, KIND_BORROWING) for m in mismatches
    )

    # -- 1/2. per-field, per-revision cell accounting -------------------------
    compared_fields = 0
    for f in fields:
        if f.decl in reported_decl:
            continue  # its own load error is the record
        field_compared = False
        for rev in revisions:
            if not _applies_at(f, rev):
                continue  # excluded by the field's own since/until
            parts: list[str] = []
            e = field_entries.get(f.uid)
            if e is not None:
                parts.append(((e.get("expected") or {}).get(rev) or {}).get("status") or "absent")
            if _page_ctx_of(f) == exp_page:
                for off in range(f.byte_lo, f.byte_hi + 1):
                    if off not in exp_offsets:
                        continue
                    be = bytes_by_key.get((rev, off))
                    for bit in range(8):
                        if not (f.bit_mask >> bit) & 1:
                            continue
                        rec = next(
                            (b for b in (be or {}).get("bits") or []
                             if b.get("bit") == bit and f.uid in (b.get("covering_field_ids") or [])),
                            None,
                        )
                        if rec is not None:
                            # A bit compared against a row that carries no
                            # access authority is unresolved, not compared.
                            parts.append(
                                "unresolved" if rec.get("access_permission") is None
                                else rec.get("status") or "absent"
                            )
                        elif e is None:
                            parts.append("absent")
            if parts and all(p in _COMPARED for p in parts):
                field_compared = True
                continue
            if rev in missing_std_revs or refused:
                continue  # the revision-level error is the record for every cell
            if any(p in _COMPARED for p in parts):
                field_compared = True
            violations.append({
                "kind": KIND_UNRESOLVED_AT_REVISION,
                "field": f.name, "field_id": f.uid, "page": f.page_id,
                "offset": f.byte_lo, "revision": rev,
                "message": (
                    f"Field {f.name!r} applies at {rev} (no since/until excludes it) but its "
                    f"marking was not completely compared there ({sorted(set(parts)) or 'not in the inventory'}); "
                    "unresolved authority is never a successful conformance claim"
                ),
            })
        if field_compared:
            compared_fields += 1
    all_revisions_reported = refused or (revisions and set(revisions) <= missing_std_revs)
    if not compared_fields and not all_revisions_reported:
        # (When every revision's comparison is already refused or missing,
        # that revision-level error is the record.)
        violations.append({
            "kind": KIND_EMPTY_COMPARISON,
            "message": f"No {family} schema field was compared against an applicable standard",
        })

    # -- 4. overlapping schema markings anywhere in the map ----------------
    already = {
        (m.get("revision"), m.get("offset"), m.get("bit"))
        for m in mismatches if m.get("kind") == KIND_OVERLAPPING_FIELDS
    }
    cover: dict[tuple[str, str, int, int], list[SchemaField]] = {}
    for f in fields:
        for rev in revisions:
            if not _applies_at(f, rev):
                continue
            for off in range(f.byte_lo, f.byte_hi + 1):
                for bit in range(8):
                    if (f.bit_mask >> bit) & 1:
                        cover.setdefault((rev, _page_ctx_of(f), off, bit), []).append(f)
    for (rev, page, off, bit), fs in sorted(cover.items()):
        if len({(f.access, f.is_dynamic) for f in fs}) > 1 and (rev, off, bit) not in already:
            violations.append({
                "kind": KIND_OVERLAPPING_FIELDS, "revision": rev, "page": page,
                "offset": off, "bit": bit, "fields": [f.name for f in fs],
                "message": (
                    f"Schema fields {[f.name for f in fs]} overlap at {page} byte {off} bit "
                    f"{bit} ({rev}) with disagreeing volatility markings"
                ),
            })

    # -- 3/4/5. byte entries: authority, effects, qualification ------------
    for be in (e for e in inventory if e.get("kind") == "byte"):
        rev, off = be.get("revision"), be.get("offset")
        cands = be.get("byte_effect_candidates") or []
        if len({(c.get("effect"), c.get("scope")) for c in cands}) > 1:
            violations.append({
                "kind": KIND_CONFLICTING_AUTHORITY, "revision": rev, "offset": off,
                "message": (
                    f"Byte {off} ({rev}) constituent rows state conflicting byte-scoped "
                    f"effects {sorted({(c.get('effect'), c.get('scope')) for c in cands})}"
                ),
            })
        for b in be.get("bits") or []:
            effect = b.get("read_effect")
            auth = b.get("authority")
            if effect not in (None, "unknown") and not (
                isinstance(auth, dict) and auth.get("source_file")
            ):
                violations.append({
                    "kind": KIND_UNESTABLISHED_AUTHORITY, "revision": rev, "offset": off,
                    "bit": b.get("bit"),
                    "message": (
                        f"Byte {off} bit {b.get('bit')} ({rev}) records read effect {effect!r} "
                        "that no cited standard passage establishes"
                    ),
                })
        effect = be.get("byte_scoped_effect")
        if isinstance(effect, dict) and effect.get("effect") == "none":
            bits = be.get("bits") or []
            if len(bits) != 8 or not all(
                b.get("read_effect") == "none" and b.get("effect_established") for b in bits
            ):
                violations.append({
                    "kind": KIND_UNESTABLISHED_AUTHORITY, "revision": rev, "offset": off,
                    "message": (
                        f"Byte {off} ({rev}) records byte-scoped effect 'none' without eight "
                        "constituent bits whose 'none' read effect is established"
                    ),
                })
    for be in (e for e in inventory if e.get("kind") == "byte"):
        _qualify_byte(be, list(mismatches) + violations)

    return violations


def _finalize_comparison(
    family: str,
    schemas_dir: Path,
    fields: list[SchemaField],
    load_errors: list[dict[str, Any]],
    res: dict[str, Any],
) -> dict[str, Any]:
    """Run the completeness accounting, then set the status from it."""
    res["mismatches"].extend(account_completeness(family, schemas_dir, fields, load_errors, res))
    res["status"] = "pass" if not res["mismatches"] else "fail"
    return res


# ---------------------------------------------------------------------------
# Receipt-level validation rules (data-driven, any family/revision)
# ---------------------------------------------------------------------------


def validate_conformance_inventory(family: str, receipt: dict[str, Any]) -> list[dict[str, Any]]:
    """Validate a receipt against the D1 conformance rules.

    These rules work on receipt content, so they catch receipts that cite
    another revision's authority, claim a 2.11 comparison from 2.12-only
    input, drop required bits or hide Unknown constituents - for any family
    and any revision, not only the cases the comparators emit today.
    """
    violations: list[dict[str, Any]] = []
    inv = receipt.get("compared_inventory") or []
    revisions = list(receipt.get("revisions_checked") or [])
    standard_files = list(receipt.get("standard_files") or [])
    std_revs = {sf.get("revision") for sf in standard_files}

    if not inv:
        violations.append({
            "kind": KIND_EMPTY_COMPARISON,
            "message": f"Empty comparison for family '{family}': no fields or bytes evaluated",
        })
        return violations

    # Every applicable revision must be checked with its own standard file.
    # The applicable set is derived from the family's own revision map (CMIS
    # and SFF-8472) or the receipt's schema-declared revision (SFF-8636),
    # never from the receipt's own revisions_checked, which would be
    # self-referential.
    declared = receipt.get("declared_revision")
    if family == "cmis":
        applicable = list(CMIS_REVISIONS)
    elif family == "sff-8472":
        applicable = list(SFF8472_REVISIONS)
    else:
        applicable = [declared] if declared else list(revisions)
    for rev in applicable:
        if rev not in revisions:
            violations.append({
                "kind": KIND_MISSING_STANDARD,
                "revision": rev,
                "message": f"Receipt does not record a comparison for applicable revision {rev}",
            })
        elif rev not in std_revs:
            violations.append({
                "kind": KIND_MISSING_STANDARD,
                "revision": rev,
                "message": f"Receipt records no standard file for applicable revision {rev}",
            })

    # A receipt that declares one revision while recording a comparison for
    # another launders the applicability: the declared revision must match
    # the checked one.
    if family == "sff-8636" and declared and revisions and list(revisions) != [declared]:
        violations.append({
            "kind": KIND_BORROWING,
            "revision": declared,
            "message": (
                f"Receipt declares revision {declared!r} but records a comparison for "
                f"{revisions!r}; an override is never an unrecorded replacement of the "
                "applicable standard"
            ),
        })

    # SFF-8636: never claim a 2.11 comparison from 2.12-only input.
    if family == "sff-8636" and "2.11" in revisions and "2.11" not in std_revs:
        violations.append({
            "kind": KIND_CLAIM_211,
            "revision": "2.11",
            "message": (
                "Receipt claims a 2.11 comparison without 2.11 standard authority; the cached "
                "2.12 text does not establish 2.11 applicability"
            ),
        })

    def check_authority(
        rec_revision: str | None, auth: Any, where: dict[str, Any], effect: Any = None
    ) -> None:
        source_file = auth.get("source_file") if isinstance(auth, dict) else None
        if not source_file:
            # A known effect (anything but unknown/absent) must cite a parsed
            # standard file; an unknown effect asserts nothing, so a missing
            # citation there loses no authority.
            if effect not in (None, "unknown"):
                violations.append({
                    "kind": KIND_NO_STANDARD_AUTHORITY,
                    "revision": rec_revision,
                    **where,
                    "message": (
                        f"Revision {rec_revision} record asserts effect {effect!r} "
                        "without citing a parsed standard file"
                    ),
                })
            return
        owners = FILE_TO_REVISIONS.get(str(source_file), [])
        if not owners:
            # Fail closed: an authority citing a file outside the parsed-
            # standard map (e.g. a derived or fabricated filename) belongs to
            # no revision and would bypass the borrowing rule entirely.
            violations.append({
                "kind": KIND_BORROWING,
                "revision": rec_revision,
                "source_file": source_file,
                **where,
                "message": (
                    f"Revision {rec_revision} record cites authority from {source_file}, "
                    "which is not an applicable parsed-standard file"
                ),
            })
        elif (family, rec_revision) not in owners:
            violations.append({
                "kind": KIND_BORROWING,
                "revision": rec_revision,
                "source_file": source_file,
                **where,
                "message": (
                    f"Revision {rec_revision} record cites authority from {source_file}, "
                    "which belongs to " + ", ".join(f"{f}/{r}" for f, r in owners)
                ),
            })

    for byte_entry in inv:
        if byte_entry.get("kind") != "byte":
            continue
        rev = byte_entry.get("revision")
        offset = byte_entry.get("offset")
        bits = byte_entry.get("bits") or []
        seen_bits = {b.get("bit") for b in bits if isinstance(b, dict) and "bit" in b}
        standard_mask = byte_entry.get("standard_bit_mask")
        required = (
            {b for b in range(8) if standard_mask & (1 << b)}
            if isinstance(standard_mask, int)
            else set(range(8))
        )
        missing_bits = required - seen_bits
        if missing_bits:
            violations.append({
                "kind": KIND_MISSING_RESERVED_BIT,
                "revision": rev,
                "offset": offset,
                "message": (
                    f"Byte at offset {offset} is missing bits {sorted(missing_bits)} "
                    "from the bit inventory"
                ),
            })

        for bit_rec in bits:
            if isinstance(bit_rec, dict):
                check_authority(
                    rev, bit_rec.get("authority") or {},
                    {"offset": offset, "bit": bit_rec.get("bit")},
                    bit_rec.get("read_effect"),
                )

        # Whole-byte qualification requires all eight bits of the physical
        # byte to be present and compared: a receipt claiming qualification
        # without a complete bit inventory is internally contradictory.
        if byte_entry.get("whole_byte_qualified") and len(seen_bits) != 8:
            violations.append({
                "kind": KIND_MISSING_RESERVED_BIT,
                "revision": rev,
                "offset": offset,
                "message": (
                    f"Byte at offset {offset} claims whole_byte_qualified with only "
                    f"{len(seen_bits)} of 8 constituent bits in the inventory"
                ),
            })

        # A qualification claim must be backed by its evidence (invariant 5):
        # re-derived from the recorded bits, so an edited or stale receipt
        # cannot claim it.
        if byte_entry.get("whole_byte_qualified"):
            blockers = byte_qualification_blockers(
                byte_entry,
                [m for m in receipt.get("mismatches") or [] if isinstance(m, dict)],
            )
            if blockers:
                violations.append({
                    "kind": KIND_UNJUSTIFIED_QUALIFICATION,
                    "revision": rev,
                    "offset": offset,
                    "message": (
                        f"Byte at offset {offset} claims whole_byte_qualified but "
                        + "; ".join(dict.fromkeys(blockers))
                    ),
                })

        # SFF-8472 expanded bytes carry a derived byte-scoped effect, never an
        # unmeasured literal "no effect" claim.
        if family == "sff-8472" and not byte_entry.get("byte_scoped_effect"):
            violations.append({
                "kind": KIND_OMITTED_BYTE_EFFECT,
                "revision": rev,
                "offset": offset,
                "message": (
                    f"SFF-8472 A2h byte {offset} omitted its byte-scoped effect record; "
                    "the entry must carry the standard-derived effect (unknown where the "
                    "standard leaves it implementation specific), not an unmeasured null"
                ),
            })

        effect = byte_entry.get("byte_scoped_effect") or {}
        if isinstance(effect, dict):
            check_authority(
                rev, effect.get("authority") or {}, {"offset": offset}, effect.get("effect")
            )
            effect_name = effect.get("effect")
            if effect_name == EFFECT_ACK_STARTUP:
                required_unknown = set(byte_entry.get("unknown_constituents_required") or [])
                for bit_rec in bits:
                    if not isinstance(bit_rec, dict):
                        continue
                    b = bit_rec.get("bit")
                    if b in required_unknown and bit_rec.get("read_effect") not in ("unknown", None):
                        if bit_rec.get("schema_access") not in ("unknown", "uncovered", None):
                            violations.append({
                                "kind": KIND_HIDDEN_UNKNOWN,
                                "revision": rev,
                                "offset": offset,
                                "bit": b,
                                "message": (
                                    f"Byte {offset} acknowledges the startup interrupt while "
                                    f"hiding Unknown constituent bit {b}"
                                ),
                            })
            if family == "sff-8636" and offset == 2 and effect_name != EFFECT_ACK_STARTUP:
                violations.append({
                    "kind": KIND_OMITTED_BYTE_EFFECT,
                    "revision": rev,
                    "offset": offset,
                    "message": (
                        "SFF-8636 byte 2 omitted the AcknowledgeStartupInterrupt "
                        "byte-scoped effect"
                    ),
                })

    for field_entry in inv:
        if field_entry.get("kind") != "field":
            continue
        expected = field_entry.get("expected") or {}
        if isinstance(expected, dict):
            for rev, exp in expected.items():
                if isinstance(exp, dict):
                    check_authority(
                        rev, exp.get("authority"),
                        {"field": field_entry.get("field_name")},
                    )
        if field_entry.get("status") in ("no_authority", "marking_error"):
            violations.append({
                "kind": (
                    KIND_NO_STANDARD_AUTHORITY if field_entry.get("status") == "no_authority"
                    else KIND_SKIPPED_FIELD
                ),
                "field": field_entry.get("field_name"),
                "message": (
                    f"Field entry for {field_entry.get('field_name')!r} has status "
                    f"{field_entry.get('status')!r}; a marked field is never silently skipped"
                ),
            })

    return violations


# ---------------------------------------------------------------------------
# Receipts
# ---------------------------------------------------------------------------


def compute_receipt_hash(receipt: dict[str, Any]) -> str:
    """Deterministic hash over every distinguishing receipt field.

    Covers the family, checked revisions, standard-file identities, authority
    report identities, the schema closure (hash and file list), the complete
    compared inventory (access permissions, change sources, read effects,
    Unknown/uncovered masks, qualifications, byte-scoped effects, citations,
    inferences, revision sources and all expanded bit markings) and every
    recorded mismatch.
    """
    canonical = {
        "family": receipt.get("family"),
        "revisions_checked": sorted(list(receipt.get("revisions_checked") or [])),
        "declared_revision": receipt.get("declared_revision"),
        "revision_override": receipt.get("revision_override"),
        "standard_files": sorted(
            list(receipt.get("standard_files") or []),
            key=lambda x: json.dumps(x, sort_keys=True),
        ),
        "authority_reports": receipt.get("authority_reports") or {},
        "schema_closure": receipt.get("schema_closure") or {},
        "compared_inventory": receipt.get("compared_inventory") or [],
        "mismatches": receipt.get("mismatches") or [],
    }
    encoded = json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def build_receipt(
    family: str,
    schemas_dir: Path,
    standards_dir: Path | None,
    authority_dir: Path | None,
    repo_root: Path | None = None,
) -> dict[str, Any]:
    """Build the full conformance receipt for one family (fail closed)."""
    try:
        closure_hash, closure_files = compute_schema_closure(family, schemas_dir, repo_root)
        closure: dict[str, Any] = {"hash": closure_hash, "files": closure_files}
    except Exception as e:  # noqa: BLE001 - recorded, never silent
        # Unresolved closure input: emit an honest failing receipt without a
        # closure hash; --check fails it because the closure cannot be
        # recomputed to match.
        mismatches = [{
            "kind": KIND_UNRESOLVED_INPUT,
            "message": f"Schema input closure could not be resolved: {type(e).__name__}: {e}",
        }]
        receipt: dict[str, Any] = {
            "family": family,
            "revisions_checked": [],
            "declared_revision": None,
            "standard_files": [],
            "schema_closure": {"error": str(e)},
            "compared_inventory": [],
            "mismatches": mismatches,
            "status": "fail",
        }
        try:
            report = load_authority_report(authority_dir, family)
            receipt["authority_reports"] = {report["path"]: report["sha256"]}
        except InputError:
            receipt["authority_reports"] = {}
        receipt["receipt_hash"] = compute_receipt_hash(receipt)
        return receipt

    base_mismatch: dict[str, Any] | None = None
    try:
        report = load_authority_report(authority_dir, family)
        authority_reports = {report["path"]: report["sha256"]}
    except InputError as e:
        # The completed authority reports are required research; failing to
        # load them is an error, never a silent pass.
        report = None
        authority_reports = {}
        base_mismatch = {"kind": KIND_MISSING_AUTHORITY_REPORT, "message": str(e)}

    if family == "cmis":
        res = compare_cmis(schemas_dir, standards_dir, report)
    elif family == "sff-8636":
        res = compare_sff8636(schemas_dir, standards_dir, report)
    elif family == "sff-8472":
        res = compare_sff8472(schemas_dir, standards_dir, report)
    else:
        raise ValueError(f"Unsupported family: {family}")

    if base_mismatch is not None:
        res["mismatches"].insert(0, base_mismatch)

    violations = validate_conformance_inventory(family, res)
    if violations:
        res["mismatches"].extend(violations)
        res["status"] = "fail"

    receipt = {
        "family": family,
        "revisions_checked": res.get("revisions_checked") or [],
        "declared_revision": res.get("declared_revision"),
        "revision_override": res.get("revision_override"),
        "standard_files": res.get("standard_files") or [],
        "authority_reports": authority_reports,
        "schema_closure": closure,
        "compared_inventory": res.get("compared_inventory") or [],
        "mismatches": res.get("mismatches") or [],
        "status": "pass" if not res.get("mismatches") else "fail",
    }
    receipt["receipt_hash"] = compute_receipt_hash(receipt)
    return receipt


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _parse_families(spec: str) -> list[str]:
    families = [f.strip() for f in (spec or "").split(",") if f.strip()]
    if not families:
        print(
            "error: --family resolves to an empty scope; refusing to check nothing",
            file=sys.stderr,
        )
        raise SystemExit(2)
    for fam in families:
        if fam not in SUPPORTED_FAMILIES:
            print(
                f"error: unknown family {fam!r}; supported: {', '.join(SUPPORTED_FAMILIES)}",
                file=sys.stderr,
            )
            raise SystemExit(2)
    return families


def _mismatch_summary(mismatches: list[dict[str, Any]]) -> str:
    counts: dict[str, int] = {}
    for m in mismatches:
        counts[m.get("kind", "?")] = counts.get(m.get("kind", "?"), 0) + 1
    return ", ".join(f"{k}={v}" for k, v in sorted(counts.items())) or "none"


def cmd_write(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    schemas_dir = Path(args.schemas_dir).resolve()
    standards_dir = Path(args.standards_dir).resolve() if args.standards_dir else None
    receipts_dir = Path(args.receipts_dir).resolve()
    authority_dir = Path(args.authority_dir).resolve() if args.authority_dir else None

    families = _parse_families(args.family)
    receipts_dir.mkdir(parents=True, exist_ok=True)

    overall_exit = 0
    for fam in families:
        try:
            receipt = build_receipt(fam, schemas_dir, standards_dir, authority_dir, repo_root)
        except Exception as e:  # noqa: BLE001 - reported, never silent
            print(f"[{fam}] Error building receipt: {e}", file=sys.stderr)
            overall_exit = 1
            continue
        out_file = receipts_dir / f"{fam}.json"
        with open(out_file, "w", encoding="utf-8") as f:
            json.dump(receipt, f, indent=2, sort_keys=True)
        if receipt["status"] != "pass":
            print(
                f"[{fam}] Wrote FAILING receipt to {out_file}: "
                f"{len(receipt['mismatches'])} mismatch(es) "
                f"[{_mismatch_summary(receipt['mismatches'])}]",
                file=sys.stderr,
            )
            overall_exit = 1
        else:
            print(f"[{fam}] Wrote conformant receipt to {out_file}")
    return overall_exit


def _canonical(obj: Any) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), default=str)


def recheck_accounting(family: str, schemas_dir: Path, receipt: dict[str, Any]) -> list[dict[str, Any]]:
    """Re-run the completeness accounting against a committed receipt.

    ``--check`` judges a receipt without the parsed standards, so it cannot
    redo the comparison - but the accounting needs only the schemas (fresh,
    once the closure hash matches) and the receipt's own inventory and
    mismatches.  Every violation it derives must be recorded in the receipt;
    one that is missing means the receipt was edited (or written by a tool
    that skipped the accounting) and is returned.  It runs on a copy: the
    receipt is never mutated.
    """
    fields, load_errors = load_family_fields(family, Path(schemas_dir))
    res = json.loads(_canonical({
        "revisions_checked": receipt.get("revisions_checked") or [],
        "compared_inventory": receipt.get("compared_inventory") or [],
        "mismatches": [m for m in receipt.get("mismatches") or [] if isinstance(m, dict)],
    }))
    recorded = {_canonical(m) for m in res["mismatches"]}
    derived = list(load_errors) + account_completeness(
        family, Path(schemas_dir), fields, load_errors, res
    )
    return [v for v in derived if _canonical(json.loads(_canonical(v))) not in recorded]


def cmd_check(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    schemas_dir = Path(args.schemas_dir).resolve()
    receipts_dir = Path(args.receipts_dir).resolve()

    families = _parse_families(args.family)
    failures: list[str] = []

    for fam in families:
        receipt_path = receipts_dir / f"{fam}.json"
        if not receipt_path.is_file():
            failures.append(f"Missing receipt for '{fam}' at {receipt_path}")
            continue
        try:
            with open(receipt_path, "r", encoding="utf-8") as f:
                receipt = json.load(f)
        except Exception as e:  # noqa: BLE001
            failures.append(f"Invalid JSON in receipt '{receipt_path}': {e}")
            continue

        if receipt.get("family") != fam:
            failures.append(f"Receipt for '{fam}' records family {receipt.get('family')!r}")
            continue

        # 1. Receipt content hash integrity.
        expected_hash = compute_receipt_hash(receipt)
        if receipt.get("receipt_hash") != expected_hash:
            failures.append(
                f"Receipt for '{fam}' has an invalid receipt_hash (content was edited)"
            )
            continue

        # 2. Closure freshness: recompute membership and content hashes.
        try:
            cur_hash, cur_files = compute_schema_closure(fam, schemas_dir, repo_root)
        except Exception as e:  # noqa: BLE001
            failures.append(f"Failed to compute current schema closure for '{fam}': {e}")
            continue
        stored = receipt.get("schema_closure") or {}
        if stored.get("hash") != cur_hash or stored.get("files") != cur_files:
            failures.append(
                f"Receipt for '{fam}' is stale: schema closure changed "
                f"(receipt {str(stored.get('hash'))[:12]}, current {cur_hash[:12]})"
            )
            continue

        # 3. Receipt-level conformance rules.
        for v in validate_conformance_inventory(fam, receipt):
            failures.append(
                f"Receipt for '{fam}' violates {v.get('kind')}: {v.get('message')}"
            )

        # 3b. Completeness accounting, re-derived (the receipt hash alone
        # does not prove the accounting ran: a self-consistent rehash of an
        # edited receipt would pass it).
        try:
            dropped = recheck_accounting(fam, schemas_dir, receipt)
        except Exception as e:  # noqa: BLE001 - reported, never silent
            failures.append(f"Completeness accounting for '{fam}' could not be re-run: {e}")
            continue
        for v in dropped:
            failures.append(
                f"Receipt for '{fam}' omits accounting error {v.get('kind')}: {v.get('message')}"
            )

        # 4. Recorded mismatches fail the check.
        mismatches = receipt.get("mismatches") or []
        if mismatches:
            failures.append(
                f"Receipt for '{fam}' records {len(mismatches)} mismatch(es) "
                f"[{_mismatch_summary(mismatches)}]"
            )
            continue

        if args.verbose:
            print(f"[{fam}] Receipt is fresh and conformant.")

    if failures:
        for fail in failures:
            print(f"[FAIL] {fail}", file=sys.stderr)
        return 1
    print("All volatility conformance receipts are fresh and conformant.")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="check-volatility-conformance.py",
        description=(
            "Check and write transceiver schema volatility conformance receipts."
        ),
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--check",
        action="store_true",
        help="Check existing receipts for freshness and recorded mismatches (needs no parsed standards)",
    )
    group.add_argument(
        "--write",
        action="store_true",
        help="Run comparisons against the parsed standards and write conformance receipts",
    )

    parser.add_argument(
        "--family",
        default="cmis,sff-8636,sff-8472",
        help="Comma-separated list of families to process (default: cmis,sff-8636,sff-8472)",
    )
    parser.add_argument(
        "--schemas-dir",
        default="schemas",
        help="Path to the schemas root directory (default: schemas)",
    )
    parser.add_argument(
        "--standards-dir",
        default=str(
            Path(os.environ.get("OPENXCVR_ROOT", "")) / "docs/stds/.parsed/markdown"
        ),
        help=(
            "Path to the parsed markdown standards directory (default: "
            "docs/stds/.parsed/markdown under OPENXCVR_ROOT)"
        ),
    )
    parser.add_argument(
        "--receipts-dir",
        default="tests/volatility-conformance",
        help="Path to the receipts directory (default: tests/volatility-conformance)",
    )
    parser.add_argument(
        "--authority-dir",
        default="authority-checks",
        help=(
            "Path to the completed authority-check evidence directory (planning "
            "evidence kept outside the published tree; maintainers supply it)"
        ),
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root directory (default: .)",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Verbose output",
    )

    args = parser.parse_args(argv)

    if args.check:
        return cmd_check(args)
    return cmd_write(args)


if __name__ == "__main__":
    sys.exit(main())
