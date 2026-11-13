package openxcvr

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

var cmisSkipKeys = map[string]bool{"_metadata": true, "module_type": true}
var legacySkipKeys = map[string]bool{"_metadata": true}

// vdmFixtureTags names the CMIS VDM page set (§ 8.27) a fixture may carry,
// matching the .page-XXh.bin fixture slice naming. 2Eh is not part of the
// VDM feature and has no slice.
var vdmFixtureTags = []string{
	"20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
	"28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
}

// assignVdmPage stores one loaded VDM fixture slice into the decode Pages.
func assignVdmPage(pages *Pages, tag string, data []byte) {
	switch tag {
	case "20h":
		pages.Page20h = data
	case "21h":
		pages.Page21h = data
	case "22h":
		pages.Page22h = data
	case "23h":
		pages.Page23h = data
	case "24h":
		pages.Page24h = data
	case "25h":
		pages.Page25h = data
	case "26h":
		pages.Page26h = data
	case "27h":
		pages.Page27h = data
	case "28h":
		pages.Page28h = data
	case "29h":
		pages.Page29h = data
	case "2ah":
		pages.Page2Ah = data
	case "2bh":
		pages.Page2Bh = data
	case "2ch":
		pages.Page2Ch = data
	case "2dh":
		pages.Page2Dh = data
	case "2fh":
		pages.Page2Fh = data
	}
}

// diff recursively compares result and golden maps, skipping selected keys.
func diff(result, golden interface{}, path string, skipKeys map[string]bool) []string {
	var diffs []string

	switch g := golden.(type) {
	case map[string]interface{}:
		r, ok := result.(map[string]interface{})
		if !ok {
			return []string{fmt.Sprintf("%s: type mismatch (result=%T, golden=map)", path, result)}
		}
		allKeys := make(map[string]bool)
		for k := range r {
			allKeys[k] = true
		}
		for k := range g {
			allKeys[k] = true
		}
		keys := make([]string, 0, len(allKeys))
		for k := range allKeys {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			if skipKeys[k] {
				continue
			}
			child := k
			if path != "" {
				child = path + "." + k
			}
			rv, inResult := r[k]
			gv, inGolden := g[k]
			if !inResult {
				diffs = append(diffs, fmt.Sprintf("%s: missing from result (golden=%v)", child, gv))
			} else if !inGolden {
				diffs = append(diffs, fmt.Sprintf("%s: extra in result (result=%v)", child, rv))
			} else {
				diffs = append(diffs, diff(rv, gv, child, skipKeys)...)
			}
		}
	case []interface{}:
		r, ok := result.([]interface{})
		if !ok {
			return []string{fmt.Sprintf("%s: type mismatch (result=%T, golden=[])", path, result)}
		}
		if len(r) != len(g) {
			diffs = append(diffs, fmt.Sprintf("%s: list length %d != %d", path, len(r), len(g)))
		}
		for i := 0; i < len(r) && i < len(g); i++ {
			diffs = append(diffs, diff(r[i], g[i], fmt.Sprintf("%s[%d]", path, i), skipKeys)...)
		}
	default:
		if fmt.Sprintf("%v", result) != fmt.Sprintf("%v", golden) {
			diffs = append(diffs, fmt.Sprintf("%s: %#v != %#v", path, result, golden))
		}
	}
	return diffs
}

func TestParity(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	runCmisParity(t, repoRoot)
	runSff8636Parity(t, repoRoot)
	runSff8472Parity(t, repoRoot)
}

func runCmisParity(t *testing.T, repoRoot string) {
	t.Helper()
	goldenDir := filepath.Join(repoRoot, "tests", "golden", "cmis")
	fixtureDir := filepath.Join(repoRoot, "tests", "fixtures", "cmis")

	for _, e := range readJSONEntries(t, goldenDir) {
		stem := strings.TrimSuffix(e.Name(), ".json")
		t.Run("cmis/"+stem, func(t *testing.T) {
			lowerPath := filepath.Join(fixtureDir, stem+".lower.bin")
			if _, err := os.Stat(lowerPath); err != nil {
				t.Skipf("no fixture for %s", stem)
				return
			}

			lower := readFile(t, lowerPath)
			page00hPath := filepath.Join(fixtureDir, stem+".page-00h.bin")
			page00h := make([]byte, 128)
			if data, err := os.ReadFile(page00hPath); err == nil {
				page00h = data
			}

			var page01h []byte
			if data, err := os.ReadFile(filepath.Join(fixtureDir, stem+".page-01h.bin")); err == nil {
				page01h = data
			}

			var page11h []byte
			if data, err := os.ReadFile(filepath.Join(fixtureDir, stem+".page-11h.bin")); err == nil {
				page11h = data
			}

			// VDM pages (CMIS § 8.27): loaded when the fixture carries them.
			// Page 2Fh gates the `vdm` block in DecodeCmis.
			decodePages := Pages{Lower: lower, Page00h: page00h, Page01h: page01h, Page11h: page11h}
			for _, tag := range vdmFixtureTags {
				path := filepath.Join(fixtureDir, stem+".page-"+tag+".bin")
				data, err := os.ReadFile(path)
				if err != nil {
					continue
				}
				assignVdmPage(&decodePages, tag, data)
			}

			result, err := DecodeCmis(decodePages)
			if err != nil {
				t.Fatalf("DecodeCmis error: %v", err)
			}
			assertGoldenParity(t, stem, result, readGolden(t, filepath.Join(goldenDir, e.Name())), cmisSkipKeys)
		})
	}
}

