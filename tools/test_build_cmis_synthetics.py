"""Focused golden-builder tests for the synthetic CMIS fixtures.

Pins the reproducibility and spec-pinned semantics of
`tools/build-cmis-3-0-synthetic.py` and `tools/build-cmis-5-2-synthetic.py`:

1. Deterministic reproduction — the builders' in-memory pages and expected
   goldens are byte-for-byte identical to the committed `.bin` fixtures and
   `.json` goldens, and `main()` rewrites the same bytes end-to-end into a
   temp directory without touching the repo tree.
2. Revision-own semantics — the expected goldens encode the CMIS revision's
   OWN tables, never borrowed 4.0+/5.3 semantics:
   - zero Page 11h DPState nibbles are Reserved (CMIS 3.0 § 1.7.7.1
     Table 66; CMIS 5.2 § 8.10.1 Table 8-77), rendered "Reserved0" — NOT
     "DpDeactivated" (that is 1h in every revision);
   - the CMIS 3.0 builder does not name 5h-7h (Reserved in 3.0; the
     DPTxTurnOn/DPTxTurnOff/DPInitialized meanings only exist from
     CMIS 4.0 on — 4.0 Table 8-58, 5.2 Table 8-77);
   - the CMIS 5.2 builder does name 5h-7h (Table 8-77 defines them).
3. Honest metadata — device revision vs canonical schema namespace,
   synthetic / monitoring_stale / vendor_strings_synthetic classification.
4. Page 11h lane-monitor arithmetic against the revision's own tables —
   CMIS 3.0 Table 70 / CMIS 5.2 Table 8-82 place the monitors at flat
   bytes 154-169/170-185/186-201, i.e. page-relative [26+2i]/[42+2i]/
   [58+2i] in the 128-byte Page 11h buffer; powers are big-endian U16 in
   0.1 uW increments, bias in 2 uA increments times the Page 01h byte 160
   bits 4-3 multiplier (CMIS 3.0 Table 45 / CMIS 5.2 Table 8-47:
   00b x1, 01b x2, 10b x4, 11b reserved -> None).
5. The unsupported-flags guard slices the revision's actual flag region
   (3.0 Tables 67-69: flat 134-152; 5.2 Table 8-75: flat 134-153) in the
   128-byte page-relative buffer — 3.0's flat byte 153 is Reserved
   (Table 70) and must not be claimed as a flag.
6. Lower byte 3 bit 0 (3.0 Table 18 "Interrupt"; 5.2 Table 8-6
   "InterruptDeasserted"): 0b = asserted, 1b = not asserted, read in
   isolation from the ModuleState bits 3-1 in the same byte.
7. Nonconformance disclosure — the all-zero Application advertisement
   violates CMIS 3.0 § 1.7.2.7 / CMIS 5.2 § 8.2.11 (at least one
   advertised Application; FFh first-unused terminator); the bytes stay
   authored as-is and `decode_status: "ok"` is decode-success, not
   conformance. The 3.0 bias-multiplier citation is Table 45, not
   Table 42 (Module Characteristics Advertising).

Run:
    python3 -m unittest tools.test_build_cmis_synthetics
"""
import contextlib
import hashlib
import importlib
import importlib.util
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

TOOLS = pathlib.Path(__file__).resolve().parent
REPO = TOOLS.parent

BUILDERS = {
    "3.0": TOOLS / "build-cmis-3-0-synthetic.py",
    "5.2": TOOLS / "build-cmis-5-2-synthetic.py",
}

PAGE_TAGS = ("lower", "page-00h", "page-01h", "page-02h", "page-10h", "page-11h")

# The reserved variant's literal CMIS 3.0 Table 66 lane vector (lanes 1..8):
# 0h Reserved, 5h Reserved (3.0), 6h Reserved (3.0), 7h Reserved (3.0),
# 3h Deinit, 4h Activated, 0h Reserved, 3h Deinit. Packed per Table 65
# (odd lanes bits 3-0, even lanes bits 7-4) this is `50 76 43 30`.
RESERVED_LANE_VECTOR = (0x0, 0x5, 0x6, 0x7, 0x3, 0x4, 0x0, 0x3)
RESERVED_P11_PREFIX = bytes.fromhex("50764330")
RESERVED_DP_LABELS = (
    "Reserved0",
    "Unknown5",
    "Unknown6",
    "Unknown7",
    "DpDeinit",
    "DpActivated",
    "Reserved0",
    "DpDeinit",
)


