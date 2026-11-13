#!/usr/bin/env python3
"""Lightweight schema lint (run after every schema edit).

Checks structure and repository hygiene without requiring a Rust toolchain: its
unique checks reject draft markers (REVIEW, TODO, TBD), verify index target
paths (cdb_commands, state_machines, tom_messages), verify checksum
self-exclusion, diagnose import cycle chains, and report optional --gaps and
--units advisories. It does not check key vocabulary.

Key vocabulary of every schema document, fragment, index and template is
enforced by the Rust guard, run with `cargo test -p openxcvr-codegen --test
strict_schema_vocabulary` (CI runs it via `cargo test --workspace`). A schema
edit is complete only when both pass.

Complements the Rust validator (`crates/openxcvr-codegen/src/validator/`). Other
checks validate repository invariants cheap to check with the YAML subset used
in `schemas/`: index includes resolve, enum files have unique names/values,
`enum_ref:` points at a declared enum registry, `openxcvr.page.v2`
version-metadata keys are well-formed, and —
when PyYAML is available — page field offsets do not
partially overlap and read-effect assertions carry non-empty provenance. Flags:
`--units` (list numeric fields missing `units:`), `--gaps` (list uncovered
bytes, including intra-byte holes), `--strict` (promote structural warnings to
failures), `--volatility=<family>[,<family>...]` (opt-in D1 volatility gate —
see `run_volatility_pass`; the default run stays unchanged until
schemagen.volatility-markings makes the pass mandatory, so commits between the
family migrations stay green).
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

try: # Optional: the offset/units checks need a real YAML
    import yaml as _yaml  # parser. The existing checks stay dependency-free; if
except ImportError:  # PyYAML is absent we simply skip the structural pass.
    _yaml = None

# Numeric primitive types, mirroring the numeric variants of `FieldType` in
# `crates/openxcvr-codegen/src/ir/pages.rs`. A numeric field with no
# `units:` annotation earns a WARNING — many are legitimately dimensionless
# (bitmaps, identifiers, raw counters) so it is never an error.
NUMERIC_TYPES = frozenset(
    {"u2", "u3", "u4", "u5", "u6", "u7", "u8", "i8",
     "u16be", "i16be", "u16le", "u32be", "u64le", "u64be", "f16be",
     "i32be", "x16be"}
)

DRAFT_MARKERS = ("REVIEW", "TODO", "TBD")
LIST_KEY_RE = re.compile(r"^([A-Za-z0-9_]+):\s*\[\]\s*$")
KEY_RE = re.compile(r"^([A-Za-z0-9_]+):\s*(.*?)\s*$")

# Valid CMIS revision strings, mirroring `Rev` in
# `crates/openxcvr-codegen/src/ir/version.rs` (Rev::CMIS_ALL). The Rust parser
# fails loud on anything else (`InvalidRevError`); this lint must agree so a
# rev-string typo (`v4.0`, `40`, `5.x`) is caught without the Rust toolchain.
# A dict keyed by spec family is the natural extension point if a non-CMIS
# family ever gains version metadata; CMIS is the only family with one today,
# so every `openxcvr.page.v2` rev string is checked against this single set.
VALID_CMIS_REVS = frozenset({"3.0", "4.0", "5.0", "5.1", "5.2", "5.3", "5.4"})

# page.v2 blocks whose entries carry a `rev:` key. `rev:` is a
# generic word, so it is treated as a CMIS rev string only while inside one of
# these blocks — an unrelated `rev:` elsewhere in the file (e.g. a provenance
# citation) is left alone.
VERSION_BLOCK_KEYS = ("changed_in", "renamed_from", "aliased_to")

# Pulls a `key: value` pair for a rev-bearing key out of one line, in both
# block style (`since: "4.0"`) and flow style (`{ rev: "4.0", enum_ref: X }`).
# The key must sit at a structural position — line start, just after `{`/`[`/`,`,
# or after a `- ` list dash — so a `since:`/`until:`/`rev:` substring inside
# quoted prose is not matched. The value token is a quoted string or a run of
# non-delimiter chars, so a trailing `# comment` and a `,`/`}`/`]` terminator
# are both excluded.
REV_KV_RE = re.compile(
    r"(?:^|[{\[,]\s*|-\s+)(since|until|rev)\s*:\s*"
    r"(\"[^\"]*\"|'[^']*'|[^\s,}\]]+)"
)


def fail(message: str) -> None:
    print(f"FAIL {message}")


def iter_yaml_files(root: Path) -> list[Path]:
    return sorted(root.glob("**/*.yaml"))


def strip_inline_comment(value: str) -> str:
    """Drop a YAML inline comment from a scalar value.

    A `#` begins a comment only when it is unquoted and preceded by whitespace
    (or starts the value) — matching the YAML spec and the serde_norway parser the
    Rust validator uses. A `#` inside single/double quotes, or not preceded by
    whitespace (e.g. `foo#bar`), stays part of the value. Without this, a line
    like `since: "4.0"  # added in 4.0` would keep its trailing comment and be
    misread as the rev string `"4.0"  # added in 4.0` — a false-positive
    rejection of a perfectly valid schema.
    """
    in_single = in_double = False
    for i, ch in enumerate(value):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double:
            if i == 0 or value[i - 1] in (" ", "\t"):
                return value[:i]
    return value


def scalar(value: str) -> str:
    value = strip_inline_comment(value).strip()
    if value.startswith('"') and value.endswith('"'):
        return value[1:-1]
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1]
    return value


def collect_list_values(lines: list[str], key: str) -> list[str]:
    values: list[str] = []
    in_key = False
    key_indent = 0

    for raw in lines:
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue

        indent = len(raw) - len(raw.lstrip(" "))
        stripped = raw.strip()

        if in_key:
            if indent <= key_indent and not stripped.startswith("- "):
                in_key = False
            elif stripped.startswith("- "):
                values.append(scalar(stripped[2:]))
                continue

        if stripped == f"{key}: []":
            return values
        if stripped == f"{key}:":
            in_key = True
            key_indent = indent

    return values


def collect_key_values(lines: list[str], key: str) -> list[str]:
    values: list[str] = []
    for raw in lines:
        stripped = raw.strip()
        if stripped.startswith(f"{key}:"):
            values.append(scalar(stripped.split(":", 1)[1]))
    return values


def validate_indexes(path: Path, lines: list[str], failures: list[str]) -> None:
    if path.name != "_index.yaml":
        return

    for key in (
        "includes",
        "imports",
        "cdb_commands",
        "state_machines",
        "tom_messages",
    ):
        for value in collect_list_values(lines, key):
            if not value or value.startswith("#"):
                continue
            target = (path.parent / value).resolve()
            if not target.exists():
                failures.append(f"{path}: {key} entry does not exist: {value}")


def validate_fragment_includes(path: Path, lines: list[str], failures: list[str]) -> None:
    for key in ("field_includes", "entry_includes"):
        for value in collect_list_values(lines, key):
            if not value or value.startswith("#"):
                continue
            target = path.parent / value
            if not target.exists():
                failures.append(f"{path}: {key} entry does not exist: {value}")


def enum_entry_blocks(lines: list[str]) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    in_entries = False

    for raw in lines:
        stripped = raw.strip()
        if stripped == "entries:":
            in_entries = True
            continue
        if not in_entries:
            continue
        if not stripped:
            continue
        if not raw.startswith(" ") and stripped.endswith(":"):
            break
        if stripped.startswith("- "):
            if current is not None:
                entries.append(current)
            current = {}
            remainder = stripped[2:]
            if ":" in remainder:
                k, v = remainder.split(":", 1)
                current[k.strip()] = scalar(v)
            continue
        if current is not None and ":" in stripped:
            k, v = stripped.split(":", 1)
            current[k.strip()] = scalar(v)

    if current is not None:
        entries.append(current)
    return entries


def validate_enum(
    path: Path,
    lines: list[str],
    file_lines: dict[Path, list[str]],
    enum_ids: set[str],
    failures: list[str],
) -> None:
    if not any(line.strip() == "kind: enum" for line in lines):
        # Not an enum file (could be a page, state machine, cdb command, etc.).
        # Substring match was replaced with exact-line match to avoid false
        # positives from field-level `kind: enumerated` encoding blocks.
        return

    if path.name == "_index.yaml":
        return

    enum_id_values = collect_key_values(lines, "id")
    enum_id = enum_id_values[0] if enum_id_values else path.stem
    enum_ids.add(enum_id)

    entries = enum_entry_blocks(lines)
    for include in collect_list_values(lines, "entry_includes"):
        target = path.parent / include
        entries.extend(enum_entry_blocks(file_lines.get(target, [])))
    if not entries:
        failures.append(f"{path}: enum has no entries")
        return

    seen_names: set[str] = set()
    seen_values: set[str] = set()
    for entry in entries:
        name = entry.get("name", "")
        value = entry.get("value", "")
        if not name:
            failures.append(f"{path}: enum entry missing name")
        if not value:
            failures.append(f"{path}: enum entry missing value")
        if name in seen_names:
            failures.append(f"{path}: duplicate enum entry name {name}")
        if value in seen_values:
            failures.append(f"{path}: duplicate enum entry value {value}")
        seen_names.add(name)
        seen_values.add(value)


def check_rev(path: Path, key: str, raw_value: str, failures: list[str]) -> None:
    """Append a failure if `raw_value` is not a known CMIS rev string."""
    value = scalar(raw_value)
    if value and value not in VALID_CMIS_REVS:
        failures.append(
            f"{path}: invalid rev string {value!r} in {key} "
            f"(expected one of {', '.join(sorted(VALID_CMIS_REVS))})"
        )


def rev_pairs(text: str, keys: tuple[str, ...]) -> list[tuple[str, str]]:
    """`(key, raw_value)` for each REV_KV_RE match in `text` whose key is wanted."""
    return [
        (m.group(1), m.group(2))
        for m in REV_KV_RE.finditer(text)
        if m.group(1) in keys
    ]


def version_block_opener(stripped: str) -> str | None:
    """Return the version-metadata block key a line opens, else None.

    Handles a bare mapping key (`changed_in:`) and a list-item key
    (`- changed_in:`) alike; the value (block or inline-flow) follows the colon.
    """
    head = stripped[2:].lstrip() if stripped.startswith("- ") else stripped
    for key in VERSION_BLOCK_KEYS:
        if head.startswith(f"{key}:"):
            return key
    return None


def validate_page_v2(path: Path, lines: list[str], failures: list[str]) -> None:
    """Validate `openxcvr.page.v2` version-metadata keys.

    Fires only on files that declare `schema: openxcvr.page.v2`. Checks:

    1. Every revision string is a known CMIS rev (mirrors `Rev::parse` in
       `version.rs`): field-level `since:` / `until:`, and `rev:` inside a
       `changed_in[]` / `renamed_from` / `aliased_to[]` block. Both block style
       (`since: "4.0"`) and flow style (`changed_in: [{ rev: "4.0", … }]`) are
       handled, inline `# comments` are ignored, and a `rev:` *outside* a
       version-metadata block is left alone (it is not a CMIS rev string).
    2. `renamed_from.old_name` does not collide with a current field `name:` in
       the same page (a collision makes the per-rev resolved name ambiguous —
       at pre-rename revs two fields would resolve to the same name).

    `changed_in[].enum_ref` resolution is *not* re-checked here: the global
    `enum_ref` pass in `main()` already collects every block-style `enum_ref:`
    line (field level and `changed_in` level alike) and resolves it against the
    enum registry. (Flow-style `enum_ref:` / `old_name:` are not collected by
    that line parser; the serde_norway-based Rust validator is authoritative for
    those forms.)
    """
    if "openxcvr.page.v2" not in collect_key_values(lines, "schema"):
        return

    # Rev-string validity (mirrors `Rev::parse` in version.rs).
    #
    # `since:` / `until:` are reserved field-level keys, so every occurrence is a
    # rev string and is checked wherever it appears (block or flow). `rev:` is
    # generic, so it is checked only while inside a `changed_in:` /
    # `renamed_from:` / `aliased_to:` block, tracked by the indentation at which
    # the block key was declared. Flow content on the block-opening line
    # (`changed_in: [{ rev: "4.0", … }]`) is scanned in place; block-style
    # entries are scanned on the following, more-indented lines.
    block_indent: int | None = None
    for raw in lines:
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        indent = len(raw) - len(raw.lstrip(" "))
        stripped = raw.strip()

        if block_indent is not None and indent <= block_indent:
            block_indent = None

        for key, value in rev_pairs(stripped, ("since", "until")):
            check_rev(path, key, value, failures)

        opener = version_block_opener(stripped)
        if opener is not None:
            for key, value in rev_pairs(stripped.split(":", 1)[1], ("rev",)):
                check_rev(path, key, value, failures)
            block_indent = indent
            continue

        if block_indent is not None:
            for key, value in rev_pairs(stripped, ("rev",)):
                check_rev(path, key, value, failures)

    # renamed_from.old_name collision with a current field name.
    field_names = {
        scalar(raw.strip()[len("- name:") :])
        for raw in lines
        if raw.strip().startswith("- name:")
    }
    for old_name in collect_key_values(lines, "old_name"):
        if old_name in field_names:
            failures.append(
                f"{path}: renamed_from.old_name {old_name!r} collides with a "
                f"current field name in the same page"
            )


def _has_provenance(value) -> bool:
    return (isinstance(value, str) and bool(value.strip())) or (
        isinstance(value, (dict, list)) and bool(value)
    )


def validate_read_effects(path: Path, doc, failures: list[str]) -> None:
    """Reject uncited side-effect-free assertions, including revision overrides.

    Fragments and templates are checked too. A partial revision override inherits
    the preceding citation; an explicit null clears it, matching the Rust IR.
    The parser remains authoritative for cross-file template resolution.
    """
    if isinstance(doc, list):
        for item in doc:
            validate_read_effects(path, item, failures)
    elif isinstance(doc, dict):
        effect = doc.get("read_effect")
        provenance = doc.get("read_effect_provenance")
        name = doc.get("name", "?")
        if effect == "none" and not _has_provenance(provenance):
            failures.append(f"{path}: field {name!r}: read_effect none requires nonempty read_effect_provenance")
        for entry in sorted(doc.get("changed_in") or [], key=lambda e: str(e.get("rev", ""))):
            effect = entry.get("read_effect", effect)
            provenance = entry.get("read_effect_provenance", provenance)
            if effect == "none" and not _has_provenance(provenance):
                failures.append(f"{path}: field {name!r} revision {entry.get('rev')}: read_effect none requires nonempty read_effect_provenance")
        for key, value in doc.items():
            if key not in ("changed_in", "read_effect_provenance", "provenance", "authority"):
                validate_read_effects(path, value, failures)


def validate_read_effect_text(path: Path, text: str, failures: list[str]) -> None:
    # Provenance is safety-critical: unlike optional structural warnings, it
    # cannot silently pass when the YAML parser is unavailable or fails.
    if "read_effect" not in text:
        return
    if _yaml is None:
        failures.append(f"{path}: PyYAML required to validate read_effect_provenance")
        return
    try:
        doc = _yaml.safe_load(text)
    except Exception as error:
        failures.append(f"{path}: cannot validate read_effect_provenance: {error}")
        return
    validate_read_effects(path, doc, failures)


def _load_yaml(path: Path):
    if _yaml is None:
        return None
    try:
        return _yaml.safe_load(path.read_text(encoding="utf-8"))
    except Exception:
        return None


def _page_fields(path: Path, doc: dict) -> list[dict]:
    """Aggregate a page's inline `fields:` plus every `field_includes:` fragment."""
    fields: list[dict] = [f for f in (doc.get("fields") or []) if isinstance(f, dict)]
    for inc in doc.get("field_includes") or []:
        frag = _load_yaml(path.parent / inc)
        if isinstance(frag, dict):
            fields.extend(f for f in (frag.get("fields") or []) if isinstance(f, dict))
    return fields


