#!/usr/bin/env python3
"""
Distill a raw transceiver-module capture into commit-ready synthetic-fixture
inputs, scrubbed of vendor identity so real-module evidence can grow the
golden corpus WITHOUT vendor-derived captures landing in this open
repository. Vendor-derived captures do not belong here.

Workflow
--------
1. Capture a real module's EEPROM slices privately (outside this repo),
   one file per memory slice, named after the fixture convention:
   ``<stem>.a0h.bin`` / ``<stem>.a2h.bin`` (SFF-8472),
   ``<stem>.lower.bin`` / ``<stem>.page-NNh.bin`` (SFF-8636, CMIS).
   The raw capture is the operator's private evidence: it must never be
   committed, pushed, or otherwise published.
2. Run this tool against each slice, naming the schema identity:
       python3 tools/distill-capture.py \
           --spec sff-8472 --input /private/captures/xyz.a0h.bin \
           --name synthetic-sff8472-mydistilled-module \
           --outdir tests/fixtures/sff-8472
   The slice is derived from the input filename suffix; ``--slice``
   overrides it. For an SFF-8472 A2h capture, declare which A2h page the
   128-255 region holds with ``--a2h-page 00h|01h|02h|03h``, or the whole
   paged region is zeroed conservatively.
3. The tool writes, into ``--outdir``:
   - ``<name>.<slice>.bin`` — the scrubbed bytes, byte length identical to
     the capture (layout preserved);
   - ``<name>.<slice>.distill.json`` — the scrub report: which schema
     fields were replaced at which offsets, which checksums were
     recomputed, and the output digest. It never contains original bytes
     or the input path.
   With ``--provenance PATH`` it also writes a provenance stub for the
   corpus's per-fixture ``.md`` convention.
4. Review the report, then commit the scrubbed ``.bin`` (+ provenance
   ``.md``) and use them like any other fixture: distill first, regenerate
   goldens from the scrubbed bytes.

What gets scrubbed (schema-driven, never guessed)
-------------------------------------------------
Vendor-identity fields — schema fields whose ``template`` is ``vendor_info``
or whose ``tags`` contain ``Information::Vendor`` (VendorName, VendorOUI,
VendorPN, VendorRev, VendorSN, DateCode, ...; plus CMIS ``clei_code`` as one
documented privacy exception) — are replaced with the ``--marker`` text
(default ``SYNTHETIC``, ASCII space-padded like the specs require) for
``ascii`` fields, or zeroed for non-ascii fields (VendorOUI).
Opaque private regions — ``template`` ``vendor_specific`` or ``storage``
(vendor-specific bytes, user EEPROM) — are zeroed. Everything else is
preserved byte-for-byte. Checksum fields (``encoding.kind: checksum``,
``algorithm: sum8``) whose schema-declared ``covers_range`` intersects a
scrubbed range are recomputed, so the scrubbed fixture still decodes with
valid checksums; untouched checksums are verified and any mismatch is
recorded in the report, never silently patched.

The tool fails closed: an unreadable input, a size that disagrees with the
schema's ``size_bytes``, an unknown spec/slice/page, or a vendor range that
escapes the capture is an error, and nothing is written. It never writes
the raw capture anywhere.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = ROOT / "schemas"

SPECS = ("sff-8472", "sff-8636", "cmis")

HEX_PAGE_RE = re.compile(r"^([0-9A-Fa-f]{2})h$")


class DistillError(Exception):
    """Fail-closed error: nothing is written when this is raised."""


# ---------------------------------------------------------------------------
# Schema loading
# ---------------------------------------------------------------------------

def load_yaml(path: Path) -> dict:
    try:
        with path.open("rb") as fh:
            data = yaml.safe_load(fh)
    except OSError as exc:
        raise DistillError(f"cannot read schema {path}: {exc}") from exc
    except yaml.YAMLError as exc:
        raise DistillError(f"cannot parse schema {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise DistillError(f"schema {path} is not a mapping")
    return data


def page_base(page: dict, path: Path) -> int:
    """File-offset base for a page schema's table-numbered offsets.

    ``lower``/``A0h``/``A2h`` pages number bytes from 0; every upper page
    (``NNh``) numbers its bytes 128-255 while its file slice starts at 0.
    """
    address = page.get("page_address")
    if address in (None, "lower", "A0h", "A2h"):
        return 0
    if HEX_PAGE_RE.match(str(address)):
        return 128
    raise DistillError(f"schema {path}: unknown page_address {address!r}")


def find_page_schemas(spec: str, slice_name: str, a2h_page: str | None) -> list[Path]:
    """Schema files describing one capture slice. Exactly-known, or error."""
    spec_dir = SCHEMAS / spec
    if not spec_dir.is_dir():
        raise DistillError(f"unknown spec {spec!r} (expected one of {', '.join(SPECS)})")
    if spec == "sff-8472":
        if slice_name == "a0h":
            return [spec_dir / "page-a0h-base-memory.yaml"]
        if slice_name == "a2h":
            paths = [spec_dir / "page-a2h-diagnostics-base.yaml"]
            if a2h_page is not None:
                matches = [
                    p for p in sorted(spec_dir.glob("page-a2h-*.yaml"))
                    if load_yaml(p).get("page_select") == a2h_page
                ]
                if len(matches) != 1:
                    raise DistillError(
                        f"a2h page {a2h_page!r} does not name exactly one schema "
                        f"(matched {len(matches)} page-a2h-*.yaml files)")
                paths.append(matches[0])
            return paths
        raise DistillError(
            f"unknown sff-8472 slice {slice_name!r} (expected a0h or a2h)")
    if slice_name == "lower":
        return [spec_dir / "lower-memory.yaml"]
    if HEX_PAGE_RE.match(slice_name):
        matches = [
            p for p in sorted(spec_dir.glob("page-*.yaml"))
            if str(load_yaml(p).get("page_address", "")).lower() == slice_name.lower()
        ]
        if len(matches) != 1:
            raise DistillError(
                f"no unique page schema with page_address {slice_name!r} in "
                f"schemas/{spec} (matched {len(matches)})")
        return matches
    raise DistillError(
        f"unknown slice {slice_name!r} (expected lower or an NNh hex page)")


def iter_fields(spec_dir: Path, page_path: Path, seen: set[Path] | None = None):
    """Yield (page, field) pairs for a page schema, following field_includes."""
    seen = seen if seen is not None else set()
    if page_path in seen:
        return
    seen.add(page_path)
    page = load_yaml(page_path)
    for field in page.get("fields") or []:
        yield page, field
    for include in page.get("field_includes") or []:
        yield from iter_fields(spec_dir, (spec_dir / include).resolve(), seen)


def slice_suffix(slice_name: str) -> str:
    if slice_name in ("a0h", "a2h", "lower"):
        return f".{slice_name}.bin"
    return f".page-{slice_name}.bin"


def slice_from_filename(path: Path) -> str | None:
    for suffix in ("a0h", "a2h", "lower"):
        if path.name.endswith(f".{suffix}.bin"):
            return suffix
    m = re.search(r"\.page-([0-9A-Fa-f]{2}h)\.bin$", path.name)
    return m.group(1) if m else None


# ---------------------------------------------------------------------------
# Scrub planning
# ---------------------------------------------------------------------------

def classify_field(field: dict) -> str | None:
    """Schema-grounded vendor-data classification for one field.

    Identity: ``template: vendor_info`` or ``tags`` containing
    ``Information::Vendor``. Opaque: ``vendor_specific`` / ``storage``.
    One documented privacy exception beyond the schema signals: CMIS
    ``clei_code`` — its own schema provenance quote says it "contains the
    vendor's CLEI code" (a product-identifying code), but the field is
    tagged only ``Information``. The distiller scrubs it rather than
    leaking a real CLEI; amending the schema tag is an owner decision.
    """
    template = field.get("template")
    tags = field.get("tags") or []
    if (template == "vendor_info" or "Information::Vendor" in tags
            or field.get("name") == "clei_code"):
        return "identity"
    if template in ("vendor_specific", "storage"):
        return "opaque"
    return None


class Range:
    __slots__ = ("start", "end", "policy", "field", "action")

    def __init__(self, start: int, end: int, policy: str, field: str, action: str):
        self.start, self.end = start, end
        self.policy, self.field, self.action = policy, field, action


def plan_scrub(spec_dir: Path, page_paths: list[Path], capture_len: int,
               schema_size: int) -> tuple[list[Range], list[str]]:
    """Vendor ranges to scrub plus ranges absent from a prefix capture.

    A capture may be a strict prefix of the schema's page (a 128-byte A0h
    read of the 256-byte map is normal). A vendor range fully outside the
    capture is recorded as absent; a range cut in half by the capture
    boundary is an error — the captured half would leak vendor bytes.
    """
    ranges: list[Range] = []
    absent: list[str] = []
    for page_path in page_paths:
        base = page_base(load_yaml(page_path), page_path)
        for _page, field in iter_fields(spec_dir, page_path):
            policy = classify_field(field)
            if policy is None:
                continue
            offset = field.get("offset")
            size = field.get("size_bytes")
            if offset is None or size is None:
                continue  # bit fields / computed entries carry no byte range
            start, end = offset - base, offset - base + size
            if end <= start or start >= schema_size:
                continue
            if end > schema_size:
                raise DistillError(
                    f"vendor range {field.get('name')!r} (bytes {start}-{end - 1}) "
                    f"escapes the schema's {schema_size}-byte page")
            if start >= capture_len:
                absent.append(str(field.get("name")))
                continue
            if end > capture_len:
                raise DistillError(
                    f"vendor range {field.get('name')!r} (bytes {start}-{end - 1}) "
                    f"is cut in half by the {capture_len}-byte capture: refusing "
                    f"to emit bytes that could still carry vendor data")
            if policy == "identity" and field.get("type") == "ascii":
                action = "marker"
            else:
                action = "zero"
            ranges.append(Range(start, end, policy, str(field.get("name")), action))
    ranges.sort(key=lambda r: (r.start, r.end))
    merged: list[Range] = []
    for r in ranges:
        if merged and r.start < merged[-1].end:
            # True overlap (e.g. a parent field and its decomposed children):
            # fold into one range; zeroing is the strictest action, so it wins.
            prev = merged[-1]
            prev.end = max(prev.end, r.end)
            if r.action == "zero" or prev.action == "zero":
                prev.action = "zero"
                prev.policy = "opaque"
        else:
            merged.append(Range(r.start, r.end, r.policy, r.field, r.action))
    return merged, absent


class Checksum:
    __slots__ = ("offset", "start", "end", "field")

    def __init__(self, offset: int, start: int, end: int, field: str):
        self.offset, self.start, self.end, self.field = offset, start, end, field


def find_checksums(spec_dir: Path, page_paths: list[Path]) -> list[Checksum]:
    out: list[Checksum] = []
    for page_path in page_paths:
        base = page_base(load_yaml(page_path), page_path)
        for _page, field in iter_fields(spec_dir, page_path):
            encoding = field.get("encoding") or {}
            if encoding.get("kind") != "checksum":
                continue
            if encoding.get("algorithm") != "sum8":
                raise DistillError(
                    f"checksum {field.get('name')!r} uses unsupported algorithm "
                    f"{encoding.get('algorithm')!r}")
            covers = encoding.get("covers_range")
            offset = field.get("offset")
            if not covers or offset is None:
                continue
            out.append(Checksum(
                offset - base, covers[0] - base, covers[1] - base + 1,
                str(field.get("name"))))
    return out


def apply_scrub(data: bytearray, ranges: list[Range], marker: bytes) -> None:
    for r in ranges:
        if r.action == "marker":
            replacement = marker[: r.end - r.start].ljust(r.end - r.start, b" ")
        else:
            replacement = b"\x00" * (r.end - r.start)
        data[r.start:r.end] = replacement


def recompute_checksums(data: bytearray, checksums: list[Checksum],
                        ranges: list[Range]) -> list[Checksum]:
    recomputed = []
    for c in checksums:
        if not any(c.start < r.end and r.start < c.end for r in ranges):
            continue
        data[c.offset] = sum(data[c.start:c.end]) & 0xFF
        recomputed.append(c)
    return recomputed


def verify_checksums(data: bytearray, checksums: list[Checksum],
                     skip: list[Checksum]) -> list[Checksum]:
    mismatched = []
    for c in checksums:
        if c in skip:
            continue
        if data[c.offset] != sum(data[c.start:c.end]) & 0xFF:
            mismatched.append(c)
    return mismatched


# ---------------------------------------------------------------------------
# Distillation
# ---------------------------------------------------------------------------

def distill(spec: str, slice_name: str, raw: bytes, marker: str,
            a2h_page: str | None) -> tuple[bytearray, dict]:
    if not raw:
        raise DistillError("capture is empty")
    page_paths = find_page_schemas(spec, slice_name, a2h_page)
    spec_dir = SCHEMAS / spec
    pages = [load_yaml(p) for p in page_paths]
    expected = pages[0].get("size_bytes")
    if not isinstance(expected, int) or expected <= 0:
        raise DistillError(f"schema {page_paths[0]} has no usable size_bytes")
    if len(raw) > expected:
        raise DistillError(
            f"capture is {len(raw)} bytes but schema {page_paths[0].name} "
            f"declares size_bytes={expected}; refusing to guess the layout")
    marker_bytes = marker.encode("ascii", errors="strict")
    if not marker_bytes or len(marker_bytes) > 64:
        raise DistillError("marker must be 1-64 ASCII characters")

    ranges, absent = plan_scrub(spec_dir, page_paths, len(raw), expected)
    if spec == "sff-8472" and slice_name == "a2h" and a2h_page is None:
        # The selected A2h page is unknown: its 128-255 region could hold
        # customer user EEPROM, so it is zeroed conservatively. Declare the
        # page with --a2h-page to scrub it per its page schema instead.
        end = min(256, len(raw))
        if end > 128:
            ranges.append(Range(128, end, "opaque", "a2h_paged_region_undeclared", "zero"))
    data = bytearray(raw)
    apply_scrub(data, ranges, marker_bytes)
    checksums = find_checksums(spec_dir, page_paths)
    in_capture = [
        c for c in checksums
        if c.end <= len(raw) and c.offset < len(raw)
    ]
    recomputed = recompute_checksums(data, in_capture, ranges)
    mismatched = verify_checksums(data, in_capture, recomputed)

    report = {
        "tool": "tools/distill-capture.py",
        "schema": {
            "spec": spec,
            "revision": pages[0].get("spec", {}).get("revision"),
            "schema_ids": [p.get("id") for p in pages],
            "page_address": pages[0].get("page_address"),
            "a2h_page": a2h_page,
        },
        "slice": slice_name,
        "suffix": slice_suffix(slice_name),
        "size_bytes": len(data),
        "marker": marker,
        "scrubbed": [
            {
                "field": r.field,
                "offset": r.start,
                "size_bytes": r.end - r.start,
                "policy": r.policy,
                "action": "marker" if r.action == "marker" else "zero",
            }
            for r in ranges
        ],
        "checksums_recomputed": [c.field for c in recomputed],
        "checksums_preserved_mismatch": [c.field for c in mismatched],
        "ranges_absent_from_capture": absent,
        "notes": [
            "Raw capture bytes are never written to the output or this report.",
            "Commit the scrubbed .bin and this report; the raw capture stays "
            "private and never lands in this repository.",
        ],
    }
    return data, report


PROVENANCE_STUB = """\
# Fixture: {name} (distilled — provenance stub)

