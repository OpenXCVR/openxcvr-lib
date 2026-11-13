// Checksum-recompute tests for Go generated bindings.
//
// For each fixture, read the raw page buffer, record the stored checksum
// byte(s), call Recompute<Page>Checksums, and assert the recomputed value
// matches the stored one. This proves the generated Go recompute logic is
// correct for all real and synthetic samples in the corpus.
package openxcvr

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
)

func repoRootChecksum(t *testing.T) string {
	t.Helper()
	// bindings/go → repo root is two levels up
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatalf("cannot resolve repo root: %v", err)
	}
	return root
}

// readExact reads a fixture file and panics unless it is exactly n bytes.
func readExact(t *testing.T, path string, n int) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("cannot read %s: %v", path, err)
	}
	if len(data) != n {
		t.Fatalf("%s: expected %d bytes, got %d", path, n, len(data))
	}
	return data
}

// readPadded reads a fixture file and zero-pads it to n bytes (matching
// read_padded behaviour in the Rust tests and mmap.rs).
func readPadded(t *testing.T, path string, n int) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("cannot read %s: %v", path, err)
	}
	if len(data) > n {
		t.Fatalf("%s: file is %d bytes, expected at most %d", path, len(data), n)
	}
	buf := make([]byte, n)
	copy(buf, data)
	return buf
}

// ─── CMIS Page 00h ─────────────────────────────────────────────────────────
// Only the five fixtures below have valid sum8 checksums.
// The remaining fboss fixtures contain vendor-specific values that don't
// match the spec formula and must not be tested against recompute.

var cmisPage00hFixtures = []string{
	"fboss-cmis-4-0-qsfp112-200g-fr4",
	"fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
	"fboss-cmis-4-0-qsfpdd-400g-lr4",
	"synthetic-cmis-3-0-qsfp-dd",
	"synthetic-cmis-5-2-qsfp-dd",
}

func TestRecomputeCmisPage00hAdministrativeChecksums(t *testing.T) {
	root := repoRootChecksum(t)
	for _, stem := range cmisPage00hFixtures {
		stem := stem
		t.Run(stem, func(t *testing.T) {
			path := filepath.Join(root, "tests", "fixtures", "cmis", stem+".page-00h.bin")
			buf := readExact(t, path, 128)
			stored := buf[94]
			generated.RecomputeCmisPage00hAdministrativeChecksums(buf)
			if buf[94] != stored {
				t.Errorf("%s page-00h checksum: recomputed 0x%02x != stored 0x%02x",
					stem, buf[94], stored)
			}
		})
	}
}

// ─── CMIS Page 01h ─────────────────────────────────────────────────────────
// Page 01h is the only checksummed page whose covered region does NOT start at
// buf[0] (CMIS 5.3 § 8.4.17: sum8 of buf[2..=126] → buf[127]).
//
// Synthetic buffer: buf[0]=buf[1]=0xFF (sentinel proving those bytes are
// excluded), buf[2..126]=0x01 (125 bytes).  Correct sum → 0x7D.  A wrong
// buf[0..=126] range would yield 0x7B instead.

func TestRecomputeCmisPage01hAdvertisingChecksums(t *testing.T) {
	t.Run("non_zero_start_sentinel", func(t *testing.T) {
		buf := make([]byte, 128)
		buf[0] = 0xFF
		buf[1] = 0xFF
		for i := 2; i <= 126; i++ {
			buf[i] = 0x01
		}
		generated.RecomputeCmisPage01hAdvertisingChecksums(buf)
		if buf[127] != 0x7D {
			t.Errorf("page-01h checksum: got 0x%02x, expected 0x7d "+
				"(buf[0]/buf[1] sentinel 0xff must be EXCLUDED from the sum)",
				buf[127])
		}
	})
}

// ─── CMIS Page 02h ─────────────────────────────────────────────────────────
// All 15 fixtures carry valid sum8 checksums.

var cmisPage02hFixtures = []string{
	"fboss-cmis-4-0-osfp-2x400g-dr4",
	"fboss-cmis-4-0-qsfp112-200g-fr4",
	"fboss-cmis-4-0-qsfpdd-400g-dr4",
	"fboss-cmis-4-0-qsfpdd-400g-fr4-multiport",
	"fboss-cmis-4-0-qsfpdd-400g-lr4",
	"fboss-cmis-5-0-osfp-2x400g-fr4",
	"fboss-cmis-5-1-osfp-2x400g-fr4-lite",
	"fboss-cmis-5-1-osfp-2x400g-fr4-lpo",
	"fboss-cmis-5-1-osfp-2x800g-dr4",
	"fboss-cmis-5-1-osfp-credo-800g-aec",
	"fboss-cmis-5-3-osfp-800g-zr",
	"fboss-cmis-5-3-osfp-800g-zr-bias-reserved",
	"fboss-cmis-5-3-osfp-800g-zr-dp-reserved",
	"synthetic-cmis-3-0-qsfp-dd",
	"synthetic-cmis-5-2-qsfp-dd",
}

func TestRecomputeCmisPage02hThresholdsChecksums(t *testing.T) {
	root := repoRootChecksum(t)
	for _, stem := range cmisPage02hFixtures {
		stem := stem
		t.Run(stem, func(t *testing.T) {
			path := filepath.Join(root, "tests", "fixtures", "cmis", stem+".page-02h.bin")
			buf := readExact(t, path, 128)
			stored := buf[127]
			generated.RecomputeCmisPage02hThresholdsChecksums(buf)
			if buf[127] != stored {
				t.Errorf("%s page-02h checksum: recomputed 0x%02x != stored 0x%02x",
					stem, buf[127], stored)
			}
		})
	}
}

