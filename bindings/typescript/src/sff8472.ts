import type { Sff8472DecodeResult } from "./types.js";
import { checkPagePadded, checksumBlock, hex, hexBytes, oui, roundTo } from "./common.js";
import { decodePageA0hBaseMemory } from "../generated/page_a0h_base_memory.js";
import { decodePageA2hDiagnosticsBase } from "../generated/page_a2h_diagnostics_base.js";
import { Sff8024IdentifierLabel } from "../generated/enums/sff8024_identifier.js";
import { Sff8024ConnectorTypeLabel } from "../generated/enums/sff8024_connector_type.js";
import { Sff8024EncodingLabel } from "../generated/enums/sff8024_encoding.js";

export interface Sff8472Pages {
  a0h: Uint8Array;
  a2h: Uint8Array;
}

/**
 * Decode SFF-8472 (SFP/SFP+) A0h base memory + A2h diagnostics pages into the
 * shared golden JSON shape. Field-for-field port of the Python facade
 * decode_sff8472 (bindings/python/openxcvr/_sff8472.py); the result is
 * ok-only. An A0h or A2h page shorter than 256 bytes is zero-padded,
 * exactly like the Python _pad_page helper; only a missing or
 * non-Uint8Array page throws instead of producing a rejected document.
 * (The CMIS and SFF-8636 page checks are stricter: they reject a page
 * under 128 bytes.)
 *
 * Telemetry in this result is uncalibrated: SFF-8472 external calibration
 * constants are not applied. Check calibration.externally_calibrated;
 * calibrated values come from the Rust SFF-8472 adapter or the observation
 * APIs.
 */
export function decodeSff8472(pages: Sff8472Pages): Sff8472DecodeResult {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }

  const a0hBuf = checkPagePadded("a0h", pages.a0h, 256);
  const a2hBuf = checkPagePadded("a2h", pages.a2h, 256);

  const a0h = decodePageA0hBaseMemory(a0hBuf);
  const a2h = decodePageA2hDiagnosticsBase(a2hBuf);

  const moduleType =
    a0h.transceiverCompliance.length > 3 && a0h.transceiverCompliance[3]
      ? "10GBASE-SR SFP+"
      : null;
  const alarmFlags = a2h.alarmWarningFlag;

  // SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is a sentinel
  // for rates > 25.4 GBd; the actual high-resolution rate is in byte 66 (250 MBd
  // units → divide by 4 to express in GBd).
  const nominalBitRateGbps =
    a0h.signalingRateNominal === 0xff
      ? a0h.signalingRateMax / 4
      : a0h.signalingRateNominal / 10;

  // SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When byte 92
  // bit 6 (digital_diagnostics_implemented) is clear, A2h holds no meaningful
  // monitoring/flag/lane data — emit null rather than zero-filled structures.
  const ddm = a0h.digitalDiagnosticsImplemented;

  const flags: Sff8472DecodeResult["flags"] = ddm
    ? {
        alarm_summary: {
          rx_power_high_alarm: alarmFlags[8],
          rx_power_low_alarm: alarmFlags[9],
          temp_high_alarm: alarmFlags[0],
          temp_low_alarm: alarmFlags[1],
          tx_bias_high_alarm: alarmFlags[4],
          tx_bias_low_alarm: alarmFlags[5],
          tx_power_high_alarm: alarmFlags[6],
          tx_power_low_alarm: alarmFlags[7],
          vcc_high_alarm: alarmFlags[2],
          vcc_low_alarm: alarmFlags[3],
        },
        warning_summary: {
          rx_power_high_warning: alarmFlags[24],
          rx_power_low_warning: alarmFlags[25],
          temp_high_warning: alarmFlags[16],
          temp_low_warning: alarmFlags[17],
          tx_bias_high_warning: alarmFlags[20],
          tx_bias_low_warning: alarmFlags[21],
          tx_power_high_warning: alarmFlags[22],
          tx_power_low_warning: alarmFlags[23],
          vcc_high_warning: alarmFlags[18],
          vcc_low_warning: alarmFlags[19],
        },
      }
    : null;

  const monitoring: Sff8472DecodeResult["monitoring"] = ddm
    ? {
        module_temperature_c: a2h.temperature / 256,
        module_voltage_v: roundTo(a2h.supplyVoltage * 0.0001, 4),
      }
    : null;

  const lanes: Sff8472DecodeResult["lanes"] = ddm
    ? [
        {
          index: 1,
          rx_los: a2h.rxLosState,
          rx_power_mw: roundTo(a2h.rxPower * 0.0001, 4),
          tx_bias_ma: roundTo(a2h.txBias * 0.002, 3),
          tx_disable: a2h.txDisableState,
          tx_fault: a2h.txFaultState,
          tx_power_mw: roundTo(a2h.txPower * 0.0001, 4),
        },
      ]
    : null;

  return {
    decode_status: "ok",
    identity: {
      date_code: a0h.dateCode.slice(0, 6),
      vendor_name: a0h.vendorName,
      vendor_oui: oui(a0h.vendorOui),
      vendor_pn: a0h.vendorPn,
      vendor_sn: a0h.vendorSn,
    },
    module: {
      calibration: {
        ddm_implemented: a0h.digitalDiagnosticsImplemented,
        externally_calibrated: a0h.externallyCalibrated,
        internally_calibrated: a0h.internallyCalibrated,
        rx_power_kind: a0h.rxPowerAverage ? "average" : "oma",
      },
      cmis_revision: null,
      connector: {
        name: Sff8024ConnectorTypeLabel(a0h.connectorType),
        raw: hex(a0h.connectorType),
      },
      encoding: {
        name: Sff8024EncodingLabel(a0h.encoding),
        raw: hex(a0h.encoding),
      },
      identifier: {
        name: Sff8024IdentifierLabel(a0h.sff8024Identifier),
        raw: hex(a0h.sff8024Identifier),
      },
      memory_model: null,
      module_state: null,
      module_type: moduleType,
      nominal_bit_rate_gbps: nominalBitRateGbps,
    },
    monitoring,
    compliance: {
      raw_bytes_a0h_3_10: hexBytes(a0hBuf.subarray(3, 11)),
      reach: {
        om1_m: a0h.lengthOm110m * 10,
        om2_m: a0h.lengthOm210m * 10,
        // SFF-8472 byte 19 is OM3 length in units of 10 m (classic flat byte;
        // base|multiplier<<6 == the raw byte). ×10 yields meters. This is
        // deliberate: the schema's base(u6)+multiplier(u2 enum) split is not
        // the wire semantics — finisar (byte19=30, golden=300) falsifies the
        // ×0.1 reading. Do not "fix" to a fractional multiplier.
        om3_m: (a0h.lengthOm3Base | (a0h.lengthOm3Multiplier << 6)) * 10,
        om4_m: a0h.lengthOm4OrCopper1m,
        smf_km: a0h.lengthSmfKm,
      },
      wavelength_nm: a0h.wavelengthOrCableCompliance,
    },
    flags,
    lanes,
    checksums: {
      cc_base: checksumBlock(a0hBuf, 0, 63, 63),
      cc_dmi: checksumBlock(a2hBuf, 0, 95, 95),
      cc_ext: checksumBlock(a0hBuf, 64, 95, 95),
    },
  };
}
