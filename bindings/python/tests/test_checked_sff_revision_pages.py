"""Runtime tests for the generated checked SFF revision decoders.

Mirrors the CMIS checked-page runtime tests for the SFF families: admission
sets, buffer floors, presence-window landmarks and the schema-derived
embedded revision cross-check (SFF-8472 A0h byte 94). Expected values are an
independent literal oracle transcribed from the specs, not from generator
output: SFF-8636 lower page 00h byte 1 is the revision-compliance byte from
Rev 1.4 (Table 6-4); bytes 89-97 held the SFF-8079 application-select
controls through Rev 2.9 and are reserved from Rev 2.10 on; SFF-8472 A0h
byte 94 is the compliance byte (Table 8-8).
"""

from __future__ import annotations

import pytest

from openxcvr._generated.page_a0h_base_memory_revision import (
    page_a0h_base_memoryChecked as PageA0hBaseMemoryChecked,
)
from openxcvr._generated.page_a2h_diagnostics_base_revision import (
    page_a2h_diagnostics_baseChecked as PageA2hDiagnosticsBaseChecked,
)
from openxcvr._generated.page_a2h_03h_high_accuracy_timing_revision import (
    page_a2h_03h_high_accuracy_timingChecked as PageA2h03hHighAccuracyTimingChecked,
)
from openxcvr._generated.sff8636_lower_memory_revision import (
    Sff8636LowerMemoryChecked,
)
from openxcvr._generated.sff8636_page22h_high_accuracy_timing_revision import (
    Sff8636Page22hHighAccuracyTimingChecked,
)

SFF8636_ADMITTED = (0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A)
SFF8472_ADMITTED = (0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A)


def test_sff8636_lower_memory_admission_and_floor():
    with pytest.raises(ValueError, match="unsupported SFF-8636 revision byte"):
        Sff8636LowerMemoryChecked.decode_for_revision(0x02, bytes(128))
    with pytest.raises(ValueError, match="expected >=128 bytes"):
        Sff8636LowerMemoryChecked.decode_for_revision(0x03, bytes(127))
    checked = Sff8636LowerMemoryChecked.decode_for_revision(0x03, bytes(128))
    assert checked.rev_compliance_byte == 0x03


def test_sff8636_legacy_application_select_windows():
    # Rev 1.3 (0x03): the SFF-8079 application-select lanes are present and
    # read bytes 89-92 (Rx) and 94-97 (Tx), lane 1 at the highest address;
    # revision_compliance itself is absent (added Rev 1.4).
    raw = bytearray(128)
    raw[89], raw[90], raw[91], raw[92] = 0x44, 0x33, 0x22, 0x11
    raw[94], raw[95], raw[96], raw[97] = 0x88, 0x77, 0x66, 0x55
    checked = Sff8636LowerMemoryChecked.decode_for_revision(0x03, bytes(raw))
    assert checked.values.sff8079_rx_application_select == [0x11, 0x22, 0x33, 0x44]
    assert checked.values.sff8079_tx_application_select == [0x55, 0x66, 0x77, 0x88]
    assert checked.present.sff8079_rx_application_select
    assert not checked.present.revision_compliance

    # Rev 2.11 (0x09): the controls were removed by Rev 2.10 — presence
    # false, values defaulted, and the revision-compliance byte present.
    checked = Sff8636LowerMemoryChecked.decode_for_revision(0x09, bytes(raw))
    assert not checked.present.sff8079_rx_application_select
    assert checked.values.sff8079_rx_application_select == [0, 0, 0, 0]
    assert checked.present.revision_compliance
    assert checked.present.software_reset


def test_sff8472_a0h_embedded_compliance_cross_check():
    raw = bytearray(256)
    raw[94] = 0x0A
    checked = PageA0hBaseMemoryChecked.decode_for_revision(0x0A, bytes(raw))
    assert checked.rev_compliance_byte == 0x0A
    # A disagreement between buf[94] and the requested revision is refused.
    with pytest.raises(ValueError, match="does not match"):
        PageA0hBaseMemoryChecked.decode_for_revision(0x09, bytes(raw))


