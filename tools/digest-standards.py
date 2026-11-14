#!/usr/bin/env python3
# tools/digest-standards.py — digest + parse the CMIS/SFF specification PDFs.
#
# Pipeline: docling (PDF -> structured markdown, table-aware) + chonkie
# (RecursiveChunker over prose, markdown tables kept atomic).
#
# Reads PDFs from <stds-root> (default: the openxcvr pointer repo's
# docs/stds cache). Writes to <out> (default <stds-root>/.parsed):
#   markdown/<slug>.md      docling markdown export (human-readable)
#   chunks/<slug>.jsonl     one JSON object per chunk, with provenance
#   manifest.json          per-PDF record: provenance, sizes, counts, errors
#   corpus/<family>/<doc>/ AI-parsable corpus per document (see
#                           corpus/README.md in the output tree)
#   corpus/catalog.json     every document + per-family ordered revisions
#   corpus/.cache/<sha256>.json
#                           docling structured-document cache, keyed by the
#                           source PDF hash so reruns skip conversion
#
# For digitally-generated spec PDFs we disable OCR (the native text layer is
# authoritative) and keep table-structure extraction on (these are dense
# register maps — the whole reason to reach for docling over pdftotext).
#
# Usage:
#   tools/digest-standards.py --smoke            # one small PDF, verify pipeline
#   tools/digest-standards.py                    # all PDFs
#   tools/digest-standards.py --force            # re-parse even if outputs exist
#   tools/digest-standards.py --limit 3          # first N (after sorting)
#   tools/digest-standards.py --stds-root <dir> --out <dir>
#
# Requires the docling+chonkie venv. See README "Parsing the specs" section.
# `python3 -m unittest tools.test_digest_standards` exercises the corpus
# derivation on synthetic docling documents, without docling or a PDF.

from __future__ import annotations
import argparse
import hashlib
import json
import re
import sys
import time
import traceback
from dataclasses import dataclass, field
from pathlib import Path

# --- defaults ---------------------------------------------------------------
# PDFs live in the sibling `openxcvr` pointer repo's docs/stds/ (this tool
# lives in openxcvr-lib/tools/). Resolve the sibling checkout the same way the
# shell tools (extract-pdf-range.sh, watch-oif-ias.sh) do: honor OPENXCVR_ROOT,
# else look for an `openxcvr` checkout beside the main checkout of this
# repository, else fall back to a cwd-relative guess. The sibling is found from
# THIS FILE, not the process cwd, so the tool behaves identically whether run as
# `tools/digest-standards.py` from the repo root or via an absolute path from
# elsewhere.
_THIS = Path(__file__).resolve()
_LIB_ROOT = _THIS.parent.parent  # openxcvr-lib/


def _workspace_root() -> Path:
    """The directory holding the OpenXCVR checkouts.

    A linked worktree lives under its repository's `.worktrees/`, so the
    workspace is the parent of the main checkout that git's common directory
    names, not the parent of this checkout. Outside git, this checkout's parent.
    """
    import subprocess
    try:
        proc = subprocess.run(
            ["git", "-C", str(_LIB_ROOT), "rev-parse", "--path-format=absolute",
             "--git-common-dir"],
            capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return _LIB_ROOT.parent
    if proc.returncode != 0:
        return _LIB_ROOT.parent
    return Path(proc.stdout.strip()).parent.parent


def _resolve_stds_root() -> Path:
    import os
    explicit = os.environ.get("OPENXCVR_ROOT")
    if explicit:
        return Path(explicit) / "docs" / "stds"
    sibling = _workspace_root() / "openxcvr" / "docs" / "stds"
    if sibling.is_dir():
        return sibling
    # last resort: assume cwd is the openxcvr pointer repo root
    return Path.cwd() / "docs" / "stds"


DEFAULT_STDS_ROOT = _resolve_stds_root()
DEFAULT_OUT = DEFAULT_STDS_ROOT / ".parsed"

CHUNK_SIZE_CHARS = 1200
MIN_CHARS_PER_CHUNK = 24


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


# --- docling conversion -----------------------------------------------------
def build_converter():
    from docling.document_converter import DocumentConverter, PdfFormatOption
    from docling.datamodel.pipeline_options import (
        PdfPipelineOptions,
    )
    from docling.datamodel.base_models import InputFormat

    # Digital spec PDFs: skip OCR (native text layer is authoritative), keep
    # table-structure extraction (dense register maps). Leave all remote/VLM
    # enrichment off — no network model calls, deterministic output.
    pipe = PdfPipelineOptions(do_ocr=False, do_table_structure=True)
    fmt = PdfFormatOption(pipeline_options=pipe)
    return DocumentConverter(
        format_options={InputFormat.PDF: fmt}
    )


# --- markdown -> structured chunks ------------------------------------------
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*)$")
# A markdown table row starts and ends with | (allowing leading/trailing space).
TABLE_ROW_RE = re.compile(r"^\s*\|.*\|\s*$")
# A table separator row like |---|:---|
TABLE_SEP_RE = re.compile(r"^\s*\|?\s*:?-{2,}.*\|\s*$")


@dataclass
class Block:
    kind: str  # "prose" | "table"
    heading_path: list[str]
    text: str


def split_into_blocks(md: str) -> list[Block]:
    """Split docling markdown into heading-scoped prose/table blocks.

    Tables (consecutive |-delimited rows) become one atomic block each so
    chonkie never splits a register table mid-row. Heading lines are tracked
    as context, not emitted as standalone chunks.
    """
    blocks: list[Block] = []
    heading_path: list[str] = []
    prose_buf: list[str] = []
    table_buf: list[str] = []

    def flush_prose():
        if prose_buf:
            txt = "\n".join(prose_buf).strip()
            if txt:
                blocks.append(Block("prose", list(heading_path), txt))
            prose_buf.clear()

    def flush_table():
        if table_buf:
            txt = "\n".join(table_buf).rstrip("\n")
            if txt:
                blocks.append(Block("table", list(heading_path), txt))
            table_buf.clear()

    for line in md.splitlines():
        m = HEADING_RE.match(line)
        if m:
            flush_prose()
            flush_table()
            level = len(m.group(1))
            title = m.group(2).strip()
            heading_path = heading_path[: level - 1]
            # pad in case of skipped levels
            while len(heading_path) < level - 1:
                heading_path.append("")
            heading_path.append(title)
            continue
        if TABLE_ROW_RE.match(line) or TABLE_SEP_RE.match(line):
            flush_prose()
            table_buf.append(line)
            continue
        # non-table, non-heading line: close any open table, accumulate prose
        if table_buf:
            flush_table()
        prose_buf.append(line)
    flush_prose()
    flush_table()
    return blocks


def split_oversized_table(table_md: str, max_chars: int) -> list[str]:
    """Split a table block that exceeds max_chars along row boundaries,
    repeating the header + separator on each continuation so each piece is a
    standalone markdown table."""
    lines = table_md.splitlines()
    # find separator row to identify header
    sep_idx = None
    for i, ln in enumerate(lines[:3]):
        if TABLE_SEP_RE.match(ln):
            sep_idx = i
            break
    if sep_idx is None or sep_idx < 1:
        # no clean header: fall back to a naive char-split
        return [table_md[i : i + max_chars] for i in range(0, len(table_md), max_chars)]
    header = lines[: sep_idx + 1]
    body = lines[sep_idx + 1 :]
    pieces: list[str] = []
    cur = list(header)
    cur_len = sum(len(l) + 1 for l in cur)
    for row in body:
        if cur_len + len(row) + 1 > max_chars and len(cur) > len(header):
            pieces.append("\n".join(cur).rstrip("\n"))
            cur = list(header)
            cur_len = sum(len(l) + 1 for l in cur)
        cur.append(row)
        cur_len += len(row) + 1
    if any(r.strip() for r in cur[len(header) :]):
        pieces.append("\n".join(cur).rstrip("\n"))
    return pieces or ["\n".join(header).rstrip("\n")]


