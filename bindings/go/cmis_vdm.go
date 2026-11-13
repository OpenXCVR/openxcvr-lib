package openxcvr

import (
	"fmt"
	"math"

	"github.com/OpenXCVR/openxcvr-lib/bindings/go/generated"
	genums "github.com/OpenXCVR/openxcvr-lib/bindings/go/generated/enums"
)

// VDM (Vendor Defined Monitoring) facade over Pages 20h-2Fh (CMIS § 8.27 /
// pre-5.4 § 8.22), mirroring the Rust reference implementation
// (crates/openxcvr-cmis/src/ops/vdm.rs):
//
//   - Group gating: the number of VDM groups a module implements is
//     advertised by Page 2Fh byte 128 bits 1-0 (`vdm_support`,
//     CmisVdmSupport: 0 = group 1 only .. 3 = groups 1-4). Only descriptor
//     (20h+group) and sample (24h+group) pages of advertised groups are
//     decoded; pages of groups beyond the advertisement are never read.
//   - Polymorphic sample decode: each instance's X16 sample register is
//     decoded by the storage type selected by its descriptor's observable
// type:
//     S16 for TEC current / laser frequency error / laser temperature /
//     ELS input power, F16 (m * 10^(s-24)) for the Pre-FEC BER and FERC
//     observables, U16 for everything else.
//
// The facade is pure: it decodes caller-supplied page bytes and returns the
// `vdm` result block; it never performs I/O. A missing or short page of the
// advertised set is an error — the block is never emitted from partial or
// zero-filled pages (fail closed).

// vdmPageSet is the complete VDM page set (CMIS § 8.27): descriptor pages
// 20h-23h, sample pages 24h-27h, threshold quad pages 28h-2Bh, TC flag page
// 2Ch, TC mask page 2Dh and the advertisement page 2Fh.
var vdmPageSet = []int{0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2f}

// vdmPagesComplete reports whether the caller supplied every page of the
// VDM page set. The `vdm` block is decoded as a whole from a whole page
// set; a partial set yields no block rather than one fabricated from
// zero-filled missing pages.
func vdmPagesComplete(pages Pages) bool {
	for _, page := range vdmPageSet {
		if vdmPageBytes(pages, page) == nil {
			return false
		}
	}
	return true
}

