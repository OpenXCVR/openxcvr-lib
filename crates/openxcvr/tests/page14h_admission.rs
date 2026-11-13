//! Stage 1 supplied-data admission tests: Page 14h selector-content gating,
//! CMIS reported-revision admission, and SFF-8636/8472 presence-prerequisite
//! gating on the generic observation paths.
//!
//! Two byte sources, labelled:
//! - `credo capture`: `tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec`
//!   (a real capture; used to prove admission BEHAVIOR — which outcomes the
//!   decoders produce — not the provenance of any acquisition assertion; see
//!   R3-A4).
//! - `synthetic`: bytes built in this test, never claimed to be a capture.
use openxcvr::catalog::{resolve, FieldQuery, LaneSelection};
use openxcvr::observation::{
    field_read_plan, field_read_prelude, field_read_remaining, observe_field, observe_field_with,
    observe_fields, observe_fields_with, AcquisitionContext, FieldObservation, ObservationState,
    ObserveOptions, PlannedRead, SelectorAcquisition,
};
use openxcvr_core::cmis_f16::CmisF16;
use openxcvr_core::field_catalog::{FieldKind, FieldPresence, Scaling, Unit};
use openxcvr_core::observation as core_observation;
use openxcvr_core::observation::FieldValue;
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp, Range,
    RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(2);

fn stamp() -> ObservationStamp {
    ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    }
}

/// `parts` are (address, bank, page, offset, bytes) — page-window offsets.
fn snapshot(parts: &[(u8, u8, u8, u8, Vec<u8>)]) -> Snapshot {
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: parts
            .iter()
            .map(
                |&(address, bank, page, offset, ref bytes)| RangeObservation {
                    range: Range {
                        address,
                        bank,
                        page,
                        offset,
                        len: bytes.len() as u16,
                    },
                    outcome: RangeOutcome::Bytes(bytes.clone()),
                    stamp: stamp(),
                    requester: ReadRequester::PassiveInspection,
                },
            )
            .collect(),
    }
}

fn fixture_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The credo capture's lower memory + page 14h windows (real capture bytes).
fn credo_page14h_snapshot() -> (Vec<u8>, Vec<u8>, Snapshot) {
    let root = fixture_root();
    let lower = std::fs::read(
        root.join("tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec.lower.bin"),
    )
    .expect("credo lower fixture");
    let page14 = std::fs::read(
        root.join("tests/fixtures/cmis/fboss-cmis-5-1-osfp-credo-800g-aec.page-14h.bin"),
    )
    .expect("credo page-14h fixture");
    assert_eq!(lower[1], 0x51, "capture reports CMIS 5.1");
    assert_eq!(page14[0], 0x06, "capture selector is 06h (SNR view)");
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, lower.clone()),
        (0x50, 0, 0x14, 128, page14.clone()),
    ]);
    (lower, page14, snap)
}

/// The 22 selector-multiplexed Diagnostics Data fields of Page 14h.
const PAGE_14H_DATA_FIELDS: &[&str] = &[
    "host_side_ber",
    "media_side_ber",
    "host_side_error_count_lane_1_4",
    "host_side_total_bits_count_lane_1_4",
    "host_side_error_count_lane_5_8",
    "host_side_total_bits_count_lane_5_8",
    "media_side_error_count_lane_1_4",
    "media_side_total_bits_count_lane_1_4",
    "media_side_error_count_lane_5_8",
    "media_side_total_bits_count_lane_5_8",
    "host_side_snr",
    "media_side_snr",
    "gated_host_side_ber",
    "gated_media_side_ber",
    "gated_host_side_error_count_lane_1_4",
    "gated_host_side_total_bits_count_lane_1_4",
    "gated_host_side_error_count_lane_5_8",
    "gated_host_side_total_bits_count_lane_5_8",
    "gated_media_side_error_count_lane_1_4",
    "gated_media_side_total_bits_count_lane_1_4",
    "gated_media_side_error_count_lane_5_8",
    "gated_media_side_total_bits_count_lane_5_8",
];

fn resolve_page14h(name: &str, rev: Rev) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::All,
        page: Some("14h"),
        bank: 0,
    };
    resolve(rev, &query).unwrap_or_else(|e| panic!("resolve {name} at {rev:?}: {e:?}"))
}

