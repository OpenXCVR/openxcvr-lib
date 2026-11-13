//! Tests for schema citations (C-CMIS 1.4, ELSFP 1.0), Page 00h descriptions
//! (far_end_configuration, media_interface_technology), CDB index
//! completeness, and page-span containment for every C-CMIS 1.4 and ELSFP
//! 1.0 page schema (each field's citation must sit inside its page's span).

use openxcvr_codegen::ir::version::{resolve_field_for_rev, Rev};
use openxcvr_codegen::parser::pages::load_page_schema;
use serde_norway::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn schemas_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn load_yaml(rel_path: &str) -> Value {
    let path = schemas_root().join(rel_path);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_norway::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse YAML from {}: {e}", path.display()))
}

fn find_field<'a>(schema: &'a Value, name: &str) -> &'a Value {
    let fields = schema
        .get("fields")
        .and_then(|f| f.as_sequence())
        .expect("schema has no fields sequence");
    fields
        .iter()
        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some(name))
        .unwrap_or_else(|| panic!("field {name} not found"))
}

fn find_enum_entry<'a>(entries: &'a [Value], value: &str) -> &'a Value {
    entries
        .iter()
        .find(|e| {
            if let Some(v_str) = e.get("value").and_then(|v| v.as_str()) {
                v_str == value
            } else if let Some(v_int) = e.get("value").and_then(|v| v.as_i64()) {
                v_int.to_string() == value
                    || format!("{v_int:#04x}") == value
                    || format!("{v_int:#x}") == value
            } else {
                false
            }
        })
        .unwrap_or_else(|| panic!("enum entry with value {value} not found"))
}

/// Page-level `provenance.pdf_pages` of a page schema, as a vector of pages.
fn page_pdf_pages(schema: &Value) -> Vec<i64> {
    schema
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .unwrap_or_else(|| panic!("page schema has no provenance.pdf_pages"))
        .iter()
        .map(|v| v.as_i64().expect("pdf_pages entry is an integer"))
        .collect()
}

/// Assert every field's `provenance.pdf_page` lies inside the parent page
/// schema's `pdf_pages` span.
///
/// Fields whose `provenance.table` is a cross-table citation (a `"/"`-joined
/// pair like `"Table 7 / Table 2"` — a row in one table whose codes are
/// defined in another) cite the *defining* table's page instead, which can
/// sit outside the page's own span: ELSFP 1Ah `fault_code` cites Table 2's
/// p.13 and `warning_code` Table 3's p.14 (both pinned directly in
/// `elsfp_1_0_citations`). Those are exempt here; every single-table
/// citation must stay inside the span.
fn assert_fields_cite_within_span(fields_doc: &Value, span: &[i64], file: &str) {
    for field in fields_doc
        .get("fields")
        .and_then(|f| f.as_sequence())
        .unwrap_or_else(|| panic!("{file}: no fields sequence"))
    {
        let name = field
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("<unnamed>");
        let provenance = field
            .get("provenance")
            .unwrap_or_else(|| panic!("{file}: field {name} has no provenance"));
        let table = provenance
            .get("table")
            .and_then(|t| t.as_str())
            .unwrap_or_default();
        if table.contains('/') {
            continue;
        }
        let page = provenance
            .get("pdf_page")
            .and_then(|p| p.as_i64())
            .unwrap_or_else(|| panic!("{file}: field {name} provenance has no pdf_page"));
        assert!(
            span.contains(&page),
            "{file}: field {name} cites pdf_page {page}, outside the page span {span:?}"
        );
    }
}

/// Pin a batch of field citations: `table` and `pdf_page` for each named field.
fn assert_field_pins(schema: &Value, table: &str, pins: &[(&str, i64)], file: &str) {
    for (name, page) in pins {
        let prov = find_field(schema, name)
            .get("provenance")
            .unwrap_or_else(|| panic!("{file}: field {name} has no provenance"));
        assert_eq!(
            prov.get("table").and_then(|t| t.as_str()),
            Some(table),
            "{file}: field {name} table must be {table:?}"
        );
        assert_eq!(
            prov.get("pdf_page").and_then(|p| p.as_i64()),
            Some(*page),
            "{file}: field {name} pdf_page must be {page}"
        );
    }
}

