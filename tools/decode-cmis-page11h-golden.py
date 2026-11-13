#!/usr/bin/env python3
"""CMIS Page 11h per-lane diagnostic (issue #105).

Diagnostic view of the `lanes` and `flags` blocks of a CMIS decode: it
renders them through the SAME schema-owned generated projection
(`bindings/python/openxcvr/_generated/enums/cmis_data_path_state.py`,
loaded from the repository tree — no installation) that the public
normalizers use, selected once from the module's revision byte (Lower:1).

Role boundaries (corrected from an earlier claim):
  - This tool is NOT the name authority and NOT a golden author. DPSM label
    semantics are schema-owned (the generated projection), and the golden
    JSON under `tests/golden/cmis/` is independently authored — the
    synthetic builders (`tools/build-cmis-*-synthetic.py`) transcribe the
    spec tables, and the corpus goldens carry established bytes. This
    diagnostic renders a view for inspection; it does not generate goldens.
  - What it still owns: the field offsets/packing below (flat CMIS page
    addresses, the Table 65 nibble packing, the Table 8-52 bias multiplier
    and its reserved code), the rounding/precision shape, and the flags-key
    remapping — the arithmetic skeleton the per-lane blocks are built on.

All field offsets are flat CMIS page addresses (the upper page begins at
byte 128). The per-page `.bin` fixtures are 128-byte slices, so the buffer
index for a flat offset is `offset - 128`.

Primary sources (CMIS 5.3):
  - Page 11h per-lane monitors/flags: § 8.10.4, Table 8-89 (pdf p.213).
  - LaserBiasTx multiplier: Table 8-52 (Page 01h:160 bits 4-3) ->
        00b: x1, 01b: x2, 10b: x4, 11b: reserved (pdf p.176).
  - LaserBiasTx LSB = 2 uA * multiplier; OpticalPower{Tx,Rx} LSB = 0.1 uW.
  - Lower-memory module flags: byte 3 bit0 (interrupt deasserted),
        byte 9 bits 0/1 (temp hi/lo alarm), byte 9 bits 4/5 (vcc hi/lo alarm).

Unsupported revision contexts fail closed: a Lower:1 byte with no
schema-owned projection (outside the admitted catalog revisions 3.0/4.0/
5.0/5.1/5.2/5.3/5.4) raises an explicit diagnostic error naming the byte
BEFORE any projection invocation — never borrowed labels, never a masked
byte. The generated factory is the admission authority; admission follows
the schema (5.4 registered by 19efcdd).
"""
import importlib.util
import json
import os
import pathlib
import struct
import sys

PAGE_BASE = 128  # flat offset of the first byte of an upper page

# Page 11h monitor base offsets (flat); each is a u16be array indexed by lane.
TX_POWER_BASE = 154   # OpticalPowerTx<lane>  = 154 + 2*(lane-1)
TX_BIAS_BASE = 170    # LaserBiasTx<lane>     = 170 + 2*(lane-1)
RX_POWER_BASE = 186   # OpticalPowerRx<lane>  = 186 + 2*(lane-1)

DP_STATE_BASE = 128   # dp_state_host_lane: byte 128 + (lane-1)//2, nibble per lane

# Page 11h per-lane flag bytes (flat); bit index within the byte = lane - 1.
# Output-key -> source flag byte. Mapping per / summary:
#   tx_power <- optical_power_*_tx, tx_bias <- laser_bias_*_tx,
#   rx_power <- optical_power_*_rx.
LANE_FLAG_BYTES = {
    "tx_power_high_alarm": 139,
    "tx_power_low_alarm": 140,
    "tx_power_high_warning": 141,
    "tx_power_low_warning": 142,
    "tx_bias_high_alarm": 143,
    "tx_bias_low_alarm": 144,
    "tx_bias_high_warning": 145,
    "tx_bias_low_warning": 146,
    "rx_power_high_alarm": 149,
    "rx_power_low_alarm": 150,
    "rx_power_high_warning": 151,
    "rx_power_low_warning": 152,
}

BIAS_MULT = {0: 1, 1: 2, 2: 4}  # 3 (0b11) is reserved

# Generated projection module, loaded from the repository tree (no install).
_GENERATED_ENUM_PATH = (
    pathlib.Path(__file__).resolve().parent.parent
    / "bindings" / "python" / "openxcvr" / "_generated" / "enums"
    / "cmis_data_path_state.py"
)