fn observe_all(
    snap: &Snapshot,
    entries: &[openxcvr::catalog::FieldEntry<'static>],
    rev: Rev,
) -> Vec<FieldObservation> {
    observe_fields(
        Family::Cmis,
        rev,
        entries,
        snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .expect("observe_fields")
}

// ─── RED-1: behavior correction on the current API ──────────────────────

/// Credo capture, selector 06h, no acquisition evidence: every one of the
/// 22 selector-multiplexed rows must be NotSupplied (missing evidence comes
/// first — R3 §2/§5), never a Valid value from a view the selector did not
/// select.
#[test]
fn credo_capture_without_evidence_all_22_page14h_rows_not_supplied() {
    let (_, _, snap) = credo_page14h_snapshot();
    for name in PAGE_14H_DATA_FIELDS {
        let entries = resolve_page14h(name, Rev::CMIS_5_1);
        let observations = observe_all(&snap, &entries, Rev::CMIS_5_1);
        assert!(!observations.is_empty(), "{name} resolved no entries");
        for observation in &observations {
            assert_eq!(
                observation.state,
                ObservationState::NotSupplied,
                "{name} lane {:?} under selector 06h without evidence: {:?}",
                observation.lane,
                observation
            );
            assert!(observation.value.is_none(), "{name}: {:?}", observation);
        }
    }
}

/// The reproduced defect, as a single-field assertion: media_side_ber decodes
/// Valid today under selector 06h; admission must refuse it instead.
#[test]
fn credo_capture_media_side_ber_is_not_valid_under_snr_view() {
    let (_, _, snap) = credo_page14h_snapshot();
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    let observations = observe_all(&snap, &entries, Rev::CMIS_5_1);
    assert_eq!(observations.len(), 8);
    for observation in &observations {
        assert_ne!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

/// Page 14h exists from CMIS 4.0 (Tables 8-92/8-94); resolution at 3.0
/// must refuse instead of returning entries.
#[test]
fn page14h_resolution_refuses_at_cmis_3_0() {
    let query = FieldQuery {
        pattern: "media_side_ber",
        lane: LaneSelection::All,
        page: Some("14h"),
        bank: 0,
    };
    let resolved = resolve(Rev::CMIS_3_0, &query);
    assert!(
        resolved.is_err(),
        "resolve(CMIS_3_0, page 14h) must refuse, got Ok({resolved:?})"
    );
}

/// The CMIS revision byte (lower memory byte 1) is a decode prerequisite for
/// every CMIS row: absent → NotSupplied.
#[test]
fn cmis_row_with_revision_byte_absent_is_not_supplied() {
    let (_, page14, _) = credo_page14h_snapshot();
    // Page 14h supplied, lower memory (revision byte) not.
    let snap = snapshot(&[(0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_all(&snap, &entries, Rev::CMIS_5_1) {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "revision byte absent: {observation:?}"
        );
        assert!(observation.value.is_none());
    }
}

/// The snapshot's revision byte names a different revision than the entries:
/// UnsupportedRevision carrying the reported revision, no value.
#[test]
fn cmis_row_with_revision_byte_0x30_at_5_1_is_unsupported_revision() {
    let (mut lower, page14, _) = credo_page14h_snapshot();
    lower[1] = 0x30; // reports CMIS 3.0
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_all(&snap, &entries, Rev::CMIS_5_1) {
        assert_eq!(
            observation.state,
            ObservationState::UnsupportedRevision(Rev::CMIS_3_0),
            "revision byte 0x30 with 5.1 entries: {observation:?}"
        );
        assert!(observation.value.is_none());
    }
}

/// An unknown revision code (0xff) is UnsupportedRevision naming the
/// reported (unknown) revision, not a decode.
#[test]
fn cmis_row_with_unknown_revision_byte_is_unsupported_revision() {
    let (mut lower, page14, _) = credo_page14h_snapshot();
    lower[1] = 0xff;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_all(&snap, &entries, Rev::CMIS_5_1) {
        assert_eq!(
            observation.state,
            ObservationState::UnsupportedRevision(Rev {
                family: Family::Cmis,
                major: 15,
                minor: 15,
            }),
            "unknown revision byte: {observation:?}"
        );
        assert!(observation.value.is_none());
    }
}

/// The core decoder itself honours the reported-revision prerequisite (B3:
/// enforcement lives in core, not only the facade).
#[test]
fn core_observe_field_honours_revision_admission() {
    let (_, page14, _) = credo_page14h_snapshot();
    // No lower memory → revision byte absent → NotSupplied out of core.
    let snap = snapshot(&[(0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for entry in &entries {
        let observation = core_observation::observe_field(
            entry,
            &snap,
            GENERATION,
            EPOCH,
            &ObserveOptions::default(),
        );
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "core-direct: {observation:?}"
        );
    }
    // Lower memory naming 3.0 while the entry is 5.1 → UnsupportedRevision.
    let (mut lower, page14b, _) = credo_page14h_snapshot();
    lower[1] = 0x30;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14b)]);
    for entry in &entries {
        let observation = core_observation::observe_field(
            entry,
            &snap,
            GENERATION,
            EPOCH,
            &ObserveOptions::default(),
        );
        assert_eq!(
            observation.state,
            ObservationState::UnsupportedRevision(Rev::CMIS_3_0),
            "core-direct: {observation:?}"
        );
    }
}

/// The single-field facade path cannot bypass admission either.
#[test]
fn facade_observe_field_single_path_honours_admission() {
    let (_, _, snap) = credo_page14h_snapshot();
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    let observation = observe_field(
        &entries[0],
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert!(observation.value.is_none());
}

/// The field read plan carries the decode's new dependencies: the CMIS
/// revision byte (lower memory byte 1) and the selector prerequisite byte
/// (page 14h byte 128) alongside the row's own data bytes. Flat-superset
/// planning only — no ordering claim.
#[test]
fn field_read_plan_names_revision_and_prerequisite_bytes() {
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    let plan = field_read_plan(Family::Cmis, Rev::CMIS_5_1, &entries).expect("plan");
    let covers = |address: u8, page: u8, offset: u8, len: u16| {
        plan.reads.iter().any(|read| {
            read.range.address == address
                && read.range.page == page
                && read.range.bank == 0
                && read.range.offset <= offset
                && read.range.end() >= u32::from(offset) + u32::from(len)
        })
    };
    assert!(
        covers(0x50, 0x00, 1, 1),
        "plan must include the CMIS revision byte (lower memory byte 1): {:?}",
        plan.reads
    );
    assert!(
        covers(0x50, 0x14, 128, 1),
        "plan must include the selector prerequisite byte (page 14h byte 128): {:?}",
        plan.reads
    );
    assert!(
        covers(0x50, 0x14, 208, 16),
        "plan must include the row's own data bytes: {:?}",
        plan.reads
    );
}

// ─── SFF-8472 presence-prerequisite gating (behaviour change, disclosed) ──

/// SFF-8472 A0h byte 94 = 0x0a admits a decode at 12.5.
fn sff8472_compliance_range() -> (u8, u8, u8, u8, Vec<u8>) {
    // A0h window: byte 94 compliance = 0x0a; byte 92 bit 4
    // externally_calibrated (caller sets the byte).
    let mut a0 = vec![0u8; 96];
    a0[94] = 0x0a;
    (0x50, 0, 0x00, 0, a0)
}

fn resolve_sff8472(name: &str, page: &str) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::Unspecified,
        page: Some(page),
        bank: 0,
    };
    resolve(
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        &query,
    )
    .unwrap_or_else(|e| panic!("resolve {name}: {e:?}"))
}

fn observe_sff8472(
    snap: &Snapshot,
    entries: &[openxcvr::catalog::FieldEntry<'static>],
) -> Vec<FieldObservation> {
    observe_fields(
        Family::Sff8472,
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        entries,
        snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .expect("observe_fields")
}

/// optional_laser_temperature is present only when externally_calibrated
/// reads 0: mismatch → Invalid, never a Valid wavelength/temperature read.
#[test]
fn sff8472_optional_laser_temperature_gated_by_externally_calibrated() {
    let a2_bytes: Vec<u8> = vec![0x12, 0x34]; // at A2h offset 106
    let mut compliance = sff8472_compliance_range();
    let entries = resolve_sff8472("optional_laser_temperature", "page_a2h_diagnostics_base");

    // externally_calibrated == 0 → Valid (matches the SFF-8472 case in R3 §5).
    compliance.4[92] = 0x00;
    let snap = snapshot(&[compliance.clone(), (0x51, 0, 0x00, 106, a2_bytes.clone())]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }

    // externally_calibrated == 1 → Invalid, no value.
    compliance.4[92] = 0x10;
    let snap = snapshot(&[compliance.clone(), (0x51, 0, 0x00, 106, a2_bytes.clone())]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }

    // The prerequisite byte itself absent → NotSupplied (field bytes are
    // supplied and the compliance byte (94) admits the decode, so only the
    // presence admission can produce this).
    let snap = snapshot(&[
        (0x50, 0, 0x00, 94, vec![0x0a]),
        (0x51, 0, 0x00, 106, a2_bytes),
    ]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

/// format_id is a two-byte big-endian enum prerequisite: fields gated on
/// `format_id == 0xca1b` must follow it (2-byte extraction, SFF-8472 §13).
#[test]
fn sff8472_format_id_gated_field_follows_two_byte_enum_prerequisite() {
    let mut compliance = sff8472_compliance_range();
    compliance.4[92] = 0x00;
    let entries = resolve_sff8472(
        "optical_number_of_lanes",
        "page_a2h_03h_high_accuracy_timing",
    );
    let lane_byte: Vec<u8> = vec![0x01]; // A2h page 03h offset 150

    // format_id == 0xca1b → Valid.
    let format_ca1b: Vec<u8> = vec![0xca, 0x1b];
    let snap = snapshot(&[
        compliance.clone(),
        (0x51, 0, 0x03, 128, format_ca1b.clone()),
        (0x51, 0, 0x03, 150, lane_byte.clone()),
    ]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }

    // format_id == 0x100b (the loopback layout) → Invalid, no value.
    let format_100b: Vec<u8> = vec![0x10, 0x0b];
    let snap = snapshot(&[
        compliance.clone(),
        (0x51, 0, 0x03, 128, format_100b),
        (0x51, 0, 0x03, 150, lane_byte.clone()),
    ]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }

    // A byte that matches neither side of the 16-bit comparison must not
    // decode as a match (extraction is the full two-byte big-endian code).
    let format_001b: Vec<u8> = vec![0x00, 0x1b];
    let snap = snapshot(&[
        compliance.clone(),
        (0x51, 0, 0x03, 128, format_001b),
        (0x51, 0, 0x03, 150, lane_byte),
    ]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
    }

    // Prerequisite bytes absent → NotSupplied.
    let snap = snapshot(&[compliance, (0x51, 0, 0x03, 150, vec![0x01])]);
    for observation in observe_sff8472(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
    }
}

// ─── SFF-8636 presence-prerequisite gating (behaviour change, disclosed) ──

const SFF8636_REV: Rev = Rev {
    family: Family::Sff8636,
    major: 2,
    minor: 12,
};

/// SFF-8636 lower + upper page 00h window (256 bytes, page 0): byte 1
/// compliance 09h (Rev 2.11); byte 194 is the option-control bitfield.
fn sff8636_window(compliance: u8, option_byte_194: u8) -> Vec<u8> {
    let mut window = vec![0u8; 256];
    window[1] = compliance;
    window[194] = option_byte_194;
    window
}

fn resolve_sff8636(name: &str) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::All,
        page: Some("03h"),
        bank: 0,
    };
    resolve(SFF8636_REV, &query).unwrap_or_else(|e| panic!("resolve {name}: {e:?}"))
}

fn observe_sff8636(
    snap: &Snapshot,
    entries: &[openxcvr::catalog::FieldEntry<'static>],
) -> Vec<FieldObservation> {
    observe_fields(
        Family::Sff8636,
        SFF8636_REV,
        entries,
        snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .expect("observe_fields")
}

/// rx_squelch_disable_lane is gated by
/// option_control_or_flag_support[rx_squelch_disable] (enum_inline value 4 →
/// page 00h byte 194 bit 3): capability clear → Invalid; set → Valid; the
/// capability byte absent → NotSupplied.
#[test]
fn sff8636_enum_inline_prerequisite_gates_rx_squelch_disable() {
    let entries = resolve_sff8636("rx_squelch_disable_lane");
    assert_eq!(entries.len(), 4, "lanes 1-4");
    let page03: Vec<u8> = vec![0x00; 128]; // page 03h window (byte 240 = index 112)

    // Capability bit set (byte 194 bit 3) → Valid.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, sff8636_window(0x0a, 0x08)),
        (0x50, 0, 0x03, 128, page03.clone()),
    ]);
    for observation in observe_sff8636(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }

    // Capability bit clear → Invalid, no value.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, sff8636_window(0x0a, 0x00)),
        (0x50, 0, 0x03, 128, page03.clone()),
    ]);
    for observation in observe_sff8636(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }

    // Capability byte not supplied → NotSupplied (the compliance byte
    // (byte 1) is supplied and admits the decode; only the presence
    // admission can produce this).
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x0a]), (0x50, 0, 0x03, 128, page03)]);
    for observation in observe_sff8636(&snap, &entries) {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

// ─── Selector-acquisition evidence (B2), through the context-taking API ──

/// A caller-asserted acquisition record for the Page 14h selector. The
/// caller asserts that, for that generation/epoch/bank, the selector value
/// was in force, writes were serialized with the read, and the data_range
/// read began after the revision's content-switch bound. Caller-asserted
/// provenance: the library performs no write, wait or timing check.
fn page14h_acquisition(selector_value: u8, data_offset: u8, data_len: u16) -> SelectorAcquisition {
    SelectorAcquisition {
        generation: GENERATION,
        epoch: EPOCH,
        address: 0x50,
        bank: 0,
        page: 0x14,
        selector_offset: 128,
        selector_value,
        data_range: Range {
            address: 0x50,
            bank: 0,
            page: 0x14,
            offset: data_offset,
            len: data_len,
        },
    }
}

fn ctx(acquisitions: &[SelectorAcquisition]) -> AcquisitionContext<'_> {
    AcquisitionContext { acquisitions }
}

/// Credo capture, selector 06h, evidence matching the snapshot (06h,
/// covering bytes 192-255): the SNR rows the selector actually selects
/// decode Valid with the capture's values; every BER/counter row is
/// Invalid (view not selected), never a value.
#[test]
fn credo_capture_with_matching_evidence_snrs_valid_others_invalid() {
    let (_, _, snap) = credo_page14h_snapshot();
    let evidence = [page14h_acquisition(0x06, 192, 64)];
    let options = ObserveOptions::default();
    for name in PAGE_14H_DATA_FIELDS {
        let entries = resolve_page14h(name, Rev::CMIS_5_1);
        let observations = observe_fields_with(
            Family::Cmis,
            Rev::CMIS_5_1,
            &entries,
            &snap,
            GENERATION,
            EPOCH,
            &options,
            &ctx(&evidence),
        )
        .expect("observe_fields_with");
        let is_snr = *name == "host_side_snr" || *name == "media_side_snr";
        for observation in &observations {
            if is_snr {
                assert_eq!(
                    observation.state,
                    ObservationState::Valid,
                    "{name} lane {:?}: {:?}",
                    observation.lane,
                    observation
                );
            } else {
                assert_eq!(
                    observation.state,
                    ObservationState::Invalid,
                    "{name} lane {:?}: {:?}",
                    observation.lane,
                    observation
                );
                assert!(observation.value.is_none(), "{name}: {observation:?}");
            }
        }
    }
    // The capture's lane-1 SNR values, transcribed independently of any
    // decoder run (16-bit little-endian 1/256 dB words at offsets 208/240).
    let entries = resolve_page14h("host_side_snr", Rev::CMIS_5_1);
    let observations = observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with");
    match &observations[0].value {
        Some(openxcvr_core::observation::FieldValue::Scaled { value, unit }) => {
            assert!((value - 8.97265625).abs() < 1e-12, "host lane1 {value}");
            assert_eq!(*unit, openxcvr_core::field_catalog::Unit::Decibel);
        }
        other => panic!("host lane1: {other:?}"),
    }
    let entries = resolve_page14h("media_side_snr", Rev::CMIS_5_1);
    let observations = observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with");
    match &observations[0].value {
        Some(openxcvr_core::observation::FieldValue::Scaled { value, .. }) => {
            assert!((value - 8.77734375).abs() < 1e-12, "media lane1 {value}");
        }
        other => panic!("media lane1: {other:?}"),
    }
}

/// Evidence claiming selector 01h on a snapshot whose selector byte is 06h:
/// the evidence contradicts the snapshot → Invalid, no value.
#[test]
fn credo_capture_with_contradictory_evidence_invalid() {
    let (_, _, snap) = credo_page14h_snapshot();
    let evidence = [page14h_acquisition(0x01, 192, 64)];
    let entries = resolve_page14h("host_side_snr", Rev::CMIS_5_1);
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

/// Synthetic bytes (the credo page-14h window with the selector forced to
/// 01h — labelled synthetic: these are the capture's SNR-view bytes forced
/// through the BER view to exercise admission, not BER measurements): BER
/// rows decode Valid F16 ratio values with matching evidence. Stage 2
/// interprets the F16 code as a ratio; no raw code is reported in Rust.
#[test]
fn synthetic_selector_01h_with_matching_evidence_ber_rows_valid_scaled() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    page14[0] = 0x01;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let evidence = [page14h_acquisition(0x01, 192, 64)];
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    let observations = observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    )
    .expect("observe_fields_with");
    assert_eq!(observations.len(), 8);
    // Raw big-endian codes transcribed from the window (offsets
    // 208+2(lane-1)); the values are those codes interpreted as F16.
    let codes: [u16; 8] = [63752, 47624, 7944, 35080, 63496, 521, 18697, 47624];
    for (observation, code) in observations.iter().zip(codes) {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
        assert_eq!(
            observation.value,
            Some(FieldValue::Scaled {
                value: CmisF16::from_raw(code).to_f64(),
                unit: Unit::Ratio,
            })
        );
    }
    // The gated (11h) rows stay Invalid: the selector selects the running
    // BER view, not the gated view.
    let entries = resolve_page14h("gated_media_side_ber", Rev::CMIS_5_1);
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::Invalid,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

/// Selector values that do not select the requested view are Invalid with
/// matching evidence: 00h (zero-filled view), 07h (reserved), 16h
/// (reserved), C0h (custom). A zero-filled window must not become BER zero.
#[test]
fn synthetic_non_selecting_selector_values_are_invalid() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for selector in [0x00u8, 0x07, 0x16, 0xc0] {
        page14[0] = selector;
        let snap = snapshot(&[
            (0x50, 0, 0x00, 0, lower.clone()),
            (0x50, 0, 0x14, 128, page14.clone()),
        ]);
        let evidence = [page14h_acquisition(selector, 192, 64)];
        for observation in observe_fields_with(
            Family::Cmis,
            Rev::CMIS_5_1,
            &entries,
            &snap,
            GENERATION,
            EPOCH,
            &ObserveOptions::default(),
            &ctx(&evidence),
        )
        .expect("observe_fields_with")
        {
            assert_eq!(
                observation.state,
                ObservationState::Invalid,
                "selector 0x{selector:02x}: {observation:?}"
            );
            assert!(
                observation.value.is_none(),
                "selector 0x{selector:02x}: {observation:?}"
            );
        }
    }
}