#[test]
fn c_cmis_1_4_citations() {
    // 1. page-38h-data-path-host-if-config.yaml
    let p38 = load_yaml("c-cmis-1.4/page-38h-data-path-host-if-config.yaml");
    let p38_pages: Vec<i64> = p38
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    // Table 17 starts on physical p.28 (caption), rows sit on pp. 28-30 (byte 128-140 on p.29, 141-248 on p.30)
    assert_eq!(
        p38_pages,
        vec![28, 29, 30],
        "page-38h pdf_pages must be [28, 29, 30]"
    );
    let rx_cons_act = find_field(&p38, "rx_cons_act");
    let rx_cons_act_prov = rx_cons_act.get("provenance").expect("provenance");
    assert_eq!(
        rx_cons_act_prov.get("table").and_then(|t| t.as_str()),
        Some("Table 17")
    );
    let rx_cons_act_page = rx_cons_act_prov
        .get("pdf_page")
        .and_then(|p| p.as_i64())
        .unwrap();
    assert_eq!(
        rx_cons_act_page, 29,
        "rx_cons_act (byte 137) pdf_page must be 29 (got {rx_cons_act_page})"
    );
    let rx_rplc = find_field(&p38, "rx_rplc_sig_insertion");
    let rx_rplc_prov = rx_rplc.get("provenance").expect("provenance");
    assert_eq!(
        rx_rplc_prov.get("table").and_then(|t| t.as_str()),
        Some("Table 17")
    );
    let rx_rplc_page = rx_rplc_prov
        .get("pdf_page")
        .and_then(|p| p.as_i64())
        .unwrap();
    assert_eq!(
        rx_rplc_page, 30,
        "rx_rplc_sig_insertion (byte 145) pdf_page must be 30 (got {rx_rplc_page})"
    );

    // 2. page-41h-rx-signal-power-advertisement.yaml
    let p41 = load_yaml("c-cmis-1.4/page-41h-rx-signal-power-advertisement.yaml");
    let p41_pages: Vec<i64> = p41
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        p41_pages,
        vec![35, 36, 37],
        "page-41h pdf_pages must be [35, 36, 37]"
    );
    let rx_los_type = find_field(&p41, "rx_los_type");
    let rx_los_type_prov = rx_los_type.get("provenance").expect("provenance");
    assert_eq!(
        rx_los_type_prov.get("table").and_then(|t| t.as_str()),
        Some("Table 22")
    );
    assert_eq!(
        rx_los_type_prov.get("pdf_page").and_then(|p| p.as_i64()),
        Some(35)
    );
    let rx_total_pwr = find_field(&p41, "rx_total_pwr_hi_alm_thresh_max");
    let rx_total_pwr_prov = rx_total_pwr.get("provenance").expect("provenance");
    assert_eq!(
        rx_total_pwr_prov.get("pdf_page").and_then(|p| p.as_i64()),
        Some(36)
    );

    // 3. page-35h-media-lane-link-pm.yaml
    let p35 = load_yaml("c-cmis-1.4/page-35h-media-lane-link-pm.yaml");
    let p35_pages: Vec<i64> = p35
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        p35_pages,
        vec![24, 25, 26, 27],
        "page-35h pdf_pages must be [24, 25, 26, 27]"
    );
    let rx_avg_cd = find_field(&p35, "rx_avg_cd_pm");
    assert_eq!(
        rx_avg_cd
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 15")
    );
    assert_eq!(
        rx_avg_cd
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(25)
    );
    let rx_avg_evm = find_field(&p35, "rx_avg_evm_modem_pm");
    assert_eq!(
        rx_avg_evm
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(26)
    );
    let rx_avg_q = find_field(&p35, "rx_avg_q_margin_pm");
    assert_eq!(
        rx_avg_q
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(27)
    );

    // 4. page-3bh-data-path-host-if-flags-and-masks.yaml
    let p3b = load_yaml("c-cmis-1.4/page-3bh-data-path-host-if-flags-and-masks.yaml");
    let p3b_pages: Vec<i64> = p3b
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        p3b_pages,
        vec![32, 33, 34],
        "page-3bh pdf_pages must be [32, 33, 34]"
    );
    let m_tx_fed = find_field(&p3b, "m_tx_fed_pm");
    assert_eq!(
        m_tx_fed
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 19")
    );
    assert_eq!(
        m_tx_fed
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(32)
    );
    let m_tx_loa = find_field(&p3b, "m_tx_loa");
    assert_eq!(
        m_tx_loa
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(33)
    );
    let l_tx_loa = find_field(&p3b, "l_tx_loa");
    assert_eq!(
        l_tx_loa
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(34)
    );

    // 5. page-42h-pm-advertisement.yaml
    let p42 = load_yaml("c-cmis-1.4/page-42h-pm-advertisement.yaml");
    let p42_pages: Vec<i64> = p42
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        p42_pages,
        vec![37, 38, 39, 40, 41],
        "page-42h pdf_pages must be [37, 38, 39, 40, 41]"
    );
    let rx_bits_pm = find_field(&p42, "rx_bits_pm_impl");
    assert_eq!(
        rx_bits_pm
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 23")
    );
    assert_eq!(
        rx_bits_pm
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(37)
    );
    let rx_cd_impl = find_field(&p42, "rx_cd_impl");
    assert_eq!(
        rx_cd_impl
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(38)
    );

    // 6. page-43h-media-lane-provisioning-advertisement.yaml
    let p43 = load_yaml("c-cmis-1.4/page-43h-media-lane-provisioning-advertisement.yaml");
    let p43_pages: Vec<i64> = p43
        .get("provenance")
        .and_then(|p| p.get("pdf_pages"))
        .and_then(|p| p.as_sequence())
        .expect("pdf_pages sequence")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        p43_pages,
        vec![41, 42],
        "page-43h pdf_pages must be [41, 42]"
    );
    let tx_payload = find_field(&p43, "tx_payload_type_impl");
    assert_eq!(
        tx_payload
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 24")
    );
    assert_eq!(
        tx_payload
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(41)
    );
    let custom = find_field(&p43, "custom");
    assert_eq!(
        custom
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(42)
    );
}

