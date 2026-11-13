from __future__ import annotations

import struct
from typing import Any, Callable, Optional

from ._common import (
    _CmisKeySet,
    _VALID_CMIS_MAJORS,
    _enum_label,
    _load_generated,
    _lookup_connector,
    _lookup_identifier,
    _make_app_entry,
    _parse_app_descriptors,
    _reject_unknown_page_keys,
)

_BIAS_MULT = {0: 1, 1: 2, 2: 4}  # Page 01h:160 bits 4-3; 0b11 reserved

# VDM descriptor/sample page (generated module, decoder class) pairs per
# group (CMIS § 8.27: page 20h+group carries the group's descriptors,
# 24h+group its samples).
_VDM_DESCRIPTOR_MODULES = (
    ("cmis_page20h_vdm_descriptors1_to64", "CmisPage20hVdmDescriptors1To64"),
    ("cmis_page21h_vdm_descriptors65_to128", "CmisPage21hVdmDescriptors65To128"),
    ("cmis_page22h_vdm_descriptors129_to192", "CmisPage22hVdmDescriptors129To192"),
    ("cmis_page23h_vdm_descriptors193_to256", "CmisPage23hVdmDescriptors193To256"),
)
_VDM_SAMPLE_MODULES = (
    ("cmis_page24h_vdm_samples1_to64", "CmisPage24hVdmSamples1To64"),
    ("cmis_page25h_vdm_samples65_to128", "CmisPage25hVdmSamples65To128"),
    ("cmis_page26h_vdm_samples129_to192", "CmisPage26hVdmSamples129To192"),
    ("cmis_page27h_vdm_samples193_to256", "CmisPage27hVdmSamples193To256"),
)
# Supervision threshold quad pages 28h-2Bh (global sets 1-64), as
# (generated module, decoder class, upper-case page tag) used in the emitted
# `page` field.
_VDM_THRESHOLD_MODULES = (
    ("cmis_page28h_vdm_thresholds1_to16", "CmisPage28hVdmThresholds1To16", "28h"),
    ("cmis_page29h_vdm_thresholds17_to32", "CmisPage29hVdmThresholds17To32", "29h"),
    ("cmis_page2_ah_vdm_thresholds33_to48", "CmisPage2AhVdmThresholds33To48", "2Ah"),
    ("cmis_page2_bh_vdm_thresholds49_to64", "CmisPage2BhVdmThresholds49To64", "2Bh"),
)
# The complete VDM page set (CMIS § 8.27): the `vdm` block is emitted only
# when the caller supplies every one of these pages. 2Eh is not part of the
# VDM feature.
_VDM_PAGE_KEYS = (
    "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
    "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
)


def _vdm_decode_sample(observable_type: int, raw: int) -> dict:
    """Decode one X16 sample register per the descriptor's observable type.

     dispatch (page-24h-vdm-samples variant table), mirroring the
    Rust decode_sample: S16 for TEC current / laser frequency error / laser
    temperature (0x02-0x04) and ELS input power (0x54); F16 (m * 10^(s-24))
    for the Pre-FEC BER and FERC families (0x09-0x1A); raw U16 otherwise.
    """
    if 0x02 <= observable_type <= 0x04 or observable_type == 0x54:
        value = raw - 0x10000 if raw & 0x8000 else raw
        return {"storage": "i16", "value": value}
    if 0x09 <= observable_type <= 0x1A:
        m = raw & 0x7FF
        s = (raw >> 11) & 0x1F
        return {"storage": "f16", "value": m * (10.0 ** (s - 24))}
    return {"storage": "u16", "value": raw}


