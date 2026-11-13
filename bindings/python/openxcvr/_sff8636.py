from __future__ import annotations

from typing import Any

from ._common import (
    _SFF8636_EXT_SPEC,
    _SFF8636_KEY_SET,
    _checksum,
    _hex,
    _hex_bytes,
    _hex_nibble,
    _load_generated,
    _lookup_connector,
    _lookup_device_technology,
    _lookup_encoding,
    _lookup_identifier,
    _oui,
    _reject_unknown_page_keys,
)

def decode_sff8636(pages: dict[str, bytes]) -> dict[str, Any]:
    _reject_unknown_page_keys("sff-8636", pages, _SFF8636_KEY_SET)
    lower_buf = pages["lower"]
    page00h_buf = pages["00h"]

    lower = _load_generated("sff8636_lower_memory").Sff8636LowerMemory.decode(lower_buf)
    page00h = _load_generated("sff8636_page00h_upper_administrative").Sff8636Page00hUpperAdministrative.decode(page00h_buf)

    ext_spec = None
    if page00h.ext_spec_compliance:
        ext_spec = {
            "name": _SFF8636_EXT_SPEC.get(page00h.ext_spec_compliance, "unknown"),
            "raw": _hex(page00h.ext_spec_compliance),
        }

    bits_set = []
    if page00h.compliance_extended:
        bits_set.append("compliance_extended")
    if page00h.compliance_40gbase_sr4:
        bits_set.append("compliance_40gbase_sr4")

    interrupt_asserted = (not lower.interrupt_deasserted) or any(lower.tx_los_flag) or any(lower.rx_los_flag) or any(
        value != 0 for value in lower.rx_power_alarm_warning_flag + lower.tx_bias_alarm_warning_flag + lower.tx_power_alarm_warning_flag
    )

    module_type = None
    if page00h.ext_spec_compliance == 0x02:
        module_type = "100GBASE-SR4 QSFP28"
    elif page00h.compliance_40gbase_sr4:
        module_type = "40GBASE-SR4 QSFP+"

    nominal_rate = page00h.ext_baud_rate / 4 if page00h.nominal_signaling_rate == 0xFF else page00h.nominal_signaling_rate / 10

    return {
        "decode_status": "ok",
        "identity": {
            "date_code": page00h.date_code_year + page00h.date_code_month + page00h.date_code_day,
            "vendor_name": page00h.vendor_name,
            "vendor_oui": _oui(page00h.vendor_oui),
            "vendor_pn": page00h.vendor_pn,
            "vendor_rev": page00h.vendor_rev,
            "vendor_sn": page00h.vendor_sn,
        },
        "module": {
            "cmis_revision": None,
            "connector": {"name": _lookup_connector(page00h.connector_type), "raw": _hex(page00h.connector_type)},
            "encoding": {"name": _lookup_encoding(page00h.encoding), "raw": _hex(page00h.encoding)},
            "extended_identifier": _hex(page00h.power_class_low | (page00h.power_class_high << 6) | (int(page00h.rx_cdr_present) << 2) | (int(page00h.tx_cdr_present) << 3) | (int(page00h.clei_present) << 4) | (int(page00h.power_class_8_implemented) << 5)),
            "identifier": {"name": _lookup_identifier(lower.sff8024_identifier), "raw": _hex(lower.sff8024_identifier)},
            "memory_model": "flat" if lower.flat_memory else "paged",
            "module_state": None,
            "module_type": module_type,
            "nominal_bit_rate_gbps": nominal_rate,
        },
        "monitoring": {
            "module_temperature_c": lower.temperature / 256.0,
            "module_voltage_v": round(lower.supply_voltage * 0.0001, 4),
        },
        "compliance": {
            "bits_set": bits_set,
            "device_technology": {"name": _lookup_device_technology(page00h.device_technology), "raw": _hex_nibble(page00h.device_technology)},
            "ext_spec_byte_192": ext_spec,
            "raw_bytes_page_00h_3_10": _hex_bytes(page00h_buf[3:11]),
            "reach": {
                "om1_m": page00h.length_om1_or_copper_attenuation_25p78ghz,
                "om2_m": page00h.length_om2_1m,
                "om3_2m_units": page00h.length_om3_2m * 2,
                "om4_or_copper_m": page00h.length_cable_or_om4,
                "smf_km": page00h.length_smf_km,
            },
        },
        "flags": {
            "data_not_ready": lower.data_not_ready,
            "interrupt_asserted": interrupt_asserted,
        },
        "lanes": [
            {
                "index": i + 1,
                "rx_power_mw": round(lower.rx_power_lane[i] * 0.0001, 4),
                "tx_bias_ma": round(lower.tx_bias_lane[i] * 0.002, 3),
                "tx_power_mw": round(lower.tx_power_lane[i] * 0.0001, 4),
            }
            for i in range(4)
        ],
        "checksums": {
            "cc_base": _checksum(page00h_buf, 0, 63, 63),
            "cc_ext": _checksum(page00h_buf, 64, 95, 95),
        },
    }


