package openxcvr

import (
	"path/filepath"
	"testing"
)

// Independent oracle for the SFF-8636 lane signal flags: the Table 6-5 bit
// positions read straight from the fixture bytes, not from generator
// output. Rev 0x07 (2.5) models every flag, including adaptive-EQ.
func TestSff8636LaneSignalFlagsFixture(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8636")
	lower := readFile(t, filepath.Join(dir, "finisar-ftlc9551repm-100g-sr4.lower.bin"))

	lanes, err := DecodeSff8636LaneSignalFlags(Sff8636Pages{Lower: lower})
	if err != nil {
		t.Fatalf("DecodeSff8636LaneSignalFlags: %v", err)
	}
	if len(lanes) != 4 {
		t.Fatalf("lane count %d != 4", len(lanes))
	}
	bit := func(b byte, n int) bool { return b&(1<<uint(n)) != 0 }
	for i := 0; i < 4; i++ {
		want := map[string]bool{
			"tx_los":               bit(lower[3], 4+i),
			"rx_los":               bit(lower[3], i),
			"tx_adaptive_eq_fault": bit(lower[4], 4+i),
			"tx_fault":             bit(lower[4], i),
			"tx_cdr_lol":           bit(lower[5], 4+i),
			"rx_cdr_lol":           bit(lower[5], i),
			"tx_disable":           bit(lower[86], i),
		}
		for key, w := range want {
			got, ok := lanes[i][key]
			if !ok {
				t.Fatalf("lane %d: key %q missing at rev 2.5", i+1, key)
			}
			if got != w {
				t.Errorf("lane %d %s = %v, want %v", i+1, key, got, w)
			}
		}
	}
	// The fixture sets byte 3 = 0xff and byte 5 = 0xff (every LOS and CDR
	// LOL latched), byte 4 = 0x00 (no fault, no adaptive-EQ fail).
	if lanes[0]["tx_los"] != true || lanes[0]["rx_los"] != true {
		t.Errorf("lane 1 LOS flags = %v/%v, want true/true", lanes[0]["tx_los"], lanes[0]["rx_los"])
	}
	if lanes[0]["tx_adaptive_eq_fault"] != false {
		t.Errorf("lane 1 adaptive EQ = %v, want false", lanes[0]["tx_adaptive_eq_fault"])
	}
}

// The adaptive-EQ flag carries a schema revision window (`since: "2.0"`):
// detected revs 1.3-1.5 (0x03-0x05) omit the key, revs >= 2.0 and the
// unadmitted union view surface it — never a borrowed later-rev reading.
func TestSff8636LaneSignalFlagsAdaptiveEqRevisionWindow(t *testing.T) {
	for rev, modeled := range map[int]bool{0x03: false, 0x05: false, 0x06: true, 0x0A: true, 0x00: true, 0x7F: true} {
		lower := make([]byte, 128)
		lower[1] = byte(rev)
		lower[4] = 0xF0 // every Tx adaptive-EQ bit set
		lanes, err := DecodeSff8636LaneSignalFlags(Sff8636Pages{Lower: lower})
		if err != nil {
			t.Fatalf("rev %#02x: %v", rev, err)
		}
		_, present := lanes[0]["tx_adaptive_eq_fault"]
		if present != modeled {
			t.Errorf("rev %#02x: tx_adaptive_eq_fault present = %v, want %v", rev, present, modeled)
		}
		if present && lanes[0]["tx_adaptive_eq_fault"] != true {
			t.Errorf("rev %#02x: adaptive EQ bit must decode as set", rev)
		}
	}
}

// Independent oracle for the SFF-8472 lane flags: Table 9-16 byte-110
// per-bit fields plus the A2h:118.4 adaptive-EQ flag. The Finisar capture
// is rev 10.2 (0x03) — the adaptive-EQ bit is not modeled there, so the
// key must be absent.
func TestSff8472LaneSignalFlagsFixture(t *testing.T) {
	dir := filepath.Join("..", "..", "tests", "fixtures", "sff-8472")
	a0h := readFile(t, filepath.Join(dir, "finisar-ftlx8571d3bcl-mup0wb0.a0h.bin"))
	a2h := readFile(t, filepath.Join(dir, "finisar-ftlx8571d3bcl-mup0wb0.a2h.bin"))

	lanes, err := DecodeSff8472LaneSignalFlags(Sff8472Pages{A0h: a0h, A2h: a2h})
	if err != nil {
		t.Fatalf("DecodeSff8472LaneSignalFlags: %v", err)
	}
	if len(lanes) != 1 {
		t.Fatalf("lane count %d != 1", len(lanes))
	}
	lane := lanes[0]
	for key, want := range map[string]bool{
		"rx_los":     a2h[110]&(1<<1) != 0,
		"tx_fault":   a2h[110]&(1<<2) != 0,
		"tx_disable": a2h[110]&(1<<7) != 0,
	} {
		if lane[key] != want {
			t.Errorf("%s = %v, want %v", key, lane[key], want)
		}
	}
	if _, present := lane["tx_adaptive_eq_fault"]; present {
		t.Errorf("tx_adaptive_eq_fault present at rev 10.2 (A0h:94 = %#02x)", a0h[94])
	}
	// SFF-8472 defines no Tx LOS / CDR LOL — the keys stay absent.
	for _, key := range []string{"tx_los", "tx_cdr_lol", "rx_cdr_lol"} {
		if _, present := lane[key]; present {
			t.Errorf("%s present, but SFF-8472 defines no such flag", key)
		}
	}
}

// At rev 12.5 (0x0A) — and on the unadmitted union view — the A2h:118.4
// adaptive-EQ flag surfaces; below 12.5 it must not.
func TestSff8472LaneSignalFlagsAdaptiveEqRevisionWindow(t *testing.T) {
	for rev, modeled := range map[int]bool{0x01: false, 0x09: false, 0x0A: true, 0x00: true} {
		a0h := make([]byte, 256)
		a0h[94] = byte(rev)
		a2h := make([]byte, 256)
		a2h[118] = 0x10
		lanes, err := DecodeSff8472LaneSignalFlags(Sff8472Pages{A0h: a0h, A2h: a2h})
		if err != nil {
			t.Fatalf("rev %#02x: %v", rev, err)
		}
		got, present := lanes[0]["tx_adaptive_eq_fault"]
		if !modeled && present {
			t.Errorf("rev %#02x: key must be absent before 12.5", rev)
		}
		if modeled && (!present || got != true) {
			t.Errorf("rev %#02x: want decoded true, got %v (present %v)", rev, got, present)
		}
	}
}
