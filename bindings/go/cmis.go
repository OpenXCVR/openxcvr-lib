package openxcvr

import (
	"fmt"
	"math"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
	genums "github.com/OpenXCVR/openxcvr-lib/bindings/go/generated/enums"
)

// DecodeCmis decodes a CMIS module's EEPROM pages into a JSON-compatible map.
//
// Optional pages follow the facade contract (bindings/README.md): a nil
// optional field is an absent page; a present (non-nil) field, including an
// empty or short one, is a supplied page and must never become absence —
// a present-but-short buffer is an error, mirroring the TS checkPage guard.
func DecodeCmis(pages Pages) (map[string]interface{}, error) {
	if len(pages.Lower) < 128 {
		return nil, fmt.Errorf("lower page too short: %d bytes", len(pages.Lower))
	}
	if pages.Page01h != nil && len(pages.Page01h) < 128 {
		return nil, fmt.Errorf("page01h must be at least 128 bytes, got %d", len(pages.Page01h))
	}
	if pages.Page11h != nil && len(pages.Page11h) < 128 {
		return nil, fmt.Errorf("page11h must be at least 128 bytes, got %d", len(pages.Page11h))
	}

	// CMIS revision: byte 1, upper nibble = major, lower nibble = minor
	cmisByte := int(pages.Lower[1])
	cmisM := (cmisByte >> 4) & 0xF
	cmism := cmisByte & 0xF

	if cmisM != 3 && cmisM != 4 && cmisM != 5 {
		return map[string]interface{}{
			"decode_status": "rejected",
			"rejection": map[string]interface{}{
				"detail": fmt.Sprintf(
					"Lower memory byte 1 = 0x%02X; "+
						"CMIS 5.3 § 8.2.1 defines major revisions 1.x, 3.0, 4.0, 5.0, 5.1, 5.3. "+
						"The fboss test corpus uses 0x%02X as an explicit anomalous-rev regression target.",
					cmisByte, cmisByte),
				"reason": fmt.Sprintf("unknown_major_revision: 0x%02X", cmisByte),
			},
		}, nil
	}

	lower, err := generated.DecodeCmisLowerMemory(pages.Lower)
	if err != nil {
		return nil, fmt.Errorf("lower: %w", err)
	}

	page00h, err := generated.DecodeCmisPage00hAdministrative(pages.Page00h)
	if err != nil {
		return nil, fmt.Errorf("page00h: %w", err)
	}

	// ─── identity ────────────────────────────────────────────────────────────
	// Vendor OUI uses uppercase hex (matches Python {:02X})
	vendorOUI := fmt.Sprintf("%02X:%02X:%02X",
		pages.Page00h[17], pages.Page00h[18], pages.Page00h[19])
	dateCode := page00h.DateCodeYear + page00h.DateCodeMonth + page00h.DateCodeDayOfMonth

	identity := map[string]interface{}{
		"date_code":   dateCode,
		"vendor_name": page00h.VendorName,
		"vendor_oui":  vendorOUI,
		"vendor_pn":   page00h.VendorPn,
		"vendor_rev":  page00h.VendorRev,
		"vendor_sn":   page00h.VendorSn,
	}

	// ─── module ───────────────────────────────────────────────────────────────
	memoryModel := "paged"
	if lower.MemoryModel {
		memoryModel = "flat"
	}

	// Module state name: matches Python CmisModuleState enum name → CamelCase
	// e.g. RESERVED_0 → Reserved0, MODULE_LOW_PWR → ModuleLowPwr
	moduleStateNames := map[int]string{
		0x00: "Reserved0",
		0x01: "ModuleLowPwr",
		0x02: "ModulePwrUp",
		0x03: "ModuleReady",
		0x04: "ModulePwrDn",
		0x05: "ModuleFault",
	}
	moduleStateName, ok := moduleStateNames[lower.ModuleState]
	if !ok {
		// Python: f"Unknown{raw_state}" (decimal)
		moduleStateName = fmt.Sprintf("Unknown%d", lower.ModuleState)
	}

	identID := lower.Sff8024Identifier
	connID := page00h.ConnectorType

	module := map[string]interface{}{
		"cmis_revision": fmt.Sprintf("%d.%d", cmisM, cmism),
		"connector": map[string]interface{}{
			"name": lookupConnector(connID),
			"raw":  fmt.Sprintf("0x%02X", connID),
		},
		"identifier": map[string]interface{}{
			"name": lookupIdentifier(identID),
			"raw":  fmt.Sprintf("0x%02X", identID),
		},
		"memory_model": memoryModel,
		"module_state": moduleStateName,
		"module_type":  nil,
	}

	// ─── monitoring ───────────────────────────────────────────────────────────
	// TempMonValue: i16be s16 1/256 °C
	// VccMonVoltage: u16be u16 × 100 µV
	// Round voltage to 4 decimal places to avoid floating-point drift
	// (100 µV = 0.0001 V; u16 * 1e-4 has precision issues in float64)
	voltageRaw := float64(lower.VccMonVoltage) * 100e-6
	voltageV := math.Round(voltageRaw*1e4) / 1e4

	monitoring := map[string]interface{}{
		"module_temperature_c": float64(lower.TempMonValue) / 256.0,
		"module_voltage_v":     voltageV,
	}

	// ─── applications ─────────────────────────────────────────────────────────
	var appsSlice []appEntry

	if !lower.MemoryModel { // paged module
		appsSlice = append(appsSlice, parseAppDescriptors(pages.Lower, 86, 8, 0, pages.Lower[85])...)
		if len(pages.Page01h) >= 128 {
			appsSlice = append(appsSlice, parseAppDescriptors(pages.Page01h, 95, 7, 8, pages.Lower[85])...)
		}
	}

	// Convert []appEntry → []interface{} for JSON; nil if empty (matches Python)
	var applications interface{}
	if len(appsSlice) > 0 {
		apps := make([]interface{}, len(appsSlice))
		for i, a := range appsSlice {
			apps[i] = a
		}
		applications = apps
	}

	// ─── lanes / flags (CMIS Page 11h) ─────────────────────────────────────────
	// Present only for paged modules that expose Page 11h; flat-memory modules
	// (no Page 11h) keep both nil, matching the OpenAPI contract. The
	// schema-owned DPSM name projection is bound ONCE here, from the module's
	// revision byte (Lower:1 = cmisByte), before the lane loop: a supported
	// context uses the bound callable; a refused — unsupported present —
	// context (admitted major, byte outside the six catalog revisions)
	// explicitly keeps a nil projection so the fallback branch is selected
	// before any call and every raw renders Unknown<n>.
	var lanes interface{}
	var flags interface{}
	if pages.Page11h != nil && !lower.MemoryModel {
		var projection genums.CmisDataPathStateProjection
		if p, ok := genums.CmisDataPathStateForRevision(byte(cmisByte)); ok {
			projection = p
		}
		l, f, err := decodeLanesFlags(pages.Lower, pages.Page01h, pages.Page11h, projection)
		if err != nil {
			return nil, err
		}
		lanes = l
		flags = f
	}

	result := map[string]interface{}{
		"decode_status": "ok",
		"identity":      identity,
		"module":        module,
		"monitoring":    monitoring,
		"applications":  applications,
		"checksums":     nil,
		"flags":         flags,
		"lanes":         lanes,
	}

	// ─── VDM (Pages 20h-2Fh) ─────────────────────────────────────────────────
	// Emitted only when the caller supplies the COMPLETE VDM page set
	// (20h-23h, 24h-27h, 28h-2Bh, 2Ch, 2Dh, 2Fh): the `vdm` block is a
	// fixture-scoped extension of the decode surface, decoded as a whole
	// from a whole page set. A partial set (e.g. a real-module capture with
	// only some VDM pages) yields no block — never a block fabricated from
	// zero-filled missing pages — and within the block the Page 2Fh
	// advertisement still gates which groups are decoded (fail closed on any
	// short page).
	if vdmPagesComplete(pages) {
		vdm, err := decodeVdm(pages)
		if err != nil {
			return nil, err
		}
		result["vdm"] = vdm
	}

	return result, nil
}

