#!/usr/bin/env python3
"""Exercise cached-chunk recovery without docling/chonkie or real PDFs.

Run: python3 -m unittest tools.test_digest_standards
"""

import contextlib
import io
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch


ds = runpy.run_path(str(Path(__file__).with_name("digest-standards.py")))


class CachedChunksTests(unittest.TestCase):
    def check_cached(self, lines, bad_line=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "test").mkdir()
            pdf = root / "test" / "spec.pdf"
            pdf.write_bytes(b"cached source; no PDF conversion needed")
            out = root / ".parsed"
            (out / "markdown").mkdir(parents=True)
            (out / "chunks").mkdir()
            cdir = out / "corpus" / "test" / "spec"
            cdir.mkdir(parents=True)
            md = out / "markdown" / "test__spec.md"
            chunks = out / "chunks" / "test__spec.jsonl"
            sha = out / "chunks" / "test__spec.sha256"
            md.write_text("cached markdown", encoding="utf-8")
            chunks.write_text("\n".join(lines) + "\n", encoding="utf-8")
            sha.write_text(ds["sha256_of"](pdf), encoding="utf-8")
            for name in ds["CORPUS_DOC_FILES"]:
                (cdir / name).write_text("" if name != "document.json" else '{}',
                                        encoding="utf-8")
            (cdir / "document.json").write_text(
                json.dumps({"sha256": ds["sha256_of"](pdf), "doc": "test__spec"}),
                encoding="utf-8",
            )
            before = {p: p.read_bytes() for p in (pdf, md, chunks, sha)}
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr), patch.dict(
                ds["process_one"].__globals__,
                {"build_converter": lambda: self.fail("cached output must not be reparsed")},
            ):
                rec = ds["process_one"](
                    (str(pdf), str(root), str(out), False, 1200, 24,
                     {"docling": "test", "chonkie": "test"})
                )
            self.assertEqual(rec["status"], "cached")
            self.assertEqual(rec["corpus"]["status"], "cached")
            self.assertEqual(rec["corpus"]["document"]["doc"], "test__spec")
            for p in before:
                self.assertEqual(before[p], p.read_bytes(), f"{p} was rewritten")
            self.assertEqual(rec["chunk_count"], len(lines))
            self.assertEqual(rec["table_chunk_count"], 2)
            self.assertEqual(rec["md_path"], "markdown/test__spec.md")
            self.assertEqual(rec["chunks_path"], "chunks/test__spec.jsonl")
            self.assertNotIn("error", rec)
            self.assertEqual(before, {p: p.read_bytes() for p in before})
            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                ds["print_record"](1, 1, rec)
            self.assertEqual(stdout.getvalue(), f"[1/1] test__spec  CACHED ({len(lines)} chunks)\n")
            if bad_line is None:
                self.assertEqual(stderr.getvalue(), "")
            else:
                try:
                    json.loads(bad_line).get("block_type")
                except (json.JSONDecodeError, AttributeError) as exc:
                    expected = str(exc)
                self.assertEqual(
                    stderr.getvalue(),
                    f"WARNING: {chunks}:2: cannot count cached table chunk: {expected}\n",
                )

    def test_valid_cache_is_quiet(self):
        self.check_cached(['{"block_type": "table"}', '{"block_type": "prose"}',
                           '{"block_type": "table"}'])

    def test_bad_cached_line_warns_and_continues(self):
        for bad_line in ("not JSON", "[]"):
            with self.subTest(bad_line=bad_line):
                self.check_cached(['{"block_type": "table"}', bad_line,
                                   '{"block_type": "table"}'], bad_line)


# --- corpus tests: synthetic docling documents, no PDF/docling needed ----------


def text_item(label, text, page, level=0, layer="body"):
    return {"label": label, "text": text, "level": level,
            "prov": [{"page_no": page}], "content_layer": layer}


def table_item(grid, page, header_rows=(0,)):
    """docling-real cell shape: column_header flags, no per-cell row_idx."""
    data_grid = [
        [{"text": c, "column_header": i in header_rows}
         for c in r]
        for i, r in enumerate(grid)
    ]
    return {"label": "table", "prov": [{"page_no": page}],
            "data": {"grid": data_grid}, "content_layer": "body"}