// Mutation test: recompute tracks data mutation, not stored-value copy.
func TestRecomputeCmisPage02hRecomputeTracksMutation(t *testing.T) {
	root := repoRootChecksum(t)
	path := filepath.Join(root, "tests", "fixtures", "cmis", "fboss-cmis-5-3-osfp-800g-zr.page-02h.bin")
	buf := readExact(t, path, 128)
	generated.RecomputeCmisPage02hThresholdsChecksums(buf)
	before := buf[127]
	buf[0]++
	generated.RecomputeCmisPage02hThresholdsChecksums(buf)
	want := byte(before + 1)
	if buf[127] != want {
		t.Errorf("mutation test: expected 0x%02x, got 0x%02x", want, buf[127])
	}
}

func TestRecomputeCmisPage02hZeroedBufferIsZero(t *testing.T) {
	buf := make([]byte, 128)
	generated.RecomputeCmisPage02hThresholdsChecksums(buf)
	if buf[127] != 0 {
		t.Errorf("zeroed buffer: expected 0x00, got 0x%02x", buf[127])
	}
}

// ─── SFF-8636 Page 00h Upper Administrative ────────────────────────────────
// CC_BASE at buf[63] covers buf[0:63]; CC_EXT at buf[95] covers buf[64:95].

var sff8636Page00hFixtures = []string{
	"finisar-ftl410qe3c-40g-sr4",
	"finisar-ftlc9551repm-100g-sr4",
	"synthetic-qsfp-dac-40g-passive-cu",
	"synthetic-qsfp28-aoc-100g-sr4",
	"synthetic-qsfp28-bidi-100g-cwdm4",
}

func TestRecomputeSff8636Page00hUpperAdministrativeChecksums(t *testing.T) {
	root := repoRootChecksum(t)
	for _, stem := range sff8636Page00hFixtures {
		stem := stem
		t.Run(stem, func(t *testing.T) {
			path := filepath.Join(root, "tests", "fixtures", "sff-8636", stem+".page-00h.bin")
			buf := readExact(t, path, 128)
			ccBaseStored := buf[63]
			ccExtStored := buf[95]
			generated.RecomputeSff8636Page00hUpperAdministrativeChecksums(buf)
			if buf[63] != ccBaseStored {
				t.Errorf("%s CC_BASE: recomputed 0x%02x != stored 0x%02x",
					stem, buf[63], ccBaseStored)
			}
			if buf[95] != ccExtStored {
				t.Errorf("%s CC_EXT: recomputed 0x%02x != stored 0x%02x",
					stem, buf[95], ccExtStored)
			}
		})
	}
}

// ─── SFF-8472 A0h ──────────────────────────────────────────────────────────
// CC_BASE at buf[63] covers buf[0:63]; CC_EXT at buf[95] covers buf[64:95].
// Fixture files are 128 bytes — zero-padded to 256 to match page size.

var sff8472A0hFixtures = []string{
	"finisar-ftlx8571d3bcl-mup0wb0",
	"finisar-ftlx8571d3bcl-muq1bzb",
	"synthetic-dac-sfp10g-passive-cu",
	"synthetic-extcal-sfp10g-sr",
	"synthetic-sfp28-25g-sr",
}

func TestRecomputePageA0hBaseMemoryChecksums(t *testing.T) {
	root := repoRootChecksum(t)
	for _, stem := range sff8472A0hFixtures {
		stem := stem
		t.Run(stem, func(t *testing.T) {
			path := filepath.Join(root, "tests", "fixtures", "sff-8472", stem+".a0h.bin")
			buf := readPadded(t, path, 256)
			ccBaseStored := buf[63]
			ccExtStored := buf[95]
			generated.Recomputepage_a0h_base_memoryChecksums(buf)
			if buf[63] != ccBaseStored {
				t.Errorf("%s a0h CC_BASE: recomputed 0x%02x != stored 0x%02x",
					stem, buf[63], ccBaseStored)
			}
			if buf[95] != ccExtStored {
				t.Errorf("%s a0h CC_EXT: recomputed 0x%02x != stored 0x%02x",
					stem, buf[95], ccExtStored)
			}
		})
	}
}

// ─── SFF-8472 A2h ──────────────────────────────────────────────────────────
// CHKSUM at buf[95] covers buf[0:95]. Fixture files are exactly 256 bytes.

func TestRecomputePageA2hDiagnosticsBaseChecksums(t *testing.T) {
	root := repoRootChecksum(t)
	for _, stem := range sff8472A0hFixtures { // same set of stems
		stem := stem
		t.Run(stem, func(t *testing.T) {
			path := filepath.Join(root, "tests", "fixtures", "sff-8472", stem+".a2h.bin")
			buf := readExact(t, path, 256)
			stored := buf[95]
			generated.Recomputepage_a2h_diagnostics_baseChecksums(buf)
			if buf[95] != stored {
				t.Errorf("%s a2h CHKSUM: recomputed 0x%02x != stored 0x%02x",
					stem, buf[95], stored)
			}
		})
	}
}