def _load(path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SyntheticBuilderTests(unittest.TestCase):
    """Shared checks, instantiated per revision via subTest."""

    def _committed_fixture(self, stem: str, tag: str) -> bytes:
        return (REPO / "tests" / "fixtures" / "cmis" / f"{stem}.{tag}.bin").read_bytes()

    def _committed_golden_text(self, stem: str) -> str:
        return (REPO / "tests" / "golden" / "cmis" / f"{stem}.json").read_text(
            encoding="utf-8"
        )

    def _pages(self, module) -> dict:
        return {
            "lower": module.lower_memory(),
            "page-00h": module.page_00h(),
            "page-01h": module.page_01h(),
            "page-02h": module.page_02h(),
            "page-10h": module.page_10h(),
            "page-11h": module.page_11h(),
        }

    def _golden_with_overrides(self, module, lower=None, p01=None, p11=None):
        """expected_golden() over the builder's authored pages, with overrides.

        The overrides are hand-authored bytes placed at the primary-table
        offsets, so every expected value in the tests below is derived from
        the spec tables independently of the builder under test — never
        from the builder's own arithmetic.
        """
        pages = self._pages(module)
        return module.expected_golden(
            pages["lower"] if lower is None else lower,
            pages["page-00h"],
            pages["page-01h"] if p01 is None else p01,
            pages["page-11h"] if p11 is None else p11,
        )

    def test_builders_reproduce_committed_artifacts(self):
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                stem = module.STEM
                pages = self._pages(module)

                # 128 bytes per page, byte-for-byte equal to the committed
                # fixtures (the binaries must never drift from the builder).
                for tag in PAGE_TAGS:
                    self.assertEqual(len(pages[tag]), 128, f"{stem}.{tag} size")
                    committed = self._committed_fixture(stem, tag)
                    self.assertEqual(
                        pages[tag],
                        committed,
                        f"{stem}.{tag}.bin drifted from {path.name}",
                    )

                # The expected golden regenerates the committed JSON exactly
                # (deterministic: pinned authorship stamp, sorted keys).
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                rendered = json.dumps(golden, indent=2, sort_keys=True) + "\n"
                self.assertEqual(
                    rendered,
                    self._committed_golden_text(stem),
                    f"{stem}.json drifted from {path.name}",
                )

    def _builder_stems(self, module) -> list:
        """Every stem a builder's main() emits (the 3.0 builder emits both
        the all-zero baseline and the reserved-DPSM variant)."""
        stems = [module.STEM]
        reserved = getattr(module, "RESERVED_STEM", None)
        if reserved is not None:
            stems.append(reserved)
        return stems

    def _golden_path_for(self, module, stem: str) -> pathlib.Path:
        """The module-level golden path constant covering `stem`."""
        if stem == module.STEM:
            return module.GOLDEN_PATH
        reserved = getattr(module, "RESERVED_GOLDEN_PATH", None)
        assert reserved is not None, f"no reserved golden path constant for {stem}"
        return reserved

    def test_main_rewrites_identical_artifacts_without_touching_repo(self):
        tracked = sorted(
            (REPO / "tests" / "fixtures" / "cmis").glob("*.bin")
        ) + sorted((REPO / "tests" / "golden" / "cmis").glob("*.json"))
        repo_hashes = {
            p: hashlib.sha256(p.read_bytes()).hexdigest() for p in tracked
        }
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                with tempfile.TemporaryDirectory() as tmp:
                    root = pathlib.Path(tmp)
                    # Redirect every path main() writes to / prints about.
                    fixture_dir = root / "tests" / "fixtures" / "cmis"
                    golden_dir = root / "tests" / "golden"
                    module.FIXTURE_DIR = fixture_dir
                    module.GOLDEN_DIR_ROOT = golden_dir
                    module.GOLDEN_PATH = golden_dir / "cmis" / f"{module.STEM}.json"
                    if hasattr(module, "RESERVED_GOLDEN_PATH"):
                        module.RESERVED_GOLDEN_PATH = (
                            golden_dir / "cmis" / f"{module.RESERVED_STEM}.json"
                        )
                    fixture_dir.mkdir(parents=True)
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(module.main(), 0)

                    # main() writes every stem the builder owns: for the 3.0
                    # builder that is BOTH the baseline and the reserved
                    # variant; each page equals the committed fixture bytes.
                    for stem in self._builder_stems(module):
                        for tag in PAGE_TAGS:
                            self.assertEqual(
                                (fixture_dir / f"{stem}.{tag}.bin").read_bytes(),
                                self._committed_fixture(stem, tag),
                                f"main() wrote a different {stem}.{tag}.bin",
                            )
                        self.assertEqual(
                            self._golden_path_for(module, stem).read_text(
                                encoding="utf-8"
                            ),
                            self._committed_golden_text(stem),
                            f"main() wrote a different {stem}.json",
                        )

        # The repo tree is untouched by the redirected runs.
        for p, digest in repo_hashes.items():
            self.assertEqual(
                hashlib.sha256(p.read_bytes()).hexdigest(),
                digest,
                f"{p} changed while builders ran against a temp dir",
            )

    def test_zero_page11h_dp_state_is_reserved_not_deactivated(self):
        # CMIS 3.0 § 1.7.7.1 Table 66 and CMIS 5.2 § 8.10.1 Table 8-77 both
        # define 0h as Reserved; DPDeactivated is 1h. The all-zero Page 11h
        # must therefore render "Reserved0" on every lane — never a
        # Deactivated label (the pre-golden builder comments claimed exactly
        # that, and this test keeps the corrected semantics pinned).
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                self.assertEqual(len(golden["lanes"]), 8)
                for lane in golden["lanes"]:
                    self.assertEqual(lane["dp_state"], "Reserved0")
                    self.assertNotEqual(lane["dp_state"], "DpDeactivated")

    def test_revision_own_dp_state_tables(self):
        # CMIS 3.0 Table 66: 5h-Fh Reserved — the builder must NOT carry the
        # 4.0+ names for 5h-7h (CMIS 4.0 Table 8-58 defined them; borrowing
        # them for a 3.0 module would substitute later-revision semantics).
        module_3_0 = _load(BUILDERS["3.0"])
        self.assertEqual(
            set(module_3_0.DP_STATE_NAMES),
            {0x0, 0x1, 0x2, 0x3, 0x4},
            "CMIS 3.0 names must cover only Table 66's 0h-4h",
        )
        for nibble in (5, 6, 7):
            self.assertEqual(module_3_0._dp_state_name(nibble), f"Unknown{nibble}")

        # CMIS 5.2 Table 8-77 defines 5h DPTxTurnOn / 6h DPTxTurnOff /
        # 7h DPInitialized; 8h-Fh stay Reserved.
        module_5_2 = _load(BUILDERS["5.2"])
        self.assertEqual(
            set(module_5_2.DP_STATE_NAMES),
            set(range(0x0, 0x8)),
            "CMIS 5.2 names must cover Table 8-77's 0h-7h",
        )
        self.assertEqual(module_5_2._dp_state_name(0x5), "DpTxTurnOn")
        self.assertEqual(module_5_2._dp_state_name(0x8), "Unknown8")

    def test_zero_lower_memory_decodes_reserved_module_state_and_asserted_interrupt(self):
        # CMIS 3.0 Tables 18/19 and CMIS 5.2 Tables 8-6/8-7: ModuleState
        # 000b = Reserved; byte 3 bit 0 = 0b means Interrupt asserted.
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                self.assertEqual(golden["module"]["module_state"], "Reserved0")
                self.assertTrue(golden["flags"]["interrupt_asserted"])
                self.assertEqual(golden["module"]["memory_model"], "paged")
                self.assertEqual(
                    golden["monitoring"],
                    {"module_temperature_c": 0.0, "module_voltage_v": 0.0},
                )

    def test_no_applications_advertised_renders_null(self):
        # Lower 86-117 (AppSel 1-8) and Page 01h 223-250 (AppSel 9-15) are
        # all zero: no slot is advertised, and the cross-binding contract
        # renders an empty advertisement as null (never an empty array and
        # never an invented descriptor).
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                self.assertIsNone(golden["applications"])

    def test_names_are_schema_approved(self):
        # Golden *.name values must be byte-identical to the locked schema
        # enum names (golden README "Enum-name source rule").
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                self.assertEqual(golden["module"]["identifier"]["name"], "qsfp_dd")
                self.assertEqual(golden["module"]["identifier"]["raw"], "0x18")
                self.assertEqual(
                    golden["module"]["connector"]["name"], "unknown_or_unspecified"
                )
                self.assertEqual(golden["module"]["connector"]["raw"], "0x00")
                self.assertIsNone(golden["module"]["module_type"])

    def test_metadata_reports_device_revision_and_canonical_schema_honestly(self):
        cases = {
            "3.0": ("3.0", "CMIS3p0_Third_Party_Spec.pdf"),
            "5.2": ("5.2", "OIF-CMIS-05.2.pdf"),
        }
        for rev, (device_rev, spec_pdf) in cases.items():
            with self.subTest(revision=rev):
                module = _load(BUILDERS[rev])
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                meta = golden["_metadata"]
                # Device revision (decoded from Lower:1) vs canonical schema
                # namespace (the cmis-5.3 namespace the generated decoders
                # are emitted from) are reported separately, never conflated.
                self.assertEqual(meta["cmis_version"], device_rev)
                self.assertEqual(golden["module"]["cmis_revision"], device_rev)
                self.assertEqual(meta["spec"], "cmis-5.3")
                self.assertEqual(meta["spec_revision"], "5.3")
                self.assertEqual(meta["schema_version"], "openxcvr.golden.v1")
                # Synthetic classification is machine-readable.
                self.assertTrue(meta["synthetic"])
                self.assertTrue(meta["monitoring_stale"])
                self.assertTrue(meta["vendor_strings_synthetic"])
                self.assertEqual(meta["fixture"], module.STEM)
                self.assertIn(spec_pdf, meta["synthetic_note"])
                self.assertIn("NOT a hardware oracle", meta["synthetic_note"])

    def test_identity_decodes_from_authored_page_00h(self):
        expected = {
            "3.0": {
                "date_code": "180918",
                "vendor_name": "CMIS_3_0_SYNTH",
                "vendor_oui": "00:00:00",
                "vendor_pn": "FIXTURE-30-SYNTH",
                "vendor_rev": "00",
                "vendor_sn": "SYNTHETIC0000003",
            },
            "5.2": {
                "date_code": "220427",
                "vendor_name": "CMIS_5_2_SYNTH",
                "vendor_oui": "00:00:00",
                "vendor_pn": "FIXTURE-SYNTH-V1",
                "vendor_rev": "00",
                "vendor_sn": "SYNTHETIC0000001",
            },
        }
        for rev, want in expected.items():
            with self.subTest(revision=rev):
                module = _load(BUILDERS[rev])
                pages = self._pages(module)
                golden = module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
                self.assertEqual(golden["identity"], want)
                self.assertEqual(golden["decode_status"], "ok")
                self.assertIsNone(golden["checksums"])

    def test_page11h_lane_monitor_offsets_and_scales(self):
        # CMIS 3.0 Table 70 / CMIS 5.2 Table 8-82 place the lane monitors
        # at flat bytes 154-169 (Tx power), 170-185 (Tx bias) and 186-201
        # (Rx power) — page-relative [26+2i:28+2i] / [42+2i:44+2i] /
        # [58+2i:60+2i] in the 128-byte Page 11h buffer the builders
        # author. Powers are big-endian U16 in 0.1 uW (1e-4 mW)
        # increments; bias is big-endian U16 in 2 uA (0.002 mA) increments.
        # Lanes 1 and 8 carry distinct non-zero values so a wrong base
        # offset, a wrong lane stride, a field swap or an endianness flip
        # all fail; the multiplier stays 00b (x1) here.
        authored = {
            1: {"tx_power": 0x1234, "tx_bias": 0x0BC4, "rx_power": 0x2345},
            8: {"tx_power": 0x00FF, "tx_bias": 0xC350, "rx_power": 0xFF00},
        }
        expected = {
            1: {"tx_power_mw": 0.466, "tx_bias_ma": 6.024, "rx_power_mw": 0.9029},
            8: {"tx_power_mw": 0.0255, "tx_bias_ma": 100.0, "rx_power_mw": 6.528},
        }
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                p11 = bytearray(128)
                for lane, vals in authored.items():
                    i = lane - 1
                    p11[26 + 2 * i : 28 + 2 * i] = vals["tx_power"].to_bytes(2, "big")
                    p11[42 + 2 * i : 44 + 2 * i] = vals["tx_bias"].to_bytes(2, "big")
                    p11[58 + 2 * i : 60 + 2 * i] = vals["rx_power"].to_bytes(2, "big")
                golden = self._golden_with_overrides(module, p11=bytes(p11))
                for lane in (1, 8):
                    for key, want in expected[lane].items():
                        self.assertEqual(
                            golden["lanes"][lane - 1][key],
                            want,
                            f"{rev} lane {lane} {key}",
                        )
                for lane in range(2, 8):
                    untouched = golden["lanes"][lane - 1]
                    self.assertEqual(
                        (
                            untouched["tx_power_mw"],
                            untouched["tx_bias_ma"],
                            untouched["rx_power_mw"],
                        ),
                        (0.0, 0.0, 0.0),
                        f"{rev} lane {lane} must stay zero (nothing authored there)",
                    )
                # Monitors live below the DPState bytes: nibbles stay Reserved0.
                for lane in golden["lanes"]:
                    self.assertEqual(lane["dp_state"], "Reserved0")

    def test_page01h_bias_multiplier_codes(self):
        # CMIS 3.0 Table 45 / CMIS 5.2 Table 8-47: Page 01h byte 160 bits
        # 4-3 (page-relative byte 32) carry the Tx bias multiplier — 00b x1,
        # 01b x2, 10b x4; 11b is reserved, making tx_bias_ma undefined
        # (None) rather than a guessed scale. Lanes 1 and 8 carry distinct
        # non-zero bias counts (100 and 200 -> 0.2 / 0.4 mA at x1).
        cases = {0b00: (0.2, 0.4), 0b01: (0.4, 0.8), 0b10: (0.8, 1.6)}
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                p11 = bytearray(128)
                p11[42:44] = (100).to_bytes(2, "big")  # lane 1 Tx bias
                p11[56:58] = (200).to_bytes(2, "big")  # lane 8 Tx bias
                for code, (lane1, lane8) in cases.items():
                    with self.subTest(multiplier_code=f"{code:02b}b"):
                        p01 = bytearray(128)
                        p01[32] = code << 3  # byte 160 bits 4-3
                        golden = self._golden_with_overrides(
                            module, p01=bytes(p01), p11=bytes(p11)
                        )
                        self.assertEqual(golden["lanes"][0]["tx_bias_ma"], lane1)
                        self.assertEqual(golden["lanes"][7]["tx_bias_ma"], lane8)
                p01 = bytearray(128)
                p01[32] = 0b11 << 3  # reserved multiplier: undefined
                golden = self._golden_with_overrides(module, p01=bytes(p01), p11=bytes(p11))
                self.assertIsNone(golden["lanes"][0]["tx_bias_ma"])
                self.assertIsNone(golden["lanes"][7]["tx_bias_ma"])

    def test_page11h_flag_guard_covers_the_actual_flag_region(self):
        # The unsupported-flags guard must slice the flag region the
        # revision's own tables define, in the 128-byte page-relative
        # buffer: CMIS 3.0 Tables 67-69 place lane flags at flat bytes
        # 134-152 (page-relative 6-24) and Table 70 reserves flat byte 153
        # (page-relative 25) — NOT a flag byte in 3.0. CMIS 5.2 Table 8-75
        # extends the Lane-specific Flags region through flat byte 153
        # (page-relative 6-25). Flat byte 154 (page-relative 26) is the
        # first Tx power monitor, never a flag.
        raises = {
            "3.0": (6, 24),
            "5.2": (6, 25),
        }
        passes = {
            ("3.0", 25): 0.0,     # Table 70: byte 153 Reserved — not read
            ("3.0", 26): 3.2768,  # Table 70: byte 154 Tx1 Power MSB
            ("5.2", 26): 3.2768,  # Table 8-82: byte 154 OpticalPowerTx1 MSB
        }
        for rev, rel_offsets in raises.items():
            module = _load(BUILDERS[rev])
            for rel in rel_offsets:
                with self.subTest(revision=rev, page_rel=rel, refuse=True):
                    p11 = bytearray(128)
                    p11[rel] = 0x80
                    with self.assertRaises(NotImplementedError):
                        self._golden_with_overrides(module, p11=bytes(p11))
        for (rev, rel), tx_power in passes.items():
            with self.subTest(revision=rev, page_rel=rel, refuse=False):
                module = _load(BUILDERS[rev])
                p11 = bytearray(128)
                p11[rel] = 0x80
                golden = self._golden_with_overrides(module, p11=bytes(p11))
                self.assertEqual(golden["lanes"][0]["dp_state"], "Reserved0")
                self.assertEqual(
                    golden["lanes"][0]["tx_power_mw"],
                    tx_power,
                    f"{rev} page-rel {rel}: byte must decode from its own field, "
                    "not be claimed by the flag guard",
                )

    def test_interrupt_bit_toggles_with_other_bits_set(self):
        # Lower byte 3 bit 0 — CMIS 3.0 Table 18 names it Interrupt, CMIS
        # 5.2 Table 8-6 names it InterruptDeasserted; both revisions agree
        # on the encoding: 0b = interrupt asserted, 1b = not asserted. The
        # bit must be read in isolation from byte 3's ModuleState bits
        # 3-1, so the toggle is exercised with those bits set too — not
        # only on the all-zero byte the committed fixture authors.
        cases = {
            0x00: True,   # all clear -> asserted (committed fixture's case)
            0x01: False,  # bit 0 set alone -> not asserted
            0x0E: True,   # ModuleState bits set, bit 0 clear -> asserted
            0x0F: False,  # ModuleState bits AND bit 0 set -> not asserted
            0x06: True,   # ModuleReady (011b) with bit 0 clear -> asserted
        }
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                for byte3, asserted in cases.items():
                    with self.subTest(lower_byte3=f"0x{byte3:02X}"):
                        lower = bytearray(module.lower_memory())
                        lower[3] = byte3
                        golden = self._golden_with_overrides(module, lower=bytes(lower))
                        self.assertEqual(
                            golden["flags"]["interrupt_asserted"],
                            asserted,
                            f"{rev} Lower:3=0x{byte3:02X}",
                        )

    def test_3_0_bias_multiplier_citation_is_table_45(self):
        # CMIS 3.0's Tx-bias multiplier (Page 01h byte 160 bits 4-3) is
        # defined by Table 45 "Implemented Monitors Advertisement" (p.69);
        # Table 42 is Module Characteristics Advertising and does not
        # define it. The 5.2 note cites its own table (8-47).
        module_3_0 = _load(BUILDERS["3.0"])
        note_3_0 = self._golden_with_overrides(module_3_0)["_metadata"][
            "synthetic_note"
        ]
        self.assertIn("Tables 70/45", note_3_0)
        self.assertNotIn("Table 42", note_3_0)
        module_5_2 = _load(BUILDERS["5.2"])
        note_5_2 = self._golden_with_overrides(module_5_2)["_metadata"][
            "synthetic_note"
        ]
        self.assertIn("Tables 8-82/8-47", note_5_2)

    def test_synthetic_note_discloses_nonconforming_application_advertisement(self):
        # CMIS 3.0 § 1.7.2.7 / CMIS 5.2 § 8.2.11: every module shall
        # advertise at least one Application, with FFh in the first unused
        # descriptor's host-interface code. The authored all-zero
        # advertisement does neither, and the golden must say so: the bytes
        # are preserved as intentional nonconformance, and
        # applications:null / decode_status:"ok" are the structural decode
        # of those bytes — "ok" is decode-success, not conformance.
        for rev, path in BUILDERS.items():
            with self.subTest(revision=rev):
                module = _load(path)
                golden = self._golden_with_overrides(module)
                note = golden["_metadata"]["synthetic_note"]
                self.assertIn("Nonconformance disclosure", note)
                self.assertIn("at least one Application", note)
                self.assertIn("decode-success, not conformance", note)
                self.assertIsNone(golden["applications"])
                self.assertEqual(golden["decode_status"], "ok")

    def test_provenance_docs_disclose_nonconforming_application_advertisement(self):
        for rev in ("3-0", "5-2"):
            with self.subTest(revision=rev):
                md_path = (
                    REPO
                    / "tests"
                    / "fixtures"
                    / "cmis"
                    / f"synthetic-cmis-{rev}-qsfp-dd.md"
                )
                text = md_path.read_text(encoding="utf-8")
                self.assertIn("Nonconformance disclosure", text)
                self.assertIn("decode-success, not conformance", text)
        # The 3.0 provenance must cite Table 45 for the bias multiplier
        # (Table 42 is Module Characteristics Advertising).
        text_3_0 = (
            REPO / "tests" / "fixtures" / "cmis" / "synthetic-cmis-3-0-qsfp-dd.md"
        ).read_text(encoding="utf-8")
        self.assertIn("Table 45", text_3_0)
        self.assertNotIn("Table 42", text_3_0)


    def test_reserved_variant_packing_and_table66_labels(self):
        # Independent reserved-DPSM variant of the CMIS 3.0 fixture. The
        # Page 11h is 128 bytes: first four `50 76 43 30` (Table 65 packing
        # of the literal lane vector below), remaining 124 zero. Both
        # directions are pinned: extraction (bytes -> vector) and packing
        # (vector -> bytes) must round-trip against literals, never against
        # the builder's own packing code alone.
        module = _load(BUILDERS["3.0"])
        p11 = module.page_11h_reserved()
        self.assertEqual(len(p11), 128)
        self.assertEqual(p11[:4], RESERVED_P11_PREFIX)
        self.assertEqual(p11[4:], bytes(124))
        # Flag region (Tables 67-69, page-relative 6-24) and lane monitors
        # (Table 70, page-relative 26-73) are zero.
        self.assertEqual(p11[6:25], bytes(19), "flag region must be zero")
        self.assertEqual(p11[26:74], bytes(48), "monitor region must be zero")

        # Extraction: bytes -> literal lane vector (odd lanes bits 3-0,
        # even lanes bits 7-4, Table 65).
        extracted = tuple(
            (p11[i // 2] >> ((i % 2) * 4)) & 0xF for i in range(8)
        )
        self.assertEqual(extracted, RESERVED_LANE_VECTOR)
        # Packing: literal lane vector -> bytes, independent re-derivation.
        packed = bytes(
            RESERVED_LANE_VECTOR[2 * i] | (RESERVED_LANE_VECTOR[2 * i + 1] << 4)
            for i in range(4)
        )
        self.assertEqual(packed, RESERVED_P11_PREFIX)
        self.assertEqual(packed, p11[:4])

        # Table 66 literal labels for the lane vector — transcribed from the
        # CMIS 3.0 spec (0h Reserved, 3h Deinit, 4h Activated, 5h-7h Reserved
        # -> Unknown<n>), NOT computed from the builder's own DP_STATE_NAMES.
        pages = self._pages(module)
        pages["page-11h"] = p11
        golden = module.expected_golden(
            pages["lower"],
            pages["page-00h"],
            pages["page-01h"],
            p11,
            reserved_variant=True,
        )
        self.assertEqual(len(golden["lanes"]), 8)
        for i, lane in enumerate(golden["lanes"]):
            self.assertEqual(lane["dp_state"], RESERVED_DP_LABELS[i])
            self.assertEqual(lane["index"], i + 1)
        # Monitors/flags stay zero-valued on the reserved variant.
        for lane in golden["lanes"]:
            self.assertEqual(lane["rx_power_mw"], 0.0)
            self.assertEqual(lane["tx_power_mw"], 0.0)
            self.assertEqual(lane["tx_bias_ma"], 0.0)
        for lf in golden["flags"]["lanes"]:
            self.assertTrue(all(value is False for value in lf.values()))

        # The five other pages are byte-identical to the baseline fixture.
        for tag in ("lower", "page-00h", "page-01h", "page-02h", "page-10h"):
            self.assertEqual(
                pages[tag],
                self._committed_fixture(module.STEM, tag),
                f"reserved variant must reuse the baseline {tag} bytes",
            )

    def test_reserved_variant_reproduces_committed_artifacts(self):
        # The committed reserved fixture/golden regenerate byte-for-byte from
        # the builder, with their OWN stem and pinned authorship date (never
        # the regeneration clock) and a synthetic note that names the CMIS
        # 3.0 Table 66 reserved 5h-7h nibbles and the 0h/3h/4h controls.
        module = _load(BUILDERS["3.0"])
        stem = module.RESERVED_STEM
        self.assertEqual(stem, "synthetic-cmis-3-0-qsfp-dd-dp-reserved")
        self.assertEqual(module.RESERVED_AUTHORED_AT, "2026-10-22")
        self.assertNotEqual(module.RESERVED_AUTHORED_AT, module.AUTHORED_AT)

        p11 = module.page_11h_reserved()
        self.assertEqual(
            p11, self._committed_fixture(stem, "page-11h"),
            f"{stem}.page-11h.bin drifted from the builder",
        )
        for tag in ("lower", "page-00h", "page-01h", "page-02h", "page-10h"):
            self.assertEqual(
                self._committed_fixture(stem, tag),
                self._committed_fixture(module.STEM, tag),
                f"{stem}.{tag}.bin must equal the baseline page bytes",
            )

        pages = self._pages(module)
        golden = module.expected_golden(
            pages["lower"],
            pages["page-00h"],
            pages["page-01h"],
            p11,
            reserved_variant=True,
        )
        rendered = json.dumps(golden, indent=2, sort_keys=True) + "\n"
        self.assertEqual(
            rendered,
            self._committed_golden_text(stem),
            f"{stem}.json drifted from the builder",
        )

        # Metadata keeps the existing schema/shape and the 3.0 device
        # revision, under the reserved stem and its own pinned date.
        meta = golden["_metadata"]
        self.assertEqual(meta["fixture"], stem)
        self.assertEqual(meta["authored_at"], module.RESERVED_AUTHORED_AT)
        self.assertEqual(meta["cmis_version"], "3.0")
        self.assertEqual(golden["module"]["cmis_revision"], "3.0")
        self.assertEqual(meta["schema_version"], "openxcvr.golden.v1")
        self.assertEqual(meta["spec"], "cmis-5.3")
        self.assertEqual(meta["spec_revision"], "5.3")
        self.assertTrue(meta["synthetic"])
        self.assertTrue(meta["monitoring_stale"])
        self.assertTrue(meta["vendor_strings_synthetic"])
        self.assertIn("CMIS3p0_Third_Party_Spec.pdf", meta["synthetic_note"])
        self.assertIn("NOT a hardware oracle", meta["synthetic_note"])
        # The note explicitly names the Table 66 reserved 5h-7h nibbles and
        # the 0h/3h/4h controls this variant exercises.
        self.assertIn("Table 66", meta["synthetic_note"])
        self.assertIn("5h", meta["synthetic_note"])
        self.assertIn("7h", meta["synthetic_note"])
        self.assertIn("Reserved", meta["synthetic_note"])
        self.assertIn("3h", meta["synthetic_note"])
        self.assertIn("4h", meta["synthetic_note"])
        self.assertIn("Deinit", meta["synthetic_note"])
        self.assertIn("Activated", meta["synthetic_note"])

        # The four-argument default still reproduces the ORIGINAL baseline
        # JSON byte-for-byte (default-preserving keyword contract).
        baseline_golden = module.expected_golden(
            pages["lower"],
            pages["page-00h"],
            pages["page-01h"],
            pages["page-11h"],
        )
        self.assertEqual(
            json.dumps(baseline_golden, indent=2, sort_keys=True) + "\n",
            self._committed_golden_text(module.STEM),
            "default expected_golden() must reproduce the baseline JSON",
        )

    def test_golden_authoring_does_not_load_production_names(self):
        # Executable independence guard: while the builders are loaded and
        # their goldens authored/regenerated into redirected owned
        # directories, any production-binding import (the `openxcvr`
        # Python package), any generated-module file load from bindings/,
        # and any decoder subprocess use is DENIED. The builders are
        # independent spec-transcription oracles: if one tried to source a
        # name from production code, this test fails with the denial, not
        # with a silently borrowed name.
        class GuardDenied(Exception):
            pass

        def deny(path_or_name):
            raise GuardDenied(
                f"builder independence violation: {path_or_name!r} — golden "
                "authoring must transcribe spec tables, never load "
                "production/generated names or run a decoder"
            )

        class DenyProductionImports:
            def find_spec(self, fullname, path=None, target=None):
                if fullname == "openxcvr" or fullname.startswith("openxcvr."):
                    deny(f"import {fullname}")
                return None

        real_spec_from_file_location = importlib.util.spec_from_file_location
        real_subprocess = {
            name: getattr(subprocess, name)
            for name in ("run", "call", "check_call", "check_output", "Popen")
        }

        def guarded_spec_from_file_location(name, location, *args, **kwargs):
            location_path = pathlib.Path(str(location)).resolve()
            if location_path.is_relative_to((REPO / "bindings").resolve()):
                deny(f"load {location}")
            return real_spec_from_file_location(name, location, *args, **kwargs)

        def guarded_subprocess(name):
            def _denied(*args, **kwargs):
                deny(f"subprocess.{name}")

            return _denied

        guard = DenyProductionImports()
        sys.meta_path.insert(0, guard)
        for name in real_subprocess:
            setattr(subprocess, name, guarded_subprocess(name))
        importlib.util.spec_from_file_location = guarded_spec_from_file_location
        try:
            # Positive control: the guard itself fires — a production binding
            # load attempt raises GuardDenied (not a normal ImportError).
            with self.assertRaises(GuardDenied):
                guarded_spec_from_file_location(
                    "captured_control",
                    REPO / "bindings" / "python" / "openxcvr" / "_cmis.py",
                )

            # Load both builders and author every golden variant under the
            # guard, then regenerate both 3.0 stems + 5.2 into redirected
            # owned directories; the outputs must still reproduce.
            module_3_0 = _load(BUILDERS["3.0"])
            module_5_2 = _load(BUILDERS["5.2"])
            for module in (module_3_0, module_5_2):
                pages = self._pages(module)
                module.expected_golden(
                    pages["lower"],
                    pages["page-00h"],
                    pages["page-01h"],
                    pages["page-11h"],
                )
            module_3_0.expected_golden(
                self._pages(module_3_0)["lower"],
                self._pages(module_3_0)["page-00h"],
                self._pages(module_3_0)["page-01h"],
                module_3_0.page_11h_reserved(),
                reserved_variant=True,
            )

            with tempfile.TemporaryDirectory() as tmp:
                root = pathlib.Path(tmp)
                fixture_dir = root / "tests" / "fixtures" / "cmis"
                golden_dir = root / "tests" / "golden"
                for module in (module_3_0, module_5_2):
                    module.FIXTURE_DIR = fixture_dir
                    module.GOLDEN_DIR_ROOT = golden_dir
                    module.GOLDEN_PATH = golden_dir / "cmis" / f"{module.STEM}.json"
                    if hasattr(module, "RESERVED_GOLDEN_PATH"):
                        module.RESERVED_GOLDEN_PATH = (
                            golden_dir / "cmis" / f"{module.RESERVED_STEM}.json"
                        )
                fixture_dir.mkdir(parents=True)
                for module in (module_3_0, module_5_2):
                    with contextlib.redirect_stdout(io.StringIO()):
                        self.assertEqual(module.main(), 0)
                    for stem in self._builder_stems(module):
                        for tag in PAGE_TAGS:
                            self.assertEqual(
                                (fixture_dir / f"{stem}.{tag}.bin").read_bytes(),
                                self._committed_fixture(stem, tag),
                            )
                        self.assertEqual(
                            self._golden_path_for(module, stem).read_text(
                                encoding="utf-8"
                            ),
                            self._committed_golden_text(stem),
                        )
        finally:
            sys.meta_path.remove(guard)
            importlib.util.spec_from_file_location = real_spec_from_file_location
            for name, fn in real_subprocess.items():
                setattr(subprocess, name, fn)


if __name__ == "__main__":
    unittest.main()


class Cmis54VdmBuilderTests(unittest.TestCase):
    """Reproduction pin for tools/build-cmis-5-4-synthetic.py.

    Separate from SyntheticBuilderTests: the 5.4 VDM builder emits a
    21-slice page set (VDM 20h-2Fh included) and its golden derives from
    the VDM pages, so the shared harness's six-tag page set and
    expected_golden(lower, p00, p01, p11) signature do not apply. The
    drift guarantees are the same: every committed .bin slice and the
    committed golden JSON must regenerate byte-for-byte and
    character-for-character from the builder.
    """

    PAGES_54 = (
        "lower",
        "page-00h", "page-01h", "page-02h", "page-10h", "page-11h",
        "page-20h", "page-21h", "page-22h", "page-23h",
        "page-24h", "page-25h", "page-26h", "page-27h",
        "page-28h", "page-29h", "page-2ah", "page-2bh",
        "page-2ch", "page-2dh", "page-2fh",
    )

    @classmethod
    def setUpClass(cls):
        cls.module = _load(TOOLS / "build-cmis-5-4-synthetic.py")

    def test_pages_reproduce_committed_fixtures(self):
        m = self.module
        emitted = {
            "page-00h": m.page_00h(),
            "page-01h": m.page_01h(),
            "page-02h": m.page_02h(),
            "page-10h": m.page_10h(),
            "page-11h": m.page_11h(),
            "page-20h": m.page_descriptors(0x20),
            "page-21h": m.page_descriptors(0x21),
            "page-22h": m.page_descriptors(0x22),
            "page-23h": m.page_descriptors(0x23),
            "page-24h": m.page_samples(0x20),
            "page-25h": m.page_samples(0x21),
            "page-26h": m.page_samples(0x22),
            "page-27h": m.page_samples(0x23),
            "page-28h": m.page_thresholds(0x28),
            "page-29h": m.page_thresholds(0x29),
            "page-2ah": m.page_thresholds(0x2A),
            "page-2bh": m.page_thresholds(0x2B),
            "page-2ch": m.page_2ch(),
            "page-2dh": m.page_2dh(),
            "page-2fh": m.page_2fh(),
        }
        emitted["lower"] = m.lower_memory()
        self.assertEqual(sorted(emitted), sorted(self.PAGES_54))
        for tag in self.PAGES_54:
            self.assertEqual(len(emitted[tag]), 128, f"{m.STEM}.{tag} size")
            committed = (REPO / "tests" / "fixtures" / "cmis" / f"{m.STEM}.{tag}.bin").read_bytes()
            self.assertEqual(emitted[tag], committed, f"{m.STEM}.{tag}.bin drifted")

    def test_golden_reproduces_committed_json(self):
        m = self.module
        rendered = json.dumps(m.build_golden(), indent=1) + "\n"
        committed = (REPO / "tests" / "golden" / "cmis" / f"{m.STEM}.json").read_text(encoding="utf-8")
        self.assertEqual(rendered, committed, f"{m.STEM}.json drifted from the builder")

    def test_vdm_semantics_match_authored_bytes(self):
        """Expected VDM decode values derive from the spec tables applied to
        the authored bytes, asserted independently of the builder's own
        arithmetic (mirrors the shared harness's override philosophy)."""
        m = self.module
        golden = m.build_golden()
        support = golden["vdm"]["support"]
        # 2Fh:128 = 0b1111: duty cycle + power saving + all four groups.
        self.assertEqual(support["vdm_support"], 3)
        self.assertEqual(support["groups_advertised"], 4)
        self.assertTrue(support["duty_cycle_support"])
        self.assertTrue(support["power_saving_support"])
        # 2Fh:144 bits 5-2 = k = 5.
        self.assertEqual(support["monitoring_duty_cycle_k"], 5)

        obs = {o["instance"]: o for o in golden["vdm"]["observables"]}
        # Group-4 instance 255: global threshold set 49 + local 0 (Table 8-179).
        self.assertEqual(obs[255]["global_threshold_set_id"], 49)
        self.assertEqual(obs[255]["observable_type"], 0x1B)
        # F16 0x98FA = 250 * 10^(19-24) = 2.5e-3 (Table 8-180 type 9).
        self.assertAlmostEqual(obs[3]["sample"]["value"], 2.5e-3, places=12)
        # F16 0xC001 = 1 * 10^0 = 1.0.
        self.assertEqual(obs[65]["sample"]["value"], 1.0)
        # S16 0xFFCE = -50 (ELS input power, type 0x54).
        self.assertEqual(obs[129]["sample"]["value"], -50)

        # TC flag nibble packing (Tables 8-183/8-184): instance 1 low nibble
        # of 2Ch:128 = 0b1001 (high alarm + low warning), instance 2 high
        # nibble = 0b0001 (high alarm) -> byte 0x19.
        flags = {f["instance"]: f for f in golden["vdm"]["tc_flags"]}
        self.assertTrue(flags[1]["high_alarm"] and flags[1]["low_warning"])
        self.assertTrue(flags[2]["high_alarm"])
        self.assertTrue(flags[129]["high_warning"])
        self.assertTrue(flags[255]["low_alarm"])

    def test_identity_vendor_name_derives_from_page00h_bytes(self):
        """VendorName is a 16-byte CMIS 5.4 Page 00h field (module bytes
        129-144): the golden must carry the authored field bytes, field-width
        truncated and trailing-space trimmed the way every decoder trims them
        — never an untruncated authored literal longer than the field."""
        m = self.module
        golden = m.build_golden()
        field = m.page_00h()[1:17]
        self.assertEqual(
            golden["identity"]["vendor_name"],
            field.decode("ascii").rstrip(" "),
        )
        self.assertEqual(field, b"CMIS_5_4_VDM_SYN")
        self.assertEqual(golden["identity"]["vendor_name"], "CMIS_5_4_VDM_SYN")
        self.assertLessEqual(len(golden["identity"]["vendor_name"]), 16)
