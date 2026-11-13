package openxcvr

import (
	"encoding/binary"
	"path/filepath"
	"testing"
)

func withinBounds(e map[string]interface{}) bool {
	for _, k := range []string{"high_alarm", "low_alarm", "high_warning", "low_warning"} {
		if v, _ := e[k].(bool); v {
			return false
		}
	}
	return true
}

func evalBool(t *testing.T, e map[string]interface{}, key string) bool {
	t.Helper()
	v, ok := e[key].(bool)
	if !ok {
		t.Fatalf("evaluation key %q missing or not bool: %v", key, e)
	}
	return v
}

// The Finisar capture carries real A2h threshold sets: its received power
// (0.0 mW) sits below the non-zero low-alarm/low-warning bounds while
// every other monitor stays within bounds — the same expectations the
// Rust evaluate_thresholds test pins.
func TestEvaluateSff8472ThresholdsFinisar(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8472")
	stem := "finisar-ftlx8571d3bcl-mup0wb0"
	result, err := EvaluateSff8472Thresholds(Sff8472Pages{
		A0h: readFile(t, filepath.Join(dir, stem+".a0h.bin")),
		A2h: readFile(t, filepath.Join(dir, stem+".a2h.bin")),
	})
	if err != nil {
		t.Fatalf("EvaluateSff8472Thresholds: %v", err)
	}
	monitors := map[string]map[string]interface{}{
		"temperature":    result["temperature"].(map[string]interface{}),
		"supply_voltage": result["supply_voltage"].(map[string]interface{}),
		"tx_bias":        result["tx_bias"].(map[string]interface{}),
		"tx_power":       result["tx_power"].(map[string]interface{}),
	}
	for name, e := range monitors {
		if !withinBounds(e) {
			t.Errorf("%s = %v, want within bounds", name, e)
		}
	}
	rx := result["rx_power"].(map[string]interface{})
	if !evalBool(t, rx, "low_alarm") || !evalBool(t, rx, "low_warning") {
		t.Errorf("rx_power = %v, want low alarm and low warning", rx)
	}
	if evalBool(t, rx, "high_alarm") || evalBool(t, rx, "high_warning") {
		t.Errorf("rx_power = %v, no high bound may trip", rx)
	}
}

// The synthetic captures leave every threshold byte zero (the
// not-implemented encoding): nothing may trip — in particular no zero
// high-alarm bound may spuriously alarm a zero reading.
func TestEvaluateSff8472ThresholdsZeroSetsStayWithinBounds(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8472")
	for _, stem := range []string{"synthetic-extcal-sfp10g-sr", "synthetic-sfp28-25g-sr"} {
		result, err := EvaluateSff8472Thresholds(Sff8472Pages{
			A0h: readFile(t, filepath.Join(dir, stem+".a0h.bin")),
			A2h: readFile(t, filepath.Join(dir, stem+".a2h.bin")),
		})
		if err != nil {
			t.Fatalf("%s: %v", stem, err)
		}
		for _, name := range []string{"temperature", "supply_voltage", "tx_bias", "tx_power", "rx_power"} {
			e := result[name].(map[string]interface{})
			if !withinBounds(e) {
				t.Errorf("%s %s = %v, want within bounds", stem, name, e)
			}
		}
	}
}

// External calibration changes the measurement: with slope ×2 linear
// terms the temperature reading doubles, so a 40 °C high-alarm bound trips
// under calibration while the raw 32 °C reading would stay within. This is
// the discriminating case the Rust module accessors define.
func TestEvaluateSff8472ThresholdsAppliesExternalCalibration(t *testing.T) {
	a0h := make([]byte, 256)
	a0h[92] = 0x10 // externally calibrated
	a2h := make([]byte, 256)
	// Linear terms at A2h:76-91: slope 512 (×2), offset 0 on every pair.
	for i := 0; i < 4; i++ {
		binary.BigEndian.PutUint16(a2h[76+4*i:], 512)
		binary.BigEndian.PutUint16(a2h[78+4*i:], 0)
	}
	// Temperature monitor: raw 8192 → uncalibrated 32 °C, calibrated 64 °C.
	binary.BigEndian.PutUint16(a2h[96:], 8192)
	// High alarm at 40 °C (raw 10240), everything else zero.
	binary.BigEndian.PutUint16(a2h[0:], 10240)

	result, err := EvaluateSff8472Thresholds(Sff8472Pages{A0h: a0h, A2h: a2h})
	if err != nil {
		t.Fatalf("EvaluateSff8472Thresholds: %v", err)
	}
	temp := result["temperature"].(map[string]interface{})
	if !evalBool(t, temp, "high_alarm") {
		t.Errorf("temperature = %v, want calibrated 64 °C to trip the 40 °C high alarm", temp)
	}
	if withinBounds(temp) {
		t.Errorf("temperature must not be within bounds under calibration")
	}
}

// A0h:92 with neither calibration bit is an error, never a silently
// uncalibrated decision (mirroring the Rust cal_mode refusal).
func TestEvaluateSff8472ThresholdsRefusesUncalibratedModule(t *testing.T) {
	a0h := make([]byte, 256) // byte 92 == 0: no calibration declared
	_, err := EvaluateSff8472Thresholds(Sff8472Pages{A0h: a0h, A2h: make([]byte, 256)})
	if err == nil {
		t.Fatal("expected calibration-mode error, got nil")
	}
}