def validate_page_offsets(
    path: Path,
    doc,
    warnings: list[str],
    advisories: list[str],
    *,
    report_gaps: bool,
) -> None:
    """Wave 2a structural checks for `kind: page` schemas.

    - **Partial offset overlap** (warning, ``--strict``-promotable): two static
      fields whose byte/bit ranges *partially* intersect — neither contains the
      other. Containment is a legal parent→sub-field relationship (a
      composite/bitmap byte and the fields decomposing it, or a reserved
      placeholder overlaid by detail fields). Fields with `instances:` /
      `offset_expr:` / `variants:` / `present_when:` are skipped — their ranges
      are computed, polymorphic, or conditional.
    - **Byte gaps** (warning, ``--gaps``): bytes with no covering field.
    - **Missing units** (advisory, ``--units``): a numeric scalar field with no
      `units:`. Advisory only — never fails, even under ``--strict`` — because
      many fields are legitimately dimensionless.
    """
    if not isinstance(doc, dict) or doc.get("kind") != "page":
        return
    fields = _page_fields(path, doc)

    occupied: list[tuple[str, dict[int, int]]] = []
    all_bytes: set[int] = set()

    for f in fields:
        name = f.get("name", "?")
        if "instances" in f or "offset_expr" in f or "variants" in f or "present_when" in f:
            continue
        ftype = f.get("type")
        bit_width = f.get("bit_width")
        bit_offset = f.get("bit_offset")
        if ftype in NUMERIC_TYPES and "units" not in f and bit_width is None:
            advisories.append(f"{path}: numeric field '{name}' ({ftype}) has no units:")
        offset = f.get("offset")
        if not isinstance(offset, int):
            continue
        occ: dict[int, int] = {}
        if isinstance(bit_width, int) and isinstance(bit_offset, int):
            occ[offset] = ((1 << bit_width) - 1) << bit_offset
        else:
            size = f.get("size_bytes")
            if isinstance(size, int):
                for b in range(offset, offset + size):
                    occ[b] = 0xFF
        if occ:
            occupied.append((name, occ))
            all_bytes.update(occ)

    def contains(big: dict[int, int], small: dict[int, int]) -> bool:
        # `big` ⊇ `small`: every byte of small is in big with a superset mask.
        return all(b in big and (big[b] & m) == m for b, m in small.items())

    for i in range(len(occupied)):
        ni, oi = occupied[i]
        for j in range(i + 1, len(occupied)):
            nj, oj = occupied[j]
            shared = oi.keys() & oj.keys()
            if not any(oi[b] & oj[b] for b in shared):
                continue  # disjoint
            if contains(oi, oj) or contains(oj, oi):
                continue  # legal parent → sub-field
            b = sorted(shared)[0]
            warnings.append(
                f"{path}: partial offset overlap at byte {b}: "
                f"'{ni}' and '{nj}' intersect but neither contains the other"
            )

    if report_gaps:
        size = doc.get("size_bytes")
        if isinstance(size, int) and all_bytes:
            base = 0 if doc.get("page_address") in (None, "lower") else min(all_bytes)
            missing = [b for b in range(base, size) if b not in all_bytes]
            if missing:
                warnings.append(
                    f"{path}: {len(missing)} byte(s) with no covering field "
                    f"(first: {missing[0]})"
                )
        # D1 (task 12): intra-byte holes. A byte whose explicit bit fields
        # cover only part of it is an uncovered range even when a whole-byte
        # container sits on it: the container is a decoding convenience, not
        # marking evidence, and an absent bit marking never makes the byte
        # safe to read. Skipped-form fields (instances/offset_expr/variants/
        # present_when) have computed or conditional extents, so a page using
        # them anywhere is excluded rather than reporting a hole the static
        # view cannot see.
        dynamic_addressing = any(
            "instances" in f or "offset_expr" in f or "variants" in f or "present_when" in f
            for f in fields
        )
        if not dynamic_addressing:
            bit_masks: dict[int, int] = {}
            for f in fields:
                bw, bo, off = f.get("bit_width"), f.get("bit_offset"), f.get("offset")
                if isinstance(bw, int) and isinstance(bo, int) and isinstance(off, int):
                    bit_masks[off] = bit_masks.get(off, 0) | (((1 << bw) - 1) << bo)
            for b in sorted(bit_masks):
                holes = [bit for bit in range(8) if not (bit_masks[b] >> bit) & 1]
                if holes:
                    warnings.append(
                        f"{path}: byte {b}: bits {holes} have no explicit bit field "
                        "(intra-byte hole; a whole-byte container is not marking evidence)"
                    )