def _decode_vdm(pages: dict[str, bytes]) -> dict:
    """Build the `vdm` block from the caller-supplied VDM pages (20h-2Fh).

    Mirrors the Rust reference implementation
    (crates/openxcvr-cmis/src/ops/vdm.rs): group gating on the Page 2Fh
    advertisement (vdm_support, CmisVdmSupport: code 0 = group 1 only,
    3 = groups 1-4), polymorphic per-observable-type sample decode, the
    28h-2Bh threshold quads and the 2Ch/2Dh TC flag/mask nibbles. Pure: no
    I/O; a missing or short page of the advertised set raises (fail closed).
    """
    page_2fh = pages["2fh"]
    if len(page_2fh) < 128:
        raise ValueError(f"VDM page 2Fh missing or short: {len(page_2fh)} bytes")
    ad = _load_generated("cmis_page2_fh_vdm_advertisement").CmisPage2FhVdmAdvertisement.decode(page_2fh)

    # CmisVdmSupport code = advertised group count - 1; only advertised
    # groups' descriptor/sample pages are decoded.
    groups = ad.vdm_support + 1

    observables = []
    for group in range(groups):
        desc_buf = pages[f"{0x20 + group:02x}h"]
        if len(desc_buf) < 128:
            raise ValueError(f"VDM descriptor page {0x20 + group:02X}h missing or short: {len(desc_buf)} bytes")
        sample_buf = pages[f"{0x24 + group:02x}h"]
        if len(sample_buf) < 128:
            raise ValueError(f"VDM sample page {0x24 + group:02X}h missing or short: {len(sample_buf)} bytes")
        desc_mod, desc_cls = _VDM_DESCRIPTOR_MODULES[group]
        desc = getattr(_load_generated(desc_mod), desc_cls).decode(desc_buf)
        sample_mod, sample_cls = _VDM_SAMPLE_MODULES[group]
        samples = getattr(_load_generated(sample_mod), sample_cls).decode(sample_buf)
        for local in range(64):
            # An unused descriptor slot is 00h in both bytes: observable
            # type 0 means the instance is not active and its sample is
            # ignored (CMIS Table 8-178).
            if desc.observable_type[local] == 0:
                continue
            raw = samples.vdm_sample[local]
            observables.append({
                "instance": group * 64 + local + 1,
                "group": group + 1,
                "threshold_set_id": desc.local_threshold_set_id[local],
                "global_threshold_set_id": group * 16 + desc.local_threshold_set_id[local] + 1,
                "monitored_resource": desc.monitored_resource[local],
                "observable_type": desc.observable_type[local],
                "active": True,
                "sample_raw": f"0x{raw:04X}",
                "sample": _vdm_decode_sample(desc.observable_type[local], raw),
            })

    threshold_quads = []
    for module, cls, page_tag in _VDM_THRESHOLD_MODULES:
        buf = pages[page_tag.lower()]
        if len(buf) < 128:
            raise ValueError(f"VDM threshold page {page_tag} missing or short: {len(buf)} bytes")
        quads = getattr(_load_generated(module), cls).decode(buf)
        for set_idx in range(16):
            hi_a = quads.high_alarm_threshold[set_idx]
            lo_a = quads.low_alarm_threshold[set_idx]
            hi_w = quads.high_warning_threshold[set_idx]
            lo_w = quads.low_warning_threshold[set_idx]
            # An all-zero quad is an undefined threshold set: emitted only
            # when at least one carrier is non-zero.
            if hi_a == 0 and lo_a == 0 and hi_w == 0 and lo_w == 0:
                continue
            threshold_quads.append({
                "page": page_tag,
                "set": set_idx + 1,
                "high_alarm": f"0x{hi_a:04X}",
                "low_alarm": f"0x{lo_a:04X}",
                "high_warning": f"0x{hi_w:04X}",
                "low_warning": f"0x{lo_w:04X}",
            })

    flags_buf = pages["2ch"]
    if len(flags_buf) < 128:
        raise ValueError(f"VDM page 2Ch missing or short: {len(flags_buf)} bytes")
    masks_buf = pages["2dh"]
    if len(masks_buf) < 128:
        raise ValueError(f"VDM page 2Dh missing or short: {len(masks_buf)} bytes")
    tc = _load_generated("cmis_page2_ch_vdm_flags").CmisPage2ChVdmFlags.decode(flags_buf)
    masks = _load_generated("cmis_page2_dh_vdm_masks").CmisPage2DhVdmMasks.decode(masks_buf)

    def nibble_entries(bits) -> list:
        """Project the 256 per-instance (high_alarm, low_alarm, high_warning,
        low_warning) bit slices into entries, emitting an instance only when
        at least one of its four bits is set (an all-zero nibble carries no
        threshold-crossing state)."""
        hi_a_bits, lo_a_bits, hi_w_bits, lo_w_bits = bits
        out = []
        for i in range(256):
            hi_a, lo_a = hi_a_bits[i], lo_a_bits[i]
            hi_w, lo_w = hi_w_bits[i], lo_w_bits[i]
            if hi_a or lo_a or hi_w or lo_w:
                out.append({
                    "instance": i + 1,
                    "high_alarm": hi_a,
                    "low_alarm": lo_a,
                    "high_warning": hi_w,
                    "low_warning": lo_w,
                })
        return out

    tc_flags = nibble_entries((
        tc.high_alarm_tc_flag, tc.low_alarm_tc_flag,
        tc.high_warning_tc_flag, tc.low_warning_tc_flag,
    ))
    tc_masks = nibble_entries((
        masks.high_alarm_tc_mask, masks.low_alarm_tc_mask,
        masks.high_warning_tc_mask, masks.low_warning_tc_mask,
    ))

    support = {
        "vdm_support": ad.vdm_support,
        "vdm_support_name": _enum_label("cmis_vdm_support", "CmisVdmSupport", ad.vdm_support),
        "groups_advertised": groups,
        "duty_cycle_support": ad.duty_cycle_support,
        "power_saving_support": ad.power_saving_support,
        "fine_interval_length_0p1ms": ad.fine_interval_length,
        "freeze_request": ad.freeze_request,
        "power_saving_mode": ad.power_saving_mode,
        "monitoring_duty_cycle_k": ad.monitoring_duty_cycle,
        "freeze_done": ad.freeze_done,
        "unfreeze_done": ad.unfreeze_done,
    }

    return {
        "support": support,
        "observables": observables,
        "threshold_quads": threshold_quads,
        "tc_flags": tc_flags,
        "tc_masks": tc_masks,
    }