def chunk_blocks(blocks: list[Block], slug: str, max_chars: int):
    """Yield chunk dicts. Prose blocks go through chonkie RecursiveChunker;
    table blocks are kept atomic (split by row only if oversized)."""
    try:
        from chonkie import RecursiveChunker
    except Exception as e:  # pragma: no cover
        raise SystemExit(f"chonkie not importable: {e}")
    chunker = RecursiveChunker(
        chunk_size=max_chars, min_characters_per_chunk=MIN_CHARS_PER_CHUNK
    )
    idx = 0
    for blk in blocks:
        hp = " / ".join(blk.heading_path) if blk.heading_path else ""
        if blk.kind == "table":
            pieces = (
                [blk.text] if len(blk.text) <= max_chars else split_oversized_table(blk.text, max_chars)
            )
            for pi, piece in enumerate(pieces):
                yield {
                    "slug": slug,
                    "chunk_index": idx,
                    "block_type": "table",
                    "heading_path": hp,
                    "table_part": f"{pi+1}/{len(pieces)}" if len(pieces) > 1 else None,
                    "char_count": len(piece),
                    "text": piece,
                }
                idx += 1
        else:
            for ch in chunker.chunk(blk.text):
                txt = ch.text if hasattr(ch, "text") else str(ch)
                if not txt.strip():
                    continue
                yield {
                    "slug": slug,
                    "chunk_index": idx,
                    "block_type": "prose",
                    "heading_path": hp,
                    "table_part": None,
                    "char_count": len(txt),
                    "text": txt,
                }
                idx += 1


# --- AI-parsable corpus -------------------------------------------------------
# A second, structured view of the same PDFs, designed for agent querying:
# find a section, table or register row by document, revision, page, section
# number, table number, page address, byte offset or field name, and get the
# text with a page citation. Everything below derives from the docling
# structured document (plain dicts) — no docling import — so the corpus
# logic is testable with synthetic documents under the system python.
#
# Layout (all gitignored, derived from copyrighted PDFs — never committed):
#   corpus/<family>/<doc-slug>/document.json    metadata + revision history
#   corpus/<family>/<doc-slug>/document.md      clean reading text, page markers
#   corpus/<family>/<doc-slug>/sections.jsonl    one record per section heading
#   corpus/<family>/<doc-slug>/tables.jsonl     one record per logical table
#   corpus/<family>/<doc-slug>/registers.jsonl  normalized register-map rows
#   corpus/catalog.json                         all documents + family revisions
#   corpus/README.md                            format documentation for readers
#   corpus/.cache/<sha256>.json                 docling dict cache per source hash

CORPUS_FORMAT_VERSION = "1"
CORPUS_DOC_FILES = (
    "document.json", "document.md", "sections.jsonl", "tables.jsonl", "registers.jsonl",
)

# docling content that never carries specification semantics for readers.
_DROPPED_LABELS = {"page_header", "page_footer"}
# Margin line numbers (CMIS-LT and the CMIS Errata render every page with
# numbered lines) surface as bare-number text items; so do stray dashes.
_LINE_NOISE_RE = re.compile(r"^(?:\d{1,3}|[-\u2013\u2014])$")


@dataclass
class FlatItem:
    """One reading-order element of a docling document, flattened to plain data."""

    kind: str  # "text" | "table"
    label: str = ""  # docling label: title, section_header, paragraph, ...
    text: str = ""
    level: int = 0  # section_header level (docling's own, 1..6)
    page: int = 0  # first page carrying this item
    grid: list = field(default_factory=list)  # table rows: list[list[str]]
    header_rows: tuple = ()  # row indices docling marked as column headers
    caption_texts: tuple = ()  # docling-linked captions (table items)


def _cell_text(cell) -> str:
    if isinstance(cell, dict):
        return (cell.get("text") or "").strip()
    return str(cell or "").strip()


def _ref_index(ref: str) -> int:
    tail = ref.rsplit("/", 1)[-1]
    try:
        return int(tail)
    except ValueError:
        return -1


def _item_page(item: dict) -> int:
    provs = item.get("prov") or []
    pages = [p.get("page", p.get("page_no", 0)) for p in provs if isinstance(p, dict)]
    return min((p for p in pages if p), default=0)


def _ref_of(child) -> str:
    """docling body children are {"$ref": "#/texts/0"} dicts (export_to_dict)
    or plain "#/texts/0" strings; accept both."""
    if isinstance(child, dict):
        return child.get("$ref") or ""
    return child if isinstance(child, str) else ""


def flatten_docling_dict(d: dict) -> list[FlatItem]:
    """Docling structured-document dict -> ordered FlatItems.

    Walks body children (recursing into groups) in reading order, drops
    furniture (running headers/footers, image placeholders) and keeps every
    body element with its page number. Key-value items are rendered as text.
    """
    pools = {k: d.get(k, []) or [] for k in ("texts", "tables", "key_value_items", "groups")}
    items: list[FlatItem] = []

    def walk(children):
        for child in children or []:
            ref = _ref_of(child)
            if not ref.startswith("#/"):
                continue
            pool = ref[2:].split("/", 1)[0]
            idx = _ref_index(ref)
            if pool == "groups":
                if 0 <= idx < len(pools["groups"]):
                    walk(pools["groups"][idx].get("children"))
                continue
            if pool == "pictures":
                continue  # image placeholders are dropped from the corpus
            if pool not in ("texts", "tables", "key_value_items"):
                continue  # furniture/form_items pools never carry body content
            if not 0 <= idx < len(pools[pool]):
                continue
            it = pools[pool][idx]
            if it.get("content_layer") == "furniture" or it.get("label") in _DROPPED_LABELS:
                continue
            page = _item_page(it)
            label = it.get("label", "")
            if pool == "tables":
                grid = [
                    [_cell_text(c) for c in row] for row in (it.get("data") or {}).get("grid") or []
                ]
                # docling links captions to their table; they are not body
                # children, so emit them right before the table to keep the
                # reading text complete and give the table its printed caption.
                caps = []
                for cref in it.get("captions") or []:
                    citem = pools["texts"][_ref_index(_ref_of(cref))] \
                        if 0 <= _ref_index(_ref_of(cref)) < len(pools["texts"]) else None
                    if citem and (citem.get("text") or "").strip():
                        caps.append(citem["text"].strip())
                        items.append(FlatItem("text", label="linked_caption",
                                              text=citem["text"].strip(), page=page))
                items.append(FlatItem("table", label=label, page=page, grid=grid,
                                      header_rows=_table_header_rows(it),
                                      caption_texts=tuple(caps)))
                continue
            if pool == "key_value_items":
                # rare in these specs; keep the printed key text
                key = (it.get("key") or {}).get("text", "") if isinstance(it.get("key"), dict) else ""
                items.append(FlatItem("text", label="key_value", text=key.strip(), page=page))
                continue
            text = (it.get("text") or "").strip()
            if label != "section_header" and _LINE_NOISE_RE.match(text):
                continue
            items.append(FlatItem(
                "text", label=label, text=text,
                level=int(it.get("level") or 0), page=page,
            ))

    walk((d.get("body") or {}).get("children"))
    return _drop_running_lines(items)


# Running page furniture that docling classifies as body text in some PDFs
# (e.g. "www.oiforum.com IA OIF-C-CMIS-01.4" on every page): a short text
# that repeats identically on several pages is never specification content.
_REPEAT_PAGES = 4


def _drop_running_lines(items: list[FlatItem]) -> list[FlatItem]:
    from collections import defaultdict

    pages_of = defaultdict(set)
    for it in items:
        if it.kind == "text" and it.label != "section_header" and 0 < len(it.text) <= 120:
            pages_of[it.text].add(it.page)
    running = {t for t, ps in pages_of.items() if len(ps) >= _REPEAT_PAGES}
    if not running:
        return items
    return [
        it for it in items
        if not (it.kind == "text" and it.label != "section_header" and it.text in running)
    ]


def _table_header_rows(table_item: dict) -> tuple:
    """Row indices docling flagged as column headers (cell `column_header`
    flags in the grid, position-derived)."""
    rows = set()
    for i, row in enumerate((table_item.get("data") or {}).get("grid") or []):
        for c in row:
            if isinstance(c, dict) and (c.get("column_header") or c.get("row_header")):
                rows.add(i)
                break
    return tuple(sorted(rows))


def page_count_of(d: dict) -> int:
    return len(d.get("pages") or {})


