package openxcvr

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSff8636ExtSpecUnknownFallback(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	fixtureDir := filepath.Join(repoRoot, "tests", "fixtures", "sff-8636")
	lowerBytes, err := os.ReadFile(filepath.Join(fixtureDir, "finisar-ftl410qe3c-40g-sr4.lower.bin"))
	if err != nil {
		t.Fatalf("failed to read lower fixture: %v", err)
	}
	page00hBytes, err := os.ReadFile(filepath.Join(fixtureDir, "finisar-ftl410qe3c-40g-sr4.page-00h.bin"))
	if err != nil {
		t.Fatalf("failed to read page00h fixture: %v", err)
	}

	// Mutate page-00h byte 64 (memory offset 192, ExtSpecCompliance) to 0x03.
	mutatedPage00h := make([]byte, len(page00hBytes))
	copy(mutatedPage00h, page00hBytes)
	mutatedPage00h[64] = 0x03

	res, err := DecodeSff8636(Sff8636Pages{
		Lower:   lowerBytes,
		Page00h: mutatedPage00h,
	})
	if err != nil {
		t.Fatalf("DecodeSff8636 failed: %v", err)
	}

	compliance, ok := res["compliance"].(map[string]interface{})
	if !ok {
		t.Fatalf("compliance missing or not a map: %v", res["compliance"])
	}

	extSpec, ok := compliance["ext_spec_byte_192"].(map[string]interface{})
	if !ok {
		t.Fatalf("compliance.ext_spec_byte_192 missing or not a map: %v", compliance["ext_spec_byte_192"])
	}

	name, _ := extSpec["name"].(string)
	raw, _ := extSpec["raw"].(string)

	if name != "unknown" {
		t.Errorf("expected compliance.ext_spec_byte_192.name == \"unknown\", got %q", name)
	}
	if raw != "0x03" {
		t.Errorf("expected compliance.ext_spec_byte_192.raw == \"0x03\", got %q", raw)
	}
}
