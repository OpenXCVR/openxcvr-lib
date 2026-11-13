use openxcvr::unified::UnifiedModule;
use openxcvr_core::module::Module;

fn fixture_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn quick_start_paths_open_committed_sff8636_fixture() {
    let root = fixture_root();
    let m = UnifiedModule::open_sff8636_mmap(
        root.join("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin"),
        root.join("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin"),
        root.join("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin"),
        root.join("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin"),
        root.join("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin"),
    )
    .unwrap();
    let vi = m.vendor_info().unwrap();
    assert_eq!(vi.name, "FINISAR CORP");

    let applications = m.applications().unwrap();
    assert_eq!(applications.len(), 1);
    assert!(applications[0]
        .spec_bits
        .contains(&"40GBASE-SR4".to_string()));

    let state = m.module_state().unwrap();
    assert!(state.status.contains("operational"));
    assert_eq!(state.data_not_ready, Some(false));
    assert_eq!(state.interrupt_asserted, Some(false));
}

#[test]
fn opens_sff8472_fixture() {
    let root = fixture_root();
    let m = UnifiedModule::open_sff8472_mmap(
        root.join("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin"),
        root.join("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin"),
    )
    .unwrap();
    assert_eq!(m.identifier().unwrap(), 0x03);

    let applications = m.applications().unwrap();
    assert_eq!(applications.len(), 1);
    assert_eq!(applications[0].spec_bits, ["10GBASE-SR"]);
    assert_eq!(applications[0].extended_spec, "Unspecified");
    assert!((applications[0].rate_gt.unwrap() - 10.3).abs() < 0.1);

    let state = m.module_state().unwrap();
    assert!(state.status.contains("operational"));
    // A2h:110 = 0x12 on this capture: Data_Not_Ready (bit 0) clear,
    // Rx_LOS state (bit 1) asserted — per Table 9-16, now decoded
    // schema-first instead of through the former hand-coded masks.
    assert_eq!(state.data_not_ready, Some(false));
    assert_eq!(state.interrupt_asserted, None);
    assert!(!state.temp_alarm);
    assert!(!state.vcc_alarm);
}

#[test]
fn unsupported_cmis_revision_remains_a_detection_error() {
    let root = std::env::temp_dir().join(format!(
        "openxcvr-unsupported-cmis-rev-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let paths: Vec<_> = (0..7).map(|i| root.join(format!("page-{i}.bin"))).collect();
    for (i, path) in paths.iter().enumerate() {
        let mut page = [0u8; 128];
        if i == 0 {
            page[1] = 0xff;
        }
        std::fs::write(path, page).unwrap();
    }

    let result = UnifiedModule::open_cmis_mmap(
        &paths[0], &paths[1], &paths[2], &paths[3], &paths[4], &paths[5], &paths[6],
    );
    std::fs::remove_dir_all(&root).unwrap();

    assert!(matches!(
        result,
        Err(openxcvr::ModuleError::Detection(error))
            if matches!(*error, openxcvr::core::DetectError::UnsupportedRev {
                detected_byte: 0xff,
                ..
            })
    ));
}

#[test]
fn cmis_mmap_lanes_and_flags_use_page_11h_capture() {
    let root = fixture_root().join("tests/fixtures/cmis");
    let page = |suffix: &str| root.join(format!("fboss-cmis-4-0-qsfpdd-400g-dr4.{suffix}.bin"));
    // Capture revision is 4.0; this tests routing, not new spec semantics.
    assert_eq!(std::fs::read(page("lower")).unwrap()[1], 0x40);
    let module = UnifiedModule::open_cmis_mmap(
        page("lower"),
        page("page-00h"),
        page("page-01h"),
        page("page-02h"),
        page("page-02h"),
        page("page-02h"),
        page("page-02h"),
    )
    .unwrap();
    assert!(module.lanes().is_err());
    let module = module.with_cmis_page_11h(page("page-11h")).unwrap();
    let lanes = module.lanes().unwrap();
    assert_eq!(lanes.len(), 8);
    assert!(lanes.iter().all(|lane| lane.rx_power_mw.is_some()));
    assert_eq!(module.flags().unwrap().lanes.len(), 8);
}
