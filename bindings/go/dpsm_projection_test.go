// Native tests for the generated revision-bound CMIS DPSM name projection
// (CmisDataPathStateForRevision, schema-owned).
//
// The expected tables below are an independent literal oracle transcribed
// from the spec tables (CMIS 3.0 § 1.7.7.1 Table 66; CMIS 4.0 Table 8-58;
// CMIS 5.3 § 8.10.1 / Table 8-84), not values derived from a decoder, the
// generated table, a raw adapter or runtime case conversion. "" means "no
// name for this raw at this revision" — the returned ok is false and the
// caller renders its own Unknown<n> fallback.
package openxcvr

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	genums "github.com/OpenXCVR/openxcvr-lib/bindings/go/generated/enums"
)

// Expected name per raw nibble at CMIS 3.0 (Table 66: raws 5-7 reserved).
var dpsmV3_0 = [16]string{
	"Reserved0", "DpDeactivated", "DpInit", "DpDeinit", "DpActivated",
	"", "", "", "", "", "", "", "", "", "", "",
}

// Expected name per raw nibble at every catalog revision from 4.0 on.
var dpsmV4_0Plus = [16]string{
	"Reserved0", "DpDeactivated", "DpInit", "DpDeinit", "DpActivated",
	"DpTxTurnOn", "DpTxTurnOff", "DpInitialized",
	"", "", "", "", "", "", "", "",
}

// The seven known CMIS context bytes (packed-BCD major/minor).
var dpsmCatalogBytes = [7]byte{0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54}

func dpsmOracle(rev byte) [16]string {
	if rev == 0x30 {
		return dpsmV3_0
	}
	return dpsmV4_0Plus
}

func TestDpsmProjectionFactoryDomainIsExactlyTheLiteralCatalog(t *testing.T) {
	for i := 0; i < 256; i++ {
		b := byte(i)
		_, ok := genums.CmisDataPathStateForRevision(b)
		expected := false
		for _, known := range dpsmCatalogBytes {
			if b == known {
				expected = true
				break
			}
		}
		if ok != expected {
			t.Fatalf("factory support at byte %#04x: got ok=%v, want %v", b, ok, expected)
		}
	}
}

func TestDpsmProjectionMatchesTheLiteralOracle(t *testing.T) {
	// 112 exact-string cases: every catalog revision × every raw nibble.
	cases := 0
	for _, rev := range dpsmCatalogBytes {
		project, ok := genums.CmisDataPathStateForRevision(rev)
		if !ok {
			t.Fatalf("catalog byte %#04x must be bound", rev)
		}
		want := dpsmOracle(rev)
		for raw := 0; raw < 16; raw++ {
			name, named := project(byte(raw))
			if want[raw] == "" {
				if named || name != "" {
					t.Fatalf("rev %#04x raw %#04x: got (%q, %v), want no name", rev, raw, name, named)
				}
			} else if !named || name != want[raw] {
				t.Fatalf("rev %#04x raw %#04x: got (%q, %v), want %q", rev, raw, name, named, want[raw])
			}
			cases++
		}
	}
	if cases != 112 {
		t.Fatalf("cases = %d, want 112", cases)
	}
}

func TestDpsmProjectionUnsupportedContextsRefuse(t *testing.T) {
	for _, b := range []byte{0x31, 0x41, 0x55, 0x00, 0xFF} {
		if project, ok := genums.CmisDataPathStateForRevision(b); ok || project != nil {
			t.Fatalf("byte %#04x is not a known context", b)
		}
	}
}

func TestDpsmProjectionUnnamedRawsHaveNoName(t *testing.T) {
	for _, rev := range dpsmCatalogBytes {
		project, _ := genums.CmisDataPathStateForRevision(rev)
		if project == nil {
			t.Fatalf("catalog byte %#04x must be bound", rev)
		}
		for _, raw := range []byte{8, 9, 10, 11, 12, 13, 14, 15, 16, 255} {
			if name, named := project(raw); named || name != "" {
				t.Fatalf("raw %#04x must be unnamed at rev %#04x", raw, rev)
			}
		}
	}
}

func TestDpsmProjectionInterleavedHandlesKeepIndependentSelections(t *testing.T) {
	v3, ok3 := genums.CmisDataPathStateForRevision(0x30)
	v5, ok5 := genums.CmisDataPathStateForRevision(0x53)
	if !ok3 || !ok5 {
		t.Fatal("both catalog bytes must be bound")
	}
	for raw := 0; raw < 16; raw++ {
		want3 := dpsmV3_0[raw]
		want5 := dpsmV4_0Plus[raw]
		name3, named3 := v3(byte(raw))
		name5, named5 := v5(byte(raw))
		if (named3 || name3 != "") != (want3 != "") || name3 != want3 {
			t.Fatalf("3.0 handle at %#04x: got (%q, %v), want %q", raw, name3, named3, want3)
		}
		if (named5 || name5 != "") != (want5 != "") || name5 != want5 {
			t.Fatalf("5.3 handle at %#04x: got (%q, %v), want %q", raw, name5, named5, want5)
		}
		// Interleave again to prove no cross-handle state.
		name3b, named3b := v3(byte(raw))
		if name3b != name3 || named3b != named3 {
			t.Fatalf("3.0 handle changed at %#04x", raw)
		}
	}
	// A fresh binding of the same revision agrees with the held one.
	if _, named := v3(0x05); named {
		t.Fatal("3.0 must not name raw 5")
	}
	if name, named := v5(0x05); !named || name != "DpTxTurnOn" {
		t.Fatalf("5.3 raw 5: got (%q, %v)", name, named)
	}
}

