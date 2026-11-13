"""Focused tests for the CMIS Page 11h diagnostic
(`tools/decode-cmis-page11h-golden.py`).

Pins, against literal spec transcriptions (never a decoder run):
1. Known CMIS 3.0 reserved labels — the diagnostic selects the schema-owned
   generated projection for the fixture's revision byte (Lower:1), so a 3.0
   module renders raw 5h/6h/7h as Unknown5/Unknown6/Unknown7 per CMIS 3.0
   § 1.7.7.1 Table 66, with the 0h/3h/4h controls Reserved0/DpDeinit/
   DpActivated.
2. Known later-revision names — a 5.3 module renders raw 5h/6h/7h with the
   CMIS 4.0+ Table 8-58 names (DPTxTurnOn/DPTxTurnOff/DPInitialized).
3. Unsupported revision contexts fail closed: invoking the ACTUAL diagnostic
   CLI on a complete present-page fixture whose Lower:1 is 0x05 or 0x55
   exits nonzero with a specific unsupported-revision diagnostic naming the
   byte — never an import/file/setup error and never borrowed labels.
   (CMIS 5.4 / 0x54 was registered as an admitted revision by 19efcdd, so it
   decodes; 0x55 is the unadmitted future revision that pins refusal.)
4. The missing-Page 11h behavior `(None, None)` is retained.

The original anomalous corpus golden (fboss-cmis-anomalous-qsfpdd-400g-cr8,
the intentional 0x05 rejection fixture) is never rewritten by these tests:
temporary fixtures are authored under a temp dir.

Run:
    python3 -m unittest tools.test_decode_cmis_page11h_golden -v
"""
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

TOOLS = pathlib.Path(__file__).resolve().parent
REPO = TOOLS.parent
DIAGNOSTIC = TOOLS / "decode-cmis-page11h-golden.py"


def _load_diagnostic():
    spec = importlib.util.spec_from_file_location("decode_cmis_page11h_golden", DIAGNOSTIC)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _lower(rev_byte: int) -> bytes:
    """Synthetic paged Lower page: identifier 0x18, Lower:1 = rev byte,
    Lower:2 = 0 (paged)."""
    buf = bytearray(128)
    buf[0] = 0x18
    buf[1] = rev_byte
    return bytes(buf)


def _state_page(state_bytes: bytes) -> bytes:
    """Page 11h carrying `state_bytes` at the four DP-state bytes (flat
    128..131), everything else zero."""
    buf = bytearray(128)
    buf[0:4] = state_bytes
    return bytes(buf)


class DiagnosticLabelTests(unittest.TestCase):
    def setUp(self):
        self.diag = _load_diagnostic()

    def _lanes_for(self, rev_byte: int, state_bytes: bytes):
        lanes, _flags = self.diag.decode(_lower(rev_byte), bytes(128), _state_page(state_bytes))
        return lanes

    def test_cmis_3_0_reserved_labels_are_table66_literals(self):
        # The reserved fixture's literal lane vector 0h,5h,6h,7h,3h,4h,0h,3h
        # (packed `50 76 43 30` per Table 65): CMIS 3.0 Table 66 leaves
        # 5h-7h Reserved (Unknown<n>), with the 0h/3h/4h controls named.
        lanes = self._lanes_for(0x30, bytes.fromhex("50764330"))
        want = [
            "Reserved0",
            "Unknown5",
            "Unknown6",
            "Unknown7",
            "DpDeinit",
            "DpActivated",
            "Reserved0",
            "DpDeinit",
        ]
        self.assertEqual([lane["dp_state"] for lane in lanes], want)
        # Monitors stay zero and the lane block shape is unchanged.
        for i, lane in enumerate(lanes):
            self.assertEqual(lane["index"], i + 1)
            self.assertEqual(lane["rx_power_mw"], 0.0)
            self.assertEqual(lane["tx_bias_ma"], 0.0)
            self.assertEqual(lane["tx_power_mw"], 0.0)

    def test_cmis_3_0_named_controls_are_table66_literals(self):
        # 1h Deactivated, 2h Init (duplicated across both nibbles).
        for raw, name in [(0x1, "DpDeactivated"), (0x2, "DpInit")]:
            lanes = self._lanes_for(0x30, bytes([raw | (raw << 4)] * 4))
            self.assertEqual([lane["dp_state"] for lane in lanes], [name] * 8)

    def test_later_revision_names_are_table8_58_literals(self):
        # CMIS 5.3 (Table 8-84 = 4.0 Table 8-58 encodings): 5h/6h/7h named.
        for raw, name in [
            (0x5, "DpTxTurnOn"),
            (0x6, "DpTxTurnOff"),
            (0x7, "DpInitialized"),
        ]:
            lanes = self._lanes_for(0x53, bytes([raw | (raw << 4)] * 4))
            self.assertEqual([lane["dp_state"] for lane in lanes], [name] * 8)

    def test_reserved_range_renders_unknown_on_every_revision(self):
        # 8h..Fh are unnamed on every revision: Unknown<n> fallback.
        for rev_byte in (0x30, 0x40, 0x53):
            for raw in range(0x8, 0x10):
                lanes = self._lanes_for(rev_byte, bytes([raw | (raw << 4)] * 4))
                self.assertEqual(
                    [lane["dp_state"] for lane in lanes],
                    [f"Unknown{raw}"] * 8,
                    f"rev {rev_byte:#04x} raw {raw:#04x}",
                )

    def test_missing_page11_keeps_none_none(self):
        # A stem with no Page 11h slice: (None, None), no error, no lanes.
        with tempfile.TemporaryDirectory() as tmp:
            fixdir = pathlib.Path(tmp)
            stem = "no-page11"
            (fixdir / f"{stem}.lower.bin").write_bytes(_lower(0x30))
            (fixdir / f"{stem}.page-01h.bin").write_bytes(bytes(128))
            self.assertEqual(self.diag.decode_fixture(str(fixdir), stem), (None, None))


