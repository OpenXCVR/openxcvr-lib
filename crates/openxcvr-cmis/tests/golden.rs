//! T35: Golden integration tests against fboss CMIS fixtures.
//!
//! The bundled fboss CMIS fixture corpus only carries lower + Pages 00h/01h/02h
//! (plus banked Pages 10h+ which are deferred to v0.2). Pages 03h/04h/05h are
//! synthesized as zero-fill at test time so v0.1's 7-page CmisModule constructor
//! has a complete unbanked surface to load. The synthesis is a test-only fixture
//! convention — production callers would supply real Page 03h/04h/05h reads.

use openxcvr_cmis::{module::CmisTransport, CmisModule};
use openxcvr_core::{error::ModuleError, module::Module, transport::Transport};

/// In-memory transport: 7 page-sized buffers, addressed page-relative.
/// Mirrors the byte-layout `CmisMmapTransport` exposes via `Transport::read`.
struct BytesTransport {
    lower: [u8; 128],
    page_00h: [u8; 128],
    page_01h: [u8; 128],
    page_02h: [u8; 128],
    page_03h: [u8; 128],
    page_04h: [u8; 128],
    page_05h: [u8; 128],
    page_11h: [u8; 128],
}

impl Transport for BytesTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x00, 128..=255) => &self.page_00h[(offset as usize - 128)..],
            (0x01, 128..=255) => &self.page_01h[(offset as usize - 128)..],
            (0x02, 128..=255) => &self.page_02h[(offset as usize - 128)..],
            (0x03, 128..=255) => &self.page_03h[(offset as usize - 128)..],
            (0x04, 128..=255) => &self.page_04h[(offset as usize - 128)..],
            (0x05, 128..=255) => &self.page_05h[(offset as usize - 128)..],
            (0x11, 128..=255) => &self.page_11h[(offset as usize - 128)..],
            _ => {
                return Err(ModuleError::Io(
                    format!("unsupported page/offset 0x{page:02x}/{offset}").into(),
                ))
            }
        };
        if buf.len() > src.len() {
            return Err(ModuleError::Io(
                format!(
                    "read overflow: page=0x{page:02x} offset={offset} len={}",
                    buf.len()
                )
                .into(),
            ));
        }
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }
    fn write(&mut self, _: u8, _: u8, _: u8, _: u8, _: &[u8]) -> Result<(), ModuleError> {
        Err(ModuleError::Io("read-only test transport".into()))
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
impl CmisTransport for BytesTransport {}

fn read_page(stem: &str, suffix: &str) -> [u8; 128] {
    let path = format!("../../tests/fixtures/cmis/{stem}.{suffix}");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("could not read fixture {path}: {e}"));
    assert_eq!(
        bytes.len(),
        128,
        "{path} expected 128 bytes, got {}",
        bytes.len()
    );
    bytes.try_into().unwrap()
}

/// Like [`read_page`] but returns zero-fill when the slice file is absent.
/// Banked Page 11h is present only for paged fixtures; flat-memory fixtures
/// (and any without per-lane data) load it as zeros.
fn read_page_opt(stem: &str, suffix: &str) -> [u8; 128] {
    let path = format!("../../tests/fixtures/cmis/{stem}.{suffix}");
    match std::fs::read(&path) {
        Ok(bytes) => {
            assert_eq!(
                bytes.len(),
                128,
                "{path} expected 128 bytes, got {}",
                bytes.len()
            );
            bytes.try_into().unwrap()
        }
        Err(_) => [0u8; 128],
    }
}

fn open_fixture(stem: &str) -> CmisModule<BytesTransport> {
    CmisModule::new(BytesTransport {
        lower: read_page(stem, "lower.bin"),
        page_00h: read_page(stem, "page-00h.bin"),
        page_01h: read_page(stem, "page-01h.bin"),
        page_02h: read_page(stem, "page-02h.bin"),
        page_03h: [0u8; 128],
        page_04h: [0u8; 128],
        page_05h: [0u8; 128],
        page_11h: read_page_opt(stem, "page-11h.bin"),
    })
    .expect("CMIS detect")
}

