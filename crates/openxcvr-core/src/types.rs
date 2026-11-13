pub mod management;

use serde::{Deserialize, Serialize};

/// Vendor identity block, decoded from the spec's identity byte ranges
/// (SFF-8472 A0h:16-95; SFF-8636 Lower:148-220; CMIS Lower:129-191).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VendorInfo {
    /// Vendor name ASCII string.
    pub name: String,
    /// Vendor part number ASCII string.
    pub part_number: String,
    /// Vendor hardware revision ASCII string (SFF-8636 Table 6-16 /
    /// SFF-8472 Table 8-2) — never the spec compliance revision.
    pub rev: String,
    /// Serial number ASCII string.
    pub serial_number: String,
    /// Date code ASCII string (per the spec's date-code format).
    pub date_code: String,
    /// IEEE organizationally unique identifier bytes.
    pub oui: [u8; 3],
    /// SFF-8024 module identifier (byte 0 of the A0h / lower page), when
    /// the family layer decoded it from the supplied bytes. Additive:
    /// `None` when absent, so existing serialized output is unchanged and
    /// existing callers (who enumerate the layout's vendor ranges field
    /// by field) keep decoding exactly as before. The family layer fills
    /// this; core does not duplicate the generated enum tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<ModuleIdentifier>,
    /// Specification compliance revision proven from the family's revision
    /// byte (CMIS Lower:1; SFF-8636 Lower:1; SFF-8472 A0h:94), when the
    /// family layer detected it. Additive and skipped when absent: `rev`
    /// above stays the vendor's hardware revision ASCII string
    /// (SFF-8636 Table 6-16 / SFF-8472 Table 8-2) and never conflates the
    /// two. `None` when the module was constructed without detect-once
    /// revision context — it is never guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_rev: Option<crate::rev::Rev>,
}

/// The SFF-8024 module identifier: the raw byte-0 code plus the meaning
/// the family's generated `Sff8024Identifier` enum gives it. `known` is
/// false for codes the schema leaves unassigned — they report their raw
/// code and an `Unknown{n}` name, never a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleIdentifier {
    /// The raw SFF-8024 byte-0 code.
    pub code: u8,
    /// The generated enum's rendering of the code (variant name, for
    /// example `QsfpDd`); `Unknown{code}` for unassigned codes.
    pub name: String,
    /// False when the code is reserved/unassigned.
    pub known: bool,
}

/// One lane's decoded status: optical power, bias current and signal flags.
/// Absent monitors are `None`, never zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaneStatus {
    /// One-based lane index: `1` is the first lane, matching the adapters
    /// and the binding JSON lane records.
    pub index: u8,
    /// Received optical power in milliwatts, when implemented.
    pub rx_power_mw: Option<f32>,
    /// Transmitted optical power in milliwatts, when implemented.
    pub tx_power_mw: Option<f32>,
    /// TX bias current in milliamperes, when implemented.
    pub tx_bias_ma: Option<f32>,
    /// Receive loss-of-signal flag, when implemented.
    pub rx_los: Option<bool>,
    /// TX-disable flag, when implemented.
    pub tx_disable: Option<bool>,
    /// TX-fault flag, when implemented.
    pub tx_fault: Option<bool>,
    /// CMIS Page 11h per-lane Data-Path state, as the `CmisDataPathState`
    /// enum variant name (e.g. `"DpActivated"`; out-of-range nibbles render
    /// `"Unknown<n>"`). `None` for SFF-8472/SFF-8636, which have no
    /// data-path state machine; skipped in serialization so the SFF lane
    /// shape is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dp_state: Option<String>,
    /// TX loss-of-signal flag (SFF-8636 Lower:3 bits 7-4, Table 6-5),
    /// latched per lane. `None` where the family defines no TX LOS
    /// (SFF-8472: RX LOS and TX fault only); additive and skipped in
    /// serialization, so existing serialized lane shapes are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_los: Option<bool>,
    /// TX CDR loss-of-lock flag (SFF-8636 Lower:5 bits 7-4, Table 6-5),
    /// latched per lane; `None` where the family defines no such flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_cdr_lol: Option<bool>,
    /// RX CDR loss-of-lock flag (SFF-8636 Lower:5 bits 3-0, Table 6-5),
    /// latched per lane; `None` where the family defines no such flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_cdr_lol: Option<bool>,
    /// TX adaptive-equalizer fault flag (SFF-8636 Lower:4 bits 7-4 since
    /// rev 2.0, Table 6-5; SFF-8472 A2h:118.4 since rev 12.5, Table 10-1),
    /// latched per lane. `None` at revisions before the bit carried this
    /// meaning — never a borrowed later-revision reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_adaptive_eq_fault: Option<bool>,
}

