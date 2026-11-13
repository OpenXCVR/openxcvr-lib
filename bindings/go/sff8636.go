package openxcvr

import (
	"fmt"
	"math"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

// DecodeSff8636 decodes SFF-8636 module pages into the shared golden JSON shape.
func DecodeSff8636(pages Sff8636Pages) (map[string]interface{}, error) {
	lower, err := generated.DecodeSff8636LowerMemory(pages.Lower)
	if err != nil {
		return nil, fmt.Errorf("sff-8636 lower: %w", err)
	}
	page00h, err := generated.DecodeSff8636Page00hUpperAdministrative(pages.Page00h)
	if err != nil {
		return nil, fmt.Errorf("sff-8636 page00h: %w", err)
	}

	extSpec := interface{}(nil)
	if page00h.ExtSpecCompliance != 0 {
		name, ok := sff8636ExtendedSpec[page00h.ExtSpecCompliance]
		if !ok {
			name = "unknown"
		}
		extSpec = map[string]interface{}{
			"name": name,
			"raw":  fmt.Sprintf("0x%02X", page00h.ExtSpecCompliance),
		}
	}

	bitsSet := []interface{}{}
	if page00h.ComplianceExtended {
		bitsSet = append(bitsSet, "compliance_extended")
	}
	if page00h.Compliance40gbaseSr4 {
		bitsSet = append(bitsSet, "compliance_40gbase_sr4")
	}

	moduleType := interface{}(nil)
	if page00h.ExtSpecCompliance == 0x02 {
		moduleType = "100GBASE-SR4 QSFP28"
	} else if page00h.Compliance40gbaseSr4 {
		moduleType = "40GBASE-SR4 QSFP+"
	}

	nominalRate := float64(page00h.NominalSignalingRate) / 10.0
	if page00h.NominalSignalingRate == 0xff {
		nominalRate = float64(page00h.ExtBaudRate) / 4.0
	}

	anyInterruptFlag := false
	for _, v := range page00hSliceBool(lower.TxLosFlag, lower.RxLosFlag) {
		anyInterruptFlag = anyInterruptFlag || v
	}
	for _, v := range append(append(lower.RxPowerAlarmWarningFlag, lower.TxBiasAlarmWarningFlag...), lower.TxPowerAlarmWarningFlag...) {
		anyInterruptFlag = anyInterruptFlag || v
	}

	lanes := make([]interface{}, 4)
	for i := 0; i < 4; i++ {
		lanes[i] = map[string]interface{}{
			"index":       i + 1,
			"rx_power_mw": math.Round(float64(lower.RxPowerLane[i])*0.0001*1e4) / 1e4,
			"tx_bias_ma":  math.Round(float64(lower.TxBiasLane[i])*0.002*1e3) / 1e3,
			"tx_power_mw": math.Round(float64(lower.TxPowerLane[i])*0.0001*1e4) / 1e4,
		}
	}

	return map[string]interface{}{
		"decode_status": "ok",
		"identity": map[string]interface{}{
			"date_code":   page00h.DateCodeYear + page00h.DateCodeMonth + page00h.DateCodeDay,
			"vendor_name": page00h.VendorName,
			"vendor_oui":  oui(page00h.VendorOui),
			"vendor_pn":   page00h.VendorPn,
			"vendor_rev":  page00h.VendorRev,
			"vendor_sn":   page00h.VendorSn,
		},
		"module": map[string]interface{}{
			"cmis_revision":         nil,
			"connector":             map[string]interface{}{"name": lookupConnector(page00h.ConnectorType), "raw": fmt.Sprintf("0x%02X", page00h.ConnectorType)},
			"encoding":              map[string]interface{}{"name": lookupEncoding(page00h.Encoding), "raw": fmt.Sprintf("0x%02X", page00h.Encoding)},
			"extended_identifier":   fmt.Sprintf("0x%02X", page00h.PowerClassLow|(page00h.PowerClassHigh<<6)|boolBit(page00h.RxCdrPresent, 2)|boolBit(page00h.TxCdrPresent, 3)|boolBit(page00h.CleiPresent, 4)|boolBit(page00h.PowerClass8Implemented, 5)),
			"identifier":            map[string]interface{}{"name": lookupIdentifier(lower.Sff8024Identifier), "raw": fmt.Sprintf("0x%02X", lower.Sff8024Identifier)},
			"memory_model":          map[bool]string{true: "flat", false: "paged"}[lower.FlatMemory],
			"module_state":          nil,
			"module_type":           moduleType,
			"nominal_bit_rate_gbps": nominalRate,
		},
		"monitoring": map[string]interface{}{
			"module_temperature_c": float64(lower.Temperature) / 256.0,
			"module_voltage_v":     math.Round(float64(lower.SupplyVoltage)*0.0001*1e4) / 1e4,
		},
		"compliance": map[string]interface{}{
			"bits_set": bitsSet,
			"device_technology": map[string]interface{}{
				"name": lookupDeviceTechnology(page00h.DeviceTechnology),
				"raw":  fmt.Sprintf("0x%X", page00h.DeviceTechnology),
			},
			"ext_spec_byte_192":       extSpec,
			"raw_bytes_page_00h_3_10": hexBytes(pages.Page00h[3:11]),
			"reach": map[string]interface{}{
				"om1_m":           page00h.LengthOm1OrCopperAttenuation25p78ghz,
				"om2_m":           page00h.LengthOm21m,
				"om3_2m_units":    page00h.LengthOm32m * 2,
				"om4_or_copper_m": page00h.LengthCableOrOm4,
				"smf_km":          page00h.LengthSmfKm,
			},
		},
		"flags": map[string]interface{}{
			"data_not_ready":     lower.DataNotReady,
			"interrupt_asserted": !lower.InterruptDeasserted || anyInterruptFlag,
		},
		"lanes": lanes,
		"checksums": map[string]interface{}{
			"cc_base": checksum(pages.Page00h, 0, 63, 63),
			"cc_ext":  checksum(pages.Page00h, 64, 95, 95),
		},
	}, nil
}

func page00hSliceBool(a, b []bool) []bool {
	out := append([]bool{}, a...)
	return append(out, b...)
}

func boolBit(v bool, shift int) int {
	if v {
		return 1 << shift
	}
	return 0
}