#[test]
fn fboss_cmis_4_0_qsfpdd_400g_dr4_vendor_info() {
    let m = open_fixture("fboss-cmis-4-0-qsfpdd-400g-dr4");
    let info = m.vendor_info().expect("vendor_info");
    // Redacted fboss fixtures zero OUI and PN; only vendor_name is preserved.
    assert!(!info.name.trim().is_empty(), "vendor name: {:?}", info.name);
}

#[test]
fn fboss_cmis_4_0_qsfpdd_400g_dr4_identifier_is_qsfpdd() {
    let m = open_fixture("fboss-cmis-4-0-qsfpdd-400g-dr4");
    let id = m.identifier().expect("identifier");
    assert_eq!(
        id, 0x18,
        "QSFP-DD identifier byte should be 0x18, got 0x{id:02x}"
    );
}

#[test]
fn fboss_cmis_4_0_qsfpdd_400g_dr4_temperature_is_plausible() {
    let m = open_fixture("fboss-cmis-4-0-qsfpdd-400g-dr4");
    let t = m
        .temperature_celsius()
        .expect("temperature_celsius")
        .expect("Some(temp)");
    assert!(
        (-40.0..=125.0).contains(&t),
        "temperature {t} °C outside operating range"
    );
}

#[test]
fn fboss_cmis_4_0_qsfpdd_400g_dr4_voltage_is_plausible() {
    let m = open_fixture("fboss-cmis-4-0-qsfpdd-400g-dr4");
    let v = m
        .supply_voltage_v()
        .expect("supply_voltage_v")
        .expect("Some(volts)");
    assert!(
        (2.5..=4.0).contains(&v),
        "supply voltage {v} V outside 3.3 V tolerance band"
    );
}