/// Evidence present but the snapshot's selector byte absent: NotSupplied
/// (nothing to cross-check the assertion against).
#[test]
fn synthetic_selector_byte_absent_with_evidence_not_supplied() {
    let (lower, page14, _) = credo_page14h_snapshot();
    // Supply the data window but not byte 128.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, lower),
        (0x50, 0, 0x14, 129, page14[1..].to_vec()),
    ]);
    let evidence = [page14h_acquisition(0x01, 192, 64)];
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
        assert!(observation.value.is_none(), "{observation:?}");
    }
}

/// The evidence's data_range must cover the row's own bytes: an assertion
/// covering only bytes 192-207 admits host_side_ber (192-207) and refuses
/// media_side_ber (208-223) as NotSupplied.
#[test]
fn evidence_data_range_must_cover_the_row_bytes() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    page14[0] = 0x01;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let evidence = [page14h_acquisition(0x01, 192, 16)];
    let options = ObserveOptions::default();
    let entries = resolve_page14h("host_side_ber", Rev::CMIS_5_1);
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
    }
}

/// Evidence from another generation or epoch does not match: NotSupplied.
#[test]
fn evidence_from_other_generation_or_epoch_does_not_match() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    page14[0] = 0x01;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for (generation, epoch) in [(ModuleGeneration(9), EPOCH), (GENERATION, CacheEpoch(9))] {
        let evidence = [SelectorAcquisition {
            generation,
            epoch,
            ..page14h_acquisition(0x01, 192, 64)
        }];
        for observation in observe_fields_with(
            Family::Cmis,
            Rev::CMIS_5_1,
            &entries,
            &snap,
            GENERATION,
            EPOCH,
            &ObserveOptions::default(),
            &ctx(&evidence),
        )
        .expect("observe_fields_with")
        {
            assert_eq!(
                observation.state,
                ObservationState::NotSupplied,
                "{observation:?}"
            );
        }
    }
}

