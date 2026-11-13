from __future__ import annotations

from typing import Any

from ._common import (
    _checksum,
    _hex,
    _hex_bytes,
    _load_generated,
    _lookup_connector,
    _lookup_encoding,
    _lookup_identifier,
    _oui,
    _pad_page,
    _reject_unknown_page_keys,
    _SFF8472_KEY_SET,
    _threshold_bounds,
    _threshold_evaluation,
)

def decode_sff8472(pages: dict[str, bytes]) -> dict[str, Any]:
    _reject_unknown_page_keys("sff-8472", pages, _SFF8472_KEY_SET)
    a0h_buf = _pad_page(pages["a0h"], 256)
    a2h_buf = _pad_page(pages["a2h"], 256)

    a0h = _load_generated("page_a0h_base_memory").page_a0h_base_memory.decode(a0h_buf)
    a2h = _load_generated("page_a2h_diagnostics_base").page_a2h_diagnostics_base.decode(a2h_buf)

    module_type = "10GBASE-SR SFP+" if len(a0h.transceiver_compliance) > 3 and a0h.transceiver_compliance[3] else None
    alarm_flags = a2h.alarm_warning_flag

    # SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is a sentinel
    # for rates > 25.4 GBd; the actual high-resolution rate is in byte 66 (250 MBd
    # units → divide by 4 to express in GBd).
    if a0h.signaling_rate_nominal == 0xFF:
        nominal_bit_rate_gbps = a0h.signaling_rate_max / 4.0
    else:
        nominal_bit_rate_gbps = a0h.signaling_rate_nominal / 10

    # SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When byte 92
    # bit 6 (digital_diagnostics_implemented) is clear, A2h holds no meaningful
    # monitoring/flag/lane data — emit null rather than zero-filled structures.
    ddm = a0h.digital_diagnostics_implemented

    flags = None
    monitoring = None
    lanes = None
    if ddm:
        flags = {
            "alarm_summary": {
                "rx_power_high_alarm": alarm_flags[8],
                "rx_power_low_alarm": alarm_flags[9],
                "temp_high_alarm": alarm_flags[0],
                "temp_low_alarm": alarm_flags[1],
                "tx_bias_high_alarm": alarm_flags[4],
                "tx_bias_low_alarm": alarm_flags[5],
                "tx_power_high_alarm": alarm_flags[6],
                "tx_power_low_alarm": alarm_flags[7],
                "vcc_high_alarm": alarm_flags[2],
                "vcc_low_alarm": alarm_flags[3],
            },
            "warning_summary": {
                "rx_power_high_warning": alarm_flags[24],
                "rx_power_low_warning": alarm_flags[25],
                "temp_high_warning": alarm_flags[16],
                "temp_low_warning": alarm_flags[17],
                "tx_bias_high_warning": alarm_flags[20],
                "tx_bias_low_warning": alarm_flags[21],
                "tx_power_high_warning": alarm_flags[22],
                "tx_power_low_warning": alarm_flags[23],
                "vcc_high_warning": alarm_flags[18],
                "vcc_low_warning": alarm_flags[19],
            },
        }
        monitoring = {
            "module_temperature_c": a2h.temperature / 256.0,
            "module_voltage_v": round(a2h.supply_voltage * 0.0001, 4),
        }
        lanes = [
            {
                "index": 1,
                "rx_los": a2h.rx_los_state,
                "rx_power_mw": round(a2h.rx_power * 0.0001, 4),
                "tx_bias_ma": round(a2h.tx_bias * 0.002, 3),
                "tx_disable": a2h.tx_disable_state,
                "tx_fault": a2h.tx_fault_state,
                "tx_power_mw": round(a2h.tx_power * 0.0001, 4),
            }
        ]

    return {
        "decode_status": "ok",
        "identity": {
            "date_code": a0h.date_code[:6],
            "vendor_name": a0h.vendor_name,
            "vendor_oui": _oui(a0h.vendor_oui),
            "vendor_pn": a0h.vendor_pn,
            "vendor_sn": a0h.vendor_sn,
        },
        "module": {
            "calibration": {
                "ddm_implemented": a0h.digital_diagnostics_implemented,
                "externally_calibrated": a0h.externally_calibrated,
                "internally_calibrated": a0h.internally_calibrated,
                "rx_power_kind": "average" if a0h.rx_power_average else "oma",
            },
            "cmis_revision": None,
            "connector": {"name": _lookup_connector(a0h.connector_type), "raw": _hex(a0h.connector_type)},
            "encoding": {"name": _lookup_encoding(a0h.encoding), "raw": _hex(a0h.encoding)},
            "identifier": {"name": _lookup_identifier(a0h.sff8024_identifier), "raw": _hex(a0h.sff8024_identifier)},
            "memory_model": None,
            "module_state": None,
            "module_type": module_type,
            "nominal_bit_rate_gbps": nominal_bit_rate_gbps,
        },
        "monitoring": monitoring,
        "compliance": {
            "raw_bytes_a0h_3_10": _hex_bytes(a0h_buf[3:11]),
            "reach": {
                "om1_m": a0h.length_om1_10m * 10,
                "om2_m": a0h.length_om2_10m * 10,
                # SFF-8472 byte 19 is OM3 length in units of 10 m (classic flat byte;
                # base|multiplier<<6 == the raw byte). ×10 yields meters. This is
                # deliberate: the schema's base(u6)+multiplier(u2 enum) split is not
                # the wire semantics — finisar (byte19=30, golden=300) falsifies the
                # ×0.1 reading. Do not "fix" to a fractional multiplier.
                "om3_m": (a0h.length_om3_base | (a0h.length_om3_multiplier << 6)) * 10,
                "om4_m": a0h.length_om4_or_copper_1m,
                "smf_km": a0h.length_smf_km,
            },
            "wavelength_nm": a0h.wavelength_or_cable_compliance,
        },
        "flags": flags,
        "lanes": lanes,
        "checksums": {
            "cc_base": _checksum(a0h_buf, 0, 63, 63),
            "cc_dmi": _checksum(a2h_buf, 0, 95, 95),
            "cc_ext": _checksum(a0h_buf, 64, 95, 95),
        },
    }