func runSff8636Parity(t *testing.T, repoRoot string) {
	t.Helper()
	goldenDir := filepath.Join(repoRoot, "tests", "golden", "sff-8636")
	fixtureDir := filepath.Join(repoRoot, "tests", "fixtures", "sff-8636")

	for _, e := range readJSONEntries(t, goldenDir) {
		stem := strings.TrimSuffix(e.Name(), ".json")
		t.Run("sff-8636/"+stem, func(t *testing.T) {
			result, err := DecodeSff8636(Sff8636Pages{
				Lower:   readFile(t, filepath.Join(fixtureDir, stem+".lower.bin")),
				Page00h: readFile(t, filepath.Join(fixtureDir, stem+".page-00h.bin")),
			})
			if err != nil {
				t.Fatalf("DecodeSff8636 error: %v", err)
			}
			assertGoldenParity(t, stem, result, readGolden(t, filepath.Join(goldenDir, e.Name())), legacySkipKeys)
		})
	}
}

func runSff8472Parity(t *testing.T, repoRoot string) {
	t.Helper()
	goldenDir := filepath.Join(repoRoot, "tests", "golden", "sff-8472")
	fixtureDir := filepath.Join(repoRoot, "tests", "fixtures", "sff-8472")

	for _, e := range readJSONEntries(t, goldenDir) {
		stem := strings.TrimSuffix(e.Name(), ".json")
		t.Run("sff-8472/"+stem, func(t *testing.T) {
			result, err := DecodeSff8472(Sff8472Pages{
				A0h: readFile(t, filepath.Join(fixtureDir, stem+".a0h.bin")),
				A2h: readFile(t, filepath.Join(fixtureDir, stem+".a2h.bin")),
			})
			if err != nil {
				t.Fatalf("DecodeSff8472 error: %v", err)
			}
			assertGoldenParity(t, stem, result, readGolden(t, filepath.Join(goldenDir, e.Name())), legacySkipKeys)
		})
	}
}

func readJSONEntries(t *testing.T, dir string) []os.DirEntry {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("cannot read golden dir %s: %v", dir, err)
	}
	var filtered []os.DirEntry
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".json") && !strings.HasPrefix(e.Name(), "_") {
			filtered = append(filtered, e)
		}
	}
	if len(filtered) == 0 {
		t.Fatalf("no golden JSON files in %s", dir)
	}
	return filtered
}

func readFile(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("cannot read %s: %v", path, err)
	}
	return data
}

func readGolden(t *testing.T, path string) map[string]interface{} {
	t.Helper()
	goldenBytes := readFile(t, path)
	var goldenMap map[string]interface{}
	if err := json.Unmarshal(goldenBytes, &goldenMap); err != nil {
		t.Fatalf("cannot parse golden JSON: %v", err)
	}
	return goldenMap
}

// TestCmisLaneStatusLatches pins the per-lane status latches (rx_los ←
// LOSFlagRx, Table 8-88; tx_fault ← FailureFlagTx, Table 8-87) on the one
// fixture whose Page 11h carries real latched values, independently of the
// deep golden compare: fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched on
// lanes 1/3 and tx_fault on lanes 2/4, everything else clear.
func TestCmisLaneStatusLatches(t *testing.T) {
	fixtureDir := filepath.Join("..", "..", "tests", "fixtures", "cmis")
	stem := "fboss-cmis-4-0-qsfp112-200g-fr4"
	result, err := DecodeCmis(Pages{
		Lower:   readFile(t, filepath.Join(fixtureDir, stem+".lower.bin")),
		Page00h: readFile(t, filepath.Join(fixtureDir, stem+".page-00h.bin")),
		Page01h: readFile(t, filepath.Join(fixtureDir, stem+".page-01h.bin")),
		Page11h: readFile(t, filepath.Join(fixtureDir, stem+".page-11h.bin")),
	})
	if err != nil {
		t.Fatalf("DecodeCmis error: %v", err)
	}
	lanes, ok := result["lanes"].([]interface{})
	if !ok || len(lanes) != 8 {
		t.Fatalf("expected 8 lanes, got %T", result["lanes"])
	}
	wantLos := map[int]bool{1: true, 3: true}
	wantFault := map[int]bool{2: true, 4: true}
	for i, laneAny := range lanes {
		lane := laneAny.(map[string]interface{})
		idx := i + 1
		if lane["rx_los"] != wantLos[idx] {
			t.Errorf("lane %d rx_los: got %v, want %v", idx, lane["rx_los"], wantLos[idx])
		}
		if lane["tx_fault"] != wantFault[idx] {
			t.Errorf("lane %d tx_fault: got %v, want %v", idx, lane["tx_fault"], wantFault[idx])
		}
	}
}

func assertGoldenParity(t *testing.T, stem string, result map[string]interface{}, goldenMap map[string]interface{}, skips map[string]bool) {
	t.Helper()
	resultJSON, _ := json.Marshal(result)
	var resultNorm map[string]interface{}
	json.Unmarshal(resultJSON, &resultNorm)

	diffs := diff(resultNorm, goldenMap, "", skips)
	if len(diffs) > 0 {
		t.Errorf("Parity failures for %s:\n  %s", stem, strings.Join(diffs, "\n  "))
	}
}