/// Page 14h is banked: the prerequisite and evidence follow the entry's
/// bank. Lower memory (revision byte) is unbanked and read at bank 0.
#[test]
fn banked_page14h_evidence_follows_the_entry_bank() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    page14[0] = 0x01;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 1, 0x14, 128, page14)]);
    let query = FieldQuery {
        pattern: "media_side_ber",
        lane: LaneSelection::All,
        page: Some("14h"),
        bank: 1,
    };
    let entries = resolve(Rev::CMIS_5_1, &query).expect("resolve bank 1");
    let options = ObserveOptions::default();

    // Evidence naming bank 0 does not cover a bank-1 read.
    let evidence = [page14h_acquisition(0x01, 192, 64)];
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
    }

    // Evidence naming bank 1 does.
    let evidence = [SelectorAcquisition {
        bank: 1,
        data_range: Range {
            bank: 1,
            ..page14h_acquisition(0x01, 192, 64).data_range
        },
        ..page14h_acquisition(0x01, 192, 64)
    }];
    for observation in observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }
}

/// The bulk context-taking path agrees with the single-field one over all
/// 22 fields at once.
#[test]
fn bulk_observe_fields_with_matches_single_field_outcomes() {
    let (_, _, snap) = credo_page14h_snapshot();
    let evidence = [page14h_acquisition(0x06, 192, 64)];
    let options = ObserveOptions::default();
    let mut entries = Vec::new();
    for name in PAGE_14H_DATA_FIELDS {
        entries.extend(resolve_page14h(name, Rev::CMIS_5_1));
    }
    let bulk = observe_fields_with(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &options,
        &ctx(&evidence),
    )
    .expect("observe_fields_with");
    assert_eq!(bulk.len(), entries.len());
    for (entry, observation) in entries.iter().zip(&bulk) {
        let single = observe_field_with(entry, &snap, GENERATION, EPOCH, &options, &ctx(&evidence));
        assert_eq!(&single, observation, "entry {}", entry.row.name);
    }
}