class DiagnosticCliUnsupportedRevisionTests(unittest.TestCase):
    def _run_cli(self, rev_byte: int):
        """Invoke the ACTUAL diagnostic CLI on a complete present-page
        fixture authored in a temp dir with Lower:1 = rev_byte."""
        with tempfile.TemporaryDirectory() as tmp:
            fixdir = pathlib.Path(tmp)
            stem = "temp-unsupported-rev"
            (fixdir / f"{stem}.lower.bin").write_bytes(_lower(rev_byte))
            (fixdir / f"{stem}.page-01h.bin").write_bytes(bytes(128))
            (fixdir / f"{stem}.page-11h.bin").write_bytes(_state_page(bytes.fromhex("50764330")))
            return subprocess.run(
                [sys.executable, str(DIAGNOSTIC), stem, str(fixdir)],
                capture_output=True,
                text=True,
                timeout=60,
            )

    def _assert_unsupported_diagnostic(self, rev_byte: int):
        result = self._run_cli(rev_byte)
        combined = result.stdout + result.stderr
        self.assertNotEqual(
            result.returncode,
            0,
            f"rev {rev_byte:#04x}: CLI must exit nonzero on an unsupported context",
        )
        self.assertIn(
            f"0x{rev_byte:02X}",
            combined,
            f"rev {rev_byte:#04x}: diagnostic must name the byte",
        )
        self.assertIn(
            "unsupported",
            combined.lower(),
            f"rev {rev_byte:#04x}: diagnostic must say the revision is unsupported",
        )
        self.assertNotIn(
            "Traceback",
            combined,
            f"rev {rev_byte:#04x}: must be a diagnostic error, not an exception",
        )
        self.assertNotIn(
            "dp_state",
            combined,
            f"rev {rev_byte:#04x}: no labels may be rendered for an unsupported context",
        )
        return result

    def test_cli_rejects_rev_0x05_with_a_specific_diagnostic(self):
        # 0x05 (major 0): not an admitted CMIS context. The anomalous corpus
        # fixture/golden is untouched — this runs on a temporary fixture.
        self._assert_unsupported_diagnostic(0x05)

    def test_cli_rejects_rev_0x55_with_a_specific_diagnostic(self):
        # 0x55 (CMIS 5.5, an unadmitted future revision). 0x54 / CMIS 5.4 was
        # registered as admitted by 19efcdd (schemas/cmis/_index.yaml revision
        # "5.4", generated enums 0x54 → v5_4), so the diagnostic now decodes
        # it — this pin moved to the next unadmitted byte, matching the
        # retarget that commit applied to every other 0x54-as-invalid test.
        self._assert_unsupported_diagnostic(0x55)

    def test_cli_rejects_neighbor_revisions_0x31_0x41(self):
        for rev_byte in (0x31, 0x41):
            self._assert_unsupported_diagnostic(rev_byte)

    def test_cli_decodes_a_supported_revision(self):
        with tempfile.TemporaryDirectory() as tmp:
            fixdir = pathlib.Path(tmp)
            stem = "temp-supported-rev"
            (fixdir / f"{stem}.lower.bin").write_bytes(_lower(0x30))
            (fixdir / f"{stem}.page-01h.bin").write_bytes(bytes(128))
            (fixdir / f"{stem}.page-11h.bin").write_bytes(_state_page(bytes.fromhex("50764330")))
            result = subprocess.run(
                [sys.executable, str(DIAGNOSTIC), stem, str(fixdir)],
                capture_output=True,
                text=True,
                timeout=60,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(
            [lane["dp_state"] for lane in payload["lanes"]],
            [
                "Reserved0",
                "Unknown5",
                "Unknown6",
                "Unknown7",
                "DpDeinit",
                "DpActivated",
                "Reserved0",
                "DpDeinit",
            ],
        )


if __name__ == "__main__":
    unittest.main()
