package openxcvr

import (
	"fmt"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

// DecodeSff8472LaneSignalFlags decodes the single channel's per-lane signal
// flags into the unified LaneStatus vocabulary the Rust `Module::lanes()`
// surface defines (openxcvr-core types::LaneStatus): the A2h byte-110 state
// mirrors (Table 9-16) plus the Adaptive Input EQ Fail Flag (A2h:118.4,
// Table 10-1, schema `since: "12.5"`).
//
// Fields the spec does not define are absent from the entry, mirroring the
// Rust serde `skip_serializing_if` contract: SFF-8472 defines no Tx LOS and
// no CDR LOL flags (they arrived with the SFF-8636/8079 parallel-module
// maps), and the adaptive-EQ bit is not modeled before rev 12.5 — a
// compliance byte (A0h:94) below 12.5 never borrows the later-revision
// reading. An A0h:94 value the family does not admit mirrors the Rust
// union-view module (`Sff8472Module::new`), which decodes the union of
// revisions and therefore surfaces the flag.
func DecodeSff8472LaneSignalFlags(pages Sff8472Pages) ([]map[string]interface{}, error) {
	a0h, err := generated.Decodepage_a0h_base_memory(padPage(pages.A0h, 256))
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a0h: %w", err)
	}
	a2h, err := generated.Decodepage_a2h_diagnostics_base(padPage(pages.A2h, 256))
	if err != nil {
		return nil, fmt.Errorf("sff-8472 a2h: %w", err)
	}

	entry := map[string]interface{}{
		"index":      1,
		"rx_los":     a2h.RxLosState,
		"tx_disable": a2h.TxDisableState,
		"tx_fault":   a2h.TxFaultState,
	}
	// Schema `since: "12.5"` (Table 10-1): 0x0A is rev 12.5; admitted
	// values 0x01-0x09 predate the modeled bit and stay absent.
	if !sff8472ComplianceAdmitted(a0h.Sff8472Compliance) || a0h.Sff8472Compliance == 0x0A {
		entry["tx_adaptive_eq_fault"] = a2h.AdaptiveInputEqFailFlag
	}
	return []map[string]interface{}{entry}, nil
}

// sff8472ComplianceAdmitted reports whether A0h:94 maps to a known
// SFF-8472 compliance revision (Rev::from_compliance_byte_sff8472).
func sff8472ComplianceAdmitted(b int) bool {
	return b >= 0x01 && b <= 0x0A
}

// DecodeSff8636LaneSignalFlags decodes the four lanes' per-lane signal
// flags into the unified LaneStatus vocabulary the Rust `Module::lanes()`
// surface defines: the latched Lower-memory flags of Table 6-5 — Tx LOS
// (byte 3 bits 7-4), Tx fault (byte 4 bits 3-0), Tx adaptive-EQ fault
// (byte 4 bits 7-4, since rev 2.0), Tx CDR LOL (byte 5 bits 7-4), Rx LOS
// (byte 3 bits 3-0), Rx CDR LOL (byte 5 bits 3-0) — plus the Tx-disable
// lane state (byte 86).
//
// The adaptive-EQ flag carries a revision window (schema `since: "2.0"`):
// a detected Lower:1 compliance byte of 0x03-0x05 (revs 1.3-1.5) leaves it
// absent, mirroring the Rust `None` — never a borrowed later-revision
// reading. A Lower:1 value the family does not admit mirrors the Rust
// union-view module (`Sff8636Module::new`), which decodes the union of
// revisions and therefore surfaces the flag.
func DecodeSff8636LaneSignalFlags(pages Sff8636Pages) ([]map[string]interface{}, error) {
	lower, err := generated.DecodeSff8636LowerMemory(padPage(pages.Lower, 128))
	if err != nil {
		return nil, fmt.Errorf("sff-8636 lower: %w", err)
	}

	// Schema `since: "2.0"`: 0x06-0x0A are revs 2.0-2.12; the admitted
	// values 0x03-0x05 (revs 1.3-1.5) predate the modeled bits.
	adaptiveEqModeled := lower.RevisionCompliance >= 0x06 || !sff8636ComplianceAdmitted(lower.RevisionCompliance)

	lanes := make([]map[string]interface{}, 4)
	for i := 0; i < 4; i++ {
		entry := map[string]interface{}{
			"index":      i + 1,
			"rx_los":     lower.RxLosFlag[i],
			"tx_disable": lower.TxDisableLane[i],
			"tx_fault":   lower.TxFaultFlag[i],
			"tx_los":     lower.TxLosFlag[i],
			"tx_cdr_lol": lower.TxCdrLolFlag[i],
			"rx_cdr_lol": lower.RxCdrLolFlag[i],
		}
		if adaptiveEqModeled {
			entry["tx_adaptive_eq_fault"] = lower.TxAdaptiveEqFaultFlag[i]
		}
		lanes[i] = entry
	}
	return lanes, nil
}

// sff8636ComplianceAdmitted reports whether Lower:1 maps to a known
// SFF-8636 compliance revision (Rev::from_compliance_byte_sff8636).
func sff8636ComplianceAdmitted(b int) bool {
	return b >= 0x03 && b <= 0x0A
}
