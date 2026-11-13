// Lane-signal flags and threshold evaluation for the SFF facades.
// Field-for-field ports of the Python/Go entry points
// (bindings/python/openxcvr/_sff8636.py, _sff8472.py and
// bindings/go/sff_lane_flags.go, sff_thresholds.go). Result keys match
// those facades. A missing required page throws; Page 03h is never
// zero-padded (a fabricated zero page would disable every threshold).

import type { Sff8636Pages } from "./sff8636.js";
import type { Sff8472Pages } from "./sff8472.js";
import { checkPagePadded } from "./common.js";
import { decodeSff8636LowerMemory } from "../generated/sff8636_lower_memory.js";
import { decodeSff8636Page03hThresholdsAndControls } from "../generated/sff8636_page03h_thresholds_and_controls.js";
import { decodePageA0hBaseMemory } from "../generated/page_a0h_base_memory.js";
import { decodePageA2hDiagnosticsBase } from "../generated/page_a2h_diagnostics_base.js";

export interface ThresholdEvaluation {
  high_alarm: boolean;
  low_alarm: boolean;
  high_warning: boolean;
  low_warning: boolean;
}

export interface Sff8636LaneSignalFlag {
  index: number;
  rx_los: boolean;
  tx_disable: boolean;
  tx_fault: boolean;
  tx_los: boolean;
  tx_cdr_lol: boolean;
  rx_cdr_lol: boolean;
  tx_adaptive_eq_fault?: boolean;
}

export interface Sff8472LaneSignalFlag {
  index: number;
  rx_los: boolean;
  tx_disable: boolean;
  tx_fault: boolean;
  tx_adaptive_eq_fault?: boolean;
}

const SFF8636_COMPLIANCE_ADMITTED = new Set([0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a]);
const SFF8472_COMPLIANCE_ADMITTED = new Set([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a]);

function thresholdBounds(raw: number[], scale: number): Array<number | null> {
  if (raw.every((value) => value === 0)) {
    return [null, null, null, null];
  }
  return raw.map((value) => value * scale);
}

function thresholdEvaluation(measurement: number, bounds: Array<number | null>): ThresholdEvaluation {
  const [highAlarm, lowAlarm, highWarning, lowWarning] = bounds;
  const tripHigh = (bound: number | null) => bound !== null && measurement >= bound;
  const tripLow = (bound: number | null) => bound !== null && measurement <= bound;
  return {
    high_alarm: tripHigh(highAlarm),
    low_alarm: tripLow(lowAlarm),
    high_warning: tripHigh(highWarning),
    low_warning: tripLow(lowWarning),
  };
}

/**
 * Decode the four SFF-8636 lanes' signal flags into the unified lane
 * vocabulary (1-based `index`). Adaptive-EQ is omitted for compliance
 * bytes 0x03-0x05. A missing `lower` page throws; a short page is
 * zero-padded to 128 bytes, matching the Python and Go decoders.
 */
export function decodeSff8636LaneSignalFlags(pages: Sff8636Pages): Sff8636LaneSignalFlag[] {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }
  const lower = decodeSff8636LowerMemory(checkPagePadded("lower", pages.lower, 128));
  const adaptiveEqModeled =
    lower.revisionCompliance >= 0x06 || !SFF8636_COMPLIANCE_ADMITTED.has(lower.revisionCompliance);
  const lanes: Sff8636LaneSignalFlag[] = [];
  for (let i = 0; i < 4; i++) {
    const entry: Sff8636LaneSignalFlag = {
      index: i + 1,
      rx_los: lower.rxLosFlag[i],
      tx_disable: lower.txDisableLane[i],
      tx_fault: lower.txFaultFlag[i],
      tx_los: lower.txLosFlag[i],
      tx_cdr_lol: lower.txCdrLolFlag[i],
      rx_cdr_lol: lower.rxCdrLolFlag[i],
    };
    if (adaptiveEqModeled) {
      entry.tx_adaptive_eq_fault = lower.txAdaptiveEqFaultFlag[i];
    }
    lanes.push(entry);
  }
  return lanes;
}

/**
 * Decode the single SFF-8472 channel's signal flags. Tx LOS and CDR LOL
 * are not part of this family and are omitted. Adaptive-EQ is omitted
 * unless compliance is 0x0A (rev 12.5) or a value this family does not
 * admit. Missing `a0h` or `a2h` throws; a short page is zero-padded to
 * 256 bytes.
 */
export function decodeSff8472LaneSignalFlags(pages: Sff8472Pages): Sff8472LaneSignalFlag[] {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }
  const a0h = decodePageA0hBaseMemory(checkPagePadded("a0h", pages.a0h, 256));
  const a2h = decodePageA2hDiagnosticsBase(checkPagePadded("a2h", pages.a2h, 256));
  const entry: Sff8472LaneSignalFlag = {
    index: 1,
    rx_los: a2h.rxLosState,
    tx_disable: a2h.txDisableState,
    tx_fault: a2h.txFaultState,
  };
  if (a0h.sff8472Compliance === 0x0a || !SFF8472_COMPLIANCE_ADMITTED.has(a0h.sff8472Compliance)) {
    entry.tx_adaptive_eq_fault = a2h.adaptiveInputEqFailFlag;
  }
  return [entry];
}

