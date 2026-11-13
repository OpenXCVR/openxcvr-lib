use openxcvr_core::module::Module;
use openxcvr_sff8472::{adapter::Sff8472Module, mmap::Sff8472MmapTransport};

fn fixture_path(name: &str) -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    root.join(name)
}

fn golden(name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let path = root.join(name);
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{}", path.display())),
    )
    .unwrap()
}

fn parse_hex(s: &str) -> u8 {
    u8::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap()
}

#[test]
fn golden_ftlx8571d3bcl_mup0wb0() {
    let t = Sff8472MmapTransport::from_files(
        fixture_path("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin"),
        fixture_path("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin"),
    )
    .unwrap();
    let m = Sff8472Module::new(t);
    let g = golden("tests/golden/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.json");

    assert_eq!(
        m.identifier().unwrap(),
        parse_hex(g["module"]["identifier"]["raw"].as_str().unwrap())
    );

    let vi = m.vendor_info().unwrap();
    assert_eq!(vi.name, g["identity"]["vendor_name"].as_str().unwrap());
    assert_eq!(vi.part_number, g["identity"]["vendor_pn"].as_str().unwrap());
    assert_eq!(
        vi.serial_number,
        g["identity"]["vendor_sn"].as_str().unwrap()
    );
    assert_eq!(vi.date_code, g["identity"]["date_code"].as_str().unwrap());
    let oui_str = format!("{:02x}:{:02x}:{:02x}", vi.oui[0], vi.oui[1], vi.oui[2]);
    assert_eq!(oui_str, g["identity"]["vendor_oui"].as_str().unwrap());

    let temp = m.temperature_celsius().unwrap().unwrap();
    let g_temp = g["monitoring"]["module_temperature_c"].as_f64().unwrap() as f32;
    assert!(
        (temp - g_temp).abs() < 0.001,
        "temp {temp} != golden {g_temp}"
    );

    let volt = m.supply_voltage_v().unwrap().unwrap();
    let g_volt = g["monitoring"]["module_voltage_v"].as_f64().unwrap() as f32;
    assert!(
        (volt - g_volt).abs() < 0.0001,
        "voltage {volt} != golden {g_volt}"
    );

    assert_eq!(m.lane_count().unwrap(), 1);
    let lanes = m.lanes().unwrap();
    assert_eq!(lanes.len(), 1);
    let l = &lanes[0];
    let gl = &g["lanes"][0];
    assert_eq!(l.index, 1);
    let bias = l.tx_bias_ma.unwrap();
    let g_bias = gl["tx_bias_ma"].as_f64().unwrap() as f32;
    assert!(
        (bias - g_bias).abs() < 0.01,
        "tx_bias_ma {bias} != {g_bias}"
    );
    let tx = l.tx_power_mw.unwrap();
    let g_tx = gl["tx_power_mw"].as_f64().unwrap() as f32;
    assert!((tx - g_tx).abs() < 0.001, "tx_power_mw {tx} != {g_tx}");
    assert_eq!(l.rx_los.unwrap(), gl["rx_los"].as_bool().unwrap());
    assert_eq!(l.tx_disable.unwrap(), gl["tx_disable"].as_bool().unwrap());
    assert_eq!(l.tx_fault.unwrap(), gl["tx_fault"].as_bool().unwrap());

    let fl = m.flags().unwrap();
    let alarms = &g["flags"]["alarm_summary"];
    assert_eq!(
        fl.temp_high_alarm,
        alarms["temp_high_alarm"].as_bool().unwrap()
    );
    assert_eq!(
        fl.temp_low_alarm,
        alarms["temp_low_alarm"].as_bool().unwrap()
    );
    assert_eq!(
        fl.lanes[0].rx_power_low_alarm,
        alarms["rx_power_low_alarm"].as_bool().unwrap()
    );
    let warns = &g["flags"]["warning_summary"];
    assert_eq!(
        fl.lanes[0].rx_power_low_warning,
        warns["rx_power_low_warning"].as_bool().unwrap()
    );

    let cs = m.checksums().unwrap();
    let cc_base = cs.cc_base.unwrap();
    assert_eq!(
        cc_base.stored,
        parse_hex(g["checksums"]["cc_base"]["stored"].as_str().unwrap())
    );
    assert!(cc_base.valid);
    let cc_dmi = cs.cc_dmi.unwrap();
    assert_eq!(
        cc_dmi.stored,
        parse_hex(g["checksums"]["cc_dmi"]["stored"].as_str().unwrap())
    );
    assert!(cc_dmi.valid);

    // applications() — returns 1 Application with spec compliance info
    let apps = m.applications().unwrap();
    assert_eq!(apps.len(), 1);
    let app = &apps[0];
    // This module is 10GBASE-SR (golden compliance has wavelength_nm: 850)
    assert!(
        !app.spec_bits.is_empty(),
        "applications should have at least one spec bit"
    );
    assert!(app.spec_bits.contains(&"10GBASE-SR".into()));
    assert_eq!(app.extended_spec, "Unspecified"); // byte 12 is 0x00 for this module
    assert!(app.rate_gt.is_some(), "should have signaling rate");
    assert!(
        (app.rate_gt.unwrap() - 10.3).abs() < 0.1,
        "10G module rate ~10.3 GT/s, got {}",
        app.rate_gt.unwrap()
    );
    assert!(!app.connector.is_empty());

    // module_state() — returns ModuleState with alarm/warning info. The
    // fixture's A2h byte 110 is 0x12: Data_Not_Ready (bit 0) clear, Rx_LOS
    // state (bit 1) asserted per Table 9-16 — the schema split of byte 110
    // corrected the earlier hand-coded masks (which read the TX Fault and
    // soft rate select bits for rx_los/tx_fault).
    let state = m.module_state().unwrap();
    assert!(!state.status.is_empty());
    assert!(state.status.contains("operational"));
    assert_eq!(state.data_not_ready, Some(false)); // A2h:110.0, Table 9-16
    assert_eq!(state.interrupt_asserted, None);
    // Golden JSON has temp_high_alarm=false for this module
    assert!(
        !state.temp_alarm,
        "temp_alarm should be false for this fixture"
    );
    // Golden JSON has vcc_low_alarm=false for this module
    assert!(
        !state.vcc_alarm,
        "vcc_alarm should be false for this fixture"
    );
}

