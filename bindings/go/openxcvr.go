// Package openxcvr decodes CMIS, SFF-8636, and SFF-8472 transceiver EEPROM pages.
package openxcvr

// Pages holds the raw per-page byte slices for a CMIS module.
type Pages struct {
	Lower   []byte
	Page00h []byte
	Page01h []byte // optional; nil if module does not expose page 01h
	Page11h []byte // optional; nil if module does not expose page 11h (flat memory)

	// VDM pages (CMIS § 8.27), all optional. The `vdm` block is emitted
	// only when the COMPLETE VDM page set (all fields below) is supplied;
	// within the block the Page 2Fh advertisement gates which groups are
	// decoded. Descriptor pages 20h-23h and sample pages 24h-27h are
	// per-group; thresholds 28h-2Bh, TC flags 2Ch, TC masks 2Dh are
	// module-wide.
	Page20h []byte
	Page21h []byte
	Page22h []byte
	Page23h []byte
	Page24h []byte
	Page25h []byte
	Page26h []byte
	Page27h []byte
	Page28h []byte
	Page29h []byte
	Page2Ah []byte
	Page2Bh []byte
	Page2Ch []byte
	Page2Dh []byte
	Page2Fh []byte
}

// Sff8636Pages holds the raw lower page and upper Page 00h bytes needed for the
// current SFF-8636 parity surface.
type Sff8636Pages struct {
	Lower   []byte
	Page00h []byte
	// Page03h carries the Upper Page 03h threshold/controls window for
	// EvaluateSff8636Thresholds (128 bytes; optional for DecodeSff8636,
	// which never reads it). The caller reads it after its own page
	// select — the facade never fabricates it.
	Page03h []byte
}

// Sff8472Pages holds the raw A0h and A2h address-space bytes for SFF-8472.
type Sff8472Pages struct {
	A0h []byte
	A2h []byte
}