def make_doc(elements, npages=30):
    """elements: ordered (kind, item) pairs -> a docling-structured dict.
    Body children use docling's {"$ref": ...} dict refs."""
    texts, tables, pictures = [], [], []
    children = []
    for kind, item in elements:
        if kind == "text":
            ref = f"#/texts/{len(texts)}"
            item["self_ref"] = ref
            texts.append(item)
            children.append({"$ref": ref})
        elif kind == "table":
            ref = f"#/tables/{len(tables)}"
            item["self_ref"] = ref
            tables.append(item)
            children.append({"$ref": ref})
        elif kind == "picture":
            ref = f"#/pictures/{len(pictures)}"
            item["self_ref"] = ref
            pictures.append(item)
            children.append({"$ref": ref})
    return {
        "pages": {str(i): {} for i in range(1, npages + 1)},
        "body": {"children": children},
        "texts": texts, "tables": tables, "pictures": pictures,
        "groups": [], "key_value_items": [],
    }


class CorpusFlattenTests(unittest.TestCase):
    def test_flatten_and_document_md(self):
        doc = make_doc([
            ("text", text_item("title", "SFF-TEST", 1)),
            ("text", text_item("page_header", "Running header", 1)),
            ("text", text_item("paragraph", "Intro prose.", 1)),
            ("picture", {"self_ref": "#/pictures/0"}),
            ("text", text_item("section_header", "1 Scope", 2, level=1)),
            ("text", text_item("paragraph", "Body text page two.", 2)),
            ("text", text_item("section_header", "1.1 Details", 3, level=2)),
            ("table", table_item([["Byte", "Bits"], ["128", "7-0"]], 3)),
            ("text", text_item("page_footer", "SFF-TEST Rev 1.0", 3)),
        ])
        items = ds["flatten_docling_dict"](doc)
        labels = [it.label for it in items]
        self.assertNotIn("page_header", labels)
        self.assertNotIn("page_footer", labels)
        self.assertTrue(all(it.kind != "table" or it.grid for it in items))
        self.assertFalse(any(it.text == "Running header" for it in items))
        md = ds["render_document_md"](items)
        self.assertIn("<!-- page 1 -->", md)
        self.assertIn("<!-- page 2 -->", md)
        self.assertIn("<!-- page 3 -->", md)
        self.assertIn("# SFF-TEST", md)
        self.assertIn("# 1 Scope", md)
        self.assertIn("## 1.1 Details", md)
        self.assertIn("| Byte | Bits |", md)
        self.assertIn("| 128 | 7-0 |", md)
        self.assertNotIn("Running header", md)
        self.assertNotIn("image", md.lower())
        self.assertEqual(ds["page_count_of"](doc), 30)


