from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

_GEN_DIR = Path(__file__).parent / "_generated"

# ─── generated module loader ──────────────────────────────────────────────────

def _load_generated(name: str):
    full_name = f"openxcvr._generated.{name.replace('/', '.')}"
    if full_name in sys.modules:
        return sys.modules[full_name]
    path = _GEN_DIR / f"{name}.py"
    spec = importlib.util.spec_from_file_location(full_name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[full_name] = mod
    spec.loader.exec_module(mod)
    return mod

# ─── SFF-8024 lookup tables ───────────────────────────────────────────────────

_VALID_CMIS_MAJORS = {3, 4, 5}

_SFF8636_EXT_SPEC = {
    0x02: "base_100g_sr4_or_25g_sr",
}

# ─── canonical page-map keys (bindings/README.md consumer facade) ────────────
# An unknown key fails closed: a language error naming the key, matching the
# Go/TS canonical adapters (rejectUnknownKeys / rejectUnknown).

_CmisKeySet = (
    frozenset({"lower", "00h", "01h", "11h"})
    | {f"{b:02x}h" for b in range(0x20, 0x28)}  # 20h-27h
    | {f"{b:02x}h" for b in range(0x28, 0x2E)}  # 28h-2dh (2eh is not a facade key)
    | {"2fh"}
)
_SFF8636_KEY_SET = frozenset({"lower", "00h", "03h"})
_SFF8472_KEY_SET = frozenset({"a0h", "a2h"})


def _reject_unknown_page_keys(family: str, pages: dict[str, bytes], allowed: frozenset[str]) -> None:
    """Fail closed on a page-map key outside the canonical facade set."""
    for key in pages:
        if key not in allowed:
            raise ValueError(f"{family} pages: unknown key {key!r}")

# ─── helpers ──────────────────────────────────────────────────────────────────

def _enum_label(module: str, class_name: str, value: int, default: str = "unknown") -> str:
    enum_cls = getattr(_load_generated(f"enums/{module}"), class_name)
    try:
        return enum_cls(value).label()
    except ValueError:
        return default


def _lookup_identifier(identifier: int) -> str:
    return _enum_label("sff8024_identifier", "Sff8024Identifier", identifier)


def _lookup_connector(connector: int) -> str:
    return _enum_label("sff8024_connector_type", "Sff8024ConnectorType", connector)


def _lookup_encoding(encoding: int) -> str:
    return _enum_label("sff8024_encoding", "Sff8024Encoding", encoding)


def _lookup_host_interface(host_interface: int) -> str:
    return _enum_label("sff8024_host_electrical_interface", "Sff8024HostElectricalInterface", host_interface)


def _lookup_device_technology(device_technology: int) -> str:
    return _enum_label("sff8636_device_technology", "Sff8636DeviceTechnology", device_technology)


def _hex(value: int) -> str:
    return f"0x{value:02X}"


def _hex_nibble(value: int) -> str:
    return f"0x{value:X}"


def _hex_bytes(buf: bytes) -> list[str]:
    return [_hex(b) for b in buf]


def _oui(buf: bytes) -> str:
    return "{:02X}:{:02X}:{:02X}".format(*buf)


def _checksum(buf: bytes, start: int, end: int, stored_offset: int) -> dict[str, Any]:
    computed = sum(buf[start:end]) & 0xFF
    stored = buf[stored_offset]
    return {
        "computed": _hex(computed),
        "stored": _hex(stored),
        "valid": computed == stored,
    }


def _pad_page(buf: bytes, size: int) -> bytes:
    return buf if len(buf) >= size else buf + bytes(size - len(buf))


def _parse_app_descriptors(buf: bytes, start_offset: int, count: int) -> list[dict]:
    """Parse 4-byte application descriptors from buf.

    Returns all entries up to first HIF=0xFF terminator. Does NOT filter MIF=0.
    """
    apps = []
    for i in range(count):
        off = start_offset + 4 * i
        if off + 4 > len(buf):
            break
        hif = buf[off]
        mif = buf[off + 1]
        lanes = buf[off + 2]
        hlao = buf[off + 3]
        if hif == 0xFF:
            break
        apps.append({
            'slot': i + 1,
            'hif': hif,
            'mif': mif,
            'hlc': (lanes >> 4) & 0xF,
            'mlc': lanes & 0xF,
            'hlao': hlao,
        })
    return apps


def _make_app_entry(code: int, hif: int, mif: int, hlc: int, mlc: int, hlao: int, media_type: int) -> dict[str, Any]:
    resolver = _load_generated("enums/cmis_media_interface").cmis_media_interface_name
    name = resolver(media_type, mif)
    return {
        "app_code": code,
        "host_interface_id": hif,
        "host_interface_name": _lookup_host_interface(hif),
        "host_lane_assignment_options": f"0x{hlao:02X}",
        "host_lane_count": hlc,
        "media_interface_id": mif,
        "media_interface_name": name if name is not None else "unknown",
        "media_lane_count": mlc,
    }


def _threshold_bounds(raw: list[int], scale: float) -> list[float | None]:
    """Convert one raw threshold set (severity order high_alarm, low_alarm,
    high_warning, low_warning) through a scale to physical units. An
    all-zero raw set is the not-implemented encoding — every bound disabled
    (`None`) — because a zero high-alarm bound would spuriously trip on
    every reading. Mirrors openxcvr-core's ``Thresholds::default()``."""
    if all(v == 0 for v in raw):
        return [None, None, None, None]
    return [v * scale for v in raw]


def _threshold_evaluation(measurement: float, bounds: list[float | None]) -> dict[str, bool]:
    """Mirror openxcvr-core's ``threshold::evaluate``: inclusive bounds (a
    measurement equal to a bound trips it, mirroring the SFF latched-flag
    semantics) and disabled (``None``) bounds never trip."""
    high_alarm, low_alarm, high_warning, low_warning = bounds
    return {
        "high_alarm": high_alarm is not None and measurement >= high_alarm,
        "low_alarm": low_alarm is not None and measurement <= low_alarm,
        "high_warning": high_warning is not None and measurement >= high_warning,
        "low_warning": low_warning is not None and measurement <= low_warning,
    }