/// Existing entry points behave exactly as with an empty context: a
/// content-multiplexed row with a matching snapshot selector but no
/// evidence stays NotSupplied through observe_field / observe_fields.
#[test]
fn existing_entry_points_behave_as_with_empty_context() {
    let (lower, mut page14, _) = credo_page14h_snapshot();
    page14[0] = 0x01;
    let snap = snapshot(&[(0x50, 0, 0x00, 0, lower), (0x50, 0, 0x14, 128, page14)]);
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    for observation in observe_all(&snap, &entries, Rev::CMIS_5_1) {
        assert_eq!(
            observation.state,
            ObservationState::NotSupplied,
            "{observation:?}"
        );
    }
    let observation = observe_field(
        &entries[0],
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    // And the explicit empty context is the same thing.
    let observation = observe_field_with(
        &entries[0],
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext { acquisitions: &[] },
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
}

/// The core decoder honours acquisition evidence directly (B2 enforcement
/// lives in core, not only the facade).
#[test]
fn core_observe_field_with_honours_evidence() {
    let (_, _, snap) = credo_page14h_snapshot();
    let entries = resolve_page14h("host_side_snr", Rev::CMIS_5_1);
    let evidence = [page14h_acquisition(0x06, 192, 64)];
    let observation = core_observation::observe_field_with(
        &entries[0],
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    );
    assert_eq!(
        observation.state,
        ObservationState::Valid,
        "{observation:?}"
    );
    // Without evidence, the same core path refuses.
    let observation = core_observation::observe_field_with(
        &entries[0],
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext { acquisitions: &[] },
    );
    assert_eq!(
        observation.state,
        ObservationState::NotSupplied,
        "{observation:?}"
    );
}

/// Acquisition evidence only gates content-multiplexed rows: a plain
/// Equals prerequisite (an SFF-8472 capability bit) needs no evidence, and
/// supplying an unrelated assertion changes nothing.
#[test]
fn acquisition_evidence_does_not_gate_plain_prerequisite_rows() {
    let a2_bytes: Vec<u8> = vec![0x12, 0x34];
    let mut compliance = sff8472_compliance_range();
    compliance.4[92] = 0x00; // externally_calibrated == 0
    let entries = resolve_sff8472("optional_laser_temperature", "page_a2h_diagnostics_base");
    let unrelated = [SelectorAcquisition {
        generation: GENERATION,
        epoch: EPOCH,
        address: 0x51,
        bank: 0,
        page: 0x00,
        selector_offset: 106,
        selector_value: 0x12,
        data_range: Range {
            address: 0x51,
            bank: 0,
            page: 0x00,
            offset: 106,
            len: 2,
        },
    }];
    let snap = snapshot(&[compliance, (0x51, 0, 0x00, 106, a2_bytes)]);
    for observation in observe_fields_with(
        Family::Sff8472,
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        &entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&unrelated),
    )
    .expect("observe_fields_with")
    {
        assert_eq!(
            observation.state,
            ObservationState::Valid,
            "{observation:?}"
        );
    }
}

// ─── Stage 2: interpreted F16 BER values (ratio) ─────────────────────────
//
// Synthetic pages only: a lower page naming the decode revision and a
// Page 14h window whose selector picks the decoded view. The lane codes
// and expected binary64 bit patterns are the slice-1 goldens, transcribed
// here independently of any decoder run.

/// The eight lane codes the synthetic BER views carry (lanes 1..8).
const F16_LANE_CODES: [u16; 8] = [
    0x0000, 0xC001, 0xFFFF, 0x6001, 0x0801, 0x07FF, 0xF800, 0xC00A,
];

/// The expected binary64 bit patterns of those codes (slice-1 goldens:
/// 0x0000 and 0xF800 -> 0.0, 0xC001 -> 1.0, 0xC00A -> 10.0, and 0xFFFF ->
/// 2.047e10 — a value above 1 stays a ratio, never a clamped BER).
const F16_LANE_BITS: [u64; 8] = [
    0x0000_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0x4213_106D_A600_0000,
    0x3D71_9799_812D_EA11,
    0x3B28_2DB3_4012_B251,
    0x3BA3_5557_A155_5996,
    0x0000_0000_0000_0000,
    0x4024_0000_0000_0000,
];

/// Every revision Page 14h exists for (the fields are since 4.0).
const PAGE14H_REVS: [Rev; 5] = [
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
];

/// (field, selecting selector): ungated rows select on 01h, gated rows on
/// 11h — the selectors the schemas' `present_when` names.
const BER_FIELDS: [(&str, u8); 4] = [
    ("host_side_ber", 0x01),
    ("media_side_ber", 0x01),
    ("gated_host_side_ber", 0x11),
    ("gated_media_side_ber", 0x11),
];

/// Synthetic lower page (bytes 0-127) whose byte 1 names `rev`.
fn synthetic_lower(rev: Rev) -> Vec<u8> {
    let mut lower = vec![0u8; 128];
    lower[1] = (rev.major << 4) | rev.minor;
    lower
}

/// Synthetic Page 14h window (bytes 128-255): `selector` at byte 128 and
/// the eight lane codes in both BER views (host 192-207, media 208-223 —
/// the ungated and gated rows share the offsets; the selector decides
/// which view the bytes carry).
fn synthetic_page14h(selector: u8) -> Vec<u8> {
    let mut page14 = vec![0u8; 128];
    page14[0] = selector;
    for (lane, code) in F16_LANE_CODES.iter().enumerate() {
        let code = code.to_be_bytes();
        let host = 64 + 2 * lane; // window 192 + 2*(lane-1)
        page14[host] = code[0];
        page14[host + 1] = code[1];
        let media = 80 + 2 * lane; // window 208 + 2*(lane-1)
        page14[media] = code[0];
        page14[media + 1] = code[1];
    }
    page14
}

/// Observe resolved entries against a synthetic (lower, page 14h) snapshot
/// with matching acquisition evidence for `selector`.
fn observe_page14h(
    lower: &[u8],
    page14: &[u8],
    selector: u8,
    entries: &[openxcvr::catalog::FieldEntry<'_>],
    rev: Rev,
) -> Vec<FieldObservation> {
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, lower.to_vec()),
        (0x50, 0, 0x14, 128, page14.to_vec()),
    ]);
    let evidence = [page14h_acquisition(selector, 192, 64)];
    observe_fields_with(
        Family::Cmis,
        rev,
        entries,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &ctx(&evidence),
    )
    .expect("observe_fields_with")
}

