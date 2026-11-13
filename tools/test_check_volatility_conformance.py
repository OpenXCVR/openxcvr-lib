#!/usr/bin/env python3
"""Unit tests for tools/check-volatility-conformance.py.

Runs completely on synthetic standard snippets and synthetic schemas, NEVER
on the gitignored parsed standards: the real-standards run is the local
tool's job and is reported as evidence, not committed.

Every error class the task prompt names has a synthetic failing test that
pins its mismatch kind, and each family is driven through the real
--write / --check / receipt-hash path.
"""

from __future__ import annotations

import contextlib
import copy
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

_TOOL_PATH = Path(__file__).resolve().parent / "check-volatility-conformance.py"
_SPEC = importlib.util.spec_from_file_location("check_volatility_conformance", _TOOL_PATH)
cvc = importlib.util.module_from_spec(_SPEC)
sys.modules["check_volatility_conformance"] = cvc
_SPEC.loader.exec_module(cvc)

CMIS_REVS = ("3.0", "4.0", "5.0", "5.1", "5.2", "5.3")


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _run_cli(*argv: str) -> int:
    """Run the CLI with stdout/stderr captured; return the exit code."""
    buf_out, buf_err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
        try:
            code = cvc.main(list(argv))
        except SystemExit as e:  # argparse errors exit 2
            code = int(e.code or 0)
    return code


class SyntheticFixtures:
    """Builders for the synthetic standards, schemas and authority reports.

    The synthetic standards mirror the parsed-markdown formats of the real
    corpus (markdown tables with Byte/Bits/Name/Description/Type columns,
    SFF-8636 support columns and writable-block tables, SFF-8472 byte
    continuation rows), so the tests exercise the real parsing paths.
    """

    # ------------------------------------------------------------------
    # Synthetic parsed standards
    # ------------------------------------------------------------------

    @staticmethod
    def cmis_standard(rev: str) -> str:
        if rev == "3.0":
            reserved_prose = (
                "Reserved bytes are for future use and shall not be used and "
                "shall be set to 0."
            )
            sec = "1.7"
            ro = "RO RQD"
            flag_ro = "RO Opt."
            flag_reserved_type = "RQD"
            flags_12_13_prose = (
                "Byte 12 is reserved for future Module level flags. "
                "Byte 13 is provided for Custom Module Level Flags."
            )
            rows_12_13 = ""
            flag_name = "L-{n}"
            flag_desc = "Latched flag {n}"
            access_types_table = ""
        elif rev == "4.0":
            reserved_prose = (
                "Reserved locations (bytes, fields, or bits) are for future use "
                "and shall neither be used (evaluated) nor modified. The module "
                "shall zero-initialize reserved locations. There are no other "
                "obligations for the module."
            )
            sec = "8"
            ro = "RO RQD"
            flag_ro = "RO Opt."
            flag_reserved_type = "RQD"
            flags_12_13_prose = ""
            rows_12_13 = (
                "| 12 | 7-0 | - | Reserved[1] |  |\n"
                "| 13 | 7-0 | - | Custom[1] |  |\n"
            )
            flag_name = "{n}"
            flag_desc = "Latched flag {n}. Clear on Read"
            access_types_table = ""
        else:
            reserved_prose = (
                "Locations within a Page marked as reserved are also reserved "
                "for future use. However, such locations are accessible, albeit "
                "without associated functionality. The module shall "
                "zero-initialize accessible reserved locations. There are no "
                "other obligations for the module."
            )
            sec = "8"
            ro = "RO Rqd."
            flag_ro = "RO/COR Adv."
            flag_reserved_type = "Rqd."
            flags_12_13_prose = ""
            rows_12_13 = (
                "| 12 | 7-0 | - | Reserved[1] |  |\n"
                "| 13 | 7-0 | - | Custom[1] |  |\n"
            )
            flag_name = "{n}"
            flag_desc = "Latched flag {n}"
            access_types_table = (
                "\n## 8.1.3.6 Access Types\n\n"
                "## Table 8-3 Access Types\n\n"
                "| Access Type | Description |\n"
                "|---|---|\n"
                "| RO/COR | A read-only element with clear-on-read "
                "(clear-after-read) side effect. All bits in a RO/COR Byte are "
                "cleared by the module after the Byte value has been read. |\n"
                "\nSide effects of a READ (such as e.g. clear-on-read) may be "
                "specified for certain Bytes. Such side effects are not "
                "guaranteed to be completed at the time when the READ is "
                "finished.\n"
            )

        latched_prose = (
            "Once asserted, the bits remain set (latched) until cleared by a "
            "read operation that includes the affected bit or reset by the "
            "Reset signal."
        ) if rev in ("3.0", "4.0") else ""

        flag_rows = (
            f"| 8 | 7 | {flag_name.format(n='CdbCmdCompleteFlag2')} | {flag_desc.format(n='CdbCmdCompleteFlag2')} | {flag_ro} |\n"
            f"| 8 | 6 | {flag_name.format(n='CdbCmdCompleteFlag1')} | {flag_desc.format(n='CdbCmdCompleteFlag1')} | {flag_ro} |\n"
            f"| 8 | 5-3 | - | Reserved | {flag_reserved_type} |\n"
            f"| 8 | 2 | {flag_name.format(n='DataPathFirmwareErrorFlag')} | {flag_desc.format(n='DataPathFirmwareErrorFlag')} | {flag_ro} |\n"
            f"| 8 | 1 | {flag_name.format(n='ModuleFirmwareErrorFlag')} | {flag_desc.format(n='ModuleFirmwareErrorFlag')} | {flag_ro} |\n"
            f"| 8 | 0 | {flag_name.format(n='ModuleStateChangedFlag')} | {flag_desc.format(n='ModuleStateChangedFlag')} | {flag_ro} |\n"
            f"| 9 | 7-0 | {flag_name.format(n='MonitorFlags')} | {flag_desc.format(n='MonitorFlags')} | {flag_ro} |\n"
            f"| 10 | 7-0 | {flag_name.format(n='MonitorFlags2')} | {flag_desc.format(n='MonitorFlags2')} | {flag_ro} |\n"
            f"| 11 | 7-0 | {flag_name.format(n='MonitorFlags3')} | {flag_desc.format(n='MonitorFlags3')} | {flag_ro} |\n"
        )

        return f"""# Synthetic CMIS {rev}

## {sec}.2 Lower Memory (Control and Status Essentials)

{reserved_prose}

{latched_prose}

{flags_12_13_prose}

## Table 8-6 Global Status Information (Lower Memory)

| Byte | Bits | Field Name | Field Description | Type |
|---|---|---|---|---|
| 0 | 7-0 | Identifier | Type of module | {ro} |
| 3 | 7-4 | - | Reserved |  |
| 3 | 3-1 | ModuleState | Current Module State | {ro} |
| 3 | 0 | InterruptDeasserted | Status of Interrupt output signal | {ro} |
| 14 | 7-0 | Temperature MSB | Internally measured temperature MSB | {ro} |
| 15 | 7-0 | Temperature LSB | Internally measured temperature LSB | {ro} |
| 26 | 7-0 | LowPwrControl | Low power control | RW Rqd. |

## Table 8-9 Module Flags (paged memory modules only) (Lower Memory)

| Byte | Bit | Field Name | Field Description | Type |
|---|---|---|---|---|
{flag_rows}{rows_12_13}
{access_types_table}
"""

    @staticmethod
    def sff8636_standard() -> str:
        return """# Synthetic SFF-8636 Rev 2.12

## 5.5 Write Operation Restrictions

Table 5-3  Writable Memory Blocks

| Byte | # Bytes | Operation | Description |
|---|---|---|---|
| Page 00h |  |  | Volatile |
| 86 | 1 | Read/Write | Control Register |
| 119-122 | 4 | Write-Only | Password Change Entry Area (Optional) |
| 127 | 1 | Read/Write | Page Select Byte |

Note: Reserved memory locations are to be filled with logic zeros in all bit locations for reserved bytes, and in reserved bit locations for partially specified byte locations.

## 6.2 Lower Page 00h

Table 6-2  Lower Page 00h Memory Map

| Byte | Description | Type | PC | AC | AO | SM |
|---|---|---|---|---|---|---|
| 0 | Identifier | Read-Only | R | R | R | R |
| 1-2 | Status | Read-Only | See Table 6-3 | See Table 6-3 | See Table 6-3 | See Table 6-3 |
| 3-21 | Interrupt Flags | Read-Only | See Table 6-5 | See Table 6-5 | See Table 6-5 | See Table 6-5 |
| 86 | Control | Read/Write | O | O | O | O |
| 119-122 | Password | Write-Only | O | O | O | O |

## 6.2.2 Status Indicators (Page 00h, Bytes 1-2)

Table 6-3  Status Indicators (Page 00h Bytes 1-2)

| Byte | Bit | Name | Description | PC | AC | AO | SM |
|---|---|---|---|---|---|---|---|
| 1 | All | Revision Compliance | See Table 6-4. | R | R | R | R |
| 2 | 7-4 | Reserved | Module State Code - reserved for microQSFP MSA. | - | - | - | - |
| 2 | 3 | Reserved |  | - | - | - | - |
| 2 | 2 | Flat_mem | Upper memory flat or paged. | R | R | R | R |
| 2 | 1 | IntL | Digital state of the IntL Interrupt output pin. | R | R | R | R |
| 2 | 0 | Data_Not_Ready | Indicates free-side does not yet have valid monitor data. | R | R | R | R |

The IntL bit will remain asserted until a read is performed of the Data_Not_Ready bit (Byte 2).

## 6.2.3 Interrupt Flags (Page 00h, Bytes 3-21)

Bytes 3-21 consist of interrupt flags for LOS, Tx Fault, warnings and alarms. The non-asserted state shall be 0b. The flag bit shall remain set until the fixed-side performs a read operation of the bit or the free side is reset.

Table 6-5  Channel Status Interrupt Flags (Page 00h Bytes 3-5)

| Byte | Bit | Name | Description | PC | AC | AO | SM |
|---|---|---|---|---|---|---|---|
| 3 | 7 | L-Tx4 LOS | Latched Tx4 LOS indicator | O | O | O | O |
| 3 | 6 | L-Tx3 LOS | Latched Tx3 LOS indicator | O | O | O | O |
| 3 | 5 | L-Tx2 LOS | Latched Tx2 LOS indicator | O | O | O | O |
| 3 | 4 | L-Tx1 LOS | Latched Tx1 LOS indicator | O | O | O | O |
| 3 | 3-0 | Reserved |  | - | - | - | - |

## 6.2.6 Control Functions (Page 00h, Bytes 86-99)

Table 6-9  Control Functions

| Byte | Bit | Name | Description | PC | AC | AO | SM |
|---|---|---|---|---|---|---|---|
| 86 | 7-0 | Soft RS(1) Select | Soft rate select control | O | O | O | O |

## 6.2.10 Password Entry and Change (Page 00h, Bytes 119-126)

Table 6-11  Password Entry Area

| Byte | Bit | Name | Description | PC | AC | AO | SM |
|---|---|---|---|---|---|---|---|
| 119 | 7-0 | Password | Password entry | O | O | O | O |
"""

    @staticmethod
    def sff8472_standard() -> str:
        return """# Synthetic SFF-8472 Rev 12.5a

## 8.10 Enhanced Options [Address A0h, Byte 93]

Bits 3 and 6 of byte 110 (see Table 9-16) allow the host to control the
Rate_Select and TX_Disable functions. If these functions are not implemented,
the bits remain readable and writable, but the transceiver ignores them.

## 9.8 Real Time Diagnostic and Control Registers [Address A2h, Bytes 96-111]

## Table 9-16  A/D Values and Status Bits

| A2h | Bit | Name | Description |
|---|---|---|---|
| 110 | 7 | TX Disable State | Digital state of the TX Disable Input Pin. |
|  | 6 | Soft TX Disable Select | Read/write bit that allows software disable of laser. |
|  | 5 | RS(1) State | Digital state of SFP input pin AS(1). |
|  | 4 | Rate_Select State | Digital state of the module's Rate_Select Input Pin. |
|  | 3 | Soft Rate_Select Select | Read/write bit that allows software rate select control. |
|  | 2 | TX Fault State | Digital state of the TX Fault Output Pin. |
|  | 1 | Rx_LOS State | Digital state of the RX_LOS Output Pin. |
|  | 0 | Data_Not_Ready | Indicates module does not yet have valid monitor data. |
| 111 | 7-0 | Reserved | Reserved (was assigned to SFF-8079). |

## 9.9 Alarm and Warning Flag Bits [Address A2h, Bytes 112, 113, 116, 117]

Bytes 112, 113, 116 and 117 contain an optional set of alarm and warning
flags. The flags may be latched or nonlatched. Implementation is vendor
specific, and the Vendor's specification sheet should be consulted.

Table 9-17  Alarm Flags

| A2h | Bit | Name | Description |
|---|---|---|---|
| 112 | 7 | Temp High Alarm | High temperature alarm flag |
| 112 | 6 | Temp Low Alarm | Low temperature alarm flag |
| 112 | 5 | Vcc High Alarm | High supply voltage alarm flag |
| 112 | 4 | Vcc Low Alarm | Low supply voltage alarm flag |
| 112 | 3 | Temp High Warning | High temperature warning flag |
| 112 | 2 | Temp Low Warning | Low temperature warning flag |
| 112 | 1 | Vcc High Warning | High supply voltage warning flag |
| 112 | 0 | Vcc Low Warning | Low supply voltage warning flag |
| 113 | 7 | Aux1 High Alarm | High Aux1 alarm flag |
| 113 | 6 | Aux1 Low Alarm | Low Aux1 alarm flag |
| 113 | 5 | Aux1 High Warning | High Aux1 warning flag |
| 113 | 4 | Aux1 Low Warning | Low Aux1 warning flag |
| 113 | 3 | Aux2 High Alarm | High Aux2 alarm flag |
| 113 | 2 | Aux2 Low Alarm | Low Aux2 alarm flag |
| 113 | 1 | Aux2 High Warning | High Aux2 warning flag |
| 113 | 0 | Aux2 Low Warning | Low Aux2 warning flag |

Table 9-18  Warning Flags

| A2h | Bit | Name | Description |
|---|---|---|---|
| 116 | 7 | Temp High Warning | High temperature warning flag |
| 116 | 6 | Temp Low Warning | Low temperature warning flag |
| 116 | 5 | Vcc High Warning | High supply voltage warning flag |
| 116 | 4 | Vcc Low Warning | Low supply voltage warning flag |
| 116 | 3 | Bias High Warning | High bias current warning flag |
| 116 | 2 | Bias Low Warning | Low bias current warning flag |
| 116 | 1 | TX Power High Warning | High TX power warning flag |
| 116 | 0 | TX Power Low Warning | Low TX power warning flag |
| 117 | 7 | Aux1 High Warning | High Aux1 warning flag |
| 117 | 6 | Aux1 Low Warning | Low Aux1 warning flag |
| 117 | 5 | Aux2 High Warning | High Aux2 warning flag |
| 117 | 4 | Aux2 Low Warning | Low Aux2 warning flag |
| 117 | 3 | Reserved |  |
| 117 | 2 | Reserved |  |
| 117 | 1 | Reserved |  |
| 117 | 0 | Reserved |  |
"""

    # ------------------------------------------------------------------
    # Synthetic authority reports (mirroring the completed reports' shape)
    # ------------------------------------------------------------------

    @staticmethod
    def cmis_authority_report() -> dict:
        rows = []
        for rev in CMIS_REVS:
            rows.append({
                "revision": rev,
                "document": f"CMIS {rev}",
                "clause_or_table": "reserved-location convention",
                "exact_passage": (
                    "Reserved bytes are for future use and shall not be used and "
                    "shall be set to 0."
                    if rev == "3.0" else
                    "Reserved locations (bytes, fields, or bits) are for future use"
                    if rev == "4.0" else
                    "such locations are accessible, albeit without associated functionality"
                ),
            })
        rows.append({
            "revision": "3.0/4.0",
            "document": "CMIS 3.0/4.0",
            "clause_or_table": "latched-flag prose",
            "exact_passage": (
                "until cleared by a read operation that includes the affected bit"
            ),
        })
        rows.append({
            "revision": "5.x",
            "document": "CMIS 5.x",
            "clause_or_table": "Table 8-3 Access Types",
            "exact_passage": (
                "A read-only element with clear-on-read (clear-after-read) side effect"
            ),
        })
        return {"kind": "report", "key": "cmis-byte3", "result": {
            "revision_evidence": rows,
        }}

    @staticmethod
    def sff8636_authority_report() -> dict:
        return {"kind": "report", "key": "sff8636-byte2", "result": {
            "exact_evidence": [
                {
                    "document": "SFF-8636 Rev 2.12",
                    "clause_or_table": "§6.2.2 prose after Table 6-3",
                    "exact_passage": (
                        "The IntL bit will remain asserted until a read is performed "
                        "of the Data_Not_Ready bit (Byte 2)."
                    ),
                },
                {
                    "document": "SFF-8636 Rev 2.12",
                    "clause_or_table": "§6.2.3 prose",
                    "exact_passage": (
                        "The flag bit shall remain set until the fixed-side performs a "
                        "read operation of the bit"
                    ),
                },
                {
                    "document": "SFF-8636 Rev 2.12",
                    "clause_or_table": "§6.1 note",
                    "exact_passage": (
                        "Reserved memory locations are to be filled with logic zeros"
                    ),
                },
            ],
        }}

    @staticmethod
    def sff8472_authority_report() -> dict:
        return {"kind": "report", "key": "sff8472-byte110", "result": {
            "exact_evidence": [
                {
                    "document": "SFF-8472 Rev 12.5a",
                    "clause_or_table": "§8.10 Enhanced Options prose",
                    "exact_passage": (
                        "the bits remain readable and writable, but the transceiver "
                        "ignores them"
                    ),
                },
                {
                    "document": "SFF-8472 Rev 12.5a",
                    "clause_or_table": "§9.9 prose",
                    "exact_passage": "The flags may be latched or nonlatched.",
                },
            ],
        }}

    # ------------------------------------------------------------------
    # Synthetic schemas
    # ------------------------------------------------------------------

    @staticmethod
    def cmis_schema(
        root: Path,
        *,
        byte3_reserved_access: str = "unknown",
        include_byte3_reserved: bool = True,
    ) -> None:
        """Write a synthetic CMIS schema tree.

        byte3_reserved_access controls the marking of lower byte 3 bits 4-7
        ("unknown" is the D1-honest marking for CMIS 3.0; "ro" only conforms
        for 4.0-5.3 via the reserved-location interpretation).
        """
        sff8024 = root / "schemas" / "sff-8024"
        _write(sff8024 / "_index.yaml", """schema: openxcvr.schema_index.v1
spec:
  id: sff-8024
  revision: "4.13.2"
templates:
  inherited_ro:
    access: ro
    flags:
      is_dynamic: false
""")

        cmis = root / "schemas" / "cmis"
        _write(cmis / "_index.yaml", """schema: openxcvr.schema_index.v1
spec:
  id: cmis
  revision: "5.3"
imports:
  - ../sff-8024/_index.yaml
kind: memory_map
includes:
  - lower-memory.yaml
templates:
  status_ro:
    access: ro
    flags:
      is_dynamic: true
  reserved_templ:
    access: unknown
    flags:
      is_dynamic: false
""")
        _write(cmis / "lower-memory.yaml", """schema: openxcvr.page.v1
kind: page
id: CmisLowerMemory
spec:
  id: cmis
page_address: lower
size_bytes: 128
field_includes:
  - fields/lower-memory-part-01.yaml
fields: []
""")

        fields = f"""fields:
  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
  - name: interrupt_deasserted
    offset: 3
    bit_offset: 0
    bit_width: 1
    template: status_ro
  - name: module_state
    offset: 3
    bit_offset: 1
    bit_width: 3
    template: status_ro
"""
        if include_byte3_reserved:
            fields += f"""  - name: reserved_byte3_bits_4_7
    offset: 3
    bit_offset: 4
    bit_width: 4
    access: {byte3_reserved_access}
    flags:
      is_dynamic: false
"""
        fields += """  - name: module_state_changed_flag
    offset: 8
    bit_offset: 0
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: module_firmware_error_flag
    offset: 8
    bit_offset: 1
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: data_path_firmware_error_flag
    offset: 8
    bit_offset: 2
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: reserved_byte8_bits_5_3
    offset: 8
    bit_offset: 3
    bit_width: 3
    template: reserved_templ
  - name: cdb_cmd_complete_flag1
    offset: 8
    bit_offset: 6
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: cdb_cmd_complete_flag2
    offset: 8
    bit_offset: 7
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: monitor_flags_1
    offset: 9
    size_bytes: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: monitor_flags_2
    offset: 10
    size_bytes: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: monitor_flags_3
    offset: 11
    size_bytes: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: reserved_byte12
    offset: 12
    size_bytes: 1
    template: reserved_templ
  - name: custom_byte13
    offset: 13
    size_bytes: 1
    access: unknown
    flags:
      is_dynamic: false
  - name: temperature
    offset: 14
    size_bytes: 2
    template: status_ro
  - name: low_pwr_control
    offset: 26
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
"""
        _write(cmis / "fields" / "lower-memory-part-01.yaml", fields)

    @staticmethod
    def sff8636_schema(
        root: Path,
        *,
        revision: str = "2.12",
        byte2_reserved_access: str = "unknown",
        include_byte2_reserved: bool = True,
    ) -> None:
        sff = root / "schemas" / "sff-8636"
        _write(sff / "_index.yaml", f"""schema: openxcvr.schema_index.v1
spec:
  id: sff-8636
  revision: "{revision}"
kind: memory_map
includes:
  - lower-memory.yaml
templates:
  status_ro:
    access: ro
    flags:
      is_dynamic: true
  reserved_templ:
    access: unknown
    flags:
      is_dynamic: false
""")
        _write(sff / "lower-memory.yaml", """schema: openxcvr.page.v1
kind: page
id: Sff8636LowerMemory
spec:
  id: sff-8636
page_address: lower
size_bytes: 128
field_includes:
  - fields/lower-memory-part-01.yaml
fields: []
""")
        fields = """fields:
  - name: revision_compliance
    offset: 1
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
  - name: data_not_ready
    offset: 2
    bit_offset: 0
    bit_width: 1
    template: status_ro
  - name: interrupt_deasserted
    offset: 2
    bit_offset: 1
    bit_width: 1
    template: status_ro
  - name: flat_memory
    offset: 2
    bit_offset: 2
    bit_width: 1
    access: ro
    flags:
      is_dynamic: false
"""
        if include_byte2_reserved:
            fields += f"""  - name: reserved_byte2_bits_3_7
    offset: 2
    bit_offset: 3
    bit_width: 5
    access: {byte2_reserved_access}
    flags:
      is_dynamic: false
"""
        fields += """  - name: tx4_los_flag
    offset: 3
    bit_offset: 7
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: tx3_los_flag
    offset: 3
    bit_offset: 6
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: tx2_los_flag
    offset: 3
    bit_offset: 5
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: tx1_los_flag
    offset: 3
    bit_offset: 4
    bit_width: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: reserved_byte3_bits_3_0
    offset: 3
    bit_offset: 0
    bit_width: 4
    template: reserved_templ
  - name: soft_rs1_select
    offset: 86
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
  - name: password_entry
    offset: 119
    size_bytes: 1
    access: wo
    flags:
      is_dynamic: false
"""
        _write(sff / "fields" / "lower-memory-part-01.yaml", fields)

    @staticmethod
    def sff8472_schema(root: Path, *, whole_byte_110: bool = False) -> None:
        sff = root / "schemas" / "sff-8472"
        _write(sff / "_index.yaml", """schema: openxcvr.schema_index.v1
spec:
  id: sff-8472
  revision: "12.5a"
kind: memory_map
includes:
  - page-a2h-diagnostics-base.yaml
templates:
  status_ro:
    access: ro
    flags:
      is_dynamic: true
  reserved_templ:
    access: unknown
    flags:
      is_dynamic: false
""")
        _write(sff / "page-a2h-diagnostics-base.yaml", """schema: openxcvr.page.v1
kind: page
id: page_a2h_diagnostics_base
spec:
  id: sff-8472
page_address: A2h
size_bytes: 128
field_includes:
  - fields/page-a2h-diagnostics-base-part-01.yaml
fields: []
""")
        if whole_byte_110:
            byte110 = """  - name: status_control
    offset: 110
    size_bytes: 1
    template: status_ro
"""
        else:
            byte110 = """  - name: tx_disable_state
    offset: 110
    bit_offset: 7
    bit_width: 1
    template: status_ro
  - name: soft_tx_disable_select
    offset: 110
    bit_offset: 6
    bit_width: 1
    access: rw
    flags:
      is_dynamic: false
  - name: rs1_state
    offset: 110
    bit_offset: 5
    bit_width: 1
    template: status_ro
  - name: rate_select_state
    offset: 110
    bit_offset: 4
    bit_width: 1
    template: status_ro
  - name: soft_rate_select_select
    offset: 110
    bit_offset: 3
    bit_width: 1
    access: rw
    flags:
      is_dynamic: false
  - name: tx_fault_state
    offset: 110
    bit_offset: 2
    bit_width: 1
    template: status_ro
  - name: rx_los_state
    offset: 110
    bit_offset: 1
    bit_width: 1
    template: status_ro
  - name: data_not_ready
    offset: 110
    bit_offset: 0
    bit_width: 1
    template: status_ro
"""
        fields = byte110 + """  - name: reserved_a2h_111
    offset: 111
    size_bytes: 1
    template: reserved_templ
  - name: temp_high_alarm
    offset: 112
    bit_offset: 7
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: temp_low_alarm
    offset: 112
    bit_offset: 6
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: vcc_high_alarm
    offset: 112
    bit_offset: 5
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: vcc_low_alarm
    offset: 112
    bit_offset: 4
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: temp_high_warning
    offset: 112
    bit_offset: 3
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: temp_low_warning
    offset: 112
    bit_offset: 2
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: vcc_high_warning
    offset: 112
    bit_offset: 1
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
  - name: vcc_low_warning
    offset: 112
    bit_offset: 0
    bit_width: 1
    access: unknown
    flags:
      is_dynamic: true
"""
        # Bytes 113/116/117 mirror byte 112: per-bit Unknown markings for the
        # latched alarm/warning flag bytes the comparison expands.
        for off, prefix in ((113, "alarm113"), (116, "warn116"), (117, "warn117")):
            for bit in range(8):
                fields += (
                    f"  - name: {prefix}_bit{bit}\n"
                    f"    offset: {off}\n"
                    f"    bit_offset: {bit}\n"
                    "    bit_width: 1\n"
                    "    access: unknown\n"
                    "    flags:\n"
                    "      is_dynamic: true\n"
                )
        _write(sff / "fields" / "page-a2h-diagnostics-base-part-01.yaml", "fields:\n" + fields)


