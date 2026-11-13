import type { Sff8636DecodeResult } from "./types.js";
import { checkPage, checksumBlock, hex, hexBytes, hexNibble, joinDateCode, oui, roundTo } from "./common.js";
import { decodeSff8636LowerMemory } from "../generated/sff8636_lower_memory.js";
import { decodeSff8636Page00hUpperAdministrative } from "../generated/sff8636_page00h_upper_administrative.js";
import { Sff8024IdentifierLabel } from "../generated/enums/sff8024_identifier.js";
import { Sff8024ConnectorTypeLabel } from "../generated/enums/sff8024_connector_type.js";
import { Sff8024EncodingLabel } from "../generated/enums/sff8024_encoding.js";
import { Sff8636DeviceTechnologyLabel } from "../generated/enums/sff8636_device_technology.js";

// SFF-8636 Page 00h byte 192 (buffer offset 64) extended specification
// compliance labels; mirrors _SFF8636_EXT_SPEC in
// bindings/python/openxcvr/_common.py ("unknown" for unmapped values).
const EXT_SPEC_LABELS: Record<number, string> = {
  0x02: "base_100g_sr4_or_25g_sr",
};

export interface Sff8636Pages {
  lower: Uint8Array;
  page00h: Uint8Array;
  /** Upper Page 03h threshold window. Required by evaluateSff8636Thresholds;
   * decodeSff8636 does not read it. Absent means not supplied — never fabricated. */
  page03h?: Uint8Array;
}

/**
 * Decode SFF-8636 (QSFP+/QSFP28) lower memory + Page 00h into the shared
 * golden JSON shape. Field-for-field port of the Python facade
 * decode_sff8636 (bindings/python/openxcvr/_sff8636.py); the result is
 * ok-only — a missing or short page throws instead of producing a rejected
 * document.
 */
export function decodeSff8636(pages: Sff8636Pages): Sff8636DecodeResult {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }

  const lowerBuf = checkPage("lower", pages.lower, true)!;
  const page00hBuf = checkPage("page00h", pages.page00h, true)!;

  const lower = decodeSff8636LowerMemory(lowerBuf);
  const page00h = decodeSff8636Page00hUpperAdministrative(page00hBuf);

  const extSpec = page00h.extSpecCompliance !== 0
    ? {
        name: EXT_SPEC_LABELS[page00h.extSpecCompliance] ?? "unknown",
        raw: hex(page00h.extSpecCompliance),
      }
    : null;

  const bitsSet: Sff8636DecodeResult["compliance"]["bits_set"] = [];
  if (page00h.complianceExtended) {
    bitsSet.push("compliance_extended");
  }
  if (page00h.compliance40gbaseSr4) {
    bitsSet.push("compliance_40gbase_sr4");
  }

  // SFF-8636 § 6.2.3: IntL is deasserted when byte 2 bit 1 is set; any tx/rx
  // LOS flag or lane alarm/warning flag (bytes 9-14) also counts. Those
  // flags are generated as booleans, so presence is truthiness, not a
  // numeric nibble compare.
  const interruptAsserted =
    !lower.interruptDeasserted ||
    lower.txLosFlag.some(Boolean) ||
    lower.rxLosFlag.some(Boolean) ||
    [
      ...lower.rxPowerAlarmWarningFlag,
      ...lower.txBiasAlarmWarningFlag,
      ...lower.txPowerAlarmWarningFlag,
    ].some(Boolean);

  let moduleType: string | null = null;
  if (page00h.extSpecCompliance === 0x02) {
    moduleType = "100GBASE-SR4 QSFP28";
  } else if (page00h.compliance40gbaseSr4) {
    moduleType = "40GBASE-SR4 QSFP+";
  }

  // SFF-8636 § 5.6: byte 12 == 0xFF is a sentinel; the high-resolution rate
  // is byte 94 in 250 MBd units (divide by 4 for GBd).
  const nominalRate =
    page00h.nominalSignalingRate === 0xff
      ? page00h.extBaudRate / 4
      : page00h.nominalSignalingRate / 10;

  return {
    decode_status: "ok",
    identity: {
      date_code: joinDateCode(page00h.dateCodeYear, page00h.dateCodeMonth, page00h.dateCodeDay),
      vendor_name: page00h.vendorName,
      vendor_oui: oui(page00h.vendorOui),
      vendor_pn: page00h.vendorPn,
      vendor_rev: page00h.vendorRev,
      vendor_sn: page00h.vendorSn,
    },
    module: {
      cmis_revision: null,
      connector: {
        name: Sff8024ConnectorTypeLabel(page00h.connectorType),
        raw: hex(page00h.connectorType),
      },
      encoding: {
        name: Sff8024EncodingLabel(page00h.encoding),
        raw: hex(page00h.encoding),
      },
      extended_identifier: hex(
        page00h.powerClassLow |
          (page00h.powerClassHigh << 6) |
          (page00h.rxCdrPresent ? 1 << 2 : 0) |
          (page00h.txCdrPresent ? 1 << 3 : 0) |
          (page00h.cleiPresent ? 1 << 4 : 0) |
          (page00h.powerClass8Implemented ? 1 << 5 : 0),
      ),
      identifier: {
        name: Sff8024IdentifierLabel(lower.sff8024Identifier),
        raw: hex(lower.sff8024Identifier),
      },
      memory_model: lower.flatMemory ? "flat" : "paged",
      module_state: null,
      module_type: moduleType,
      nominal_bit_rate_gbps: nominalRate,
    },
    monitoring: {
      module_temperature_c: lower.temperature / 256,
      module_voltage_v: roundTo(lower.supplyVoltage * 0.0001, 4),
    },
    compliance: {
      bits_set: bitsSet,
      device_technology: {
        name: Sff8636DeviceTechnologyLabel(page00h.deviceTechnology),
        raw: hexNibble(page00h.deviceTechnology),
      },
      ext_spec_byte_192: extSpec,
      raw_bytes_page_00h_3_10: hexBytes(page00hBuf.subarray(3, 11)),
      reach: {
        om1_m: page00h.lengthOm1OrCopperAttenuation25p78ghz,
        om2_m: page00h.lengthOm21m,
        om3_2m_units: page00h.lengthOm32m * 2,
        om4_or_copper_m: page00h.lengthCableOrOm4,
        smf_km: page00h.lengthSmfKm,
      },
    },
    flags: {
      data_not_ready: lower.dataNotReady,
      interrupt_asserted: interruptAsserted,
    },
    lanes: [0, 1, 2, 3].map((i) => ({
      index: i + 1,
      rx_power_mw: roundTo(lower.rxPowerLane[i] * 0.0001, 4),
      tx_bias_ma: roundTo(lower.txBiasLane[i] * 0.002, 3),
      tx_power_mw: roundTo(lower.txPowerLane[i] * 0.0001, 4),
    })),
    checksums: {
      cc_base: checksumBlock(page00hBuf, 0, 63, 63),
      cc_ext: checksumBlock(page00hBuf, 64, 95, 95),
    },
  };
}
