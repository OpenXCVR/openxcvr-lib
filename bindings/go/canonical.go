package openxcvr

import "fmt"

// Canonical page maps use the wire tags shared with the Python facade
// ("lower", "00h", "a0h", …). A missing key is an absent page. A present
// slice, even if empty or nil, is supplied and is not dropped. An unknown
// key fails closed. These adapters do not zero-fill a missing required page;
// the typed decoders' historical pad of a nil lower/A0h/A2h buffer is not
// reachable through a missing key.

func rejectUnknownKeys(family string, pages map[string][]byte, allowed map[string]struct{}) error {
	if pages == nil {
		return fmt.Errorf("%s pages: map is nil", family)
	}
	for key := range pages {
		if _, ok := allowed[key]; !ok {
			return fmt.Errorf("%s pages: unknown key %q", family, key)
		}
	}
	return nil
}

func requireKey(family string, pages map[string][]byte, key string) ([]byte, error) {
	buf, ok := pages[key]
	if !ok {
		return nil, fmt.Errorf("%s pages: missing required key %q", family, key)
	}
	return buf, nil
}

// suppliedBytes preserves presence: a present key holding a nil slice is
// still a present page, so it maps to a non-nil empty slice the typed
// decoders can distinguish from an absent (nil) optional page.
func suppliedBytes(buf []byte) []byte {
	if buf == nil {
		return []byte{}
	}
	return buf
}

// CmisPagesFromCanonical maps canonical CMIS keys onto Pages.
// Required keys are "lower" and "00h". Optional keys are "01h", "11h", and
// the VDM set "20h"-"2fh" excluding "2eh".
func CmisPagesFromCanonical(pages map[string][]byte) (Pages, error) {
	allowed := map[string]struct{}{
		"lower": {}, "00h": {}, "01h": {}, "11h": {},
		"20h": {}, "21h": {}, "22h": {}, "23h": {},
		"24h": {}, "25h": {}, "26h": {}, "27h": {},
		"28h": {}, "29h": {}, "2ah": {}, "2bh": {},
		"2ch": {}, "2dh": {}, "2fh": {},
	}
	if err := rejectUnknownKeys("cmis", pages, allowed); err != nil {
		return Pages{}, err
	}
	lower, err := requireKey("cmis", pages, "lower")
	if err != nil {
		return Pages{}, err
	}
	page00h, err := requireKey("cmis", pages, "00h")
	if err != nil {
		return Pages{}, err
	}
	out := Pages{Lower: lower, Page00h: page00h}
	if buf, ok := pages["01h"]; ok {
		out.Page01h = suppliedBytes(buf)
	}
	if buf, ok := pages["11h"]; ok {
		out.Page11h = suppliedBytes(buf)
	}
	optional := []struct {
		key string
		dst *[]byte
	}{
		{"20h", &out.Page20h}, {"21h", &out.Page21h}, {"22h", &out.Page22h},
		{"23h", &out.Page23h}, {"24h", &out.Page24h}, {"25h", &out.Page25h},
		{"26h", &out.Page26h}, {"27h", &out.Page27h}, {"28h", &out.Page28h},
		{"29h", &out.Page29h}, {"2ah", &out.Page2Ah}, {"2bh", &out.Page2Bh},
		{"2ch", &out.Page2Ch}, {"2dh", &out.Page2Dh}, {"2fh", &out.Page2Fh},
	}
	for _, item := range optional {
		if buf, ok := pages[item.key]; ok {
			*item.dst = suppliedBytes(buf)
		}
	}
	return out, nil
}

// Sff8636PagesFromCanonical maps canonical SFF-8636 keys onto Sff8636Pages.
// Required keys are "lower" and "00h". "03h" is optional and stays nil when
// omitted so EvaluateSff8636Thresholds can refuse it instead of seeing a
// fabricated zero page.
func Sff8636PagesFromCanonical(pages map[string][]byte) (Sff8636Pages, error) {
	allowed := map[string]struct{}{"lower": {}, "00h": {}, "03h": {}}
	if err := rejectUnknownKeys("sff-8636", pages, allowed); err != nil {
		return Sff8636Pages{}, err
	}
	lower, err := requireKey("sff-8636", pages, "lower")
	if err != nil {
		return Sff8636Pages{}, err
	}
	page00h, err := requireKey("sff-8636", pages, "00h")
	if err != nil {
		return Sff8636Pages{}, err
	}
	out := Sff8636Pages{Lower: lower, Page00h: page00h}
	if buf, ok := pages["03h"]; ok {
		out.Page03h = suppliedBytes(buf)
	}
	return out, nil
}

// Sff8472PagesFromCanonical maps canonical SFF-8472 keys onto Sff8472Pages.
// Required keys are "a0h" and "a2h".
func Sff8472PagesFromCanonical(pages map[string][]byte) (Sff8472Pages, error) {
	allowed := map[string]struct{}{"a0h": {}, "a2h": {}}
	if err := rejectUnknownKeys("sff-8472", pages, allowed); err != nil {
		return Sff8472Pages{}, err
	}
	a0h, err := requireKey("sff-8472", pages, "a0h")
	if err != nil {
		return Sff8472Pages{}, err
	}
	a2h, err := requireKey("sff-8472", pages, "a2h")
	if err != nil {
		return Sff8472Pages{}, err
	}
	return Sff8472Pages{A0h: a0h, A2h: a2h}, nil
}
