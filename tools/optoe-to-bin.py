#!/usr/bin/env python3
"""Extract real transceiver EEPROMs from SONiC / OPTOE-based Linux platforms
into canonical per-page `.bin` files matching the openxcvr.page.v1 schema topology.

This tool runs **on the switch** (or any Linux host with an OPTOE-driven
transceiver visible on i2c). Output per-page `.bin` files + a draft `.md`
provenance template can then be rsync'd / scp'd off the switch and submitted
to the openxcvr fixture corpus via PR.

Read paths (priority order):

    1. `ethtool --module-eeprom-page` (ethtool v5.x+; cleanest, supports
       arbitrary page+offset reads on every supported transceiver type).
    2. `ethtool -m <iface> hex on raw on` (older ethtool; covers Lower + first
       Upper page only — sufficient for SFP and QSFP flat-mem, partial for
       paged QSFP-DD/CMIS).
    3. Direct sysfs read at `/sys/bus/i2c/devices/<bus>-0050/eeprom`. Requires
       knowing the i2c bus number (typically derivable from `sfputil` or
       platform-specific tooling). Bypasses ethtool entirely.
    4. SONiC `sfputil show eeprom-pages -p <port>` (last-resort; SONiC-only).

Outputs the same per-page topology as the existing OOM and fboss converters:

    <stem>.a0h.bin              SFP A0h base ID memory (SFF-8472)
    <stem>.a2h.bin              SFP A2h diagnostics (SFF-8472)
    <stem>.lower.bin            Lower memory (SFF-8636 / CMIS)
    <stem>.page-NNh.bin         Upper Page 0xNN (SFF-8636 / CMIS)
    <stem>.md                   Draft provenance — REQUIRES MANUAL REVIEW

Dependencies: Python stdlib only. Subprocess calls to `ethtool` / `sfputil`
where available — no Python packages required.

License: same as the openxcvr project (MIT; see LICENSE).
Contributed fixtures land under per-fixture license declarations (Apache-2.0
or MIT preferred for new submissions; see tests/fixtures/README.md).

Usage:
    sudo tools/optoe-to-bin.py --interface Ethernet0 --stem /tmp/myswitch-port-0
    sudo tools/optoe-to-bin.py --sysfs /sys/bus/i2c/devices/2-0050/eeprom --stem /tmp/myswitch-port-0
    sudo tools/optoe-to-bin.py --sysfs-a0h /sys/bus/i2c/devices/2-0050/eeprom --stem /tmp/myswitch-port-1

The tool emits a draft `<stem>.md` provenance file. **Review and edit it**
before submission — fields like vendor S/N and date code may need redaction
depending on whether the deployment is identifiable.
"""
from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

# ---------- SFF-8024 identifier table (subset) -----------------------------------

IDENTIFIER_TO_SPEC: dict[int, str] = {
    0x03: "sff-8472",   # SFP / SFP+ / SFP28
    0x0B: "sff-8472",   # Module + cable
    0x0C: "sff-8636",   # QSFP (legacy)
    0x0D: "sff-8636",   # QSFP+
    0x11: "sff-8636",   # QSFP28
    0x18: "cmis",       # QSFP-DD double density
    0x19: "cmis",       # OSFP
    0x1B: "cmis",       # DSFP
    0x1E: "cmis",       # QSFP112
    0x1F: "cmis",       # QSFP-DD CMIS r5+
}

IDENTIFIER_LABELS: dict[int, str] = {
    0x03: "SFP / SFP+ / SFP28",
    0x0B: "Module + cable",
    0x0C: "QSFP (legacy)",
    0x0D: "QSFP+",
    0x11: "QSFP28",
    0x18: "QSFP-DD",
    0x19: "OSFP",
    0x1B: "DSFP",
    0x1E: "QSFP112",
    0x1F: "QSFP-DD (CMIS r5+)",
}