// decodeVdm builds the `vdm` block. Called only when the complete VDM page
// set is supplied; short pages still fail closed here.
func decodeVdm(pages Pages) (map[string]interface{}, error) {
	if len(pages.Page2Fh) < 128 {
		return nil, fmt.Errorf("VDM page 2Fh missing or short: %d bytes", len(pages.Page2Fh))
	}
	ad, err := generated.DecodeCmisPage2FhVdmAdvertisement(pages.Page2Fh)
	if err != nil {
		return nil, fmt.Errorf("page2Fh: %w", err)
	}

	// CmisVdmSupport code = advertised group count - 1.
	groups := ad.VdmSupport + 1

	// ─── observables (Pages 20h-23h descriptors + 24h-27h samples) ──────────
	var observables []interface{}
	for group := 0; group < groups; group++ {
		descPage := 0x20 + group
		samplePage := 0x24 + group
		descBuf := vdmPageBytes(pages, descPage)
		if len(descBuf) < 128 {
			return nil, fmt.Errorf("VDM descriptor page %02Xh not advertised-set-complete: %d bytes", descPage, len(descBuf))
		}
		sampleBuf := vdmPageBytes(pages, samplePage)
		if len(sampleBuf) < 128 {
			return nil, fmt.Errorf("VDM sample page %02Xh not advertised-set-complete: %d bytes", samplePage, len(sampleBuf))
		}
		tsid, res, otype, err := vdmDescriptors(descPage, descBuf)
		if err != nil {
			return nil, err
		}
		samples, err := vdmSamples(samplePage, sampleBuf)
		if err != nil {
			return nil, err
		}
		for local := 0; local < 64; local++ {
			// An unused descriptor slot is 00h in both bytes (CMIS Table
			// 8-178): observable type 0 means the instance is not active
			// and its sample is ignored.
			if otype[local] == 0 {
				continue
			}
			raw := samples[local]
			storage, value := decodeVdmSample(otype[local], raw)
			observables = append(observables, map[string]interface{}{
				"instance":                group*64 + local + 1,
				"group":                   group + 1,
				"threshold_set_id":        tsid[local],
				"global_threshold_set_id": group*16 + tsid[local] + 1,
				"monitored_resource":      res[local],
				"observable_type":         otype[local],
				"active":                  true,
				"sample_raw":              fmt.Sprintf("0x%04X", raw),
				"sample": map[string]interface{}{
					"storage": storage,
					"value":   value,
				},
			})
		}
	}

	// ─── threshold quads (Pages 28h-2Bh, global sets 1-64) ──────────────────
	var thresholdQuads []interface{}
	for i := 0; i < 4; i++ {
		page := 0x28 + i
		buf := vdmPageBytes(pages, page)
		if len(buf) < 128 {
			return nil, fmt.Errorf("VDM threshold page %02Xh missing or short: %d bytes", page, len(buf))
		}
		hiA, loA, hiW, loW, err := vdmThresholds(page, buf)
		if err != nil {
			return nil, err
		}
		for set := 0; set < 16; set++ {
			// An all-zero quad is an undefined threshold set: emitted only
			// when at least one carrier is non-zero, matching the golden's
			// honest decode of zeroed supervision pages.
			if hiA[set] == 0 && loA[set] == 0 && hiW[set] == 0 && loW[set] == 0 {
				continue
			}
			thresholdQuads = append(thresholdQuads, map[string]interface{}{
				"page":         fmt.Sprintf("%02Xh", page),
				"set":          set + 1,
				"high_alarm":   fmt.Sprintf("0x%04X", hiA[set]),
				"low_alarm":    fmt.Sprintf("0x%04X", loA[set]),
				"high_warning": fmt.Sprintf("0x%04X", hiW[set]),
				"low_warning":  fmt.Sprintf("0x%04X", loW[set]),
			})
		}
	}

	// ─── TC flags and masks (Pages 2Ch / 2Dh, 256 instances) ────────────────
	if len(pages.Page2Ch) < 128 {
		return nil, fmt.Errorf("VDM page 2Ch missing or short: %d bytes", len(pages.Page2Ch))
	}
	if len(pages.Page2Dh) < 128 {
		return nil, fmt.Errorf("VDM page 2Dh missing or short: %d bytes", len(pages.Page2Dh))
	}
	flags2ch, err := generated.DecodeCmisPage2ChVdmFlags(pages.Page2Ch)
	if err != nil {
		return nil, fmt.Errorf("page2Ch: %w", err)
	}
	masks2dh, err := generated.DecodeCmisPage2DhVdmMasks(pages.Page2Dh)
	if err != nil {
		return nil, fmt.Errorf("page2Dh: %w", err)
	}

	var tcFlags []interface{}
	var tcMasks []interface{}
	for i := 0; i < 256; i++ {
		ha, la := flags2ch.HighAlarmTcFlag[i], flags2ch.LowAlarmTcFlag[i]
		hw, lw := flags2ch.HighWarningTcFlag[i], flags2ch.LowWarningTcFlag[i]
		if ha || la || hw || lw {
			tcFlags = append(tcFlags, map[string]interface{}{
				"instance":     i + 1,
				"high_alarm":   ha,
				"low_alarm":    la,
				"high_warning": hw,
				"low_warning":  lw,
			})
		}
		mha, mla := masks2dh.HighAlarmTcMask[i], masks2dh.LowAlarmTcMask[i]
		mhw, mlw := masks2dh.HighWarningTcMask[i], masks2dh.LowWarningTcMask[i]
		if mha || mla || mhw || mlw {
			tcMasks = append(tcMasks, map[string]interface{}{
				"instance":     i + 1,
				"high_alarm":   mha,
				"low_alarm":    mla,
				"high_warning": mhw,
				"low_warning":  mlw,
			})
		}
	}

	// ─── support block (Page 2Fh advertisement and dynamic controls) ────────
	support := map[string]interface{}{
		"vdm_support":                ad.VdmSupport,
		"vdm_support_name":           genums.CmisVdmSupport(ad.VdmSupport).Label(),
		"groups_advertised":          groups,
		"duty_cycle_support":         ad.DutyCycleSupport,
		"power_saving_support":       ad.PowerSavingSupport,
		"fine_interval_length_0p1ms": ad.FineIntervalLength,
		"freeze_request":             ad.FreezeRequest,
		"power_saving_mode":          ad.PowerSavingMode,
		"monitoring_duty_cycle_k":    ad.MonitoringDutyCycle,
		"freeze_done":                ad.FreezeDone,
		"unfreeze_done":              ad.UnfreezeDone,
	}

	return map[string]interface{}{
		"support":         support,
		"observables":     observables,
		"threshold_quads": thresholdQuads,
		"tc_flags":        tcFlags,
		"tc_masks":        tcMasks,
	}, nil
}

// decodeVdmSample decodes one X16 sample register per the descriptor's
// observable type:
//
//   - 0x02-0x04 TEC current / laser frequency error / laser temperature and
//     0x54 ELS input power: S16;
//   - 0x09-0x1a Pre-FEC BER and FERC families: F16, m * 10^(s-24);
//   - everything else (0x00 unused, 0x01 laser age, 0x05-0x08 SNR/LTP,
//     0x1b-0x22 SEWmax, reserved and custom ranges): raw U16.
func decodeVdmSample(observableType, raw int) (string, interface{}) {
	switch {
	case (observableType >= 0x02 && observableType <= 0x04) || observableType == 0x54:
		return "i16", int(int16(raw))
	case observableType >= 0x09 && observableType <= 0x1a:
		m := raw & 0x7FF
		s := (raw >> 11) & 0x1F
		return "f16", float64(m) * math.Pow10(s-24)
	default:
		return "u16", raw
	}
}