class ConformanceTestBase(unittest.TestCase):
    """Shared fixture setup: synthetic standards, schemas and authority reports."""

    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.schemas_dir = self.root / "schemas"
        self.standards_dir = self.root / "standards"
        self.receipts_dir = self.root / "receipts"
        self.authority_dir = self.root / "authority"

        self.schemas_dir.mkdir(parents=True)
        self.standards_dir.mkdir(parents=True)
        self.receipts_dir.mkdir(parents=True)
        self.authority_dir.mkdir(parents=True)

        self._write_all_standards()
        self._write_all_authority_reports()

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    # -- fixture mutation helpers -----------------------------------------

    def _rewrite_standard(self, family: str, rev: str, old: str, new: str) -> None:
        """Rewrite part of a synthetic standard file (content perturbation)."""
        path = self.standards_dir / cvc.STANDARD_FILENAMES[(family, rev)]
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def _restore_standard(self, family: str, rev: str) -> None:
        """Restore one synthetic standard file from the fixture."""
        if family == "cmis":
            text = SyntheticFixtures.cmis_standard(rev)
        elif family == "sff-8636":
            text = SyntheticFixtures.sff8636_standard()
        else:
            text = SyntheticFixtures.sff8472_standard()
        _write(self.standards_dir / cvc.STANDARD_FILENAMES[(family, rev)], text)

    def _fields_file(self, family: str) -> Path:
        return {
            "cmis": self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml",
            "sff-8636": self.schemas_dir / "sff-8636" / "fields" / "lower-memory-part-01.yaml",
            "sff-8472": self.schemas_dir / "sff-8472" / "fields" / "page-a2h-diagnostics-base-part-01.yaml",
        }[family]

    def _replace_in_fields(self, family: str, old: str, new: str) -> None:
        path = self._fields_file(family)
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def _drop_fields(self, family: str, drop: set[tuple[str, int]]) -> None:
        """Remove fields by (name, offset) from a synthetic fields file."""
        path = self._fields_file(family)
        data = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
        kept = [
            f for f in (data.get("fields") or [])
            if (f.get("name"), f.get("offset")) not in drop
        ]
        self.assertEqual(len(kept), len(data.get("fields") or []) - len(drop))
        data["fields"] = kept
        path.write_text(yaml.safe_dump(data, sort_keys=False), encoding="utf-8")

    # -- fixture helpers ------------------------------------------------

    def _write_all_standards(self) -> None:
        for rev in CMIS_REVS:
            _write(
                self.standards_dir / cvc.STANDARD_FILENAMES[("cmis", rev)],
                SyntheticFixtures.cmis_standard(rev),
            )
        _write(
            self.standards_dir / cvc.STANDARD_FILENAMES[("sff-8636", "2.12")],
            SyntheticFixtures.sff8636_standard(),
        )
        _write(
            self.standards_dir / cvc.STANDARD_FILENAMES[("sff-8472", "12.5a")],
            SyntheticFixtures.sff8472_standard(),
        )

    def _write_all_authority_reports(self) -> None:
        for family, report in (
            ("cmis", SyntheticFixtures.cmis_authority_report()),
            ("sff-8636", SyntheticFixtures.sff8636_authority_report()),
            ("sff-8472", SyntheticFixtures.sff8472_authority_report()),
        ):
            _write(
                self.authority_dir / cvc.AUTHORITY_REPORT_FILES[family],
                json.dumps(report, indent=2),
            )

    def _cli_write(self, family: str, **kw: Path | str | None) -> int:
        return _run_cli(
            "--write", "--family", family,
            "--schemas-dir", str(kw.get("schemas_dir", self.schemas_dir)),
            "--standards-dir", str(kw.get("standards_dir", self.standards_dir)),
            "--receipts-dir", str(kw.get("receipts_dir", self.receipts_dir)),
            "--authority-dir", str(kw.get("authority_dir", self.authority_dir)),
            "--repo-root", str(kw.get("repo_root", self.root)),
        )

    def _cli_check(self, family: str, **kw: Path | str | None) -> int:
        return _run_cli(
            "--check", "--family", family,
            "--schemas-dir", str(kw.get("schemas_dir", self.schemas_dir)),
            "--standards-dir", str(kw.get("standards_dir", "nonexistent-standards")),
            "--receipts-dir", str(kw.get("receipts_dir", self.receipts_dir)),
            "--authority-dir", str(kw.get("authority_dir", self.authority_dir)),
            "--repo-root", str(kw.get("repo_root", self.root)),
        )

    def _receipt(self, family: str, receipts_dir: Path | None = None) -> dict:
        path = (receipts_dir or self.receipts_dir) / f"{family}.json"
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)

    def _kinds(self, family: str) -> set[str]:
        return {m.get("kind") for m in self._receipt(family).get("mismatches") or []}

    def _build(self, family: str, **kw) -> dict:
        return cvc.build_receipt(
            family,
            Path(kw.get("schemas_dir", self.schemas_dir)),
            Path(kw.get("standards_dir", self.standards_dir)),
            Path(kw.get("authority_dir", self.authority_dir)),
            Path(kw.get("repo_root", self.root)),
        )