# CMIS upper pages worth probing on every QSFP-DD/OSFP module. Vendor-specific
# pages (0x9F CDB, 0xA0-0xAF CDB EPL window, banked Pages 0x10-0x16) are NOT
# probed here because page presence depends on advertisement bits in Page 0x01.
# A future revision can read Page 0x01 first and conditionally enumerate.
CMIS_PROBE_PAGES = [
    0x00,  # Administrative
    0x01,  # Advertising
    0x02,  # Thresholds (instances 1-8)
    0x10,  # Lane Control (banked — bank 0)
    0x11,  # Lane Status (banked — bank 0)
    0x12,  # Tunable Laser (banked — bank 0; optional)
    0x13,  # Diagnostic Control (banked — bank 0)
    0x14,  # Diagnostic Results (banked — bank 0)
    0x15,  # Timing (banked — bank 0)
    0x16,  # Network Path (banked — bank 0)
    0x20,  # VDM Descriptors 1-64
    0x21,  # VDM Descriptors 65-128
    0x22,  # VDM Descriptors 129-192
    0x23,  # VDM Descriptors 193-256
    0x24,  # VDM Samples 1-64
    0x25,  # VDM Samples 65-128
    0x26,  # VDM Samples 129-192
    0x27,  # VDM Samples 193-256
    0x2C,  # VDM Flags
    0x2D,  # VDM Masks
    0x2F,  # VDM Advertisement
]

SFF8636_PAGES = [0x00, 0x01, 0x02, 0x03, 0x20, 0x21]


# ---------- Read paths -----------------------------------------------------------


class ReadError(RuntimeError):
    """Raised when a read path fails. Caller can fall through to the next path."""


def _check_command_exists(cmd: str) -> bool:
    return shutil.which(cmd) is not None


def _run(cmd: list[str]) -> str:
    """Run a subprocess; return stdout as text; raise ReadError on failure."""
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, check=False, timeout=30)
    except FileNotFoundError as e:
        raise ReadError(f"command not found: {cmd[0]}") from e
    except subprocess.TimeoutExpired as e:
        raise ReadError(f"timeout: {' '.join(cmd)}") from e
    if proc.returncode != 0:
        raise ReadError(
            f"{' '.join(cmd)} failed (exit {proc.returncode}): {proc.stderr.strip()}"
        )
    return proc.stdout


def read_via_ethtool_paged(interface: str, page: int, offset: int = 0, length: int = 128) -> bytes:
    """Read one page slice via `ethtool --module-eeprom-page` (v5.x+)."""
    out = _run(
        [
            "ethtool",
            "--module-eeprom-page",
            interface,
            "page",
            str(page),
            "offset",
            str(offset),
            "length",
            str(length),
        ]
    )
    return _parse_ethtool_hex(out, expected_bytes=length)


def read_via_ethtool_legacy(interface: str) -> bytes:
    """Read EEPROM via `ethtool -m <iface> hex on raw on`. Returns whatever the
    legacy interface gives us — typically 256 bytes for SFP or 256 bytes
    (lower + page 00h) for QSFP. Insufficient for paged CMIS but works as
    fallback for SFP and QSFP flat-mem."""
    out = _run(["ethtool", "-m", interface, "hex", "on", "raw", "on"])
    return _parse_ethtool_hex(out, expected_bytes=None)


_HEX_LINE_RE = re.compile(r"^\s*(?:0x)?[0-9a-fA-F]+:\s*((?:[0-9a-fA-F]{2}\s*)+)")


def _parse_ethtool_hex(text: str, *, expected_bytes: int | None) -> bytes:
    """Parse `ethtool -m ... hex on` output. Lines look like:
        0x0000: 18 40 80 03 00 00 00 00 00 00 ...
    Some ethtool builds also accept `raw on` to emit binary on stdout — but
    Python's subprocess captures stdout as text by default; we accept both.
    """
    out = bytearray()
    for line in text.splitlines():
        m = _HEX_LINE_RE.match(line)
        if not m:
            continue
        for token in m.group(1).split():
            if len(token) != 2:
                continue
            try:
                out.append(int(token, 16))
            except ValueError:
                continue
    data = bytes(out)
    if expected_bytes is not None and len(data) != expected_bytes:
        raise ReadError(
            f"ethtool returned {len(data)} bytes, expected {expected_bytes}"
        )
    return data