// Rev-detected Finisar 100G SR4 (Lower:1 = 0x07 → rev 2.5): monitors sit
// below the low bias/power bounds with temperature and supply within —
// the same expectations the Rust evaluate_thresholds test pins.
func TestEvaluateSff8636ThresholdsRevDetectedFixture(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8636")
	stem := "finisar-ftlc9551repm-100g-sr4"
	pages := Sff8636Pages{
		Lower:   readFile(t, filepath.Join(dir, stem+".lower.bin")),
		Page00h: readFile(t, filepath.Join(dir, stem+".page-00h.bin")),
		Page03h: readFile(t, filepath.Join(dir, stem+".page-03h.bin")),
	}
	result, err := EvaluateSff8636Thresholds(pages)
	if err != nil {
		t.Fatalf("EvaluateSff8636Thresholds: %v", err)
	}
	if !withinBounds(result["temperature"].(map[string]interface{})) {
		t.Errorf("temperature = %v, want within bounds", result["temperature"])
	}
	if !withinBounds(result["supply_voltage"].(map[string]interface{})) {
		t.Errorf("supply_voltage = %v, want within bounds", result["supply_voltage"])
	}
	for _, group := range []string{"rx_power", "tx_bias", "tx_power"} {
		lanes := result[group].([]interface{})
		for i, lane := range lanes {
			e := lane.(map[string]interface{})
			if !evalBool(t, e, "low_alarm") || !evalBool(t, e, "low_warning") {
				t.Errorf("%s lane %d = %v, want low alarm and low warning", group, i+1, e)
			}
			if evalBool(t, e, "high_alarm") || evalBool(t, e, "high_warning") {
				t.Errorf("%s lane %d = %v, no high bound may trip", group, i+1, e)
			}
		}
	}
}

// Union path (Lower:1 = 0x00): lane 2 TX power (0.9152 mW) exceeds the
// 0.7943 mW high-warning bound without reaching the 1.5848 mW high alarm;
// every other lane monitor stays within.
func TestEvaluateSff8636ThresholdsUnionPathFixture(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8636")
	stem := "finisar-ftl410qe3c-40g-sr4"
	result, err := EvaluateSff8636Thresholds(Sff8636Pages{
		Lower:   readFile(t, filepath.Join(dir, stem+".lower.bin")),
		Page00h: readFile(t, filepath.Join(dir, stem+".page-00h.bin")),
		Page03h: readFile(t, filepath.Join(dir, stem+".page-03h.bin")),
	})
	if err != nil {
		t.Fatalf("EvaluateSff8636Thresholds: %v", err)
	}
	tx := result["tx_power"].([]interface{})
	for i, lane := range tx {
		e := lane.(map[string]interface{})
		if i == 1 {
			if !evalBool(t, e, "high_warning") || evalBool(t, e, "high_alarm") || evalBool(t, e, "low_alarm") {
				t.Errorf("tx_power lane 2 = %v, want high warning only", e)
			}
		} else if !withinBounds(e) {
			t.Errorf("tx_power lane %d = %v, want within bounds", i+1, e)
		}
	}
	for _, group := range []string{"rx_power", "tx_bias"} {
		for _, lane := range result[group].([]interface{}) {
			if !withinBounds(lane.(map[string]interface{})) {
				t.Errorf("%s = %v, want within bounds", group, lane)
			}
		}
	}
}

// All-zero threshold sets (the DAC fixture does not implement Page 03h
// monitors) stay within bounds on every monitor.
func TestEvaluateSff8636ThresholdsZeroSetsStayWithinBounds(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8636")
	stem := "synthetic-qsfp-dac-40g-passive-cu"
	result, err := EvaluateSff8636Thresholds(Sff8636Pages{
		Lower:   readFile(t, filepath.Join(dir, stem+".lower.bin")),
		Page00h: readFile(t, filepath.Join(dir, stem+".page-00h.bin")),
		Page03h: readFile(t, filepath.Join(dir, stem+".page-03h.bin")),
	})
	if err != nil {
		t.Fatalf("EvaluateSff8636Thresholds: %v", err)
	}
	if !withinBounds(result["temperature"].(map[string]interface{})) {
		t.Errorf("temperature = %v, want within bounds", result["temperature"])
	}
	for _, group := range []string{"rx_power", "tx_bias", "tx_power"} {
		for _, lane := range result[group].([]interface{}) {
			if !withinBounds(lane.(map[string]interface{})) {
				t.Errorf("%s = %v, want within bounds", group, lane)
			}
		}
	}
}

// A missing Page 03h window is an error — the caller supplies the bytes,
// and no page is ever fabricated.
func TestEvaluateSff8636ThresholdsRequiresPage03h(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8636")
	_, err := EvaluateSff8636Thresholds(Sff8636Pages{
		Lower: readFile(t, filepath.Join(dir, "synthetic-qsfp-dac-40g-passive-cu.lower.bin")),
	})
	if err == nil {
		t.Fatal("expected missing-page03h error, got nil")
	}
}