export interface Sff8636ThresholdResult {
  temperature: ThresholdEvaluation;
  supply_voltage: ThresholdEvaluation;
  rx_power: ThresholdEvaluation[];
  tx_bias: ThresholdEvaluation[];
  tx_power: ThresholdEvaluation[];
}

/**
 * Evaluate SFF-8636 monitors against Upper Page 03h. `page03h` must be
 * supplied by the caller and at least 128 bytes; it is never zero-filled.
 */
export function evaluateSff8636Thresholds(pages: Sff8636Pages): Sff8636ThresholdResult {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }
  const lower = decodeSff8636LowerMemory(checkPagePadded("lower", pages.lower, 128));
  const raw = pages.page03h;
  if (!(raw instanceof Uint8Array) || raw.length < 128) {
    const got = raw instanceof Uint8Array ? raw.length : 0;
    throw new RangeError("sff-8636 page03h: expected >=128 bytes, got " + got);
  }
  const page03h = decodeSff8636Page03hThresholdsAndControls(raw.subarray(0, 128));
  const rxBounds = thresholdBounds(page03h.rxPowerThreshold, 0.0001);
  const biasBounds = thresholdBounds(page03h.txBiasThreshold, 0.002);
  const txBounds = thresholdBounds(page03h.txPowerThreshold, 0.0001);
  return {
    temperature: thresholdEvaluation(
      lower.temperature / 256.0,
      thresholdBounds(page03h.tempMonThreshold, 1.0 / 256.0),
    ),
    supply_voltage: thresholdEvaluation(
      lower.supplyVoltage * 0.0001,
      thresholdBounds(page03h.vccMonThreshold, 0.0001),
    ),
    rx_power: lower.rxPowerLane.map((rawPower) => thresholdEvaluation(rawPower * 0.0001, rxBounds)),
    tx_bias: lower.txBiasLane.map((rawBias) => thresholdEvaluation(rawBias * 0.002, biasBounds)),
    tx_power: lower.txPowerLane.map((rawPower) => thresholdEvaluation(rawPower * 0.0001, txBounds)),
  };
}

export interface Sff8472ThresholdResult {
  temperature: ThresholdEvaluation;
  supply_voltage: ThresholdEvaluation;
  tx_bias: ThresholdEvaluation;
  tx_power: ThresholdEvaluation;
  rx_power: ThresholdEvaluation;
}

function u16be(buf: Uint8Array, offset: number): number {
  return (buf[offset] << 8) | buf[offset + 1];
}

function i16be(buf: Uint8Array, offset: number): number {
  const value = u16be(buf, offset);
  return value > 0x7fff ? value - 0x10000 : value;
}

function f32be(buf: Uint8Array, offset: number): number {
  return new DataView(buf.buffer, buf.byteOffset, buf.byteLength).getFloat32(offset, false);
}

/**
 * Evaluate the five SFF-8472 base monitors against A2h alarm/warning
 * thresholds. Neither calibration bit set on A0h:92 is an error. An
 * all-zero threshold set does not trip.
 */
export function evaluateSff8472Thresholds(pages: Sff8472Pages): Sff8472ThresholdResult {
  if (!pages || typeof pages !== "object") {
    throw new TypeError("pages must be an object");
  }
  const a0h = decodePageA0hBaseMemory(checkPagePadded("a0h", pages.a0h, 256));
  const a2h = decodePageA2hDiagnosticsBase(checkPagePadded("a2h", pages.a2h, 256));
  let external = false;
  if (a0h.internallyCalibrated) {
    external = false;
  } else if (a0h.externallyCalibrated) {
    external = true;
  } else {
    throw new RangeError("sff-8472 A0h:92: neither internal nor external calibration set");
  }

  const linear: Array<{ slope: number; offset: number }> = [];
  const poly: number[] = [];
  if (external) {
    const cal = a2h.externalCalibrationConstants;
    for (let i = 0; i < 4; i++) {
      linear.push({
        slope: u16be(cal, 76 - 56 + 4 * i) / 256.0,
        offset: i16be(cal, 78 - 56 + 4 * i),
      });
    }
    for (let i = 0; i < 5; i++) {
      poly.push(f32be(cal, 4 * i));
    }
  }
  const lin = (raw: number, i: number): number => {
    if (!external) {
      return raw;
    }
    return linear[i].slope * raw + linear[i].offset;
  };
  const rxRaw = a2h.rxPower;
  const rxPower = external
    ? poly[0] * rxRaw ** 4 + poly[1] * rxRaw ** 3 + poly[2] * rxRaw ** 2 + poly[3] * rxRaw + poly[4]
    : rxRaw;

  return {
    temperature: thresholdEvaluation(
      lin(a2h.temperature, 2) / 256.0,
      thresholdBounds(a2h.temperatureThreshold, 1.0 / 256.0),
    ),
    supply_voltage: thresholdEvaluation(
      lin(a2h.supplyVoltage, 3) * 0.0001,
      thresholdBounds(a2h.supplyVoltageThreshold, 0.0001),
    ),
    tx_bias: thresholdEvaluation(lin(a2h.txBias, 0) * 0.002, thresholdBounds(a2h.txBiasThreshold, 0.002)),
    tx_power: thresholdEvaluation(
      lin(a2h.txPower, 1) * 0.0001,
      thresholdBounds(a2h.txPowerThreshold, 0.0001),
    ),
    rx_power: thresholdEvaluation(rxPower * 0.0001, thresholdBounds(a2h.rxPowerThreshold, 0.0001)),
  };
}