#[test]
fn c_cmis_1_4_page_pdf_pages_and_field_containment() {
    // All 15 C-CMIS 1.4 page schemas (30h through 45h), with the pinned
    // page-level `pdf_pages` span each table actually occupies in
    // OIF-C-CMIS-01.4_IA.pdf (physical pages; printed footers verified).
    let pinned: &[(&str, &[i64])] = &[
        (
            "page-30h-media-lane-configurable-thresholds.yaml",
            &[18, 19],
        ),
        ("page-31h-media-lane-provisioning.yaml", &[19, 20]),
        ("page-32h-media-lane-flag-masks.yaml", &[20, 21, 22]),
        ("page-33h-media-lane-flags-and-status.yaml", &[22, 23]),
        ("page-34h-media-lane-fec-pm.yaml", &[23, 24]),
        ("page-35h-media-lane-link-pm.yaml", &[24, 25, 26, 27]),
        ("page-38h-data-path-host-if-config.yaml", &[28, 29, 30]),
        ("page-3ah-data-path-host-if-pm.yaml", &[30, 31]),
        (
            "page-3bh-data-path-host-if-flags-and-masks.yaml",
            &[32, 33, 34],
        ),
        ("page-40h-applications-advertisement.yaml", &[35]),
        ("page-41h-rx-signal-power-advertisement.yaml", &[35, 36, 37]),
        ("page-42h-pm-advertisement.yaml", &[37, 38, 39, 40, 41]),
        (
            "page-43h-media-lane-provisioning-advertisement.yaml",
            &[41, 42],
        ),
        ("page-44h-alarm-advertisement.yaml", &[42, 43, 44, 45]),
        ("page-45h-host-lane-provisioning-advertisement.yaml", &[45]),
    ];
    assert_eq!(
        pinned.len(),
        15,
        "all 15 C-CMIS 1.4 page schemas must be pinned"
    );
    for (file, expected) in pinned {
        let schema = load_yaml(&format!("c-cmis-1.4/{file}"));
        let span = page_pdf_pages(&schema);
        assert_eq!(
            &span, expected,
            "c-cmis-1.4/{file} provenance.pdf_pages must be {expected:?}"
        );
        assert_fields_cite_within_span(&schema, &span, &format!("c-cmis-1.4/{file}"));
    }
}