def read_via_sysfs(eeprom_path: Path, page: int | None = None, length: int = 128, offset: int = 0) -> bytes:
    """Read EEPROM bytes via direct sysfs.

    The OPTOE driver exposes a single file at /sys/bus/i2c/devices/<bus>-0050/eeprom.
    Page selection on QSFP+/CMIS is driver-specific:
      - For optoe1 / optoe3 (QSFP+ / CMIS): the file presents a flat 32KB
        address space where offset = 128 + page * 128 reads upper page <page>
        bytes 0-127.
      - For new optoe-compat SFP: A0h is at offsets 0..255 and A2h
        at offsets 256..511 of the same 0050 file. Legacy drivers may
        instead expose a separate 0051 file, read at offset 0.

    Unbacked pages may return ENODATA or a short read; both raise ReadError.
    For QSFP+/CMIS lower memory, pass `page=None` and `offset=0`.
    For QSFP+/CMIS upper page N, pass `page=N` (computes offset internally).
    """
    if page is None:
        seek_offset = offset
    else:
        # Flat-offset paged model: lower at 0..127, upper page N at 128+N*128.
        seek_offset = 128 + page * 128 + offset
    try:
        with eeprom_path.open("rb") as f:
            f.seek(seek_offset)
            data = f.read(length)
    except OSError as e:
        raise ReadError(f"sysfs {eeprom_path} at offset {seek_offset}: {e}") from e
    if len(data) != length:
        raise ReadError(
            f"sysfs {eeprom_path} returned {len(data)} bytes at offset {seek_offset}, "
            f"expected {length} (driver may not support flat-offset paging)"
        )
    return data


def read_via_sfputil_pages(port: str) -> dict[int, bytes]:
    """Read all available pages via SONiC `sfputil show eeprom-pages -p <port>`.

    sfputil emits JSON-like output with hex values keyed by page id. Returns
    {page_id: 128-byte slice} where page_id 0 represents Lower memory
    (or A0h base for SFP). Vendor-specific output formats may vary; this
    parser accepts the common layout shipped on Broadcom / Mellanox / Marvell
    SONiC images.
    """
    out = _run(["sfputil", "show", "eeprom-pages", "-p", port])
    return _parse_sfputil_pages(out)


_SFPUTIL_PAGE_HEADER_RE = re.compile(r"Page\s+(0x[0-9a-fA-F]+|\d+|Lower)\s*:?", re.IGNORECASE)
_SFPUTIL_HEX_LINE_RE = re.compile(r"^\s*(?:0x)?([0-9a-fA-F]{2,4}):?\s*((?:[0-9a-fA-F]{2}\s*)+)")


def _parse_sfputil_pages(text: str) -> dict[int, bytes]:
    """Parse sfputil eeprom-pages output. The format is loosely standardized
    across SONiC platform plugins; we accept anything that has Page section
    headers and hex-byte lines."""
    pages: dict[int, bytearray] = {}
    current: int | None = None
    for line in text.splitlines():
        hdr = _SFPUTIL_PAGE_HEADER_RE.match(line)
        if hdr:
            tok = hdr.group(1).lower()
            if tok == "lower":
                current = -1  # sentinel for Lower memory
            else:
                current = int(tok, 0)
            pages.setdefault(current, bytearray())
            continue
        if current is None:
            continue
        m = _SFPUTIL_HEX_LINE_RE.match(line)
        if not m:
            continue
        for token in m.group(2).split():
            if len(token) == 2:
                try:
                    pages[current].append(int(token, 16))
                except ValueError:
                    continue
    out: dict[int, bytes] = {}
    for k, v in pages.items():
        if len(v) == 0:
            continue
        out[k] = bytes(v[:128])  # cap at 128 per page
    return out


# ---------- Top-level extraction logic ------------------------------------------


def detect_spec(lower_bytes: bytes) -> str:
    if not lower_bytes:
        raise ReadError("empty lower memory — cannot detect spec family")
    ident = lower_bytes[0]
    spec = IDENTIFIER_TO_SPEC.get(ident)
    if spec is None:
        raise ReadError(f"unknown identifier byte 0x{ident:02x} — cannot detect spec family")
    return spec


