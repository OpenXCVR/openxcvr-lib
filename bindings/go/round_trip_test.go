package openxcvr

// Round-trip tests (encode-round-trip Wave 2, Go): Set<Type><Field> is the
// exact inverse of Decode; writing decoded values back is byte-identical.

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	gen "github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

func loadPage00h(t *testing.T) []byte {
	t.Helper()
	p := filepath.Join("..", "..", "tests", "fixtures", "cmis",
		"fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin")
	data, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	return data[:128]
}

func TestU8SetterIsDecodeInverse(t *testing.T) {
	buf := make([]byte, 128)
	gen.SetCmisPage00hAdministrativeMaxPower(buf, 42)
	d, err := gen.DecodeCmisPage00hAdministrative(buf)
	if err != nil {
		t.Fatal(err)
	}
	if d.MaxPower != 42 {
		t.Fatalf("max_power = %d, want 42", d.MaxPower)
	}
}

func TestWritingDecodedValuesBackIsIdentity(t *testing.T) {
	orig := loadPage00h(t)
	d, err := gen.DecodeCmisPage00hAdministrative(orig)
	if err != nil {
		t.Fatal(err)
	}
	cp := make([]byte, len(orig))
	copy(cp, orig)
	gen.SetCmisPage00hAdministrativeMaxPower(cp, d.MaxPower)
	gen.SetCmisPage00hAdministrativeVendorName(cp, d.VendorName)
	gen.SetCmisPage00hAdministrativeVendorOui(cp, d.VendorOui)
	gen.SetCmisPage00hAdministrativeVendorPn(cp, d.VendorPn)
	if !bytes.Equal(cp, orig) {
		t.Fatal("writing decoded values back must be a no-op")
	}
}

func TestBitFieldSetterPreservesNeighbors(t *testing.T) {
	buf := make([]byte, 128)
	buf[82] = 0xFF
	gen.SetCmisPage00hAdministrativeMediaLaneUnsupportedLane1(buf, false)
	if buf[82] != 0xFE {
		t.Fatalf("byte 82 = %#x, want 0xFE", buf[82])
	}
	gen.SetCmisPage00hAdministrativeMediaLaneUnsupportedLane2(buf, false)
	if buf[82] != 0xFC {
		t.Fatalf("byte 82 = %#x, want 0xFC", buf[82])
	}
}