class TestCmisConformance(ConformanceTestBase):
    """CMIS: all six revisions through the real comparison and receipt paths."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)

    def test_conformant_write_and_check_pass(self) -> None:
        """A conformant CMIS schema writes a passing receipt and checks clean.

        All six revision standards are present and compared; bits 4-7 of
        lower byte 3 are marked unknown (the D1-honest marking that conforms
        in every revision) with a blocked-read qualifier.
        """
        self.assertEqual(self._cli_write("cmis"), 0)
        receipt = self._receipt("cmis")
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(receipt["mismatches"], [])
        self.assertEqual(
            sorted(receipt["revisions_checked"]), sorted(CMIS_REVS)
        )
        self.assertEqual(len(receipt["standard_files"]), 6)
        self.assertEqual(self._cli_check("cmis"), 0)

    def test_all_six_revisions_compared_not_one_byte(self) -> None:
        """Every revision has its own standard file and byte-3 entry, and the
        general fields (byte 0, 14-15, 26) are compared too."""
        receipt = self._build("cmis")
        by_rev = {
            e["revision"]: e
            for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
        }
        self.assertEqual(sorted(by_rev), sorted(CMIS_REVS))
        # ... and the module-flag block bytes are expanded per revision too.
        for rev in CMIS_REVS:
            offsets = {
                e["offset"] for e in receipt["compared_inventory"]
                if e.get("kind") == "byte" and e.get("revision") == rev
            }
            self.assertEqual(offsets, {3, 8, 9, 10, 11, 12, 13}, rev)
        field_entries = [
            e for e in receipt["compared_inventory"] if e.get("kind") == "field"
        ]
        names = {e["field_name"] for e in field_entries}
        self.assertIn("identifier", names)
        self.assertIn("temperature", names)
        self.assertIn("low_pwr_control", names)
        for e in field_entries:
            self.assertNotEqual(e["status"], "not_compared", e["field_name"])

    def test_passing_receipt_explicit_unknown_blocked_read(self) -> None:
        """Passing receipt with explicit Unknown and a blocked-read qualifier."""
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "pass")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3 and e.get("revision") == "3.0"
        )
        self.assertFalse(entry["whole_byte_qualified"])
        self.assertEqual(entry["qualification"]["readiness"], "blocked")
        # Round 5: CMIS 3.0 has no read-effect passage, so the typed RO bits
        # 0-3 carry an unknown read effect too (never a default "none").
        self.assertEqual(entry["unknown_constituents_required"], [0, 1, 2, 3, 4, 5, 6, 7])
        for bit in entry["bits"]:
            if bit["bit"] >= 4:
                self.assertEqual(bit["schema_access"], "unknown")
                self.assertEqual(bit["read_effect"], "unknown")
                self.assertEqual(bit["status"], "conformant")
        # The 4.0-5.3 reserved-location interpretation is recorded as
        # inference, separately from the primary authority.
        e53 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3 and e.get("revision") == "5.3"
        )
        reserved_bit = next(b for b in e53["bits"] if b["bit"] == 4)
        # The 4.0-5.3 reserved-location interpretation is recorded as
        # inference on the expected side, separately from primary authority.
        self.assertIn("reserved-location interpretation", reserved_bit["inference"])
        self.assertEqual(reserved_bit["read_effect"], "none")
        self.assertEqual(reserved_bit["schema_access"], "unknown")
        self.assertIsNotNone(reserved_bit["authority"])

    def test_passing_cmis_30_table18_type_column(self) -> None:
        """Passing CMIS 3.0 Table 18 Type-column comparison."""
        receipt = self._build("cmis")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3 and e.get("revision") == "3.0"
        )
        bits = {b["bit"]: b for b in entry["bits"]}
        self.assertEqual(bits[0]["standard_type"], "RO RQD")
        self.assertEqual(bits[0]["access_permission"], "ro")
        self.assertEqual(bits[1]["standard_type"], "RO RQD")
        self.assertEqual(bits[3]["access_permission"], "ro")
        # The reserved row carries no Type: bits 4-7 stay unknown for 3.0.
        self.assertEqual(bits[4]["access_permission"], "unknown")
        self.assertEqual(bits[4]["read_effect"], "unknown")

    def test_reserved_bits_ro_fails_for_30_only(self) -> None:
        """Marking byte-3 bits 4-7 ro borrows the 4.0-5.3 interpretation:
        it conforms for 4.0-5.3 (with the inference recorded) but is an
        access mismatch for CMIS 3.0, whose byte-scoped reserved statement
        leaves them unknown."""
        SyntheticFixtures.cmis_schema(self.root, byte3_reserved_access="ro")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        revs_mismatched = {
            m["revision"] for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch" and m.get("offset") == 3
        }
        self.assertEqual(revs_mismatched, {"3.0"})
        e53 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3 and e.get("revision") == "5.3"
        )
        bit4 = next(b for b in e53["bits"] if b["bit"] == 4)
        self.assertEqual(bit4["status"], "conformant")
        self.assertIn("reserved-location interpretation", bit4["inference"])
        self.assertEqual(e53["qualification"]["readiness"], "qualified_with_inference")

    def test_missing_standard_file_is_error(self) -> None:
        """A missing applicable standard file is an error, never a pass."""
        (self.standards_dir / cvc.STANDARD_FILENAMES[("cmis", "5.3")]).unlink()
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("missing_applicable_standard_comparison", self._kinds_after(receipt))
        self.assertEqual(self._cli_write("cmis"), 1)
        written = self._receipt("cmis")
        self.assertEqual(written["status"], "fail")

    def _kinds_after(self, receipt: dict) -> set[str]:
        return {m.get("kind") for m in receipt["mismatches"]}

    def test_missing_marking_is_error(self) -> None:
        """A field without any access or is_dynamic marking is an error."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text = text.replace(
            """  - name: low_pwr_control
    offset: 26
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
""",
            """  - name: low_pwr_control
    offset: 26
    size_bytes: 1
""",
        )
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("missing_marking", self._kinds_after(receipt))
        msgs = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "missing_marking" and m.get("field") == "low_pwr_control"
        ]
        axes = {m.get("axis") for m in msgs}
        self.assertEqual(axes, {"access", "is_dynamic"})

    def test_unparseable_marking_is_error(self) -> None:
        """A marking outside the D1 vocabulary is an error with its own kind."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text = text.replace("    access: rw\n", "    access: invalid_marking\n")
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("unparseable_marking", self._kinds_after(receipt))

    def test_retired_marking_spelling_is_error(self) -> None:
        """behavior.latching left the D1 vocabulary; using it is an error."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text = text.replace(
            """  - name: module_state_changed_flag
    offset: 8
    bit_offset: 0
    bit_width: 1
    access: ro_cor
""",
            """  - name: module_state_changed_flag
    offset: 8
    bit_offset: 0
    bit_width: 1
    access: ro
    behavior: { latching: clear_on_read }
""",
        )
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        retired = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "retired_marking_spelling"
            and m.get("field") == "module_state_changed_flag"
        ]
        self.assertTrue(retired)
        self.assertIn("behavior.latching", retired[0]["spelling"])

    def test_unresolved_input_is_error(self) -> None:
        """A missing field_includes file fails closed (no silent drop)."""
        page_file = self.schemas_dir / "cmis" / "lower-memory.yaml"
        text = page_file.read_text(encoding="utf-8")
        text = text.replace(
            "  - fields/lower-memory-part-01.yaml",
            "  - fields/does-not-exist.yaml",
        )
        page_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("unresolved_input", self._kinds_after(receipt))
        self.assertEqual(self._cli_write("cmis"), 1)

    def test_unparseable_yaml_input_is_error(self) -> None:
        """An unparseable schema input fails closed instead of being skipped."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        field_file.write_text("fields: [ broken", encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("unresolved_input", self._kinds_after(receipt))

    def test_unknown_template_reference_is_error(self) -> None:
        """A field referencing an undefined template is an unresolved input."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text = text.replace("template: status_ro", "template: missing_template")
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("unresolved_input", self._kinds_after(receipt))

    def test_no_standard_authority_is_error(self) -> None:
        """A marked field at a location the applicable tables do not type is
        an error, never a silent skip."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text += """  - name: unmapped_field
    offset: 60
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
"""
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("no_standard_authority", self._kinds_after(receipt))

    def test_unlocatable_field_is_error(self) -> None:
        """A field whose offset expression cannot be expanded is skipped
        only with an explicit skipped_field_or_range error."""
        field_file = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml"
        text = field_file.read_text(encoding="utf-8")
        text += """  - name: offset_expr_field
    offset_expr: "4 + bank"
    bit_offset: 0
    bit_width: 1
    access: ro
    flags:
      is_dynamic: false
"""
        field_file.write_text(text, encoding="utf-8")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("skipped_field_or_range", self._kinds_after(receipt))

    def test_empty_comparison_is_error(self) -> None:
        """No fields and no standards at all: the comparison is empty - an
        error, never a zero-mismatch pass."""
        (self.schemas_dir / "cmis" / "fields" / "lower-memory-part-01.yaml").write_text(
            "fields: []\n", encoding="utf-8"
        )
        receipt = self._build("cmis", standards_dir=self.root / "empty-standards")
        self.assertEqual(receipt["status"], "fail")
        kinds = self._kinds_after(receipt)
        self.assertIn("empty_comparison", kinds)
        self.assertIn("missing_applicable_standard_comparison", kinds)

    def test_missing_authority_report_is_error(self) -> None:
        """The completed authority reports are required research: a missing
        report is an error, never a silent pass."""
        (self.authority_dir / "cmis-byte3-authority.json").unlink()
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("missing_authority_report", self._kinds_after(receipt))

    def test_changing_inherited_template_invalidates_receipt(self) -> None:
        """Changing only an inherited template marking makes the receipt stale."""
        self.assertEqual(self._cli_write("cmis"), 0)
        self.assertEqual(self._cli_check("cmis"), 0)
        # Mutate ONLY the inherited template in schemas/sff-8024/_index.yaml.
        inherited = self.schemas_dir / "sff-8024" / "_index.yaml"
        inherited.write_text(
            inherited.read_text(encoding="utf-8").replace(
                "    access: ro\n", "    access: unknown\n"
            ),
            encoding="utf-8",
        )
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_closure_membership_changes_invalidate_receipt(self) -> None:
        """Added and removed relevant inputs make the receipt stale."""
        self.assertEqual(self._cli_write("cmis"), 0)
        # Added input: a new field include joins the closure.
        extra = self.schemas_dir / "cmis" / "fields" / "lower-memory-part-02.yaml"
        _write(extra, "fields: []\n")
        page = self.schemas_dir / "cmis" / "lower-memory.yaml"
        page.write_text(
            page.read_text(encoding="utf-8").replace(
                "  - fields/lower-memory-part-01.yaml",
                "  - fields/lower-memory-part-01.yaml\n  - fields/lower-memory-part-02.yaml",
            ),
            encoding="utf-8",
        )
        self.assertEqual(self._cli_check("cmis"), 1)
        # Removed input: drop it again - still stale against the old receipt.
        page.write_text(
            page.read_text(encoding="utf-8").replace(
                "\n  - fields/lower-memory-part-02.yaml", ""
            ),
            encoding="utf-8",
        )
        extra.unlink()
        self.assertEqual(self._cli_check("cmis"), 0)

    def test_write_failing_receipt_exits_nonzero(self) -> None:
        """--write of a failing receipt exits non-zero but still writes it."""
        SyntheticFixtures.cmis_schema(self.root, include_byte3_reserved=False)
        self.assertEqual(self._cli_write("cmis"), 1)
        receipt = self._receipt("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("missing_reserved_bit_comparison", {m.get("kind") for m in receipt["mismatches"]})
        # --check fails on the recorded mismatches.
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_uncovered_reserved_bits_are_compared(self) -> None:
        """Reserved bits inside a covered byte are compared: without an
        explicit schema marking they are missing_reserved_bit_comparison
        errors, never a silent pass."""
        SyntheticFixtures.cmis_schema(self.root, include_byte3_reserved=False)
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        missing = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "missing_reserved_bit_comparison" and m.get("offset") == 3
        ]
        self.assertEqual({m.get("bit") for m in missing}, {4, 5, 6, 7})
        e30 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3 and e.get("revision") == "3.0"
        )
        self.assertEqual(e30["uncovered_mask"], 0xF0)

    def test_deterministic_receipts(self) -> None:
        """Two runs produce byte-identical receipts."""
        self.assertEqual(self._cli_write("cmis"), 0)
        first = (self.receipts_dir / "cmis.json").read_text(encoding="utf-8")
        self.assertEqual(self._cli_write("cmis"), 0)
        second = (self.receipts_dir / "cmis.json").read_text(encoding="utf-8")
        self.assertEqual(first, second)

    def test_non_page_include_with_fields_is_error(self) -> None:
        """An include whose kind is not page but which carries fields is an
        error: round 2 skipped it silently and its fields were never
        compared while the receipt still passed."""
        _write(
            self.schemas_dir / "cmis" / "notes.yaml",
            """schema: openxcvr.notes.v1
kind: notes
fields:
  - name: note_field
    offset: 90
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
""",
        )
        index = self.schemas_dir / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "  - lower-memory.yaml", "  - lower-memory.yaml\n  - notes.yaml"
            ),
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        skipped = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "skipped_field_or_range" and m.get("include") == "notes.yaml"
        ]
        self.assertTrue(skipped)
        self.assertIn("not kind: page", skipped[0]["message"])

    def test_non_dict_field_entry_is_error(self) -> None:
        """A non-mapping entry in a fields list is an error, never a silent
        drop."""
        field_file = self._fields_file("cmis")
        field_file.write_text(
            field_file.read_text(encoding="utf-8") + "  - bare_string_entry\n",
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        skipped = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "skipped_field_or_range"
            and m.get("field") == "<non-mapping entry>"
        ]
        self.assertTrue(skipped)
        self.assertIn("non-mapping entry", skipped[0]["message"])

    def test_instances_malformed_shapes_are_errors(self) -> None:
        """instances with a literal offset, an empty range, or a missing
        start are errors; a count-only range expands to every occurrence
        (round 2 collapsed it to one, silently)."""
        # (a) instances with a literal offset: round 2 yielded one occurrence
        # at the literal offset and ignored the range.
        self._replace_in_fields(
            "cmis",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
""",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
  - name: literal_instances_field
    offset: 3
    bit_offset: 1
    bit_width: 1
    instances: { index: i, start: 0, end: 3 }
    access: ro
    flags:
      is_dynamic: false
""",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        skipped = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "skipped_field_or_range"
            and m.get("field") == "literal_instances_field"
        ]
        self.assertTrue(skipped)
        self.assertIn("without offset_expr", skipped[0]["message"])

    def test_instances_count_expands_every_occurrence(self) -> None:
        """instances with start+count (no end) expands to count occurrences:
        round 2 defaulted end to 0 and silently compared one."""
        self._replace_in_fields(
            "cmis",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
""",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
  - name: count_instances_field
    offset_expr: "3 + i"
    bit_offset: 0
    bit_width: 1
    instances: { index: i, start: 0, count: 4 }
    access: ro
    flags:
      is_dynamic: false
""",
        )
        fields, errors = cvc.load_family_fields("cmis", self.schemas_dir)
        occurrences = [f for f in fields if f.name == "count_instances_field"]
        self.assertEqual(len(occurrences), 4)
        self.assertEqual([f.byte_lo for f in occurrences], [3, 4, 5, 6])

    def test_instances_empty_or_startless_range_is_error(self) -> None:
        """An instances range with end < start (zero occurrences) or without
        an explicit start is an error, never a silent vanish/default."""
        self._replace_in_fields(
            "cmis",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
""",
            """  - name: identifier
    offset: 0
    size_bytes: 1
    template: inherited_ro
  - name: empty_range_field
    offset_expr: "3 + i"
    bit_offset: 0
    bit_width: 1
    instances: { index: i, start: 3, end: 0 }
    access: ro
    flags:
      is_dynamic: false
  - name: startless_range_field
    offset_expr: "3 + i"
    bit_offset: 0
    bit_width: 1
    instances: { index: i, end: 3 }
    access: ro
    flags:
      is_dynamic: false
""",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        skipped = {
            m.get("field") for m in receipt["mismatches"]
            if m.get("kind") == "skipped_field_or_range"
            and m.get("field") in ("empty_range_field", "startless_range_field")
        }
        self.assertEqual(skipped, {"empty_range_field", "startless_range_field"})

    def test_unknown_constituents_block_qualification_every_revision(self) -> None:
        """For every revision (not only 3.0), a schema's Unknown markings on
        byte-3 bits 4-7 block whole-byte qualification: unknown_mask is
        recorded and readiness is blocked, while all eight bits are compared."""
        receipt = self._build("cmis")
        for rev in CMIS_REVS:
            with self.subTest(rev=rev):
                entry = next(
                    e for e in receipt["compared_inventory"]
                    if e.get("kind") == "byte" and e.get("offset") == 3
                    and e.get("revision") == rev
                )
                self.assertEqual(entry["unknown_mask"], 0xF0)
                self.assertFalse(entry["whole_byte_qualified"])
                self.assertEqual(entry["qualification"]["readiness"], "blocked")
                self.assertEqual(entry["compared_mask"], 0xFF)
        # 3.0 is the revision whose reserved statement leaves bits 4-7
        # required-Unknown; the 4.0-5.3 interpretation reads them ro with an
        # inference, but the schema's honest Unknown still blocks readiness.
        e30 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
            and e.get("revision") == "3.0"
        )
        # (Round 5: and bits 0-3 too - 3.0 has no read-effect passage, so its
        # typed RO rows establish no "none" effect.)
        self.assertEqual(e30["unknown_constituents_required"], [0, 1, 2, 3, 4, 5, 6, 7])