def validate_page_checksums(path: Path, doc, failures: list[str]) -> None:
    """Validate `encoding.kind == checksum` fields: a well-formed
    `covers_range` [start,end] within the page, excluding the checksum's own
    byte, with `algorithm: sum8`."""
    if not isinstance(doc, dict) or doc.get("kind") != "page":
        return
    size = doc.get("size_bytes")
    # Page base mirrors the emitters: upper pages use flat offsets 128.. ;
    # lower / A0h / A2h (and >128-byte pages) are 0-based.
    addr = str(doc.get("page_address") or "").lower()
    base = 0 if (addr in ("lower", "a0h", "a2h") or (isinstance(size, int) and size > 128)) else 128
    hi = base + size if isinstance(size, int) else None
    for f in _page_fields(path, doc):
        enc = f.get("encoding")
        if not isinstance(enc, dict) or enc.get("kind") != "checksum":
            continue
        name = f.get("name", "?")
        cr = enc.get("covers_range")
        if not (isinstance(cr, list) and len(cr) == 2 and all(isinstance(x, int) for x in cr)):
            failures.append(f"{path}: checksum field '{name}' missing/invalid covers_range")
            continue
        start, end = cr
        if start > end:
            failures.append(f"{path}: checksum '{name}' covers_range start > end")
        if hi is not None and (start < base or end >= hi):
            failures.append(
                f"{path}: checksum '{name}' covers_range [{start},{end}] outside page [{base},{hi})"
            )
        off = f.get("offset")
        if isinstance(off, int) and start <= off <= end:
            failures.append(
                f"{path}: checksum '{name}' covers its own byte (offset {off} in [{start},{end}])"
            )
        if enc.get("algorithm") != "sum8":
            failures.append(f"{path}: checksum '{name}' algorithm must be 'sum8'")