# --- document.md --------------------------------------------------------------
def markdown_table_md(grid: list, header_rows: tuple = ()) -> list[str]:
    """Render a grid as a GitHub-markdown pipe table."""
    if not grid:
        return []
    ncols = max(len(r) for r in grid)
    rows = [(list(r) + [""] * ncols)[:ncols] for r in grid]
    hdr = header_rows[0] if header_rows else 0
    hdr = min(hdr, len(rows) - 1)

    def emit(cells):
        return "| " + " | ".join(c.replace("|", "\\|") for c in cells) + " |"

    out = [emit(r) for r in rows[:hdr]]
    out.append(emit(rows[hdr]))
    out.append("|" + " --- |" * ncols)
    out.extend(emit(r) for r in rows[hdr + 1 :])
    return out


def render_document_md(items: list[FlatItem]) -> str:
    """Clean reading text: page markers at every page boundary, numbered
    headings, tables inline; running headers/footers and image placeholders
    are already dropped by flatten_docling_dict."""
    lines: list[str] = []
    page = None
    for it in items:
        if it.page and it.page != page:
            lines.append(f"<!-- page {it.page} -->")
            lines.append("")
            page = it.page
        if it.kind == "table":
            lines.extend(markdown_table_md(it.grid, it.header_rows))
            lines.append("")
            continue
        t = it.text.strip()
        if not t:
            continue
        if it.label == "title":
            lines.append(f"# {t}")
        elif it.label == "section_header":
            lines.append("#" * min(max(it.level, 1), 6) + " " + t)
        elif it.label == "list_item":
            lines.append(f"- {t}")
        elif it.label == "key_value":
            lines.append(t)
        else:  # paragraph, caption, footnote, reference, formula, ...
            lines.append(t)
        lines.append("")
    return "\n".join(lines).strip() + "\n"


# --- sections -----------------------------------------------------------------
SECTION_NUM_RE = re.compile(r"^\s*(\d+(?:\.\d+)*)\.?\s+(.+)$")
ANNEX_RE = re.compile(r"^\s*Annex\s+([A-Z0-9]+)\b[.:)]?\s*(.*)$", re.IGNORECASE)
TABLE_CAPTION_RE = re.compile(
    r"^\s*Table\s+(\d+(?:\s*[-\u2013]\s*\d+)?|\d+)\s*[:.]?\s*(.*)$", re.IGNORECASE
)
TOC_TITLE_RE = re.compile(
    r"^(contents|table of contents|list of tables|list of figures|list of tables and figures)$",
    re.IGNORECASE,
)


def parse_section_title(text: str):
    """-> (id, title, depth) where id/depth are None for unnumbered headings."""
    t = text.strip()
    m = SECTION_NUM_RE.match(t)
    if m:
        return m.group(1), m.group(2).strip(), len(m.group(1).split("."))
    m = ANNEX_RE.match(t)
    if m:
        return f"Annex {m.group(1).upper()}", (m.group(2) or "").strip() or t, 1
    return None, t, None


def build_sections(items: list[FlatItem]) -> list[dict]:
    """One record per section heading:
    {id, title, level, page_start, page_end, parent, text}.

    `level` is the printed numbering depth when the heading is numbered,
    else docling's own heading level. A section spans until the next heading
    of the same or shallower level; `text` holds only its direct prose, not
    nested subsection text. `parent` is the id of the nearest enclosing
    section, null at the top.
    """
    headings = []
    for i, it in enumerate(items):
        if it.kind != "text" or it.label != "section_header":
            continue
        if TABLE_CAPTION_RE.match(it.text):  # table captions, not sections
            continue
        sid, title, depth = parse_section_title(it.text)
        level_key = depth if depth is not None else max(it.level, 1)
        headings.append({"index": i, "id": sid, "title": title, "level_key": level_key,
                         "level": depth if depth is not None else max(it.level, 1),
                         "page": it.page})
    records = []
    for h_i, h in enumerate(headings):
        # span: until the next heading at same-or-shallower level
        end_index = len(items)
        for h2 in headings[h_i + 1 :]:
            if h2["level_key"] <= h["level_key"]:
                end_index = h2["index"]
                break
        # parent: nearest preceding heading with a shallower level
        parent = None
        for h2 in reversed(headings[:h_i]):
            if h2["level_key"] < h["level_key"]:
                parent = h2["id"]
                break
        span = items[h["index"] + 1 : end_index]
        pages = [it.page for it in span if it.page]
        page_end = max(pages, default=h["page"])
        # direct prose: until the first deeper heading
        direct = []
        for it in span:
            if it.kind == "text" and it.label == "section_header" \
                    and not TABLE_CAPTION_RE.match(it.text):
                _, _, d = parse_section_title(it.text)
                if (d if d is not None else max(it.level, 1)) > h["level_key"]:
                    break
            if it.kind == "text" and it.text.strip() and it.label not in ("title",):
                direct.append(it.text.strip())
        records.append({
            "id": h["id"], "title": h["title"], "level": h["level"],
            "page_start": h["page"], "page_end": page_end,
            "parent": parent, "text": "\n\n".join(direct),
            "_heading_index": h["index"],  # internal; stripped before writing
            "_span_end": end_index,        # internal; stripped before writing
        })
    return records


# --- tables --------------------------------------------------------------------
PageAddr_RE = re.compile(r"(?i)\b(?:page|address)\s+([0-9a-f]{1,2}\s*h)\b")


def _norm_cells(cells) -> tuple:
    return tuple(re.sub(r"\s+", " ", (c or "").strip().lower()) for c in cells)


def _table_id_of(caption: str):
    m = TABLE_CAPTION_RE.match(caption or "")
    if not m:
        return None
    num = re.sub(r"\s+", "", m.group(1))
    return f"Table {num}"


def build_tables(items: list[FlatItem], sections: list[dict]) -> list[dict]:
    """One record per logical table, multi-page continuations merged:
    {id, caption, section, page_start, page_end, columns, rows, is_toc}.

    Merge rules (applied to consecutive tables in reading order):
      - same printed table id (a continuation repeating the caption, possibly
        with "(cont.)"), or
      - an uncaptioned table whose first row repeats the previous table's
        header row on an adjacent page.
    Repeated header rows inside a merged table are dropped. Each logical
    record also carries `parts` ({page_start, rows}) for per-part citation.
    """
    # innermost open section at each item index (single clean pass)
    hpos = [
        (i, (parse_section_title(it.text)[2] if parse_section_title(it.text)[2] is not None
             else max(it.level, 1)),
         parse_section_title(it.text)[0], parse_section_title(it.text)[1])
        for i, it in enumerate(items)
        if it.kind == "text" and it.label == "section_header"
        and not TABLE_CAPTION_RE.match(it.text)
    ]
    section_at: dict[int, str | None] = {}
    section_title_at: dict[int, str] = {}
    stack: list[tuple] = []
    hptr = 0
    for i, it in enumerate(items):
        while hptr < len(hpos) and hpos[hptr][0] == i:
            _, key, sid, stitle = hpos[hptr]
            while stack and stack[-1][1] >= key:
                stack.pop()
            stack.append((sid, key, stitle))
            hptr += 1
        if stack:
            section_at[i] = stack[-1][0]
            section_title_at[i] = stack[-1][2]

    merged: list[dict] = []
    for i, it in enumerate(items):
        if it.kind != "table" or not it.grid:
            continue
        caption = _caption_for(items, i, it)
        header_idx = min(it.header_rows[0] if it.header_rows else 0, len(it.grid) - 1)
        header = it.grid[header_idx]
        body = it.grid[header_idx + 1 :]
        rec = {
            "id": _table_id_of(caption), "caption": caption,
            "section": section_at.get(i), "section_title": section_title_at.get(i) or "",
            "page_start": it.page, "page_end": it.page,
            "columns": list(header), "rows": list(body), "header": list(header),
            "is_toc": it.label == "document_index"
                      or bool(TOC_TITLE_RE.match(section_title_at.get(i) or ""))
                      or bool(TOC_TITLE_RE.match(caption)),
            "parts": [{"page_start": it.page, "row_count": len(body)}],
        }
        prev = merged[-1] if merged else None
        if prev and _tables_mergeable(prev, rec):
            hdr_key = _norm_cells(prev["header"])
            kept = [r for r in rec["rows"]
                    if not (len(r) == len(prev["header"]) and _norm_cells(r) == hdr_key)]
            prev["rows"].extend(kept)
            prev["parts"].append({"page_start": rec["page_start"], "row_count": len(kept)})
            prev["page_end"] = max(prev["page_end"], rec["page_end"])
        else:
            merged.append(rec)
    return [{
        "id": t["id"], "caption": t["caption"], "section": t["section"],
        "section_title": t["section_title"],
        "page_start": t["page_start"], "page_end": t["page_end"],
        "columns": t["columns"], "rows": t["rows"], "is_toc": t["is_toc"],
        "parts": t["parts"],
    } for t in merged]


