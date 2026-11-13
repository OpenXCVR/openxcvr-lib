package openxcvr

import (
	"bytes"
	"strings"
	"testing"
)

// Consumer example: a synthetic canonical page map, no hardware, fed to the
// public facade. Lane "index" in the result is 1-based.

func syntheticSff8636Lower() []byte {
	page := make([]byte, 128)
	page[1] = 0x07
	page[3] = 0x11
	page[86] = 0x01
	page[22] = 0x02
	page[23] = 0x00
	return page
}

func syntheticSff8636Page03h() []byte {
	page := make([]byte, 128)
	copy(page[0:8], []byte{0x01, 0x00, 0xFF, 0x00, 0x00, 0x80, 0xFF, 0x80})
	return page
}

func TestSyntheticSff8636FacadeFromCanonicalPages(t *testing.T) {
	pages, err := Sff8636PagesFromCanonical(map[string][]byte{
		"lower": syntheticSff8636Lower(),
		"00h":   make([]byte, 128),
		"03h":   syntheticSff8636Page03h(),
	})
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	lanes, err := DecodeSff8636LaneSignalFlags(pages)
	if err != nil {
		t.Fatalf("lane flags: %v", err)
	}
	if len(lanes) != 4 {
		t.Fatalf("lane count %d", len(lanes))
	}
	if lanes[0]["index"] != 1 || lanes[0]["rx_los"] != true || lanes[0]["tx_los"] != true || lanes[0]["tx_disable"] != true {
		t.Fatalf("lane 1 = %#v", lanes[0])
	}
	if lanes[0]["tx_adaptive_eq_fault"] != false {
		t.Fatalf("adaptive eq = %#v", lanes[0]["tx_adaptive_eq_fault"])
	}
	if lanes[1]["rx_los"] != false {
		t.Fatalf("lane 2 rx_los = %#v", lanes[1]["rx_los"])
	}

	evaluated, err := EvaluateSff8636Thresholds(pages)
	if err != nil {
		t.Fatalf("thresholds: %v", err)
	}
	temp := evaluated["temperature"].(map[string]interface{})
	if temp["high_alarm"] != true || temp["high_warning"] != true || temp["low_alarm"] != false || temp["low_warning"] != false {
		t.Fatalf("temperature = %#v", temp)
	}
	supply := evaluated["supply_voltage"].(map[string]interface{})
	for _, key := range []string{"high_alarm", "low_alarm", "high_warning", "low_warning"} {
		if supply[key] != false {
			t.Fatalf("supply %s = %#v", key, supply[key])
		}
	}
}

func TestCanonicalPagesFailClosed(t *testing.T) {
	_, err := Sff8636PagesFromCanonical(map[string][]byte{"00h": make([]byte, 128)})
	if err == nil || !strings.Contains(err.Error(), "lower") {
		t.Fatalf("missing lower: %v", err)
	}
	_, err = Sff8636PagesFromCanonical(map[string][]byte{
		"lower": syntheticSff8636Lower(),
		"00h":   make([]byte, 128),
		"99h":   []byte{1},
	})
	if err == nil || !strings.Contains(err.Error(), "99h") {
		t.Fatalf("unknown key: %v", err)
	}
	pages, err := Sff8636PagesFromCanonical(map[string][]byte{
		"lower": syntheticSff8636Lower(),
		"00h":   bytes.Repeat([]byte{0}, 128),
	})
	if err != nil {
		t.Fatalf("without 03h: %v", err)
	}
	if pages.Page03h != nil {
		t.Fatal("omitted 03h was fabricated")
	}
	if _, err := EvaluateSff8636Thresholds(pages); err == nil {
		t.Fatal("missing page 03h evaluated")
	}
}

func TestSyntheticSff8472LaneFlagsFromCanonicalPages(t *testing.T) {
	a0h := make([]byte, 256)
	a0h[92] = 0x20
	a0h[94] = 0x01
	a2h := make([]byte, 256)
	a2h[110] = 0x02
	a2h[118] = 0x10
	pages, err := Sff8472PagesFromCanonical(map[string][]byte{"a0h": a0h, "a2h": a2h})
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	lanes, err := DecodeSff8472LaneSignalFlags(pages)
	if err != nil {
		t.Fatalf("lane flags: %v", err)
	}
	if len(lanes) != 1 || lanes[0]["index"] != 1 || lanes[0]["rx_los"] != true {
		t.Fatalf("lane = %#v", lanes)
	}
	if _, ok := lanes[0]["tx_adaptive_eq_fault"]; ok {
		t.Fatal("pre-12.5 adaptive-EQ flag was borrowed")
	}
	if _, ok := lanes[0]["tx_los"]; ok {
		t.Fatal("SFF-8472 emitted tx_los")
	}
}
