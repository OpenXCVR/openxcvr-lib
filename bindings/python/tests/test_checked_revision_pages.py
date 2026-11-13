"""Runtime tests for the seven generated checked CMIS revision decoders.

Expected values here are an independent literal oracle: admission set,
defaults, presence semantics and direct-read landmarks are transcribed from
the settled spec (python-spec.md), not derived from the generator output.
"""

from __future__ import annotations

import dataclasses
import enum

import pytest

from openxcvr._generated.cmis_lower_memory_revision import CmisLowerMemoryChecked
from openxcvr._generated.cmis_page00h_administrative_revision import (
    CmisPage00hAdministrativeChecked,
    CmisPage00hAdministrativeCheckedValues,
)
from openxcvr._generated.cmis_page01h_advertising_revision import (
    CmisPage01hAdvertisingChecked,
)
from openxcvr._generated.cmis_page11h_lane_status_revision import (
    CmisPage11hLaneStatusChecked,
)
from openxcvr._generated.cmis_page14h_diagnostics_results_revision import (
    CmisPage14hDiagnosticsResultsChecked,
)
from openxcvr._generated.cmis_page20h_vdm_descriptors1_to64_revision import (
    CmisPage20hVdmDescriptors1To64Checked,
)
from openxcvr._generated.cmis_page9_fh_cdb_message_revision import (
    CmisPage9FhCdbMessageChecked,
)
from openxcvr._generated.cmis_lower_memory import CmisLowerMemory
from openxcvr._generated.cmis_page01h_advertising import CmisPage01hAdvertising
from openxcvr._generated.cmis_page11h_lane_status import CmisPage11hLaneStatus
from openxcvr._generated.cmis_page14h_diagnostics_results import (
    CmisPage14hDiagnosticsResults,
)
from openxcvr._generated.cmis_page20h_vdm_descriptors1_to64 import (
    CmisPage20hVdmDescriptors1To64,
)
from openxcvr._generated.cmis_page9_fh_cdb_message import CmisPage9FhCdbMessage

ADMITTED = (0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54)

# (decoder, buffer length, is the lower-memory page with embedded buf[1])
PAGES = [
    (CmisLowerMemoryChecked, 256, True),
    (CmisPage00hAdministrativeChecked, 128, False),
    (CmisPage01hAdvertisingChecked, 128, False),
    (CmisPage11hLaneStatusChecked, 128, False),
    (CmisPage14hDiagnosticsResultsChecked, 128, False),
    (CmisPage20hVdmDescriptors1To64Checked, 128, False),
    (CmisPage9FhCdbMessageChecked, 128, False),
]


def page_ids():
    return [c.__name__ for c, _, _ in PAGES]


def decode(cls, size, lower, rev, buf=None):
    """Decode a zeroed buffer (with optional pre-set bytes) for `rev`."""
    raw = bytearray(size)
    for index, value in (buf or {}).items():
        raw[index] = value
    if lower:
        raw[1] = rev
    return cls.decode_for_revision(rev, bytes(raw))


# ─── type identities ─────────────────────────────────────────────────────────


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_checked_shape_and_frozen_wrapper(cls, size, lower):
    checked = decode(cls, size, lower, 0x53)
    assert checked.cmis_rev_byte == 0x53
    with pytest.raises(dataclasses.FrozenInstanceError):
        checked.cmis_rev_byte = 0x30
    with pytest.raises(dataclasses.FrozenInstanceError):
        checked.present.__setattr__("value", True)  # type: ignore[attr-defined]
    assert type(checked.values).__name__