_SFF8472_COMPLIANCE_ADMITTED = frozenset(range(0x01, 0x0B))


def decode_sff8472_lane_signal_flags(pages: dict[str, bytes]) -> list[dict[str, Any]]:
    """Decode the single channel's per-lane signal flags into the unified
    LaneStatus vocabulary the Rust ``Module::lanes()`` surface defines
    (openxcvr-core ``types::LaneStatus``): the A2h byte-110 state mirrors
    (Table 9-16) plus the Adaptive Input EQ Fail Flag (A2h:118.4,
    Table 10-1, schema ``since: "12.5"``).

    Fields the spec does not define are absent from the entry, mirroring
    the Rust serde ``skip_serializing_if`` contract: SFF-8472 defines no
    Tx LOS and no CDR LOL flags, and the adaptive-EQ bit is not modeled
    before rev 12.5 — a compliance byte (A0h:94) below 12.5 never borrows
    the later-revision reading. An A0h:94 value the family does not admit
    mirrors the Rust union-view module (``Sff8472Module::new``), which
    decodes the union of revisions and therefore surfaces the flag."""
    _reject_unknown_page_keys("sff-8472", pages, _SFF8472_KEY_SET)
    a0h = _load_generated("page_a0h_base_memory").page_a0h_base_memory.decode(_pad_page(pages["a0h"], 256))
    a2h = _load_generated("page_a2h_diagnostics_base").page_a2h_diagnostics_base.decode(
        _pad_page(pages["a2h"], 256)
    )

    entry: dict[str, Any] = {
        "index": 1,
        "rx_los": a2h.rx_los_state,
        "tx_disable": a2h.tx_disable_state,
        "tx_fault": a2h.tx_fault_state,
    }
    # Schema `since: "12.5"` (Table 10-1): 0x0A is rev 12.5; admitted
    # values 0x01-0x09 predate the modeled bit and stay absent.
    if a0h.sff8472_compliance == 0x0A or a0h.sff8472_compliance not in _SFF8472_COMPLIANCE_ADMITTED:
        entry["tx_adaptive_eq_fault"] = a2h.adaptive_input_eq_fail_flag
    return [entry]