#[test]
fn golden_ftlx8571d3bcl_muq1bzb() {
    let t = Sff8472MmapTransport::from_files(
        fixture_path("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-muq1bzb.a0h.bin"),
        fixture_path("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-muq1bzb.a2h.bin"),
    )
    .unwrap();
    let m = Sff8472Module::new(t);
    let g = golden("tests/golden/sff-8472/finisar-ftlx8571d3bcl-muq1bzb.json");

    let vi = m.vendor_info().unwrap();
    assert_eq!(vi.name, g["identity"]["vendor_name"].as_str().unwrap());
    assert_eq!(vi.part_number, g["identity"]["vendor_pn"].as_str().unwrap());

    let temp = m.temperature_celsius().unwrap().unwrap();
    let g_temp = g["monitoring"]["module_temperature_c"].as_f64().unwrap() as f32;
    assert!((temp - g_temp).abs() < 0.001);

    let cs = m.checksums().unwrap();
    assert!(cs.cc_base.unwrap().valid);
    assert!(cs.cc_dmi.unwrap().valid);

    // applications() — sanity check
    let apps = m.applications().unwrap();
    assert_eq!(apps.len(), 1);
    assert!(!apps[0].spec_bits.is_empty());
    assert!(!apps[0].connector.is_empty());

    // module_state() — sanity check; this fixture's A2h byte 110 is 0x12,
    // so Data_Not_Ready (bit 0) decodes clear (A2h:110.0, Table 9-16).
    let state = m.module_state().unwrap();
    assert!(!state.status.is_empty());
    assert_eq!(state.data_not_ready, Some(false));
}