@pytest.mark.parametrize(
    "cls,raw_type",
    [
        (CmisLowerMemoryChecked, CmisLowerMemory),
        (CmisPage01hAdvertisingChecked, CmisPage01hAdvertising),
        (CmisPage11hLaneStatusChecked, CmisPage11hLaneStatus),
        (CmisPage14hDiagnosticsResultsChecked, CmisPage14hDiagnosticsResults),
        (
            CmisPage20hVdmDescriptors1To64Checked,
            CmisPage20hVdmDescriptors1To64,
        ),
        (CmisPage9FhCdbMessageChecked, CmisPage9FhCdbMessage),
    ],
    ids=lambda v: getattr(v, "__name__", str(v)),
)
def test_six_pages_reuse_raw_values_type(cls, raw_type):
    checked = decode(cls, 256 if raw_type is CmisLowerMemory else 128,
                     raw_type is CmisLowerMemory, 0x40)
    assert type(checked.values) is raw_type


def test_page00h_uses_checked_values_union():
    checked = decode(CmisPage00hAdministrativeChecked, 128, False, 0x40)
    assert type(checked.values) is CmisPage00hAdministrativeCheckedValues
    # The promoted CMIS4-only members exist on the stable union.
    assert hasattr(checked.values, "reserved_administrative_208_cmis4")
    assert hasattr(checked.values, "reserved_administrative_213_bit7_cmis4")
    assert hasattr(checked.values, "reserved_administrative_213_bits6_0_cmis4")
    # No invented encode/setter contract on the values union.
    assert not hasattr(checked.values, "encode")
    assert not hasattr(checked.values, "set_reserved_administrative_208_cmis4")


# ─── admission: all 256 revision bytes ────────────────────────────────────────


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_all_256_revision_bytes(cls, size, lower):
    for rev in range(256):
        if rev in ADMITTED:
            checked = decode(cls, size, lower, rev)
            assert checked.cmis_rev_byte == rev
        else:
            with pytest.raises(ValueError):
                decode(cls, size, lower, rev)


# ─── strict inputs and error order ───────────────────────────────────────────


class BytesSubclass(bytes):
    pass


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_strict_bytes_input(cls, size, lower):
    buf = bytearray(size)
    if lower:
        buf[1] = 0x40
    with pytest.raises(TypeError):
        cls.decode_for_revision(0x40, bytearray(buf))
    with pytest.raises(TypeError):
        cls.decode_for_revision(0x40, memoryview(bytes(buf)))
    # bytes subclasses are accepted per the isinstance(bytes) contract.
    checked = cls.decode_for_revision(0x40, BytesSubclass(bytes(buf)))
    assert checked.cmis_rev_byte == 0x40
    # Extra trailing bytes are accepted.
    cls.decode_for_revision(0x40, bytes(buf) + b"\x00" * 16)


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_strict_revision_type(cls, size, lower):
    buf = bytes(size)
    for bad in (True, False, "0x40", 0x40 + 0.5, None):
        with pytest.raises(TypeError):
            cls.decode_for_revision(bad, buf)


# ─── exact built-in int revision type ────────────────────────────────────────


class RevisionSubclass(int):
    """A minimal non-bool int subclass: value-compatible, type-incompatible."""


class RevisionEnum(enum.IntEnum):
    """IntEnum is an int subclass too and must be rejected like any other."""

    REV_0X40 = 0x40


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_plain_int_revision_accepted(cls, size, lower):
    # Positive control: every admitted revision decodes when supplied as the
    # plain built-in int.
    for rev in ADMITTED:
        checked = decode(cls, size, lower, rev)
        assert checked.cmis_rev_byte == rev