func TestDpsmProjectionFlatLabelsRemainRevisionBlind(t *testing.T) {
	// Existing revision-blind raw labels/cases are unchanged: the eight
	// named constants keep their snake_case Label() strings and raws 8-15
	// render "unknown" through the flat surface.
	want := [16]string{
		"reserved_0", "dp_deactivated", "dp_init", "dp_deinit", "dp_activated",
		"dp_tx_turn_on", "dp_tx_turn_off", "dp_initialized",
		"unknown", "unknown", "unknown", "unknown", "unknown", "unknown", "unknown", "unknown",
	}
	consts := map[byte]genums.CmisDataPathState{
		0x00: genums.CmisDataPathStateReserved0,
		0x01: genums.CmisDataPathStateDpDeactivated,
		0x02: genums.CmisDataPathStateDpInit,
		0x03: genums.CmisDataPathStateDpDeinit,
		0x04: genums.CmisDataPathStateDpActivated,
		0x05: genums.CmisDataPathStateDpTxTurnOn,
		0x06: genums.CmisDataPathStateDpTxTurnOff,
		0x07: genums.CmisDataPathStateDpInitialized,
	}
	for raw := 0; raw < 16; raw++ {
		var value genums.CmisDataPathState
		if c, ok := consts[byte(raw)]; ok {
			value = c
		} else {
			value = genums.CmisDataPathState(raw)
		}
		if got := value.Label(); got != want[raw] {
			t.Fatalf("flat Label() at raw %#04x: got %q, want %q", raw, got, want[raw])
		}
	}
}

// ─── Public normalizer behavior on the shared reserved fixture ─────────────
//
// These exercise the PUBLIC DecodeCmis entrypoint (not the projection
// factory) over the independently authored reserved fixture's pages, built
// in memory — no corpus file is modified. Expected non-label snapshot values
// come from the independently authored golden, never from a decoder run.

func unsupportedContextBytes() []byte {
	out := []byte{}
	for b := 0x31; b <= 0x3F; b++ {
		out = append(out, byte(b))
	}
	for b := 0x41; b <= 0x4F; b++ {
		out = append(out, byte(b))
	}
	for b := 0x55; b <= 0x5F; b++ {
		out = append(out, byte(b))
	}
	// 0x31, 0x41 and 0x55 are explicitly present in the sweep.
	for _, named := range []byte{0x31, 0x41, 0x55} {
		found := false
		for _, b := range out {
			if b == named {
				found = true
			}
		}
		if !found {
			panic("named unsupported context missing from sweep")
		}
	}
	return out
}

func reservedFixturePages(t *testing.T) Pages {
	t.Helper()
	repoRoot := filepath.Join("..", "..")
	fixDir := filepath.Join(repoRoot, "tests", "fixtures", "cmis")
	stem := "synthetic-cmis-3-0-qsfp-dd-dp-reserved"
	read := func(suffix string) []byte {
		data, err := os.ReadFile(filepath.Join(fixDir, stem+"."+suffix+".bin"))
		if err != nil || len(data) != 128 {
			t.Fatalf("read %s.%s: %v len=%d", stem, suffix, err, len(data))
		}
		return data
	}
	return Pages{
		Lower:   read("lower"),
		Page00h: read("page-00h"),
		Page01h: read("page-01h"),
		Page11h: read("page-11h"),
	}
}

func reservedGolden(t *testing.T) map[string]interface{} {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", "tests", "golden", "cmis", "synthetic-cmis-3-0-qsfp-dd-dp-reserved.json"))
	if err != nil {
		t.Fatalf("read reserved golden: %v", err)
	}
	var golden map[string]interface{}
	if err := json.Unmarshal(data, &golden); err != nil {
		t.Fatalf("parse reserved golden: %v", err)
	}
	return golden
}

func contextPages(base Pages, contextByte byte, raw byte) Pages {
	lower := append([]byte(nil), base.Lower...)
	lower[1] = contextByte
	p11 := append([]byte(nil), base.Page11h...)
	state := raw | (raw << 4)
	for i := 0; i < 4; i++ {
		p11[i] = state
	}
	return Pages{Lower: lower, Page00h: base.Page00h, Page01h: base.Page01h, Page11h: p11}
}

