"""Native tests for the generated revision-bound CMIS DPSM name projection
(`cmis_data_path_state_for_revision`, schema-owned).

The expected tables below are an independent literal oracle transcribed from
the spec tables (CMIS 3.0 § 1.7.7.1 Table 66; CMIS 4.0 Table 8-58; CMIS 5.3
§ 8.10.1 / Table 8-84), not values derived from a decoder, the generated
table, a raw adapter or runtime case conversion. None means "no name for this
raw at this revision" — the caller renders its own Unknown<n> fallback.
"""

from __future__ import annotations

from typing import Callable, Optional

import pytest

from openxcvr._generated.enums.cmis_data_path_state import (
    CmisDataPathState,
    cmis_data_path_state_for_revision,
)

# Expected name per raw nibble at CMIS 3.0 (Table 66: raws 5-7 reserved).
V3_0: list[Optional[str]] = [
    "Reserved0",
    "DpDeactivated",
    "DpInit",
    "DpDeinit",
    "DpActivated",
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
]

# Expected name per raw nibble at every catalog revision from 4.0 on.
V4_0_PLUS: list[Optional[str]] = [
    "Reserved0",
    "DpDeactivated",
    "DpInit",
    "DpDeinit",
    "DpActivated",
    "DpTxTurnOn",
    "DpTxTurnOff",
    "DpInitialized",
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
]

# The seven known CMIS context bytes (packed-BCD major/minor).
CATALOG_BYTES = (0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54)


def _oracle(rev: int) -> list[Optional[str]]:
    return V3_0 if rev == 0x30 else V4_0_PLUS


def test_factory_domain_is_exactly_the_literal_catalog() -> None:
    for byte in range(256):
        bound = cmis_data_path_state_for_revision(byte) is not None
        assert bound == (byte in CATALOG_BYTES), hex(byte)


def test_projections_match_the_literal_oracle() -> None:
    """112 exact-string cases: every catalog revision x every raw nibble."""
    cases = 0
    for rev in CATALOG_BYTES:
        project = cmis_data_path_state_for_revision(rev)
        assert project is not None, hex(rev)
        for raw in range(16):
            assert project(raw) == _oracle(rev)[raw], (hex(rev), hex(raw))
            cases += 1
    assert cases == 112


def test_unsupported_contexts_refuse() -> None:
    for byte in (0x31, 0x41, 0x55, 0x00, 0xFF):
        assert cmis_data_path_state_for_revision(byte) is None, hex(byte)


def test_out_of_range_revision_arguments_refuse_without_aliasing() -> None:
    """-1, 256 and 0x130 must refuse; none may alias 0x30 or any entry."""
    for bad in (-1, 256, 0x130):
        assert cmis_data_path_state_for_revision(bad) is None, bad


def test_out_of_range_raw_arguments_never_alias() -> None:
    """Raw is not masked or coerced: out-of-byte raws have no name."""
    for rev in CATALOG_BYTES:
        project = cmis_data_path_state_for_revision(rev)
        assert project is not None
        for bad in (-1, 256, 0x130):
            assert project(bad) is None, (hex(rev), bad)


def test_unnamed_raws_have_no_name() -> None:
    for rev in CATALOG_BYTES:
        project = cmis_data_path_state_for_revision(rev)
        assert project is not None
        for raw in (8, 9, 10, 11, 12, 13, 14, 15, 16, 255):
            assert project(raw) is None, (hex(rev), hex(raw))


def test_interleaved_handles_keep_independent_selections() -> None:
    v3: Callable[[int], Optional[str]] = cmis_data_path_state_for_revision(0x30)
    v5: Callable[[int], Optional[str]] = cmis_data_path_state_for_revision(0x53)
    for raw in range(16):
        assert v3(raw) == _oracle(0x30)[raw], raw
        assert v5(raw) == _oracle(0x53)[raw], raw
        # Interleave again to prove no cross-handle state.
        assert v3(raw) == _oracle(0x30)[raw], raw
    # A fresh binding of the same revision agrees with the held one.
    again = cmis_data_path_state_for_revision(0x30)
    assert again is not None and again(0x05) is None
    assert v5(0x05) == "DpTxTurnOn"


def test_flat_enum_semantics_remain_revision_blind() -> None:
    """Existing revision-blind raw labels/cases are unchanged: the 8 named
    members keep their snake_case labels, and raws 8-15 stay unconstructible
    IntEnum values (the decoder owns the Unknown<n> fallback)."""
    labels = {
        0: "reserved_0",
        1: "dp_deactivated",
        2: "dp_init",
        3: "dp_deinit",
        4: "dp_activated",
        5: "dp_tx_turn_on",
        6: "dp_tx_turn_off",
        7: "dp_initialized",
    }
    assert CmisDataPathState.DP_TX_TURN_ON == 5
    assert CmisDataPathState(5).name == "DP_TX_TURN_ON"
    for member in CmisDataPathState:
        assert member.label() == labels[int(member)], member
    for raw in range(8, 16):
        with pytest.raises(ValueError):
            CmisDataPathState(raw)


# ─── Public normalizer behavior on the shared reserved fixture ───────────────
#
# These exercise the PUBLIC `decode_cmis` entrypoint (not the projection
# factory) over the independently authored reserved fixture's pages, built in
# memory — no corpus file is modified. Expected non-label snapshot values come
# from the independently authored golden, never from a decoder run.

import json
import pathlib

from openxcvr import decode_cmis

REPO_ROOT = pathlib.Path(__file__).resolve().parents[3]
RESERVED_STEM = "synthetic-cmis-3-0-qsfp-dd-dp-reserved"

