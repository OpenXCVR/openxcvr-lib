package openxcvr

import (
	"fmt"

	genums "github.com/OpenXCVR/openxcvr-lib/bindings/go/generated/enums"
)

// ─── SFF-8024 lookup tables ───────────────────────────────────────────────────

var sff8636ExtendedSpec = map[int]string{
	0x02: "base_100g_sr4_or_25g_sr",
}

// ─── lookup helpers ───────────────────────────────────────────────────────────

func lookupIdentifier(id int) string {
	return genums.Sff8024Identifier(id).Label()
}

func lookupConnector(id int) string {
	return genums.Sff8024ConnectorType(id).Label()
}

func lookupEncoding(id int) string {
	return genums.Sff8024Encoding(id).Label()
}

func lookupHostInterface(id int) string {
	return genums.Sff8024HostElectricalInterface(id).Label()
}

func lookupDeviceTechnology(id int) string {
	return genums.Sff8636DeviceTechnology(id).Label()
}

func checksum(buf []byte, start, end, storedOffset int) map[string]interface{} {
	computed := 0
	for _, b := range buf[start:end] {
		computed = (computed + int(b)) & 0xff
	}
	stored := int(buf[storedOffset])
	return map[string]interface{}{
		"computed": fmt.Sprintf("0x%02X", computed),
		"stored":   fmt.Sprintf("0x%02X", stored),
		"valid":    computed == stored,
	}
}

func hexBytes(buf []byte) []interface{} {
	values := make([]interface{}, len(buf))
	for i, b := range buf {
		values[i] = fmt.Sprintf("0x%02X", b)
	}
	return values
}

func oui(buf []byte) string {
	return fmt.Sprintf("%02X:%02X:%02X", buf[0], buf[1], buf[2])
}

func padPage(buf []byte, size int) []byte {
	if len(buf) >= size {
		return buf
	}
	padded := make([]byte, size)
	copy(padded, buf)
	return padded
}

// ─── app descriptor parsing ───────────────────────────────────────────────────

type appEntry struct {
	AppCode                   int    `json:"app_code"`
	HostInterfaceID           int    `json:"host_interface_id"`
	HostInterfaceName         string `json:"host_interface_name"`
	HostLaneCount             int    `json:"host_lane_count"`
	HostLaneAssignmentOptions string `json:"host_lane_assignment_options"`
	MediaInterfaceID          int    `json:"media_interface_id"`
	MediaInterfaceName        string `json:"media_interface_name"`
	MediaLaneCount            int    `json:"media_lane_count"`
}

// parseAppDescriptors reads 4-byte app records starting at buf[startOffset].
// count is the maximum number of records to read. Stops at HIF=0xFF.
// Slots with MIF=0 are skipped but their position is preserved for code
// numbering (same semantics as Python: filter mif!=0 after parsing all slots).
// baseCode is added to the 1-based slot index to form the app_code.
func parseAppDescriptors(buf []byte, startOffset, count, baseCode int, mediaType byte) []appEntry {
	var entries []appEntry
	for i := 0; i < count; i++ {
		off := startOffset + i*4
		if off+4 > len(buf) {
			break
		}
		hif := int(buf[off])
		mif := int(buf[off+1])
		hlc := int(buf[off+2] >> 4)
		mlc := int(buf[off+2] & 0xF)
		hlao := int(buf[off+3])
		if hif == 0xFF {
			break
		}
		if mif == 0 {
			// skip but preserve slot position for code numbering
			continue
		}
		code := baseCode + i + 1
		mediaName, ok := genums.CmisMediaInterfaceName(mediaType, byte(mif))
		if !ok {
			mediaName = "unknown"
		}
		entries = append(entries, appEntry{
			AppCode:                   code,
			HostInterfaceID:           hif,
			HostInterfaceName:         lookupHostInterface(hif),
			HostLaneCount:             hlc,
			HostLaneAssignmentOptions: fmt.Sprintf("0x%02X", hlao),
			MediaInterfaceID:          mif,
			MediaInterfaceName:        mediaName,
			MediaLaneCount:            mlc,
		})
	}
	return entries
}