def _caption_for(items: list, i: int, it: FlatItem) -> str:
    """A table's printed "Table N ..." caption, by precedence:
    docling-linked captions; a caption line printed above the table (CMIS
    prints them as headings or plain text, linked ones as linked_caption);
    a loose docling caption item emitted right after the table (observed
    SFF reading order). A caption-labeled item that directly follows another
    table belongs to that table, not this one."""
    for cand in it.caption_texts:
        if TABLE_CAPTION_RE.match(cand):
            return cand
    if it.caption_texts:
        return it.caption_texts[0]
    prev = items[i - 1] if i > 0 else None
    if prev is not None and prev.kind == "text" and TABLE_CAPTION_RE.match(prev.text) \
            and (prev.label != "caption" or (i >= 2 and items[i - 2].kind != "table")):
        # a caption directly above the table introduces it — unless it sits
        # between two tables, where it belongs to the earlier one
        return prev.text.strip()
    nxt = items[i + 1] if i + 1 < len(items) else None
    if nxt is not None and nxt.kind == "text" and nxt.label == "caption" \
            and TABLE_CAPTION_RE.match(nxt.text) and (i == 0 or items[i - 1].kind != "table"):
        return nxt.text.strip()
    return ""


# A repeated header row holds short word-like column labels, not data
# values; used to keep data-shaped adjacent tables apart.
_HEADER_CELL_RE = re.compile(r"(?i)^[a-z][a-z0-9 /.()-]{0,30}$")


def _tables_mergeable(prev: dict, cur: dict) -> bool:
    if prev.get("is_toc") != cur.get("is_toc"):
        return False
    gap = cur["page_start"] - prev["page_end"]
    if prev.get("id") and cur.get("id") and prev["id"] == cur["id"]:
        return -2 <= gap <= 3  # same printed id: a repeated-caption continuation
    if not cur.get("id") and prev.get("rows") and not prev.get("caption"):
        if 0 <= gap <= 1 and _norm_cells(cur["columns"]) == _norm_cells(prev["columns"]) \
                and all(_HEADER_CELL_RE.match(c or "") for c in cur["columns"]):
            return True  # uncaptioned continuation repeating a word-like header row
    return False


# --- registers -----------------------------------------------------------------
# Column-role vocabulary observed across the cached families (CMIS, C-CMIS,
# CMIS-LT/VCS/FF, ELSFP, SFF-8636, SFF-8472). A table is treated as a
# register map when a byte-or-bits column and a name-or-description column
# are all present; everything else is left out of registers.jsonl.


def _hdr_norm(c: str) -> str:
    return re.sub(r"\s+", " ", (c or "").strip().lower()).rstrip(":")


def _role_of(cell: str):
    h = _hdr_norm(cell)
    if not h:
        return None
    if h.startswith("byte") or h in ("address", "addr", "offset", "byte map"):
        return "byte"
    if h.startswith("bit"):
        return "bits"
    if h in ("name", "field", "field name", "name/field", "byte name"):
        return "field"
    if h.startswith("description"):
        return "description"
    if h in ("type", "access", "ro", "rw", "r/w", "pc", "ac", "ao", "sm", "am") \
            or h.startswith("access"):
        return "access"
    return None


BYTE_VALUE_RE = re.compile(r"^\(?\d+(?:\s*[-\u2013]\s*\d+)?\)?$")
BITS_VALUE_RE = re.compile(r"^(all|\d+(?:\s*[:\u2013-]\s*\d+)?(?:\.\d+)*)$", re.IGNORECASE)


def _column_roles(header: list) -> dict:
    roles = {}
    for idx, cell in enumerate(header):
        role = _role_of(cell)
        if role and role not in roles:
            roles[role] = idx
        elif role == "access":
            roles.setdefault("access_cols", []).append(idx)
    # single access column -> also record in access_cols for uniform handling
    if "access" in roles:
        roles.setdefault("access_cols", []).insert(0, roles["access"])
    return roles


def _find_header_row(columns: list, rows: list) -> tuple:
    """-> (roles, data_rows). The table's extracted header columns are the
    primary candidate; if they carry no register roles, the first rows are
    scanned for a role-bearing header row (docling sometimes leaves the real
    column names inside the body when it cannot mark a header)."""
    roles = _column_roles(columns)
    if ("byte" in roles or "bits" in roles) and ("field" in roles or "description" in roles):
        return roles, rows
    for i, r in enumerate(rows[:3]):
        r_roles = _column_roles(r)
        if ("byte" in r_roles or "bits" in r_roles) \
                and ("field" in r_roles or "description" in r_roles):
            return r_roles, rows[i + 1 :]
    return {}, []


def build_registers(tables: list[dict], doc: str, revision, section_titles_by_id: dict) -> list[dict]:
    """Normalize register-map rows where the columns allow it:
    {doc, revision, table, page_address, byte, bits, field, description,
     access, page, raw}. Fields that cannot be read from the columns are
    null — never guessed. `access` joins multiple access columns (SFF-8636
    PC/AC/AO/SM) with "/" in column order. `page` cites the page the row's
    part of the logical table starts on."""
    out = []
    for t in tables:
        if t.get("is_toc") or not t.get("rows"):
            continue
        roles, data_rows = _find_header_row(t.get("columns") or [], t.get("rows") or [])
        if not (("byte" in roles) or ("bits" in roles)):
            continue
        if not (("field" in roles) or ("description" in roles)):
            continue
        header = t.get("columns") or (t["rows"][0] if t["rows"] else [])
        page_address = _page_address_from(t.get("caption") or "")
        if not page_address:
            for _sid, stitle in _section_chain(t.get("section"), section_titles_by_id):
                page_address = _page_address_from(stitle)
                if page_address:
                    break
        ncols = len(header)
        offset = 0
        for part in t.get("parts") or [{"page_start": t["page_start"],
                                         "row_count": len(t["rows"])}]:
            part_rows = t["rows"][offset : offset + part["row_count"]]
            offset += part["row_count"]
            for r in part_rows:
                row = (list(r) + [""] * ncols)[:ncols]
                if _norm_cells(row) == _norm_cells(header):
                    continue  # repeated header
                if not any(c.strip() for c in row):
                    continue
                byte = _cell(row, roles, "byte")
                byte = re.sub(r"\s+", "", byte) if BYTE_VALUE_RE.match(re.sub(r"\s+", "", byte)) else None
                bits = _cell(row, roles, "bits")
                bits = re.sub(r"\s+", "", bits) if BITS_VALUE_RE.match(re.sub(r"\s+", "", bits)) else None
                if byte is None and bits is None:
                    continue  # group/section rows inside the table
                field = _cell(row, roles, "field") or None
                if field in ("-", ""):
                    field = None
                description = _cell(row, roles, "description") or None
                access_vals = [row[i].strip() for i in roles.get("access_cols", []) if i < len(row)]
                access_vals = [v for v in access_vals if v and v != "-"]
                access = "/".join(access_vals) if access_vals else None
                out.append({
                    "doc": doc, "revision": revision, "table": t.get("id"),
                    "page_address": page_address,
                    "byte": byte, "bits": bits, "field": field,
                    "description": description, "access": access,
                    "page": part["page_start"],
                    "raw": row,
                })
    return out


def _cell(row: list, roles: dict, role: str) -> str:
    idx = roles.get(role)
    return row[idx].strip() if idx is not None and idx < len(row) else ""


def _page_address_from(text: str):
    m = PageAddr_RE.search(text or "")
    if not m:
        return None
    return re.sub(r"\s+", "", m.group(1))


def _section_chain(section_id, section_titles_by_id: dict) -> list:
    """Innermost-to-outermost (id, title) chain for a section id."""
    chain = []
    sid = section_id
    guard = 0
    while sid is not None and guard < 20:
        entry = section_titles_by_id.get(sid)
        if entry is None:
            break
        chain.append((sid, entry["title"]))
        sid = entry["parent"]
        guard += 1
    return chain