def extract_sfp(stem: Path, *, interface: str | None, sysfs_a0h: Path | None, sysfs_a2h: Path | None) -> list[Path]:
    """SFF-8472: dump A0h + A2h via a combined or legacy two-file window."""
    written: list[Path] = []

    # A0h
    if interface and _check_command_exists("ethtool"):
        try:
            a0h = read_via_ethtool_legacy(interface)[:256]
        except ReadError:
            a0h = b""
    elif sysfs_a0h:
        a0h = read_via_sysfs(sysfs_a0h, page=None, length=256, offset=0)
    else:
        raise ReadError("no input source for A0h — pass --interface or --sysfs-a0h")

    out = stem.with_suffix(".a0h.bin")
    out.write_bytes(a0h)
    written.append(out)

    # An explicit legacy 0051 path takes priority. Otherwise probe A2h in
    # the combined optoe-compat 0050 window (absence is reported by the read).
    a2h = b""
    try:
        if sysfs_a2h:
            a2h = read_via_sysfs(sysfs_a2h, length=256, offset=0)
        elif sysfs_a0h:
            a2h = read_via_sysfs(sysfs_a0h, length=256, offset=256)
        elif interface and _check_command_exists("ethtool"):
            # Best-effort legacy driver convention: page 1 denotes A2h.
            a2h = read_via_ethtool_paged(interface, page=1, offset=0, length=256)
    except ReadError as e:
        print(f"warning: A2h read failed ({e}); skipping A2h", file=sys.stderr)

    if a2h:
        out = stem.with_suffix(".a2h.bin")
        out.write_bytes(a2h)
        written.append(out)
    else:
        print(
            f"warning: no A2h memory captured for {stem.name}. "
            f"Use --sysfs-a0h <bus>-0050/eeprom for combined optoe-compat, "
            f"or --sysfs-a2h <bus>-0051/eeprom for legacy two-file drivers",
            file=sys.stderr,
        )

    return written


def extract_paged(
    stem: Path,
    *,
    spec: str,
    interface: str | None,
    sysfs: Path | None,
    sfputil_port: str | None,
) -> list[Path]:
    """SFF-8636 or CMIS: dump Lower (128B) + each upper page slice."""
    written: list[Path] = []

    pages_to_probe = SFF8636_PAGES if spec == "sff-8636" else CMIS_PROBE_PAGES

    # Try sfputil first (it returns all pages in one shot).
    if sfputil_port and _check_command_exists("sfputil"):
        try:
            sfputil_pages = read_via_sfputil_pages(sfputil_port)
            return _emit_paged_outputs(stem, sfputil_pages)
        except ReadError as e:
            print(f"info: sfputil failed ({e}); falling back to ethtool/sysfs", file=sys.stderr)

    # Otherwise: read Lower first, then each upper page.
    def _read_page(page: int | None) -> bytes:
        if interface and _check_command_exists("ethtool"):
            try:
                if page is None:
                    return read_via_ethtool_paged(interface, page=0, offset=0, length=128)
                return read_via_ethtool_paged(interface, page=page, offset=128, length=128)
            except ReadError as e_ethtool:
                if sysfs is None:
                    raise
                print(f"info: ethtool page {page} failed ({e_ethtool}); falling back to sysfs", file=sys.stderr)
        if sysfs:
            return read_via_sysfs(sysfs, page=page, length=128, offset=0)
        raise ReadError(f"no input source for page {page}")

    lower = _read_page(None)
    out = stem.with_suffix(".lower.bin")
    out.write_bytes(lower)
    written.append(out)

    for page in pages_to_probe:
        try:
            data = _read_page(page)
        except ReadError as e:
            print(f"info: skipping page 0x{page:02x} (not present): {e}", file=sys.stderr)
            continue
        if len(data) != 128:
            print(f"warning: page 0x{page:02x} returned {len(data)} bytes; skipping", file=sys.stderr)
            continue
        if data == b"\x00" * 128:
            print(f"info: page 0x{page:02x} all-zero; skipping (not advertised)", file=sys.stderr)
            continue
        out = stem.with_suffix(f".page-{page:02x}h.bin")
        out.write_bytes(data)
        written.append(out)

    return written


def _emit_paged_outputs(stem: Path, pages: dict[int, bytes]) -> list[Path]:
    """Helper: emit Lower + per-page .bin from a {page_id: bytes} dict."""
    written: list[Path] = []
    if -1 in pages:
        out = stem.with_suffix(".lower.bin")
        out.write_bytes(pages[-1])
        written.append(out)
    for page_id in sorted(p for p in pages if p >= 0):
        out = stem.with_suffix(f".page-{page_id:02x}h.bin")
        out.write_bytes(pages[page_id])
        written.append(out)
    return written