// vdmPageBytes returns the caller-supplied slice for a VDM page address
// (20h-2Fh); nil when the page was not supplied.
func vdmPageBytes(pages Pages, page int) []byte {
	switch page {
	case 0x20:
		return pages.Page20h
	case 0x21:
		return pages.Page21h
	case 0x22:
		return pages.Page22h
	case 0x23:
		return pages.Page23h
	case 0x24:
		return pages.Page24h
	case 0x25:
		return pages.Page25h
	case 0x26:
		return pages.Page26h
	case 0x27:
		return pages.Page27h
	case 0x28:
		return pages.Page28h
	case 0x29:
		return pages.Page29h
	case 0x2a:
		return pages.Page2Ah
	case 0x2b:
		return pages.Page2Bh
	case 0x2c:
		return pages.Page2Ch
	case 0x2d:
		return pages.Page2Dh
	case 0x2f:
		return pages.Page2Fh
	default:
		return nil
	}
}

// vdmDescriptors decodes one group's descriptor page (20h+group) into the
// per-instance threshold-set-id, monitored-resource and observable-type
// slices, via the generated per-page decoders.
func vdmDescriptors(page int, buf []byte) ([]int, []int, []int, error) {
	switch page {
	case 0x20:
		d, err := generated.DecodeCmisPage20hVdmDescriptors1To64(buf)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("page20h: %w", err)
		}
		return d.LocalThresholdSetId, d.MonitoredResource, d.ObservableType, nil
	case 0x21:
		d, err := generated.DecodeCmisPage21hVdmDescriptors65To128(buf)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("page21h: %w", err)
		}
		return d.LocalThresholdSetId, d.MonitoredResource, d.ObservableType, nil
	case 0x22:
		d, err := generated.DecodeCmisPage22hVdmDescriptors129To192(buf)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("page22h: %w", err)
		}
		return d.LocalThresholdSetId, d.MonitoredResource, d.ObservableType, nil
	case 0x23:
		d, err := generated.DecodeCmisPage23hVdmDescriptors193To256(buf)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("page23h: %w", err)
		}
		return d.LocalThresholdSetId, d.MonitoredResource, d.ObservableType, nil
	default:
		return nil, nil, nil, fmt.Errorf("not a VDM descriptor page: %02Xh", page)
	}
}

// vdmSamples decodes one group's sample page (24h+group) into the
// per-instance raw X16 registers, via the generated per-page decoders.
func vdmSamples(page int, buf []byte) ([]int, error) {
	switch page {
	case 0x24:
		d, err := generated.DecodeCmisPage24hVdmSamples1To64(buf)
		if err != nil {
			return nil, fmt.Errorf("page24h: %w", err)
		}
		return d.VdmSample, nil
	case 0x25:
		d, err := generated.DecodeCmisPage25hVdmSamples65To128(buf)
		if err != nil {
			return nil, fmt.Errorf("page25h: %w", err)
		}
		return d.VdmSample, nil
	case 0x26:
		d, err := generated.DecodeCmisPage26hVdmSamples129To192(buf)
		if err != nil {
			return nil, fmt.Errorf("page26h: %w", err)
		}
		return d.VdmSample, nil
	case 0x27:
		d, err := generated.DecodeCmisPage27hVdmSamples193To256(buf)
		if err != nil {
			return nil, fmt.Errorf("page27h: %w", err)
		}
		return d.VdmSample, nil
	default:
		return nil, fmt.Errorf("not a VDM sample page: %02Xh", page)
	}
}

// vdmThresholds decodes one supervision-threshold page (28h-2Bh) into the
// per-set high-alarm / low-alarm / high-warning / low-warning X16 carriers.
func vdmThresholds(page int, buf []byte) ([]int, []int, []int, []int, error) {
	switch page {
	case 0x28:
		d, err := generated.DecodeCmisPage28hVdmThresholds1To16(buf)
		if err != nil {
			return nil, nil, nil, nil, fmt.Errorf("page28h: %w", err)
		}
		return d.HighAlarmThreshold, d.LowAlarmThreshold, d.HighWarningThreshold, d.LowWarningThreshold, nil
	case 0x29:
		d, err := generated.DecodeCmisPage29hVdmThresholds17To32(buf)
		if err != nil {
			return nil, nil, nil, nil, fmt.Errorf("page29h: %w", err)
		}
		return d.HighAlarmThreshold, d.LowAlarmThreshold, d.HighWarningThreshold, d.LowWarningThreshold, nil
	case 0x2a:
		d, err := generated.DecodeCmisPage2AhVdmThresholds33To48(buf)
		if err != nil {
			return nil, nil, nil, nil, fmt.Errorf("page2Ah: %w", err)
		}
		return d.HighAlarmThreshold, d.LowAlarmThreshold, d.HighWarningThreshold, d.LowWarningThreshold, nil
	case 0x2b:
		d, err := generated.DecodeCmisPage2BhVdmThresholds49To64(buf)
		if err != nil {
			return nil, nil, nil, nil, fmt.Errorf("page2Bh: %w", err)
		}
		return d.HighAlarmThreshold, d.LowAlarmThreshold, d.HighWarningThreshold, d.LowWarningThreshold, nil
	default:
		return nil, nil, nil, nil, fmt.Errorf("not a VDM threshold page: %02Xh", page)
	}
}