@pytest.mark.parametrize(
    "make_revision",
    [
        lambda rev: RevisionSubclass(rev),
        lambda rev: RevisionEnum(rev),
    ],
    ids=["int-subclass", "int-enum"],
)
@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_int_subclass_revision_rejected(cls, size, lower, make_revision):
    # The settled contract admits exactly the built-in int type: any int
    # subclass raises TypeError, matching the bool rejection already pinned
    # in test_strict_revision_type.
    with pytest.raises(TypeError):
        decode(cls, size, lower, make_revision(0x40))


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_subclass_rejection_keeps_buffer_first_precedence(cls, size, lower):
    # Error order (1) buf type, (2) revision type: an int-subclass revision
    # with a non-bytes buffer still reports the buffer error first.
    with pytest.raises(TypeError, match="buf must be bytes"):
        cls.decode_for_revision(RevisionSubclass(0x40), bytearray(size))


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_subclass_rejection_keeps_bytes_subclass_acceptance(cls, size, lower):
    # bytes subclasses remain accepted alongside the exact-int revision rule.
    buf = bytearray(size)
    if lower:
        buf[1] = 0x40
    checked = cls.decode_for_revision(0x40, BytesSubclass(bytes(buf)))
    assert checked.cmis_rev_byte == 0x40


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_unsupported_revision_value(cls, size, lower):
    for bad in (0x00, 0x2F, 0x55, 0x99, 0xFF, -1, 256, 0x100):
        with pytest.raises(ValueError):
            cls.decode_for_revision(bad, bytes(size))


@pytest.mark.parametrize("cls,size,lower", PAGES, ids=page_ids())
def test_short_buffer_and_error_order(cls, size, lower):
    with pytest.raises(ValueError, match="expected >=128"):
        cls.decode_for_revision(0x40, bytes(127))
    # (1) buf type precedes everything.
    with pytest.raises(TypeError):
        cls.decode_for_revision(0x99, bytearray(size))
    with pytest.raises(TypeError):
        cls.decode_for_revision(True, bytearray(size))
    # (2) revision type precedes the length check.
    with pytest.raises(TypeError):
        cls.decode_for_revision(True, bytes(127))
    # (3) unsupported revision value precedes the length check.
    with pytest.raises(ValueError, match="unsupported CMIS revision byte"):
        cls.decode_for_revision(0x99, bytes(127))


def test_lower_embedded_revision_mismatch_and_precedence():
    buf = bytearray(256)
    buf[1] = 0x40
    with pytest.raises(ValueError, match="buf\\[1\\]"):
        CmisLowerMemoryChecked.decode_for_revision(0x53, bytes(buf))
    buf[1] = 0x53
    checked = CmisLowerMemoryChecked.decode_for_revision(0x53, bytes(buf))
    assert checked.cmis_rev_byte == 0x53
    # The embedded check follows the length check: a 2-byte buffer with a
    # mismatched embedded byte still reports the length error.
    with pytest.raises(ValueError, match="expected >=128"):
        CmisLowerMemoryChecked.decode_for_revision(0x53, bytes([0x0D, 0x40]))


# ─── Page 00h landmark: the three CMIS4-only members ─────────────────────────


def test_page00h_landmark_165_true_1():
    checked = decode(
        CmisPage00hAdministrativeChecked,
        128,
        False,
        0x40,
        {80: 0xA5, 85: 0x81},
    )
    assert checked.values.reserved_administrative_208_cmis4 == 165
    assert checked.values.reserved_administrative_213_bit7_cmis4 is True
    assert checked.values.reserved_administrative_213_bits6_0_cmis4 == 1
    assert checked.present.reserved_administrative_208_cmis4 is True
    assert checked.present.reserved_administrative_213_bit7_cmis4 is True
    assert checked.present.reserved_administrative_213_bits6_0_cmis4 is True


def test_page00h_present_zero_stays_present():
    checked = decode(
        CmisPage00hAdministrativeChecked,
        128,
        False,
        0x40,
        {80: 0x00, 85: 0x00},
    )
    assert checked.values.reserved_administrative_208_cmis4 == 0
    assert checked.values.reserved_administrative_213_bit7_cmis4 is False
    assert checked.values.reserved_administrative_213_bits6_0_cmis4 == 0
    assert checked.present.reserved_administrative_208_cmis4 is True
    assert checked.present.reserved_administrative_213_bit7_cmis4 is True
    assert checked.present.reserved_administrative_213_bits6_0_cmis4 is True