def validate_import_cycles(files: list[Path], failures: list[str]) -> None:
    """Detect cycles in `_index.yaml` `imports:` (the spec import graph)."""
    if _yaml is None:
        return
    graph: dict[str, list[str]] = {}
    for path in files:
        if path.name != "_index.yaml":
            continue
        doc = _load_yaml(path)
        if not isinstance(doc, dict):
            continue
        spec = doc.get("spec") or {}
        sid = spec.get("id") if isinstance(spec, dict) else None
        if not sid:
            continue
        imports = spec.get("imports") if isinstance(spec, dict) else None
        graph[sid] = list(imports or [])

    WHITE, GREY, BLACK = 0, 1, 2
    color = {n: WHITE for n in graph}

    def visit(node: str, stack: list[str]) -> None:
        color[node] = GREY
        for nxt in graph.get(node, []):
            if nxt not in graph:
                continue
            if color.get(nxt) == GREY:
                cycle = " -> ".join(stack[stack.index(nxt):] + [nxt])
                failures.append(f"import cycle in _index.yaml imports: {cycle}")
            elif color.get(nxt) == WHITE:
                visit(nxt, stack + [nxt])
        color[node] = BLACK

    for node in graph:
        if color[node] == WHITE:
            visit(node, [node])


# ---------------------------------------------------------------------------
# Opt-in volatility pass.
#
# Enabled only with `--volatility=<family>[,<family>...]` over the families in
# VOLATILITY_FAMILIES. The default run is unchanged until
# schemagen.volatility-markings makes the pass mandatory (task 19).
#
# There is exactly one schema loader: `tools/check-volatility-conformance.py`
# (task 11) is imported below and supplies template/instance/field_includes
# expansion, per-revision applicability, the canonical schema-closure hash and
# the receipt accounting. The pass is built around one accounting step: every
# declared field and range in the expanded family is either checked here or
# reported as a failure, and an input form the pass cannot read is a failure,
# never a skip. It fails closed on missing/unparseable access or is_dynamic
# markings, retired spellings, uncovered bytes and intra-byte holes (per
# revision, containers are not marking evidence), disagreeing overlapping
# markings, invalid bit spans, access/effect contradictions, effect metadata
# without scope or citation, unsupported revision applicability, and committed
# receipts that are stale or record mismatches.
# ---------------------------------------------------------------------------

VOLATILITY_FAMILIES = (
    "cmis",
    "sff-8636",
    "sff-8472",
    "c-cmis-1.4",
    "c-cmis-1.5",
    "elsfp-1.0",
    "cmis-ff-1.0",
    "cmis-lt-1.0",
)
# Retired access spellings the plan names that check-volatility-conformance
# does not itself list (it reports those as unparseable instead).
VOLATILITY_RETIRED_ACCESS_EXTRA = frozenset({"rw_nv", "rw_v", "rw_triggered"})
VOLATILITY_READ_EFFECTS = frozenset(
    {"none", "clear_on_read", "acknowledge_startup_interrupt", "unknown", "write_only"}
)
VOLATILITY_READ_EFFECT_SCOPES = frozenset({"field", "byte"})
# Detailed FAIL lines per family before the remainder is counted only.
VOLATILITY_MAX_DETAIL = 200

_VOLATILITY_CONFORMANCE = None