#[test]
fn c_cmis_1_4_remaining_citation_pins() {
    // 31h — Table 11's byte-128 row splits at the p.19/p.20 break: bits 1/0
    // and bytes 129-130 print on p.20.
    let p31 = load_yaml("c-cmis-1.4/page-31h-media-lane-provisioning.yaml");
    assert_field_pins(
        &p31,
        "Table 11",
        &[
            ("lf_insertion_on_ld_enable", 20),
            ("tx_filter_enable", 20),
            ("tx_filter_type", 20),
            ("tx_filter_roll_off", 20),
        ],
        "page-31h",
    );

    // 32h — Table 12's byte-128 row splits at p.20/p.21 (bits 3-0 and bytes
    // 129-133 on p.21); bytes 134-247 and 248-255 print on p.22.
    let p32 = load_yaml("c-cmis-1.4/page-32h-media-lane-flag-masks.yaml");
    assert_field_pins(
        &p32,
        "Table 12",
        &[
            ("m_tx_lol_cmu", 21),
            ("m_tx_lol_ref_clk", 21),
            ("m_tx_lol_de_skew", 21),
            ("m_tx_fifo", 21),
            ("reserved_129_129", 21),
            ("reserved_134_247", 22),
            ("custom", 22),
        ],
        "page-32h",
    );

    // 33h — Table 13's byte-130 row splits at p.22/p.23 (bit 0 and byte 131 on p.23).
    let p33 = load_yaml("c-cmis-1.4/page-33h-media-lane-flags-and-status.yaml");
    assert_field_pins(
        &p33,
        "Table 13",
        &[("l_rx_lol_fifo", 23), ("reserved_131_131", 23)],
        "page-33h",
    );

    // 34h — Table 14's p.23 ends after byte 135; bytes 144-151 (rxCorrBitsPm)
    // are the first rows of p.24.
    let p34 = load_yaml("c-cmis-1.4/page-34h-media-lane-fec-pm.yaml");
    assert_field_pins(&p34, "Table 14", &[("rx_corr_bits_pm", 24)], "page-34h");

    // 3ah — Table 18's p.30 carries only bytes 128-135; bytes 136-248 print on p.31.
    let p3a = load_yaml("c-cmis-1.4/page-3ah-data-path-host-if-pm.yaml");
    assert_field_pins(
        &p3a,
        "Table 18",
        &[
            ("tx_bits_sub_int_pm", 31),
            ("tx_corr_bits_pm", 31),
            ("tx_min_corr_bits_sub_int_pm", 31),
            ("tx_max_corr_bits_sub_int_pm", 31),
        ],
        "page-3ah",
    );

    // 40h — whole file: Table 21's caption and body (128, 129-247, 248-255)
    // are on p.35; p.28 holds Table 16/17.
    let p40 = load_yaml("c-cmis-1.4/page-40h-applications-advertisement.yaml");
    assert_field_pins(
        &p40,
        "Table 21",
        &[
            ("c_cmis_revision", 35),
            ("reserved_129_247", 35),
            ("custom", 35),
        ],
        "page-40h",
    );

    // 44h — whole file: Table 25 spans p.42-p.45. Row→page map: p.42 = byte
    // 128 (all bits) + byte 129 bits 7,6; p.43 = byte 129 bits 5-0, bytes
    // 130-132, byte 133 bits 7-1; p.44 = byte 133 bit 0, bytes 134-137,
    // 138-190, 191, 192-247; p.45 = 248-255 custom.
    let p44 = load_yaml("c-cmis-1.4/page-44h-alarm-advertisement.yaml");
    assert_field_pins(
        &p44,
        "Table 25",
        &[
            ("reserved_128_7_6", 42),
            ("media_tx_loa_impl", 42),
            ("media_tx_ooa_impl", 42),
            ("media_tx_lol_cmu_impl", 42),
            ("media_tx_lol_ref_clk_impl", 42),
            ("media_tx_lol_de_skew_impl", 42),
            ("media_tx_fifo_impl", 42),
            ("media_rx_lof_impl", 42),
            ("media_rx_lom_impl", 42),
            ("media_rx_lol_demod_impl", 43),
            ("media_rx_lol_cd_impl", 43),
            ("media_rx_loa_impl", 43),
            ("media_rx_ooa_impl", 43),
            ("media_rx_lol_deskew_impl", 43),
            ("media_rx_lol_fifo_impl", 43),
            ("reserved_130_7_2", 43),
            ("media_rx_fed_alm_impl", 43),
            ("media_rx_fdd_alm_impl", 43),
            ("reserved_131_7_6", 43),
            ("rx_stat_mnt_ais_impl", 43),
            ("rx_stat_mnt_lck_impl", 43),
            ("media_rx_pyld_typ_mm_impl", 43),
            ("media_rd_impl", 43),
            ("media_ld_impl", 43),
            ("media_stat_rf_impl", 43),
            ("reserved_132_7_2", 43),
            ("host_tx_fed_alm_impl", 43),
            ("host_tx_fdd_alm_impl", 43),
            ("reserved_133_7_5", 43),
            ("host_rx_rd_impl", 43),
            ("host_rx_ld_impl", 43),
            ("host_rx_cstat_mnt_lck_impl", 43),
            ("host_tx_rd_impl", 43),
            ("host_tx_ld_impl", 44),
            ("host_rx_flexe_rpf_impl", 44),
            ("host_rx_flexe_gid_mm_impl", 44),
            ("host_rx_flexe_instance_map_mm_impl", 44),
            ("host_rx_flexe_calendar_mm_impl", 44),
            ("host_rx_flexe_iid_mm_impl", 44),
            ("host_rx_flexe_lof_impl", 44),
            ("host_rx_flexe_lom_impl", 44),
            ("host_rx_flexe_lopb_impl", 44),
            ("reserved_135_7_3", 44),
            ("host_tx_loa_impl", 44),
            ("host_tx_rf_impl", 44),
            ("host_tx_lf_impl", 44),
            ("reserved_136_7_3", 44),
            ("host_rx_loa_impl", 44),
            ("host_rx_rf_impl", 44),
            ("host_rx_lf_impl", 44),
            ("reserved_137_7_2", 44),
            ("rx_msim_impl", 44),
            ("rx_cstat_csf_impl", 44),
            ("reserved_138_190", 44),
            ("reserved_191_7_1", 44),
            ("rx_payload_type_impl", 44),
            ("reserved_192_247", 44),
            ("custom", 45),
        ],
        "page-44h",
    );

    // 45h — whole file: Table 26's caption and body are on p.45.
    let p45 = load_yaml("c-cmis-1.4/page-45h-host-lane-provisioning-advertisement.yaml");
    assert_field_pins(
        &p45,
        "Table 26",
        &[
            ("reserved_128_7_6", 45),
            ("txfed_cons_act_enable_impl", 45),
            ("rxfed_cons_act_enable_impl", 45),
            ("rxlf_insertion_on_csf_enable_impl", 45),
            ("txlf_insertion_on_ld_enable_impl", 45),
            ("fed_mon_enable_impl", 45),
            ("fdd_mon_enable_impl", 45),
            ("reserved_129_7", 45),
            ("tx_cstat_lck_insertion_impl", 45),
            ("rx_rplc_sig_insertion_impl", 45),
            ("tx_rplc_sig_insertion_impl", 45),
            ("tx_cons_act_hold_off_tmr_impl", 45),
            ("rx_cons_act_hold_off_tmr_impl", 45),
            ("rx_cons_act_impl", 45),
            ("tx_cons_act_impl", 45),
            ("reserved_130_247", 45),
            ("custom", 45),
        ],
        "page-45h",
    );
}