func TestDpsmProjectionUnsupportedPresentContextsKeepSnapshotAndUseUnknown(t *testing.T) {
	golden := reservedGolden(t)
	gIdentity := golden["identity"].(map[string]interface{})
	gMonitoring := golden["monitoring"].(map[string]interface{})
	gModule := golden["module"].(map[string]interface{})
	gFlags := golden["flags"].(map[string]interface{})
	base := reservedFixturePages(t)
	for _, ctx := range unsupportedContextBytes() {
		for raw := 0; raw <= 15; raw++ {
			result, err := DecodeCmis(contextPages(base, ctx, byte(raw)))
			if err != nil {
				t.Fatalf("ctx %#04x raw %#04x: DecodeCmis: %v", ctx, raw, err)
			}
			if result["decode_status"] != "ok" {
				t.Fatalf("ctx %#04x raw %#04x: admission changed: %v", ctx, raw, result)
			}
			// Admission/snapshot unchanged apart from revision text and labels.
			if !reflect.DeepEqual(result["identity"], gIdentity) {
				t.Fatalf("ctx %#04x raw %#04x: identity snapshot changed", ctx, raw)
			}
			if !reflect.DeepEqual(result["monitoring"], gMonitoring) {
				t.Fatalf("ctx %#04x raw %#04x: monitoring snapshot changed", ctx, raw)
			}
			if result["applications"] != nil {
				t.Fatalf("ctx %#04x raw %#04x: applications snapshot changed", ctx, raw)
			}
			module := result["module"].(map[string]interface{})
			wantRev := fmt.Sprintf("%d.%d", ctx>>4, ctx&0xF)
			if module["cmis_revision"] != wantRev {
				t.Fatalf("ctx %#04x: revision text %v, want %v", ctx, module["cmis_revision"], wantRev)
			}
			if module["memory_model"] != "paged" || module["module_state"] != gModule["module_state"] || module["module_type"] != nil {
				t.Fatalf("ctx %#04x raw %#04x: module snapshot changed: %v", ctx, raw, module)
			}
			flags := result["flags"].(map[string]interface{})
			if flags["interrupt_asserted"] != gFlags["interrupt_asserted"] {
				t.Fatalf("ctx %#04x raw %#04x: flag snapshot changed", ctx, raw)
			}
			if !reflect.DeepEqual(flags["lanes"], gFlags["lanes"]) {
				t.Fatalf("ctx %#04x raw %#04x: lane-flag snapshot changed", ctx, raw)
			}
			// Every present raw renders Unknown<n> — including Unknown0..Unknown4.
			want := fmt.Sprintf("Unknown%d", raw)
			lanes := result["lanes"].([]interface{})
			if len(lanes) != 8 {
				t.Fatalf("ctx %#04x raw %#04x: lane count %d", ctx, raw, len(lanes))
			}
			for i, laneIface := range lanes {
				lane := laneIface.(map[string]interface{})
				if lane["dp_state"] != want {
					t.Fatalf("ctx %#04x raw %#04x lane %d: dp_state %v, want %q",
						ctx, raw, i+1, lane["dp_state"], want)
				}
				gLane := golden["lanes"].([]interface{})[i].(map[string]interface{})
				if float64(lane["index"].(int)) != gLane["index"].(float64) {
					t.Fatalf("ctx %#04x raw %#04x lane %d: index changed to %v (golden %v)",
						ctx, raw, i+1, lane["index"], gLane["index"])
				}
				for _, key := range []string{"rx_power_mw", "tx_bias_ma", "tx_power_mw"} {
					if !reflect.DeepEqual(lane[key], gLane[key]) {
						t.Fatalf("ctx %#04x raw %#04x lane %d: %v changed to %v (golden %v)",
							ctx, raw, i+1, key, lane[key], gLane[key])
					}
				}
			}
		}
	}
}

func TestDpsmProjectionMissingPage11BehaviorIsUnchanged(t *testing.T) {
	base := reservedFixturePages(t)
	// Paged module without Page 11h: existing null lane/flag blocks.
	result, err := DecodeCmis(Pages{Lower: base.Lower, Page00h: base.Page00h, Page01h: base.Page01h})
	if err != nil {
		t.Fatalf("DecodeCmis without page 11h: %v", err)
	}
	if result["decode_status"] != "ok" || result["lanes"] != nil || result["flags"] != nil {
		t.Fatalf("missing page 11h behavior changed: %v", result)
	}
	// Flat-memory module: null blocks even with Page 11h bytes supplied.
	flat := append([]byte(nil), base.Lower...)
	flat[2] |= 0x80
	result, err = DecodeCmis(Pages{Lower: flat, Page00h: base.Page00h, Page01h: base.Page01h, Page11h: base.Page11h})
	if err != nil {
		t.Fatalf("DecodeCmis flat: %v", err)
	}
	if result["decode_status"] != "ok" || result["lanes"] != nil || result["flags"] != nil {
		t.Fatalf("flat-memory behavior changed: %v", result)
	}
	// Page 11h present without Page 01h: existing explicit prerequisite error.
	_, err = DecodeCmis(Pages{Lower: base.Lower, Page00h: base.Page00h, Page11h: base.Page11h})
	if err == nil || !strings.Contains(err.Error(), "page 01h missing") {
		t.Fatalf("prerequisite error changed: %v", err)
	}
}