/// All four BER fields, at every revision Page 14h exists for, decode
/// Valid scaled ratio values with the slice-1 golden bits.
#[test]
fn all_four_ber_fields_decode_scaled_ratio_at_every_supported_revision() {
    for rev in PAGE14H_REVS {
        let lower = synthetic_lower(rev);
        for &(name, selector) in &BER_FIELDS {
            let page14 = synthetic_page14h(selector);
            let entries = resolve_page14h(name, rev);
            let observations = observe_page14h(&lower, &page14, selector, &entries, rev);
            assert_eq!(observations.len(), 8, "{name} at {rev:?}");
            for (lane, observation) in observations.iter().enumerate() {
                assert_eq!(
                    observation.state,
                    ObservationState::Valid,
                    "{name} lane {} at {rev:?}: {observation:?}",
                    lane + 1
                );
                match observation.value {
                    Some(FieldValue::Scaled { value, unit }) => {
                        assert_eq!(unit, Unit::Ratio, "{name} lane {}", lane + 1);
                        assert_eq!(
                            value.to_bits(),
                            F16_LANE_BITS[lane],
                            "{name} lane {} at {rev:?}",
                            lane + 1
                        );
                    }
                    other => panic!("{name} lane {} at {rev:?}: {other:?}", lane + 1),
                }
            }
        }
    }
}