/// Ground the Rust reference adapter in the cross-binding golden JSON. Rust is
/// never JSON-compared in the parity runner (it asserts at the field level), so
/// this is the only place the Rust `lanes()`/`flags()` output is checked against
/// the same `tests/golden/cmis/*.json` the C/Go/Python aggregators reproduce.
fn assert_lanes_flags_match_golden(stem: &str) {
    let m = open_fixture(stem);

    let golden_path = format!("../../tests/golden/cmis/{stem}.json");
    let raw = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("read golden {golden_path}: {e}"));
    let golden: serde_json::Value = serde_json::from_str(&raw).expect("parse golden json");

    assert_eq!(m.lane_count().expect("lane_count"), 8);

    // --- lanes ---
    let lanes = m.lanes().expect("lanes");
    let g_lanes = golden["lanes"].as_array().expect("golden lanes array");
    assert_eq!(lanes.len(), g_lanes.len(), "{stem}: lane count");
    for (i, (lane, g)) in lanes.iter().zip(g_lanes).enumerate() {
        assert_eq!(
            lane.index as u64,
            g["index"].as_u64().unwrap(),
            "{stem} lane {i} index"
        );
        assert_eq!(
            lane.dp_state.as_deref(),
            g["dp_state"].as_str(),
            "{stem} lane {i} dp_state"
        );
        for (got, key) in [
            (lane.rx_power_mw, "rx_power_mw"),
            (lane.tx_power_mw, "tx_power_mw"),
            (lane.tx_bias_ma, "tx_bias_ma"),
        ] {
            match (got, &g[key]) {
                // Present numeric value: f32 (adapter) vs f64 (golden)
                // representation noise is ~1e-3 worst case at a rounding
                // boundary; a real decode error shifts the value by whole units
                // or a 2-4x scale factor, so 2e-3 is meaningfully tight.
                (Some(got), want) if want.is_number() => {
                    let want = want.as_f64().unwrap();
                    assert!(
                        (got as f64 - want).abs() < 2e-3,
                        "{stem} lane {i} {key}: got {got}, want {want}"
                    );
                }
                // Reserved/undefined scaling (e.g. tx_bias_ma under the reserved
                // 0b11 Page 01h:160 code): the adapter's None must pair with a
                // present golden null — the byte-identical shape the other
                // bindings emit.
                (None, want) if want.is_null() => {}
                (got, want) => panic!(
                    "{stem} lane {i} {key}: option/null mismatch — decoded {got:?}, golden {want}"
                ),
            }
        }
        // Per-lane status latches (§ 8.10.3): rx_los ← LOSFlagRx<lane>
        // (Table 8-88), tx_fault ← FailureFlagTx<lane> (Table 8-87). Both
        // exist in every admitted revision's reader, so the adapter must
        // always surface them as booleans on a paged module — never None.
        assert_eq!(lane.rx_los, g["rx_los"].as_bool(), "{stem} lane {i} rx_los");
        assert_eq!(
            lane.tx_fault,
            g["tx_fault"].as_bool(),
            "{stem} lane {i} tx_fault"
        );
    }

    // --- flags ---
    let flags = m.flags().expect("flags");
    let gf = &golden["flags"];
    assert!(gf["data_not_ready"].is_null() && flags.data_not_ready.is_none());
    assert_eq!(
        flags.interrupt_asserted,
        gf["interrupt_asserted"].as_bool(),
        "{stem} interrupt_asserted"
    );
    for (got, key) in [
        (flags.temp_high_alarm, "temp_high_alarm"),
        (flags.temp_low_alarm, "temp_low_alarm"),
        (flags.supply_high_alarm, "supply_high_alarm"),
        (flags.supply_low_alarm, "supply_low_alarm"),
    ] {
        assert_eq!(got, gf[key].as_bool().unwrap(), "{stem} {key}");
    }

    let g_lane_flags = gf["lanes"].as_array().expect("golden flags.lanes");
    assert_eq!(
        flags.lanes.len(),
        g_lane_flags.len(),
        "{stem}: flag-lane count"
    );
    for (i, (lf, g)) in flags.lanes.iter().zip(g_lane_flags).enumerate() {
        for (got, key) in [
            (lf.rx_power_high_alarm, "rx_power_high_alarm"),
            (lf.rx_power_low_alarm, "rx_power_low_alarm"),
            (lf.rx_power_high_warning, "rx_power_high_warning"),
            (lf.rx_power_low_warning, "rx_power_low_warning"),
            (lf.tx_bias_high_alarm, "tx_bias_high_alarm"),
            (lf.tx_bias_low_alarm, "tx_bias_low_alarm"),
            (lf.tx_bias_high_warning, "tx_bias_high_warning"),
            (lf.tx_bias_low_warning, "tx_bias_low_warning"),
            (lf.tx_power_high_alarm, "tx_power_high_alarm"),
            (lf.tx_power_low_alarm, "tx_power_low_alarm"),
            (lf.tx_power_high_warning, "tx_power_high_warning"),
            (lf.tx_power_low_warning, "tx_power_low_warning"),
        ] {
            assert_eq!(got, g[key].as_bool().unwrap(), "{stem} flag-lane {i} {key}");
        }
    }
}

#[test]
fn cmis_lanes_flags_match_golden_qsfpdd_dr4() {
    assert_lanes_flags_match_golden("fboss-cmis-4-0-qsfpdd-400g-dr4");
}

#[test]
fn cmis_lanes_flags_match_golden_osfp_zr() {
    assert_lanes_flags_match_golden("fboss-cmis-5-3-osfp-800g-zr");
}

#[test]
fn cmis_lanes_flags_match_golden_lpo() {
    // The 800G LPO fixture exercises the ×4 TxBias scaling multiplier.
    assert_lanes_flags_match_golden("fboss-cmis-5-1-osfp-2x400g-fr4-lpo");
}

#[test]
fn cmis_lanes_flags_match_golden_dp_state_reserved() {
    // Regression guard for the Page 11h reserved-nibble decode panic (untrusted
    // EEPROM DoS). This fixture is the zr capture with page-11h byte 0 flipped
    // 0x44->0x48, putting lane 1's dp_state in the reserved_8_f range (CMIS 5.3
    // § 8.10.1 / Table 8-84). Before the totality fix the generated enum's
    // fallible TryFrom panicked inside decode() on this byte; now it must render
    // "Unknown8" — byte-identical to what the C/Go/Python/PHP bindings emit.
    assert_lanes_flags_match_golden("fboss-cmis-5-3-osfp-800g-zr-dp-reserved");
}