# --- revision history + document metadata -------------------------------------
REVHIST_RE = re.compile(r"(?i)revision\s+history|change\s+history|history\s+of\s+changes")
# "May 21, 2026", "September 4-2024", "Jan 14 th 2020", "April 2022", "2021-03-31"
DATE_RE = re.compile(
    r"\b([A-Z][a-z]{2,8}\s+\d{1,2}\s*(?:st|nd|rd|th)?\s*,?\s*[-\u2013]?\s*\d{4}"
    r"|[A-Z][a-z]{2,8}\s+\d{4}|\d{4}-\d{2}-\d{2})\b"
)
REV_TITLE_RE = re.compile(r"(?i)^\s*rev\.?\s*([0-9]+(?:\.[0-9]+)*[a-z]?)\b\s*[.,;:\-\u2013]?\s*(.*)$")


def build_revision_history(tables: list[dict], sections: list[dict],
                            items: list[FlatItem]) -> list[dict]:
    """The document's own revision history, parsed into
    [{revision, date, description}] — this is how "all versions" are
    enumerated. The cached families print it in three shapes, all handled:
      - a table under a Revision/Change History heading (SFF-8472, SFF-8024,
        SFF-8636's first pages), including headerless ones inferred by content;
      - "Rev N" section headings, date printed in the heading or the body
        (CMIS 5.2+, SFF-8636's later pages);
      - prose "Rev N - date" lines inside a history section (C-CMIS).
    Entries are returned in document order; the newest revision is last.
    """
    entries = []  # (page, kind_rank, order, entry)
    titles = {s["id"]: s for s in sections if s["id"]}
    for t in tables:
        if t.get("is_toc"):
            continue
        hay = " ".join(x for x in (t.get("caption"), t.get("section_title")) if x)
        if t.get("section") in titles:
            hay += " " + titles[t["section"]]["title"]
        hay += " " + " ".join(t.get("columns") or [])
        if t.get("rows"):
            hay += " " + " ".join(" ".join(r[:2]) for r in t["rows"][:2])
        if not REVHIST_RE.search(hay):
            continue
        hdr_i, roles, mode = _find_revhist_header(t["rows"])
        if hdr_i is None:
            continue
        if mode == "header":
            data = t["rows"][hdr_i + 1 :]
        else:
            # headerless table: build_tables parked the first data row in
            # `columns` — put it back, and keep every row (rev-less rows are
            # skipped below)
            data = list(t["rows"])
            cols = t.get("columns") or []
            rev_i = roles.get("revision")
            if cols and rev_i is not None and rev_i < len(cols) \
                    and re.match(r"(?i)^(rev\.?\s*)?[0-9]+(\.[0-9]+)*[a-z]?$", cols[rev_i].strip()):
                data = [cols] + data
        for order, r in enumerate(data):
            rev = _cell(r, roles, "revision").strip()
            rev = REV_TITLE_RE.sub(r"\1", rev) if rev else rev  # "Rev 0.7" -> "0.7"
            if rev.lower().startswith("oif"):  # "OIF-C-CMIS-01.0" -> "01.0"
                m2 = re.search(r"-(\d[\w.]*)$", rev)
                if m2:
                    rev = m2.group(1)
            date = _cell(r, roles, "date").strip()
            desc = _cell(r, roles, "description").strip()
            if not date:
                m = DATE_RE.search(" ".join(c.strip() for c in r))
                date = m.group(1) if m else ""
            if not rev:
                continue
            entries.append((t["page_start"], 0, order, {
                "revision": rev, "date": date or None,
                "description": desc or None,
            }))
    # prose/flat form: a history heading starts a block of revision entries
    # that later revisions extend as plain body text (CMIS 5.3/5.4) or their
    # own unnumbered "Rev N" headings (CMIS 3.0-5.1) — all siblings at body
    # level, so section spans do not cover them. Scan items forward from the
    # heading and stop at the first numbered or contents-like heading.
    heading_indices = {s["_heading_index"]: s for s in sections if "_heading_index" in s}
    for s in sections:
        if not REVHIST_RE.search(s.get("title") or "") or "_heading_index" not in s:
            continue
        cur = None
        prose_order = 0

        def _flush():
            nonlocal cur, prose_order
            if not cur:
                return
            dm = DATE_RE.search(cur["description"] or "")
            if dm:
                cur["date"] = dm.group(1)
                cur["description"] = (
                    cur["description"][: dm.start()] + " " + cur["description"][dm.end() :]
                ).strip()
            cur["description"] = cur["description"] or None
            entries.append((s["page_start"], 1, prose_order, cur))
            prose_order += 1
            cur = None

        for it in items[s["_heading_index"] + 1:]:
            if it.kind == "text" and it.label == "section_header":
                sid, title, _depth = parse_section_title(it.text)
                if sid is not None or TOC_TITLE_RE.match(it.text) \
                        or TABLE_CAPTION_RE.match(it.text):
                    break  # the document body starts: history block over
                if REV_TITLE_RE.match(it.text):
                    ln = it.text
                elif REVHIST_RE.search(it.text):
                    continue  # a sibling history heading: not entry content
                else:
                    ln = it.text  # changes-subblock title (e.g. Compatibility)
            elif it.kind == "text":
                ln = it.text
            else:
                continue
            m = REV_TITLE_RE.match(ln.strip())
            if m:
                _flush()
                cur = {"revision": m.group(1), "date": None, "description": None}
                rest = m.group(2).strip()
                dm = DATE_RE.search(rest)
                if dm:
                    cur["date"] = dm.group(1)
                    rest = (rest[: dm.start()] + " " + rest[dm.end() :]).strip()
                cur["description"] = rest or None
            elif cur is not None:
                cur["description"] = (cur["description"] + "\n" + ln).strip() \
                    if cur["description"] else (ln.strip() or cur["description"])
        _flush()
    # heading form: "Rev N" sections; dates may sit in the heading or the
    # body, and a later revision block sometimes prints inside one section
    # (docling keeps it as body text), so re-split on "Rev N" lines the same
    # way the prose form does.
    for order, s in enumerate(sections):
        m = REV_TITLE_RE.match(s.get("title") or "")
        if not m or s.get("id"):
            continue  # numbered body sections never print a revision this way
        text = s.get("text") or ""
        lines = ([m.group(2).strip()] if m.group(2).strip() else []) + text.split("\n")
        cur = {"revision": m.group(1), "date": None, "description": None}
        sub = 0

        def flush():
            nonlocal cur, sub
            if not cur:
                return
            date = None
            dm = DATE_RE.search(cur["description"] or "")
            if dm:
                date = dm.group(1)
                cur["description"] = (
                    cur["description"][: dm.start()] + " " + cur["description"][dm.end() :]
                ).strip()
            desc = re.sub(r"^[\s:,;.\u2013-]+", "", (cur["description"] or "").strip()).strip() or None
            if date or desc:  # a heading with neither date nor changes is noise
                entries.append((s["page_start"], 2, order * 1000 + sub,
                                {"revision": cur["revision"], "date": date,
                                 "description": desc}))
            sub += 1
            cur = None

        for ln in lines:
            m2 = REV_TITLE_RE.match(ln.strip())
            if m2:
                flush()
                cur = {"revision": m2.group(1), "date": None, "description": None}
                rest = m2.group(2).strip()
                if rest:
                    cur["description"] = rest
            elif cur is not None:
                cur["description"] = (cur["description"] + "\n" + ln).strip() \
                    if cur["description"] else (ln.strip() or cur["description"])
        flush()
    entries.sort(key=lambda e: (e[0], e[1], e[2]))
    seen = {}
    for e in entries:
        rev = e[3]["revision"]
        prev = seen.get(rev)
        if prev is None:
            seen[rev] = e[3]
        elif not prev.get("date") and e[3].get("date"):
            prev["date"] = e[3]["date"]  # keep the dated variant of the same rev
    return list(seen.values())