/// A selector that selects the other BER view (11h for ungated rows, 01h
/// for gated rows) or the SNR view (06h) is Invalid with matching
/// evidence: no value, never a reinterpreted view.
#[test]
fn wrong_selector_for_each_ber_field_is_invalid() {
    let rev = Rev::CMIS_5_1;
    let lower = synthetic_lower(rev);
    for &(name, selecting) in &BER_FIELDS {
        let entries = resolve_page14h(name, rev);
        for selector in [0x11u8, 0x01, 0x06] {
            if selector == selecting {
                continue;
            }
            let page14 = synthetic_page14h(selector);
            let observations = observe_page14h(&lower, &page14, selector, &entries, rev);
            for observation in &observations {
                assert_eq!(
                    observation.state,
                    ObservationState::Invalid,
                    "{name} selector 0x{selector:02x}: {observation:?}"
                );
                assert!(observation.value.is_none(), "{name}: {observation:?}");
            }
        }
    }
}

/// A snapshot whose revision byte reports CMIS 3.0 (Page 14h does not
/// exist there) is UnsupportedRevision naming 3.0, for every BER field.
#[test]
fn ber_fields_at_reported_cmis_3_0_are_unsupported_revision() {
    let rev = Rev::CMIS_5_1;
    let mut lower = synthetic_lower(rev);
    lower[1] = 0x30; // reports CMIS 3.0
    for &(name, selector) in &BER_FIELDS {
        let page14 = synthetic_page14h(selector);
        let entries = resolve_page14h(name, rev);
        let observations = observe_page14h(&lower, &page14, selector, &entries, rev);
        for observation in &observations {
            assert_eq!(
                observation.state,
                ObservationState::UnsupportedRevision(Rev::CMIS_3_0),
                "{name}: {observation:?}"
            );
            assert!(observation.value.is_none(), "{name}: {observation:?}");
        }
    }
}