class CorpusSectionsTests(unittest.TestCase):
    def test_sections_numbering_parent_text(self):
        doc = make_doc([
            ("text", text_item("section_header", "1 Scope", 1, level=1)),
            ("text", text_item("paragraph", "Scope prose.", 1)),
            ("text", text_item("section_header", "1.1 Subscope", 2, level=2)),
            ("text", text_item("paragraph", "Subscope prose.", 3)),
            ("text", text_item("section_header", "2 References", 4, level=1)),
            ("text", text_item("paragraph", "Ref prose.", 5)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        by_id = {s["id"]: s for s in secs}
        self.assertEqual(by_id["1"]["parent"], None)
        self.assertEqual(by_id["1"]["title"], "Scope")
        self.assertEqual(by_id["1"]["level"], 1)
        self.assertEqual(by_id["1.1"]["parent"], "1")
        self.assertEqual(by_id["1.1"]["page_start"], 2)
        self.assertEqual(by_id["1"]["page_end"], 3)  # spans the subsection
        self.assertEqual(by_id["1"]["text"], "Scope prose.")
        self.assertEqual(by_id["1.1"]["text"], "Subscope prose.")
        self.assertEqual(by_id["2"]["page_start"], 4)
        self.assertEqual(by_id["2"]["page_end"], 5)


class CorpusTablesTests(unittest.TestCase):
    def test_continuation_merge_by_id_and_header(self):
        hdr = ["Byte", "Bits", "Name", "Description", "Type"]
        doc = make_doc([
            ("text", text_item("section_header", "8.2 Register Map", 10, level=2)),
            ("text", text_item("paragraph", "Table 8-28 SFF8024IdentifierCopy (Byte 00h:128)", 10)),
            ("table", table_item([hdr, ["128", "7-0", "Ident", "desc", "RO"]], 10)),
            ("text", text_item("paragraph", "Table 8-28 SFF8024IdentifierCopy (Byte 00h:128) (cont.)", 11)),
            ("table", table_item([hdr, ["Byte", "Bits", "Name", "Description", "Type"],
                                   ["129", "7-0", "Next", "d2", "RW"]], 11)),
            ("table", table_item([hdr, ["200", "7-0", "Far", "d3", "RO"]], 25)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        self.assertEqual(len(tables), 2)  # continuation merged; distant one not
        t = tables[0]
        self.assertEqual(t["id"], "Table 8-28")
        self.assertEqual(t["section"], "8.2")
        self.assertEqual((t["page_start"], t["page_end"]), (10, 11))
        self.assertEqual(len(t["rows"]), 2)  # repeated header row dropped
        self.assertEqual(t["rows"], [["128", "7-0", "Ident", "desc", "RO"],
                                      ["129", "7-0", "Next", "d2", "RW"]])
        self.assertEqual([p["row_count"] for p in t["parts"]], [1, 1])
        self.assertEqual(tables[1]["id"], None)
        self.assertEqual(tables[1]["page_start"], 25)

    def test_uncaptioned_header_continuation_adjacent_page_only(self):
        hdr = ["Byte", "Name", "Description"]
        doc = make_doc([
            ("table", table_item([hdr, ["5", "A", "a"]], 7)),
            ("table", table_item([hdr, ["6", "B", "b"]], 8)),   # adjacent -> merge
            ("table", table_item([hdr, ["7", "C", "c"]], 20)),  # far -> no merge
        ])
        items = ds["flatten_docling_dict"](doc)
        tables = ds["build_tables"](items, ds["build_sections"](items))
        self.assertEqual(len(tables), 2)
        self.assertEqual(len(tables[0]["rows"]), 2)
        self.assertEqual((tables[0]["page_start"], tables[0]["page_end"]), (7, 8))

    def test_caption_above_table_after_footnote(self):
        # CMIS 5.4 Table 8-44: docling emits the caption above the table,
        # preceded by a footnote. It must introduce the table that follows.
        hdr = ["Byte", "Bits", "Name", "Description", "Type"]
        doc = make_doc([
            ("text", text_item("text", "...multimode fiber. The value is in units of two meters. 23", 9)),
            ("text", text_item("caption", "Table 8-44 Module Inactive Firmware and Hardware Revisions (Page 01h)", 9)),
            ("table", table_item([hdr, ["128", "7-0", "ModuleInactiveFirmwareMajorRevision", "U8", "RO Rqd."]], 9)),
        ])
        items = ds["flatten_docling_dict"](doc)
        tables = ds["build_tables"](items, ds["build_sections"](items))
        self.assertEqual(tables[0]["id"], "Table 8-44")

    def test_caption_after_table_not_previous_neighbor(self):
        # CMIS emits the caption AFTER the table; a caption item that follows
        # a table belongs to that table, never to the next one.
        hdr = ["Byte", "Bits", "Name"]
        doc = make_doc([
            ("text", text_item("text", "Some prose paragraph.", 9)),
            ("table", table_item([hdr, ["1", "7-0", "A"]], 9)),
            ("text", text_item("caption", "Table 8-44 Module Inactive Firmware and Hardware Revisions (Page 01h)", 9)),
            ("table", table_item([hdr, ["2", "7-0", "B"]], 10)),
        ])
        items = ds["flatten_docling_dict"](doc)
        tables = ds["build_tables"](items, ds["build_sections"](items))
        self.assertEqual([t["id"] for t in tables], ["Table 8-44", None])
        self.assertIn("Module Inactive Firmware", tables[0]["caption"])
        self.assertEqual(tables[1]["caption"], "")

    def test_linked_caption_attribution(self):
        # docling links captions to their table; the caption text is not a
        # body child. It must land in document.md next to the table and give
        # the table its printed id.
        tbl = table_item([["Byte", "Name"], ["7", "X"]], 9)
        tbl["captions"] = [{"$ref": "#/texts/0"}]
        doc = {
            "pages": {"9": {}},
            "body": {"children": [{"$ref": "#/tables/0"}]},
            "texts": [text_item("caption", "Table 6-2  Lower Page 00h Memory Map", 9)],
            "tables": [tbl], "pictures": [], "groups": [], "key_value_items": [],
        }
        items = ds["flatten_docling_dict"](doc)
        kinds = [(it.kind, it.label) for it in items]
        self.assertEqual(kinds, [("text", "linked_caption"), ("table", "table")])
        tables = ds["build_tables"](items, ds["build_sections"](items))
        self.assertEqual(tables[0]["id"], "Table 6-2")
        md = ds["render_document_md"](items)
        self.assertIn("Table 6-2  Lower Page 00h Memory Map", md)
        self.assertIn("| Byte | Name |", md)

    def test_toc_marking(self):
        doc = make_doc([
            ("text", text_item("section_header", "Contents", 2, level=1)),
            ("table", table_item([["Section", "Page"], ["1 Scope", "4"]], 2)),
            ("text", text_item("section_header", "1 Scope", 4, level=1)),
            ("text", text_item("paragraph", "Table 1-1 Real data", 4)),
            ("table", table_item([["Byte", "Bits"], ["0", "7-0"]], 4)),
        ])
        items = ds["flatten_docling_dict"](doc)
        tables = ds["build_tables"](items, ds["build_sections"](items))
        self.assertTrue(tables[0]["is_toc"])
        self.assertFalse(tables[1]["is_toc"])


class CorpusRegistersTests(unittest.TestCase):
    def test_cmis_register_row(self):
        hdr = ["Byte", "Bits", "Name", "Description", "Type"]
        doc = make_doc([
            ("text", text_item("section_header", "8.4 Page 01h (Advertising)", 100, level=2)),
            ("text", text_item("paragraph", "Table 8-44 Some Fields (Page 01h)", 100)),
            ("table", table_item([
                hdr,
                ["Power Alarm Thresholds", "Power Alarm Thresholds", "Power Alarm Thresholds",
                 "Power Alarm Thresholds", "Power Alarm Thresholds"],
                ["128- 129", "15- 0", "totalPwrHiAlarmThresh", "Configured threshold.", "RWW Opt."],
                ["222", "7-0", "-", "Reserved", "-"],
            ], 100)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        regs = ds["build_registers"](tables, "cmis__OIF-CMIS-05.4", "5.4",
                                      {s["id"]: s for s in secs if s["id"]})
        self.assertEqual(len(regs), 2)  # the group-header row is skipped
        r = regs[0]
        self.assertEqual(
            {k: v for k, v in r.items() if k != "raw"},
            {
                "doc": "cmis__OIF-CMIS-05.4", "revision": "5.4",
                "table": "Table 8-44", "page_address": "01h",
                "byte": "128-129", "bits": "15-0",
                "field": "totalPwrHiAlarmThresh",
                "description": "Configured threshold.", "access": "RWW Opt.",
                "page": 100,
            },
        )
        self.assertEqual(r["raw"], ["128- 129", "15- 0", "totalPwrHiAlarmThresh",
                                    "Configured threshold.", "RWW Opt."])
        self.assertIsNone(regs[1]["field"])  # "-" is not a field
        self.assertIsNone(regs[1]["access"])

    def test_sff8636_multi_access_columns(self):
        hdr = ["Byte", "Bit", "Name", "Description", "PC", "AC", "AO", "SM"]
        doc = make_doc([
            ("text", text_item("section_header", "6.2.2 Status Indicators (Page 00h, Bytes 1-2)", 32, level=3)),
            ("text", text_item("paragraph", "Table 6-3  Status Indicators (Page 00h Bytes 1-2)", 32)),
            ("table", table_item([
                hdr,
                ["1", "All", "Revision Compliance", "See Table 6-4.", "R", "R", "R", "R"],
                ["2", "7-4", "Reserved", "Module State Code.", "-", "-", "-", "-"],
            ], 32)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        regs = ds["build_registers"](tables, "sff-8636__SFF-8636-Rev2.12", "2.12",
                                      {s["id"]: s for s in secs if s["id"]})
        self.assertEqual(regs[0]["bits"], "All")
        self.assertEqual(regs[0]["access"], "R/R/R/R")
        self.assertEqual(regs[0]["page_address"], "00h")
        self.assertEqual(regs[0]["table"], "Table 6-3")
        self.assertEqual(regs[1]["access"], None)
        self.assertEqual(regs[1]["field"], "Reserved")

    def test_sff8472_a2h_from_section(self):
        hdr = ["Byte", "Bit", "Name", "Description"]
        doc = make_doc([
            ("text", text_item("section_header",
                                "10.1 Extended Module Control/Status Bytes [Address A2h, Bytes 118-119]",
                                40, level=2)),
            ("text", text_item("paragraph", "Table 9-16  Extended Status Bits", 40)),
            ("table", table_item([
                hdr,
                ["118", "3", "Soft RS(1) Select", "Read/write bit that allows software Tx rate control."],
            ], 40)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        regs = ds["build_registers"](tables, "sff-8472__SFF-8472-Rev12.5a", "12.5a",
                                      {s["id"]: s for s in secs if s["id"]})
        self.assertEqual(regs[0]["page_address"], "A2h")  # from the section title
        self.assertEqual(regs[0]["byte"], "118")
        self.assertIsNone(regs[0]["access"])  # no access column -> null

    def test_toc_tables_not_in_registers(self):
        doc = make_doc([
            ("text", text_item("section_header", "Contents", 2, level=1)),
            ("table", table_item([["Byte", "Name", "Description"], ["0", "X", "y"]], 2)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        self.assertEqual(ds["build_registers"](tables, "d", "r", {}), [])


class RevisionHistoryTests(unittest.TestCase):
    def test_table_with_header_sff8472(self):
        doc = make_doc([
            ("text", text_item("section_header", "Change History", 3, level=1)),
            ("table", table_item([
                ["Rev.", "Description", "Date"],
                ["12.4", "Replaced BR with signaling rate.", "2021-03-31"],
                ["12.5", "Section 1. Updated scope text.", "2025-08-01"],
            ], 3)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        hist = ds["build_revision_history"](tables, secs, items)
        self.assertEqual(hist, [
            {"revision": "12.4", "date": "2021-03-31",
             "description": "Replaced BR with signaling rate."},
            {"revision": "12.5", "date": "2025-08-01",
             "description": "Section 1. Updated scope text."},
        ])

    def test_headerless_table_sff8024(self):
        doc = make_doc([
            ("text", text_item("section_header", "Revision History", 2, level=1)),
            ("table", table_item([
                ["Rev 0.7", "-", "Table 3-1 changed per comments."],
                ["Rev 0.8", "-", "As requested by Transceiver SSWG, added color."],
                ["Rev 4.14", "-", "Published revision."],
            ], 2, header_rows=())),  # headerless: docling marks no header row
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        tables = ds["build_tables"](items, secs)
        hist = ds["build_revision_history"](tables, secs, items)
        self.assertEqual([e["revision"] for e in hist], ["0.7", "0.8", "4.14"])
        self.assertEqual(hist[0]["date"], None)
        self.assertEqual(hist[2]["description"], "Published revision.")

    def test_heading_form_cmis(self):
        doc = make_doc([
            ("text", text_item("section_header",
                                "Revision History (Versions Produced and Published by the QSFP-DD MSA)",
                                2, level=1)),
            ("text", text_item("section_header", "Rev 3.0", 2, level=1)),
            ("text", text_item("paragraph", "August 17, 2018", 2)),
            ("text", text_item("list_item", "Initial public release", 2)),
            ("text", text_item("section_header", "Rev 5.4", 3, level=1)),
            ("text", text_item("paragraph", "September 10, 2025 This revision adds support for stuff.", 3)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        hist = ds["build_revision_history"](ds["build_tables"](items, secs), secs, items)
        self.assertEqual(hist, [
            {"revision": "3.0", "date": "August 17, 2018",
             "description": "Initial public release"},
            {"revision": "5.4", "date": "September 10, 2025",
             "description": "This revision adds support for stuff."},
        ])

    def test_prose_form_c_cmis(self):
        doc = make_doc([
            ("text", text_item("section_header", "4 Document Revision History", 6, level=1)),
            ("text", text_item("paragraph", "Rev 1.0  - Jan 14 th 2020", 6)),
            ("text", text_item("paragraph", "Initial document release", 6)),
            ("text", text_item("paragraph", "Rev 1.2 - March 21 st 2022", 6)),
            ("text", text_item("paragraph", "Revision 1.2 contains the following updates.", 6)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        hist = ds["build_revision_history"](ds["build_tables"](items, secs), secs, items)
        self.assertEqual(hist[0]["revision"], "1.0")
        self.assertEqual(hist[0]["date"], "Jan 14 th 2020")
        self.assertEqual(hist[0]["description"], "Initial document release")
        self.assertEqual(hist[1]["revision"], "1.2")

    def test_date_in_heading_sff8636(self):
        doc = make_doc([
            ("text", text_item("section_header", "Rev 2.4 November 7, 2014:", 7, level=2)),
            ("text", text_item("paragraph", "Editorial changes only.", 7)),
        ])
        items = ds["flatten_docling_dict"](doc)
        secs = ds["build_sections"](items)
        hist = ds["build_revision_history"](ds["build_tables"](items, secs), secs, items)
        self.assertEqual(hist, [
            {"revision": "2.4", "date": "November 7, 2014",
             "description": "Editorial changes only."},
        ])


class DocMetaTests(unittest.TestCase):
    def test_merged_title_revision_heading(self):
        # docling merges the cover title with "Revision 5.2" (OIF covers) or
        # "Rev 3.0 <date>" (third-party covers) into one heading line.
        for heading in ("Common Management Interface Specification (CMIS) Revision 5.2",
                        "COMMON MANAGEMENT INTERFACE SPECIFICATION (CMIS) Rev 3.0     September 18, 2018"):
            with self.subTest(heading=heading):
                doc = make_doc([
                    ("text", text_item("section_header", "OIF-CMIS-05.2", 1, level=1)),
                    ("text", text_item("section_header", heading, 1, level=1)),
                    ("text", text_item("text", "April 27, 2022", 1)),
                    ("text", text_item("text",
                                        "Implementation Agreement created and approved OIF www.oiforum.com", 1)),
                ])
                items = ds["flatten_docling_dict"](doc)
                meta = ds["extract_docmeta"](items, "cmis", "OIF-CMIS-05.2", [])
                self.assertTrue(
                    meta["title"].upper().startswith("COMMON MANAGEMENT INTERFACE"),
                    meta["title"])
                self.assertIn("(CMIS)", meta["title"])

    def test_oif_cover(self):
        doc = make_doc([
            ("text", text_item("paragraph",
                                "OIF-CMIS-05.4 Implementation Agreement (IA) "
                                "Common Management Interface Specification (CMIS) Revision 5.4 "
                                "IA Identification # OIF-CMIS-05.4 May 21, 2026 "
                                "Implementation Agreement created and approved OIF www.oiforum.com Page 1",
                                1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](items, "cmis", "OIF-CMIS-05.4", [])
        self.assertEqual(meta["title"], "Common Management Interface Specification (CMIS)")
        self.assertEqual(meta["revision"], "5.4")
        self.assertEqual(meta["publication_date"], "May 21, 2026")
        self.assertEqual(meta["publisher"], "OIF")
        self.assertEqual(meta["ia_id"], "OIF-CMIS-05.4")

    def test_c_cmis_cover_no_revision_line(self):
        doc = make_doc([
            ("text", text_item("paragraph",
                                "OIF-C-CMIS-01.5 Implementation Agreement for Coherent CMIS "
                                "IA OIF-C-CMIS-01.5 September 1, 2026 "
                                "Implementation Agreement created and approved OIF www.oiforum.com Page 1",
                                1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](items, "cmis", "OIF-C-CMIS-01.5", [])
        self.assertEqual(meta["title"], "Coherent CMIS")
        self.assertEqual(meta["revision"], "01.5")  # from the IA id
        self.assertEqual(meta["publication_date"], "September 1, 2026")

    def test_third_party_cover(self):
        doc = make_doc([
            ("text", text_item("paragraph",
                                "COMMON MANAGEMENT INTERFACE SPECIFICATION (CMIS) "
                                "Rev 5.1 November 2, 2021 (From QSFP-DD MSA) OIF LEGAL NOTICES "
                                "THIRD PARTY SPECIFICATION THIS [STANDARD/SPECIFICATION] WAS DEVELOPED, "
                                "APPROVED, AND ORIGINALLY PUBLISHED BY QSFP-DD MSA",
                                1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](items, "cmis", "CMIS5p1_Third_Party_Spec", [])
        self.assertEqual(meta["title"], "COMMON MANAGEMENT INTERFACE SPECIFICATION (CMIS)")
        self.assertEqual(meta["revision"], "5.1")
        self.assertEqual(meta["publication_date"], "November 2, 2021")
        self.assertEqual(meta["publisher"], "QSFP-DD MSA")
        self.assertIsNone(meta["ia_id"])

    def test_sff_cover(self):
        doc = make_doc([
            ("text", text_item("title", "SFF-8636", 1)),
            ("text", text_item("paragraph",
                                "Rev 2.12 April 29, 2026 SECRETARIAT: SFF TWG "
                                "This document has been released by SNIA. The SFF TWG believes "
                                "that the ideas described in this document are technically accurate.",
                                1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](items, "sff-8636", "SFF-8636-Rev2.12", [])
        self.assertEqual(meta["title"], "SFF-8636")
        self.assertEqual(meta["revision"], "2.12")
        self.assertEqual(meta["publication_date"], "April 29, 2026")
        self.assertEqual(meta["publisher"], "SNIA")

    def test_elsfp_cover_two_line_title(self):
        doc = make_doc([
            ("text", text_item("paragraph",
                                "OIF-ELSFP-CMIS-01.0 Implementation Agreement for External Laser "
                                "Small Form Factor Pluggable (ELSFP) CMIS OIF-ELSFP-CMIS-01.0 "
                                "September 4, 2024 Implementation Agreement created and approved OIF "
                                "www.oiforum.com Page 1",
                                1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](items, "cmis", "OIF-ELSFP-CMIS-01.0", [])
        self.assertEqual(meta["title"],
                         "External Laser Small Form Factor Pluggable (ELSFP) CMIS")
        self.assertEqual(meta["revision"], "01.0")
        self.assertEqual(meta["publication_date"], "September 4, 2024")

    def test_revision_fallback_from_history(self):
        doc = make_doc([
            ("text", text_item("paragraph", "OIF-CMIS-FF-01.0 Formfactor Specific Hardware "
                                "Management CMIS-FF Implementation Agreement OIF-CMIS-FF-01.0 "
                                "September 4-2024 Implementation Agreement created and approved OIF "
                                "www.oiforum.com Page 1", 1)),
        ])
        items = ds["flatten_docling_dict"](doc)
        meta = ds["extract_docmeta"](
            items, "cmis", "OIF-CMIS-FF-01.0",
            [{"revision": "0.9", "date": None, "description": "draft"},
             {"revision": "1.0", "date": "September 4-2024", "description": "published"}],
        )
        # no Revision/Rev line; IA id suffix wins over the history
        self.assertEqual(meta["revision"], "01.0")
        self.assertEqual(meta["title"], "Formfactor Specific Hardware Management CMIS-FF")


class CatalogTests(unittest.TestCase):
    def test_catalog_ordering(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            (out / "corpus").mkdir()
            def rec(slug, fam, rev):
                return {"slug": slug, "status": "ok", "corpus": {
                    "status": "ok", "path": f"corpus/{fam}/{slug}",
                    "document": {"family": fam, "doc": f"{fam}__{slug}", "slug": slug,
                                 "revision": rev, "publication_date": None,
                                 "title": None, "publisher": None, "ia_id": None,
                                 "source_path": None, "sha256": None}}}
            manifest = [
                rec("OIF-CMIS-05.4", "cmis", "5.4"),
                rec("OIF-CMIS-05.4-changebars", "cmis", "5.4"),
                rec("CMIS3p0_Third_Party_Spec", "cmis", "3.0"),
                rec("OIF-C-CMIS-01.5", "cmis", "01.5"),
                rec("SFF-8472-Rev12.5a", "sff-8472", "12.5a"),
                {"slug": "broken.pdf", "status": "error", "error": "x"},
            ]
            missing, listed = ds["write_catalog_and_readme"](out, manifest, {})
            self.assertEqual((missing, listed), (1, 5))
            cat = json.loads((out / "corpus" / "catalog.json").read_text())
            fams = cat["families"]
            self.assertEqual([e["doc"] for e in fams["cmis"]],
                             ["cmis__OIF-C-CMIS-01.5", "cmis__CMIS3p0_Third_Party_Spec",
                              "cmis__OIF-CMIS-05.4", "cmis__OIF-CMIS-05.4-changebars"])
            self.assertEqual(len(fams["sff-8472"]), 1)
            self.assertEqual(cat["documents"][0]["family"], "cmis")
            self.assertTrue((out / "corpus" / "README.md").exists())


class BuildCorpusTests(unittest.TestCase):
    def test_build_corpus_writes_full_document_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            pdf = root / "cmis" / "OIF-TEST-01.0.pdf"
            pdf.parent.mkdir()
            pdf.write_bytes(b"fake pdf")
            out = root / ".parsed"
            hdr = ["Byte", "Bits", "Name", "Description", "Type"]
            doc = make_doc([
                ("text", text_item("title", "OIF-TEST-01.0 Implementation Agreement (IA) "
                                    "Test Spec Revision 1.0 IA Identification # OIF-TEST-01.0 "
                                    "May 21, 2026 Implementation Agreement created and approved OIF "
                                    "www.oiforum.com Page 1", 1)),
                ("text", text_item("section_header", "1 Scope", 2, level=1)),
                ("text", text_item("paragraph", "Scope prose.", 2)),
                ("text", text_item("paragraph", "Table 1-1 Fields (Page 01h)", 2)),
                ("table", table_item([hdr, ["128", "7-0", "Ident", "d", "RO"]], 2)),
                ("text", text_item("section_header", "Change History", 3, level=1)),
                ("table", table_item([["Rev.", "Description", "Date"],
                                      ["1.0", "Initial.", "May 21, 2026"]], 3)),
            ])
            res = ds["build_corpus"](doc, pdf, root, out, {"docling": "t", "chonkie": "t"},
                                    ds["sha256_of"](pdf))
            self.assertEqual(res["status"], "ok")
            cdir = out / "corpus" / "cmis" / "OIF-TEST-01.0"
            for name in ds["CORPUS_DOC_FILES"]:
                self.assertTrue((cdir / name).is_file(), name)
            dj = json.loads((cdir / "document.json").read_text())
            self.assertEqual(dj["doc"], "cmis__OIF-TEST-01.0")
            self.assertEqual(dj["revision"], "1.0")
            self.assertEqual(dj["sha256"], ds["sha256_of"](pdf))
            self.assertEqual(dj["revision_history"],
                             [{"revision": "1.0", "date": "May 21, 2026",
                               "description": "Initial."}])
            self.assertEqual(dj["counts"], {"sections": 2, "tables": 2,
                                             "registers": 1,
                                             "revision_history": 1})
            regs = [json.loads(l) for l in (cdir / "registers.jsonl").read_text().splitlines()]
            self.assertEqual(regs[0]["field"], "Ident")
            self.assertEqual(regs[0]["page_address"], "01h")
            secs = [json.loads(l) for l in (cdir / "sections.jsonl").read_text().splitlines()]
            self.assertEqual([s["id"] for s in secs], ["1", None])
            self.assertEqual([s["title"] for s in secs], ["Scope", "Change History"])
            md = (cdir / "document.md").read_text()
            self.assertIn("<!-- page 2 -->", md)
            tables = [json.loads(l) for l in (cdir / "tables.jsonl").read_text().splitlines()]
            self.assertEqual(tables[0]["id"], "Table 1-1")
            # corpus freshness round-trips on the stored sha
            self.assertTrue(ds["corpus_fresh"](cdir, ds["sha256_of"](pdf)))
            self.assertFalse(ds["corpus_fresh"](cdir, "deadbeef"))


if __name__ == "__main__":
    unittest.main()
