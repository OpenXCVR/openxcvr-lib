"""Tests for tools/distill-capture.py — capture → scrubbed synthetic-fixture
builder inputs.

Every "capture" used here is itself synthetic (built in memory), so these
tests add no vendor-derived bytes to the repository. They pin:

- the schema-driven scrub (offsets, marker replacement, zeroing),
- checksum recomputation for scrub-intersecting checksums and preservation
  of untouched ones,
- the fail-closed rules (size, unknown spec/slice/page, half-cut vendor
  ranges, unreadable input),
- that no original vendor identity byte or path ever reaches the outputs.
"""

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

spec_ = importlib.util.spec_from_file_location(
    "distill_capture", Path(__file__).with_name("distill-capture.py"))
distill_capture = importlib.util.module_from_spec(spec_)
spec_.loader.exec_module(distill_capture)

ROOT = Path(__file__).resolve().parents[1]


def sff8472_a0h_capture(vendor=b"ACME OPTICS", pn=b"ACME-PN-1", sn=b"SN-000042",
                        oui=b"\x01\x02\x03") -> bytes:
    """A synthetic 'raw capture': plausible identity strings, valid checksums."""
    data = bytearray(256)
    data[0] = 0x03                                   # Identifier: SFP
    data[20:36] = vendor.ljust(16, b" ")
    data[37:40] = oui
    data[40:56] = pn.ljust(16, b" ")
    data[56:60] = b"A1  "
    data[68:84] = sn.ljust(16, b" ")
    data[84:92] = b"26010100"
    data[63] = sum(data[0:63]) & 0xFF                # cc_base
    data[95] = sum(data[64:95]) & 0xFF               # cc_ext
    data[200:220] = b"SECRETVENDORPAYLOADS"       # vendor-specific region
    return bytes(data)


def sff8636_page00h_capture() -> bytes:
    """128-byte upper Page 00h with identity strings at table 148+ (file 20+)."""
    data = bytearray(128)
    data[0] = 0x0D
    data[20:36] = b"VND" .ljust(16, b" ")
    data[37:40] = b"\x0a\x0b\x0c"
    data[40:56] = b"VND-QSFP" .ljust(16, b" ")
    data[56:58] = b"42"
    data[63] = sum(data[0:63]) & 0xFF                # table 191: covers 128-190
    data[95] = sum(data[64:95]) & 0xFF               # table 223: covers 192-222
    return bytes(data)


def cmis_page00h_capture() -> bytes:
    """128-byte upper Page 00h with identity strings at table 129+ (file 1+)."""
    data = bytearray(128)
    data[0] = 0x18
    data[1:17] = b"CMISVND".ljust(16, b" ")
    data[17:20] = b"\x11\x22\x33"
    data[20:36] = b"CMIS-PN".ljust(16, b" ")
    data[36:38] = b"01"
    data[38:54] = b"CMIS-SN".ljust(16, b" ")
    data[54:62] = b"26010101"
    data[94] = sum(data[0:94]) & 0xFF                # table 222: covers 128-221
    return bytes(data)