/// One lane's latched alarm/warning flags. A `false` flag means the bound
/// was not met, not that the monitor is implemented — see the family layer
/// for which monitors exist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaneFlags {
    /// RX power exceeded the high-alarm threshold.
    pub rx_power_high_alarm: bool,
    /// RX power fell below the low-alarm threshold.
    pub rx_power_low_alarm: bool,
    /// TX bias exceeded the high-alarm threshold.
    pub tx_bias_high_alarm: bool,
    /// TX bias fell below the low-alarm threshold.
    pub tx_bias_low_alarm: bool,
    /// TX power exceeded the high-alarm threshold.
    pub tx_power_high_alarm: bool,
    /// TX power fell below the low-alarm threshold.
    pub tx_power_low_alarm: bool,
    /// RX power exceeded the high-warning threshold.
    pub rx_power_high_warning: bool,
    /// RX power fell below the low-warning threshold.
    pub rx_power_low_warning: bool,
    /// TX bias exceeded the high-warning threshold.
    pub tx_bias_high_warning: bool,
    /// TX bias fell below the low-warning threshold.
    pub tx_bias_low_warning: bool,
    /// TX power exceeded the high-warning threshold.
    pub tx_power_high_warning: bool,
    /// TX power fell below the low-warning threshold.
    pub tx_power_low_warning: bool,
}

/// Module-level status and alarm/warning flags, plus the per-lane flag
/// sets. Status flags are `None` for families without the status byte
/// (SFF-8472); a `false` alarm/warning flag means the bound was not met.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleFlags {
    /// Data-not-ready status flag; `None` for SFF-8472.
    pub data_not_ready: Option<bool>,
    /// Interrupt-asserted status flag; `None` for SFF-8472.
    pub interrupt_asserted: Option<bool>,
    /// Case temperature exceeded the high-alarm threshold.
    pub temp_high_alarm: bool,
    /// Case temperature fell below the low-alarm threshold.
    pub temp_low_alarm: bool,
    /// Supply voltage exceeded the high-alarm threshold.
    pub supply_high_alarm: bool,
    /// Supply voltage fell below the low-alarm threshold.
    pub supply_low_alarm: bool,
    /// Per-lane alarm/warning flag sets, positionally aligned with the
    /// `lanes()` Vec: entry `i` is the lane whose [`LaneStatus::index`] is
    /// `i + 1`.
    pub lanes: Vec<LaneFlags>,
}

/// One code-block checksum comparison: the stored byte against the value
/// recomputed from the covered bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksumEntry {
    /// The checksum byte as stored in the module.
    pub stored: u8,
    /// The checksum recomputed from the covered byte range.
    pub computed: u8,
    /// True when the stored byte equals the recomputed value.
    pub valid: bool,
}

/// Checksum validation for the code blocks the spec defines. `None` for a
/// block the family does not have (SFF-8472 DMI) or that was not supplied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChecksumStatus {
    /// Base code-block checksum (SFF-8472 A0h:63; SFF-8636 Lower:191;
    /// CMIS Lower:95).
    pub cc_base: Option<ChecksumEntry>,
    /// Extended/upper code-block checksum, when the family defines one.
    pub cc_ext: Option<ChecksumEntry>,
    /// Digital-diagnostics-monitoring checksum (SFF-8472 A2h:95).
    pub cc_dmi: Option<ChecksumEntry>,
}

// --- Application & ModuleState (SFF-8472 / SFF-8636) ---

