package openxcvr

import (
	"strings"
	"testing"
)

// Regression (bindings/README.md facade contract): a present buffer —
// including an empty one — is supplied and must never become absence.
// Before the fix, DecodeCmis silently treated a present-but-short Page 11h
// (and Page 01h) as absent, while the TS facade threw RangeError.

func syntheticCmisLowerPaged() []byte {
	page := make([]byte, 128)
	page[1] = 0x53 // CMIS 5.3, paged memory
	return page
}

func cmisCanonicalPages(extra map[string][]byte) map[string][]byte {
	pages := map[string][]byte{
		"lower": syntheticCmisLowerPaged(),
		"00h":   make([]byte, 128),
	}
	for k, v := range extra {
		pages[k] = v
	}
	return pages
}

func TestCmisPresentButEmptyPage11hErrors(t *testing.T) {
	pages, err := CmisPagesFromCanonical(cmisCanonicalPages(map[string][]byte{"11h": {}}))
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	if _, err := DecodeCmis(pages); err == nil {
		t.Fatal("DecodeCmis accepted a present-but-empty page 11h: a supplied page must not become absence")
	} else if !strings.Contains(err.Error(), "11h") {
		t.Fatalf("error must name the page: %v", err)
	}
}

func TestCmisPresentNilPage11hIsSupplied(t *testing.T) {
	// A present key holding a nil slice is still a present page: the
	// adapter preserves presence instead of letting nil read as absence.
	pages, err := CmisPagesFromCanonical(cmisCanonicalPages(map[string][]byte{"11h": nil}))
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	if _, err := DecodeCmis(pages); err == nil {
		t.Fatal("DecodeCmis accepted a present nil page 11h")
	}
}

func TestCmisAbsentPage11hStillDecodes(t *testing.T) {
	pages, err := CmisPagesFromCanonical(cmisCanonicalPages(nil))
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	result, err := DecodeCmis(pages)
	if err != nil {
		t.Fatalf("DecodeCmis: %v", err)
	}
	if result["decode_status"] != "ok" {
		t.Fatalf("decode_status = %#v", result["decode_status"])
	}
	if result["lanes"] != nil || result["flags"] != nil {
		t.Fatalf("absent 11h must decode without lanes/flags: %#v / %#v", result["lanes"], result["flags"])
	}
}

func TestCmisPresentShortPage01hErrors(t *testing.T) {
	pages, err := CmisPagesFromCanonical(cmisCanonicalPages(map[string][]byte{"01h": make([]byte, 64)}))
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	if _, err := DecodeCmis(pages); err == nil {
		t.Fatal("DecodeCmis accepted a present-but-short page 01h")
	} else if !strings.Contains(err.Error(), "01h") {
		t.Fatalf("error must name the page: %v", err)
	}
}

func TestCmisPresentButEmptyVdmPagesFailClosed(t *testing.T) {
	extra := map[string][]byte{}
	for _, key := range []string{
		"20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
		"28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
	} {
		extra[key] = []byte{}
	}
	pages, err := CmisPagesFromCanonical(cmisCanonicalPages(extra))
	if err != nil {
		t.Fatalf("canonical pages: %v", err)
	}
	if _, err := DecodeCmis(pages); err == nil {
		t.Fatal("DecodeCmis accepted a present-but-empty VDM page set")
	}
}
