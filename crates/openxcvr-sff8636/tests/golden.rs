use openxcvr_core::module::Module;
use openxcvr_core::testkit::{assert_lanes, LaneExpectation};
use openxcvr_sff8636::{adapter::Sff8636Module, mmap::Sff8636MmapTransport};

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
    let s = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden: {}", path.display()));
    serde_json::from_str(&s).unwrap()
}

fn parse_hex(s: &str) -> u8 {
    u8::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap()
}

#[test]
fn golden_ftl410qe3c_40g_sr4() {
    let t = Sff8636MmapTransport::from_files(
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin"),
    )
    .unwrap();
    let m = Sff8636Module::new(t);
    let g = golden("tests/golden/sff-8636/finisar-ftl410qe3c-40g-sr4.json");

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
    assert_eq!(vi.rev, g["identity"]["vendor_rev"].as_str().unwrap());
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

    assert_eq!(m.lane_count().unwrap(), 4);

    let lanes = m.lanes().unwrap();
    assert_eq!(lanes.len(), 4);
    for (i, l) in lanes.iter().enumerate() {
        let gl = &g["lanes"][i];
        assert_eq!(l.index, (i + 1) as u8);
        let rx = l.rx_power_mw.unwrap();
        let g_rx = gl["rx_power_mw"].as_f64().unwrap() as f32;
        assert!(
            (rx - g_rx).abs() < 0.001,
            "lane {i} rx_power_mw {rx} != {g_rx}"
        );
        let bias = l.tx_bias_ma.unwrap();
        let g_bias = gl["tx_bias_ma"].as_f64().unwrap() as f32;
        assert!(
            (bias - g_bias).abs() < 0.01,
            "lane {i} tx_bias_ma {bias} != {g_bias}"
        );
        let tx = l.tx_power_mw.unwrap();
        let g_tx = gl["tx_power_mw"].as_f64().unwrap() as f32;
        assert!(
            (tx - g_tx).abs() < 0.001,
            "lane {i} tx_power_mw {tx} != {g_tx}"
        );
    }

    let fl = m.flags().unwrap();
    assert_eq!(
        fl.data_not_ready.unwrap(),
        g["flags"]["data_not_ready"].as_bool().unwrap()
    );
    assert_eq!(
        fl.interrupt_asserted.unwrap(),
        g["flags"]["interrupt_asserted"].as_bool().unwrap()
    );

    let cs = m.checksums().unwrap();
    let cc_base = cs.cc_base.unwrap();
    assert_eq!(
        cc_base.stored,
        parse_hex(g["checksums"]["cc_base"]["stored"].as_str().unwrap())
    );
    assert_eq!(
        cc_base.computed,
        parse_hex(g["checksums"]["cc_base"]["computed"].as_str().unwrap())
    );
    assert!(cc_base.valid);
    let cc_ext = cs.cc_ext.unwrap();
    assert!(cc_ext.valid);

    // applications() — returns 1 Application with spec compliance info
    let apps = m.applications().unwrap();
    assert_eq!(apps.len(), 1);
    let app = &apps[0];
    // This module is 40GBASE-SR4 (golden compliance bits_set: ["compliance_40gbase_sr4"])
    assert!(
        !app.spec_bits.is_empty(),
        "applications should have at least one spec bit"
    );
    assert!(app.spec_bits.contains(&"40GBASE-SR4".into()));
    assert!(app.rate_gt.is_some(), "should have signaling rate");
    assert!(!app.connector.is_empty());

    // module_state() — returns ModuleState with status info
    let state = m.module_state().unwrap();
    assert!(!state.status.is_empty());
    assert!(state.status.contains("operational"));
    // Golden JSON has data_not_ready: false
    assert_eq!(state.data_not_ready, Some(false));
    // Golden JSON has interrupt_asserted: false
    assert_eq!(state.interrupt_asserted, Some(false));
}

#[test]
fn golden_ftlc9551repm_100g_sr4() {
    let t = Sff8636MmapTransport::from_files(
        fixture_path("tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4.lower.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4.page-00h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4.page-01h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4.page-02h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftlc9551repm-100g-sr4.page-03h.bin"),
    )
    .unwrap();
    let m = Sff8636Module::new(t);
    let g = golden("tests/golden/sff-8636/finisar-ftlc9551repm-100g-sr4.json");

    let vi = m.vendor_info().unwrap();
    assert_eq!(vi.name, g["identity"]["vendor_name"].as_str().unwrap());
    assert_eq!(vi.part_number, g["identity"]["vendor_pn"].as_str().unwrap());

    let temp = m.temperature_celsius().unwrap().unwrap();
    let g_temp = g["monitoring"]["module_temperature_c"].as_f64().unwrap() as f32;
    assert!((temp - g_temp).abs() < 0.001, "temp {temp} != {g_temp}");

    let cs = m.checksums().unwrap();
    assert!(cs.cc_base.unwrap().valid);
    assert!(cs.cc_ext.unwrap().valid);

    // applications() — sanity check
    let apps = m.applications().unwrap();
    assert_eq!(apps.len(), 1);
    assert!(!apps[0].spec_bits.is_empty());
    assert!(!apps[0].connector.is_empty());

    // module_state() — sanity check
    let state = m.module_state().unwrap();
    assert!(!state.status.is_empty());
    assert!(state.data_not_ready.is_some());
    assert!(state.interrupt_asserted.is_some());
}

/// A real (fixture-backed) module reports one-based [`LaneStatus::index`]
/// values — the same basis as the golden `lanes[].index` and the binding
/// JSON lane records — and the consumer test kit's `assert_lanes` accepts
/// one-based expectations for those indexes.
#[test]
fn lanes_indexes_are_one_based_and_assert_lanes_matches() {
    let t = Sff8636MmapTransport::from_files(
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin"),
        fixture_path("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin"),
    )
    .unwrap();
    let m = Sff8636Module::new(t);
    let g = golden("tests/golden/sff-8636/finisar-ftl410qe3c-40g-sr4.json");

    let lanes = m.lanes().unwrap();
    let indexes: Vec<u8> = lanes.iter().map(|l| l.index).collect();
    assert_eq!(
        indexes,
        vec![1, 2, 3, 4],
        "adapter lane indexes are one-based"
    );

    // assert_lanes takes the same one-based indexes; a real power value
    // still compares within tolerance.
    let expect: Vec<LaneExpectation> = (0..4usize)
        .map(|i| LaneExpectation {
            index: (i + 1) as u8,
            rx_power_mw: g["lanes"][i]["rx_power_mw"].as_f64().map(|v| v as f32),
            ..LaneExpectation::default()
        })
        .collect();
    assert_lanes(&lanes, &expect, 0.001).expect("one-based expectations match the real module");
}