**Source:** Private raw module capture, distilled by `tools/distill-capture.py`
(see `tests/fixtures/README.md` § Distilling real captures).
**Spec:** {spec} {revision}
**Slice:** {suffix}

The raw capture is operator-private evidence and is **not** in this repository
(vendor-derived captures do not belong in this repository). Every vendor-identity field
(schema `vendor_info` template / `Information::Vendor` tag) was replaced with
the synthetic marker `{marker}`; opaque vendor-specific / user-EEPROM regions
were zeroed; schema-declared checksums intersecting a scrubbed range were
recomputed. The byte-for-byte scrub list is in `{name}{suffix}.distill.json`.

Before committing, complete this stub with the module description, and verify
by hex dump that no original vendor identity string remains.
"""


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Distill a raw module capture into scrubbed synthetic-fixture "
                    "builder inputs (vendor identity replaced, layout preserved).")
    parser.add_argument("--spec", required=True, choices=SPECS,
                        help="schema identity: which spec's schemas describe the capture")
    parser.add_argument("--input", required=True, type=Path,
                        help="raw capture file (private; never committed)")
    parser.add_argument("--name", required=True,
                        help="fixture stem for the emitted files")
    parser.add_argument("--outdir", required=True, type=Path,
                        help="directory for the scrubbed outputs")
    parser.add_argument("--slice",
                        help="memory slice (a0h/a2h/lower/NNh); derived from the "
                             "input filename suffix when omitted")
    parser.add_argument("--marker", default="SYNTHETIC",
                        help="synthetic ASCII marker for vendor-identity fields "
                             "(default: %(default)s)")
    parser.add_argument("--a2h-page", choices=("00h", "01h", "02h", "03h"),
                        help="sff-8472 a2h only: which A2h page the capture's "
                             "bytes 128-255 hold; without it the whole paged "
                             "region is zeroed conservatively")
    parser.add_argument("--provenance", type=Path,
                        help="also write a provenance stub to this path")
    args = parser.parse_args(argv)

    slice_name = args.slice or slice_from_filename(args.input)
    if slice_name is None:
        parser.error(
            f"cannot derive a slice from {args.input.name!r}; pass --slice "
            "(a0h, a2h, lower, or an NNh hex page)")
    if args.a2h_page is not None and not (args.spec == "sff-8472" and slice_name == "a2h"):
        parser.error("--a2h-page applies only to --spec sff-8472 --slice a2h")

    try:
        raw = args.input.read_bytes()
    except OSError as exc:
        print(f"distill-capture: cannot read capture {args.input}: {exc}",
              file=sys.stderr)
        return 1

    try:
        data, report = distill(args.spec, slice_name, raw, args.marker, args.a2h_page)
    except DistillError as exc:
        print(f"distill-capture: {exc}", file=sys.stderr)
        return 1

    suffix = report["suffix"]
    out_bin = args.outdir / f"{args.name}{suffix}"
    out_json = args.outdir / f"{args.name}{suffix}.distill.json"
    if args.input.resolve() in (out_bin.resolve(), out_json.resolve()):
        print("distill-capture: refusing to overwrite the raw capture",
              file=sys.stderr)
        return 1
    try:
        args.outdir.mkdir(parents=True, exist_ok=True)
        out_bin.write_bytes(data)
        report["output_sha256"] = hashlib.sha256(data).hexdigest()
        out_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        if args.provenance:
            args.provenance.write_text(PROVENANCE_STUB.format(
                name=args.name, spec=args.spec,
                revision=report["schema"]["revision"] or "",
                suffix=suffix, marker=args.marker), encoding="utf-8")
    except OSError as exc:
        print(f"distill-capture: cannot write outputs: {exc}", file=sys.stderr)
        return 1

    print(f"distilled {len(data)} bytes: {out_bin}")
    print(f"scrub report: {out_json}")
    print("the raw capture stays private — never commit, push or publish it",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
