package openxcvr

import (
	"fmt"
	"math"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

// DecodeSff8472 decodes SFF-8472 A0h/A2h bytes into the shared golden JSON shape.
func DecodeSff8472(pages Sff8472Pages) (map[string]interface{}, error) {
	a0hBuf := padPage(pages.A0h, 256)
	a2hBuf := padPage(pages.A2h, 256)
	a0h, err := generated.Decodepage_a0h_base_memory(a0hBuf)
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a0h: %w", err)
	}
	a2h, err := generated.Decodepage_a2h_diagnostics_base(a2hBuf)
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a2h: %w", err)
	}

	moduleType := interface{}(nil)
	if len(a0h.TransceiverCompliance) > 3 && a0h.TransceiverCompliance[3] {
		moduleType = "10GBASE-SR SFP+"
	}
	flags := a2h.AlarmWarningFlag

	// SFF-8472 v12.5a § 5.6: nominal signaling rate byte 12 == 0xFF is a sentinel
	// for rates > 25.4 GBd; the high-resolution rate is in byte 66 (250 MBd units
	// → divide by 4 to express in GBd).
	nominalBitRate := float64(a0h.SignalingRateNominal) / 10.0
	if a0h.SignalingRateNominal == 0xff {
		nominalBitRate = float64(a0h.SignalingRateMax) / 4.0
	}

	// SFF-8472 § 8.8: DDM (A2h real-time monitoring) is optional. When byte 92
	// bit 6 (DigitalDiagnosticsImplemented) is clear, A2h holds no meaningful
	// monitoring/flag/lane data — emit null rather than zero-filled structures.
	ddm := a0h.DigitalDiagnosticsImplemented

	var flagsOut interface{} = nil
	var monitoringOut interface{} = nil
	var lanesOut interface{} = nil
	if ddm {
		flagsOut = map[string]interface{}{
			"alarm_summary": map[string]interface{}{
				"rx_power_high_alarm": flags[8],
				"rx_power_low_alarm":  flags[9],
				"temp_high_alarm":     flags[0],
				"temp_low_alarm":      flags[1],
				"tx_bias_high_alarm":  flags[4],
				"tx_bias_low_alarm":   flags[5],
				"tx_power_high_alarm": flags[6],
				"tx_power_low_alarm":  flags[7],
				"vcc_high_alarm":      flags[2],
				"vcc_low_alarm":       flags[3],
			},
			"warning_summary": map[string]interface{}{
				"rx_power_high_warning": flags[24],
				"rx_power_low_warning":  flags[25],
				"temp_high_warning":     flags[16],
				"temp_low_warning":      flags[17],
				"tx_bias_high_warning":  flags[20],
				"tx_bias_low_warning":   flags[21],
				"tx_power_high_warning": flags[22],
				"tx_power_low_warning":  flags[23],
				"vcc_high_warning":      flags[18],
				"vcc_low_warning":       flags[19],
			},
		}
		monitoringOut = map[string]interface{}{
			"module_temperature_c": float64(a2h.Temperature) / 256.0,
			"module_voltage_v":     math.Round(float64(a2h.SupplyVoltage)*0.0001*1e4) / 1e4,
		}
		lanesOut = []interface{}{
			map[string]interface{}{
				"index":       1,
				"rx_los":      a2h.RxLosState,
				"rx_power_mw": math.Round(float64(a2h.RxPower)*0.0001*1e4) / 1e4,
				"tx_bias_ma":  math.Round(float64(a2h.TxBias)*0.002*1e3) / 1e3,
				"tx_disable":  a2h.TxDisableState,
				"tx_fault":    a2h.TxFaultState,
				"tx_power_mw": math.Round(float64(a2h.TxPower)*0.0001*1e4) / 1e4,
			},
		}
	}

	return map[string]interface{}{
		"decode_status": "ok",
		"identity": map[string]interface{}{
			"date_code":   a0h.DateCode[:6],
			"vendor_name": a0h.VendorName,
			"vendor_oui":  oui(a0h.VendorOui),
			"vendor_pn":   a0h.VendorPn,
			"vendor_sn":   a0h.VendorSn,
		},
		"module": map[string]interface{}{
			"calibration": map[string]interface{}{
				"ddm_implemented":       a0h.DigitalDiagnosticsImplemented,
				"externally_calibrated": a0h.ExternallyCalibrated,
				"internally_calibrated": a0h.InternallyCalibrated,
				"rx_power_kind":         map[bool]string{true: "average", false: "oma"}[a0h.RxPowerAverage],
			},
			"cmis_revision":         nil,
			"connector":             map[string]interface{}{"name": lookupConnector(a0h.ConnectorType), "raw": fmt.Sprintf("0x%02X", a0h.ConnectorType)},
			"encoding":              map[string]interface{}{"name": lookupEncoding(a0h.Encoding), "raw": fmt.Sprintf("0x%02X", a0h.Encoding)},
			"identifier":            map[string]interface{}{"name": lookupIdentifier(a0h.Sff8024Identifier), "raw": fmt.Sprintf("0x%02X", a0h.Sff8024Identifier)},
			"memory_model":          nil,
			"module_state":          nil,
			"module_type":           moduleType,
			"nominal_bit_rate_gbps": nominalBitRate,
		},
		"monitoring": monitoringOut,
		"compliance": map[string]interface{}{
			"raw_bytes_a0h_3_10": hexBytes(a0hBuf[3:11]),
			"reach": map[string]interface{}{
				"om1_m": a0h.LengthOm110m * 10,
				"om2_m": a0h.LengthOm210m * 10,
				// SFF-8472 byte 19 is OM3 length in units of 10 m (classic flat byte;
				// base|multiplier<<6 == the raw byte). ×10 yields meters. Deliberate:
				// the schema's base(u6)+multiplier(u2 enum) split is not the wire
				// semantics — finisar (byte19=30, golden=300) falsifies the ×0.1
				// reading. Do not "fix" to a fractional multiplier.
				"om3_m":  (a0h.LengthOm3Base | (a0h.LengthOm3Multiplier << 6)) * 10,
				"om4_m":  a0h.LengthOm4OrCopper1m,
				"smf_km": a0h.LengthSmfKm,
			},
			"wavelength_nm": a0h.WavelengthOrCableCompliance,
		},
		"flags": flagsOut,
		"lanes": lanesOut,
		"checksums": map[string]interface{}{
			"cc_base": checksum(a0hBuf, 0, 63, 63),
			"cc_dmi":  checksum(a2hBuf, 0, 95, 95),
			"cc_ext":  checksum(a0hBuf, 64, 95, 95),
		},
	}, nil
}