#[test]
fn elsfp_1_0_citations() {
    // part-01.yaml
    let p1 = load_yaml("elsfp-1.0/fields/page-1ah-lane-mgmt-part-01.yaml");
    let max_opt = find_field(&p1, "max_optical_power");
    assert_eq!(
        max_opt
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 4")
    );
    assert_eq!(
        max_opt
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(15)
    );

    let max_bias = find_field(&p1, "max_laser_bias");
    assert_eq!(
        max_bias
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 4")
    );
    assert_eq!(
        max_bias
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(16),
        "max_laser_bias must be on PDF page 16"
    );

    let min_bias = find_field(&p1, "min_laser_bias");
    assert_eq!(
        min_bias
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 4")
    );
    assert_eq!(
        min_bias
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(16),
        "min_laser_bias must be on PDF page 16"
    );

    let res_136 = find_field(&p1, "reserved_136_139");
    assert_eq!(
        res_136
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 4")
    );
    assert_eq!(
        res_136
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(16),
        "reserved_136_139 must be on PDF page 16"
    );

    let bias_hi = find_field(&p1, "bias_hi_alarm_thresh");
    assert_eq!(
        bias_hi
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 4")
    );
    assert_eq!(
        bias_hi
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(16)
    );

    let lane_sum = find_field(&p1, "lane_summary_fault");
    assert_eq!(
        lane_sum
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 5")
    );
    assert_eq!(
        lane_sum
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(17)
    );

    let save_restore_status = find_field(&p1, "save_restore_status");
    assert_eq!(
        save_restore_status
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 6"),
        "save_restore_status must cite Table 6"
    );
    assert_eq!(
        save_restore_status
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(20),
        "save_restore_status must sit on PDF page 20"
    );

    // part-02.yaml
    let p2 = load_yaml("elsfp-1.0/fields/page-1ah-lane-mgmt-part-02.yaml");
    let hi_bias_alm = find_field(&p2, "high_bias_alarm");
    assert_eq!(
        hi_bias_alm
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 7")
    );
    assert_eq!(
        hi_bias_alm
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(20)
    );

    let fault_code = find_field(&p2, "fault_code");
    assert_eq!(
        fault_code
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 7 / Table 2"),
        "fault_code must cite Table 7 / Table 2"
    );
    assert_eq!(
        fault_code
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(13),
        "fault_code must cite Table 2 defining page (p.13)"
    );

    let warning_code = find_field(&p2, "warning_code");
    assert_eq!(
        warning_code
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 7 / Table 3"),
        "warning_code must cite Table 7 / Table 3"
    );
    assert_eq!(
        warning_code
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(14),
        "warning_code must cite Table 3 defining page (p.14)"
    );

    let lane_enable = find_field(&p2, "lane_enable");
    assert_eq!(
        lane_enable
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 8")
    );
    assert_eq!(
        lane_enable
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(26)
    );

    let fiber_flag = find_field(&p2, "output_fiber_checked_flag");
    assert_eq!(
        fiber_flag
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 9")
    );
    assert_eq!(
        fiber_flag
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(27)
    );

    let lane_mapping = find_field(&p2, "lane_to_fiber_mapping");
    assert_eq!(
        lane_mapping
            .get("provenance")
            .and_then(|p| p.get("table"))
            .and_then(|t| t.as_str()),
        Some("Table 10")
    );
    assert_eq!(
        lane_mapping
            .get("provenance")
            .and_then(|p| p.get("pdf_page"))
            .and_then(|p| p.as_i64()),
        Some(27)
    );
}

