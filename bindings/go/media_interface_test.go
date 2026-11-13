package openxcvr

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

var mediaInterfaceCases = []struct {
	mediaType byte
	mif       byte
	expected  string
}{
	{0x01, 0x01, "base_10g_sw"},
	{0x01, 0x1C, "fc_128g_mm"},
	{0x01, 0xC0, "vendor_specific"},
	{0x02, 0x01, "base_10g_lw"},
	{0x02, 0x1C, "base_400g_dr4"},
	{0x02, 0x68, "flexo_8e_dpo_16qam_foic8e_8_dpo"},
	{0x02, 0xC0, "vendor_specific"},
	{0x02, 0xC1, "vendor_specific"},
	{0x02, 0xF7, "vendor_specific_f7"},
	{0x03, 0x01, "copper_cable"},
	{0x03, 0x1C, "reserved_02_be"},
	{0x03, 0xC0, "linear_active_copper_loopback_module"},
	{0x03, 0xC1, "vendor_specific"},
	{0x04, 0x01, "active_cable_ber_1e_12"},
	{0x04, 0x1C, "reserved_05_be"},
	{0x04, 0xF7, "vendor_specific"},
	{0x05, 0x01, "base_1000_t"},
	{0x05, 0xC1, "vendor_specific"},
	{0x00, 0x1C, "unknown"},
	{0x06, 0x1C, "unknown"},
	{0x40, 0x1C, "unknown"},
	{0x90, 0x1C, "unknown"},
	{0xFF, 0x1C, "unknown"},
}

func getAppMediaInterfaceName(v interface{}) string {
	if entry, ok := v.(appEntry); ok {
		return entry.MediaInterfaceName
	}
	if m, ok := v.(map[string]interface{}); ok {
		if s, ok := m["media_interface_name"].(string); ok {
			return s
		}
	}
	return ""
}

func getAppCode(v interface{}) int {
	if entry, ok := v.(appEntry); ok {
		return entry.AppCode
	}
	if m, ok := v.(map[string]interface{}); ok {
		if f, ok := m["app_code"].(float64); ok {
			return int(f)
		}
		if n, ok := m["app_code"].(int); ok {
			return n
		}
	}
	return -1
}

func TestMediaInterfaceLowerCaller(t *testing.T) {
	fixtureDir := filepath.Join("..", "..", "tests", "fixtures", "cmis")
	lowerPath := filepath.Join(fixtureDir, "fboss-cmis-4-0-qsfpdd-400g-dr4.lower.bin")
	lowerOrig, err := os.ReadFile(lowerPath)
	if err != nil {
		t.Fatalf("read lower: %v", err)
	}
	page00hOrig, _ := os.ReadFile(filepath.Join(fixtureDir, "fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin"))
	page01hOrig, _ := os.ReadFile(filepath.Join(fixtureDir, "fboss-cmis-4-0-qsfpdd-400g-dr4.page-01h.bin"))
	page11hOrig, _ := os.ReadFile(filepath.Join(fixtureDir, "fboss-cmis-4-0-qsfpdd-400g-dr4.page-11h.bin"))

	for _, tc := range mediaInterfaceCases {
		testName := fmt.Sprintf("0x%02X_0x%02X_%s", tc.mediaType, tc.mif, tc.expected)
		t.Run(testName, func(t *testing.T) {
			lower := make([]byte, len(lowerOrig))
			copy(lower, lowerOrig)
			lower[85] = tc.mediaType
			lower[87] = tc.mif

			pages := Pages{
				Lower:   lower,
				Page00h: page00hOrig,
				Page01h: page01hOrig,
				Page11h: page11hOrig,
			}
			result, err := DecodeCmis(pages)
			if err != nil {
				t.Fatalf("DecodeCmis error: %v", err)
			}
			apps, ok := result["applications"].([]interface{})
			if !ok || len(apps) == 0 {
				t.Fatalf("expected non-empty applications")
			}
			actual := getAppMediaInterfaceName(apps[0])
			if actual != tc.expected {
				t.Errorf("lower caller: got %q, expected %q", actual, tc.expected)
			}
		})
	}
}

func TestMediaInterfacePage01Caller(t *testing.T) {
	fixtureDir := filepath.Join("..", "..", "tests", "fixtures", "cmis")
	stem := "fboss-cmis-4-0-osfp-2x400g-dr4"
	lowerOrig, err := os.ReadFile(filepath.Join(fixtureDir, stem+".lower.bin"))
	if err != nil {
		t.Fatalf("read lower: %v", err)
	}
	page00hOrig, _ := os.ReadFile(filepath.Join(fixtureDir, stem+".page-00h.bin"))
	page01hOrig, err := os.ReadFile(filepath.Join(fixtureDir, stem+".page-01h.bin"))
	if err != nil {
		t.Fatalf("read page01h: %v", err)
	}
	page11hOrig, _ := os.ReadFile(filepath.Join(fixtureDir, stem+".page-11h.bin"))

	for _, tc := range mediaInterfaceCases {
		testName := fmt.Sprintf("0x%02X_0x%02X_%s", tc.mediaType, tc.mif, tc.expected)
		t.Run(testName, func(t *testing.T) {
			lower := make([]byte, len(lowerOrig))
			copy(lower, lowerOrig)
			lower[85] = tc.mediaType

			page01h := make([]byte, len(page01hOrig))
			copy(page01h, page01hOrig)
			page01h[96] = tc.mif

			pages := Pages{
				Lower:   lower,
				Page00h: page00hOrig,
				Page01h: page01h,
				Page11h: page11hOrig,
			}
			result, err := DecodeCmis(pages)
			if err != nil {
				t.Fatalf("DecodeCmis error: %v", err)
			}
			apps, ok := result["applications"].([]interface{})
			if !ok || len(apps) == 0 {
				t.Fatalf("expected non-empty applications")
			}
			var entry interface{}
			for _, a := range apps {
				if getAppCode(a) == 9 {
					entry = a
					break
				}
			}
			if entry == nil {
				t.Fatalf("entry with app_code 9 not found")
			}
			actual := getAppMediaInterfaceName(entry)
			if actual != tc.expected {
				t.Errorf("page01 caller: got %q, expected %q", actual, tc.expected)
			}
		})
	}
}