def test_sff8472_windows_and_floor():
    raw = bytearray(256)
    raw[94] = 0x05
    with pytest.raises(ValueError, match="expected >=256 bytes"):
        PageA0hBaseMemoryChecked.decode_for_revision(0x05, bytes(255))
    # Rev 11.0 (0x05): OM4 length (added 11.3) and 64GFC (added 12.4)
    # absent; at 12.5 (0x0A) both present plus remote performance monitoring.
    at_110 = PageA0hBaseMemoryChecked.decode_for_revision(0x05, bytes(raw))
    assert not at_110.present.length_om4_or_copper_1m
    assert not at_110.present.fc_speed2_64gfc
    raw[94] = 0x0A
    at_125 = PageA0hBaseMemoryChecked.decode_for_revision(0x0A, bytes(raw))
    assert at_125.present.length_om4_or_copper_1m
    assert at_125.present.fc_speed2_64gfc
    assert at_125.present.remote_performance_monitoring_supported


def test_sff8472_a2h_diagnostic_windows():
    raw = bytes(256)
    # Optional monitors (laser temperature, TEC current) arrived at Rev 11.4.
    at_110 = PageA2hDiagnosticsBaseChecked.decode_for_revision(0x05, raw)
    assert not at_110.present.optional_laser_temperature
    at_114 = PageA2hDiagnosticsBaseChecked.decode_for_revision(0x07, raw)
    assert at_114.present.optional_laser_temperature
    # Equalization controls (bytes 114-115) arrived at Rev 11.9b; 11.9b is
    # not a compliance-code revision, so the window opens at 12.3 (0x08).
    assert not at_114.present.tx_input_equalization_control
    at_123 = PageA2hDiagnosticsBaseChecked.decode_for_revision(0x08, raw)
    assert at_123.present.tx_input_equalization_control


def test_high_accuracy_timing_format_id_decodes_sixteen_bit():
    # SFF-8636 v2.12 § 7 and SFF-8472 v12.5a § 13: bytes 128-129 are the
    # 16-bit big-endian Format ID ("CA1Bh" = the calibration formats). The
    # checked decoders must read the full width — 0xCA1B is not equal to the
    # one-byte truncation 0x00, so this is an independent oracle against a
    # one-byte read as well as against the pre-fix footprint mismatch.
    raw8636 = bytearray(128)
    raw8636[0:2] = b"\xca\x1b"
    checked = Sff8636Page22hHighAccuracyTimingChecked.decode_for_revision(
        0x0A, bytes(raw8636)
    )
    assert checked.values.format_id == 0xCA1B
    assert checked.present.format_id

    raw8472 = bytearray(256)
    raw8472[128:130] = b"\xca\x1b"
    checked = PageA2h03hHighAccuracyTimingChecked.decode_for_revision(
        0x0A, bytes(raw8472)
    )
    assert checked.values.format_id == 0xCA1B
    assert checked.present.format_id


@pytest.mark.parametrize(
    ("cls", "admitted"),
    [
        (Sff8636LowerMemoryChecked, SFF8636_ADMITTED),
        (PageA0hBaseMemoryChecked, SFF8472_ADMITTED),
        (PageA2hDiagnosticsBaseChecked, SFF8472_ADMITTED),
    ],
)
def test_every_admitted_revision_byte_decodes(cls, admitted):
    size = 128 if cls is Sff8636LowerMemoryChecked else 256
    raw = bytearray(size)
    if cls is PageA0hBaseMemoryChecked:
        for byte in admitted:
            raw[94] = byte
            assert cls.decode_for_revision(byte, bytes(raw)).rev_compliance_byte == byte
    else:
        for byte in admitted:
            assert cls.decode_for_revision(byte, bytes(raw)).rev_compliance_byte == byte