def _load_generated_factory():
    """Load `cmis_data_path_state_for_revision` from the repo tree once."""
    spec = importlib.util.spec_from_file_location(
        "openxcvr_diag_cmis_data_path_state", _GENERATED_ENUM_PATH
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.cmis_data_path_state_for_revision


_CmisDataPathStateForRevision = _load_generated_factory()


def u16be(buf, flat_off):
    i = flat_off - PAGE_BASE
    return struct.unpack(">H", buf[i:i + 2])[0]


def flag_bit(buf, flat_off, lane):
    """Bit (lane-1) of the byte at `flat_off` in a 128-byte upper-page slice."""
    return bool(buf[flat_off - PAGE_BASE] & (1 << (lane - 1)))


def dp_state_str(p11, lane, projection):
    """Render a lane's Data Path State label via the bound projection.

    The projection (selected once from the module's revision byte) is the
    single name authority; a raw it does not name renders the Unknown<n>
    fallback, exactly like the public normalizers.
    """
    byte = p11[(DP_STATE_BASE + (lane - 1) // 2) - PAGE_BASE]
    nibble = (byte >> (((lane - 1) % 2) * 4)) & 0xF
    name = projection(nibble)
    return name if name is not None else "Unknown%d" % nibble


def _bind_projection(rev_byte):
    """Select the schema-owned projection for a revision byte, once.

    Fails closed BEFORE any projection invocation when the byte admits no
    generated projection (not one of the catalog revisions): an explicit
    diagnostic error naming the byte, never borrowed labels and never a
    setup/import failure.
    """
    projection = _CmisDataPathStateForRevision(rev_byte)
    if projection is None:
        raise SystemExit(
            "unsupported CMIS revision byte 0x%02X at Lower:1: no schema-owned "
            "Data Path State projection exists for this context; refusing to "
            "render revision-unqualified labels" % rev_byte
        )
    return projection


def bias_multiplier(p01):
    """LaserBiasTx scaling multiplier from Page 01h:160 bits 4:3 (CMIS 5.3
    Table 8-52). Reserved code 0b11 has no defined multiplier, so tx_bias_ma
    is unrepresentable: return None and let the caller emit null, matching
    the five bindings (issues #198/#200) rather than dropping the whole lane block."""
    code = (p01[160 - PAGE_BASE] >> 3) & 0x03
    return BIAS_MULT.get(code)


def decode(lower, p01, p11):
    """Return (lanes, flags) dicts for a CMIS module with Page 11h present."""
    projection = _bind_projection(lower[1])
    mult = bias_multiplier(p01)
    lanes = []
    lane_flags = []
    for lane in range(1, 9):
        tx_raw = u16be(p11, TX_POWER_BASE + 2 * (lane - 1))
        bias_raw = u16be(p11, TX_BIAS_BASE + 2 * (lane - 1))
        rx_raw = u16be(p11, RX_POWER_BASE + 2 * (lane - 1))
        lanes.append({
            "index": lane,
            "dp_state": dp_state_str(p11, lane, projection),
            "rx_power_mw": round(rx_raw * 0.0001, 4),
            "tx_bias_ma": None if mult is None else round(bias_raw * 0.002 * mult, 3),
            "tx_power_mw": round(tx_raw * 0.0001, 4),
        })
        lane_flags.append({
            key: flag_bit(p11, off, lane) for key, off in LANE_FLAG_BYTES.items()
        })

    flags = {
        "data_not_ready": None,
        "interrupt_asserted": not bool(lower[3] & (1 << 0)),
        "temp_high_alarm": bool(lower[9] & (1 << 0)),
        "temp_low_alarm": bool(lower[9] & (1 << 1)),
        "supply_high_alarm": bool(lower[9] & (1 << 4)),
        "supply_low_alarm": bool(lower[9] & (1 << 5)),
        "lanes": lane_flags,
    }
    return lanes, flags


def load(fixdir, stem, suffix):
    path = os.path.join(fixdir, "%s.%s" % (stem, suffix))
    if not os.path.exists(path):
        return None
    with open(path, "rb") as fh:
        return fh.read()


def decode_fixture(fixdir, stem):
    """Return (lanes, flags) or (None, None) when Page 11h is absent."""
    p11 = load(fixdir, stem, "page-11h.bin")
    if p11 is None:
        return None, None
    lower = load(fixdir, stem, "lower.bin")
    p01 = load(fixdir, stem, "page-01h.bin")
    if lower is None or p01 is None:
        raise SystemExit("%s: page-11h present but lower/page-01h missing" % stem)
    return decode(lower, p01, p11)


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: decode-cmis-page11h-golden.py <stem> [fixdir]")
    stem = sys.argv[1]
    fixdir = sys.argv[2] if len(sys.argv) > 2 else "tests/fixtures/cmis"
    lanes, flags = decode_fixture(fixdir, stem)
    print(json.dumps({"lanes": lanes, "flags": flags}, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