class TestSff8636Conformance(ConformanceTestBase):
    """SFF-8636 through the real comparison and receipt paths."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.sff8636_schema(self.root)

    def test_conformant_write_and_check_pass(self) -> None:
        """A conformant SFF-8636 schema runs through the real write/hash/check
        path: receipt passes, --check passes with no parsed standards."""
        self.assertEqual(self._cli_write("sff-8636"), 0)
        receipt = self._receipt("sff-8636")
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(receipt["mismatches"], [])
        self.assertEqual(receipt["revisions_checked"], ["2.12"])
        self.assertEqual(self._cli_check("sff-8636"), 0)

    def test_byte2_startup_acknowledgment_recorded(self) -> None:
        """Byte 2 carries AcknowledgeStartupInterrupt, distinct from the
        alarm-latch effect in bytes 3-21; the receipt does not require the
        byte-2 read effect to be None and does not assert that other pending
        interrupts are cleared."""
        receipt = self._build("sff-8636")
        byte2 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 2
        )
        effect = byte2["byte_scoped_effect"]
        self.assertEqual(effect["effect"], "acknowledge_startup_interrupt")
        self.assertFalse(effect["asserts_other_pending_interrupts_cleared"])
        self.assertIsNotNone(effect["authority"])
        byte3 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
        )
        self.assertEqual(
            byte3["byte_scoped_effect"]["effect"], "interrupt_flag_clear_on_read"
        )
        self.assertNotEqual(
            byte3["byte_scoped_effect"]["effect"],
            byte2["byte_scoped_effect"]["effect"],
        )
        # Bits 3-7 stay Unknown; the byte is not qualified for a whole read.
        self.assertEqual(byte2["unknown_constituents_required"], [3, 4, 5, 6, 7])
        self.assertFalse(byte2["whole_byte_qualified"])
        self.assertEqual(byte2["qualification"]["readiness"], "blocked")

    def test_claiming_211_comparison_from_212_only_input(self) -> None:
        """Schema declares 2.11 but only the 2.12 text exists: the receipt
        refuses the comparison instead of borrowing 2.12 authority."""
        SyntheticFixtures.sff8636_schema(self.root, revision="2.11")
        self.assertEqual(self._cli_write("sff-8636"), 1)
        receipt = self._receipt("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn("claiming_211_comparison_from_212_only_input", self._kinds("sff-8636"))
        # Nothing was compared: no 2.12 citations may appear anywhere - not
        # on bit records, not on byte-scoped effects, not on field entries.
        self.assertEqual(receipt["standard_files"], [])
        for entry in receipt["compared_inventory"]:
            for bit in entry.get("bits") or []:
                self.assertIsNone((bit.get("authority") or {}).get("source_file"))
            effect = entry.get("byte_scoped_effect") or {}
            self.assertIsNone((effect.get("authority") or {}).get("source_file"))
            for exp in (entry.get("expected") or {}).values():
                if isinstance(exp, dict):
                    self.assertIsNone((exp.get("authority") or {}).get("source_file"))

    def test_acknowledging_startup_while_hiding_unknown_constituent(self) -> None:
        """Marking byte-2 bits 3-7 known (ro) while the byte carries the
        startup acknowledgment hides an Unknown constituent: an error."""
        SyntheticFixtures.sff8636_schema(self.root, byte2_reserved_access="ro")
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("acknowledging_startup_while_hiding_unknown_constituent", kinds)
        # The reserved bits are also access mismatches: no read-effect
        # authority exists for them (the reserved-fill rule is write-side).
        access = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch" and m.get("offset") == 2
        ]
        self.assertTrue(access)

    def test_omitted_byte_scoped_effect(self) -> None:
        """A standard text lacking the startup-interrupt prose cannot carry
        the byte-2 effect: omitted_byte_scoped_effect, never a silent pass."""
        std_file = self.standards_dir / cvc.STANDARD_FILENAMES[("sff-8636", "2.12")]
        text = std_file.read_text(encoding="utf-8")
        text = text.replace(
            "The IntL bit will remain asserted until a read is performed of the "
            "Data_Not_Ready bit (Byte 2).",
            "Byte 2 holds the status indicators.",
        )
        self.assertNotIn("The IntL bit will remain asserted", text)
        std_file.write_text(text, encoding="utf-8")
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("omitted_byte_scoped_effect", kinds)

    def test_validator_omitted_byte_effect(self) -> None:
        """The receipt validator catches a byte-2 entry whose effect was
        dropped, independent of the comparator."""
        receipt = self._build("sff-8636")
        for entry in receipt["compared_inventory"]:
            if entry.get("kind") == "byte" and entry.get("offset") == 2:
                entry["byte_scoped_effect"] = None
        receipt["receipt_hash"] = cvc.compute_receipt_hash(receipt)
        violations = cvc.validate_conformance_inventory("sff-8636", receipt)
        self.assertTrue(
            any(v.get("kind") == "omitted_byte_scoped_effect" for v in violations)
        )

    def test_wo_password_area_compared(self) -> None:
        """The Write-Only password area is compared against the writable
        memory blocks table."""
        receipt = self._build("sff-8636")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "field" and e.get("field_name") == "password_entry"
        )
        exp = entry["expected"]["2.12"]
        self.assertEqual(exp["access"], "wo")
        self.assertEqual(exp["status"], "conformant")

    def test_expanded_byte_without_standard_rows_is_error(self) -> None:
        """A mandated expanded byte with no standard rows is an error and its
        schema fields are still compared, never silently dropped with the
        byte entry (round-2 blocker: bytes 6-21 vanished from the receipt
        while a wrong rw marking at byte 6 was scored by nothing)."""
        # Shrink the memory-map overview row so bytes 6-21 have no rows.
        self._rewrite_standard(
            "sff-8636", "2.12",
            "| 3-21 | Interrupt Flags | Read-Only |",
            "| 3-5 | Interrupt Flags | Read-Only |",
        )
        # A wrong-but-legal rw marking at a byte the standard no longer rows.
        self._replace_in_fields(
            "sff-8636",
            """  - name: soft_rs1_select
    offset: 86
""",
            """  - name: byte6_mystery_field
    offset: 6
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
  - name: soft_rs1_select
    offset: 86
""",
        )
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("no_standard_authority", kinds)
        byte_level = {
            m.get("offset") for m in receipt["mismatches"]
            if m.get("kind") == "no_standard_authority" and m.get("bit") is None
        }
        self.assertTrue(set(range(6, 22)) <= byte_level, byte_level)
        # The byte-6 field is compared (as a field entry), not dropped.
        field_entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "field" and e.get("field_name") == "byte6_mystery_field"
        )
        self.assertEqual(field_entry["status"], "no_authority")

    def test_missing_alarm_latch_passage_is_error_not_no_effect(self) -> None:
        """A missing alarm-latch passage is an error, never a silent 'no read
        effect': with the conformant-schema ro markings removed (bits marked
        ro), round 2 returned zero mismatches and a passing receipt."""
        self._rewrite_standard(
            "sff-8636", "2.12",
            "remain set until the fixed-side performs a read operation of the bit",
            "cleared by vendor-specific means",
        )
        # Bits 4-7 of byte 3 marked ro: against round 2 this conformed.
        self._replace_in_fields("sff-8636", "    access: ro_cor\n", "    access: ro\n")
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("authority_passage_not_found_in_standard", kinds)
        byte3 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
        )
        # The expectation is downgraded to unknown, not silently 'no effect'.
        for bit in byte3["bits"]:
            if bit["bit"] >= 4:
                self.assertEqual(bit["read_effect"], "unknown")
        self.assertEqual(byte3["byte_scoped_effect"]["effect"], "unknown")
        self.assertFalse(byte3["whole_byte_qualified"])

    def test_ingest_reads_writable_block_count_column(self) -> None:
        """The '# Bytes' column extends a writable block: a 2-byte
        Read/Write block at bytes 86-87 makes an ro marking at byte 87 a
        mismatch, where round 2 truncated the block to byte 86 and the ro
        marking conformed silently."""
        self._rewrite_standard(
            "sff-8636", "2.12",
            "| 86 | 1 | Read/Write | Control Register |",
            "| 86 | 2 | Read/Write | Control Registers |",
        )
        self._rewrite_standard(
            "sff-8636", "2.12",
            """| 86 | 7-0 | Soft RS(1) Select | Soft rate select control | O | O | O | O |""",
            """| 86 | 7-0 | Soft RS(1) Select | Soft rate select control | O | O | O | O |
| 87 | 7-0 | Soft RS(0) Select | Second rate select control | O | O | O | O |""",
        )
        self._replace_in_fields(
            "sff-8636",
            """  - name: soft_rs1_select
    offset: 86
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
""",
            """  - name: soft_rs1_select
    offset: 86
    size_bytes: 1
    access: rw
    flags:
      is_dynamic: false
  - name: soft_rs0_select
    offset: 87
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
""",
        )
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        mismatches = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch" and m.get("offset") == 87
        ]
        self.assertTrue(mismatches)
        self.assertEqual(mismatches[0]["expected"], "rw")
        self.assertEqual(mismatches[0]["actual"], "ro")

    def test_ingest_unparseable_rows_recorded(self) -> None:
        """Writable-block rows that cannot be placed are recorded as
        unparseable_standard_row errors, never silently dropped."""
        self._rewrite_standard(
            "sff-8636", "2.12",
            """| Page 00h |  |  | Volatile |
| 86 | 1 | Read/Write | Control Register |""",
            """| 95 | 1 | Read/Write | Block before any Page label |
| Page 00h |  |  | Volatile |
| 86 | 1 | Read/Write | Control Register |
| 90 | 1 | Read and Write | Odd operation spelling |
| Note | 1 | Read/Write | Non-numeric byte cell |""",
        )
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        unparseable = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "unparseable_standard_row"
        ]
        messages = " ".join(m.get("message", "") for m in unparseable)
        self.assertTrue(unparseable)
        self.assertIn("before any Page label", messages)
        self.assertIn("unknown Operation spelling", messages)
        self.assertIn("unparseable Byte cell", messages)


class TestSff8472Conformance(ConformanceTestBase):
    """SFF-8472 through the real comparison and receipt paths."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.sff8472_schema(self.root)

    def test_conformant_write_and_check_pass(self) -> None:
        """A conformant SFF-8472 schema with per-bit byte-110 markings runs
        through the real write/hash/check path."""
        self.assertEqual(self._cli_write("sff-8472"), 0)
        receipt = self._receipt("sff-8472")
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(receipt["mismatches"], [])
        self.assertEqual(self._cli_check("sff-8472"), 0)

    def test_byte110_mixed_access_per_bit(self) -> None:
        """Byte 110 is compared per bit: RW bits 3 and 6, RO state bits,
        read effect recorded as an inference from S:8.10."""
        receipt = self._build("sff-8472")
        byte110 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 110
        )
        bits = {b["bit"]: b for b in byte110["bits"]}
        self.assertEqual(bits[6]["access_permission"], "rw")
        self.assertEqual(bits[3]["access_permission"], "rw")
        self.assertEqual(bits[7]["access_permission"], "ro")
        self.assertEqual(bits[0]["access_permission"], "ro")
        self.assertEqual(bits[0]["read_effect"], "none")
        self.assertIn("inference", (bits[0]["inference"] or "").lower())
        self.assertEqual(byte110["qualification"]["readiness"], "qualified_with_inference")

    def test_mixed_access_not_expanded(self) -> None:
        """A whole-byte status_control field over byte 110 is not generated
        bit coverage: mixed_access_not_expanded plus per-bit mismatches."""
        SyntheticFixtures.sff8472_schema(self.root, whole_byte_110=True)
        self.assertEqual(self._cli_write("sff-8472"), 1)
        receipt = self._receipt("sff-8472")
        self.assertEqual(receipt["status"], "fail")
        kinds = self._kinds("sff-8472")
        self.assertIn("mixed_access_not_expanded", kinds)
        self.assertIn("access_mismatch", kinds)
        rw_mismatches = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch" and m.get("offset") == 110
            and m.get("expected") == "rw" and m.get("bit") is not None
        ]
        self.assertEqual({m.get("bit") for m in rw_mismatches}, {3, 6})

    def test_latched_flags_stay_unknown(self) -> None:
        """S:9.9 flag latching is implementation specific: the read effect
        stays unknown and the byte is not qualified for a whole read."""
        receipt = self._build("sff-8472")
        byte112 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 112
        )
        self.assertEqual(byte112["unknown_constituents_required"], list(range(8)))
        for bit in byte112["bits"]:
            self.assertEqual(bit["read_effect"], "unknown")
            self.assertEqual(bit["status"], "conformant")  # schema marks unknown
        self.assertFalse(byte112["whole_byte_qualified"])
        self.assertEqual(byte112["qualification"]["readiness"], "blocked")

    def test_vacuous_whole_byte_qualification_blocked(self) -> None:
        """A byte whose only standard rows are byte-level and which no schema
        field covers is never recorded as qualified: all eight bits must be
        present and compared (round 2 recorded whole_byte_qualified: true over
        an empty bit list - an internally contradictory record)."""
        # Replace byte 116's bit rows with one byte-level overview row.
        self._rewrite_standard(
            "sff-8472", "12.5a",
            """| 116 | 7 | Temp High Warning | High temperature warning flag |
| 116 | 6 | Temp Low Warning | Low temperature warning flag |
| 116 | 5 | Vcc High Warning | High supply voltage warning flag |
| 116 | 4 | Vcc Low Warning | Low supply voltage warning flag |
| 116 | 3 | Bias High Warning | High bias current warning flag |
| 116 | 2 | Bias Low Warning | Low bias current warning flag |
| 116 | 1 | TX Power High Warning | High TX power warning flag |
| 116 | 0 | TX Power Low Warning | Low TX power warning flag |""",
            """| 116 | All | Warning Flags | Vendor-specific warning flags byte. |""",
        )
        # Remove the schema's byte-116 coverage: nothing marks those bits.
        self._drop_fields(
            "sff-8472", {(f"warn116_bit{b}", 116) for b in range(8)}
        )
        receipt = self._build("sff-8472")
        byte116 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 116
        )
        self.assertEqual(byte116["bits"], [])
        self.assertEqual(byte116["unknown_constituents_required"], list(range(8)))
        self.assertFalse(byte116["whole_byte_qualified"])
        self.assertEqual(byte116["qualification"]["readiness"], "blocked")
        self.assertIn("uncompared", byte116["qualification"]["reason"])
        self.assertEqual(byte116["compared_mask"], 0)
        # No comparison-relevant loss: no schema marking exists there, so no
        # error is recorded - but the honest record is a blocked byte.
        self.assertEqual(receipt["status"], "pass")

    def test_expanded_fields_compared_beyond_access(self) -> None:
        """The expanded A2h bytes compare every marking the other families
        compare: their fields produce field entries with per-revision
        authority, the byte entries carry a derived byte-scoped effect
        (never an unmeasured null), and change source is recorded per bit."""
        receipt = self._build("sff-8472")
        field_entries = {
            e["field_name"]: e for e in receipt["compared_inventory"]
            if e.get("kind") == "field"
        }
        # Expanded-byte fields DO produce field entries now.
        self.assertIn("soft_tx_disable_select", field_entries)
        self.assertIn("temp_high_alarm", field_entries)
        exp = field_entries["soft_tx_disable_select"]["expected"]["12.5a"]
        self.assertEqual(exp["access"], "rw")
        self.assertEqual(exp["status"], "conformant")
        self.assertEqual(
            exp["authority"]["source_file"],
            cvc.STANDARD_FILENAMES[("sff-8472", "12.5a")],
        )
        byte110 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 110
        )
        # Byte 110's side-effect-free read is the recorded inference, not a
        # literal null.
        self.assertEqual(byte110["byte_scoped_effect"]["effect"], "none")
        self.assertIn("inference", byte110["byte_scoped_effect"])
        bits110 = {b["bit"]: b for b in byte110["bits"]}
        self.assertEqual(bits110[6]["schema_change_source"], "host_or_neither")
        self.assertEqual(bits110[7]["schema_change_source"], "module")
        self.assertEqual(
            bits110[6]["authority"]["source_file"],
            cvc.STANDARD_FILENAMES[("sff-8472", "12.5a")],
        )
        # The latched-flag bytes carry the S:9.9 implementation-specific
        # statement as their byte-scoped effect.
        byte112 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 112
        )
        self.assertEqual(byte112["byte_scoped_effect"]["effect"], "unknown")
        self.assertEqual(byte112["byte_scoped_effect"]["scope"], "byte")
        self.assertEqual(
            byte112["byte_scoped_effect"]["authority"]["source_file"],
            cvc.STANDARD_FILENAMES[("sff-8472", "12.5a")],
        )

    def test_nothing_compared_with_standards_present_is_error(self) -> None:
        """Standards present, no schema fields: every expanded byte's bits are
        uncovered and the receipt fails - and, since round 4, zero compared
        schema fields is also empty_comparison (round 3 asserted its absence:
        standard-derived byte entries made the inventory non-empty, which is
        how a zero-field SFF-8472 family could pass)."""
        self._fields_file("sff-8472").write_text("fields: []\n", encoding="utf-8")
        receipt = self._build("sff-8472")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("missing_reserved_bit_comparison", kinds)
        self.assertIn("empty_comparison", kinds)