def evaluate_sff8472_thresholds(pages: dict[str, bytes]) -> dict[str, Any]:
    """Evaluate the five base monitors (temperature, supply voltage, TX
    bias, TX power, RX power) against their A2h alarm/warning threshold
    sets (§ 9.4, Table 9-5) — the counterpart of the Rust
    ``Sff8472Module::evaluate_thresholds`` pure decision.

    Measurements are calibrated exactly as the Rust module accessors:
    internally calibrated modules read the raw A2h monitors through their
    LSB scales, externally calibrated modules apply the linear calibration
    terms (A2h:76-91, slope/256·raw + offset) and the RX-power polynomial
    (A2h:56-75). A0h:92 setting neither calibration bit is an error, never
    a silently uncalibrated decision. Threshold sets the module does not
    implement (all-zero) keep their monitor within bounds."""
    _reject_unknown_page_keys("sff-8472", pages, _SFF8472_KEY_SET)
    import struct as _struct

    a0h_mod = _load_generated("page_a0h_base_memory").page_a0h_base_memory
    a2h_mod = _load_generated("page_a2h_diagnostics_base").page_a2h_diagnostics_base
    a0h = a0h_mod.decode(_pad_page(pages["a0h"], 256))
    a2h = a2h_mod.decode(_pad_page(pages["a2h"], 256))

    if a0h.internally_calibrated:
        external = False
    elif a0h.externally_calibrated:
        external = True
    else:
        raise ValueError("sff-8472 A0h:92: neither internal nor external calibration set")

    linear: list[tuple[float, float]] = []
    poly: list[float] = []
    if external:
        cal = a2h.external_calibration_constants  # bytes 56..91
        for i in range(4):
            slope = _struct.unpack_from(">H", cal, 76 - 56 + 4 * i)[0]
            offset = _struct.unpack_from(">h", cal, 78 - 56 + 4 * i)[0]
            linear.append((slope / 256.0, float(offset)))
        poly = [_struct.unpack_from(">f", cal, 4 * i)[0] for i in range(5)]

    def lin(raw: int, i: int) -> float:
        if not external:
            return float(raw)
        return linear[i][0] * raw + linear[i][1]

    rx_raw = float(a2h.rx_power)
    if not external:
        rx_power = rx_raw
    else:
        rx_power = (
            poly[0] * rx_raw**4 + poly[1] * rx_raw**3 + poly[2] * rx_raw**2 + poly[3] * rx_raw + poly[4]
        )

    return {
        "temperature": _threshold_evaluation(
            lin(a2h.temperature, 2) / 256.0, _threshold_bounds(a2h.temperature_threshold, 1.0 / 256.0)
        ),
        "supply_voltage": _threshold_evaluation(
            lin(a2h.supply_voltage, 3) * 0.0001,
            _threshold_bounds(a2h.supply_voltage_threshold, 0.0001),
        ),
        "tx_bias": _threshold_evaluation(
            lin(a2h.tx_bias, 0) * 0.002, _threshold_bounds(a2h.tx_bias_threshold, 0.002)
        ),
        "tx_power": _threshold_evaluation(
            lin(a2h.tx_power, 1) * 0.0001, _threshold_bounds(a2h.tx_power_threshold, 0.0001)
        ),
        "rx_power": _threshold_evaluation(
            rx_power * 0.0001, _threshold_bounds(a2h.rx_power_threshold, 0.0001)
        ),
    }