def _find_revhist_header(rows: list):
    """-> (start_row, roles, mode) where mode is "header" (data follows the
    header row) or "content" (the start row is itself data — headerless
    tables like SFF-8024's)."""
    rev_re = re.compile(r"(?i)^(rev(ision)?|version|document)")  # C-CMIS titles its rev column "Document"
    for i, r in enumerate(rows[:3]):
        roles = {}
        for idx, c in enumerate(r):
            h = _hdr_norm(c)
            if not h:
                continue
            if rev_re.match(h):
                roles["revision"] = idx
            elif h.startswith("date"):
                roles["date"] = idx
            elif h.startswith("description") or h in ("summary", "changes", "summary of changes"):
                roles["description"] = idx
        if "revision" in roles and ("date" in roles or "description" in roles):
            return i, roles, "header"
    # content fallback: rows that look like ("Rev 1.2" | ... date ...)
    rev_like = re.compile(r"(?i)^(rev\.?\s*)?(oif[- a-z0-9]*-)?[0-9]+(\.[0-9]+)*[a-z]?$")
    for i, r in enumerate(rows[:5]):
        rev_i = None
        for idx, c in enumerate(r):
            if rev_like.match(c.strip()):
                rev_i = idx
                break
        if rev_i is None:
            continue
        roles = {"revision": rev_i}
        rest = [idx for idx in range(len(r)) if idx != rev_i]
        date_i = next((idx for idx in rest if DATE_RE.search(r[idx])), None)
        if date_i is not None:
            roles["date"] = date_i
        desc = [idx for idx in rest if idx != date_i and r[idx].strip()]
        if desc:
            roles["description"] = max(desc, key=lambda k: len(r[k]))
        if len(roles) >= 2:
            return i, roles, "content"
    return None, {}, "content"


def extract_docmeta(items: list[FlatItem], family: str, stem: str,
                    revision_history: list[dict]) -> dict:
    """Title / revision / publication date / publisher as printed on the
    cover, parsed with per-family rules and explicit fallbacks. Anything not
    confidently printed is null — no guessing. Fallback order for
    `revision`: printed "Revision/Rev" line, IA id suffix, newest
    revision-history entry, then null."""
    first = min((it.page for it in items if it.page), default=0)
    page1 = " ".join(it.text for it in items if it.kind == "text" and it.page == first)
    page1 = re.sub(r"\s+", " ", page1)
    meta = {"title": None, "revision": None, "publication_date": None,
            "publisher": None, "ia_id": None}

    # --- IA identification (OIF docs print an OIF-CMIS-05.4-style id)
    m = re.search(r"\b(OIF[- ](?:CMIS|C-CMIS|ELSFP|CMIS-FF|CMIS-LT|CMIS-VCS)[-A-Z0-9.]*\d[-0-9A-Za-z.]*)", page1)
    if m:
        cand = m.group(1)
        base = re.sub(r"-changebars$", "", stem, flags=re.IGNORECASE)
        if base.lower().startswith(cand.replace(" ", "-").lower()) or cand.lower().startswith(base.lower()):
            meta["ia_id"] = cand
        else:
            meta["ia_id"] = cand  # printed on the cover; keep it either way

    # --- title
    if family.startswith("sff-"):
        for it in items:
            if it.kind == "text" and re.match(r"^SFF-\d{3,4}$", it.text.strip()):
                meta["title"] = it.text.strip()
                break
    else:
        # docling sometimes merges the cover title and the revision line
        # into one heading ("Common Management Interface Specification
        # (CMIS) Revision 5.2"); split it back apart.
        merged_text = next(
            (it.text for it in items
             if it.kind == "text" and it.label in ("title", "section_header")
             and it.page == first
             and re.match(r"^.+\s+(?:Revision|Rev)\s+[\w.]+", it.text)),
            "",
        )
        merged = re.match(r"^(.{8,160}?)\s+(?:Revision|Rev)\s+[\w.]+", merged_text)
        m = re.search(
            r"(?i)implementation agreement for (.+?)(?=\s+(?:IA\s|Revision\s|OIF[\s-]|www\.)|$)",
            page1,
        )
        if merged:
            meta["title"] = merged.group(1).strip()
        elif m:
            meta["title"] = m.group(1).strip()
        else:
            m = re.search(r"(?i)implementation agreement \(IA\)\s+(.+?)\s+Revision\s", page1)
            if m:
                t = m.group(1).strip()
                t = re.sub(r"^Errata\s+", "", t)
                meta["title"] = t
            else:
                m = re.search(r"([A-Z][A-Za-z0-9 ,()\-/]+?)\s+Implementation Agreement\b", page1)
                if m:
                    t = m.group(1).strip()
                    t = re.sub(r"^OIF[- A-Za-z0-9.]*?\d+(\.\d+)*\s+", "", t)
                    meta["title"] = t or None
                else:
                    # third-party specs print the title as one uppercase
                    # line; 5p1's cover is a legal-notice page, so scan the
                    # first two pages
                    cands = [it.text.strip() for it in items
                             if it.kind == "text" and first <= it.page <= first + 1
                             and re.match(r"^[A-Z][A-Z0-9 .,()\-/]+$", it.text.strip())
                             and "SPECIFICATION" in it.text
                             and len(it.text) <= 90 and not it.text.endswith(".")]
                    if cands:
                        meta["title"] = max(cands, key=len)
                    else:
                        m = re.search(
                            r"\b([A-Z][A-Z0-9 ,()\-/]{8,}?SPECIFICATION(?:\s*\([A-Z0-9.]+\))?)",
                            page1,
                        )
                        if m:
                            meta["title"] = m.group(1).strip()

    # --- revision as printed
    m = re.search(r"(?i)\bRevision\s+(\d[\w.]*)", page1)
    if not m:
        m = re.search(r"(?i)\bRev\.?\s+(\d[\w.]*)", page1)
    if m:
        meta["revision"] = m.group(1)
    elif meta["ia_id"]:
        m = re.search(r"-(\d[\w.]*)$", meta["ia_id"].replace(" ", "-"))
        if m:
            meta["revision"] = m.group(1)
    elif revision_history:
        # histories are printed oldest-first; the document's own revision is last
        meta["revision"] = revision_history[-1].get("revision")

    # --- publication date (printed on the cover, else the matching history row)
    m = DATE_RE.search(page1)
    if m:
        meta["publication_date"] = m.group(1)
    else:
        for row in reversed(revision_history):
            if row.get("revision") == meta["revision"] and row.get("date"):
                meta["publication_date"] = row["date"]
                break

    # --- publisher
    if "Implementation Agreement created and approved" in page1 and re.search(r"\bOIF\b", page1):
        meta["publisher"] = "OIF"
    elif "QSFP-DD MSA" in page1:
        meta["publisher"] = "QSFP-DD MSA"
    elif "released by SNIA" in page1 or "SECRETARIAT: SFF TWG" in page1:
        meta["publisher"] = "SNIA"
    return meta


# --- corpus writer ----------------------------------------------------------------
def corpus_paths(out: Path, family: str, dslug: str) -> Path:
    return out / "corpus" / family / dslug


def corpus_fresh(cdir: Path, src_sha: str) -> bool:
    try:
        if not all((cdir / f).is_file() for f in CORPUS_DOC_FILES):
            return False
        return json.loads((cdir / "document.json").read_text(encoding="utf-8")).get("sha256") == src_sha
    except (OSError, ValueError):
        return False


def build_corpus(d: dict, pdf: Path, stds_root: Path, out: Path, versions: dict,
                 src_sha: str) -> dict:
    """Derive the whole per-document corpus tree from a docling dict."""
    rel = pdf.relative_to(stds_root)
    family = rel.parts[0]
    stem = rel.with_suffix("").parts[-1]
    dslug = stem
    doc = "__".join((family, dslug))
    cdir = corpus_paths(out, family, dslug)
    cdir.mkdir(parents=True, exist_ok=True)

    items = flatten_docling_dict(d)
    docmeta = None
    revision_history = []
    sections = build_sections(items)
    section_titles = {s["id"]: s for s in sections if s["id"]}
    tables = build_tables(items, sections)
    revision_history = build_revision_history(tables, sections, items)
    docmeta = extract_docmeta(items, family, stem, revision_history)
    registers = build_registers(tables, doc, docmeta["revision"], section_titles)

    document = {
        "format_version": CORPUS_FORMAT_VERSION,
        "doc": doc,
        "slug": dslug,
        "family": family,
        "title": docmeta["title"],
        "revision": docmeta["revision"],
        "publication_date": docmeta["publication_date"],
        "publisher": docmeta["publisher"],
        "ia_id": docmeta["ia_id"],
        "source_path": rel.as_posix(),
        "source_file": pdf.name,
        "sha256": src_sha,
        "page_count": page_count_of(d),
        "tool_versions": versions,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "revision_history": revision_history,
        "counts": {
            "sections": len(sections),
            "tables": len(tables),
            "registers": len(registers),
            "revision_history": len(revision_history),
        },
    }
    (cdir / "document.json").write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n",
                                         encoding="utf-8")
    (cdir / "document.md").write_text(render_document_md(items), encoding="utf-8")
    def public(r):
        return {k: v for k, v in r.items() if not k.startswith("_")}

    for name, rows in (("sections.jsonl", sections), ("tables.jsonl", tables),
                       ("registers.jsonl", registers)):
        with (cdir / name).open("w", encoding="utf-8") as f:
            for r in rows:
                f.write(json.dumps(public(r), ensure_ascii=False) + "\n")
    return {
        "status": "ok",
        "path": str(cdir.relative_to(out)),
        "document": {k: v for k, v in document.items() if k != "tool_versions"},
    }