class DistillTests(unittest.TestCase):
    def run_tool(self, *argv):
        with tempfile.TemporaryDirectory() as tmp:
            outdir = Path(tmp) / "out"
            code = distill_capture.main(list(argv) + ["--outdir", str(outdir)])
            return code, outdir

    def test_sff8472_a0h_scrub_marker_and_zero_and_checksums(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = sff8472_a0h_capture()
            (root / "cap.a0h.bin").write_bytes(raw)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.a0h.bin").read_bytes()
            self.assertEqual(len(out), 256)
            self.assertEqual(out[20:36], b"SYNTHETIC".ljust(16, b" "))
            self.assertEqual(out[40:56], b"SYNTHETIC".ljust(16, b" "))
            self.assertEqual(out[68:84], b"SYNTHETIC".ljust(16, b" "))
            self.assertEqual(out[84:92], b"SYNTHETI")  # 8-byte date_code truncates the marker
            self.assertEqual(out[37:40], b"\x00\x00\x00")        # vendor_oui zeroed
            self.assertEqual(out[96:128], b"\x00" * 32)         # vendor_specific
            self.assertEqual(out[128:256], b"\x00" * 128)       # vendor_specific
            self.assertEqual(out[200:220], b"\x00" * 20)        # payload gone
            self.assertEqual(out[0], 0x03)                      # identifier kept
            # Checksums recomputed over the scrubbed bytes.
            self.assertEqual(out[63], sum(out[0:63]) & 0xFF)
            self.assertEqual(out[95], sum(out[64:95]) & 0xFF)
            report = json.loads(
                (root / "out" / "t.a0h.bin.distill.json").read_text())
            self.assertEqual(report["schema"]["spec"], "sff-8472")
            self.assertEqual(report["size_bytes"], 256)
            self.assertIn("cc_base", report["checksums_recomputed"])
            self.assertIn("cc_ext", report["checksums_recomputed"])
            self.assertEqual(report["output_sha256"], hashlib.sha256(out).hexdigest())
            scrubbed_names = [r["field"] for r in report["scrubbed"]]
            self.assertIn("vendor_name", scrubbed_names)
            self.assertIn("vendor_sn", scrubbed_names)

    def test_no_original_identity_reaches_outputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = sff8472_a0h_capture(vendor=b"ACME OPTICS", sn=b"SN-000042")
            (root / "private-cap.a0h.bin").write_bytes(raw)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "private-cap.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            outdir = root / "out"
            for path in outdir.iterdir():
                blob = path.read_bytes()
                self.assertNotIn(b"ACME OPTICS", blob, f"{path.name} leaks vendor name")
                self.assertNotIn(b"SN-000042", blob, f"{path.name} leaks serial")
                self.assertNotIn(b"SECRETVENDORPAYLOAD", blob, f"{path.name} leaks vendor bytes")
                self.assertNotIn(b"private-cap", blob, f"{path.name} leaks the input path")

    def test_sff8636_page00h_offsets_and_both_checksums(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cap.page-00h.bin").write_bytes(sff8636_page00h_capture())
            code = distill_capture.main([
                "--spec", "sff-8636", "--input", str(root / "cap.page-00h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.page-00h.bin").read_bytes()
            self.assertEqual(out[20:36], b"SYNTHETIC".ljust(16, b" "))
            self.assertEqual(out[0], 0x0D)                       # untouched byte kept
            self.assertEqual(out[63], sum(out[0:63]) & 0xFF)
            self.assertEqual(out[95], sum(out[64:95]) & 0xFF)
            report = json.loads(
                (root / "out" / "t.page-00h.bin.distill.json").read_text())
            self.assertEqual(report["suffix"], ".page-00h.bin")
            self.assertEqual(len(report["checksums_recomputed"]), 2)

    def test_cmis_page00h_includes_clei_privacy_exception(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            data = bytearray(cmis_page00h_capture())
            data[62:72] = b"CLEI1234XY"                        # clei_code, table 190
            data[94] = sum(data[0:94]) & 0xFF
            (root / "cap.page-00h.bin").write_bytes(bytes(data))
            code = distill_capture.main([
                "--spec", "cmis", "--input", str(root / "cap.page-00h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.page-00h.bin").read_bytes()
            self.assertEqual(out[1:17], b"SYNTHETIC".ljust(16, b" "))
            self.assertNotIn(b"CLEI1234XY", out)
            self.assertEqual(out[94], sum(out[0:94]) & 0xFF)
            report = json.loads(
                (root / "out" / "t.page-00h.bin.distill.json").read_text())
            self.assertIn("clei_code", [r["field"] for r in report["scrubbed"]])
            self.assertIn("page_checksum", report["checksums_recomputed"])

    def test_distillation_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cap.a0h.bin").write_bytes(sff8472_a0h_capture())
            distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            first = (root / "out" / "t.a0h.bin").read_bytes()
            # Distilling the already-scrubbed output changes nothing.
            (root / "again.a0h.bin").write_bytes(first)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "again.a0h.bin"),
                "--name", "t2", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            self.assertEqual((root / "out" / "t2.a0h.bin").read_bytes(), first)

    def test_a2h_default_zeroes_undeclared_paged_region(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            data = bytearray(256)
            data[120:127] = b"VSPEC!"                        # vendor_specific_a2h_120_126
            data[128:256] = b"USER EEPROM CONTENT".ljust(128, b"\x00")
            (root / "cap.a2h.bin").write_bytes(bytes(data))
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a2h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.a2h.bin").read_bytes()
            self.assertEqual(out[120:127], b"\x00" * 7)
            self.assertNotIn(b"USER EEPROM CONTENT", out)
            self.assertEqual(out[128:256], b"\x00" * 128)

    def test_a2h_declared_user_eeprom_page_zeroed_per_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            data = bytearray(256)
            data[128:248] = b"U" * 120                       # user_eeprom, table 128-247
            data[248:256] = b"V" * 8                         # vendor control, table 248-255
            (root / "cap.a2h.bin").write_bytes(bytes(data))
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a2h.bin"),
                "--name", "t", "--a2h-page", "00h",
                "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.a2h.bin").read_bytes()
            self.assertEqual(out[128:256], b"\x00" * 128)
            report = json.loads(
                (root / "out" / "t.a2h.bin.distill.json").read_text())
            self.assertEqual(report["schema"]["a2h_page"], "00h")
            names = [r["field"] for r in report["scrubbed"]]
            # The base schema's storage/vendor_specific ranges cover the same
            # bytes as the page-00h fields, so merged ranges carry base names.
            self.assertIn("user_eeprom_or_paged_region", names)
            self.assertIn("vendor_specific_a2h_120_126", names)
            self.assertIn("paged_region_vendor_specific_tail", names)

    def test_marker_override_and_custom_marker_appears(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cap.a0h.bin").write_bytes(sff8472_a0h_capture())
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "t", "--marker", "OPENXCVR-SYNTH",
                "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            out = (root / "out" / "t.a0h.bin").read_bytes()
            self.assertEqual(out[20:36], b"OPENXCVR-SYNTH".ljust(16, b" "))

    def test_prefix_capture_with_half_cut_vendor_range_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # 90 bytes: date_code (84-91) is cut in half by the capture end.
            (root / "short.a0h.bin").write_bytes(sff8472_a0h_capture()[:90])
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "short.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 1)
            self.assertFalse((root / "out").exists() and (root / "out" / "t.a0h.bin").exists())

    def test_prefix_capture_without_vendor_overlap_is_allowed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # 128 bytes: like the committed sff-8472 corpus captures — the
            # 128-255 vendor region is simply absent, not half-cut.
            (root / "cap.a0h.bin").write_bytes(sff8472_a0h_capture()[:128])
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 0)
            report = json.loads(
                (root / "out" / "t.a0h.bin.distill.json").read_text())
            self.assertIn("vendor_specific_a0h_128_255",
                          report["ranges_absent_from_capture"])
            self.assertEqual(report["size_bytes"], 128)

    def test_oversize_capture_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "big.a0h.bin").write_bytes(sff8472_a0h_capture() + b"\x00" * 4)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "big.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 1)

    def test_unknown_slice_and_page_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cap.page-01h.bin").write_bytes(b"\x00" * 128)
            # SFF-8636 has no page 01h schema.
            code = distill_capture.main([
                "--spec", "sff-8636", "--input", str(root / "cap.page-01h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 1)
            # A filename with no recognizable slice needs --slice.
            (root / "cap.bin").write_bytes(b"\x00" * 128)
            with self.assertRaises(SystemExit) as raised:
                distill_capture.main([
                    "--spec", "sff-8636", "--input", str(root / "cap.bin"),
                    "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(raised.exception.code, 2)  # argparse error

    def test_unreadable_input_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "missing.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out")])
            self.assertEqual(code, 1)

    def test_refuses_to_overwrite_the_raw_capture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = sff8472_a0h_capture()
            (root / "cap.a0h.bin").write_bytes(raw)
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "cap", "--outdir", str(root)])
            self.assertEqual(code, 1)
            self.assertEqual((root / "cap.a0h.bin").read_bytes(), raw)

    def test_provenance_stub_written(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cap.a0h.bin").write_bytes(sff8472_a0h_capture())
            code = distill_capture.main([
                "--spec", "sff-8472", "--input", str(root / "cap.a0h.bin"),
                "--name", "t", "--outdir", str(root / "out"),
                "--provenance", str(root / "out" / "t.md")])
            self.assertEqual(code, 0)
            stub = (root / "out" / "t.md").read_text()
            self.assertIn("distill-capture.py", stub)
            self.assertIn("t.a0h.bin.distill.json", stub)
            self.assertIn("**not** in this repository", stub)


if __name__ == "__main__":
    unittest.main()