#[test]
fn elsfp_1_0_page_spans_and_citation_pins() {
    // page-1ah: the span was truncated at p.23; Page 1Ah's tables run
    // Table 4 (p.15-16) → Table 5 (p.17-19) → Table 6 (p.19-20) → Table 7
    // (p.20-25) → Table 8 (p.26) → Table 9 (p.26-27) → Table 10 (p.27-28).
    let p1a = load_yaml("elsfp-1.0/page-1ah-lane-mgmt.yaml");
    let span_1a = page_pdf_pages(&p1a);
    assert_eq!(
        span_1a,
        (15..=28).collect::<Vec<i64>>(),
        "page-1ah provenance.pdf_pages must cover pp. 15-28"
    );
    // Every field reached through field_includes must cite a page inside the
    // span (cross-table defining citations excepted — see the helper).
    for part in [
        "elsfp-1.0/fields/page-1ah-lane-mgmt-part-01.yaml",
        "elsfp-1.0/fields/page-1ah-lane-mgmt-part-02.yaml",
    ] {
        let part_doc = load_yaml(part);
        assert_fields_cite_within_span(&part_doc, &span_1a, part);
    }
    // Spot-pin the fields whose citations sit on the pages the truncated span
    // used to omit (Table 7's mask rows on p.24-25, Table 8 on p.26, Tables
    // 9/10 on p.27-28).
    let p1a_part2 = load_yaml("elsfp-1.0/fields/page-1ah-lane-mgmt-part-02.yaml");
    assert_field_pins(
        &p1a_part2,
        "Table 7",
        &[("low_pwr_alarm_mask", 24), ("global_warn_mask", 25)],
        "page-1ah part-02",
    );
    assert_field_pins(
        &p1a_part2,
        "Table 8",
        &[("lane_enable", 26), ("lane_state", 26)],
        "page-1ah part-02",
    );
    assert_field_pins(
        &p1a_part2,
        "Table 9",
        &[("output_fiber_checked_flag", 27)],
        "page-1ah part-02",
    );
    assert_field_pins(
        &p1a_part2,
        "Table 10",
        &[
            ("lane_to_fiber_mapping", 27),
            ("lane_freq", 27),
            ("opt_check_power_setpoint", 28),
            ("reserved_249_255", 28),
        ],
        "page-1ah part-02",
    );

    // page-1bh: Page 1Bh runs p.28-p.32 and its tables are numbered Table 11
    // (setpoints, caption p.28) and Table 12 (monitors, caption p.30) — the
    // schema's former Table 14/15 citations named tables that do not exist
    // in OIF-ELSFP-CMIS-01.0.pdf. reserved_160_183 sits inside Table 11; the
    // monitors and tail reservations are Table 12.
    let p1b = load_yaml("elsfp-1.0/page-1bh-lane-controls-monitors.yaml");
    let span_1b = page_pdf_pages(&p1b);
    assert_eq!(
        span_1b,
        vec![28, 29, 30, 31, 32],
        "page-1bh provenance.pdf_pages must cover pp. 28-32"
    );
    assert_fields_cite_within_span(
        &p1b,
        &span_1b,
        "elsfp-1.0/page-1bh-lane-controls-monitors.yaml",
    );
    assert_field_pins(
        &p1b,
        "Table 11",
        &[
            ("bias_current_setpoint", 28),
            ("opt_power_setpoint", 29),
            ("reserved_160_183", 30),
        ],
        "page-1bh",
    );
    assert_field_pins(
        &p1b,
        "Table 12",
        &[
            ("bias_current_monitor", 30),
            ("opt_power_monitor", 30),
            ("reserved_216_231", 31),
            ("voltage_monitor", 31),
            ("icc_monitor", 32),
            ("reserved_242_255", 32),
        ],
        "page-1bh",
    );
}