def _decode_lanes_flags(
    lower_buf,
    page01h_buf,
    page11h_buf,
    projection: Optional[Callable[[int], Optional[str]]],
):
    """Build the (lanes, flags) JSON blocks from CMIS Page 11h.

    Leans on the generated Page 11h decoder for all offset/bit math; applies
    only scaling, dp_state naming, and flag-key remapping. `projection` is
    the schema-owned generated name projection bound once by the caller for
    the module's revision byte (Lower:1): its literal string names a raw at
    that revision, and `None` (reserved/unapplicable raw, or a refused —
    unsupported present — context, selected before any call) renders the
    `Unknown<n>` fallback.
    """
    p11 = _load_generated("cmis_page11h_lane_status").CmisPage11hLaneStatus.decode(page11h_buf)

    # LaserBiasTx multiplier from Page 01h:160 bits 4-3 (CMIS 5.3 Table 8-52).
    if page01h_buf is None:
        raise ValueError("Page 11h present but Page 01h missing (needed for Tx-bias multiplier)")
    code = (page01h_buf[160 - 128] >> 3) & 0x03
    # Reserved 0b11 (CMIS 5.3 Table 8-52): the multiplier is undefined, so
    # tx_bias_ma is unrepresentable. mult is None → emit null for it on every
    # lane and keep the rest of the lane block intact.
    mult = _BIAS_MULT.get(code)

    lanes = []
    lane_flags = []
    for i in range(8):
        nibble = p11.dp_state_host_lane[i]
        if projection is not None:
            name = projection(nibble)
        else:
            name = None  # refused context: fallback selected before any call
        lanes.append({
            "index": i + 1,
            "dp_state": name if name is not None else f"Unknown{nibble}",
            # Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
            # (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
            # both from the generated Page 11h decoder.
            "rx_los": p11.los_flag_rx[i],
            "tx_fault": p11.failure_flag_tx[i],
            "rx_power_mw": round(p11.optical_power_rx[i] * 0.0001, 4),
            "tx_bias_ma": None if mult is None else round(p11.laser_bias_tx[i] * 0.002 * mult, 3),
            "tx_power_mw": round(p11.optical_power_tx[i] * 0.0001, 4),
        })
        lane_flags.append({
            "rx_power_high_alarm": p11.optical_power_high_alarm_flag_rx[i],
            "rx_power_low_alarm": p11.optical_power_low_alarm_flag_rx[i],
            "rx_power_high_warning": p11.optical_power_high_warning_flag_rx[i],
            "rx_power_low_warning": p11.optical_power_low_warning_flag_rx[i],
            "tx_bias_high_alarm": p11.laser_bias_high_alarm_flag_tx[i],
            "tx_bias_low_alarm": p11.laser_bias_low_alarm_flag_tx[i],
            "tx_bias_high_warning": p11.laser_bias_high_warning_flag_tx[i],
            "tx_bias_low_warning": p11.laser_bias_low_warning_flag_tx[i],
            "tx_power_high_alarm": p11.optical_power_high_alarm_flag_tx[i],
            "tx_power_low_alarm": p11.optical_power_low_alarm_flag_tx[i],
            "tx_power_high_warning": p11.optical_power_high_warning_flag_tx[i],
            "tx_power_low_warning": p11.optical_power_low_warning_flag_tx[i],
        })

    flags = {
        "data_not_ready": None,
        "interrupt_asserted": not bool(lower_buf[3] & (1 << 0)),
        "temp_high_alarm": bool(lower_buf[9] & (1 << 0)),
        "temp_low_alarm": bool(lower_buf[9] & (1 << 1)),
        "supply_high_alarm": bool(lower_buf[9] & (1 << 4)),
        "supply_low_alarm": bool(lower_buf[9] & (1 << 5)),
        "lanes": lane_flags,
    }
    return lanes, flags