# ---------- Provenance template generation --------------------------------------


def _decode_ascii_field(data: bytes, start: int, end: int) -> str:
    """Decode an ASCII field that may be NUL- or space-padded."""
    if end > len(data):
        return "(short read)"
    raw = data[start:end]
    return raw.replace(b"\x00", b"").decode("ascii", errors="replace").strip()


def _decode_oui(data: bytes, start: int) -> str:
    if start + 3 > len(data):
        return "??:??:??"
    return ":".join(f"{b:02X}" for b in data[start : start + 3])


def render_provenance_md(
    stem: Path,
    spec: str,
    lower_bytes: bytes,
    page0_or_a0h: bytes,
    *,
    captured_from: dict[str, str | None],
    notes: str | None,
) -> str:
    """Generate a draft .md provenance template. The user MUST review and edit
    this file before submitting the fixture upstream — vendor S/N or date code
    may need redaction depending on whether the deployment is identifiable.
    """
    ident = lower_bytes[0] if lower_bytes else 0
    ident_label = IDENTIFIER_LABELS.get(ident, f"unknown(0x{ident:02x})")

    # Per-spec identity field offsets — SFF-8472, SFF-8636, and CMIS each have
    # a different Page 00h / A0h layout. Don't merge these branches.
    if spec == "sff-8472":
        # SFF-8472 v12.5a § 5 / Table 4-1 — A0h base ID
        vendor = _decode_ascii_field(page0_or_a0h, 20, 36)
        oui = _decode_oui(page0_or_a0h, 37)
        pn = _decode_ascii_field(page0_or_a0h, 40, 56)
        rev = _decode_ascii_field(page0_or_a0h, 56, 60)
        sn = _decode_ascii_field(page0_or_a0h, 68, 84)
        date = _decode_ascii_field(page0_or_a0h, 84, 92)
        compliance_rev = page0_or_a0h[94] if len(page0_or_a0h) > 94 else 0
        rev_byte = f"compliance byte 94 = 0x{compliance_rev:02x}"
        identity_rows = [
            ("Vendor name", vendor, "A0h:20-35"),
            ("Vendor OUI", oui, "A0h:37-39"),
            ("Vendor PN", pn, "A0h:40-55"),
            ("Vendor revision", rev, "A0h:56-59"),
            ("Vendor S/N", sn, "A0h:68-83"),
            ("Date code", date, "A0h:84-91"),
        ]
    elif spec == "sff-8636":
        # SFF-8636 v2.11 § 6.3 / Table 6-15 — Upper Page 00h
        rev_byte = (
            f"Lower:1 = 0x{lower_bytes[1]:02x} (compliance byte; see Table 6-3)"
        ) if len(lower_bytes) > 1 else "n/a"
        vendor = _decode_ascii_field(page0_or_a0h, 20, 36)
        oui = _decode_oui(page0_or_a0h, 37)
        pn = _decode_ascii_field(page0_or_a0h, 40, 56)
        rev = _decode_ascii_field(page0_or_a0h, 56, 58)
        sn = _decode_ascii_field(page0_or_a0h, 68, 84)
        date = _decode_ascii_field(page0_or_a0h, 84, 92)
        identity_rows = [
            ("Vendor name", vendor, "Page 00h:20-35"),
            ("Vendor OUI", oui, "Page 00h:37-39"),
            ("Vendor PN", pn, "Page 00h:40-55"),
            ("Vendor revision", rev, "Page 00h:56-57"),
            ("Vendor S/N", sn, "Page 00h:68-83"),
            ("Date code", date, "Page 00h:84-91"),
        ]
    else:
        # CMIS 5.3 § 8.3.1 / Table 8-19 — Page 00h Administrative
        rev_byte = (
            f"Lower:1 = 0x{lower_bytes[1]:02x} "
            f"({(lower_bytes[1] >> 4) & 0xF}.{lower_bytes[1] & 0xF})"
        ) if len(lower_bytes) > 1 else "n/a"
        vendor = _decode_ascii_field(page0_or_a0h, 1, 17)
        oui = _decode_oui(page0_or_a0h, 17)
        pn = _decode_ascii_field(page0_or_a0h, 20, 36)
        rev = _decode_ascii_field(page0_or_a0h, 36, 38)
        sn = _decode_ascii_field(page0_or_a0h, 38, 54)
        date = _decode_ascii_field(page0_or_a0h, 54, 62)
        identity_rows = [
            ("Vendor name", vendor, "Page 00h:1-16"),
            ("Vendor OUI", oui, "Page 00h:17-19"),
            ("Vendor PN", pn, "Page 00h:20-35"),
            ("Vendor revision", rev, "Page 00h:36-37"),
            ("Vendor S/N", sn, "Page 00h:38-53"),
            ("Date code", date, "Page 00h:54-61"),
        ]

    captured_from_str = (
        ", ".join(f"{k}={v}" for k, v in captured_from.items() if v)
        or "(unspecified — fill in)"
    )
    notes_str = (
        notes
        or "(fill in — what makes this fixture interesting? notable schema "
        "interactions exercised? edge cases?)"
    )

    identity_table = "\n".join(
        f"| {label} | `{value or '(empty)'}` | {source} |"
        for label, value, source in identity_rows
    )

    # Per-spec anonymization guidance — S/N + date offsets differ across specs,
    # and the checksum that covers the S/N + date region is CC_EXT (not CC_BASE)
    # for both SFF-8472 and SFF-8636. CC_BASE only covers the base-ID region
    # (vendor name / OUI / PN / revision); the S/N + date code live in the
    # CC_EXT-covered extended region.
    if spec == "sff-8472":
        redaction_targets = (
            "vendor S/N at A0h:68-83 + date code at A0h:84-91, then recompute "
            "CC_EXT at A0h:95 (low-order 8 bits of sum of A0h:64-94, per "
            "SFF-8472 v12.5a § 8.12)"
        )
    elif spec == "sff-8636":
        redaction_targets = (
            "vendor S/N at Page 00h:68-83 + date code at Page 00h:84-91, then "
            "recompute CC_EXT at Page 00h:95 (low-order 8 bits of sum of "
            "Page 00h:64-94, per SFF-8636 v2.11 § 6.3.29)"
        )
    else:
        redaction_targets = (
            "vendor S/N at Page 00h:38-53 + date code at Page 00h:54-61 "
            "(CMIS does not checksum identity bytes, so no recomputation needed)"
        )

    return f"""\
# {stem.name}

> **DRAFT — REVIEW BEFORE SUBMITTING UPSTREAM.** This file is auto-generated
> by `tools/optoe-to-bin.py`. Verify license compatibility, edit anonymization
> section if vendor S/N or date code could identify the source deployment, and
> add the per-fixture notes that motivate the submission.

## Module identification

| Field | Value | Source |
|------|-------|--------|
| Identifier (Lower:0) | `0x{ident:02x}` ({ident_label}) | SFF-8024 Table 4-1 |
| Spec family | `{spec}` | byte-0 detection |
| Revision | {rev_byte} | byte 1 |
{identity_table}

## Pages included

See sibling `.bin` files in the same directory.

## Source

- **Captured by**: `tools/optoe-to-bin.py`
- **Captured from**: {captured_from_str}
- **Capture date**: (fill in capture date)
- **Switch / host**: (fill in switch model + SONiC version, or other Linux host details)
- **Firmware revision**: (fill in module firmware revision if known — useful for cross-fixture comparisons)

## License

- **Submitter's right to redistribute**: (fill in — vendor permission, retired hardware, test-bench module never in production, etc.)
- **Proposed license**: (Apache-2.0 / MIT / CC0 / public domain — match what the openxcvr project accepts per `tests/fixtures/README.md`)

## Anonymization

Vendor S/N and date code can identify a specific deployment. Decide whether to:

- **Preserve verbatim** (typical for retired hardware or vendor-supplied test modules).
- **Redact**: zero out {redaction_targets}.

Note any redaction here: (fill in — what was redacted and why)

## Notes

{notes_str}
"""


