package openxcvr

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

var expectedHIF0x53 = []int{81, 82, 79, 80, 77, 78, 75, 76, 81, 82, 79, 80, 77, 78, 75}
var revCases = []byte{0x30, 0x40, 0x50, 0x51, 0x52, 0x53, 0x54}

func loadZRBuffers(t *testing.T) ([]byte, []byte, []byte) {
	fixtureDir := filepath.Join("..", "..", "tests", "fixtures", "cmis")
	stem := "fboss-cmis-5-3-osfp-800g-zr"
	lower, err := os.ReadFile(filepath.Join(fixtureDir, stem+".lower.bin"))
	if err != nil {
		t.Fatalf("read lower: %v", err)
	}
	p00, err := os.ReadFile(filepath.Join(fixtureDir, stem+".page-00h.bin"))
	if err != nil {
		t.Fatalf("read page00h: %v", err)
	}
	p01, err := os.ReadFile(filepath.Join(fixtureDir, stem+".page-01h.bin"))
	if err != nil {
		t.Fatalf("read page01h: %v", err)
	}
	return lower, p00, p01
}

func getAppCodesSlice(result map[string]interface{}) []int {
	appsRaw, ok := result["applications"].([]interface{})
	if !ok || len(appsRaw) == 0 {
		return nil
	}
	var codes []int
	for _, app := range appsRaw {
		codes = append(codes, getAppCode(app))
	}
	return codes
}

func getAppHIFsSlice(result map[string]interface{}) []int {
	appsRaw, ok := result["applications"].([]interface{})
	if !ok || len(appsRaw) == 0 {
		return nil
	}
	var hifs []int
	for _, app := range appsRaw {
		if entry, ok := app.(appEntry); ok {
			hifs = append(hifs, entry.HostInterfaceID)
		} else if m, ok := app.(map[string]interface{}); ok {
			if f, ok := m["host_interface_id"].(float64); ok {
				hifs = append(hifs, int(f))
			} else if n, ok := m["host_interface_id"].(int); ok {
				hifs = append(hifs, n)
			}
		}
	}
	return hifs
}

func TestApplicationNumberingRevisionsAdditive(t *testing.T) {
	// (a) For each Lower:1 revision byte 0x30..0x53: app codes [1..15], and for 0x53 HIF matches.
	lowerOrig, p00, p01 := loadZRBuffers(t)
	expectedCodes := []int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15}

	for _, rev := range revCases {
		testName := fmt.Sprintf("rev_0x%02X", rev)
		t.Run(testName, func(t *testing.T) {
			lower := make([]byte, len(lowerOrig))
			copy(lower, lowerOrig)
			lower[1] = rev

			pages := Pages{
				Lower:   lower,
				Page00h: p00,
				Page01h: p01,
			}
			res, err := DecodeCmis(pages)
			if err != nil {
				t.Fatalf("DecodeCmis: %v", err)
			}
			codes := getAppCodesSlice(res)
			if !reflect.DeepEqual(codes, expectedCodes) {
				t.Errorf("got codes %v, expected %v", codes, expectedCodes)
			}
			if rev == 0x53 {
				hifs := getAppHIFsSlice(res)
				if !reflect.DeepEqual(hifs, expectedHIF0x53) {
					t.Errorf("got hifs %v, expected %v", hifs, expectedHIF0x53)
				}
			}
		})
	}
}

func TestApplicationNumberingPage01hNotSupplied(t *testing.T) {
	// (b) Page 01h not supplied: app codes exactly [1..8].
	lowerOrig, p00, _ := loadZRBuffers(t)
	expectedCodes := []int{1, 2, 3, 4, 5, 6, 7, 8}

	pages := Pages{
		Lower:   lowerOrig,
		Page00h: p00,
	}
	res, err := DecodeCmis(pages)
	if err != nil {
		t.Fatalf("DecodeCmis: %v", err)
	}
	codes := getAppCodesSlice(res)
	if !reflect.DeepEqual(codes, expectedCodes) {
		t.Errorf("got codes %v, expected %v", codes, expectedCodes)
	}
}

func TestApplicationNumberingLowerSlot3Terminator(t *testing.T) {
	// (c) Lower slot 3 HIF set to 0xFF (terminator): Lower yields [1,2], Page 01h still yields [9..15].
	// Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
	lowerOrig, p00, p01 := loadZRBuffers(t)
	lower := make([]byte, len(lowerOrig))
	copy(lower, lowerOrig)
	// Lower slot 3 descriptor is at bytes 86 + 2*4 = 94. Byte 94 is HIF.
	lower[94] = 0xFF
	expectedCodes := []int{1, 2, 9, 10, 11, 12, 13, 14, 15}

	pages := Pages{
		Lower:   lower,
		Page00h: p00,
		Page01h: p01,
	}
	res, err := DecodeCmis(pages)
	if err != nil {
		t.Fatalf("DecodeCmis: %v", err)
	}
	codes := getAppCodesSlice(res)
	if !reflect.DeepEqual(codes, expectedCodes) {
		t.Errorf("got codes %v, expected %v", codes, expectedCodes)
	}
}

func TestApplicationNumberingPage01hAllMIFZero(t *testing.T) {
	// (d) Every Page 01h descriptor MIF set to 0: app codes [1..8] (MIF-0 skip preserved).
	// Preservation of existing facade behaviour, not normative spec rules (design-decision A3).
	lowerOrig, p00, p01Orig := loadZRBuffers(t)
	p01 := make([]byte, len(p01Orig))
	copy(p01, p01Orig)
	// Page 01h has 7 descriptors at buffer offsets 95-122.
	// Each descriptor is 4 bytes: [HIF, MIF, ...]. MIF is offset + 1.
	for i := 0; i < 7; i++ {
		mifOffset := 95 + i*4 + 1
		p01[mifOffset] = 0
	}
	expectedCodes := []int{1, 2, 3, 4, 5, 6, 7, 8}

	pages := Pages{
		Lower:   lowerOrig,
		Page00h: p00,
		Page01h: p01,
	}
	res, err := DecodeCmis(pages)
	if err != nil {
		t.Fatalf("DecodeCmis: %v", err)
	}
	codes := getAppCodesSlice(res)
	if !reflect.DeepEqual(codes, expectedCodes) {
		t.Errorf("got codes %v, expected %v", codes, expectedCodes)
	}
}