class TestConventionPassagePresence(ConformanceTestBase):
    """Every convention passage the comparison relies on must be present in
    the applicable parsed standard: removing one is an error that reaches the
    receipt (round 2 discarded the KIND_PASSAGE_NOT_FOUND error at every call
    site - it was dead code)."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def _remove_passage(self, family: str, rev: str, fragment: str) -> None:
        path = self.standards_dir / cvc.STANDARD_FILENAMES[(family, rev)]
        text = path.read_text(encoding="utf-8")
        self.assertIn(fragment, text)
        # Reword the passage so the fragment is genuinely absent.
        path.write_text(
            text.replace(fragment, "reworded beyond recognition"), encoding="utf-8"
        )

    def test_required_convention_passage_missing_is_error(self) -> None:
        cases = [
            ("cmis", "3.0", "Reserved bytes are for future use and shall not be used and shall be set to 0"),
            ("cmis", "3.0", "until cleared by a read operation that includes the affected bit"),
            ("cmis", "3.0", "Byte 12 is reserved for future Module level flags"),
            ("cmis", "5.3", "A read-only element with clear-on-read (clear-after-read) side effect"),
            ("cmis", "5.3", "such locations are accessible, albeit without associated functionality"),
            ("cmis", "5.3", "Side effects of a READ"),
            ("sff-8636", "2.12", "remain set until the fixed-side performs a read operation of the bit"),
            ("sff-8636", "2.12", "Reserved memory locations are to be filled with logic zeros"),
            ("sff-8636", "2.12", "The IntL bit will remain asserted until a read is performed of the Data_Not_Ready bit"),
            ("sff-8472", "12.5a", "remain readable and writable"),
            ("sff-8472", "12.5a", "may be latched or nonlatched"),
        ]
        for family, rev, fragment in cases:
            with self.subTest(family=family, rev=rev, fragment=fragment[:40]):
                self._restore_standard(family, rev)
                self._remove_passage(family, rev, fragment)
                receipt = self._build(family)
                self.assertEqual(receipt["status"], "fail", (family, rev, fragment))
                self.assertIn(
                    "authority_passage_not_found_in_standard",
                    {m.get("kind") for m in receipt["mismatches"]},
                )
                passage_errors = [
                    m for m in receipt["mismatches"]
                    if m.get("kind") == "authority_passage_not_found_in_standard"
                ]
                # Deduplicated: one error per convention and standard file.
                self.assertEqual(len(passage_errors), 1, passage_errors)
                self._restore_standard(family, rev)

    def test_missing_latched_prose_downgrades_effect_and_records(self) -> None:
        """CMIS 3.0 L- flags: without the latched prose the effect is not
        established - unknown, with the error recorded - instead of asserting
        clear-on-read on a table line whose Type cell says only 'RO' (round 2
        kept the effect, the default authority and a passing receipt)."""
        self._remove_passage(
            "cmis", "3.0",
            "until cleared by a read operation that includes the affected bit",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn(
            "authority_passage_not_found_in_standard",
            {m.get("kind") for m in receipt["mismatches"]},
        )
        byte8 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 8
            and e.get("revision") == "3.0"
        )
        flag_bits = [b for b in byte8["bits"] if b["bit"] in (0, 1, 2, 6, 7)]
        for bit in flag_bits:
            self.assertEqual(bit["read_effect"], "unknown")
            self.assertFalse(bit["effect_established"])
        # No bit-scoped effect is derived from unestablished per-bit effects.
        self.assertIsNone(byte8["byte_scoped_effect"])

    def test_missing_access_types_does_not_fabricate_bit_scope(self) -> None:
        """CMIS 5.x RO/COR: without the access-types definition the byte-scoped
        clear is not recorded and never silently narrowed to a bit-scoped one
        (round 2 substituted scope 'bit' and still passed)."""
        self._remove_passage(
            "cmis", "5.3",
            "A read-only element with clear-on-read (clear-after-read) side effect",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn(
            "authority_passage_not_found_in_standard",
            {m.get("kind") for m in receipt["mismatches"]},
        )
        byte9 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 9
            and e.get("revision") == "5.3"
        )
        # Not a fabricated bit-scoped clear: the byte scope is not established.
        self.assertIsNone(byte9["byte_scoped_effect"])
        for bit in byte9["bits"]:
            self.assertIn("not established", bit["authority"].get("note", ""))


class TestStandardContentPerturbation(ConformanceTestBase):
    """Perturbing the synthetic standard CONTENT changes the outcome, so a
    comparator with hard-coded expectations would go red (round-2 test
    blocker: only SFF-8636's IntL prose was ever mutated)."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def test_cmis_type_cell_perturbation_fails(self) -> None:
        # 5.3 flag Type cell RO/COR -> RO: the schema's ro_cor markings now
        # assert an effect the table no longer states.
        self._rewrite_standard("cmis", "5.3", "RO/COR Adv.", "RO Rqd.")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        mismatched = {
            m.get("offset") for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch"
        }
        self.assertTrue({8, 9, 10, 11} <= mismatched, mismatched)

        # 3.0 ModuleState Type cell RO RQD -> RW Rqd.: the schema's ro marking
        # now mismatches at bits 1-3 for revision 3.0.
        self._restore_standard("cmis", "3.0")
        self._rewrite_standard(
            "cmis", "3.0",
            "| 3 | 3-1 | ModuleState | Current Module State | RO RQD |",
            "| 3 | 3-1 | ModuleState | Current Module State | RW Rqd. |",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        mismatched = {
            (m.get("revision"), m.get("offset"), m.get("bit"))
            for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch"
        }
        self.assertTrue(
            {("3.0", 3, b) for b in (1, 2, 3)} <= mismatched, mismatched
        )

    def test_sff8472_rw_description_perturbation_fails(self) -> None:
        # The Soft TX Disable Select row loses its 'Read/write bit' authority:
        # the schema's rw marking must now mismatch.
        self._rewrite_standard(
            "sff-8472", "12.5a",
            "Read/write bit that allows software disable of laser.",
            "Digital state of the TX Disable control input.",
        )
        receipt = self._build("sff-8472")
        self.assertEqual(receipt["status"], "fail")
        mismatched = {
            (m.get("offset"), m.get("bit")) for m in receipt["mismatches"]
            if m.get("kind") == "access_mismatch"
        }
        self.assertIn((110, 6), mismatched)

    def test_sff8472_latched_flag_passage_perturbation_fails(self) -> None:
        self._rewrite_standard(
            "sff-8472", "12.5a",
            "The flags may be latched or nonlatched.",
            "The flags are plain status bits.",
        )
        receipt = self._build("sff-8472")
        self.assertEqual(receipt["status"], "fail")
        kinds = {m.get("kind") for m in receipt["mismatches"]}
        self.assertIn("authority_passage_not_found_in_standard", kinds)


class TestWrongMarkingAndOmittedByteSweep(ConformanceTestBase):
    """Round-2 test blocker: refusal is pinned beyond the three fixture knobs
    with a parametrized sweep over each compared byte class - a wrong-but-
    legal D1 marking, and an omitted byte, must fail everywhere."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def test_wrong_legal_marking_refused_across_byte_classes(self) -> None:
        cases = [
            # (family, mutation of the conformant schema's fields file)
            ("cmis",  # module-flag block: latched flag ro_cor -> ro
             """  - name: module_state_changed_flag
    offset: 8
    bit_offset: 0
    bit_width: 1
    access: ro_cor
""",
             """  - name: module_state_changed_flag
    offset: 8
    bit_offset: 0
    bit_width: 1
    access: ro
    flags:
      is_dynamic: true
"""),
            ("cmis",  # monitor byte 9: ro_cor -> rw
             """  - name: monitor_flags_1
    offset: 9
    size_bytes: 1
    access: ro_cor
""",
             """  - name: monitor_flags_1
    offset: 9
    size_bytes: 1
    access: rw
"""),
            ("cmis",  # reserved byte 12: unknown -> ro (fails for 3.0)
             """  - name: reserved_byte12
    offset: 12
    size_bytes: 1
    template: reserved_templ
""",
             """  - name: reserved_byte12
    offset: 12
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
"""),
            ("cmis",  # custom byte 13: unknown -> rw
             """  - name: custom_byte13
    offset: 13
    size_bytes: 1
    access: unknown
""",
             """  - name: custom_byte13
    offset: 13
    size_bytes: 1
    access: rw
"""),
            ("sff-8636",  # byte 2 Data_Not_Ready: ro -> rw (beyond the bits-3-7 knob)
             """  - name: data_not_ready
    offset: 2
    bit_offset: 0
    bit_width: 1
    template: status_ro
""",
             """  - name: data_not_ready
    offset: 2
    bit_offset: 0
    bit_width: 1
    access: rw
    flags:
      is_dynamic: false
"""),
            ("sff-8636",  # alarm flag byte 3: ro_cor -> ro (beyond byte 2)
             """  - name: tx4_los_flag
    offset: 3
    bit_offset: 7
    bit_width: 1
    access: ro_cor
""",
             """  - name: tx4_los_flag
    offset: 3
    bit_offset: 7
    bit_width: 1
    access: ro
"""),
            ("sff-8472",  # byte 110 RW control: rw -> ro (beyond whole_byte_110)
             """  - name: soft_tx_disable_select
    offset: 110
    bit_offset: 6
    bit_width: 1
    access: rw
""",
             """  - name: soft_tx_disable_select
    offset: 110
    bit_offset: 6
    bit_width: 1
    access: ro
"""),
            ("sff-8472",  # latched byte 112: unknown -> ro_cor
             """  - name: temp_high_alarm
    offset: 112
    bit_offset: 7
    bit_width: 1
    access: unknown
""",
             """  - name: temp_high_alarm
    offset: 112
    bit_offset: 7
    bit_width: 1
    access: ro_cor
"""),
        ]
        for family, old, new in cases:
            with self.subTest(family=family, old=old.splitlines()[1].strip()):
                # Fresh conformant fields file per case.
                if family == "cmis":
                    SyntheticFixtures.cmis_schema(self.root)
                elif family == "sff-8636":
                    SyntheticFixtures.sff8636_schema(self.root)
                else:
                    SyntheticFixtures.sff8472_schema(self.root)
                self._replace_in_fields(family, old, new)
                receipt = self._build(family)
                self.assertEqual(receipt["status"], "fail")
                self.assertIn(
                    "access_mismatch", {m.get("kind") for m in receipt["mismatches"]}
                )

    def test_omitted_byte_refused_across_byte_classes(self) -> None:
        drop_specs = [
            ("cmis", {("monitor_flags_1", 9)}),
            ("cmis", {("reserved_byte12", 12)}),
            ("cmis", {("custom_byte13", 13)}),
            ("sff-8636", {(f"tx{n}_los_flag", 3) for n in (4, 3, 2, 1)}),
            ("sff-8472", {(f"tx_disable_state", 110), ("soft_tx_disable_select", 110),
                          ("rs1_state", 110), ("rate_select_state", 110),
                          ("soft_rate_select_select", 110), ("tx_fault_state", 110),
                          ("rx_los_state", 110), ("data_not_ready", 110)}),
            ("sff-8472", {(f"temp_{level}_{kind}", 112)
                          for level in ("high", "low") for kind in ("alarm", "warning")}
                         | {(f"vcc_{level}_{kind}", 112)
                            for level in ("high", "low") for kind in ("alarm", "warning")}),
        ]
        for family, drop in drop_specs:
            with self.subTest(family=family, drop=sorted(drop)):
                if family == "cmis":
                    SyntheticFixtures.cmis_schema(self.root)
                elif family == "sff-8636":
                    SyntheticFixtures.sff8636_schema(self.root)
                else:
                    SyntheticFixtures.sff8472_schema(self.root)
                self._drop_fields(family, drop)
                receipt = self._build(family)
                self.assertEqual(receipt["status"], "fail")
                self.assertIn(
                    "missing_reserved_bit_comparison",
                    {m.get("kind") for m in receipt["mismatches"]},
                )


class TestRound3SkipAudit(ConformanceTestBase):
    """Round-3 closed skip audit: every place the comparison could skip,
    default or narrow an input, row, field, bit, passage or marking either
    records an error that reaches the receipt or is shown to lose nothing."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def _parse(self, family: str, text: str) -> "cvc.ParsedStandard":
        path = self.root / "snippet.md"
        _write(path, text)
        return cvc.parse_standard_file(path, family)

    # -- passage search ---------------------------------------------------

    def test_passage_found_despite_pdf_space_before_punctuation(self) -> None:
        """PDF extraction leaves "accessible , albeit": the passage is
        present and must be found (the real CMIS 5.3 text has exactly this),
        while a genuinely reworded passage still fails."""
        self._rewrite_standard(
            "cmis", "5.3",
            "such locations are accessible, albeit",
            "such locations are accessible , albeit",
        )
        receipt = self._build("cmis")
        self.assertNotIn(
            "authority_passage_not_found_in_standard",
            {m.get("kind") for m in receipt["mismatches"]},
        )
        self.assertEqual(receipt["status"], "pass")

    def test_cmis_40_ro_cor_rests_on_own_prose_never_borrowed(self) -> None:
        """A CMIS 4.0 RO/COR Type cell is established by 4.0's own latched-
        flag prose (bit-scoped), never by the 5.x access-types definition
        (round-3 real run: 4.0 searched for the 5.x passage)."""
        self._rewrite_standard("cmis", "4.0", "| RO Opt. |", "| RO/COR Opt. |")
        receipt = self._build("cmis")
        passage_errors = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "authority_passage_not_found_in_standard"
        ]
        self.assertEqual(passage_errors, [])
        self.assertEqual(receipt["status"], "pass")
        byte9 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 9
            and e.get("revision") == "4.0"
        )
        self.assertEqual(byte9["byte_scoped_effect"]["scope"], "bit")
        for bit in byte9["bits"]:
            self.assertEqual(bit["read_effect"], "clear_on_read")
            self.assertTrue(bit["effect_established"])
            self.assertEqual(
                bit["authority"]["passage"],
                cvc.CONVENTION_PASSAGES[("cmis", "latched_prose")]["fragment"],
            )

    # -- standard ingestion -------------------------------------------------

    def test_overview_address_column_rows_ingested(self) -> None:
        """Overview tables keyed by an Address column (with a Size count) or
        by From/To columns are ingested, not dropped as byte-less tables."""
        std = self._parse("cmis", """## 8.2 Lower Memory

Table 8-4 Lower Memory Overview

| Address | Size | Subject Area | Description | Type |
|---|---|---|---|---|
| 0 - 2 | 3 | Management Characteristics | Basic advertisement | Adv. |
| 46-55 | 10 | - | Reserved[10] | - |
""")
        spans = {(r.byte_lo, r.byte_hi) for r in std.rows}
        self.assertEqual(spans, {(0, 2), (46, 55)})
        std = self._parse("sff-8636", """Table 6-1  Common Memory Map

| From | To | Content | No. of bytes | Type |
|---|---|---|---|---|
| Lower Page 00h | Lower Page 00h | Lower Page 00h | Lower Page 00h | Lower Page 00h |
| 3 | 21 | Interrupt Flags | 19 | Read-Only |
| Page 02h (Optional) | Page 02h (Optional) | Page 02h (Optional) | Page 02h (Optional) | Page 02h (Optional) |
| 128 | 255 | User EEPROM Data | 128 | Read/Write |
""")
        placed = {(r.page_ctx, r.byte_lo, r.byte_hi, r.type_text) for r in std.rows}
        self.assertEqual(placed, {
            ("lower", 3, 21, "Read-Only"), ("02h", 128, 255, "Read/Write"),
        })
        self.assertEqual(std.ingest_errors, [])

    def test_unplaceable_general_rows_recorded(self) -> None:
        """A general field-table row that names a location but cannot be
        placed - unparseable Byte cell with digits, unparseable Bits cell,
        a continuation row before any byte, or a data-shaped header whose
        table cannot be joined - is an unparseable_standard_row error that
        reaches the receipt; round 2 dropped each silently."""
        self._rewrite_standard(
            "cmis", "5.3",
            "| 0 | 7-0 | Identifier | Type of module | RO Rqd. |",
            "|  | 3 | Orphan | Continuation row before any byte | RO Rqd. |\n"
            "| 0 | 7-0 | Identifier | Type of module | RO Rqd. |\n"
            "| 215 216 | 7-0 | MergedBytes | PDF-merged byte cell | RO Rqd. |\n"
            "| 1 | 7-4 3-0 | MergedBits | PDF-merged bits cell | RO Rqd. |",
        )
        self._rewrite_standard(
            "cmis", "5.3",
            "## Table 8-9 Module Flags",
            "A prose paragraph separates this fragment from any located table.\n\n"
            "| 40 | 7-0 | HeaderlessRow | header lost at a page break | RO Rqd. |\n"
            "|---|---|---|---|---|\n"
            "| 41 | 7-0 | HeaderlessRow2 | continued | RO Rqd. |\n\n"
            "## Table 8-9 Module Flags",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        messages = " ".join(
            m.get("message", "") for m in receipt["mismatches"]
            if m.get("kind") == "unparseable_standard_row"
        )
        self.assertIn("no Byte cell and no earlier row", messages)
        self.assertIn("unparseable Byte cell '215 216'", messages)
        self.assertIn("unparseable Bits cell '7-4 3-0'", messages)
        self.assertIn("table header is a data row", messages)

    def test_page_break_continuation_carries_byte(self) -> None:
        """A table split by a PDF page break (blank lines, a page number, a
        running heading, then the same header) carries the byte into the
        continued rows instead of dropping them."""
        std = self._parse("sff-8472", """## 9.9 Alarm and Warning Flag Bits [Address A2h, Bytes 112, 113, 116, 117]

Table 9-17  Alarm and Warning Flag Bits

| A2h | Bit | Name | Description |
|---|---|---|---|
| 116 | 7 | Temp High Warning | flag |
| 116 | 6 | Temp Low Warning | flag |

12

## SFF-8472 Rev 12.5a

| A2h | Bit | Name | Description |
|---|---|---|---|
|  | 5 | Vcc High Warning | flag |
|  | 4 | Vcc Low Warning | flag |
""")
        self.assertEqual(std.ingest_errors, [])
        placed = {(r.byte_lo, r.bit_lo) for r in std.rows}
        self.assertEqual(placed, {(116, 7), (116, 6), (116, 5), (116, 4)})
        self.assertTrue(all(r.page_ctx == "a2h" for r in std.rows))

    def test_lost_header_continuation_joined(self) -> None:
        """A page break that lost the header ("header" is a data row)
        continues the previous located table: its rows are placed."""
        std = self._parse("cmis", """## 8.2 Lower Memory

Table 8-6 Global Status Information (Lower Memory)

| Byte | Bits | Field Name | Field Description | Type |
|---|---|---|---|---|
| 92 | 7-4 | HostLaneCount | lanes | RO Rqd. |

|  | 3-0 | MediaLaneCount | lanes | RO Rqd. |
|---|---|---|---|---|
| 93 | 7-0 | HostLaneAssignment | lanes | RO Rqd. |
""")
        self.assertEqual(std.ingest_errors, [])
        placed = {(r.byte_lo, r.bit_lo, r.name) for r in std.rows}
        self.assertEqual(placed, {
            (92, 4, "HostLaneCount"), (92, 0, "MediaLaneCount"),
            (93, 0, "HostLaneAssignment"),
        })

    def test_prose_sentence_starting_with_table_does_not_set_page(self) -> None:
        """Prose that merely begins "Table N ..." and mentions another page
        is not a caption: it must not refile later rows under that page."""
        std = self._parse("sff-8472", """## 12.2 Variable Receiver Decision Threshold [A2h, Page 02h, Bytes 130-131]

Table 12-3 defines the registers. Availability is indicated in address A0h, byte 65.

Table 12-3  Variable Receiver Decision Threshold Control

| Address | Bits # | Name | Description |
|---|---|---|---|
| 130 | 7-1 | Reserved |  |
""")
        self.assertEqual([(r.page_ctx, r.byte_lo) for r in std.rows], [("02h", 130)])

    def test_multibyte_bits_cell_forms_are_whole_bytes(self) -> None:
        """'Bits #' counts (16 over two bytes), '31-0' over four bytes and
        '7-0 7-0' over two bytes cover whole bytes; anything else on a
        multi-byte row is recorded, never widened silently."""
        std = self._parse("sff-8472", """## 12.3.4 Remote Performance Monitoring Error Counters [A2h, Page 02h]

Table 12-8  Page 02h Remote Performance Monitoring Error Counters

| Address | Bits # | Name | Description |
|---|---|---|---|
| 198- 199 | 16 | Frame LOL Counter | counter |
| 200 - 203 | 31-0 | Frame Count | counter |
| 204- 205 | 7-0 7-0 | frameTomErrCount | counter |
| 206- 207 | 9 | frameMsgErrCount | counter |
""")
        placed = {(r.byte_lo, r.byte_hi, r.bit_lo) for r in std.rows}
        self.assertEqual(placed, {(198, 199, None), (200, 203, None), (204, 205, None)})
        self.assertEqual(len(std.ingest_errors), 1)
        self.assertIn("unparseable Bits cell '9'", std.ingest_errors[0]["message"])

    def test_custom_row_does_not_inherit_carried_type(self) -> None:
        """A Custom row with an empty Type cell does not inherit the previous
        row's Type (its access is vendor-defined): no invented authority."""
        std = self._parse("cmis", """## 8.2 Lower Memory

Table 8-6 Global Status Information (Lower Memory)

| Byte | Bits | Field Name | Field Description | Type |
|---|---|---|---|---|
| 26 | 3 | SoftwareReset | reset | RW Rqd. |
| 26 | 2-0 | Custom | Custom |  |
""")
        custom = next(r for r in std.rows if r.name == "Custom")
        self.assertEqual(custom.type_text, "")

    # -- schema-side markings ------------------------------------------------

    def test_overlapping_fields_all_compared(self) -> None:
        """Every schema field covering a bit is compared, not only the first
        listed (round 2 read covering[0]): a wrongly marked child listed after
        a correct whole-byte field fails, and the disagreeing overlap itself
        is an error; an agreeing overlap passes."""
        child = """  - name: monitor_flags_1
    offset: 9
    size_bytes: 1
    access: ro_cor
    flags:
      is_dynamic: true
  - name: monitor_flags_1_bit0
    offset: 9
    bit_offset: 0
    bit_width: 1
    access: {access}
    flags:
      is_dynamic: true
"""
        old = """  - name: monitor_flags_1
    offset: 9
    size_bytes: 1
    access: ro_cor
    flags:
      is_dynamic: true
"""
        self._replace_in_fields("cmis", old, child.format(access="ro_cor"))
        self.assertEqual(self._build("cmis")["status"], "pass")

        SyntheticFixtures.cmis_schema(self.root)
        self._replace_in_fields("cmis", old, child.format(access="ro"))
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        kinds = {
            (m.get("kind"), m.get("revision"), m.get("offset"), m.get("bit"))
            for m in receipt["mismatches"]
        }
        for rev in CMIS_REVS:
            self.assertIn(("overlapping_fields_disagree", rev, 9, 0), kinds)
            self.assertIn(("access_mismatch", rev, 9, 0), kinds)
        byte9 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 9 and e.get("revision") == "5.3"
        )
        bit0 = next(b for b in byte9["bits"] if b["bit"] == 0)
        self.assertEqual(
            bit0["covering_fields"], ["monitor_flags_1", "monitor_flags_1_bit0"]
        )


    def test_partial_field_coverage_is_error(self) -> None:
        """A field only partly covered by typed standard rows is an error:
        round 2 compared the rows it found and passed the rest silently."""
        self._replace_in_fields(
            "cmis",
            """  - name: temperature
    offset: 14
    size_bytes: 2
""",
            """  - name: temperature
    offset: 14
    size_bytes: 3
""",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        partial = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "no_standard_authority" and m.get("field") == "temperature"
        ]
        self.assertEqual({m["revision"] for m in partial}, set(CMIS_REVS))
        self.assertTrue(all(m["uncovered"] == {"16": 255} for m in partial))

    def test_sff8636_untyped_byte_is_no_authority_not_default_ro(self) -> None:
        """A byte no Type cell, writable block or overview row types has no
        access authority: round 2 defaulted it to 'ro', so an ro marking
        conformed silently.  Now it is no_standard_authority."""
        self._rewrite_standard(
            "sff-8636", "2.12",
            "| 86 | 7-0 | Soft RS(1) Select | Soft rate select control | O | O | O | O |",
            "| 86 | 7-0 | Soft RS(1) Select | Soft rate select control | O | O | O | O |\n"
            "| 99 | 7-0 | Hardware Config | Untyped control byte | O | O | O | O |",
        )
        self._replace_in_fields(
            "sff-8636",
            """  - name: soft_rs1_select
    offset: 86
""",
            """  - name: hw_config
    offset: 99
    size_bytes: 1
    access: ro
    flags:
      is_dynamic: false
  - name: soft_rs1_select
    offset: 86
""",
        )
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "field" and e.get("field_name") == "hw_config"
        )
        self.assertEqual(entry["status"], "no_authority")
        self.assertIn(
            ("no_standard_authority", "hw_config"),
            {(m.get("kind"), m.get("field")) for m in receipt["mismatches"]},
        )

    def test_field_extent_never_defaults(self) -> None:
        """A bit field without bit_width, a byte field without size_bytes and
        a bit span leaving its byte are skipped_field_or_range errors; round 2
        defaulted them to one bit / one byte silently."""
        cases = [
            ("""  - name: module_state
    offset: 3
    bit_offset: 1
    bit_width: 3
""", """  - name: module_state
    offset: 3
    bit_offset: 1
"""),
            ("""  - name: temperature
    offset: 14
    size_bytes: 2
""", """  - name: temperature
    offset: 14
"""),
            ("""  - name: module_state
    offset: 3
    bit_offset: 1
    bit_width: 3
""", """  - name: module_state
    offset: 3
    bit_offset: 6
    bit_width: 3
"""),
        ]
        for old, new in cases:
            with self.subTest(new=new.strip().splitlines()[-1]):
                SyntheticFixtures.cmis_schema(self.root)
                self._replace_in_fields("cmis", old, new)
                receipt = self._build("cmis")
                self.assertEqual(receipt["status"], "fail")
                skipped = [
                    m for m in receipt["mismatches"]
                    if m.get("kind") == "skipped_field_or_range"
                ]
                self.assertTrue(skipped)
                self.assertIn("could not be located", skipped[0]["message"])

    def test_partly_expanded_field_compared_outside_expanded_bytes(self) -> None:
        """A field spanning an expanded byte and a non-expanded byte is
        compared at both: round 2 routed it only to the byte builder, so its
        bytes outside the expanded set were compared by nothing."""
        self._replace_in_fields(
            "cmis",
            """  - name: custom_byte13
    offset: 13
    size_bytes: 1
    access: unknown
""",
            """  - name: custom_byte13
    offset: 13
    size_bytes: 2
    access: unknown
""",
        )
        self._replace_in_fields(
            "cmis",
            """  - name: temperature
    offset: 14
    size_bytes: 2
""",
            """  - name: temperature
    offset: 15
    size_bytes: 1
""",
        )
        receipt = self._build("cmis")
        entry = next(
            (e for e in receipt["compared_inventory"]
             if e.get("kind") == "field" and e.get("field_name") == "custom_byte13"),
            None,
        )
        self.assertIsNotNone(entry)
        # Byte 14 is RO-typed: the unknown marking conforms there only as an
        # honest Unknown; what matters is that it is compared at all.
        self.assertIn(entry["expected"]["5.3"]["status"], ("conformant", "mismatch"))
        self.assertTrue(entry["expected"]["5.3"]["compared_rows"])

    def test_non_mapping_template_is_unresolved_input(self) -> None:
        cmis_index = self.schemas_dir / "cmis" / "_index.yaml"
        text = cmis_index.read_text(encoding="utf-8")
        cmis_index.write_text(
            text.replace("""  reserved_templ:
    access: unknown
    flags:
      is_dynamic: false
""", """  reserved_templ: [unknown]
"""),
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        unresolved = [
            m for m in receipt["mismatches"] if m.get("kind") == "unresolved_input"
        ]
        self.assertTrue(any("not a mapping" in m["message"] for m in unresolved))

    def test_validator_refuses_known_effect_without_citation(self) -> None:
        """Round 2's borrowing rule returned early for an authority without a
        source_file, so a known effect could be asserted on no citation."""
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(cvc.validate_conformance_inventory("cmis", receipt), [])
        byte9 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 9 and e.get("revision") == "5.3"
        )
        self.assertEqual(byte9["byte_scoped_effect"]["effect"], "clear_on_read")
        byte9["byte_scoped_effect"]["authority"] = {"note": "no citation"}
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        self.assertIn(
            ("no_standard_authority", 9),
            {(v.get("kind"), v.get("offset")) for v in violations},
        )

    def test_authority_report_citation_only_when_report_carries_passage(self) -> None:
        """A citation names the authority report only when a report row
        carries the passage; round 2 named the report regardless, claiming
        reuse of research that contributed nothing."""
        receipt = self._build("cmis")
        byte8 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 8 and e.get("revision") == "3.0"
        )
        flag = next(b for b in byte8["bits"] if b["bit"] == 0)
        self.assertEqual(flag["authority"]["authority_report"], "cmis-byte3-authority.json")
        report = json.loads(
            (self.authority_dir / "cmis-byte3-authority.json").read_text(encoding="utf-8")
        )
        report["result"]["revision_evidence"] = [
            r for r in report["result"]["revision_evidence"]
            if r["clause_or_table"] != "latched-flag prose"
        ]
        (self.authority_dir / "cmis-byte3-authority.json").write_text(
            json.dumps(report), encoding="utf-8"
        )
        receipt = self._build("cmis")
        byte8 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 8 and e.get("revision") == "3.0"
        )
        flag = next(b for b in byte8["bits"] if b["bit"] == 0)
        self.assertIsNone(flag["authority"]["authority_report"])
        self.assertIn("authority_report_note", flag["authority"])

    def test_conflicting_equally_specific_rows_recorded(self) -> None:
        """Two equally specific standard rows that disagree about one bit are
        a conflict recorded in the receipt; round 2 picked one by line order
        silently."""
        self._rewrite_standard(
            "cmis", "5.3",
            "| 3 | 0 | InterruptDeasserted | Status of Interrupt output signal | RO Rqd. |",
            "| 3 | 0 | InterruptDeasserted | Status of Interrupt output signal | RO Rqd. |\n"
            "| 3 | 0 | InterruptControl | Conflicting duplicate row | RW Rqd. |",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        conflicts = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "conflicting_standard_rows"
        ]
        self.assertEqual(
            [(m["revision"], m["offset"], m["bit"]) for m in conflicts], [("5.3", 3, 0)]
        )

    def test_since_restricts_compared_revisions_and_is_recorded(self) -> None:
        """Revision applicability is a compared marking: a since-bound field
        is not compared (or failed) at earlier revisions, the receipt records
        why, and an unparseable bound is an error."""
        self._replace_in_fields(
            "cmis",
            """  - name: low_pwr_control
    offset: 26
    size_bytes: 1
    access: rw
""",
            """  - name: low_pwr_control
    offset: 26
    size_bytes: 1
    since: "5.0"
    access: rw
""",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "pass")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "field" and e.get("field_name") == "low_pwr_control"
        )
        self.assertEqual(entry["since"], "5.0")
        for rev in ("3.0", "4.0"):
            self.assertEqual(entry["expected"][rev]["status"], "not_applicable_at_revision")
        for rev in ("5.0", "5.3"):
            self.assertEqual(entry["expected"][rev]["status"], "conformant")

        self._replace_in_fields("cmis", 'since: "5.0"', 'since: "five"')
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn(
            ("unparseable_marking", "since"),
            {(m.get("kind"), m.get("axis")) for m in receipt["mismatches"]},
        )

    def test_since_bound_field_excluded_from_byte_coverage(self) -> None:
        """In the bit-expanded bytes a field applicable only from a later
        revision does not cover the bit at earlier revisions: the bit is
        reported uncovered there, never silently counted as compared."""
        self._replace_in_fields(
            "cmis",
            """  - name: custom_byte13
    offset: 13
    size_bytes: 1
""",
            """  - name: custom_byte13
    offset: 13
    size_bytes: 1
    since: "5.0"
""",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        uncovered = {
            (m.get("revision"), m.get("offset"))
            for m in receipt["mismatches"]
            if m.get("kind") == "missing_reserved_bit_comparison"
        }
        self.assertIn(("4.0", 13), uncovered)
        self.assertNotIn(("5.0", 13), uncovered)


class TestValidatorRules(ConformanceTestBase):
    """Receipt-level rules that must hold for any family and revision."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def _byte_entry(self, **kw) -> dict:
        base = {
            "kind": "byte", "revision": "5.0", "page": "lower-memory", "offset": 3,
            "byte_scoped_effect": None, "unknown_constituents_required": [],
            "unknown_mask": 0, "uncovered_mask": 0, "whole_byte_qualified": True,
            "qualification": {"readiness": "qualified"},
            "bits": [
                {"bit": b, "schema_access": "ro", "read_effect": "none"}
                for b in range(8)
            ],
        }
        base.update(kw)
        return base

    def _receipt_with(self, family: str, inventory: list, **kw) -> dict:
        receipt = {
            "family": family,
            "revisions_checked": kw.pop("revisions_checked", ["5.0"]),
            "standard_files": kw.pop(
                "standard_files",
                [{"revision": "5.0", "file": cvc.STANDARD_FILENAMES[(family, "5.0")]
                  if (family, "5.0") in cvc.STANDARD_FILENAMES else "x.md",
                 "sha256": "0" * 64}],
            ),
            "schema_closure": {"hash": "x"},
            "compared_inventory": inventory,
            "mismatches": [],
        }
        receipt.update(kw)
        return receipt

    def test_no_violations_on_conformant_receipts(self) -> None:
        for family in ("cmis", "sff-8636", "sff-8472"):
            with self.subTest(family=family):
                self.assertEqual(self._cli_write(family), 0)
                receipt = self._receipt(family)
                self.assertEqual(
                    cvc.validate_conformance_inventory(family, receipt), []
                )

    def test_empty_comparison_is_violation(self) -> None:
        receipt = self._receipt_with("cmis", [], revisions_checked=["3.0"])
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        self.assertTrue(
            any(v.get("kind") == "empty_comparison" for v in violations)
        )

    def test_missing_reserved_bit_comparison(self) -> None:
        entry = self._byte_entry(bits=[{"bit": b} for b in range(5)])
        receipt = self._receipt_with("cmis", [entry], revisions_checked=["5.0"])
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        found = [v for v in violations if v.get("kind") == "missing_reserved_bit_comparison"]
        self.assertTrue(found)
        self.assertIn("missing bits [5, 6, 7]", found[0]["message"])

    def test_borrowing_another_revision_authority_cmis(self) -> None:
        """A CMIS 5.0 record citing the 5.3 standard file borrows authority."""
        entry = self._byte_entry(revision="5.0")
        entry["bits"][0]["authority"] = {
            "source_file": cvc.STANDARD_FILENAMES[("cmis", "5.3")],
        }
        receipt = self._receipt_with("cmis", [entry], revisions_checked=["5.0"])
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        self.assertTrue(
            any(v.get("kind") == "borrowing_another_revision_authority" for v in violations)
        )

    def test_borrowing_another_revision_authority_sff8636(self) -> None:
        """A 2.11 record citing the 2.12 standard file borrows authority."""
        entry = self._byte_entry(revision="2.11", offset=2)
        entry["bits"][0]["authority"] = {
            "source_file": cvc.STANDARD_FILENAMES[("sff-8636", "2.12")],
        }
        receipt = self._receipt_with(
            "sff-8636", [entry],
            revisions_checked=["2.11"], standard_files=[],
        )
        violations = cvc.validate_conformance_inventory("sff-8636", receipt)
        kinds = {v.get("kind") for v in violations}
        self.assertIn("borrowing_another_revision_authority", kinds)
        self.assertIn("claiming_211_comparison_from_212_only_input", kinds)

    def test_borrowing_another_revision_authority_field_entry(self) -> None:
        """Field entries' per-revision citations are checked too."""
        entry = {
            "kind": "field", "field_name": "x", "status": "conformant",
            "expected": {
                "3.0": {
                    "status": "conformant",
                    "authority": {
                        "source_file": cvc.STANDARD_FILENAMES[("cmis", "5.0")],
                    },
                }
            },
        }
        receipt = self._receipt_with("cmis", [entry], revisions_checked=["3.0"])
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        self.assertTrue(
            any(v.get("kind") == "borrowing_another_revision_authority" for v in violations)
        )

    def test_missing_applicable_revision_is_violation(self) -> None:
        """A CMIS receipt that skipped revisions violates the rules."""
        receipt = self._receipt_with(
            "cmis", [self._byte_entry(revision="5.3")], revisions_checked=["5.3"]
        )
        violations = cvc.validate_conformance_inventory("cmis", receipt)
        missing = [
            v for v in violations
            if v.get("kind") == "missing_applicable_standard_comparison"
        ]
        self.assertEqual(len(missing), 6)  # 5 unchecked revisions + 1 without a standard file

    def test_claiming_211_without_211_authority(self) -> None:
        receipt = self._receipt_with(
            "sff-8636", [self._byte_entry(revision="2.11")],
            revisions_checked=["2.11"], standard_files=[],
        )
        violations = cvc.validate_conformance_inventory("sff-8636", receipt)
        self.assertTrue(
            any(
                v.get("kind") == "claiming_211_comparison_from_212_only_input"
                for v in violations
            )
        )

    def test_hidden_unknown_constituent(self) -> None:
        entry = self._byte_entry(
            offset=2,
            byte_scoped_effect={"effect": cvc.EFFECT_ACK_STARTUP},
            unknown_constituents_required=[5],
        )
        receipt = self._receipt_with("sff-8636", [entry], revisions_checked=["2.12"])
        violations = cvc.validate_conformance_inventory("sff-8636", receipt)
        self.assertTrue(
            any(
                v.get("kind") == "acknowledging_startup_while_hiding_unknown_constituent"
                for v in violations
            )
        )


class TestReceiptHashAndCli(ConformanceTestBase):
    """Receipt hash coverage and CLI fail-closed behaviour."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def test_receipt_hash_covers_all_distinguishing_fields(self) -> None:
        """Mutating every distinguishing receipt field changes the hash:
        effect metadata, citations, revision sources, expanded bit markings,
        Unknown/uncovered masks, qualifications, byte-scoped effects, the
        closure file list, authority report identities and mismatches."""
        self.assertEqual(self._cli_write("cmis"), 0)
        receipt = self._receipt("cmis")
        base_hash = receipt["receipt_hash"]

        def mutate(fn) -> None:
            mutated = copy.deepcopy(receipt)
            fn(mutated)
            mutated.pop("receipt_hash", None)
            self.assertNotEqual(
                cvc.compute_receipt_hash(mutated), base_hash,
                f"hash did not change after {fn.__name__}",
            )

        def bit3_30(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            e["bits"][4]["read_effect"] = "none"

        mutate(bit3_30)  # read-effect metadata

        def citation(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            e["bits"][0]["authority"]["clause"] = "elsewhere"

        mutate(citation)

        def bit_marking(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            e["bits"][1]["schema_access"] = "rw"

        mutate(bit_marking)

        def unknown_mask(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "5.3"
            )
            e["unknown_mask"] = 0x00  # hide the Unknown constituents

        mutate(unknown_mask)

        def uncovered_mask(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 8
                and x.get("revision") == "5.3"
            )
            e["uncovered_mask"] = 0x08

        mutate(uncovered_mask)

        def qualification(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 9
                and x.get("revision") == "5.3"
            )
            e["qualification"]["readiness"] = "blocked"

        mutate(qualification)

        def byte_effect(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 9
                and x.get("revision") == "5.3"
            )
            e["byte_scoped_effect"]["effect"] = "none"

        mutate(byte_effect)

        def revision_source(r):
            r["standard_files"][0]["sha256"] = "f" * 64

        mutate(revision_source)

        def closure_files(r):
            r["schema_closure"]["files"][0]["sha256"] = "e" * 64

        mutate(closure_files)

        def authority_id(r):
            r["authority_reports"]["cmis-byte3-authority.json"] = "d" * 64

        mutate(authority_id)

        def mismatch_entry(r):
            r["mismatches"].append({"kind": "access_mismatch"})

        mutate(mismatch_entry)

        def whole_byte_qualified(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            e["whole_byte_qualified"] = True

        mutate(whole_byte_qualified)

        def bit_change_source(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            # Bit 0 (InterruptDeasserted) is module-dynamic; flip it.
            bit = next(b for b in e["bits"] if b["bit"] == 0)
            self.assertEqual(bit["schema_change_source"], "module")
            bit["schema_change_source"] = "host_or_neither"

        mutate(bit_change_source)  # per-bit change-source metadata

        def field_change_source(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "field" and x.get("field_name") == "identifier"
            )
            self.assertEqual(e["change_source"], "host_or_neither")
            e["change_source"] = "module"

        mutate(field_change_source)  # field-entry change-source metadata

        def revisions_checked(r):
            r["revisions_checked"] = ["3.0"]

        mutate(revisions_checked)  # the checked revision set

        def closure_hash(r):
            r["schema_closure"]["hash"] = "0" * 64

        mutate(closure_hash)  # the closure hash itself

        def revision_override(r):
            r["revision_override"] = "2.12"

        mutate(revision_override)  # a recorded revision override

        def bit_index(r):
            e = next(
                x for x in r["compared_inventory"]
                if x.get("kind") == "byte" and x.get("offset") == 3
                and x.get("revision") == "3.0"
            )
            e["bits"][0]["bit"] = 5

        mutate(bit_index)  # the per-bit index

    def test_change_source_recorded_in_receipts(self) -> None:
        """Receipts record change source - one of the five distinguishing
        axes - on every bit and every field entry (round-2 test blocker:
        'change source' was asserted nowhere)."""
        for family in ("cmis", "sff-8636", "sff-8472"):
            with self.subTest(family=family):
                self.assertEqual(self._cli_write(family), 0)
                receipt = self._receipt(family)
                byte_entries = [
                    e for e in receipt["compared_inventory"] if e.get("kind") == "byte"
                ]
                self.assertTrue(byte_entries)
                for e in byte_entries:
                    for bit in e["bits"]:
                        self.assertIn(bit["schema_change_source"], ("module", "host_or_neither"))
                field_entries = [
                    e for e in receipt["compared_inventory"] if e.get("kind") == "field"
                ]
                self.assertTrue(field_entries)
                for e in field_entries:
                    self.assertIn(e["change_source"], ("module", "host_or_neither"))
        # And the concrete values derive from flags.is_dynamic: the CMIS
        # module flags are module-dynamic, the reserved bits are not.
        receipt = self._receipt("cmis")
        byte8 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 8
            and e.get("revision") == "5.3"
        )
        bits8 = {b["bit"]: b for b in byte8["bits"]}
        self.assertEqual(bits8[0]["schema_change_source"], "module")
        self.assertEqual(bits8[4]["schema_change_source"], "host_or_neither")

    def test_check_fails_on_edited_receipt(self) -> None:
        """A receipt whose content was edited after hashing fails --check:
        tampering with a bit marking, and hiding the mismatches of a failing
        receipt, both break the recorded hash."""
        self.assertEqual(self._cli_write("cmis"), 0)
        path = self.receipts_dir / "cmis.json"
        receipt = json.loads(path.read_text(encoding="utf-8"))
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
            and e.get("revision") == "3.0"
        )
        entry["bits"][0]["schema_access"] = "rw"  # tamper, no rehash
        path.write_text(json.dumps(receipt, indent=2, sort_keys=True), encoding="utf-8")
        self.assertEqual(self._cli_check("cmis"), 1)

        # Hiding the mismatches of a failing receipt also breaks the hash.
        SyntheticFixtures.cmis_schema(self.root, include_byte3_reserved=False)
        self.assertEqual(self._cli_write("cmis"), 1)
        receipt = json.loads(path.read_text(encoding="utf-8"))
        self.assertTrue(receipt["mismatches"])
        receipt["mismatches"] = []
        path.write_text(json.dumps(receipt, indent=2, sort_keys=True), encoding="utf-8")
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_check_fails_on_missing_receipt(self) -> None:
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_check_fails_on_invalid_json_receipt(self) -> None:
        (self.receipts_dir / "cmis.json").write_text("{not json", encoding="utf-8")
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_check_needs_no_parsed_standards(self) -> None:
        """--check runs against a nonexistent standards directory."""
        for family in ("cmis", "sff-8636", "sff-8472"):
            self.assertEqual(self._cli_write(family), 0)
        self.assertEqual(
            self._cli_check("cmis,sff-8636,sff-8472",
                            standards_dir=self.root / "definitely-missing"),
            0,
        )

    def test_cli_help(self) -> None:
        """--help exits 0 and prints usage."""
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            with self.assertRaises(SystemExit) as cm:
                cvc.main(["--help"])
        self.assertEqual(cm.exception.code, 0)
        self.assertIn("volatility conformance receipts", out.getvalue())

    def test_bare_invocation_is_a_usage_error(self) -> None:
        """Running with neither --check nor --write is a usage error, not a
        silent zero-exit."""
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            with self.assertRaises(SystemExit) as cm:
                cvc.main([])
        self.assertEqual(cm.exception.code, 2)

    def test_empty_family_scope_refused(self) -> None:
        """An empty --family scope is refused: never a zero-work pass.

        The synthetic directory overrides keep these invocations off the
        ambient default repo paths.
        """
        for spec in ("", ",", "  "):
            with self.subTest(spec=spec):
                self.assertEqual(
                    _run_cli("--check", "--family", spec,
                             "--schemas-dir", str(self.schemas_dir),
                             "--standards-dir", str(self.standards_dir),
                             "--receipts-dir", str(self.receipts_dir),
                             "--repo-root", str(self.root)), 2)
                self.assertEqual(
                    _run_cli("--write", "--family", spec,
                             "--schemas-dir", str(self.schemas_dir),
                             "--standards-dir", str(self.standards_dir),
                             "--receipts-dir", str(self.receipts_dir),
                             "--repo-root", str(self.root)), 2)

    def test_unknown_family_refused(self) -> None:
        self.assertEqual(
            _run_cli("--check", "--family", "not-a-family",
                     "--schemas-dir", str(self.schemas_dir),
                     "--standards-dir", str(self.standards_dir),
                     "--receipts-dir", str(self.receipts_dir),
                     "--repo-root", str(self.root)), 2)

    def test_all_families_end_to_end(self) -> None:
        """The full default family set runs end to end and checks clean."""
        self.assertEqual(self._cli_write("cmis,sff-8636,sff-8472"), 0)
        self.assertEqual(
            _run_cli(
                "--check", "--family", "cmis,sff-8636,sff-8472",
                "--schemas-dir", str(self.schemas_dir),
                "--receipts-dir", str(self.receipts_dir),
                "--repo-root", str(self.root),
            ),
            0,
        )



class TestRound4Reproductions(ConformanceTestBase):
    """Regression tests for the five round-3 review blockers.

    Each one is the executable reproduction in
    /tmp/kcr-native/w0-rework/r4/repro/ driven through the public path
    (load_family_fields / build_receipt); each fails against the round-3
    tool and passes now.
    """

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def _mismatches(self, receipt: dict, kind: str) -> list[dict]:
        return [m for m in receipt["mismatches"] if m.get("kind") == kind]

    def test_r1_sff8472_zero_fields_never_passes(self) -> None:
        """Repro 1: an SFF-8472 family that loads zero fields with zero load
        errors failed only if the inventory was empty, but byte entries come
        from the standard: round 3 wrote a passing receipt."""
        _write(self.schemas_dir / "sff-8472" / "_index.yaml", "includes: []\n")
        fields, errors = cvc.load_family_fields("sff-8472", self.schemas_dir)
        self.assertEqual((fields, errors), ([], []))
        receipt = self._build("sff-8472")
        self.assertEqual(receipt["status"], "fail")
        self.assertTrue(self._mismatches(receipt, "empty_comparison"))

    def test_r2_conflicting_writable_block_rows_are_an_error(self) -> None:
        """Repro 2: two writable-block rows give byte 86 different
        operations; round 3 kept the later row silently and passed."""
        self._rewrite_standard(
            "sff-8636", "2.12",
            "| 86 | 1 | Read/Write | Control Register |",
            "| 86 | 1 | Write-Only | Control Register WO |\n"
            "| 86 | 1 | Read/Write | Control Register RW |",
        )
        receipt = self._build("sff-8636")
        self.assertEqual(receipt["status"], "fail")
        conflicts = [
            m for m in self._mismatches(receipt, "conflicting_standard_rows")
            if m.get("offset") == 86
        ]
        self.assertEqual(len(conflicts), 1)
        self.assertEqual(conflicts[0]["operations"], ["wo", "rw"])

    def test_r3_list_shaped_include_is_an_error(self) -> None:
        """Repro 3: an include whose top level is a list of field mappings
        was hashed but contributed zero fields and zero errors."""
        index = self.schemas_dir / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "  - lower-memory.yaml", "  - lower-memory.yaml\n  - extra-fields-list.yaml"
            ),
            encoding="utf-8",
        )
        _write(
            self.schemas_dir / "cmis" / "extra-fields-list.yaml",
            "- name: extra_field_in_list\n  offset: 100\n  size_bytes: 1\n"
            "  access: ro\n  flags:\n    is_dynamic: false\n",
        )
        receipt = self._build("cmis")
        self.assertIn(
            "cmis/extra-fields-list.yaml",
            [f["path"].split("schemas/", 1)[-1] for f in receipt["schema_closure"]["files"]],
        )
        self.assertEqual(receipt["status"], "fail")
        self.assertTrue([
            m for m in self._mismatches(receipt, "skipped_field_or_range")
            if m.get("include") == "extra-fields-list.yaml"
        ])

    def test_r4_nested_schema_reference_is_an_error(self) -> None:
        """Repro 4: a field_includes inside a field-includes fragment is in
        the hashed closure but the loader never reads it: round 3 passed."""
        frag = self._fields_file("cmis")
        frag.write_text(
            frag.read_text(encoding="utf-8") + "field_includes:\n  - nested-subpart.yaml\n",
            encoding="utf-8",
        )
        _write(
            frag.parent / "nested-subpart.yaml",
            "fields:\n  - name: nested_field_never_compared\n    offset: 105\n"
            "    size_bytes: 1\n    access: ro\n    flags:\n      is_dynamic: false\n",
        )
        self.assertTrue(any(
            p.name == "nested-subpart.yaml"
            for p in cvc.find_schema_closure_files("cmis", self.schemas_dir)
        ))
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertTrue([
            m for m in receipt["mismatches"] if "nested-subpart.yaml" in json.dumps(m)
        ])

    def test_r5_sff8472_byte_effect_none_needs_established_bits(self) -> None:
        """Repro 5: byte 111 as eight Read/write control bits carries no read
        effect authority; round 3 recorded byte_scoped_effect "none" and
        qualified the byte."""
        self._rewrite_standard(
            "sff-8472", "12.5a",
            "| 111 | 7-0 | Reserved | Reserved (was assigned to SFF-8079). |",
            "\n".join(
                f"| {'111' if b == 7 else ''} | {b} | Soft Control {b} | Read/write bit for control |"
                for b in range(7, -1, -1)
            ),
        )
        self._replace_in_fields(
            "sff-8472",
            "  - name: reserved_a2h_111\n    offset: 111\n    size_bytes: 1\n"
            "    template: reserved_templ\n",
            "".join(
                f"  - name: soft_ctrl_111_{b}\n    offset: 111\n    bit_offset: {b}\n"
                "    bit_width: 1\n    access: rw\n    flags:\n      is_dynamic: false\n"
                for b in range(8)
            ),
        )
        receipt = self._build("sff-8472")
        b111 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 111
        )
        self.assertEqual(len(b111["bits"]), 8)
        self.assertTrue(all(b["read_effect"] is None for b in b111["bits"]))
        self.assertNotEqual(b111["byte_scoped_effect"]["effect"], "none")
        self.assertIs(b111["whole_byte_qualified"], False)
        self.assertEqual(b111["qualification"]["readiness"], "blocked")
        self.assertIn("no established read effect", b111["qualification"]["reason"])


class TestRound4CompletenessInvariants(ConformanceTestBase):
    """One test per completeness invariant.  Each goes red when
    account_completeness (the accounting step run before a receipt's status
    is set) is disabled, e.g. by making it ``return []``."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)
        SyntheticFixtures.sff8636_schema(self.root)
        SyntheticFixtures.sff8472_schema(self.root)

    def _kinds_of(self, res: dict) -> set[str]:
        return {m.get("kind") for m in res["mismatches"]}

    def test_inv1_loaded_fields_must_equal_declared_fields(self) -> None:
        """Input accounting: whatever drops a declared field between the
        closure and the comparison - a loader defect included - is an
        unaccounted input, and the result fails."""
        self.assertEqual(self._build("cmis")["status"], "pass")
        report = cvc.load_authority_report(self.authority_dir, "cmis")
        fields, errors = cvc.load_family_fields("cmis", self.schemas_dir)
        dropped = next(f for f in fields if f.name == "low_pwr_control")
        res = cvc.compare_cmis(
            self.schemas_dir, self.standards_dir, report,
            fields=[f for f in fields if f is not dropped], load_errors=errors,
        )
        self.assertEqual(res["status"], "fail")
        unaccounted = [
            m for m in res["mismatches"] if m.get("kind") == "unaccounted_schema_input"
        ]
        self.assertEqual([m["decl"] for m in unaccounted], [dropped.decl])
        # Zero compared fields fail as an empty comparison.
        res = cvc.compare_cmis(
            self.schemas_dir, self.standards_dir, report, fields=[], load_errors=[],
        )
        self.assertIn("empty_comparison", self._kinds_of(res))
        # An unsupported per-revision override form fails explicitly.
        self._replace_in_fields(
            "cmis", "    offset: 26\n",
            "    offset: 26\n    changed_in:\n      - rev: \"5.0\"\n        access: rw\n",
        )
        receipt = self._build("cmis")
        self.assertIn(
            ("unaccounted_schema_input", "low_pwr_control"),
            {(m.get("kind"), m.get("field")) for m in receipt["mismatches"]},
        )

    def test_inv2_applicable_revision_without_typed_rows_is_unresolved(self) -> None:
        """Applicability: a field without since/until applies at every
        revision; where a revision has no row (or only untyped rows) for it,
        the cell is unresolved and the receipt fails - round 3 recorded
        absent_from_standard and passed.  since/until (until exclusive, as in
        the codegen's applicable_revs) is the only exclusion."""
        row30 = "| 26 | 7-0 | LowPwrControl | Low power control | RW Rqd. |"
        self._rewrite_standard("cmis", "3.0", row30, "")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        cells = {
            (m.get("field"), m.get("revision")) for m in receipt["mismatches"]
            if m.get("kind") == "unresolved_at_applicable_revision"
        }
        self.assertEqual(cells, {("low_pwr_control", "3.0")})
        # Only untyped rows at the revision: also unresolved.
        self._restore_standard("cmis", "3.0")
        self._rewrite_standard(
            "cmis", "3.0", row30, "| 26 | 7-0 | LowPwrControl | Low power control | - |"
        )
        receipt = self._build("cmis")
        self.assertIn(
            ("low_pwr_control", "3.0"),
            {(m.get("field"), m.get("revision")) for m in receipt["mismatches"]
             if m.get("kind") == "unresolved_at_applicable_revision"},
        )
        # An established revision marking excludes the revision: passes.
        self._replace_in_fields("cmis", "    offset: 26\n", '    offset: 26\n    since: "4.0"\n')
        self.assertEqual(self._build("cmis")["status"], "pass")
        # until is exclusive: until 5.0 excludes 5.0..5.3, which do have rows,
        # and the field is then unresolved nowhere.
        self._restore_standard("cmis", "3.0")
        self._replace_in_fields("cmis", 'since: "4.0"', 'until: "5.0"')
        receipt = self._build("cmis")
        entry = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "field" and e.get("field_name") == "low_pwr_control"
        )
        self.assertEqual(entry["expected"]["5.0"]["status"], "not_applicable_at_revision")
        self.assertEqual(entry["expected"]["4.0"]["status"], "conformant")

    def test_inv3_no_default_authority(self) -> None:
        """No default authority: a known read effect with no cited standard
        source, or a byte-scoped "none" without eight established "none"
        constituents, is an error - wherever the comparator produced it."""
        report = cvc.load_authority_report(self.authority_dir, "sff-8472")
        fields, errors = cvc.load_family_fields("sff-8472", self.schemas_dir)
        res = cvc.compare_sff8472(self.schemas_dir, self.standards_dir, report,
                                  fields=fields, load_errors=errors)
        self.assertEqual(res["status"], "pass")
        b111 = next(
            e for e in res["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 111
        )
        # The round-3 shape: "none" over constituents with no established
        # effect, and a bit asserting "none" with no cited source.
        b111["byte_scoped_effect"] = {"effect": "none", "scope": "byte"}
        b111["bits"][0]["read_effect"] = "none"
        b111["bits"][0]["authority"] = {"note": "no source"}
        res["mismatches"] = []
        violations = cvc.account_completeness(
            "sff-8472", self.schemas_dir, fields, errors, res
        )
        unestablished = [v for v in violations if v.get("kind") == "unestablished_authority"]
        self.assertEqual(
            sorted(((v.get("offset"), v.get("bit")) for v in unestablished), key=str),
            [(111, 0), (111, None)],
        )
        self.assertIs(b111["whole_byte_qualified"], False)

    def test_inv4_order_dependent_resolution_is_an_error(self) -> None:
        """No order-dependent resolution: overlapping schema fields with
        disagreeing markings outside the bit-expanded bytes, and one template
        name defined differently by an imported and the family index, are
        errors (round 3 compared each overlap separately and let the family
        template win by update order)."""
        self._replace_in_fields(
            "cmis", "  - name: identifier\n",
            "  - name: identifier_alias\n    offset: 0\n    size_bytes: 1\n"
            "    access: ro\n    flags:\n      is_dynamic: true\n"
            "  - name: identifier\n",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn(
            ("overlapping_fields_disagree", 0),
            {(m.get("kind"), m.get("offset")) for m in receipt["mismatches"]},
        )
        SyntheticFixtures.cmis_schema(self.root)
        index = self.schemas_dir / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "templates:\n", "templates:\n  inherited_ro:\n    access: ro\n"
                "    flags:\n      is_dynamic: true\n", 1,
            ),
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertIn(
            ("conflicting_authority", "inherited_ro"),
            {(m.get("kind"), m.get("template")) for m in receipt["mismatches"]},
        )

    def test_inv5_whole_byte_qualification_needs_full_evidence(self) -> None:
        """Whole-byte qualification: all eight constituents compared, each
        with established, compatible access and effect, and no mismatch on
        the byte.  Round 3 qualified a byte holding a mismatching bit."""
        receipt = self._build("sff-8472")
        byte110 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 110
        )
        self.assertIs(byte110["whole_byte_qualified"], True)
        self._replace_in_fields(
            "sff-8472",
            "  - name: soft_tx_disable_select\n    offset: 110\n    bit_offset: 6\n"
            "    bit_width: 1\n    access: rw\n",
            "  - name: soft_tx_disable_select\n    offset: 110\n    bit_offset: 6\n"
            "    bit_width: 1\n    access: ro\n",
        )
        receipt = self._build("sff-8472")
        byte110 = next(
            e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 110
        )
        self.assertIs(byte110["whole_byte_qualified"], False)
        self.assertEqual(byte110["qualification"]["readiness"], "blocked")
        self.assertIn("bit 6 comparison status is 'mismatch'",
                      byte110["qualification"]["reason"])
        # The receipt validator re-derives the claim: an edited qualification
        # is refused.
        byte110["whole_byte_qualified"] = True
        self.assertIn(
            "unjustified_whole_byte_qualification",
            {v.get("kind") for v in cvc.validate_conformance_inventory("sff-8472", receipt)},
        )