# Every major-3/4/5 context byte outside the literal seven-byte catalog
# (0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54): 0x31..0x3F, 0x41..0x4F, 0x55..0x5F —
# 41 unsupported contexts, explicitly including 0x31/0x41/0x55.
UNSUPPORTED_CONTEXTS = [
    *range(0x31, 0x40),
    *range(0x41, 0x50),
    *range(0x55, 0x60),
]
assert 0x31 in UNSUPPORTED_CONTEXTS
assert 0x41 in UNSUPPORTED_CONTEXTS
assert 0x55 in UNSUPPORTED_CONTEXTS


def _reserved_golden() -> dict:
    path = REPO_ROOT / "tests" / "golden" / "cmis" / f"{RESERVED_STEM}.json"
    return json.loads(path.read_text())


def _reserved_page(suffix: str) -> bytearray:
    path = REPO_ROOT / "tests" / "fixtures" / "cmis" / f"{RESERVED_STEM}.{suffix}.bin"
    data = bytearray(path.read_bytes())
    assert len(data) == 128
    return data


def _pages_for(context_byte: int, raw: int) -> dict[str, bytes]:
    """Reserved-fixture pages, in memory, with Lower:1 = context byte and all
    eight lane state nibbles = raw (packed per Table 65)."""
    lower = _reserved_page("lower")
    lower[1] = context_byte
    p11 = _reserved_page("page-11h")
    state = raw | (raw << 4)
    for i in range(4):
        p11[i] = state
    return {
        "lower": bytes(lower),
        "00h": bytes(_reserved_page("page-00h")),
        "01h": bytes(_reserved_page("page-01h")),
        "11h": bytes(p11),
    }


@pytest.mark.parametrize("ctx", UNSUPPORTED_CONTEXTS, ids=[f"ctx-0x{b:02x}" for b in UNSUPPORTED_CONTEXTS])
def test_unsupported_present_contexts_keep_snapshot_and_use_unknown(ctx: int) -> None:
    """Every nibble 0-15 across all major-3/4/5 context bytes outside the
    six-byte catalog: admission is unchanged (decode_status ok, paged model,
    same identity/monitors/flags) and every present DPSM raw renders
    Unknown<n> — including Unknown0..Unknown4, the values a borrowed
    unversioned table would wrongly name."""
    golden = _reserved_golden()
    major, minor = ctx >> 4, ctx & 0xF
    for raw in range(16):
        result = decode_cmis(_pages_for(ctx, raw))
        assert result["decode_status"] == "ok", (ctx, raw, result.get("rejection"))
        lanes = result["lanes"]
        assert isinstance(lanes, list) and len(lanes) == 8, (ctx, raw)
        want = f"Unknown{raw}"
        for i, lane in enumerate(lanes):
            assert lane["dp_state"] == want, (ctx, raw, i, lane["dp_state"])
            assert lane["index"] == i + 1
        # Non-label snapshot values: byte-identical to the independently
        # authored golden apart from the device revision text.
        assert result["identity"] == golden["identity"]
        assert result["monitoring"] == golden["monitoring"]
        assert result["applications"] == golden["applications"] is None
        assert result["module"]["connector"] == golden["module"]["connector"]
        assert result["module"]["identifier"] == golden["module"]["identifier"]
        assert result["module"]["memory_model"] == "paged"
        assert result["module"]["module_state"] == golden["module"]["module_state"]
        assert result["module"]["module_type"] is None
        assert result["module"]["cmis_revision"] == f"{major}.{minor}"
        assert result["flags"]["interrupt_asserted"] == golden["flags"]["interrupt_asserted"]
        for key in ("temp_high_alarm", "temp_low_alarm", "supply_high_alarm", "supply_low_alarm"):
            assert result["flags"][key] == golden["flags"][key]
        for i, lf in enumerate(result["flags"]["lanes"]):
            assert lf == golden["flags"]["lanes"][i]
        for i, lane in enumerate(lanes):
            for key in ("rx_power_mw", "tx_bias_ma", "tx_power_mw"):
                assert lane[key] == golden["lanes"][i][key], (ctx, raw, i, key)


def test_missing_page11_behavior_is_unchanged() -> None:
    """Missing Page 11h keeps the existing null lane/flag blocks on a paged
    module; a flat-memory module keeps them null even with Page 11h bytes
    supplied; Page 11h present without Page 01h keeps the existing explicit
    prerequisite error. Present-page behavior is untouched by the projection
    integration."""
    lower = _reserved_page("lower")
    pages = {
        "lower": bytes(lower),
        "00h": bytes(_reserved_page("page-00h")),
        "01h": bytes(_reserved_page("page-01h")),
    }
    result = decode_cmis(pages)
    assert result["decode_status"] == "ok"
    assert result["lanes"] is None
    assert result["flags"] is None

    flat = bytearray(lower)
    flat[2] |= 0x80
    result = decode_cmis({
        "lower": bytes(flat),
        "00h": bytes(_reserved_page("page-00h")),
        "01h": bytes(_reserved_page("page-01h")),
        "11h": bytes(_reserved_page("page-11h")),
    })
    assert result["decode_status"] == "ok"
    assert result["lanes"] is None
    assert result["flags"] is None

    with pytest.raises(ValueError, match="Page 01h missing"):
        decode_cmis({
            "lower": bytes(lower),
            "00h": bytes(_reserved_page("page-00h")),
            "11h": bytes(_reserved_page("page-11h")),
        })