/// Application profile describing the transceiver's compliance specification(s).
///
/// Populated by `applications()` from spec-compliance bytes in the EEPROM.
/// For SFF-8472, this covers bytes 3-12 (transceiver compliance bitmap +
/// extended spec compliance code) and bytes 13-14 (rate identifier + signaling rate).
/// For SFF-8636, this covers page 00h bytes 3-10 (compliance bitmap),
/// byte 192 (extended spec), and byte 158 (signaling rate).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Application {
    /// Extended spec compliance code name (e.g. "10GBASE-SR", "40GBASE-SR4",
    /// "100GBASE-SR4") derived from Sff8024ExtendedSpecCompliance.
    pub extended_spec: String,
    /// Active spec-compliance bit descriptions (e.g. ["10GBASE-SR"],
    /// ["40GBASE-SR4", "10GBASE-SR"]).
    pub spec_bits: Vec<String>,
    /// Nominal signaling rate in GT/s (e.g. 10.3125, 25.781, 29.952).
    pub rate_gt: Option<f32>,
    /// Connector type (e.g. "LC", "MPO", "DA").
    pub connector: String,
}

/// Module operational state synthesized from status and alarm/warning flags.
///
/// Populated by `module_state()` from status bytes in the EEPROM.
/// For SFF-8472, status is derived from options/enhanced-options bytes
/// (A0h:120, A0h:124-125); `data_not_ready` and `interrupt_asserted` are `None`
/// since SFF-8472 has no explicit status byte.
/// For SFF-8636, `data_not_ready` and `interrupt_asserted` come from byte 2
/// of the Lower memory page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleState {
    /// Module status description (e.g. "operational", "data_not_ready",
    /// "interrupt_asserted").
    pub status: String,
    /// Data-not-ready flag from status byte. `None` for SFF-8472.
    pub data_not_ready: Option<bool>,
    /// Interrupt-asserted flag. `None` for SFF-8472.
    pub interrupt_asserted: Option<bool>,
    /// Module temperature high alarm.
    pub temp_alarm: bool,
    /// Module temperature high warning.
    pub temp_warning: bool,
    /// VCC high alarm.
    pub vcc_alarm: bool,
    /// VCC high warning.
    pub vcc_warning: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn types_compile() {
        let _: ModuleFlags = ModuleFlags::default();
        let _: ChecksumStatus = ChecksumStatus::default();
        let _: Application = Application::default();
        let _: ModuleState = ModuleState::default();
        let _: LaneStatus = LaneStatus::default();
        let _ = ChecksumEntry {
            stored: 0,
            computed: 0,
            valid: true,
        };
        // Verify Application and ModuleState are serializable
        let app = Application {
            extended_spec: "10GBASE-SR".into(),
            spec_bits: vec!["10GBASE-SR".into()],
            rate_gt: Some(10.3125),
            connector: "LC".into(),
        };
        let _json = serde_json::to_string(&app).expect("Application serializable");
        let state = ModuleState {
            status: "operational".into(),
            data_not_ready: Some(false),
            interrupt_asserted: Some(false),
            temp_alarm: false,
            temp_warning: false,
            vcc_alarm: false,
            vcc_warning: false,
        };
        let _json = serde_json::to_string(&state).expect("ModuleState serializable");
    }

    #[test]
    fn lane_status_additive_signal_flags_skip_when_none() {
        // A lane built the pre-flag way (Default + assignment of the old
        // vocabulary) serializes to exactly the old shape: the old
        // `Option` fields emit `null`, the additive signal flags are
        // `None`-skipped (the `dp_state` precedent), not null-emitting.
        let mut lane = LaneStatus {
            index: 2,
            rx_power_mw: Some(0.5),
            ..Default::default()
        };
        let json = serde_json::to_value(&lane).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "index": 2,
                "rx_power_mw": 0.5,
                "tx_power_mw": null,
                "tx_bias_ma": null,
                "rx_los": null,
                "tx_disable": null,
                "tx_fault": null
            })
        );

        // The new vocabulary round-trips when set.
        lane.tx_los = Some(true);
        lane.tx_cdr_lol = Some(false);
        lane.rx_cdr_lol = Some(true);
        lane.tx_adaptive_eq_fault = Some(false);
        let json = serde_json::to_value(&lane).unwrap();
        assert_eq!(json["tx_los"], serde_json::json!(true));
        assert_eq!(json["tx_cdr_lol"], serde_json::json!(false));
        assert_eq!(json["rx_cdr_lol"], serde_json::json!(true));
        assert_eq!(json["tx_adaptive_eq_fault"], serde_json::json!(false));
        let back: LaneStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back.tx_los, Some(true));
        assert_eq!(back.tx_cdr_lol, Some(false));
        assert_eq!(back.rx_cdr_lol, Some(true));
        assert_eq!(back.tx_adaptive_eq_fault, Some(false));
        assert_eq!(back.index, 2);
    }
}