#[test]
fn cmis_page00h_far_end_configuration_description() {
    let p00_p4 = load_yaml("cmis/fields/page-00h-administrative-part-04.yaml");
    let far_end = find_field(&p00_p4, "far_end_configuration");

    // Check base block
    let base_entries = far_end
        .get("enum_inline")
        .and_then(|e| e.as_sequence())
        .expect("base enum_inline");
    let base_entry = find_enum_entry(base_entries, "4..26");
    assert_eq!(
        base_entry.get("name").and_then(|n| n.as_str()),
        Some("heterogeneous_breakout_4_26")
    );
    let base_desc = base_entry
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap();
    assert!(
        base_desc.contains("Table 8-38") && base_desc.contains("Table 8-37"),
        "base block 4..26 description must cite Table 8-38 and Table 8-37 (got {base_desc:?})"
    );
    assert!(
        !base_desc.contains("Table 34")
            && !base_desc.contains("Table 8-22")
            && !base_desc.contains("Table 8-32"),
        "base block must not cite Table 34/8-22/8-32 (got {base_desc:?})"
    );

    // Check changed_in blocks
    let changed_in = far_end
        .get("changed_in")
        .and_then(|c| c.as_sequence())
        .expect("changed_in sequence");

    for rev in ["3.0", "4.0", "5.0"] {
        let block = changed_in
            .iter()
            .find(|b| b.get("rev").and_then(|r| r.as_str()) == Some(rev))
            .unwrap_or_else(|| panic!("changed_in for rev {rev} not found"));
        let entries = block
            .get("enum_inline")
            .and_then(|e| e.as_sequence())
            .unwrap();
        let entry = find_enum_entry(entries, "4..26");
        assert_eq!(
            entry.get("name").and_then(|n| n.as_str()),
            Some("heterogeneous_breakout_4_26")
        );
        let desc = entry.get("description").and_then(|d| d.as_str()).unwrap();
        assert_eq!(
            desc, "Heterogeneous far-end lane grouping codes",
            "rev {rev} 4..26 description must be 'Heterogeneous far-end lane grouping codes' (got {desc:?})"
        );
        assert!(
            !desc.contains("Table 34")
                && !desc.contains("Table 8-22")
                && !desc.contains("Table 8-32")
                && !desc.contains("Table 8-37")
                && !desc.contains("Table 8-38"),
            "rev {rev} 4..26 description must not cite Table 34, 8-22, 8-32, 8-37, or 8-38 (got {desc:?})"
        );
    }

    let block_52 = changed_in
        .iter()
        .find(|b| b.get("rev").and_then(|r| r.as_str()) == Some("5.2"))
        .unwrap_or_else(|| panic!("changed_in for rev 5.2 not found"));
    let entries = block_52
        .get("enum_inline")
        .and_then(|e| e.as_sequence())
        .unwrap();
    let entry = find_enum_entry(entries, "4..26");
    assert_eq!(
        entry.get("name").and_then(|n| n.as_str()),
        Some("heterogeneous_breakout_4_26")
    );
    let desc = entry.get("description").and_then(|d| d.as_str()).unwrap();
    assert_eq!(
        desc, "Heterogeneous far-end lane grouping codes",
        "rev 5.2 4..26 description must be 'Heterogeneous far-end lane grouping codes' (got {desc:?})"
    );
    assert!(
        !desc.contains("Table 34")
            && !desc.contains("Table 8-22")
            && !desc.contains("Table 8-32")
            && !desc.contains("Table 8-37")
            && !desc.contains("Table 8-38"),
        "rev 5.2 4..26 description must not cite Table 34, 8-22, 8-32, 8-37, or 8-38 (got {desc:?})"
    );
    // CMIS 5.2 renumbered the byte-211 row into Table 8-33 and the code table
    // into Table 8-34 (p.145), so the 5.0 citation cannot be reused for 5.2.
    let prov_52 = block_52
        .get("provenance")
        .expect("rev 5.2 changed_in provenance");
    assert_eq!(
        prov_52.get("table").and_then(|t| t.as_str()),
        Some("Table 8-34"),
        "rev 5.2 provenance.table must be \"Table 8-34\""
    );
    assert_eq!(
        prov_52.get("pdf_page").and_then(|p| p.as_i64()),
        Some(145),
        "rev 5.2 provenance.pdf_page must be 145"
    );

    let block_53 = changed_in
        .iter()
        .find(|b| b.get("rev").and_then(|r| r.as_str()) == Some("5.3"))
        .unwrap_or_else(|| panic!("changed_in for rev 5.3 not found"));
    let entries_53 = block_53
        .get("enum_inline")
        .and_then(|e| e.as_sequence())
        .unwrap();
    let entry_53 = find_enum_entry(entries_53, "4..26");
    assert_eq!(
        entry_53.get("name").and_then(|n| n.as_str()),
        Some("heterogeneous_breakout_4_26")
    );
    let desc_53 = entry_53
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap();
    assert!(
        desc_53.contains("Table 8-38") && desc_53.contains("Table 8-37"),
        "rev 5.3 4..26 description must cite Table 8-38 and Table 8-37 (got {desc_53:?})"
    );
    assert!(
        !desc_53.contains("Table 34")
            && !desc_53.contains("Table 8-22")
            && !desc_53.contains("Table 8-32"),
        "rev 5.3 must not cite Table 34/8-22/8-32 (got {desc_53:?})"
    );
}