#[test]
fn cmis_lanes_flags_match_golden_bias_reserved() {
    // Regression guard for the reserved Tx-bias scaling code 0b11 (Page 01h:160
    // bits 4-3, CMIS 5.3 Table 8-52). This fixture is the zr capture with
    // page-01h byte 160 flipped 0x17->0x1F (bits 4:3 10b->11b → reserved). The
    // scaling multiplier is module-wide, so every lane's tx_bias_ma must render
    // null while rx_power_mw/tx_power_mw/dp_state and all flags stay intact —
    // byte-identical to what the C/Go/Python/PHP bindings emit. Before the fix
    // all five bindings aborted the entire per-lane block on this code (sibling
    // bug to the Page 11h dp_state reserved-nibble decode panic).
    assert_lanes_flags_match_golden("fboss-cmis-5-3-osfp-800g-zr-bias-reserved");
}

/// CMIS 3.0 synthetic baseline (all-zero Page 11h): the unversioned
/// generated Page 11h names and the revision-qualified `lanes()` agree with
/// the independently authored golden — every lane `Reserved0`.
#[test]
fn synthetic_cmis_3_0_baseline_matches_golden() {
    assert_lanes_flags_match_golden("synthetic-cmis-3-0-qsfp-dd");
}

/// CMIS 3.0 reserved-DPSM synthetic variant: Page 11h packs the literal
/// Table 66 lane vector 0h,5h,6h,7h,3h,4h,0h,3h (`50 76 43 30`). The
/// fixture slice must actually exist with those bytes (never zero-fill):
/// a wrong rev-qualified rendering of raw 5/6/7 as the 4.0+ names would
/// fail the golden's `Unknown5`/`Unknown6`/`Unknown7` expectations while
/// the raw 0/3/4 controls stay `Reserved0`/`DpDeinit`/`DpActivated`.
#[test]
fn synthetic_cmis_3_0_reserved_matches_golden() {
    let p11 = read_page("synthetic-cmis-3-0-qsfp-dd-dp-reserved", "page-11h.bin");
    assert_eq!(
        &p11[..4],
        &[0x50, 0x76, 0x43, 0x30],
        "reserved fixture Page 11h must open with the packed Table 66 lane vector"
    );
    assert_eq!(
        &p11[4..],
        &[0u8; 124],
        "reserved fixture Page 11h flag/monitor region must be zero"
    );
    assert_lanes_flags_match_golden("synthetic-cmis-3-0-qsfp-dd-dp-reserved");
}

#[test]
fn cmis_lanes_unsupported_for_flat_memory() {
    // Flat-memory modules have no banked Page 11h: lanes()/flags()/lane_count()
    // all report Unsupported, which the JSON layer renders as null per contract.
    let m = CmisModule::new(BytesTransport {
        lower: read_page("fboss-cmis-4-0-qsfpdd-flatmem", "lower.bin"),
        page_00h: read_page("fboss-cmis-4-0-qsfpdd-flatmem", "page-00h.bin"),
        page_01h: [0u8; 128],
        page_02h: [0u8; 128],
        page_03h: [0u8; 128],
        page_04h: [0u8; 128],
        page_05h: [0u8; 128],
        page_11h: [0u8; 128],
    })
    .expect("CMIS detect");
    assert!(matches!(m.lanes(), Err(ModuleError::Unsupported)));
    assert!(matches!(m.flags(), Err(ModuleError::Unsupported)));
    assert!(matches!(m.lane_count(), Err(ModuleError::Unsupported)));
}

#[test]
fn cmis_checksums_returns_empty() {
    let m = open_fixture("fboss-cmis-4-0-qsfpdd-400g-dr4");
    let cs = m.checksums().expect("checksums");
    assert!(cs.cc_base.is_none() && cs.cc_ext.is_none() && cs.cc_dmi.is_none());
}

#[test]
fn fboss_cmis_5_3_osfp_zr_vendor_info() {
    let m = open_fixture("fboss-cmis-5-3-osfp-800g-zr");
    let info = m.vendor_info().expect("vendor_info");
    assert!(
        !info.name.trim().is_empty(),
        "ZR vendor name: {:?}",
        info.name
    );
    let id = m.identifier().expect("identifier");
    assert_eq!(
        id, 0x19,
        "OSFP identifier byte should be 0x19, got 0x{id:02x}"
    );
}