class TestRound5CompletenessHoles(ConformanceTestBase):
    """Round-4 coherent review: two blockers and two advisories.  Each test
    fails against the round-4 tool (/tmp/kcr-native/w0-review-r5/files/)."""

    def setUp(self) -> None:
        super().setUp()
        SyntheticFixtures.cmis_schema(self.root)

    def _add_import(self, payload: str, extra: dict[str, str] | None = None) -> None:
        index = self.schemas_dir / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "imports:\n  - ../sff-8024/_index.yaml\n",
                "imports:\n  - ../sff-8024/_index.yaml\n  - shared-marks.yaml\n",
            ),
            encoding="utf-8",
        )
        _write(self.schemas_dir / "cmis" / "shared-marks.yaml", payload)
        for name, text in (extra or {}).items():
            _write(self.schemas_dir / "cmis" / name, text)

    def _import_errors(self) -> list[dict]:
        return [
            e for e in cvc.schema_declarations("cmis", self.schemas_dir)["errors"]
            if e.get("kind") == "unaccounted_schema_input"
            and e.get("decl_file") == "cmis/shared-marks.yaml"
        ]

    def test_z5_imported_file_declaring_fields_is_unaccounted(self) -> None:
        """Blocker 1: a closure file reached through the index's imports that
        declares fields as a top-level list, or as a dict carrying only
        field_includes, was neither declared nor reported (the reviewer's
        input; repro /tmp/kcr-native/w0-rework/r5/repro/)."""
        field = ("- name: extra_field\n  offset: 100\n  size_bytes: 1\n  access: ro\n"
                 "  flags: {is_dynamic: false}\n")
        # Top-level list form.
        self._add_import(field)
        self.assertIn(
            "shared-marks.yaml",
            [p.name for p in cvc.find_schema_closure_files("cmis", self.schemas_dir)],
        )
        self.assertEqual(len(self._import_errors()), 1)
        self.assertEqual(
            cvc.schema_declarations("cmis", self.schemas_dir)["classification"][
                "cmis/shared-marks.yaml"],
            "index_or_import",
        )
        # Dict carrying only field_includes, aimed at a fragment a page also
        # includes: the fragment's fields are compared through the page, so
        # only the import's own declaration is never loaded - round 4 passed.
        SyntheticFixtures.cmis_schema(self.root)
        self._add_import("field_includes:\n  - fields/lower-memory-part-01.yaml\n")
        self.assertEqual(len(self._import_errors()), 1)
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertTrue([
            m for m in receipt["mismatches"]
            if m.get("kind") == "unaccounted_schema_input"
            and m.get("decl_file") == "cmis/shared-marks.yaml"
        ])
        # The same form naming a file nothing else loads.
        SyntheticFixtures.cmis_schema(self.root)
        self._add_import("field_includes:\n  - more.yaml\n", {"more.yaml": "fields:\n" + "".join(
            "  " + line + "\n" for line in field.splitlines())})
        self.assertEqual(len(self._import_errors()), 1)
        self.assertEqual(self._build("cmis")["status"], "fail")

    def test_z5_cmis_30_40_typed_row_without_read_effect_passage_is_unknown(self) -> None:
        """Blocker 2: a CMIS 3.0/4.0 typed row with no read-effect passage of
        its revision has an unknown read effect, not an established "none",
        and cannot qualify a byte (round 4 qualified it outright)."""
        std = cvc.parse_standard_file(
            self.standards_dir / cvc.STANDARD_FILENAMES[("cmis", "3.0")], "cmis")
        row = next(r for r in std.rows if r.byte_lo == 0)
        exp = cvc._cmis_row_expectation(std, "3.0", row, None, [])
        self.assertEqual(exp["access"], "ro")
        self.assertEqual(exp["read_effect"], "unknown")
        self.assertFalse(exp["effect_established"])
        # Type all eight bits of byte 3 RO at 3.0 and mark them ro: every bit
        # conforms, yet no 3.0 passage establishes a side-effect-free read.
        self._rewrite_standard(
            "cmis", "3.0", "| 3 | 7-4 | - | Reserved |  |",
            "| 3 | 7-4 | StatusHigh | Status bits | RO RQD |",
        )
        SyntheticFixtures.cmis_schema(self.root, byte3_reserved_access="ro")
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "pass")
        by_rev = {
            e["revision"]: e for e in receipt["compared_inventory"]
            if e.get("kind") == "byte" and e.get("offset") == 3
        }
        for rev in ("3.0", "4.0"):
            with self.subTest(rev=rev):
                e = by_rev[rev]
                for b in e["bits"][:4] if rev == "4.0" else e["bits"]:
                    self.assertEqual(b["read_effect"], "unknown", (rev, b["bit"]))
                    self.assertFalse(b["effect_established"])
                    self.assertEqual(b["status"], "conformant")
                self.assertIs(e["whole_byte_qualified"], False)
                self.assertEqual(e["qualification"]["readiness"], "blocked")
                self.assertIn("no established read effect", e["qualification"]["reason"])
        # 5.x still rests on its own READ side-effect clause, as before.
        self.assertEqual(by_rev["5.0"]["qualification"]["readiness"], "qualified_with_inference")

    def test_z5_check_reruns_completeness_accounting(self) -> None:
        """Advisory: --check re-derives the accounting, so a receipt edited
        to drop an accounting error (rehashed to be self-consistent) fails."""
        self._rewrite_standard(
            "cmis", "3.0", "| 26 | 7-0 | LowPwrControl | Low power control | RW Rqd. |", "")
        self.assertEqual(self._cli_write("cmis"), 1)
        path = self.receipts_dir / "cmis.json"
        receipt = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(
            [m["kind"] for m in receipt["mismatches"]], ["unresolved_at_applicable_revision"])
        receipt["mismatches"] = []
        receipt["status"] = "pass"
        receipt["receipt_hash"] = cvc.compute_receipt_hash(receipt)
        path.write_text(json.dumps(receipt, indent=2, sort_keys=True), encoding="utf-8")
        self.assertEqual(self._cli_check("cmis"), 1)
        self.assertEqual(
            [v["kind"] for v in cvc.recheck_accounting("cmis", self.schemas_dir, receipt)],
            ["unresolved_at_applicable_revision"],
        )
        # An inventory edited to omit a declared field's entry also fails.
        self._restore_standard("cmis", "3.0")
        self.assertEqual(self._cli_write("cmis"), 0)
        receipt = json.loads(path.read_text(encoding="utf-8"))
        receipt["compared_inventory"] = [
            e for e in receipt["compared_inventory"]
            if e.get("field_name") != "low_pwr_control"
        ]
        receipt["receipt_hash"] = cvc.compute_receipt_hash(receipt)
        path.write_text(json.dumps(receipt, indent=2, sort_keys=True), encoding="utf-8")
        self.assertEqual(self._cli_check("cmis"), 1)

    def test_z5_duplicate_include_is_an_error(self) -> None:
        """Advisory: a page listed twice loads one occurrence twice under one
        uid; round 4's set-based accounting could not see it."""
        index = self.schemas_dir / "cmis" / "_index.yaml"
        index.write_text(
            index.read_text(encoding="utf-8").replace(
                "  - lower-memory.yaml\n", "  - lower-memory.yaml\n  - lower-memory.yaml\n"),
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        dup = [
            m for m in receipt["mismatches"]
            if m.get("kind") == "unaccounted_schema_input" and m.get("occurrences") == 2
        ]
        self.assertIn("low_pwr_control", {m["field"] for m in dup})
        # A fragment listed twice in one page's field_includes: the same.
        SyntheticFixtures.cmis_schema(self.root)
        page = self.schemas_dir / "cmis" / "lower-memory.yaml"
        page.write_text(
            page.read_text(encoding="utf-8").replace(
                "  - fields/lower-memory-part-01.yaml\n",
                "  - fields/lower-memory-part-01.yaml\n  - fields/lower-memory-part-01.yaml\n"),
            encoding="utf-8",
        )
        receipt = self._build("cmis")
        self.assertEqual(receipt["status"], "fail")
        self.assertTrue([
            m for m in receipt["mismatches"]
            if m.get("kind") == "unaccounted_schema_input" and m.get("occurrences") == 2
        ])

    def test_z5_fragment_with_non_memory_map_kind_is_declared(self) -> None:
        """Advisory: the loader loads a field_includes fragment's fields
        whatever its kind, so a fragment labelled with a non-memory-map kind
        is declared too - its compared fields are not misreported as
        unaccounted input (round 4 failed the family)."""
        frag = self._fields_file("cmis")
        frag.write_text("kind: enum\n" + frag.read_text(encoding="utf-8"), encoding="utf-8")
        receipt = self._build("cmis")
        self.assertNotIn(
            "unaccounted_schema_input", {m.get("kind") for m in receipt["mismatches"]})
        self.assertEqual(receipt["status"], "pass")
        decls = cvc.schema_declarations("cmis", self.schemas_dir)
        self.assertEqual(
            decls["classification"]["cmis/fields/lower-memory-part-01.yaml"],
            "field_includes_fragment",
        )


if __name__ == "__main__":
    unittest.main()