#[test]
fn cmis_page00h_far_end_configuration_resolved_provenance() {
    // Resolve far_end_configuration through the production loader and
    // `resolve_field_for_rev` — the same path generation uses — and pin the
    // provenance each revision resolves to.
    let page_path = schemas_root().join("cmis/page-00h-administrative.yaml");
    let schema = load_page_schema(&page_path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e:#}", page_path.display()));
    let field = schema
        .fields
        .iter()
        .find(|f| f.name == "far_end_configuration")
        .expect("far_end_configuration field");

    let pinned: &[(Rev, &str, i64)] = &[
        (Rev::V3_0, "Table 35", 63),
        (Rev::V4_0, "Table 8-23", 98),
        (Rev::V5_0, "Table 8-33", 125),
        (Rev::V5_2, "Table 8-34", 145),
        (Rev::V5_3, "Table 8-37 / Table 8-38", 165),
    ];
    for (rev, table, page) in pinned {
        let resolved = resolve_field_for_rev(field, *rev);
        let prov = resolved
            .provenance
            .as_ref()
            .unwrap_or_else(|| panic!("resolved at {} has no provenance", rev.as_str()));
        assert_eq!(
            prov.get("table").and_then(|t| t.as_str()),
            Some(*table),
            "far_end_configuration resolved at {} must cite table {table}",
            rev.as_str()
        );
        assert_eq!(
            prov.get("pdf_page").and_then(|p| p.as_i64()),
            Some(*page),
            "far_end_configuration resolved at {} must cite pdf_page {page}",
            rev.as_str()
        );
    }
}

#[test]
fn cmis_page00h_media_interface_technology_descriptions() {
    let p00_p4 = load_yaml("cmis/fields/page-00h-administrative-part-04.yaml");
    let mit = find_field(&p00_p4, "media_interface_technology");
    let changed_in = mit
        .get("changed_in")
        .and_then(|c| c.as_sequence())
        .expect("changed_in sequence");

    // 3.0, 4.0, 5.0 exact strings: "1310 nm FP", "Copper cable unequalized", "Copper cable passive equalized"
    for rev in ["3.0", "4.0", "5.0"] {
        let block = changed_in
            .iter()
            .find(|b| b.get("rev").and_then(|r| r.as_str()) == Some(rev))
            .unwrap_or_else(|| panic!("changed_in for rev {rev} not found"));
        let entries = block
            .get("enum_inline")
            .and_then(|e| e.as_sequence())
            .unwrap();

        let e_03 = find_enum_entry(entries, "3");
        assert_eq!(
            e_03.get("description").and_then(|d| d.as_str()),
            Some("1310 nm FP"),
            "rev {rev} 0x03 must be '1310 nm FP'"
        );

        let e_0a = find_enum_entry(entries, "10");
        assert_eq!(
            e_0a.get("description").and_then(|d| d.as_str()),
            Some("Copper cable unequalized"),
            "rev {rev} 0x0a must be 'Copper cable unequalized'"
        );

        let e_0b = find_enum_entry(entries, "11");
        assert_eq!(
            e_0b.get("description").and_then(|d| d.as_str()),
            Some("Copper cable passive equalized"),
            "rev {rev} 0x0b must be 'Copper cable passive equalized'"
        );
    }

    // 5.3 and base block keep: "1310 nm FP laser", "Copper cable, passive, unequalized", "Copper cable, passive, equalized"
    let block_53 = changed_in
        .iter()
        .find(|b| b.get("rev").and_then(|r| r.as_str()) == Some("5.3"))
        .unwrap();
    let entries_53 = block_53
        .get("enum_inline")
        .and_then(|e| e.as_sequence())
        .unwrap();
    assert_eq!(
        find_enum_entry(entries_53, "3")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("1310 nm FP laser")
    );
    assert_eq!(
        find_enum_entry(entries_53, "10")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("Copper cable, passive, unequalized")
    );
    assert_eq!(
        find_enum_entry(entries_53, "11")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("Copper cable, passive, equalized")
    );

    let base_entries = mit
        .get("enum_inline")
        .and_then(|e| e.as_sequence())
        .unwrap();
    assert_eq!(
        find_enum_entry(base_entries, "3")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("1310 nm FP laser")
    );
    assert_eq!(
        find_enum_entry(base_entries, "10")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("Copper cable, passive, unequalized")
    );
    assert_eq!(
        find_enum_entry(base_entries, "11")
            .get("description")
            .and_then(|d| d.as_str()),
        Some("Copper cable, passive, equalized")
    );
}

#[test]
fn cmis_root_index_cdb_commands_completeness() {
    let root_index = load_yaml("cmis/_index.yaml");
    let cdb_index = load_yaml("cmis/cdb-commands/_index.yaml");

    let root_cmds: Vec<String> = root_index
        .get("cdb_commands")
        .and_then(|c| c.as_sequence())
        .expect("cdb_commands in root index")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    let cdb_cmds: Vec<String> = cdb_index
        .get("cdb_commands")
        .and_then(|c| c.as_sequence())
        .expect("cdb_commands in cdb-commands index")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // 16 through the CMIS 5.3 baseline; 33 with the stage-2 CMIS 5.4 CDB
    // modeling (0005h/0006h/0050h/0051h/0109h/010Bh/010Ch/010Dh/0200h/
    // 0212h-0217h/0233h/0281h, CDB-01..12).
    assert_eq!(
        root_cmds.len(),
        33,
        "root _index.yaml must list exactly 33 CDB commands"
    );
    assert_eq!(
        cdb_cmds.len(),
        33,
        "cdb-commands _index.yaml must list exactly 33 CDB commands"
    );

    // Normalized comparison
    let normalized_root: Vec<String> = root_cmds
        .iter()
        .map(|c| c.strip_prefix("cdb-commands/").unwrap_or(c).to_string())
        .collect();

    assert_eq!(
        normalized_root, cdb_cmds,
        "root index cdb_commands and cdb-commands index cdb_commands must match exactly"
    );
}