// decodeLanesFlags builds the per-lane `lanes` array and the module/per-lane
// `flags` tree from CMIS Page 11h (issue #105). It leans on the generated
// Page 11h decoder for all offset/bit math and applies only scaling, dp_state
// naming via the schema-owned projection, and flag-key remapping
// (OpticalPower*Tx→tx_power, LaserBias*Tx→tx_bias, OpticalPower*Rx→rx_power).
// `projection` is the generated name projection bound once by the caller for
// the module's revision byte: a nil projection (refused — unsupported
// present — context, selected before any call) renders every raw with the
// Unknown<n> fallback, as does a reserved raw the projection does not name.
func decodeLanesFlags(lower, page01h, page11h []byte, projection genums.CmisDataPathStateProjection) (interface{}, interface{}, error) {
	p11, err := generated.DecodeCmisPage11hLaneStatus(page11h)
	if err != nil {
		return nil, nil, fmt.Errorf("page11h: %w", err)
	}

	// LaserBiasTx multiplier from Page 01h:160 bits 4-3 (CMIS 5.3 Table 8-52).
	if len(page01h) < 128 {
		return nil, nil, fmt.Errorf("page 11h present but page 01h missing (Tx-bias multiplier)")
	}
	mult := 0.0
	biasReserved := false
	switch (page01h[160-128] >> 3) & 0x03 {
	case 0:
		mult = 1.0
	case 1:
		mult = 2.0
	case 2:
		mult = 4.0
	default:
		// Reserved 0b11 (CMIS 5.3 Table 8-52): the multiplier is undefined, so
		// tx_bias_ma is unrepresentable. Emit null for it on every lane and keep
		// the rest of the lane block intact rather than dropping the whole block.
		biasReserved = true
	}

	lanes := make([]interface{}, 8)
	laneFlags := make([]interface{}, 8)
	for i := 0; i < 8; i++ {
		nibble := p11.DpStateHostLane[i]
		var dpState string
		if projection != nil {
			if name, named := projection(byte(nibble)); named {
				dpState = name
			}
		}
		if dpState == "" {
			dpState = fmt.Sprintf("Unknown%d", nibble)
		}
		var txBiasMa interface{}
		if !biasReserved {
			txBiasMa = math.Round(float64(p11.LaserBiasTx[i])*0.002*mult*1e3) / 1e3
		} // else nil → JSON null
		lanes[i] = map[string]interface{}{
			"index": i + 1,
			// Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
			// (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87),
			// both from the generated Page 11h decoder.
			"rx_los":      p11.LosFlagRx[i],
			"tx_fault":    p11.FailureFlagTx[i],
			"dp_state":    dpState,
			"rx_power_mw": math.Round(float64(p11.OpticalPowerRx[i])*0.0001*1e4) / 1e4,
			"tx_bias_ma":  txBiasMa,
			"tx_power_mw": math.Round(float64(p11.OpticalPowerTx[i])*0.0001*1e4) / 1e4,
		}
		laneFlags[i] = map[string]interface{}{
			"rx_power_high_alarm":   p11.OpticalPowerHighAlarmFlagRx[i],
			"rx_power_low_alarm":    p11.OpticalPowerLowAlarmFlagRx[i],
			"rx_power_high_warning": p11.OpticalPowerHighWarningFlagRx[i],
			"rx_power_low_warning":  p11.OpticalPowerLowWarningFlagRx[i],
			"tx_bias_high_alarm":    p11.LaserBiasHighAlarmFlagTx[i],
			"tx_bias_low_alarm":     p11.LaserBiasLowAlarmFlagTx[i],
			"tx_bias_high_warning":  p11.LaserBiasHighWarningFlagTx[i],
			"tx_bias_low_warning":   p11.LaserBiasLowWarningFlagTx[i],
			"tx_power_high_alarm":   p11.OpticalPowerHighAlarmFlagTx[i],
			"tx_power_low_alarm":    p11.OpticalPowerLowAlarmFlagTx[i],
			"tx_power_high_warning": p11.OpticalPowerHighWarningFlagTx[i],
			"tx_power_low_warning":  p11.OpticalPowerLowWarningFlagTx[i],
		}
	}

	flags := map[string]interface{}{
		"data_not_ready":     nil,
		"interrupt_asserted": lower[3]&(1<<0) == 0,
		"temp_high_alarm":    lower[9]&(1<<0) != 0,
		"temp_low_alarm":     lower[9]&(1<<1) != 0,
		"supply_high_alarm":  lower[9]&(1<<4) != 0,
		"supply_low_alarm":   lower[9]&(1<<5) != 0,
		"lanes":              laneFlags,
	}
	return lanes, flags, nil
}