# ---------- CLI ----------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--stem", type=Path, required=True, help="output path stem (e.g., /tmp/myswitch-port-0)")
    src = p.add_argument_group("input source (choose at least one)")
    src.add_argument("--interface", help="network interface (e.g., Ethernet0); used with `ethtool`")
    src.add_argument("--sysfs", type=Path, help="OPTOE sysfs eeprom path (QSFP+/CMIS)")
    src.add_argument(
        "--sysfs-a0h",
        type=Path,
        help="SFP sysfs path (0050; A2h auto-read at offset 256 for combined optoe-compat)",
    )
    src.add_argument(
        "--sysfs-a2h",
        type=Path,
        help="Legacy separate SFP A2h sysfs path (0051; read at offset 0)",
    )
    src.add_argument("--sfputil-port", help="SONiC sfputil port (last resort)")
    p.add_argument(
        "--spec",
        choices=["auto", "sff-8472", "sff-8636", "cmis"],
        default="auto",
        help="force a spec family (default: auto-detect from byte 0)",
    )
    p.add_argument(
        "--no-md",
        action="store_true",
        help="skip generating the .md provenance template",
    )
    p.add_argument(
        "--notes",
        help="freeform notes inserted into the .md provenance template",
    )
    args = p.parse_args(argv)

    if not (args.interface or args.sysfs or args.sysfs_a0h or args.sfputil_port):
        p.error("at least one input source required (--interface / --sysfs / --sysfs-a0h / --sfputil-port)")

    # First read: detect spec by reading 1 byte (Lower:0 or A0h:0).
    # Strategy: read enough of the lower memory / base ID to identify the spec.
    if args.spec == "auto":
        try:
            if args.interface and _check_command_exists("ethtool"):
                first = read_via_ethtool_legacy(args.interface)[:128]
            elif args.sysfs:
                first = read_via_sysfs(args.sysfs, page=None, length=128, offset=0)
            elif args.sysfs_a0h:
                first = read_via_sysfs(args.sysfs_a0h, page=None, length=128, offset=0)
            elif args.sfputil_port:
                pages = read_via_sfputil_pages(args.sfputil_port)
                first = pages.get(-1) or pages.get(0) or b""
            else:
                p.error("auto-detect requires at least one input source")
                return 2
        except ReadError as e:
            print(f"error: cannot read for spec auto-detection: {e}", file=sys.stderr)
            return 1
        try:
            spec = detect_spec(first)
        except ReadError as e:
            print(f"error: {e}", file=sys.stderr)
            return 1
    else:
        spec = args.spec
        first = b""

    print(f"info: detected spec family = {spec}", file=sys.stderr)

    if spec == "sff-8472":
        if not (args.interface or args.sysfs_a0h):
            p.error("SFF-8472 requires --interface or --sysfs-a0h")
        written = extract_sfp(
            args.stem,
            interface=args.interface,
            sysfs_a0h=args.sysfs_a0h,
            sysfs_a2h=args.sysfs_a2h,
        )
    else:
        # SFF-8636 / CMIS
        sysfs_path = args.sysfs or args.sysfs_a0h
        written = extract_paged(
            args.stem,
            spec=spec,
            interface=args.interface,
            sysfs=sysfs_path,
            sfputil_port=args.sfputil_port,
        )

    for out in written:
        print(f"wrote {out} ({out.stat().st_size} bytes)")

    # Provenance template
    if not args.no_md and written:
        # Read back lower + page0/A0h for identity decoding
        lower_path = args.stem.with_suffix(".lower.bin")
        a0h_path = args.stem.with_suffix(".a0h.bin")
        page0_path = args.stem.with_suffix(".page-00h.bin")
        if spec == "sff-8472" and a0h_path.exists():
            lower_bytes = a0h_path.read_bytes()  # for SFP, lower==A0h base ID
            page0 = lower_bytes
        elif lower_path.exists() and page0_path.exists():
            lower_bytes = lower_path.read_bytes()
            page0 = page0_path.read_bytes()
        else:
            lower_bytes = first or b""
            page0 = b""

        captured_from = {
            "interface": args.interface,
            "sysfs": str(args.sysfs) if args.sysfs else None,
            "sysfs_a0h": str(args.sysfs_a0h) if args.sysfs_a0h else None,
            "sysfs_a2h": str(args.sysfs_a2h) if args.sysfs_a2h else None,
            "sfputil_port": args.sfputil_port,
        }

        md = render_provenance_md(
            args.stem,
            spec,
            lower_bytes,
            page0,
            captured_from=captured_from,
            notes=args.notes,
        )
        md_path = args.stem.with_suffix(".md")
        md_path.write_text(md)
        print(f"wrote {md_path} (draft — review before submitting upstream)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