@pytest.mark.parametrize("rev", [0x30, 0x50, 0x51, 0x52, 0x53])
def test_page00h_other_revisions_absent_with_defaults(rev):
    checked = decode(
        CmisPage00hAdministrativeChecked,
        128,
        False,
        rev,
        {80: 0xA5, 85: 0x81},
    )
    assert checked.values.reserved_administrative_208_cmis4 == 0
    assert checked.values.reserved_administrative_213_bit7_cmis4 is False
    assert checked.values.reserved_administrative_213_bits6_0_cmis4 == 0
    assert checked.present.reserved_administrative_208_cmis4 is False
    assert checked.present.reserved_administrative_213_bit7_cmis4 is False
    assert checked.present.reserved_administrative_213_bits6_0_cmis4 is False


# ─── Lower memory: direct reads, aliases, widths, raw enums ──────────────────


@pytest.mark.parametrize("rev", [0x30, 0x40, 0x50, 0x51, 0x52])
def test_mci_max_speed_narrow_width_direct_read(rev):
    # bits 5:2 = 0b1110 for the u4 view, but the u2 view (bits 3:2) is 0b10.
    checked = decode(CmisLowerMemoryChecked, 256, True, rev, {2: 0b00111000})
    assert checked.values.mci_max_speed == 0b10
    assert checked.present.mci_max_speed is True


def test_mci_max_speed_wide_width_at_5_3():
    checked = decode(CmisLowerMemoryChecked, 256, True, 0x53, {2: 0b00111000})
    assert checked.values.mci_max_speed == 0b1110
    # The stable member name survived the 4.0 twi_max_speed_cmis4 alias.
    assert not hasattr(checked.values, "twi_max_speed_cmis4")


@pytest.mark.parametrize("rev", [0x30, 0x40])
def test_module_fault_cause_absent_before_5_0(rev):
    checked = decode(CmisLowerMemoryChecked, 256, True, rev, {41: 0x22})
    assert checked.values.module_fault_cause == 0
    assert checked.present.module_fault_cause is False


@pytest.mark.parametrize("rev", [0x50, 0x51, 0x52, 0x53])
def test_module_fault_cause_raw_enum_storage(rev):
    # Raw wire integer is kept verbatim at every revision: the 5.3 symbolic
    # reinterpretation never rewrites earlier-revision raw storage.
    checked = decode(CmisLowerMemoryChecked, 256, True, rev, {41: 0x22})
    assert checked.values.module_fault_cause == 0x22
    assert checked.present.module_fault_cause is True


def test_reserved_module_status_cmis4_window():
    # byte 3 bits 7:4, present only at 4.0.
    checked = decode(CmisLowerMemoryChecked, 256, True, 0x40, {3: 0xA0})
    assert checked.values.reserved_module_status_cmis4 == 0xA
    assert checked.present.reserved_module_status_cmis4 is True
    for rev in (0x30, 0x53):
        other = decode(CmisLowerMemoryChecked, 256, True, rev, {3: 0xA0})
        assert other.values.reserved_module_status_cmis4 == 0
        assert other.present.reserved_module_status_cmis4 is False


def test_dpsm_reserved_states_stay_raw():
    # CMIS 3.0 DPSM reserved states 0/5/6/7 decode as raw nibbles.
    checked = decode(
        CmisPage11hLaneStatusChecked,
        128,
        False,
        0x30,
        # lanes 1..8 low/high nibbles of bytes 128..131 (file offsets 0..3):
        # lane1=5, lane2=6, lane3=7, lane4=0.
        {0: 0x65, 1: 0x07},
    )
    assert checked.values.dp_state_host_lane == [5, 6, 7, 0, 0, 0, 0, 0]
    assert checked.present.dp_state_host_lane is True


def test_auto_commissioning_only_at_5_3():
    # byte 2 bits 1:0, since 5.3.
    checked = decode(CmisLowerMemoryChecked, 256, True, 0x40, {2: 0x03})
    assert checked.values.auto_commissioning == 0
    assert checked.present.auto_commissioning is False
    checked53 = decode(CmisLowerMemoryChecked, 256, True, 0x53, {2: 0x03})
    assert checked53.values.auto_commissioning == 0b11
    assert checked53.present.auto_commissioning is True


