package openxcvr

import (
	"encoding/binary"
	"fmt"
	"math"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

// thresholdBounds converts one raw threshold set (severity order per the
// schema: high_alarm, low_alarm, high_warning, low_warning) through a scale
// to physical units. An all-zero raw set is the not-implemented encoding —
// every bound disabled (nil) — mirroring openxcvr-core's
// `Thresholds::default()`: a zero high-alarm bound would spuriously trip on
// every reading, so it never becomes a live bound.
func thresholdBounds(raw []int, scale float64) [4]*float64 {
	var out [4]*float64
	implemented := false
	for _, v := range raw {
		if v != 0 {
			implemented = true
			break
		}
	}
	if !implemented {
		return out
	}
	for i := range out {
		bound := float64(raw[i]) * scale
		out[i] = &bound
	}
	return out
}

// thresholdEvaluation mirrors openxcvr-core's `threshold::evaluate`:
// inclusive bounds (a measurement equal to a bound trips it, mirroring the
// SFF latched-flag semantics) and disabled (nil) bounds never trip.
func thresholdEvaluation(measurement float64, bounds [4]*float64) map[string]interface{} {
	tripHigh := func(b *float64) bool { return b != nil && measurement >= *b }
	tripLow := func(b *float64) bool { return b != nil && measurement <= *b }
	return map[string]interface{}{
		"high_alarm":   tripHigh(bounds[0]),
		"low_alarm":    tripLow(bounds[1]),
		"high_warning": tripHigh(bounds[2]),
		"low_warning":  tripLow(bounds[3]),
	}
}

// EvaluateSff8472Thresholds evaluates the five base monitors (temperature,
// supply voltage, TX bias, TX power, RX power) against their A2h alarm and
// warning threshold sets (§ 9.4, Table 9-5) — the counterpart of the Rust
// `Sff8472Module::evaluate_thresholds` pure decision.
//
// Measurements are calibrated exactly as the Rust module accessors:
// internally calibrated modules read the raw A2h monitors through their LSB
// scales, externally calibrated modules apply the linear calibration terms
// (A2h:76-91, slope/256·raw + offset) and the RX-power polynomial
// (A2h:56-75). A0h:92 setting neither calibration bit is an error, never a
// silently uncalibrated decision. Threshold sets the module does not
// implement (all-zero) keep their monitor within bounds.
func EvaluateSff8472Thresholds(pages Sff8472Pages) (map[string]interface{}, error) {
	a0h, err := generated.Decodepage_a0h_base_memory(padPage(pages.A0h, 256))
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a0h: %w", err)
	}
	a2h, err := generated.Decodepage_a2h_diagnostics_base(padPage(pages.A2h, 256))
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a2h: %w", err)
	}

	external := false
	switch {
	case a0h.InternallyCalibrated:
	case a0h.ExternallyCalibrated:
		external = true
	default:
		return nil, fmt.Errorf("sff-8472 A0h:92: neither internal nor external calibration set")
	}

	// External calibration terms, Table 9-5: linear pairs at A2h:76-91
	// ([0] tx bias, [1] tx power, [2] temperature, [3] supply voltage)
	// and the RX-power polynomial coefficients at A2h:56-75.
	var linear [4]struct{ slope, offset float64 }
	var poly [5]float64
	if external {
		cal := a2h.ExternalCalibrationConstants
		for i := range linear {
			linear[i].slope = float64(binary.BigEndian.Uint16(cal[76-56+4*i:78-56+4*i])) / 256.0
			linear[i].offset = float64(int16(binary.BigEndian.Uint16(cal[78-56+4*i : 80-56+4*i])))
		}
		for i := range poly {
			poly[i] = float64(math.Float32frombits(binary.BigEndian.Uint32(cal[4*i : 4*i+4])))
		}
	}
	lin := func(raw int, i int) float64 {
		if !external {
			return float64(raw)
		}
		return linear[i].slope*float64(raw) + linear[i].offset
	}
	rxRaw := float64(a2h.RxPower)
	var rxPower float64
	if !external {
		rxPower = rxRaw
	} else {
		rxPower = poly[0]*rxRaw*rxRaw*rxRaw*rxRaw +
			poly[1]*rxRaw*rxRaw*rxRaw +
			poly[2]*rxRaw*rxRaw +
			poly[3]*rxRaw +
			poly[4]
	}

	return map[string]interface{}{
		"temperature": thresholdEvaluation(
			lin(a2h.Temperature, 2)/256.0,
			thresholdBounds(a2h.TemperatureThreshold, 1.0/256.0),
		),
		"supply_voltage": thresholdEvaluation(
			lin(a2h.SupplyVoltage, 3)*0.0001,
			thresholdBounds(a2h.SupplyVoltageThreshold, 0.0001),
		),
		"tx_bias": thresholdEvaluation(
			lin(a2h.TxBias, 0)*0.002,
			thresholdBounds(a2h.TxBiasThreshold, 0.002),
		),
		"tx_power": thresholdEvaluation(
			lin(a2h.TxPower, 1)*0.0001,
			thresholdBounds(a2h.TxPowerThreshold, 0.0001),
		),
		"rx_power": thresholdEvaluation(
			rxPower*0.0001,
			thresholdBounds(a2h.RxPowerThreshold, 0.0001),
		),
	}, nil
}