/// Hand-built rows (synthetic): F16 scaling on a row that is not F16Be, or
/// whose declared length is not two bytes, is UnsupportedField with no
/// value — the mismatch is stated, never a narrowed reinterpretation.
#[test]
fn f16_scaling_on_mismatched_hand_built_rows_is_unsupported() {
    let rev = Rev::CMIS_5_1;
    let entries = resolve_page14h("media_side_ber", rev);
    let base = &entries[0];
    // The generated row carries the F16 ratio scaling this test breaks.
    assert_eq!(
        base.row.scaling,
        Scaling::F16 { unit: Unit::Ratio },
        "media_side_ber row: {:?}",
        base.row.scaling
    );

    // A non-F16Be kind (U16Be) with the F16 scaling kept.
    let mut kind_row = base.row.clone();
    kind_row.kind = FieldKind::U16Be;
    kind_row.presence = FieldPresence::Unconditional;
    let kind_entry = openxcvr::catalog::FieldEntry {
        row: &kind_row,
        rev,
        lane: None,
        bank: 0,
        offset: base.offset,
        bit_offset: base.bit_offset,
        revision_probe: None,
    };
    let snap = snapshot(&[(0x50, 0, 0x14, 208, vec![0xC0, 0x01])]);
    let observation = core_observation::observe_field(
        &kind_entry,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(
        observation.state,
        ObservationState::UnsupportedField,
        "{observation:?}"
    );
    assert!(observation.value.is_none(), "{observation:?}");
    assert_eq!(
        observation.note,
        Some("f16 scaling requires a two-byte f16be row"),
        "{observation:?}"
    );

    // A one-byte declared length: the two-byte F16 extraction refuses
    // before any value exists.
    let mut short_row = base.row.clone();
    short_row.presence = FieldPresence::Unconditional;
    short_row.len = 1;
    let short_entry = openxcvr::catalog::FieldEntry {
        row: &short_row,
        rev,
        lane: None,
        bank: 0,
        offset: base.offset,
        bit_offset: base.bit_offset,
        revision_probe: None,
    };
    let snap = snapshot(&[(0x50, 0, 0x14, 208, vec![0xC0])]);
    let observation = core_observation::observe_field(
        &short_entry,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(
        observation.state,
        ObservationState::UnsupportedField,
        "{observation:?}"
    );
    assert!(observation.value.is_none(), "{observation:?}");
}

/// A three-byte F16Be row with F16 scaling reaches the F16 arm — the
/// two-byte extraction succeeds on the first two of the three supplied
/// bytes — so the declared-length half of the guard is what refuses it:
/// UnsupportedField, no value, the F16 note.
#[test]
fn f16_scaling_on_a_three_byte_f16be_row_is_unsupported() {
    let rev = Rev::CMIS_5_1;
    let entries = resolve_page14h("media_side_ber", rev);
    let base = &entries[0];
    // The generated row carries the F16 ratio scaling this test keeps.
    assert_eq!(
        base.row.scaling,
        Scaling::F16 { unit: Unit::Ratio },
        "media_side_ber row: {:?}",
        base.row.scaling
    );

    // A three-byte declared length with the F16Be kind kept: extraction
    // reads the first two of the three supplied bytes, so decode reaches
    // the F16 arm, whose declared-length half refuses the row shape.
    let mut long_row = base.row.clone();
    long_row.presence = FieldPresence::Unconditional;
    long_row.len = 3;
    let long_entry = openxcvr::catalog::FieldEntry {
        row: &long_row,
        rev,
        lane: None,
        bank: 0,
        offset: base.offset,
        bit_offset: base.bit_offset,
        revision_probe: None,
    };
    let snap = snapshot(&[(0x50, 0, 0x14, 208, vec![0xC0, 0x01, 0x00])]);
    let observation = core_observation::observe_field(
        &long_entry,
        &snap,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    );
    assert_eq!(
        observation.state,
        ObservationState::UnsupportedField,
        "{observation:?}"
    );
    assert!(observation.value.is_none(), "{observation:?}");
    assert_eq!(
        observation.note,
        Some("f16 scaling requires a two-byte f16be row"),
        "{observation:?}"
    );
}

/// The planners still plan a BER entry's bytes: field_read_plan,
/// field_read_prelude and field_read_remaining all return the row's bytes
/// with no refusal — F16 scaling adds no decode dependency.
#[test]
fn media_side_ber_is_planned_by_all_three_planners() {
    let rev = Rev::CMIS_5_1;
    let entries = resolve_page14h("media_side_ber", rev);
    let covers_media = |reads: &[PlannedRead]| {
        reads.iter().any(|read| {
            read.range.address == 0x50
                && read.range.bank == 0
                && read.range.page == 0x14
                && read.range.offset <= 208
                && read.range.end() >= 224
        })
    };

    // The full plan: the row's own bytes, no refusals.
    let plan = field_read_plan(Family::Cmis, rev, &entries).expect("field_read_plan");
    assert!(
        plan.refusals.is_empty(),
        "plan refusals: {:?}",
        plan.refusals
    );
    assert!(covers_media(&plan.reads), "plan reads: {:?}", plan.reads);

    // Round 1 (prelude): no refusals; the selector prerequisite byte is
    // planned.
    let prelude = field_read_prelude(Family::Cmis, rev, &entries).expect("field_read_prelude");
    assert!(
        prelude.refusals.is_empty(),
        "prelude refusals: {:?}",
        prelude.refusals
    );
    assert!(
        prelude.reads.iter().any(|read| {
            read.range.address == 0x50
                && read.range.page == 0x14
                && read.range.offset <= 128
                && read.range.end() >= 129
        }),
        "prelude must plan the selector byte: {:?}",
        prelude.reads
    );

    // Round 2 (remaining), with the prelude bytes supplied: the row's own
    // bytes are planned, no refusals (an acquisition requirement is not a
    // refusal).
    let snap = snapshot(&[
        (0x50, 0, 0x00, 0, synthetic_lower(rev)),
        (0x50, 0, 0x14, 128, synthetic_page14h(0x01)),
    ]);
    let remaining = field_read_remaining(Family::Cmis, rev, &entries, &snap, GENERATION, EPOCH)
        .expect("field_read_remaining");
    assert!(
        remaining.refusals.is_empty(),
        "remaining refusals: {:?}",
        remaining.refusals
    );
    assert!(
        covers_media(&remaining.reads),
        "remaining reads: {:?}",
        remaining.reads
    );
}
