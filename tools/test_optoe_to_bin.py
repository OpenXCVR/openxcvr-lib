"""Direct-sysfs capture regression tests (no switch or root required)."""
import contextlib
import errno
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("optoe_to_bin", Path(__file__).with_name("optoe-to-bin.py"))
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class SysfsCaptureTests(unittest.TestCase):
    def test_single_file_sfp_a2h(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            eeprom = root / "0050"
            a0h, a2h = bytes(range(256)), bytes(reversed(range(256)))
            eeprom.write_bytes(a0h + a2h)
            capture.extract_sfp(root / "cap", interface=None, sysfs_a0h=eeprom, sysfs_a2h=None)
            self.assertEqual((root / "cap.a0h.bin").read_bytes(), a0h)
            self.assertEqual((root / "cap.a2h.bin").read_bytes(), a2h)

    def test_legacy_separate_a2h(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "0050").write_bytes(b"A" * 256)
            (root / "0051").write_bytes(b"B" * 256)
            capture.extract_sfp(root / "cap", interface=None, sysfs_a0h=root / "0050", sysfs_a2h=root / "0051")
            self.assertEqual((root / "cap.a2h.bin").read_bytes(), b"B" * 256)

    def test_enodata_is_skipped(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            eeprom = root / "0050"
            eeprom.write_bytes(b"L" * 128 + b"P" * 128)
            real_open = Path.open
            def opened(path, *args, **kwargs):
                if path == eeprom:
                    stream = real_open(path, *args, **kwargs)
                    class SparseFile:
                        def __enter__(self):
                            return self
                        def __exit__(self, *args):
                            stream.close()
                        def seek(self, offset):
                            if offset >= 256:
                                raise OSError(errno.ENODATA, "page absent")
                            return stream.seek(offset)
                        def read(self, length):
                            return stream.read(length)
                    return SparseFile()
                return real_open(path, *args, **kwargs)
            with patch.object(Path, "open", opened), contextlib.redirect_stderr(io.StringIO()) as log:
                files = capture.extract_paged(root / "cap", spec="cmis", interface=None, sysfs=eeprom, sfputil_port=None)
            self.assertEqual(len(files), 2)
            self.assertIn("skipping page", log.getvalue())

    def test_short_read_and_missing_file_are_read_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "short").write_bytes(b"x")
            for path in [root / "short", root / "missing"]:
                with self.assertRaises(capture.ReadError):
                    capture.read_via_sysfs(path)


if __name__ == "__main__":
    unittest.main()