# --- main -------------------------------------------------------------------
def slug_for(pdf: Path, stds_root: Path) -> str:
    rel = pdf.relative_to(stds_root)
    # cmis/OIF-CMIS-05.3.pdf -> cmis__OIF-CMIS-05.3
    return "__".join(rel.with_suffix("").parts)


def print_record(i: int, total: int, rec: dict) -> None:
    slug = rec.get("slug", "?")
    st = rec.get("status", "?")
    if st == "ok":
        print(
            f"[{i}/{total}] {slug}  OK  md={rec.get('md_chars',0)}c "
            f"blocks={rec.get('block_count',0)} (tbl={rec.get('table_block_count',0)}) "
            f"chunks={rec.get('chunk_count',0)} {rec.get('duration_s',0)}s"
        )
    elif st == "cached":
        print(f"[{i}/{total}] {slug}  CACHED ({rec.get('chunk_count',0)} chunks)")
    else:
        print(f"[{i}/{total}] {slug}  ERROR: {rec.get('error','')}", file=sys.stderr)


def discover_pdfs(stds_root: Path) -> list[Path]:
    pdfs = sorted(p for p in stds_root.rglob("*.pdf"))
    return pdfs


def markdown_from_docling_dict(d: dict) -> str:
    """docling markdown for a cached structured-document dict. Round-trips
    through DoclingDocument so the bytes match a direct export (verified by
    tools/test_digest_standards regression cases on real runs)."""
    from docling_core.types.doc import DoclingDocument

    return DoclingDocument.model_validate(d).export_to_markdown()


def process_one(args_tuple):
    """Convert + chunk + build the corpus for a single PDF. Module-level so it
    is picklable by ProcessPoolExecutor. Returns the manifest record dict."""
    pdf_str, stds_root_str, out_str, force, chunk_size, min_chars, versions = args_tuple
    pdf = Path(pdf_str)
    stds_root = Path(stds_root_str)
    out = Path(out_str)
    slug = slug_for(pdf, stds_root)
    rel = pdf.relative_to(stds_root)
    family = rel.parts[0]
    dslug = rel.with_suffix("").parts[-1]
    cdir = corpus_paths(out, family, dslug)
    md_path = out / "markdown" / f"{slug}.md"
    chunk_path = out / "chunks" / f"{slug}.jsonl"
    sha_path = out / "chunks" / f"{slug}.sha256"  # provenance: source sha these outputs were parsed from
    src_sha = sha256_of(pdf)
    dict_cache = out / "corpus" / ".cache" / f"{src_sha}.json"
    record = {
        "slug": slug,
        "source_path": str(pdf.relative_to(stds_root)),
        "abs_path": str(pdf),
        "sha256": src_sha,
        "bytes": pdf.stat().st_size,
    }
    # cached: reuse existing parse ONLY if the outputs exist AND they were
    # parsed from the same source sha256. A PDF refreshed in place under the
    # same filename (e.g. OIF silently revises OIF-CMIS-05.3.pdf) would
    # otherwise leave stale chunks while manifest.json records the new hash.
    stale = False
    if md_path.exists() and chunk_path.exists() and not force:
        prev_sha = sha_path.read_text(encoding="utf-8").strip() if sha_path.exists() else None
        if prev_sha != src_sha:
            stale = True  # source changed since last parse -> regenerate
    else:
        prev_sha = None
    chunks_fresh = not stale and md_path.exists() and chunk_path.exists() and not force
    corpus_fresh_ = not force and corpus_fresh(cdir, src_sha)
    if chunks_fresh and corpus_fresh_:
        n = tables = 0
        with chunk_path.open() as f:
            for line in f:
                n += 1
                try:
                    if json.loads(line).get("block_type") == "table":
                        tables += 1
                except Exception as e:
                    print(
                        f"WARNING: {chunk_path}:{n}: cannot count cached table chunk: {e}",
                        file=sys.stderr,
                    )
        corpus_rec = {
            "status": "cached",
            "path": str(cdir.relative_to(out)),
            "document": json.loads((cdir / "document.json").read_text(encoding="utf-8")),
        }
        record.update(
            {"status": "cached",
             "md_path": str(md_path.relative_to(out)),
             "chunks_path": str(chunk_path.relative_to(out)),
             "chunk_count": n, "table_chunk_count": tables,
             "corpus": corpus_rec}
        )
        return record
    # something is missing or stale: need the structured document
    t0 = time.time()
    try:
        d = None
        md = None
        if dict_cache.exists() and not force:
            d = json.loads(dict_cache.read_text(encoding="utf-8"))
        if d is None:
            converter = build_converter()
            res = converter.convert(pdf)
            d = res.document.export_to_dict()
            dict_cache.parent.mkdir(parents=True, exist_ok=True)
            dict_cache.write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
            md = res.document.export_to_markdown()
        if not chunks_fresh:
            if md is None:
                md = markdown_from_docling_dict(d)
            md_path.write_text(md, encoding="utf-8")
            blocks = split_into_blocks(md)
            chunks = list(chunk_blocks(blocks, slug, chunk_size))
            with chunk_path.open("w", encoding="utf-8") as f:
                for c in chunks:
                    f.write(json.dumps(c, ensure_ascii=False) + "\n")
            sha_path.write_text(src_sha, encoding="utf-8")
        else:
            # chunks served from cache: only the corpus is being rebuilt
            blocks = []
            with chunk_path.open() as f:
                chunks = [json.loads(line) for line in f if line.strip()]
        corpus_rec = record.get("corpus")
        if not corpus_fresh_:
            corpus_rec = build_corpus(d, pdf, stds_root, out, versions, src_sha)
        dur = time.time() - t0
        update = {
            "status": "ok",
            "md_path": str(md_path.relative_to(out)),
            "chunks_path": str(chunk_path.relative_to(out)),
            "chunk_count": len(chunks),
            "table_chunk_count": sum(1 for c in chunks if c["block_type"] == "table"),
            "duration_s": round(dur, 1),
            "corpus": corpus_rec,
        }
        if blocks:  # only when the markdown/chunk pass actually ran
            update.update({
                "md_chars": len(md),
                "block_count": len(blocks),
                "table_block_count": sum(1 for b in blocks if b.kind == "table"),
            })
        record.update(update)
    except Exception as e:
        record.update({"status": "error", "error": str(e),
                       "traceback": traceback.format_exc(limit=8)})
    return record


def _revision_key(rev, name):
    """Deterministic sort key for a printed revision string."""
    m = re.match(r"^(\d+)(?:\.(\d+))?([a-z]+)?$", (rev or "").strip().lower())
    if not m:
        return (9, 0, 0, "", name or "")
    return (0, int(m.group(1)), int(m.group(2) or 0), m.group(3) or "", name or "")