_SFF8636_COMPLIANCE_ADMITTED = frozenset(range(0x03, 0x0B))


def decode_sff8636_lane_signal_flags(pages: dict[str, bytes]) -> list[dict[str, Any]]:
    """Decode the four lanes' per-lane signal flags into the unified
    LaneStatus vocabulary the Rust ``Module::lanes()`` surface defines: the
    latched Lower-memory flags of Table 6-5 — Tx LOS (byte 3 bits 7-4),
    Tx fault (byte 4 bits 3-0), Tx adaptive-EQ fault (byte 4 bits 7-4,
    since rev 2.0), Tx CDR LOL (byte 5 bits 7-4), Rx LOS (byte 3 bits
    3-0), Rx CDR LOL (byte 5 bits 3-0) — plus the Tx-disable lane state
    (byte 86).

    The adaptive-EQ flag carries a revision window (schema
    ``since: "2.0"``): a detected Lower:1 compliance byte of 0x03-0x05
    (revs 1.3-1.5) leaves it absent, mirroring the Rust ``None`` — never a
    borrowed later-revision reading. A Lower:1 value the family does not
    admit mirrors the Rust union-view module (``Sff8636Module::new``),
    which decodes the union of revisions and therefore surfaces the flag."""
    _reject_unknown_page_keys("sff-8636", pages, _SFF8636_KEY_SET)
    from ._common import _load_generated, _pad_page

    lower = _load_generated("sff8636_lower_memory").Sff8636LowerMemory.decode(_pad_page(pages["lower"], 128))

    adaptive_eq_modeled = lower.revision_compliance >= 0x06 or (
        lower.revision_compliance not in _SFF8636_COMPLIANCE_ADMITTED
    )

    lanes = []
    for i in range(4):
        entry: dict[str, Any] = {
            "index": i + 1,
            "rx_los": lower.rx_los_flag[i],
            "tx_disable": lower.tx_disable_lane[i],
            "tx_fault": lower.tx_fault_flag[i],
            "tx_los": lower.tx_los_flag[i],
            "tx_cdr_lol": lower.tx_cdr_lol_flag[i],
            "rx_cdr_lol": lower.rx_cdr_lol_flag[i],
        }
        if adaptive_eq_modeled:
            entry["tx_adaptive_eq_fault"] = lower.tx_adaptive_eq_fault_flag[i]
        lanes.append(entry)
    return lanes


def evaluate_sff8636_thresholds(pages: dict[str, bytes]) -> dict[str, Any]:
    """Evaluate the Lower-memory monitors (case temperature, supply
    voltage, per-lane RX power / TX bias / TX power) against the Upper
    Page 03h threshold sets (§ 6.6.1, Table 6-28) — the counterpart of the
    Rust ``Sff8636Module::evaluate_thresholds`` pure decision, with the
    same caller-supplied-bytes contract: ``pages["03h"]`` is the 128-byte
    Page 03h window the caller already read (missing or short raises — a
    zero page is never fabricated, it would silently disable every
    threshold set). Monitor scales mirror the module accessors
    (temperature 1/256 °C, supply voltage 100 µV, optical power 0.1 µW,
    bias 2 µA); a threshold set the module does not implement (all-zero)
    keeps its monitor within bounds."""
    _reject_unknown_page_keys("sff-8636", pages, _SFF8636_KEY_SET)
    from ._common import _load_generated, _pad_page, _threshold_bounds, _threshold_evaluation

    lower = _load_generated("sff8636_lower_memory").Sff8636LowerMemory.decode(_pad_page(pages["lower"], 128))
    page03h_raw = pages.get("03h")
    if page03h_raw is None or len(page03h_raw) < 128:
        raise ValueError(f"sff-8636 page03h: expected >=128 bytes, got {len(page03h_raw) if page03h_raw is not None else 0}")
    page03h = _load_generated("sff8636_page03h_thresholds_and_controls").Sff8636Page03hThresholdsAndControls.decode(
        bytes(page03h_raw[:128])
    )

    rx_bounds = _threshold_bounds(page03h.rx_power_threshold, 0.0001)
    bias_bounds = _threshold_bounds(page03h.tx_bias_threshold, 0.002)
    tx_bounds = _threshold_bounds(page03h.tx_power_threshold, 0.0001)
    return {
        "temperature": _threshold_evaluation(
            lower.temperature / 256.0, _threshold_bounds(page03h.temp_mon_threshold, 1.0 / 256.0)
        ),
        "supply_voltage": _threshold_evaluation(
            lower.supply_voltage * 0.0001, _threshold_bounds(page03h.vcc_mon_threshold, 0.0001)
        ),
        "rx_power": [
            _threshold_evaluation(lower.rx_power_lane[i] * 0.0001, rx_bounds) for i in range(4)
        ],
        "tx_bias": [
            _threshold_evaluation(lower.tx_bias_lane[i] * 0.002, bias_bounds) for i in range(4)
        ],
        "tx_power": [
            _threshold_evaluation(lower.tx_power_lane[i] * 0.0001, tx_bounds) for i in range(4)
        ],
    }