// EvaluateSff8636Thresholds evaluates the Lower-memory monitors (case
// temperature, supply voltage, per-lane RX power / TX bias / TX power)
// against the Upper Page 03h threshold sets (§ 6.6.1, Table 6-28) — the
// counterpart of the Rust `Sff8636Module::evaluate_thresholds` pure
// decision, with the same caller-supplied-bytes contract: `pages.Page03h`
// is the 128-byte Page 03h window the caller already read; this function
// never fabricates it. Monitor scales mirror the module accessors
// (temperature 1/256 °C, supply voltage 100 µV, optical power 0.1 µW,
// bias 2 µA); a threshold set the module does not implement (all-zero)
// keeps its monitor within bounds.
func EvaluateSff8636Thresholds(pages Sff8636Pages) (map[string]interface{}, error) {
	lower, err := generated.DecodeSff8636LowerMemory(padPage(pages.Lower, 128))
	if err != nil {
		return nil, fmt.Errorf("sff-8636 lower: %w", err)
	}
	page03h, err := generated.DecodeSff8636Page03hThresholdsAndControls(pages.Page03h)
	if err != nil {
		// No padding: the Page 03h window is caller-supplied, and a short
		// or missing slice is refused rather than zero-filled — a fabricated
		// zero page would silently disable every threshold set.
		return nil, fmt.Errorf("sff-8636 page03h: %w", err)
	}

	rxBounds := thresholdBounds(page03h.RxPowerThreshold, 0.0001)
	biasBounds := thresholdBounds(page03h.TxBiasThreshold, 0.002)
	txBounds := thresholdBounds(page03h.TxPowerThreshold, 0.0001)
	rxPower := make([]interface{}, 4)
	txBias := make([]interface{}, 4)
	txPower := make([]interface{}, 4)
	for i := 0; i < 4; i++ {
		rxPower[i] = thresholdEvaluation(float64(lower.RxPowerLane[i])*0.0001, rxBounds)
		txBias[i] = thresholdEvaluation(float64(lower.TxBiasLane[i])*0.002, biasBounds)
		txPower[i] = thresholdEvaluation(float64(lower.TxPowerLane[i])*0.0001, txBounds)
	}

	return map[string]interface{}{
		"temperature": thresholdEvaluation(
			float64(lower.Temperature)/256.0,
			thresholdBounds(page03h.TempMonThreshold, 1.0/256.0),
		),
		"supply_voltage": thresholdEvaluation(
			float64(lower.SupplyVoltage)*0.0001,
			thresholdBounds(page03h.VccMonThreshold, 0.0001),
		),
		"rx_power": rxPower,
		"tx_bias":  txBias,
		"tx_power": txPower,
	}, nil
}