def write_catalog_and_readme(out: Path, manifest: list[dict], versions: dict) -> tuple[int, int]:
    """corpus/catalog.json (every document + per-family ordered revisions)
    and corpus/README.md. Returns (documents missing a corpus, docs listed)."""
    docs = []
    missing = 0
    for r in manifest:
        c = r.get("corpus")
        if not c or not c.get("document"):
            missing += 1
            continue
        document = c["document"]
        docs.append(document)
    docs.sort(key=lambda dd: (dd["family"], _revision_key(dd.get("revision"), dd["slug"])))
    families = {}
    for dd in docs:
        families.setdefault(dd["family"], []).append({
            "doc": dd["doc"], "revision": dd.get("revision"),
            "date": dd.get("publication_date"), "path": c_path(dd),
        })
    for fam in families.values():
        fam.sort(key=lambda e: _revision_key(e["revision"], e["doc"]))
    catalog = {
        "format_version": CORPUS_FORMAT_VERSION,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "tool_versions": versions,
        "documents": [{
            "family": dd["family"], "doc": dd["doc"], "slug": dd["slug"],
            "title": dd.get("title"), "revision": dd.get("revision"),
            "date": dd.get("publication_date"), "publisher": dd.get("publisher"),
            "ia_id": dd.get("ia_id"), "path": c_path(dd),
            "source_path": dd.get("source_path"), "sha256": dd.get("sha256"),
        } for dd in docs],
        "families": families,
    }
    (out / "corpus" / "catalog.json").write_text(
        json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    (out / "corpus" / "README.md").write_text(CORPUS_README, encoding="utf-8")
    return missing, len(docs)


def c_path(dd: dict) -> str:
    return f"corpus/{dd['family']}/{dd['slug']}"


CORPUS_README = """# Standards corpus — how to read this tree

A structured, machine-readable version of every cached specification PDF
(CMIS family, SFF-8636, SFF-8472, SFF-8024), built by
`openxcvr-lib/tools/digest-standards.py` from the docling structured
document. Derived from copyrighted PDFs — never committed, regenerated from
`docs/stds/` on demand.

## Layout

- `catalog.json` — every document (family, revision, date, paths) plus, per
  family, the ordered list of available revisions.
- `<family>/<doc>/document.json` — document identity: family, printed title,
  revision, publication date, publisher, IA id, source file + sha256, page
  count, tool versions, and the document's own revision-history table parsed
  into `[{revision, date, description}]` (the enumeration of all versions).
- `<family>/<doc>/document.md` — clean reading text. `<!-- page N -->`
  markers sit at every PDF page that carries body content (a page whose
  only content was dropped furniture gets no marker), headings keep their
  printed numbering, tables are inline markdown. Running headers/footers and image
  placeholders are dropped.
- `<family>/<doc>/sections.jsonl` — one record per section heading:
  `{id, title, level, page_start, page_end, parent, text}`. `id` is the printed
  section number (`"8.4.2"`, `"Annex A"`) or null for unnumbered headings;
  `level` is the numbering depth; `text` is the section's direct prose only
  (nested subsections have their own records).
- `<family>/<doc>/tables.jsonl` — one record per logical table, multi-page
  continuations merged: `{id, caption, section, page_start, page_end,
  columns, rows, is_toc}`. `id` is the printed "Table 8-28" style number when
  captioned, else null. Table-of-contents / list-of-tables tables carry
  `is_toc: true`. `parts` lists `{page_start, row_count}` per merged piece
  (page citation for a row = the part page holding it).
- `<family>/<doc>/registers.jsonl` — rows of register-map tables, normalized
  where the columns allow it:
  `{doc, revision, table, page_address, byte, bits, field, description,
  access, page, raw}`. `raw` keeps the original row next to the normalized
  fields. Unreadable fields are null — never guessed. A row appears here only
  when its byte or bits cell parses (group/section rows inside a table are
  skipped). `page_address` (e.g. `"01h"`, `"A2h"`, `"30h"`) comes from the
  table caption or enclosing section. `access` joins multi-column access
  (SFF-8636 PC/AC/AO/SM) with `/` in column order. `page` is the PDF page the
  row's part starts on.

## Query recipes

- All revisions of CMIS: `jq '.families.cmis' catalog.json`.
- One register row, e.g. CMIS 5.4 `totalPwrHiAlarmThresh`-style lookups:
  `jq -c 'select(.field != null and (.field|test("VendorName")))' cmis/OIF-CMIS-05.4/registers.jsonl`.
- Section by number:
  `jq -c 'select(.id == "8.4")' cmis/OIF-CMIS-05.4/sections.jsonl`.
- Table by printed number:
  `jq -c 'select(.id == "Table 8-28")' cmis/OIF-CMIS-05.4/tables.jsonl`.
- Page-cited reading text: `grep -n "<!-- page 128 -->" .../document.md`.
- Everything on one page address:
  `jq -c 'select(.page_address == "01h")' .../registers.jsonl`.

Page numbers are PDF page numbers (docling page indices), which is what the
`<!-- page N -->` markers and every `page*` field cite.
"""


def main():
    ap = argparse.ArgumentParser(description="Digest CMIS/SFF spec PDFs with docling + chonkie.")
    ap.add_argument("--stds-root", type=Path, default=DEFAULT_STDS_ROOT)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--force", action="store_true", help="re-parse even if outputs exist")
    ap.add_argument("--limit", type=int, default=0, help="process first N PDFs (0 = all)")
    ap.add_argument(
        "--smoke",
        action="store_true",
        help="smoke test: parse the smallest PDF only and exit",
    )
    ap.add_argument(
        "--jobs", "-j", type=int, default=1,
        help="parallel worker processes (1 = serial). Recommended ~16 on a many-core host.",
    )
    args = ap.parse_args()

    stds_root: Path = args.stds_root
    out: Path = args.out
    if not stds_root.is_dir():
        sys.exit(f"--stds-root not found: {stds_root}")
    pdfs = discover_pdfs(stds_root)
    if not pdfs:
        sys.exit(f"--stds-root: no PDFs found under {stds_root}")

    if args.smoke:
        # smallest PDF by size = fastest model warmup verification
        pdfs = [min(pdfs, key=lambda p: p.stat().st_size)]
        print(f"[smoke] selected {pdfs[0].name} ({pdfs[0].stat().st_size} bytes)")
    if args.limit:
        pdfs = pdfs[: args.limit]

    (out / "markdown").mkdir(parents=True, exist_ok=True)
    (out / "chunks").mkdir(parents=True, exist_ok=True)
    (out / "corpus" / ".cache").mkdir(parents=True, exist_ok=True)

    versions = {}
    for mod in ("docling", "chonkie"):
        try:
            import importlib.metadata as md

            versions[mod] = md.version(mod)
        except Exception:
            versions[mod] = "unknown"

    jobs = max(1, args.jobs)
    print(f"Digesting {len(pdfs)} PDF(s) from {stds_root} -> {out}  (jobs={jobs})")
    print(f"  docling={versions.get('docling')} chonkie={versions.get('chonkie')}")

    work = [
        (str(pdf), str(stds_root), str(out), args.force, CHUNK_SIZE_CHARS,
         MIN_CHARS_PER_CHUNK, versions)
        for pdf in pdfs
    ]

    manifest = []
    if jobs == 1:
        for i, w in enumerate(work, 1):
            rec = process_one(w)
            manifest.append(rec)
            print_record(i, len(work), rec)
    else:
        from concurrent.futures import ProcessPoolExecutor, as_completed
        with ProcessPoolExecutor(max_workers=jobs) as ex:
            futs = {ex.submit(process_one, w): w for w in work}
            done = 0
            for fut in as_completed(futs):
                done += 1
                try:
                    rec = fut.result()
                except Exception as e:
                    # worker-level crash: record against the work slug
                    w = futs[fut]
                    rec = {"slug": slug_for(Path(w[0]), Path(w[1])),
                           "status": "error", "error": f"worker crashed: {e}"}
                manifest.append(rec)
                print_record(done, len(work), rec)

    summary = {
        "stds_root": str(stds_root),
        "out": str(out),
        "tool_versions": versions,
        "chunk_size_chars": CHUNK_SIZE_CHARS,
        "min_chars_per_chunk": MIN_CHARS_PER_CHUNK,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "pdf_count": len(pdfs),
        "ok": sum(1 for r in manifest if r["status"] == "ok"),
        "cached": sum(1 for r in manifest if r["status"] == "cached"),
        "errors": sum(1 for r in manifest if r["status"] == "error"),
        "files": sorted(manifest, key=lambda r: r.get("slug", "")),
    }
    (out / "manifest.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False))

    corpus_errors = write_catalog_and_readme(out, manifest, versions)
    print(
        f"\nDONE: {summary['ok']} ok, {summary['cached']} cached, "
        f"{summary['errors']} errors of {len(pdfs)} PDFs; "
        f"corpus catalog lists {corpus_errors[1]} documents"
    )
    if corpus_errors[0]:
        print(f"WARNING: corpus records with errors: {corpus_errors[0]}", file=sys.stderr)
    print(f"manifest: {out / 'manifest.json'}")
    return 0 if summary["errors"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