# ─── arrays and payload defaults ─────────────────────────────────────────────


def test_page20h_arrays_default_zero_filled_when_absent():
    # The VDM pages landed with CMIS 4.0, not 5.3: the since-window repair
    # (schema commit 0109e2e, citing the CMIS 5.3 revision history Rev 4.0
    # block 'Added VDM Feature' and CMIS 4.0 Tables 8-95..8-105) set the
    # page-20h descriptor fields since 4.0. So the arrays are absent
    # (zero-filled defaults, presence False) at 3.0, and directly read
    # (presence True) from 4.0 onward.
    checked = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x30)
    for name in ("local_threshold_set_id", "monitored_resource", "observable_type"):
        array = getattr(checked.values, name)
        assert array == [0] * 64
        assert getattr(checked.present, name) is False
    for revision in (0x40, 0x50, 0x51, 0x52, 0x53, 0x54):
        present = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, revision)
        for name in ("local_threshold_set_id", "monitored_resource", "observable_type"):
            assert getattr(present.present, name) is True
    checked53 = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x53)
    for name in ("local_threshold_set_id", "monitored_resource", "observable_type"):
        assert getattr(checked53.present, name) is True
    # A present VDM row decodes by direct read: each descriptor is two bytes
    # (file offsets 0..1 for vdm 1) — high nibble local threshold set id,
    # low nibble monitored resource, then observable type.
    probes = decode(
        CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x53, {0: 0x3F, 1: 2}
    )
    assert probes.values.local_threshold_set_id[0] == 3
    assert probes.values.monitored_resource[0] == 0xF
    assert probes.values.observable_type[0] == 2


def test_default_arrays_are_independently_allocated():
    first = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x40)
    second = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x40)
    first.values.monitored_resource[0] = 99
    assert second.values.monitored_resource[0] == 0
    third = decode(CmisPage20hVdmDescriptors1To64Checked, 128, False, 0x40)
    assert third.values.monitored_resource[0] == 0


def test_bytes_payload_zero_filled_to_declared_length():
    # page 14h custom_diagnostics_selection: bytes, size 2, flat offset 130
    # (file offset 2), since 4.0 — absent at 3.0 with a zero-filled default,
    # present and directly read at 4.0.
    checked = decode(CmisPage14hDiagnosticsResultsChecked, 128, False, 0x30)
    assert checked.values.custom_diagnostics_selection == b"\x00\x00"
    assert checked.present.custom_diagnostics_selection is False
    checked40 = decode(
        CmisPage14hDiagnosticsResultsChecked, 128, False, 0x40, {2: 0xAB, 3: 0xCD}
    )
    assert checked40.values.custom_diagnostics_selection == b"\xab\xcd"
    assert checked40.present.custom_diagnostics_selection is True


def test_page9fh_lpl_zero_filled_and_direct_read():
    # lpl: bytes, size 120, file offset 8..127, present in every revision.
    checked = decode(CmisPage9FhCdbMessageChecked, 128, False, 0x40)
    assert checked.values.lpl == bytes(120)
    assert checked.present.lpl is True
    probe = decode(CmisPage9FhCdbMessageChecked, 128, False, 0x40, {8: 1, 9: 2, 10: 3})
    assert probe.values.lpl[:3] == b"\x01\x02\x03"


def test_ascii_member_decodes_by_direct_read():
    # vendor_name: ascii, 16 bytes at file offsets 1..16, present everywhere.
    probe = decode(
        CmisPage00hAdministrativeChecked,
        128,
        False,
        0x30,
        dict(zip(range(1, 17), b"VendorNameExacts")),
    )
    assert probe.values.vendor_name == "VendorNameExacts"
    assert probe.present.vendor_name is True