def decode_cmis(pages: dict[str, bytes]) -> dict[str, Any]:
    """Decode CMIS module pages into a structured dict.

    pages keys: 'lower' (required), '00h' (required), '01h' (optional),
    '11h' (optional; present -> per-lane lanes/flags blocks), and the VDM
    pages '20h'-'2fh' (optional; the complete VDM page set present -> the
    `vdm` block). An unknown key fails closed (ValueError naming the key).
    """
    _reject_unknown_page_keys("cmis", pages, _CmisKeySet)
    lower_buf = pages["lower"]
    page00h_buf = pages["00h"]
    page01h_buf = pages.get("01h")
    page11h_buf = pages.get("11h")

    cmis_byte = lower_buf[1]
    cmis_major = cmis_byte >> 4
    cmis_minor = cmis_byte & 0xF

    if cmis_major not in _VALID_CMIS_MAJORS:
        return {
            "decode_status": "rejected",
            "rejection": {
                "detail": (
                    f"Lower memory byte 1 = 0x{cmis_byte:02X}; "
                    "CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. "
                    f"The fboss test corpus uses 0x{cmis_byte:02X} as an explicit anomalous-rev "
                    "regression target."
                ),
                "reason": f"unknown_major_revision: 0x{cmis_byte:02X}",
            },
        }

    lower = _load_generated("cmis_lower_memory").CmisLowerMemory.decode(lower_buf)
    page00h = _load_generated("cmis_page00h_administrative").CmisPage00hAdministrative.decode(page00h_buf)

    vendor_oui = "{:02X}:{:02X}:{:02X}".format(*page00h_buf[17:20])
    date_code = page00h.date_code_year + page00h.date_code_month + page00h.date_code_day_of_month

    sff8024_id = lower_buf[0]
    identifier_name = _lookup_identifier(sff8024_id)

    connector_raw = page00h.connector_type
    connector_name = _lookup_connector(connector_raw)

    memory_model_is_flat = bool((lower_buf[2] >> 7) & 1)
    memory_model = "flat" if memory_model_is_flat else "paged"

    raw_state = lower.module_state
    state_mod = _load_generated("enums/cmis_module_state")
    try:
        state_name = state_mod.CmisModuleState(raw_state).name
        module_state = "".join(w.capitalize() for w in state_name.split("_"))
    except ValueError:
        module_state = f"Unknown{raw_state}"

    temp_raw = struct.unpack_from('>h', lower_buf, 14)[0]
    module_temperature_c = temp_raw / 256.0

    vcc_raw = struct.unpack_from('>H', lower_buf, 16)[0]
    module_voltage_v = vcc_raw * 100 / 1_000_000

    if memory_model_is_flat:
        applications: Optional[list] = None
    else:
        lower_raw = _parse_app_descriptors(lower_buf, 86, 8)
        p01h_raw = _parse_app_descriptors(page01h_buf, 95, 7) if page01h_buf else []

        lower_valid = [a for a in lower_raw if a['mif'] != 0]
        p01h_valid = [a for a in p01h_raw if a['mif'] != 0]

        apps = [_make_app_entry(a['slot'], a['hif'], a['mif'], a['hlc'], a['mlc'], a['hlao'], lower_buf[85])
                for a in lower_valid]
        apps += [_make_app_entry(a['slot'] + 8, a['hif'], a['mif'], a['hlc'], a['mlc'], a['hlao'], lower_buf[85])
                 for a in p01h_valid]

        applications = apps if apps else None

    # ─── lanes / flags (CMIS Page 11h) ────────────────────────────────────────
    # Present only for paged modules that expose Page 11h; flat-memory modules
    # (no Page 11h fixture) keep both null, matching the OpenAPI contract.
    # The schema-owned DPSM name projection is bound ONCE here, from the
    # module's revision byte (Lower:1), before the lane loop: a supported
    # context uses the bound callable (ordinary missing-name fallback for
    # reserved raws); an unsupported present context (admitted major, byte
    # outside the six catalog revisions) selects the fallback branch before
    # any call — every raw renders Unknown<n>.
    if page11h_buf is not None and not memory_model_is_flat:
        projection = _load_generated(
            "enums/cmis_data_path_state"
        ).cmis_data_path_state_for_revision(cmis_byte)
        lanes, flags = _decode_lanes_flags(lower_buf, page01h_buf, page11h_buf, projection)
    else:
        lanes = None
        flags = None

    result = {
        "decode_status": "ok",
        "identity": {
            "date_code": date_code,
            "vendor_name": page00h.vendor_name,
            "vendor_oui": vendor_oui,
            "vendor_pn": page00h.vendor_pn,
            "vendor_rev": page00h.vendor_rev,
            "vendor_sn": page00h.vendor_sn,
        },
        "module": {
            "cmis_revision": f"{cmis_major}.{cmis_minor}",
            "connector": {
                "name": connector_name,
                "raw": f"0x{connector_raw:02X}",
            },
            "identifier": {
                "name": identifier_name,
                "raw": f"0x{sff8024_id:02X}",
            },
            "memory_model": memory_model,
            "module_state": module_state,
            "module_type": None,
        },
        "monitoring": {
            "module_temperature_c": module_temperature_c,
            "module_voltage_v": module_voltage_v,
        },
        "applications": applications,
        "checksums": None,
        "flags": flags,
        "lanes": lanes,
    }

    # ─── VDM (Pages 20h-2Fh) ────────────────────────────────────────────────
    # Emitted only when the caller supplies the COMPLETE VDM page set
    # (20h-2fh): the `vdm` block is a fixture-scoped extension of the decode
    # surface, decoded as a whole from a whole page set. A partial set (e.g.
    # a real-module capture with only some VDM pages) yields no block —
    # never a block fabricated from zero-filled missing pages — and within
    # the block the Page 2Fh advertisement still gates which groups are
    # decoded (fail closed on any short page).
    if all(pages.get(key) is not None for key in _VDM_PAGE_KEYS):
        result["vdm"] = _decode_vdm(pages)

    return result
