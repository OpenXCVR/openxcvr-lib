#!/usr/bin/env python3
"""Self-tests for the Wave 2a additions to ``validate-schemas.py``.

Run: ``python3 tools/test_validate_schemas.py`` (no pytest dependency).

Loads the hyphenated sibling module via importlib and exercises the
containment-aware overlap detector, the conditional/computed skip rules, and the
``_index.yaml`` import-cycle detector against tiny synthetic docs.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "validate_schemas", Path(__file__).with_name("validate-schemas.py")
)
vs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vs)

FAKE = Path("schemas/fake/page.yaml")


def _page(fields):
    return {"kind": "page", "page_address": "lower", "size_bytes": 128, "fields": fields}


def check_partial_overlap_flagged():
    doc = _page([
        {"name": "a", "offset": 10, "size_bytes": 4, "type": "u32be"},
        {"name": "b", "offset": 12, "size_bytes": 4, "type": "u32be"},
    ])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert any("partial offset overlap" in w for w in warns), warns


def check_containment_skipped():
    # `parent` (8 bytes) fully contains `child` (2 bytes) — legal sub-field.
    doc = _page([
        {"name": "parent", "offset": 20, "size_bytes": 8, "type": "composite"},
        {"name": "child", "offset": 22, "size_bytes": 2, "type": "u16be"},
    ])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert not warns, warns


def check_bitmap_byte_contains_bits():
    # Full-byte bitmap + named bits within it — legal.
    doc = _page([
        {"name": "flags", "offset": 92, "size_bytes": 1, "type": "u8"},
        {"name": "bit6", "offset": 92, "bit_offset": 6, "bit_width": 1, "type": "bool"},
        {"name": "bit5", "offset": 92, "bit_offset": 5, "bit_width": 1, "type": "bool"},
    ])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert not warns, warns


def check_present_when_skipped():
    # Conditional field overlapping an unconditional one is a legal overlay.
    doc = _page([
        {"name": "ext", "offset": 56, "size_bytes": 36, "type": "composite",
         "present_when": "x == 1"},
        {"name": "reserved", "offset": 75, "size_bytes": 20, "type": "bytes"},
    ])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert not warns, warns


def check_computed_fields_skipped():
    doc = _page([
        {"name": "inst", "offset_expr": "3 + (bit / 8)", "type": "bool",
         "instances": {"index": "bit", "start": 0, "end": 63}},
        {"name": "scalar", "offset": 3, "size_bytes": 1, "type": "u8"},
    ])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert not warns, warns


def check_units_advisory():
    doc = _page([{"name": "n", "offset": 0, "size_bytes": 1, "type": "u8"}])
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=False)
    assert adv and "no units" in adv[0], adv


def check_import_cycle_detected():
    files = [Path("a/_index.yaml"), Path("b/_index.yaml")]
    docs = {
        Path("a/_index.yaml"): {"spec": {"id": "a", "imports": ["b"]}},
        Path("b/_index.yaml"): {"spec": {"id": "b", "imports": ["a"]}},
    }
    orig = vs._load_yaml
    vs._load_yaml = lambda p: docs.get(p)
    try:
        failures: list[str] = []
        vs.validate_import_cycles(files, failures)
    finally:
        vs._load_yaml = orig
    assert any("import cycle" in f for f in failures), failures


def check_no_cycle_when_acyclic():
    files = [Path("a/_index.yaml"), Path("b/_index.yaml")]
    docs = {
        Path("a/_index.yaml"): {"spec": {"id": "a", "imports": ["b"]}},
        Path("b/_index.yaml"): {"spec": {"id": "b", "imports": []}},
    }
    orig = vs._load_yaml
    vs._load_yaml = lambda p: docs.get(p)
    try:
        failures: list[str] = []
        vs.validate_import_cycles(files, failures)
    finally:
        vs._load_yaml = orig
    assert not failures, failures


def _checksum_doc(covers, *, addr="A0h", size=256, offset=63):
    return {
        "kind": "page",
        "page_address": addr,
        "size_bytes": size,
        "fields": [{
            "name": "cc",
            "offset": offset,
            "encoding": {"kind": "checksum", "covers_range": covers, "algorithm": "sum8"},
        }],
    }


def check_checksum_valid_passes():
    f: list[str] = []
    vs.validate_page_checksums(Path("x"), _checksum_doc([0, 62]), f)
    assert not f, f


def check_checksum_covers_self_flagged():
    f: list[str] = []
    vs.validate_page_checksums(Path("x"), _checksum_doc([0, 63]), f)
    assert any("covers its own" in m for m in f), f


def check_checksum_out_of_range_flagged():
    f: list[str] = []
    vs.validate_page_checksums(Path("x"), _checksum_doc([0, 300]), f)
    assert any("outside page" in m for m in f), f


def check_checksum_flat_base_ok():
    # SFF-8636-style: 128-byte page, flat offsets 128.., base 128.
    f: list[str] = []
    vs.validate_page_checksums(
        Path("x"), _checksum_doc([128, 190], addr="00h", size=128, offset=191), f
    )
    assert not f, f


def check_checksum_bad_algorithm_flagged():
    doc = _checksum_doc([0, 62])
    doc["fields"][0]["encoding"]["algorithm"] = "crc16"
    f: list[str] = []
    vs.validate_page_checksums(Path("x"), doc, f)
    assert any("algorithm" in m for m in f), f


def check_uncited_none_rejected():
    for field in [
        {"name": "uncited", "read_effect": "none"},
        {"name": "uncited", "changed_in": [{"rev": "5.3", "read_effect": "none"}]},
        {"name": "cleared", "read_effect": "none", "read_effect_provenance": "synthetic authority",
         "changed_in": [{"rev": "5.3", "read_effect_provenance": None}]},
        *[{"name": "uncited", "read_effect": "none", "read_effect_provenance": v}
          for v in [None, {}, [], ""]],
    ]:
        failures = []
        vs.validate_read_effects(FAKE, {"fields": [field]}, failures)
        assert any("read_effect_provenance" in f for f in failures), failures


def check_cited_none_and_unmarked_ro_pass():
    failures = []
    vs.validate_read_effects(FAKE, {"fields": [
        {"name": "safe", "access": "ro", "read_effect": "none",
         "read_effect_provenance": {"source": "synthetic test authority"}},
        {"name": "unmarked", "access": "ro"},
    ]}, failures)
    assert not failures, failures


def check_read_effect_text_fails_closed_without_yaml():
    original = vs._yaml
    vs._yaml = None
    try:
        failures = []
        vs.validate_read_effect_text(FAKE, "fields: [{read_effect: none}]", failures)
        assert any("PyYAML required" in f for f in failures), failures
    finally:
        vs._yaml = original


def check_numeric_types_matches_field_type():
    # Mirrors the numeric variants of FieldType in crates/openxcvr-codegen/src/ir/pages.rs.
    expected = {
        "u2", "u3", "u4", "u5", "u6", "u7", "u8", "i8",
        "u16be", "i16be", "u16le", "u32be", "u64le", "u64be", "f16be",
        "i32be", "x16be",
    }
    assert vs.NUMERIC_TYPES == expected, (
        f"NUMERIC_TYPES drift: missing {expected - vs.NUMERIC_TYPES}, "
        f"extra {vs.NUMERIC_TYPES - expected}"
    )


def check_read_effect_cli_rejects_uncited_fragment():
    # Exercises main's wiring (including fragment files), not just the helper.
    import subprocess
    import sys
    import tempfile
    with tempfile.TemporaryDirectory() as temp:
        path = Path(temp) / "part.yaml"
        path.write_text("fields:\n  - {name: uncited, read_effect: none}\n")
        result = subprocess.run([sys.executable, str(Path(__file__).with_name("validate-schemas.py")), temp],
                                capture_output=True, text=True)
        assert result.returncode == 1, result.stdout + result.stderr
        assert "read_effect_provenance" in result.stdout, result.stdout
# ---------------------------------------------------------------------------
# Opt-in volatility pass tests.
#
# Every rule the pass enforces has a synthetic failing fixture that pins the
# diagnostic text and the nonzero status, plus the positive shapes the prompt
# names (reversed order, explicit-false is_dynamic, mixed readable children,
# container+children, startup acknowledgment plus Unknown, fresh receipt).
# The receipt tests reuse the synthetic conformance fixtures from
# tools/test_check_volatility_conformance.py (task 11) so the gate and the
# conformance tool share one fixture corpus.
# ---------------------------------------------------------------------------


def _vol_fixtures():
    """SyntheticFixtures from the conformance tool's own test module."""
    if vs._yaml is None:
        return None
    path = Path(__file__).with_name("test_check_volatility_conformance.py")
    spec = importlib.util.spec_from_file_location("cvc_test_fixtures", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.SyntheticFixtures


_SF = _vol_fixtures()


def _vol_index(family: str, revision: str, templates: str = "") -> str:
    return (
        "schema: openxcvr.schema_index.v1\n"
        "spec:\n"
        f"  id: {family}\n"
        f'  revision: "{revision}"\n'
        "kind: memory_map\n"
        "includes:\n"
        "  - page.yaml\n"
        + templates
    )


def _vol_page(size: int, address: str = "lower") -> str:
    return (
        "schema: openxcvr.page.v1\n"
        "kind: page\n"
        "id: VolPage\n"
        "title: Volatility Test Page\n"
        f"page_address: {address}\n"
        f"size_bytes: {size}\n"
        "field_includes:\n"
        "  - fields/part.yaml\n"
        "fields: []\n"
    )


def _vol_tree(family, fields_text, *, revision="5.3", templates="", size=16, address="lower"):
    """Synthetic one-page family tree under a temp dir (fail loud on misuse)."""
    assert _SF is not None, "PyYAML required for volatility fixtures"
    td = tempfile.TemporaryDirectory()
    root = Path(td.name)
    fam = root / "schemas" / family
    (fam / "fields").mkdir(parents=True)
    (fam / "_index.yaml").write_text(_vol_index(family, revision, templates), encoding="utf-8")
    (fam / "page.yaml").write_text(_vol_page(size, address), encoding="utf-8")
    (fam / "fields" / "part.yaml").write_text("fields:\n" + fields_text, encoding="utf-8")
    return td


def _vol_run(td, family):
    failures: list[str] = []
    vs.run_volatility_pass(Path(td.name) / "schemas", [family], failures)
    return failures


def _run_main(*argv):
    """Run vs.main() in-process with captured output; (rc, stdout, stderr)."""
    buf_out, buf_err = io.StringIO(), io.StringIO()
    old = sys.argv
    sys.argv = ["validate-schemas.py", *argv]
    try:
        with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
            rc = vs.main()
    finally:
        sys.argv = old
    return rc, buf_out.getvalue(), buf_err.getvalue()


_VOL_TEMPLATES = (
    "templates:\n"
    "  vol_ro_static:\n"
    "    access: ro\n"
    "    flags:\n"
    "      is_dynamic: false\n"
)


def _vol_clean_fields() -> list[str]:
    """A fully covered 16-byte page exercising every legal D1 shape."""
    return [
        "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    flags: { is_dynamic: false }\n",
        "  - name: temperature\n    offset: 1\n    size_bytes: 2\n    type: u16be\n"
        "    access: ro\n    flags: { is_dynamic: true }\n",
        # byte 2: mixed readable children + explicit Unknown reserved bits
        "  - name: bit0\n    offset: 2\n    bit_offset: 0\n    bit_width: 1\n    type: bool\n"
        "    access: ro\n    flags: { is_dynamic: true }\n",
        "  - name: bits_1_2\n    offset: 2\n    bit_offset: 1\n    bit_width: 2\n    type: u2\n"
        "    access: rw\n    flags: { is_dynamic: false }\n",
        "  - name: reserved_2_3\n    offset: 2\n    bit_offset: 3\n    bit_width: 2\n    type: u2\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n",
        "  - name: bits_5_7\n    offset: 2\n    bit_offset: 5\n    bit_width: 3\n    type: u3\n"
        "    access: ro_cor\n    flags: { is_dynamic: true }\n",
        # bytes 3-6: instance expansion
        "  - name: lane_flag\n    offset_expr: '3 + i'\n"
        "    instances: { index: i, start: 0, end: 3 }\n    size_bytes: 1\n    type: u8\n"
        "    access: ro_cor\n    flags: { is_dynamic: true }\n",
        # byte 7: revision swap (until exclusive, since inclusive)
        "  - name: legacy_control\n    offset: 7\n    size_bytes: 1\n    type: u8\n"
        "    access: rw\n    until: '4.0'\n    flags: { is_dynamic: false }\n",
        "  - name: modern_control\n    offset: 7\n    size_bytes: 1\n    type: u8\n"
        "    access: rw\n    since: '4.0'\n    flags: { is_dynamic: false }\n",
        # bytes 8-13: readable range
        "  - name: telemetry\n    offset: 8\n    size_bytes: 6\n    type: bytes\n"
        "    access: ro\n    flags: { is_dynamic: true }\n",
        # byte 14: template-provided marking
        "  - name: vendor_byte\n    offset: 14\n    size_bytes: 1\n    type: u8\n"
        "    template: vol_ro_static\n",
        # byte 15: byte-scoped startup acknowledgment with citation
        "  - name: startup_ack\n    offset: 15\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    read_effect: acknowledge_startup_interrupt\n"
        "    read_effect_scope: byte\n"
        "    read_effect_provenance: { source: primary, clause: startup }\n"
        "    flags: { is_dynamic: true }\n",
    ]


def check_volatility_clean_tree_passes():
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert not failures, failures
        rc, out, err = _run_main(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc == 0, (rc, out, err)
        assert "Schema validation passed" in out, out


def check_volatility_reversed_field_order_passes():
    if _SF is None:
        return
    # Same fields, opposite order: coverage and conflict checks must be
    # order-independent, never resolved by list position.
    td = _vol_tree("cmis", "".join(reversed(_vol_clean_fields())), templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert not failures, failures


def check_volatility_default_run_unchanged_without_flag():
    if _SF is None:
        return
    # A tree missing access markings passes the default run: the pass is
    # opt-in until task 19 (schemagen.volatility-markings).
    fields = _vol_clean_fields()[0].replace("    access: ro\n", "")
    td = _vol_tree("cmis", fields)
    with td:
        rc, out, err = _run_main(str(Path(td.name) / "schemas"))
        assert rc == 0, (rc, out, err)


def check_volatility_missing_access_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace("    access: ro\n", "") + "".join(_vol_clean_fields()[1:])
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("missing_marking" in f and "access" in f for f in failures), failures
        rc, out, _ = _run_main(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc == 1, out
        assert any(
            line.startswith("FAIL ") and "access" in line for line in out.splitlines()
        ), out


def check_volatility_omitted_is_dynamic_fails_but_explicit_false_passes():
    if _SF is None:
        return
    # Omitted flags.is_dynamic is a missing marking; explicit false is a
    # valid conservative declaration and must not be conflated with omission.
    omitted = _vol_clean_fields()[0].replace("    flags: { is_dynamic: false }\n", "")
    td = _vol_tree("cmis", omitted)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("missing_marking" in f and "is_dynamic" in f for f in failures), failures
    explicit = _vol_clean_fields()[0]  # carries explicit false
    td = _vol_tree("cmis", explicit)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("identifier" in f for f in failures) is False or not [
            f for f in failures if "identifier" in f and "is_dynamic" in f
        ], failures


def check_volatility_unparseable_access_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace("access: ro", "access: sometimes")
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("unparseable_marking" in f and "sometimes" in f for f in failures), failures


def check_volatility_retired_access_spellings_fail():
    if _SF is None:
        return
    for spelling in ("cor", "rw_with_state_guard", "rw_nv", "rw_v", "rw_triggered"):
        fields = _vol_clean_fields()[0].replace("access: ro", f"access: {spelling}")
        td = _vol_tree("cmis", fields)
        with td:
            failures = _vol_run(td, "cmis")
            assert any(
                ("retired_marking_spelling" in f and spelling in f) for f in failures
            ), (spelling, failures)


def check_volatility_retired_latching_spellings_fail():
    if _SF is None:
        return
    for value in ("clear_on_read", "clear_on_read_when_condition_clears", "implementation_defined"):
        fields = _vol_clean_fields()[0].replace(
            "    access: ro\n",
            "    access: ro\n    behavior:\n      latching: " + value + "\n",
        )
        td = _vol_tree("cmis", fields)
        with td:
            failures = _vol_run(td, "cmis")
            assert any(
                "retired_marking_spelling" in f and value in f for f in failures
            ), (value, failures)


def _vol_cmis_byte3_fields(*, bit7: bool = True) -> str:
    fields = (
        "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    flags: { is_dynamic: false }\n"
        "  - name: temperature\n    offset: 1\n    size_bytes: 2\n    type: u16be\n"
        "    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: interrupt_deasserted\n    offset: 3\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: module_state\n    offset: 3\n    bit_offset: 1\n    bit_width: 3\n"
        "    type: u3\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: reserved_byte3_bits_4_6\n    offset: 3\n    bit_offset: 4\n    bit_width: 3\n"
        "    type: u3\n    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    if bit7:
        fields += (
            "  - name: reserved_byte3_bit_7\n    offset: 3\n    bit_offset: 7\n"
            "    bit_width: 1\n    type: bool\n    access: unknown\n"
            "    flags: { is_dynamic: false }\n"
        )
    fields += (
        "  - name: reserved_tail\n    offset: 4\n    size_bytes: 12\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    return fields


def check_volatility_cmis_byte3_bit7_removal_fails():
    if _SF is None:
        return
    td = _vol_tree("cmis", _vol_cmis_byte3_fields(bit7=False))
    with td:
        failures = _vol_run(td, "cmis")
        hits = [f for f in failures if "uncovered_bit" in f and "byte 3" in f]
        assert hits and all("[7]" in f for f in hits), failures
        rc, out, _ = _run_main(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc == 1 and any("^" != line[0] and line.startswith("FAIL ") and "bit" in line for line in out.splitlines()), out
    td = _vol_tree("cmis", _vol_cmis_byte3_fields(bit7=True))
    with td:
        assert not _vol_run(td, "cmis")


def check_volatility_sff8636_byte2_bit3_removal_fails():
    if _SF is None:
        return
    head = (
        "  - name: identifier\n    offset: 0\n    size_bytes: 2\n    type: u16be\n"
        "    access: ro\n    flags: { is_dynamic: false }\n"
    )
    body = (
        "  - name: data_not_ready\n    offset: 2\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: interrupt_deasserted\n    offset: 2\n    bit_offset: 1\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: flat_memory\n    offset: 2\n    bit_offset: 2\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: false }\n"
    )
    bit3 = (
        "  - name: reserved_byte2_bit_3\n    offset: 2\n    bit_offset: 3\n    bit_width: 1\n"
        "    type: bool\n    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    tail = (
        "  - name: reserved_byte2_bits_4_7\n    offset: 2\n    bit_offset: 4\n    bit_width: 4\n"
        "    type: u4\n    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: reserved_tail\n    offset: 3\n    size_bytes: 13\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("sff-8636", head + body + bit3 + tail, revision="2.11")
    with td:
        assert not _vol_run(td, "sff-8636")
    td = _vol_tree("sff-8636", head + body + tail, revision="2.11")
    with td:
        failures = _vol_run(td, "sff-8636")
        hits = [f for f in failures if "uncovered_bit" in f and "byte 2" in f]
        assert hits and all("[3]" in f for f in hits), failures


def check_volatility_sff8472_byte110_bit0_removal_fails():
    if _SF is None:
        return
    head = (
        "  - name: reserved_head\n    offset: 100\n    size_bytes: 10\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
        # whole-byte raw container: legal, but never bit-marking evidence
        "  - name: status_control\n    offset: 110\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    flags: { is_dynamic: true }\n"
    )
    children = (
        "  - name: data_not_ready\n    offset: 110\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: rx_los_state\n    offset: 110\n    bit_offset: 1\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: tx_fault_state\n    offset: 110\n    bit_offset: 2\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: soft_tx_disable_select\n    offset: 110\n    bit_offset: 3\n    bit_width: 1\n"
        "    type: bool\n    access: rw\n    flags: { is_dynamic: false }\n"
        "  - name: reserved_bits_4_5\n    offset: 110\n    bit_offset: 4\n    bit_width: 2\n"
        "    type: u2\n    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: soft_tx_disable_select_2\n    offset: 110\n    bit_offset: 6\n    bit_width: 1\n"
        "    type: bool\n    access: rw\n    flags: { is_dynamic: false }\n"
        "  - name: tx_disable_state\n    offset: 110\n    bit_offset: 7\n    bit_width: 1\n"
        "    type: bool\n    access: rw\n    flags: { is_dynamic: false }\n"
    )
    bit0 = children.split("  - name: rx_los_state")[0]
    rest = "  - name: rx_los_state" + children.split("  - name: rx_los_state")[1]
    tail = (
        "  - name: reserved_111\n    offset: 111\n    size_bytes: 1\n    type: u8\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("sff-8472", head + children + tail, revision="12.5a", size=112, address="A2h")
    with td:
        assert not _vol_run(td, "sff-8472")
    # remove the bit-0 child: the raw container must NOT fill the hole
    td = _vol_tree("sff-8472", head + rest + tail, revision="12.5a", size=112, address="A2h")
    with td:
        failures = _vol_run(td, "sff-8472")
        hits = [f for f in failures if "uncovered_bit" in f and "byte 110" in f]
        assert hits and all("[0]" in f for f in hits), failures


def check_volatility_uncovered_byte_fails():
    if _SF is None:
        return
    blocks = [b for b in _vol_clean_fields() if "telemetry" not in b]
    td = _vol_tree("cmis", "".join(blocks), templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        hits = [f for f in failures if "uncovered_byte" in f]
        assert any("byte 8" in f for f in hits), failures


def check_volatility_conflicting_overlap_fails():
    if _SF is None:
        return
    extra = (
        "  - name: bit0_duplicate\n    offset: 2\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: rw\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("cmis", "".join(_vol_clean_fields()) + extra, templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert any(
            "overlapping_fields_disagree" in f and "bit 0" in f for f in failures
        ), failures


def check_volatility_conditional_overlay_is_not_a_conflict():
    if _SF is None:
        return
    # present_when overlays (mutually exclusive alternatives) are legal and
    # cover their bits: no disagreement may be reported between them.
    extra = (
        "  - name: alt_byte0\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
        "    access: rw\n    flags: { is_dynamic: false }\n"
        "    present_when: 'identifier == 1'\n"
    )
    td = _vol_tree("cmis", "".join(_vol_clean_fields()) + extra, templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert not [f for f in failures if "overlapping_fields_disagree" in f], failures


def check_volatility_invalid_bit_bounds_fail():
    if _SF is None:
        return
    fields = (
        "  - name: bad_span\n    offset: 0\n    bit_offset: 6\n    bit_width: 3\n"
        "    type: u3\n    access: ro\n    flags: { is_dynamic: false }\n"
        "  - name: rest\n    offset: 1\n    size_bytes: 15\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("does not fit inside one byte" in f for f in failures), failures


def check_volatility_access_effect_contradiction_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace(
        "access: ro", "access: ro_cor\n    read_effect: none\n    read_effect_scope: field"
    )
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("effect_contradiction" in f for f in failures), failures


def check_volatility_effect_requires_scope_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace(
        "access: ro", "access: ro\n    read_effect: clear_on_read"
    )
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("effect_missing_scope" in f for f in failures), failures


def check_volatility_byte_scoped_effect_requires_citation_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace(
        "access: ro",
        "access: ro\n    read_effect: acknowledge_startup_interrupt\n"
        "    read_effect_scope: byte",
    )
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("effect_missing_citation" in f for f in failures), failures


def check_volatility_byte_effect_vs_none_neighbour_fails():
    if _SF is None:
        return
    # byte-scoped acknowledgment + explicit side-effect-free neighbour on the
    # same byte is a contradiction, not an aggregate.
    fields = "".join(_vol_clean_fields()) + (
        "  - name: safe_neighbour\n    offset: 15\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    read_effect: none\n    read_effect_scope: byte\n"
        "    read_effect_provenance: { source: primary, clause: safe }\n"
        "    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("cmis", fields, templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert any("effect_contradiction" in f for f in failures), failures


def check_volatility_startup_ack_plus_unknown_passes():
    if _SF is None:
        return
    # A byte-scoped startup acknowledgment coexisting with an Unknown
    # reserved constituent is representable and conservative: never a
    # contradiction, and never treated as readiness qualification.
    fields = (
        "  - name: filler\n    offset: 0\n    size_bytes: 15\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: startup_ack\n    offset: 15\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    read_effect: acknowledge_startup_interrupt\n"
        "    read_effect_scope: byte\n"
        "    read_effect_provenance: { source: primary, clause: startup }\n"
        "    flags: { is_dynamic: true }\n"
        "  - name: status_bits_0_6\n    offset: 15\n    bit_offset: 0\n    bit_width: 7\n"
        "    type: u7\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: unknown_bit_7\n    offset: 15\n    bit_offset: 7\n    bit_width: 1\n"
        "    type: bool\n    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("cmis", fields)
    with td:
        failures = _vol_run(td, "cmis")
        assert not failures, failures


def check_volatility_unsupported_revision_applicability_fails():
    if _SF is None:
        return
    fields = _vol_clean_fields()[7].replace("until: '4.0'", "until: 'v4.0'")
    blocks = list(_vol_clean_fields())
    blocks[7] = fields
    td = _vol_tree("cmis", "".join(blocks), templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        assert any(
            "is not a revision number" in f and "until" in f for f in failures
        ), failures


def check_volatility_revision_swap_coverage_hole_fails():
    if _SF is None:
        return
    # Drop the successor: byte 7 must be uncovered at 4.0 and later, not
    # silently covered by the retired field's earlier applicability.
    blocks = [b for b in _vol_clean_fields() if "modern_control" not in b]
    td = _vol_tree("cmis", "".join(blocks), templates=_VOL_TEMPLATES)
    with td:
        failures = _vol_run(td, "cmis")
        hit = [f for f in failures if "uncovered_byte" in f and "byte 7" in f]
        assert hit and all("4.0" in f for f in hit), failures


def check_volatility_template_without_is_dynamic_fails():
    if _SF is None:
        return
    templates = (
        "templates:\n"
        "  vol_ro_static:\n"
        "    access: ro\n"
    )
    td = _vol_tree(
        "cmis", "".join(_vol_clean_fields()), templates=templates
    )
    with td:
        failures = _vol_run(td, "cmis")
        assert any(
            "is_dynamic" in f and "vendor_byte" in f for f in failures
        ), failures


def check_volatility_unknown_family_rejected():
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        rc, out, _ = _run_main(str(Path(td.name) / "schemas"), "--volatility=nope")
        assert rc == 1, out
        assert any("unknown volatility family" in line for line in out.splitlines()), out


def _run_tool(*argv):
    """Run validate-schemas.py as invoked (a real subprocess): (rc, output).

    Every new failure test asserts the nonzero exit status of the tool as
    invoked, not only the in-process diagnostic list (review requirement).
    """
    import subprocess

    tool = Path(__file__).with_name("validate-schemas.py")
    proc = subprocess.run(
        [sys.executable, str(tool), *argv], capture_output=True, text=True
    )
    return proc.returncode, proc.stdout + proc.stderr


def check_volatility_flag_empty_value_fails():
    # Blocker 1: a present flag with no usable family name must fail closed.
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=")
        assert rc != 0, out
        assert any(
            line.startswith("FAIL ") and "--volatility='' names no family" in line
            for line in out.splitlines()
        ), out


def check_volatility_flag_whitespace_value_fails():
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=   ")
        assert rc != 0, out
        assert any(
            line.startswith("FAIL ") and "--volatility='   ' names no family" in line
            for line in out.splitlines()
        ), out


def check_volatility_flag_commas_only_value_fails():
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=,,,")
        assert rc != 0, out
        assert any(
            line.startswith("FAIL ") and "--volatility=',,,' names no family" in line
            for line in out.splitlines()
        ), out


def check_volatility_repeated_flags_union():
    # Repeated --volatility flags union their families (documented in the
    # help text): repeating widens what is checked, never narrows it to the
    # first value. cmis violates here, sff-8472 is clean: the union run must
    # fail on cmis even though sff-8472 alone passes.
    if _SF is None:
        return
    fields = _vol_clean_fields()[0].replace("    access: ro\n", "")  # missing access
    td = tempfile.TemporaryDirectory()
    root = Path(td.name)
    for fam, fam_fields in (("cmis", fields), ("sff-8472", "".join(_vol_clean_fields()))):
        fam_dir = root / "schemas" / fam
        (fam_dir / "fields").mkdir(parents=True)
        (fam_dir / "_index.yaml").write_text(
            _vol_index(
                fam,
                "12.5a" if fam == "sff-8472" else "5.3",
                _VOL_TEMPLATES if fam == "sff-8472" else "",
            ),
            encoding="utf-8",
        )
        (fam_dir / "page.yaml").write_text(_vol_page(16), encoding="utf-8")
        (fam_dir / "fields" / "part.yaml").write_text(
            "fields:\n" + fam_fields, encoding="utf-8"
        )
    with td:
        rc, out = _run_tool(
            str(root / "schemas"), "--volatility=sff-8472", "--volatility=cmis"
        )
        assert rc != 0, out
        assert any("missing_marking" in line for line in out.splitlines()), out
        # sff-8472 alone is clean: the failure comes from the second flag's
        # family, so a first-wins parse would have exited 0.
        rc, out = _run_tool(str(root / "schemas"), "--volatility=sff-8472")
        assert rc == 0, out
        # The union contract is stated in the help text.
        rc, out = _run_tool("--help")
        assert rc == 0, out
        assert "union their families" in out, out


def check_volatility_page_in_closure_not_included_fails():
    # Blocker 2, fix 1: a page of the family's own memory map that the hashed
    # closure reaches but `_index.yaml includes:` does not list is never
    # coverage-checked and never enters `loaded` — it must be a failure.
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        index = Path(td.name) / "schemas" / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8")
            + "cdb_commands:\n  - orphan.yaml\n",
            encoding="utf-8",
        )
        (Path(td.name) / "schemas" / "cmis" / "orphan.yaml").write_text(
            "schema: openxcvr.page.v1\nkind: page\nid: Orphan\npage_address: lower\n"
            "size_bytes: 16\nfields: []\n",
            encoding="utf-8",
        )
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc != 0, out
        assert any(
            "unaccounted_input" in line and "orphan.yaml" in line
            and "not in _index.yaml includes:" in line
            for line in out.splitlines()
        ), out
    # Control: the same page listed in includes: is enumerated by coverage
    # (its empty body fails there as uncovered bytes) — no longer invisible.
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        index = Path(td.name) / "schemas" / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "includes:\n", "includes:\n  - orphan.yaml\n"
            ),
            encoding="utf-8",
        )
        (Path(td.name) / "schemas" / "cmis" / "orphan.yaml").write_text(
            "schema: openxcvr.page.v1\nkind: page\nid: Orphan\npage_address: lower\n"
            "size_bytes: 16\nfields: []\n",
            encoding="utf-8",
        )
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc != 0, out
        assert not [l for l in out.splitlines() if "not in _index.yaml includes:" in l], out
        assert any("uncovered_byte" in l and "Orphan" in l for l in out.splitlines()), out


def check_volatility_loader_defect_is_unaccounted():
    # Blocker 2, fix 3: the accounting loop is load-bearing. A loader defect
    # that silently loses a declared entry (or injects an undeclared one)
    # must fail the run — the spine rule: a declaration in the hashed closure
    # that is neither loaded nor reported fails. Runs as a subprocess whose
    # exit status is asserted; the defect itself is injected by patching the
    # loader inside that subprocess (a real tree cannot express a loader
    # defect — task 11's suite pins the same invariant the same way).
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    tool = Path(__file__).with_name("validate-schemas.py")
    with td:
        import subprocess

        drop = (
            "import importlib.util, sys\n"
            f"spec = importlib.util.spec_from_file_location('vs', {str(tool)!r})\n"
            "vs = importlib.util.module_from_spec(spec); spec.loader.exec_module(vs)\n"
            "cvc = vs._volatility_conformance()\n"
            "_orig = cvc.load_family_fields\n"
            "def defect(family, schemas_dir):\n"
            "    fields, errors = _orig(family, schemas_dir)\n"
            "    return [f for f in fields if f.name != 'identifier'], errors\n"
            "cvc.load_family_fields = defect\n"
            "sys.exit(vs.main())\n"
        )
        proc = subprocess.run(
            [sys.executable, "-c", drop, "--volatility=cmis", str(Path(td.name) / "schemas")],
            capture_output=True, text=True,
        )
        out = proc.stdout + proc.stderr
        assert proc.returncode != 0, out
        assert any(
            "unaccounted_input" in line and "neither loaded nor reported" in line
            for line in out.splitlines()
        ), out
        phantom = (
            "import copy, importlib.util, sys\n"
            f"spec = importlib.util.spec_from_file_location('vs', {str(tool)!r})\n"
            "vs = importlib.util.module_from_spec(spec); spec.loader.exec_module(vs)\n"
            "cvc = vs._volatility_conformance()\n"
            "_orig = cvc.load_family_fields\n"
            "def defect(family, schemas_dir):\n"
            "    fields, errors = _orig(family, schemas_dir)\n"
            "    phantom = copy.deepcopy(fields[0])\n"
            "    phantom.decl = phantom.decl.rsplit('#', 1)[0] + '#999'\n"
            "    phantom.uid = phantom.decl\n"
            "    return fields + [phantom], errors\n"
            "cvc.load_family_fields = defect\n"
            "sys.exit(vs.main())\n"
        )
        proc = subprocess.run(
            [sys.executable, "-c", phantom, "--volatility=cmis", str(Path(td.name) / "schemas")],
            capture_output=True, text=True,
        )
        out = proc.stdout + proc.stderr
        assert proc.returncode != 0, out
        assert any(
            "unaccounted_input" in line and "is not declared by the hashed schema closure" in line
            for line in out.splitlines()
        ), out


def check_volatility_bare_flag_rejected():
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        rc, out, _ = _run_main(str(Path(td.name) / "schemas"), "--volatility")
        assert rc == 1, out
        assert any("--volatility requires" in line for line in out.splitlines()), out


def check_volatility_foreign_inputs_left_to_their_own_pass():
    if _SF is None:
        return
    # Blocker 2, fix 2 — pin skip path `_volatility_foreign`: c-cmis-1.4
    # imports cmis's index; the imported family's pages and fragments are
    # cmis's own memory map, accounted by the cmis pass — not unaccounted
    # inputs of the c-cmis-1.4 pass. Both directions are pinned: the
    # importing pass stays silent even about a real violation in the
    # imported map, and the imported family's own pass fails on it.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        cmis = root / "schemas" / "cmis"
        (cmis / "fields").mkdir(parents=True)
        (cmis / "_index.yaml").write_text(
            _vol_index("cmis", "5.3"), encoding="utf-8"
        )
        (cmis / "page.yaml").write_text(_vol_page(2), encoding="utf-8")
        # Missing access marking: a real violation, visible to cmis's pass.
        (cmis / "fields" / "part.yaml").write_text(
            "fields:\n"
            "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
            "    flags: { is_dynamic: false }\n"
            "  - name: reserved\n    offset: 1\n    size_bytes: 1\n    type: u8\n"
            "    access: unknown\n    flags: { is_dynamic: false }\n",
            encoding="utf-8",
        )
        ccmis = root / "schemas" / "c-cmis-1.4"
        (ccmis / "fields").mkdir(parents=True)
        (ccmis / "_index.yaml").write_text(
            _vol_index("c-cmis-1.4", "01.4").replace(
                "includes:\n", "imports:\n  - ../cmis/_index.yaml\nincludes:\n"
            ),
            encoding="utf-8",
        )
        (ccmis / "page.yaml").write_text(_vol_page(2), encoding="utf-8")
        (ccmis / "fields" / "part.yaml").write_text(
            "fields:\n"
            "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    type: u8\n"
            "    access: ro\n    flags: { is_dynamic: false }\n"
            "  - name: reserved\n    offset: 1\n    size_bytes: 1\n    type: u8\n"
            "    access: unknown\n    flags: { is_dynamic: false }\n",
            encoding="utf-8",
        )
        failures: list[str] = []
        vs.run_volatility_pass(root / "schemas", ["c-cmis-1.4"], failures)
        # Direction 1: the importing pass reports nothing about the imported
        # family's inputs — the foreign skip exists precisely because cmis's
        # own pass accounts the same hashed files.
        assert not [f for f in failures if "cmis/" in f], failures
        # Direction 2: cmis's own pass fails on the planted violation (as the
        # tool, from a subprocess, with a nonzero exit status), so the skip
        # conceals nothing the family set leaves unaccounted.
        rc, out = _run_tool(str(root / "schemas"), "--volatility=cmis")
        assert rc != 0, out
        assert any("missing_marking" in l for l in out.splitlines()), out


def check_volatility_reported_file_skip_is_safe():
    # Blocker 2, fix 2 — pin skip path `d["file"] in reported_file`: a
    # non-page include carrying fields produces a load FAIL naming the file
    # (its fields would be silently dropped), and the run fails on that
    # FAIL; the accounting loop then skips the file's declared entries
    # because re-reporting them adds no signal. The pin: the skip cannot
    # hide a failure — the load error that triggers it IS one.
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        index = Path(td.name) / "schemas" / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "includes:\n", "includes:\n  - extra.yaml\n"
            ),
            encoding="utf-8",
        )
        (Path(td.name) / "schemas" / "cmis" / "extra.yaml").write_text(
            "schema: openxcvr.enum.v1\nkind: enum\nid: Extra\nfields:\n"
            "  - name: smuggled\n    offset: 20\n    size_bytes: 1\n    type: u8\n"
            "    access: ro\n    flags: { is_dynamic: false }\n",
            encoding="utf-8",
        )
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc != 0, out
        assert any(
            "skipped_field" in line and "extra.yaml" in line
            and "not kind: page" in line
            for line in out.splitlines()
        ), out
        # The declared entries of the reported file are suppressed — the
        # load FAIL above is the failure of record for them.
        assert not [
            l for l in out.splitlines()
            if "neither loaded nor reported" in l and "extra.yaml" in l
        ], out


def check_volatility_reported_page_skip_is_safe():
    # Blocker 2, fix 2 — pin skip path `any(pg in reported_file ...)`: a
    # page whose `fields` payload is not a list produces a load FAIL naming
    # the page; the entries its field_includes fragment declares are
    # skipped via the pages-match because the loader cannot read past that
    # page error. The pin: the run still fails on the page's own error.
    if _SF is None:
        return
    td = _vol_tree("cmis", "".join(_vol_clean_fields()), templates=_VOL_TEMPLATES)
    with td:
        page = Path(td.name) / "schemas" / "cmis" / "page.yaml"
        page.write_text(
            page.read_text(encoding="utf-8").replace(
                "fields: []\n", "fields:\n  broken: mapping\n"
            ),
            encoding="utf-8",
        )
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=cmis")
        assert rc != 0, out
        assert any(
            "skipped_field" in line and "fields is not a list" in line
            for line in out.splitlines()
        ), out
        # The fragment's entries are suppressed by the pages-match, not
        # lost: the page FAIL is the failure of record for everything the
        # loader could not read past.
        assert not [
            l for l in out.splitlines()
            if "neither loaded nor reported" in l and "part.yaml" in l
        ], out


def check_volatility_upper_page_range_covered_and_holed():
    # Advisory (adopted): the base-128 upper-page byte range — where task 19
    # will make the gate mandatory — must be exercised. Flat offsets on a
    # `page_address: 01h` 128-byte page: the clean form passes; a removed
    # byte and a removed bit each fail with the upper-page offset named.
    if _SF is None:
        return
    clean = (
        "  - name: identifier\n    offset: 128\n    size_bytes: 1\n    type: u8\n"
        "    access: ro\n    flags: { is_dynamic: false }\n"
        "  - name: reserved_129\n    offset: 129\n    size_bytes: 1\n    type: u8\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: flag_bit0\n    offset: 130\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: flag_rest\n    offset: 130\n    bit_offset: 1\n    bit_width: 7\n"
        "    type: u7\n    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: reserved_131\n    offset: 131\n    size_bytes: 1\n    type: u8\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
        "  - name: telemetry\n    offset: 132\n    size_bytes: 6\n    type: bytes\n"
        "    access: ro\n    flags: { is_dynamic: true }\n"
        "  - name: reserved_tail\n    offset: 138\n    size_bytes: 118\n    type: bytes\n"
        "    access: unknown\n    flags: { is_dynamic: false }\n"
    )
    td = _vol_tree("sff-8636", clean, revision="2.11", size=128, address="01h")
    with td:
        failures = _vol_run(td, "sff-8636")
        assert not failures, failures
    no_byte = clean.replace(
        "  - name: telemetry\n    offset: 132\n    size_bytes: 6\n    type: bytes\n"
        "    access: ro\n    flags: { is_dynamic: true }\n",
        "",
    )
    td = _vol_tree("sff-8636", no_byte, revision="2.11", size=128, address="01h")
    with td:
        failures = _vol_run(td, "sff-8636")
        assert any("uncovered_byte" in f and "byte 132" in f for f in failures), failures
        rc, out = _run_tool(str(Path(td.name) / "schemas"), "--volatility=sff-8636")
        assert rc != 0 and any("byte 132" in l for l in out.splitlines()), out
    no_bit = clean.replace(
        "  - name: flag_bit0\n    offset: 130\n    bit_offset: 0\n    bit_width: 1\n"
        "    type: bool\n    access: ro\n    flags: { is_dynamic: true }\n",
        "",
    )
    td = _vol_tree("sff-8636", no_bit, revision="2.11", size=128, address="01h")
    with td:
        failures = _vol_run(td, "sff-8636")
        assert any("uncovered_bit" in f and "byte 130" in f and "[0]" in f for f in failures), failures


def _vol_receipt_dir(root: Path) -> Path:
    path = root / "tests" / "volatility-conformance"
    path.mkdir(parents=True, exist_ok=True)
    return path


def check_volatility_fresh_receipt_not_flagged():
    if _SF is None:
        return
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _vol_cmis_receipt(root)
        failures: list[str] = []
        vs.run_volatility_pass(root / "schemas", ["cmis"], failures)
        receipt_failures = [f for f in failures if "receipt" in f]
        assert not receipt_failures, receipt_failures


def check_volatility_receipt_recording_mismatch_fails():
    if _SF is None:
        return
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _SF.cmis_schema(root)
        # no standards dir: the receipt honestly records missing-standard
        # mismatches instead of a silent pass
        _vol_write(
            root / "authority"
            / vs._volatility_conformance().AUTHORITY_REPORT_FILES["cmis"],
            json.dumps(_SF.cmis_authority_report()),
        )
        _vol_cli(
            "--write", "--family", "cmis",
            "--schemas-dir", str(root / "schemas"),
            "--standards-dir", str(root / "nonexistent"),
            "--receipts-dir", str(_vol_receipt_dir(root)),
            "--authority-dir", str(root / "authority"),
            "--repo-root", str(root),
        )
        failures: list[str] = []
        vs.run_volatility_pass(root / "schemas", ["cmis"], failures)
        assert any("records" in f and "mismatch" in f for f in failures), failures


def check_volatility_stale_receipt_from_bit_level_effect_fails():
    if _SF is None:
        return
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _vol_cmis_receipt(root)
        _vol_cli(
            "--write", "--family", "cmis",
            "--schemas-dir", str(root / "schemas"),
            "--standards-dir", str(root / "standards"),
            "--receipts-dir", str(_vol_receipt_dir(root)),
            "--authority-dir", str(root / "authority"),
            "--repo-root", str(root),
        )
        # Change ONLY a bit-level read effect: the schema closure changes, so
        # the committed receipt is stale even though nothing moved.
        part = root / "schemas" / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = part.read_text(encoding="utf-8")
        old = "  - name: identifier\n    offset: 0\n    size_bytes: 1\n    template: inherited_ro\n"
        assert old in text
        part.write_text(
            text.replace(
                old,
                old + "    read_effect: unknown\n    read_effect_scope: field\n",
            ),
            encoding="utf-8",
        )
        failures: list[str] = []
        vs.run_volatility_pass(root / "schemas", ["cmis"], failures)
        assert any("stale" in f and "receipt" in f for f in failures), failures


def _vol_cmis_receipt(root: Path) -> None:
    """Write a conformant synthetic cmis receipt (real fixture corpus)."""
    cvc = vs._volatility_conformance()
    _SF.cmis_schema(root)
    for rev in cvc.CMIS_REVISIONS:
        _vol_write(
            root / "standards" / cvc.STANDARD_FILENAMES[("cmis", rev)],
            _SF.cmis_standard(rev),
        )
    _vol_write(
        root / "authority" / cvc.AUTHORITY_REPORT_FILES["cmis"],
        json.dumps(_SF.cmis_authority_report()),
    )
    _vol_cli(
        "--write", "--family", "cmis",
        "--schemas-dir", str(root / "schemas"),
        "--standards-dir", str(root / "standards"),
        "--receipts-dir", str(_vol_receipt_dir(root)),
        "--authority-dir", str(root / "authority"),
        "--repo-root", str(root),
    )


def _vol_write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _vol_cli(*argv: str) -> int:
    import argparse

    cvc = vs._volatility_conformance()
    buf_out, buf_err = io.StringIO(), io.StringIO()
    ns = argparse.Namespace(
        write=True,
        check=False,
        family=argv[argv.index("--family") + 1],
        schemas_dir=argv[argv.index("--schemas-dir") + 1],
        standards_dir=argv[argv.index("--standards-dir") + 1],
        receipts_dir=argv[argv.index("--receipts-dir") + 1],
        authority_dir=argv[argv.index("--authority-dir") + 1],
        repo_root=argv[argv.index("--repo-root") + 1],
        verbose=False,
    )
    with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
        return cvc.cmd_write(ns)


def check_gaps_reports_intrabyte_hole():
    if _SF is None:
        return
    doc = {
        "kind": "page",
        "page_address": "lower",
        "size_bytes": 8,
        "fields": [
            {"name": "a", "offset": 0, "size_bytes": 1, "type": "u8"},
            {"name": "hi", "offset": 5, "bit_offset": 0, "bit_width": 4, "type": "u4"},
            {"name": "tail", "offset": 6, "size_bytes": 2, "type": "bytes"},
        ],
    }
    warns, adv = [], []
    vs.validate_page_offsets(FAKE, doc, warns, adv, report_gaps=True)
    assert any(
        "intra-byte hole" in w and "byte 5" in w and "4, 5, 6, 7" in w for w in warns
    ), warns


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("check_")]
    for t in tests:
        t()
        print(f"ok  {t.__name__}")
    print(f"{len(tests)} passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