def _volatility_conformance():
    """Load tools/check-volatility-conformance.py once (the single loader)."""
    global _VOLATILITY_CONFORMANCE
    if _VOLATILITY_CONFORMANCE is None:
        import importlib.util

        path = Path(__file__).resolve().with_name("check-volatility-conformance.py")
        spec = importlib.util.spec_from_file_location("_check_volatility_conformance", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _VOLATILITY_CONFORMANCE = module
    return _VOLATILITY_CONFORMANCE


def _parse_volatility_flag(argv: list[str]) -> tuple[bool, list[str], list[str]]:
    """(flag present, families, errors) for --volatility=<family>[,...].

    Fails closed: a present occurrence that names no usable family is an
    error naming the offending value, never an empty family list that would
    iterate zero families and exit 0 having checked nothing. Repeated
    occurrences union their families (deduplicated, first-seen order), so
    repeating the flag can only widen what is checked, never silently
    narrow it to the first value. See usage() for the operator contract.
    """
    occurrences = [a for a in argv if a.startswith("--volatility")]
    if not occurrences:
        return False, [], []
    families: list[str] = []
    errors: list[str] = []
    for arg in occurrences:
        if not arg.startswith("--volatility="):
            errors.append(
                "--volatility requires =<family>[,<family>...] "
                f"(families: {', '.join(VOLATILITY_FAMILIES)})"
            )
            continue
        value = arg[len("--volatility=") :]
        named = 0
        occ_errors = 0
        for name in value.split(","):
            name = name.strip()
            if not name:
                continue
            if name not in VOLATILITY_FAMILIES:
                occ_errors += 1
                errors.append(
                    f"unknown volatility family {name!r}; "
                    f"supported: {', '.join(VOLATILITY_FAMILIES)}"
                )
            else:
                named += 1
                if name not in families:
                    families.append(name)
        if named == 0 and occ_errors == 0:
            # Empty / all-whitespace / commas-only: without this arm the
            # flag would fail open (zero families checked, exit 0).
            errors.append(
                f"--volatility={value!r} names no family; expected "
                "--volatility=<family>[,<family>...] with a family from: "
                f"{', '.join(VOLATILITY_FAMILIES)}"
            )
    return True, families, errors


def _volatility_revisions(cvc, family: str, schemas_dir: Path) -> list[str]:
    """Revisions at which coverage is judged: every supported CMIS revision
    for cmis, the family's own declared revision otherwise."""
    if family == "cmis":
        return list(cvc.CMIS_REVISIONS)
    try:
        index = cvc._load_yaml(schemas_dir / family / "_index.yaml")
    except Exception:  # noqa: BLE001 - single-revision fallback, still checked
        index = None
    spec = index.get("spec") if isinstance(index, dict) else None
    rev = spec.get("revision") if isinstance(spec, dict) else None
    return [str(rev)] if rev else [family]


def _volatility_foreign(schemas_dir: Path, family: str, rel: str) -> bool:
    """True when a closure file belongs to another family's own memory map.

    A family index imports its base family (c-cmis-1.4 and elsfp-1.0 import
    cmis); the imported family's pages and fragments are that family's memory
    map, accounted by that family's own pass, never silently dropped here.
    """
    first = (rel or "").split("/", 1)[0]
    return first != family and (schemas_dir / first / "_index.yaml").is_file()


def _volatility_receipt(cvc, family: str, schemas_dir: Path, repo_root: Path, out: list[str]) -> None:
    """Judge a committed conformance receipt (task 18 commits them; task 19
    makes a missing receipt fail the default run — at this opt-in stage a
    missing receipt is simply not yet committed)."""
    path = repo_root / "tests" / "volatility-conformance" / f"{family}.json"
    if not path.is_file():
        return
    prefix = f"volatility[{family}] receipt:"
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001 - reported, never skipped
        out.append(f"{prefix} unreadable: {e}")
        return
    if not isinstance(receipt, dict) or receipt.get("family") != family:
        recorded = receipt.get("family") if isinstance(receipt, dict) else type(receipt).__name__
        out.append(f"{prefix} records family {recorded!r}")
        return
    if receipt.get("receipt_hash") != cvc.compute_receipt_hash(receipt):
        out.append(f"{prefix} receipt_hash is invalid (content was edited)")
    try:
        cur_hash, cur_files = cvc.compute_schema_closure(family, schemas_dir, repo_root)
        stored = receipt.get("schema_closure") or {}
        if stored.get("hash") != cur_hash or stored.get("files") != cur_files:
            out.append(
                f"{prefix} stale: schema closure changed "
                f"(receipt {str(stored.get('hash'))[:12]}, current {cur_hash[:12]})"
            )
    except Exception as e:  # noqa: BLE001
        out.append(f"{prefix} schema closure could not be recomputed: {e}")
    for v in cvc.validate_conformance_inventory(family, receipt):
        out.append(f"{prefix} violates {v.get('kind')}: {v.get('message')}")
    try:
        for v in cvc.recheck_accounting(family, schemas_dir, receipt):
            out.append(f"{prefix} omits accounting error {v.get('kind')}: {v.get('message')}")
    except Exception as e:  # noqa: BLE001
        out.append(f"{prefix} completeness accounting could not be re-run: {e}")
    mismatches = receipt.get("mismatches") or []
    if mismatches:
        out.append(f"{prefix} records {len(mismatches)} mismatch(es)")


def _volatility_effect_checks(cvc, family, schemas_dir, fields, out) -> None:
    """Read-effect metadata and changed_in overrides, per declaring entry.

    The loader resolves markings but not effect metadata, so the declaring
    entry is re-read here (by decl path, the loader's own identity) — this is
    the same input, not a second loader.
    """
    templates = cvc.load_family_templates(family, schemas_dir)
    first_by_decl = {}
    for f in fields:
        first_by_decl.setdefault(f.decl, f)
    entries: dict[str, dict | None] = {}
    for decl in first_by_decl:
        file_rel, _, idx = decl.rpartition("#")
        entries[decl] = None
        try:
            doc = cvc._load_yaml(schemas_dir / file_rel)
            payload = (
                doc if isinstance(doc, list)
                else (doc.get("fields") if isinstance(doc, dict) else None)
            )
            if isinstance(payload, list) and int(idx) < len(payload) \
                    and isinstance(payload[int(idx)], dict):
                entries[decl] = payload[int(idx)]
        except Exception:  # noqa: BLE001 - stays None, reported below
            pass

    def template_of(entry: dict):
        name = entry.get("template")
        return templates.get(name) if isinstance(name, str) else None

    for decl in sorted(entries):
        entry = entries[decl]
        base = first_by_decl[decl]
        if not isinstance(entry, dict):
            out.append(
                f"volatility[{family}] unresolved_input: declaring entry {decl} "
                "could not be re-read for effect metadata"
            )
            continue
        template = template_of(entry) or {}

        def val(key: str):
            if key in entry:
                return entry[key]
            return template.get(key)

        effect = val("read_effect")
        if effect is not None:
            if effect not in VOLATILITY_READ_EFFECTS:
                out.append(
                    f"volatility[{family}] unparseable_marking: read_effect {effect!r} "
                    f"is not in the D1 effect vocabulary ({decl})"
                )
            else:
                scope = val("read_effect_scope")
                if scope is None:
                    out.append(
                        f"volatility[{family}] effect_missing_scope: read_effect {effect!r} "
                        f"carries no read_effect_scope ({decl})"
                    )
                elif scope not in VOLATILITY_READ_EFFECT_SCOPES:
                    out.append(
                        f"volatility[{family}] unparseable_marking: read_effect_scope "
                        f"{scope!r} is not 'field' or 'byte' ({decl})"
                    )
                elif scope == "byte" and effect != "none" \
                        and val("read_effect_provenance") in (None, {}, []):
                    out.append(
                        f"volatility[{family}] effect_missing_citation: byte-scoped "
                        f"read_effect {effect!r} carries no read_effect_provenance "
                        f"citation ({decl})"
                    )
                # access/effect contradiction on one declaration
                effects = {effect}
                access = base.access
                if access == "ro_cor":
                    effects.add("clear_on_read")
                elif access in ("wo", "wo_sc"):
                    effects.add("write_only")
                elif access == "unknown":
                    effects.add("unknown")
                if "none" in effects and len(effects) > 1:
                    out.append(
                        f"volatility[{family}] effect_contradiction: read_effect 'none' "
                        f"conflicts with {sorted(effects - {'none'})} ({decl})"
                    )

        # per-revision overrides: the resolved marking must stay in the
        # vocabulary at every changed_in revision (light resolution: an
        # entry's access/flags override the base marking key-wise).
        for change in entry.get("changed_in") or []:
            if not isinstance(change, dict):
                out.append(
                    f"volatility[{family}] unparseable_marking: changed_in entry is "
                    f"not a mapping ({decl})"
                )
                continue
            rev = change.get("rev")
            if rev is not None and cvc._rev_key(rev) is None:
                out.append(
                    f"volatility[{family}] unparseable_marking: changed_in rev {rev!r} "
                    f"is not a revision number ({decl})"
                )
            c_access = change.get("access")
            if c_access is not None:
                if c_access in cvc.RETIRED_ACCESS_SPELLINGS \
                        or c_access in VOLATILITY_RETIRED_ACCESS_EXTRA:
                    out.append(
                        f"volatility[{family}] retired_marking_spelling: changed_in "
                        f"access={c_access} ({decl})"
                    )
                elif c_access not in cvc.ALLOWED_ACCESS:
                    out.append(
                        f"volatility[{family}] unparseable_marking: changed_in access "
                        f"{c_access!r} is not in the D1 vocabulary ({decl})"
                    )
            flags = change.get("flags")
            if flags is not None and not isinstance(flags, dict):
                out.append(
                    f"volatility[{family}] unparseable_marking: changed_in flags is "
                    f"not a mapping ({decl})"
                )

    # byte-scoped non-none effect vs an explicit side-effect-free neighbour
    # on the same byte (mirrors the Rust parser's cross-field rule)
    for decl, entry in entries.items():
        if not isinstance(entry, dict):
            continue
        template = template_of(entry) or {}
        effect = entry.get("read_effect", template.get("read_effect"))
        scope = entry.get("read_effect_scope", template.get("read_effect_scope"))
        if effect in (None, "none") or scope != "byte":
            continue
        base = first_by_decl[decl]
        for other_decl, other in entries.items():
            if other_decl == decl or not isinstance(other, dict):
                continue
            o_tpl = template_of(other) or {}
            if other.get("read_effect", o_tpl.get("read_effect")) != "none":
                continue
            o_base = first_by_decl[other_decl]
            if o_base.page_id != base.page_id:
                continue
            if base.byte_lo <= o_base.byte_hi and o_base.byte_lo <= base.byte_hi:
                out.append(
                    f"volatility[{family}] effect_contradiction: byte-scoped "
                    f"read_effect {effect!r} ({decl}) conflicts with explicit "
                    f"side-effect-free field ({other_decl})"
                )


def _volatility_coverage(cvc, family, schemas_dir, fields, load_errors, out) -> None:
    """Per-revision bit coverage over every defined byte, plus overlap
    consistency. A byte with explicit sub-byte fields needs all eight bits
    covered by them: a whole-byte decoding container is not independent
    evidence that fills missing child markings. Conditional (present_when)
    fields are legal overlays: they cover their bits but are exempt from the
    disagreement check.
    """
    revisions = _volatility_revisions(cvc, family, schemas_dir)
    index = cvc._load_yaml(schemas_dir / family / "_index.yaml")
    errored_pages = {e.get("page") for e in load_errors if e.get("page")}
    by_byte: dict[tuple[str, int], list] = {}
    for f in fields:
        for b in range(f.byte_lo, f.byte_hi + 1):
            by_byte.setdefault((f.page_id, b), []).append(f)
    # Enumerate exactly the pages the index includes: the accounting step in
    # _volatility_check_family separately fails any page of this family's
    # own memory map that the closure reaches but `includes:` does not list,
    # so a page absent here is reported there — never silently unenumerated.
    for inc in (index or {}).get("includes") or []:
        doc = cvc._load_yaml(schemas_dir / family / str(inc))
        if not (isinstance(doc, dict) and doc.get("kind") == "page"):
            continue
        page_id = doc.get("id") or Path(str(inc)).stem
        if page_id in errored_pages:
            continue  # its load error is already reported
        size = doc.get("size_bytes")
        if not isinstance(size, int) or size <= 0:
            out.append(
                f"volatility[{family}] unresolved_input: page {page_id} declares no "
                "positive size_bytes; its defined bytes cannot be enumerated"
            )
            continue
        addr = str(doc.get("page_address") or "").lower()
        base = 0 if addr in ("lower", "a0h", "a2h") else 128
        covered = [b for (pid, b) in by_byte if pid == page_id]
        # Mirror the --gaps byte-range semantics: lower pages are defined from
        # byte 0; other pages from their first covered byte (a page whose
        # fields start later defines nothing before them).
        lo = 0 if addr == "lower" else (min(covered) if covered else base)
        for byte in range(lo, base + size):
            fs = by_byte.get((page_id, byte), [])
            uncovered_revs: list[str] = []
            hole_bits: dict[str, list[int]] = {}
            for rev in revisions:
                applicable = [f for f in fs if cvc._applies_at(f, rev)]
                subs = [f for f in applicable if f.bit_lo is not None]
                if subs:
                    mask = 0
                    for f in subs:
                        mask |= f.bit_mask
                    missing = [bit for bit in range(8) if not (mask >> bit) & 1]
                    if missing:
                        hole_bits[rev] = missing
                elif not any(f.bit_lo is None for f in applicable):
                    uncovered_revs.append(rev)
            if uncovered_revs:
                out.append(
                    f"volatility[{family}] uncovered_byte: page {page_id} byte {byte} "
                    f"has no covering field at revision(s) {uncovered_revs}"
                )
            for rev, bits in hole_bits.items():
                out.append(
                    f"volatility[{family}] uncovered_bit: page {page_id} byte {byte} "
                    f"bit(s) {bits} have no explicit field marking at revision {rev} "
                    "(a whole-byte container is not marking evidence)"
                )
            for rev in revisions:
                whole = None
                per_bit: dict[int, tuple] = {}
                per_bit_name: dict[int, str] = {}
                for f in fs:
                    if f.present_when is not None or not cvc._applies_at(f, rev):
                        continue
                    mark = (f.access, f.is_dynamic)
                    if f.bit_lo is None:
                        if whole is not None and whole[0] != mark:
                            out.append(
                                f"volatility[{family}] overlapping_fields_disagree: "
                                f"page {page_id} byte {byte}: fields {whole[1]!r} and "
                                f"{f.name!r} disagree at revision {rev}"
                            )
                        elif whole is None:
                            whole = (mark, f.name)
                        continue
                    for bit in range(f.bit_lo, (f.bit_hi if f.bit_hi is not None else f.bit_lo) + 1):
                        if bit in per_bit and per_bit[bit] != mark:
                            out.append(
                                f"volatility[{family}] overlapping_fields_disagree: "
                                f"page {page_id} byte {byte} bit {bit}: fields "
                                f"{per_bit_name[bit]!r} and {f.name!r} disagree at "
                                f"revision {rev}"
                            )
                        elif bit not in per_bit:
                            per_bit[bit] = mark
                            per_bit_name[bit] = f.name


def _volatility_check_family(cvc, family, schemas_dir, repo_root, out) -> None:
    fields, load_errors = cvc.load_family_fields(family, schemas_dir)
    for err in load_errors:
        out.append(f"volatility[{family}] {err.get('kind')}: {err.get('message')}")

    # -- accounting: every declared entry is loaded or reported -------------
    #
    # The spine of the gate (the task's own words): a declaration present in
    # the hashed closure that is neither loaded nor reported must fail. Three
    # facts keep this loop honest, each pinned by a test:
    #
    #   * `d["file"] in reported_file` — the declaring file itself already
    #     produced a load FAIL above (e.g. a non-page include whose fields
    #     would be silently dropped), so the run already fails on that file;
    #     re-reporting its entries adds no signal. Pinned by
    #     check_volatility_reported_file_skip_is_safe.
    #   * `any(pg in reported_file for pg in d["pages"])` — the page through
    #     which the loader reaches this declaration (a field_includes
    #     fragment of a page whose own `fields` payload is unreadable) already
    #     produced a load FAIL above; the fragment's entries cannot be loaded
    #     past that page error, so the run already fails. Pinned by
    #     check_volatility_reported_page_skip_is_safe.
    #   * `_volatility_foreign(...)` — the file belongs to an imported
    #     family's own memory map (that family has its own `_index.yaml` and
    #     its own pass in this very invocation's family list): judging it
    #     here would double-count it, and skipping it loses nothing because
    #     the imported family's own pass accounts the same hashed closure
    #     files. Pinned (both directions) by
    #     check_volatility_foreign_inputs_left_to_their_own_pass.
    #
    # The loop is load-bearing: check_volatility_loader_defect_is_unaccounted
    # reddens if it is removed (verified by mutation in the fix round).
    decls = cvc.schema_declarations(family, schemas_dir)
    for err in decls["errors"]:
        if err.get("kind") == cvc.KIND_UNACCOUNTED_INPUT \
                and _volatility_foreign(schemas_dir, family, err.get("decl_file") or ""):
            continue  # the imported family's own memory map: its pass accounts it
        out.append(f"volatility[{family}] {err.get('kind')}: {err.get('message')}")
    reported_decl = {e.get("decl") for e in load_errors if e.get("decl")}
    reported_file = {e.get("decl_file") for e in load_errors if e.get("decl_file")}
    loaded = {f.decl for f in fields}
    for key, d in sorted(decls["declared"].items()):
        if key in loaded or key in reported_decl or d["file"] in reported_file \
                or any(pg in reported_file for pg in d["pages"]):
            continue
        if _volatility_foreign(schemas_dir, family, d["file"]):
            continue
        out.append(
            f"volatility[{family}] unaccounted_input: schema entry {key} "
            f"({d.get('name')!r}) is declared by the hashed closure but neither "
            "loaded nor reported"
        )
    for key in sorted(loaded - set(decls["declared"])):
        out.append(
            f"volatility[{family}] unaccounted_input: loaded field {key} is not "
            "declared by the hashed schema closure"
        )
    # A page of this family's own memory map that the hashed closure reaches
    # but `_index.yaml includes:` does not list is never coverage-checked and
    # never enters `loaded`: without this check an empty page dropped from
    # `includes:` (still referenced through another reference key) would be
    # invisible. A page that carries fields fails here AND in the
    # unsupported-form errors above — two true statements about one root
    # cause. Fragments (field_includes targets of included pages) are loaded
    # as fields of the including page; foreign pages belong to the imported
    # family's own pass.
    index_doc = cvc._load_yaml(schemas_dir / family / "_index.yaml") or {}
    included_pages = {
        (schemas_dir / family / str(r)).resolve()
        for r in (index_doc.get("includes") or [])
    }
    fragment_files: set = set()
    for page in included_pages:
        doc = cvc._load_yaml(page) if page.is_file() else None
        if isinstance(doc, dict) and doc.get("kind") == "page":
            for r in doc.get("field_includes") or []:
                fragment_files.add((page.parent / str(r)).resolve())
    for path in cvc.find_schema_closure_files(family, schemas_dir):
        doc = cvc._load_yaml(path)
        if not (isinstance(doc, dict) and doc.get("kind") == "page"):
            continue
        if path in included_pages or path in fragment_files:
            continue
        rel = cvc._decl_path(path, schemas_dir)
        if _volatility_foreign(schemas_dir, family, rel):
            continue  # the imported family's own memory map: its pass accounts it
        out.append(
            f"volatility[{family}] unaccounted_input: page {rel!r} is in the "
            "hashed closure but not in _index.yaml includes:; it is neither "
            "coverage-checked nor loaded"
        )
    uid_count: dict[str, int] = {}
    for f in fields:
        uid_count[f.uid] = uid_count.get(f.uid, 0) + 1
    for uid, n in sorted(uid_count.items()):
        if n > 1:
            out.append(
                f"volatility[{family}] unaccounted_input: field occurrence {uid} is "
                f"loaded {n} times (a duplicated include)"
            )
    # conflicting template definitions would resolve by import order: reject
    for name, defs in sorted(cvc.template_definitions(family, schemas_dir).items()):
        bodies = {json.dumps(d["body"], sort_keys=True, default=str) for d in defs}
        if len(bodies) > 1:
            out.append(
                f"volatility[{family}] conflicting_templates: template {name!r} is "
                f"defined differently in {[d['source'] for d in defs]}"
            )

    # -- markings: vocabulary, retired spellings, revision bounds ------------
    checked_decl: set[str] = set()
    mismatches: list[dict] = []
    for f in fields:
        if f.decl in checked_decl:
            continue
        checked_decl.add(f.decl)
        before = len(mismatches)
        cvc._record_marking_errors(f, mismatches)
        for m in mismatches[before:]:
            out.append(f"volatility[{family}] {m['kind']}: {m['message']} ({f.decl})")
        if f.access in VOLATILITY_RETIRED_ACCESS_EXTRA:
            out.append(
                f"volatility[{family}] retired_marking_spelling: access={f.access} ({f.decl})"
            )

    _volatility_effect_checks(cvc, family, schemas_dir, fields, out)
    _volatility_coverage(cvc, family, schemas_dir, fields, load_errors, out)
    _volatility_receipt(cvc, family, schemas_dir, repo_root, out)


def run_volatility_pass(schemas_root: Path, families: list[str], failures: list[str]) -> None:
    """Run the opt-in D1 volatility gate over `families` (fail closed).

    Appends `FAIL`-worthy messages to `failures`; each family is isolated so
    one family's unresolved input cannot silence another's diagnostics.
    """
    cvc = _volatility_conformance()
    schemas_dir = Path(schemas_root)
    repo_root = schemas_dir.resolve().parent
    for family in families:
        fam_failures: list[str] = []
        try:
            _volatility_check_family(cvc, family, schemas_dir, repo_root, fam_failures)
        except Exception as e:  # noqa: BLE001 - the gate never silently skips
            fam_failures.append(
                f"volatility[{family}] unresolved_input: pass aborted: "
                f"{type(e).__name__}: {e}"
            )
        failures.extend(fam_failures[:VOLATILITY_MAX_DETAIL])
        if len(fam_failures) > VOLATILITY_MAX_DETAIL:
            failures.append(
                f"volatility[{family}]: {len(fam_failures) - VOLATILITY_MAX_DETAIL} "
                "further failure(s) suppressed"
            )


def usage() -> str:
    """Operator contract for every flag (printed by --help)."""
    return (
        "usage: validate-schemas.py [schema_root] [flags]\n"
        "\n"
        "  schema_root       schema tree to validate (default: schemas)\n"
        "  --strict          promote structural warnings to failures\n"
        "  --gaps            list uncovered bytes, including intra-byte holes\n"
        "  --units           list numeric fields missing units (advisories)\n"
        "  --volatility=<family>[,<family>...]\n"
        "                    opt-in D1 volatility gate over: "
        + ", ".join(VOLATILITY_FAMILIES)
        + "\n"
        "                    A present flag must name at least one family: an\n"
        "                    empty, whitespace or commas-only value is an error,\n"
        "                    never a silent no-op. Repeated --volatility flags\n"
        "                    union their families (deduplicated), so repeating\n"
        "                    the flag widens what is checked; it never narrows\n"
        "                    it to the first value.\n"
    )


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = {a for a in sys.argv[1:] if a.startswith("--")}
    if "--help" in flags or "-h" in flags:
        print(usage(), end="")
        return 0
    strict = "--strict" in flags
    report_gaps = "--gaps" in flags
    report_units = "--units" in flags
    volatility_present, volatility_families, volatility_errors = (
        _parse_volatility_flag(sys.argv[1:])
    )

    root = Path("schemas")
    if args:
        root = Path(args[0])

    if not root.exists():
        print(f"FAIL schema root not found: {root}")
        return 1

    files = iter_yaml_files(root)
    if not files:
        print(f"FAIL no schema YAML files found under {root}")
        return 1

    failures: list[str] = []
    warnings: list[str] = []
    advisories: list[str] = []
    enum_ids: set[str] = set()
    file_lines: dict[Path, list[str]] = {}
    failures.extend(volatility_errors)

    for path in files:
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        file_lines[path] = lines

        for marker in DRAFT_MARKERS:
            if marker in text:
                failures.append(f"{path}: draft marker remains: {marker}")

    for path, lines in file_lines.items():
        validate_indexes(path, lines, failures)
        validate_fragment_includes(path, lines, failures)
        validate_enum(path, lines, file_lines, enum_ids, failures)
        validate_page_v2(path, lines, failures)
        validate_read_effect_text(path, "\n".join(lines), failures)

    # Wave 2a structural checks (PyYAML-backed; skipped if PyYAML is absent).
    if _yaml is not None:
        for path in files:
            doc = _load_yaml(path)
            validate_page_offsets(
                path, doc, warnings, advisories, report_gaps=report_gaps
            )
            validate_page_checksums(path, doc, failures)
        validate_import_cycles(files, failures)

    # Opt-in D1 volatility gate (task 12); the default run is unchanged.
    if volatility_present:
        run_volatility_pass(root, volatility_families, failures)

    enum_refs: list[tuple[Path, str]] = []
    for path, lines in file_lines.items():
        for value in collect_key_values(lines, "enum_ref"):
            if value:
                enum_refs.append((path, value))

    for path, enum_ref in enum_refs:
        if enum_ref not in enum_ids:
            failures.append(f"{path}: unresolved enum_ref {enum_ref}")

    # Promote structural warnings to failures only under --strict. Advisories
    # (missing units) are never promoted — they are surfaced only with --units.
    if strict:
        failures.extend(warnings)
        warnings = []

    if report_units and advisories:
        for message in advisories:
            print(f"WARN {message}", file=sys.stderr)
        print(f"{len(advisories)} unit advisory(ies).", file=sys.stderr)

    if warnings:
        for message in warnings:
            print(f"WARN {message}", file=sys.stderr)
        print(f"{len(warnings)} warning(s) (non-fatal; --strict to enforce).", file=sys.stderr)

    if failures:
        for message in failures:
            fail(message)
        print(f"Schema validation failed: {len(failures)} issue(s).", file=sys.stderr)
        return 1

    print(
        f"Schema validation passed for {len(files)} file(s), {len(enum_ids)} enum(s)."
    )
    return 0



if __name__ == "__main__":
    raise SystemExit(main())
