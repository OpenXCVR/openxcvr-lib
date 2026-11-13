//! Stage 1b field read prelude/remaining tests (R3 §4, R3-A1, R3-A2):
//! which bytes the prelude acquires before any entry data, how remaining
//! refuses per entry, and what an admitted content-multiplexed entry
//! requires as acquisition evidence.
//!
//! Byte sources, labelled (as in page14h_admission.rs):
//! - `credo capture`: the real fboss CMIS 5.1 / selector-06h capture.
//! - `synthetic`: bytes built in this test, never claimed to be a capture.
use openxcvr::catalog::{
    resolve, CatalogRow, ExternalLinear, FieldEntry, FieldKind, FieldLoc, FieldQuery,
    LaneSelection, Scaling,
};
use openxcvr::observation::{
    field_read_prelude, field_read_remaining, AcquisitionRequirement, FieldAdmissionReason,
    FieldPlanError, FieldPlanRefusalReason, FieldRemainingPlan,
};
use openxcvr_core::field_catalog::{FieldPresence, RevisionProbe};
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

fn resolve_page14h(name: &str, rev: Rev) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::All,
        page: Some("14h"),
        bank: 0,
    };
    resolve(rev, &query).unwrap_or_else(|e| panic!("resolve {name} at {rev:?}: {e:?}"))
}

/// Page 12h counterpart of [`resolve_page14h`] (the tunable-laser page
/// whose per-lane flag rows carry the instance geometry P-B3 reproduces).
fn resolve_page12h(name: &str, rev: Rev) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::All,
        page: Some("12h"),
        bank: 0,
    };
    resolve(rev, &query).unwrap_or_else(|e| panic!("resolve {name} at {rev:?}: {e:?}"))
}

/// One byte of overlap is enough (the shape the existing plan tests use).
fn ranges_intersect(a: Range, b: Range) -> bool {
    a.address == b.address
        && a.bank == b.bank
        && a.page == b.page
        && u32::from(a.offset) < b.end()
        && u32::from(b.offset) < a.end()
}

/// The single acquisition requirement a plan names (test 3's shape).
fn sole_requirement(plan: &FieldRemainingPlan) -> &AcquisitionRequirement {
    assert_eq!(
        plan.acquisition_required.len(),
        1,
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
    &plan.acquisition_required[0]
}

// ─── Prelude shape ─────────────────────────────────────────────────────

/// The prelude for the two Page 14h selector-multiplexed fields plans only
/// the family gate byte (CMIS lower-memory byte 1) and the shared selector
/// prerequisite (Page 14h byte 128) — once each, no entry data bytes, and
/// nothing on Page 14h past the selector byte.
#[test]
fn prelude_lists_revision_and_selector_bytes_only() {
    let mut entries = resolve_page14h("media_side_snr", Rev::CMIS_5_1);
    entries.extend(resolve_page14h("media_side_ber", Rev::CMIS_5_1));
    assert_eq!(entries.len(), 16, "8 lanes of each field");

    let plan = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, &entries).expect("prelude");
    let gate = Range {
        address: 0x50,
        bank: 0,
        page: 0x00,
        offset: 1,
        len: 1,
    };
    let selector = Range {
        address: 0x50,
        bank: 0,
        page: 0x14,
        offset: 128,
        len: 1,
    };
    let ranges: Vec<Range> = plan.reads.iter().map(|read| read.range).collect();
    assert_eq!(ranges.len(), 2, "prelude reads: {ranges:?}");
    assert!(ranges.contains(&gate), "gate byte missing: {ranges:?}");
    assert!(
        ranges.contains(&selector),
        "selector prerequisite missing: {ranges:?}"
    );
    let page14h_data = Range {
        address: 0x50,
        bank: 0,
        page: 0x14,
        offset: 129,
        len: 127,
    };
    for range in &ranges {
        assert!(
            !ranges_intersect(*range, page14h_data),
            "prelude overlaps Page 14h bytes 129..=255: {range:?}"
        );
    }
    assert!(plan.refusals.is_empty(), "refusals: {:?}", plan.refusals);
}

// ─── Remaining on a mismatched reported revision ────────────────────────

/// Synthetic view (labelled synthetic, never claimed a capture): lower
/// byte 1 = 0x30 (reports CMIS 3.0) and the Page 14h selector 06h. Every
/// entry is refused with UnsupportedRevision before any selector-dependent
/// range is planned (R3-A2): no reads, no acquisition requirement.
#[test]
fn remaining_refuses_on_mismatched_revision_and_plans_no_dependent_range() {
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x30]),
        (0x50, 0, 0x14, 128, vec![0x06]),
    ]);
    let mut entries = resolve_page14h("media_side_snr", Rev::CMIS_5_1);
    entries.extend(resolve_page14h("media_side_ber", Rev::CMIS_5_1));
    assert_eq!(entries.len(), 16);

    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), entries.len(), "{:?}", plan.refusals);
    for entry in &entries {
        let refusal = plan
            .refusals
            .iter()
            .find(|r| r.field == entry.row.name && r.lane == entry.lane)
            .unwrap_or_else(|| panic!("no refusal for {} lane {:?}", entry.row.name, entry.lane));
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::UnsupportedRevision {
                reported: Rev::CMIS_3_0
            },
            "{} lane {:?}",
            entry.row.name,
            entry.lane
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── Remaining on the real capture: admission and evidence ─────────────

/// Credo capture (CMIS 5.1, selector 06h = SNR view), lane 1 of each
/// field: media_side_snr is admitted — its data bytes planned and one
/// acquisition requirement naming the selector — while media_side_ber
/// (the 01h view) is refused with ViewNotSelected and no read is
/// attributable to the BER entry alone.
#[test]
fn remaining_admits_snr_under_matching_view_and_requires_evidence() {
    let (_, _, snap) = credo_page14h_snapshot();
    let mut entries: Vec<_> = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .filter(|entry| entry.lane == Some(1))
        .collect();
    entries.extend(
        resolve_page14h("media_side_ber", Rev::CMIS_5_1)
            .into_iter()
            .filter(|entry| entry.lane == Some(1)),
    );
    assert_eq!(entries.len(), 2, "lane 1 of each field");
    let snr = entries[0];
    let ber = entries[1];
    assert_eq!(snr.row.name, "media_side_snr");
    assert_eq!(ber.row.name, "media_side_ber");

    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");

    // Admitted (A12): exact read identity — the admitted SNR entry's own
    // range is the one read, exactly it. The selector prerequisite at
    // Page 14h byte 128 is adjacent to the data (lane 1 begins at 240,
    // the window at 192): an overlap-shaped assertion cannot pin that
    // the prerequisite is never re-listed, because a re-listed selector
    // would coalesce with the data range into one covering read.
    assert!(
        !plan
            .refusals
            .iter()
            .any(|r| r.field == snr.row.name && r.lane == snr.lane),
        "media_side_snr lane 1 refused: {:?}",
        plan.refusals
    );
    assert_eq!(plan.reads.len(), 1, "reads: {:?}", plan.reads);
    assert_eq!(plan.reads[0].range, snr.range());

    // Refused: the view names the 06h (SNR) view; the BER row needs 01h.
    // The expected value is read from the entry's own row presence, not
    // hard-coded; the actual is the capture's selector value.
    let expected = match &ber.row.presence {
        FieldPresence::Equals(pre) => pre.expected,
        other => panic!("media_side_ber presence: {other:?}"),
    };
    assert_eq!(plan.refusals.len(), 1, "refusals: {:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!((refusal.field, refusal.lane), (ber.row.name, ber.lane));
    match &refusal.reason {
        FieldAdmissionReason::ViewNotSelected {
            expected: reported_expected,
            actual,
            ..
        } => {
            assert_eq!(*reported_expected, expected, "schema's BER selector value");
            assert_eq!(*actual, 6, "capture selector is 06h");
        }
        other => panic!("media_side_ber lane 1 refusal: {other:?}"),
    }

    // The admitted entry requires selector-acquisition evidence — with
    // its data window asserted too (A12), read from the row's own
    // content switch rather than hard-coded.
    let requirement = sole_requirement(&plan);
    assert_eq!(
        (requirement.field, requirement.lane),
        (snr.row.name, snr.lane)
    );
    assert_eq!(requirement.selector_value, 6);
    assert_eq!(
        requirement.selector_range,
        Range {
            address: 0x50,
            bank: 0,
            page: 0x14,
            offset: 128,
            len: 1
        }
    );
    let switch_data = match &snr.row.presence {
        FieldPresence::Equals(pre) => pre
            .content_switch
            .as_ref()
            .expect("media_side_snr content switch"),
        other => panic!("media_side_snr presence: {other:?}"),
    };
    assert_eq!(requirement.data_range, switch_data.data.range(0));

    // No read is attributable to the BER entry alone: the sole read is
    // exactly the admitted SNR entry's own range (asserted above).
}

// ─── Test-2 helpers for the appended Stage 1b tests ─────────────────────

/// Resolve an uninstanced field (lower-memory and A2h rows are not
/// lane-instanced; `LaneSelection::All` would refuse them).
fn resolve_field(
    rev: Rev,
    name: &str,
    page: &'static str,
) -> Vec<openxcvr::catalog::FieldEntry<'static>> {
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::Unspecified,
        page: Some(page),
        bank: 0,
    };
    resolve(rev, &query).unwrap_or_else(|e| panic!("resolve {name} at {rev:?}: {e:?}"))
}

/// The family gate byte's range for CMIS: lower-memory byte 1.
fn cmis_revision_byte() -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0x00,
        offset: 1,
        len: 1,
    }
}

// ─── (c) Remaining on a missing gate byte ─────────────────────────────

/// Synthetic view (labelled synthetic, never claimed a capture): the
/// Page 14h selector byte is present (06h) but lower-memory byte 1 —
/// the family gate the prelude acquires — is not. Rule 1 refuses every
/// entry with `PreludeMissing` naming byte 1, before any prerequisite is
/// read or any selector-dependent range is planned: no reads, no
/// acquisition requirement.
#[test]
fn remaining_refuses_every_entry_when_gate_byte_absent() {
    let snap = snapshot(&[(0x50, 0, 0x14, 128, vec![0x06])]);
    let mut entries = resolve_page14h("media_side_snr", Rev::CMIS_5_1);
    entries.extend(resolve_page14h("media_side_ber", Rev::CMIS_5_1));
    assert_eq!(entries.len(), 16, "8 lanes of each field");

    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), entries.len(), "{:?}", plan.refusals);
    for refusal in &plan.refusals {
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::PreludeMissing {
                range: cmis_revision_byte(),
            },
            "{:?}",
            refusal
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (d) BER alone under the SNR view ──────────────────────────────────

/// Credo capture (selector 06h = the SNR view), BER entries only: every
/// lane is refused `ViewNotSelected` (the BER row needs the 01h view)
/// with the row's own expected selector value and the capture's actual
/// one — and the shared-bytes rule leaves nothing planned: no admitted
/// entry needs Page 14h bytes, so none are read and no acquisition is
/// required.
#[test]
fn remaining_ber_alone_under_snr_view_plans_nothing() {
    let (_, _, snap) = credo_page14h_snapshot();
    let entries = resolve_page14h("media_side_ber", Rev::CMIS_5_1);
    assert_eq!(entries.len(), 8, "8 lanes");
    let (selector, expected) = match &entries[0].row.presence {
        FieldPresence::Equals(pre) => (pre.field, pre.expected),
        other => panic!("media_side_ber presence: {other:?}"),
    };

    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), entries.len(), "{:?}", plan.refusals);
    for (entry, refusal) in entries.iter().zip(&plan.refusals) {
        assert_eq!((refusal.field, refusal.lane), (entry.row.name, entry.lane));
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::ViewNotSelected {
                selector,
                expected,
                actual: 6,
            },
            "{} lane {:?}",
            entry.row.name,
            entry.lane
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (e) BER admitted under its own view ───────────────────────────

/// Synthetic view (labelled synthetic, never claimed a capture): CMIS
/// 5.1 revision byte and a Page 14h selector of 01h — the BER view.
/// Lane 1 of each field: `media_side_ber` is admitted — its own range
/// planned, exactly it, plus one acquisition requirement naming the 01h
/// selector and the multiplexed data window — while `media_side_snr` is
/// refused `ViewNotSelected` with the row's expected value and the
/// view's actual selector value.
#[test]
fn remaining_admits_ber_under_ber_view_and_requires_evidence() {
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x14, 128, vec![0x01]),
    ]);
    let mut entries: Vec<_> = resolve_page14h("media_side_ber", Rev::CMIS_5_1)
        .into_iter()
        .filter(|entry| entry.lane == Some(1))
        .collect();
    entries.extend(
        resolve_page14h("media_side_snr", Rev::CMIS_5_1)
            .into_iter()
            .filter(|entry| entry.lane == Some(1)),
    );
    assert_eq!(entries.len(), 2, "lane 1 of each field");
    let ber = entries[0];
    let snr = entries[1];
    assert_eq!(ber.row.name, "media_side_ber");
    assert_eq!(snr.row.name, "media_side_snr");

    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");

    // Admitted: not refused, and exactly its own range is planned — the
    // refused SNR entry contributes no range of its own.
    assert!(
        !plan
            .refusals
            .iter()
            .any(|r| r.field == ber.row.name && r.lane == ber.lane),
        "media_side_ber lane 1 refused: {:?}",
        plan.refusals
    );
    assert_eq!(plan.reads.len(), 1, "reads: {:?}", plan.reads);
    assert_eq!(plan.reads[0].range, ber.range());

    // Refused: the view's 01h selector is the BER view, not the SNR view.
    let (selector, snr_expected) = match &snr.row.presence {
        FieldPresence::Equals(pre) => (pre.field, pre.expected),
        other => panic!("media_side_snr presence: {other:?}"),
    };
    assert_eq!(plan.refusals.len(), 1, "refusals: {:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!((refusal.field, refusal.lane), (snr.row.name, snr.lane));
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::ViewNotSelected {
            selector,
            expected: snr_expected,
            actual: 1,
        }
    );

    // The admitted entry requires selector-acquisition evidence: the
    // 01h selector byte and the multiplexed data window it controls
    // (Page 14h bytes 192..=255).
    let requirement = sole_requirement(&plan);
    assert_eq!(
        (requirement.field, requirement.lane),
        (ber.row.name, ber.lane)
    );
    assert_eq!(requirement.selector_value, 1);
    assert_eq!(
        requirement.selector_range,
        Range {
            address: 0x50,
            bank: 0,
            page: 0x14,
            offset: 128,
            len: 1,
        }
    );
    assert_eq!(
        requirement.data_range,
        Range {
            address: 0x50,
            bank: 0,
            page: 0x14,
            offset: 192,
            len: 64,
        }
    );
}

// ─── (f) Selectors that select neither view ──────────────────────────

/// Synthetic views (labelled synthetic, never claimed a capture): CMIS
/// 5.1 revision byte with Page 14h selectors 00h, 07h and C0h — none of
/// them selects either row's view. Lane 1 of both fields is refused
/// `ViewNotSelected` under each: nothing is planned, no acquisition is
/// required.
#[test]
fn remaining_refuses_both_fields_under_non_selecting_selectors() {
    for selector_value in [0x00u8, 0x07, 0xc0] {
        let snap = snapshot(&[
            (0x50, 0, 0x00, 1, vec![0x51]),
            (0x50, 0, 0x14, 128, vec![selector_value]),
        ]);
        let mut entries: Vec<_> = resolve_page14h("media_side_ber", Rev::CMIS_5_1)
            .into_iter()
            .filter(|entry| entry.lane == Some(1))
            .collect();
        entries.extend(
            resolve_page14h("media_side_snr", Rev::CMIS_5_1)
                .into_iter()
                .filter(|entry| entry.lane == Some(1)),
        );
        assert_eq!(entries.len(), 2, "lane 1 of each field");

        let plan = field_read_remaining(
            Family::Cmis,
            Rev::CMIS_5_1,
            &entries,
            &snap,
            GENERATION,
            EPOCH,
        )
        .expect("field_read_remaining");
        assert_eq!(
            plan.refusals.len(),
            2,
            "selector {selector_value:02x}: {:?}",
            plan.refusals
        );
        for entry in &entries {
            let refusal = plan
                .refusals
                .iter()
                .find(|r| r.field == entry.row.name && r.lane == entry.lane)
                .unwrap_or_else(|| {
                    panic!(
                        "selector {selector_value:02x}: no refusal for {}",
                        entry.row.name
                    )
                });
            assert!(
                matches!(refusal.reason, FieldAdmissionReason::ViewNotSelected { .. }),
                "selector {selector_value:02x}: {refusal:?}"
            );
        }
        assert!(
            plan.reads.is_empty(),
            "selector {selector_value:02x}: {:?}",
            plan.reads
        );
        assert!(
            plan.acquisition_required.is_empty(),
            "selector {selector_value:02x}: {:?}",
            plan.acquisition_required
        );
    }
}

// ─── (g) SFF-8472 plain (non-multiplexed) prerequisite ──────────────

/// SFF-8472 A0h bytes 0..=95 (synthetic, labelled: never claimed a
/// capture) with the caller-chosen compliance code at byte 94 and the
/// caller-chosen `externally_calibrated` bit at byte 92 bit 4 — the two
/// prelude bytes the `optional_laser_temperature` admission reads (the
/// same bytes `sff8472_compliance_range` in page14h_admission.rs builds;
/// 0x0a is the code that admits the 12.5 schema revision).
fn sff8472_a0h(externally_calibrated: u8, compliance: u8) -> (u8, u8, u8, u8, Vec<u8>) {
    let mut a0 = vec![0u8; 96];
    a0[92] = externally_calibrated;
    a0[94] = compliance;
    (0x50, 0, 0x00, 0, a0)
}

/// `optional_laser_temperature` (A2h bytes 106-107) is present only
/// while A0h byte 92 bit 4 (`externally_calibrated`) reads 0 — a plain
/// `Equals` prerequisite, not a content switch. The prelude lists
/// exactly the compliance byte and that prerequisite byte, never the
/// field's own A2h bytes; with the bit clear the remaining round plans
/// the field's range, with it set the entry is refused
/// `PrerequisiteNotSatisfied` and nothing is planned for it.
#[test]
fn sff8472_optional_laser_temperature_prelude_and_remaining() {
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let entries = resolve_field(
        rev,
        "optional_laser_temperature",
        "page_a2h_diagnostics_base",
    );
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];

    // The prerequisite's own coordinates, read from the row (not
    // hard-coded): the schema field path and the expected raw code.
    let (prereq_field, expected) = match &entry.row.presence {
        FieldPresence::Equals(pre) => (pre.field, pre.expected),
        other => panic!("optional_laser_temperature presence: {other:?}"),
    };
    assert_eq!(expected, 0, "present while externally_calibrated reads 0");

    // Prelude: the compliance gate byte (A0h byte 94) and the
    // externally_calibrated prerequisite byte (A0h byte 92) — the
    // field's own A2h bytes are absent.
    let prelude = field_read_prelude(Family::Sff8472, rev, &entries).expect("prelude");
    let ranges: Vec<Range> = prelude.reads.iter().map(|read| read.range).collect();
    assert_eq!(
        ranges,
        vec![
            Range {
                address: 0x50,
                bank: 0,
                page: 0x00,
                offset: 92,
                len: 1,
            },
            Range {
                address: 0x50,
                bank: 0,
                page: 0x00,
                offset: 94,
                len: 1,
            },
        ],
        "prelude reads: {:?}",
        prelude.reads
    );
    assert!(prelude.refusals.is_empty(), "{:?}", prelude.refusals);

    // Remaining, externally_calibrated = 0: admitted, the field's own
    // range planned, no refusal, no acquisition requirement (no content
    // switch). The prelude bytes are all the view needs — the data bytes
    // have not been acquired yet in the two-round flow.
    let snap = snapshot(&[sff8472_a0h(0x00, 0x0a)]);
    let plan = field_read_remaining(Family::Sff8472, rev, &entries, &snap, GENERATION, EPOCH)
        .expect("field_read_remaining");
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert_eq!(plan.reads.len(), 1, "reads: {:?}", plan.reads);
    assert_eq!(plan.reads[0].range, entry.range());
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );

    // Remaining, externally_calibrated = 1: PrerequisiteNotSatisfied
    // with the row's expected value and the view's actual bit, nothing
    // planned for the field.
    let snap = snapshot(&[sff8472_a0h(0x10, 0x0a)]);
    let plan = field_read_remaining(Family::Sff8472, rev, &entries, &snap, GENERATION, EPOCH)
        .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!((refusal.field, refusal.lane), (entry.row.name, entry.lane));
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::PrerequisiteNotSatisfied {
            prerequisite: prereq_field,
            expected,
            actual: 1,
        }
    );
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (h) Unsupported presence (synthetic row, labelled) ────────────

/// A synthetic `FieldEntry` (labelled synthetic, never a schema row: no
/// generated catalog row carries `FieldPresence::Unsupported`) — a real
/// CMIS 5.1 row cloned with its presence replaced. The prelude lists
/// only the revision byte (an Unsupported entry adds no prerequisite);
/// the remaining round refuses it with `UnsupportedPresence` and plans
/// nothing.
#[test]
fn unsupported_presence_entry_refused_in_remaining_only() {
    let base = resolve_field(Rev::CMIS_5_1, "temp_mon_value", "CmisLowerMemory");
    assert_eq!(base.len(), 1);
    let mut row = base[0].row.clone();
    row.presence = FieldPresence::Unsupported {
        expr: "synthetic unsupported presence (never a generated row)",
    };
    let entry = FieldEntry {
        row: &row,
        rev: base[0].rev,
        lane: None,
        bank: 0,
        offset: row.offset,
        bit_offset: row.bit_offset,
        revision_probe: None,
    };
    let entries = [entry];

    // Prelude: only the family gate byte — the revision byte, resolved
    // from the catalog row because the hand-built entry carries no
    // revision probe of its own.
    let prelude = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, &entries).expect("prelude");
    let ranges: Vec<Range> = prelude.reads.iter().map(|read| read.range).collect();
    assert_eq!(
        ranges,
        vec![cmis_revision_byte()],
        "prelude reads: {:?}",
        prelude.reads
    );
    assert!(prelude.refusals.is_empty(), "{:?}", prelude.refusals);

    // Remaining: UnsupportedPresence, nothing planned, no requirement.
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x51])]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!(refusal.field, row.name);
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::UnsupportedPresence {
            expr: "synthetic unsupported presence (never a generated row)",
        }
    );
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (i) Positive control: unconditional lower-memory monitor ──────

/// A non-conditional CMIS 5.1 lower-memory monitor (`temp_mon_value`,
/// lower bytes 14-15) under a view reporting CMIS 5.1. The prelude is
/// the revision byte alone (an unconditional entry adds no
/// prerequisite); the remaining round plans the field's own range with
/// no refusal and no acquisition requirement.
#[test]
fn unconditional_monitor_planned_after_revision_gate() {
    let entries = resolve_field(Rev::CMIS_5_1, "temp_mon_value", "CmisLowerMemory");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert!(matches!(entry.row.presence, FieldPresence::Unconditional));

    let prelude = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, &entries).expect("prelude");
    let ranges: Vec<Range> = prelude.reads.iter().map(|read| read.range).collect();
    assert_eq!(
        ranges,
        vec![cmis_revision_byte()],
        "prelude reads: {:?}",
        prelude.reads
    );
    assert!(prelude.refusals.is_empty(), "{:?}", prelude.refusals);

    // Synthetic view (labelled): the revision byte alone — the prelude's
    // own bytes — is all the remaining round needs to plan the rest.
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x51])]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert_eq!(plan.reads.len(), 1, "reads: {:?}", plan.reads);
    assert_eq!(plan.reads[0].range, entry.range());
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (j) View with a stale generation ─────────────────────────────

/// A view whose observations carry a different module generation than
/// the one the plan is asked against: a view-level problem, not a
/// per-entry refusal — `Err(FieldPlanError::Input(..))`.
#[test]
fn remaining_rejects_stale_generation_view() {
    let mut stale = stamp();
    stale.generation = ModuleGeneration(999);
    let snap = Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: ModuleGeneration(999),
        epoch: EPOCH,
        ranges: vec![RangeObservation {
            range: cmis_revision_byte(),
            outcome: RangeOutcome::Bytes(vec![0x51]),
            stamp: stale,
            requester: ReadRequester::PassiveInspection,
        }],
    };
    let entries = resolve_page14h("media_side_snr", Rev::CMIS_5_1);
    let error = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect_err("stale generation must be a plan-level input error");
    assert!(
        matches!(error, FieldPlanError::Input(..)),
        "unexpected error: {error:?}"
    );
}

// ─── Stage 1b Rust corrections (a) and (b) ─────────────────────────────

/// Correction (a), prerequisite half, without prelude filtering: a real
/// CMIS 5.1 row (`media_side_snr` lane 1) cloned (labelled synthetic)
/// with its `Equals` prerequisite relocated to a clear-on-read
/// lower-memory latched-flag byte (byte 8, `module_state_changed_flag`,
/// CMIS 5.1 catalog), then passed straight to `field_read_remaining` —
/// no prelude result applied first. With the prerequisite bytes supplied
/// AND matching the expected code, and with them absent, the entry is
/// refused `Effect(ClearOnRead)` — an effectful prerequisite outranks
/// both a satisfied and a missing one — and no read covers the entry and
/// no acquisition requirement is published.
#[test]
fn effectful_prerequisite_refused_in_remaining_without_prelude_filtering() {
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    // The row's own prerequisite, cloned and relocated: kind, bit shape,
    // expected code and content switch kept from the real row, so a
    // supplied 06h at byte 8 satisfies it exactly as 06h at the Page 14h
    // selector would.
    let mut prereq = match snr.row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("media_side_snr presence: {other:?}"),
    };
    prereq.loc = FieldLoc {
        address: 0x50,
        page: 0x00,
        offset: 8,
        len: 1,
    };
    prereq.banked = false;
    let mut row = snr.row.clone();
    row.presence = FieldPresence::Equals(prereq);
    let entry = FieldEntry {
        row: &row,
        rev: snr.rev,
        lane: snr.lane,
        bank: snr.bank,
        offset: snr.offset,
        bit_offset: snr.bit_offset,
        revision_probe: snr.revision_probe,
    };
    let entries = [entry];

    // Context: the prelude already refuses this entry (an effect-tainted
    // prerequisite refuses into `FieldPlanRefusal`); remaining must reach
    // the same refusal without the caller filtering on that result.
    let prelude = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, &entries).expect("prelude");
    assert_eq!(prelude.refusals.len(), 1, "{:?}", prelude.refusals);
    assert_eq!(
        prelude.refusals[0].reason,
        FieldPlanRefusalReason::ClearOnRead
    );
    assert!(
        prelude.reads.is_empty(),
        "prelude reads: {:?}",
        prelude.reads
    );

    // Prerequisite bytes supplied and matching: still refused — the
    // effect is checked before the bytes are read or interpreted.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x00, 8, vec![0x06]),
    ]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!((refusal.field, refusal.lane), (snr.row.name, snr.lane));
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead)
    );
    assert!(
        !plan
            .reads
            .iter()
            .any(|read| ranges_intersect(read.range, entry.range())),
        "a read covers the refused entry's range {:?}: {:?}",
        entry.range(),
        plan.reads
    );
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );

    // Prerequisite bytes absent: still `Effect` — the effect outranks the
    // missing byte that would otherwise refuse `PreludeMissing`.
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x51])]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    assert_eq!(
        plan.refusals[0].reason,
        FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead)
    );
    assert!(
        !plan
            .reads
            .iter()
            .any(|read| ranges_intersect(read.range, entry.range())),
        "a read covers the refused entry's range {:?}: {:?}",
        entry.range(),
        plan.reads
    );
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

/// Correction (a), gate half. No real family gate byte is effectful — the
/// CMIS gate is lower-memory byte 1 and the SFF gates are the compliance
/// bytes, all passive in every cataloged revision — so the gate-effect
/// branch is reached through public inputs with a hand-built entry
/// (labelled synthetic): a real `temp_mon_value` row whose revision
/// probe, the only per-entry input `family_gate` consults, points at the
/// clear-on-read latched-flag byte 8, followed by a real `media_side_snr`
/// entry whose own probe is cleared. The effectful gate refuses every
/// entry before the gate byte is read: with the byte supplied as 0x30
/// (which, read as a revision byte, would report CMIS 3.0) and with the
/// byte absent (which would otherwise refuse `PreludeMissing`) — no
/// reads, no acquisition requirements, refusals in input order.
#[test]
fn effectful_gate_refuses_every_entry() {
    let probe = RevisionProbe {
        field: "synthetic gate probe at a clear-on-read byte (never a generated row)",
        loc: FieldLoc {
            address: 0x50,
            page: 0x00,
            offset: 8,
            len: 1,
        },
    };
    let base = resolve_field(Rev::CMIS_5_1, "temp_mon_value", "CmisLowerMemory");
    assert_eq!(base.len(), 1);
    let row = base[0].row.clone();
    let gated = FieldEntry {
        row: &row,
        rev: base[0].rev,
        lane: None,
        bank: 0,
        offset: base[0].offset,
        bit_offset: base[0].bit_offset,
        revision_probe: Some(&probe),
    };
    let mut snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    // `family_gate` takes the first entry's probe; clearing this one's
    // keeps the synthetic probe unambiguously the gate.
    snr.revision_probe = None;
    let entries = [gated, snr];

    // Gate byte supplied with a would-be revision mismatch: the effect
    // refuses every entry before the byte is interpreted.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x00, 8, vec![0x30]),
    ]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), entries.len(), "{:?}", plan.refusals);
    for (entry, refusal) in entries.iter().zip(&plan.refusals) {
        assert_eq!((refusal.field, refusal.lane), (entry.row.name, entry.lane));
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead),
            "{} lane {:?}",
            entry.row.name,
            entry.lane
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );

    // Gate byte absent: the effect still outranks the missing byte.
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x51])]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), entries.len(), "{:?}", plan.refusals);
    for (entry, refusal) in entries.iter().zip(&plan.refusals) {
        assert_eq!((refusal.field, refusal.lane), (entry.row.name, entry.lane));
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead)
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

/// Rule 18: at most one refusal per input entry, published in input
/// order — including refusals discovered while an entry's remaining
/// decode dependencies are processed. Credo capture (selector 06h), five
/// entries in a mixed order: the refused BER entry (immediate
/// `ViewNotSelected`), the admitted revision-irrelevant unconditional
/// monitor, a synthetic row (labelled) refused through its effect-tainted
/// external-calibration constants at the clear-on-read latched-flag
/// bytes, a synthetic `Unsupported` row (labelled), and the admitted SNR
/// entry. Two synthetic rows are renamed so each refusal names exactly
/// its entry; the shared bias-advertisement refusal path itself cannot
/// be made effectful through public inputs (its range is a fixed
/// catalog row, passive in every revision).
#[test]
fn one_refusal_per_entry_in_input_order() {
    let (_, _, snap) = credo_page14h_snapshot();

    // Entry 0 — refused immediately: BER lane 1 under the 06h view.
    let ber = resolve_page14h("media_side_ber", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_ber lane 1");

    // Entry 1 — admitted, revision-irrelevant: the unconditional
    // lower-memory monitor.
    let temp = resolve_field(Rev::CMIS_5_1, "temp_mon_value", "CmisLowerMemory");
    assert_eq!(temp.len(), 1);
    let temp = temp[0];

    // Entry 2 — synthetic (labelled): the monitor row cloned with
    // external-calibration constants at the clear-on-read latched-flag
    // bytes 8..=11, so its refusal surfaces during dependency
    // processing, not at its own (clean) range.
    let mut constants_row = temp.row.clone();
    constants_row.name = "temp_mon_value (synthetic constants clone)";
    let internal = match constants_row.scaling.clone() {
        Scaling::Scaled(lsb) => lsb,
        other => panic!("temp_mon_value scaling: {other:?}"),
    };
    constants_row.scaling = Scaling::Linear {
        internal,
        external: Some(ExternalLinear {
            slope: FieldLoc {
                address: 0x50,
                page: 0x00,
                offset: 8,
                len: 2,
            },
            offset: FieldLoc {
                address: 0x50,
                page: 0x00,
                offset: 10,
                len: 2,
            },
        }),
    };
    let constants_entry = FieldEntry {
        row: &constants_row,
        rev: temp.rev,
        lane: None,
        bank: 0,
        offset: temp.offset,
        bit_offset: temp.bit_offset,
        revision_probe: None,
    };

    // Entry 3 — synthetic (labelled): a real row cloned with an
    // `Unsupported` presence, refused before any prerequisite.
    let mut unsupported_row = ber.row.clone();
    unsupported_row.name = "media_side_ber (synthetic unsupported clone)";
    unsupported_row.presence = FieldPresence::Unsupported {
        expr: "synthetic unsupported presence (never a generated row)",
    };
    let unsupported_entry = FieldEntry {
        row: &unsupported_row,
        rev: ber.rev,
        lane: ber.lane,
        bank: ber.bank,
        offset: ber.offset,
        bit_offset: ber.bit_offset,
        revision_probe: None,
    };

    // Entry 4 — admitted: SNR lane 1 under the 06h view.
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");

    let entries = [ber, temp, constants_entry, unsupported_entry, snr];
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");

    // One refusal per refused entry, in input order: the immediate BER
    // refusal (entry 0), the dependency-processing refusal (entry 2),
    // the Unsupported refusal (entry 3).
    assert_eq!(plan.refusals.len(), 3, "{:?}", plan.refusals);
    let refusal_fields: Vec<(&str, Option<u16>)> =
        plan.refusals.iter().map(|r| (r.field, r.lane)).collect();
    assert_eq!(
        refusal_fields,
        vec![
            (ber.row.name, ber.lane),
            (constants_row.name, None),
            (unsupported_row.name, unsupported_entry.lane),
        ],
        "refusals: {:?}",
        plan.refusals
    );
    assert!(matches!(
        plan.refusals[0].reason,
        FieldAdmissionReason::ViewNotSelected { .. }
    ));
    assert_eq!(
        plan.refusals[1].reason,
        FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead)
    );
    assert!(matches!(
        plan.refusals[2].reason,
        FieldAdmissionReason::UnsupportedPresence { .. }
    ));

    // The admitted entries' ranges are still planned, once each.
    for admitted_entry in [temp, snr] {
        assert!(
            plan.reads
                .iter()
                .any(|read| read.range.contains(admitted_entry.range())),
            "no read covers {}'s range {:?}: {:?}",
            admitted_entry.row.name,
            admitted_entry.range(),
            plan.reads
        );
    }
}

/// Admission-only views (synthetic, labelled): lower byte 1 and the
/// Page 14h selector byte 128 and nothing else — exactly the prelude's
/// own planned bytes. Selector 06h admits the SNR entry (its data range
/// planned, its acquisition requirement present) and refuses BER; selector
/// 01h the reverse. Admission needs no dependent data bytes: the remaining
/// round reads only the gate and prerequisite values from the view.
#[test]
fn remaining_with_prelude_bytes_only() {
    for (selector, admitted, refused) in [
        (0x06u8, "media_side_snr", "media_side_ber"),
        (0x01, "media_side_ber", "media_side_snr"),
    ] {
        let snap = snapshot(&[
            (0x50, 0, 0x00, 1, vec![0x51]),
            (0x50, 0, 0x14, 128, vec![selector]),
        ]);
        let mut entries: Vec<_> = resolve_page14h(refused, Rev::CMIS_5_1)
            .into_iter()
            .filter(|e| e.lane == Some(1))
            .collect();
        entries.extend(
            resolve_page14h(admitted, Rev::CMIS_5_1)
                .into_iter()
                .filter(|e| e.lane == Some(1)),
        );
        assert_eq!(entries.len(), 2);

        let plan = field_read_remaining(
            Family::Cmis,
            Rev::CMIS_5_1,
            &entries,
            &snap,
            GENERATION,
            EPOCH,
        )
        .expect("field_read_remaining");

        // The refused entry: ViewNotSelected with the row's own expected
        // value and the view's actual selector.
        assert_eq!(
            plan.refusals.len(),
            1,
            "selector {selector:02x}: {:?}",
            plan.refusals
        );
        let refusal = &plan.refusals[0];
        assert_eq!(
            (refusal.field, refusal.lane),
            (entries[0].row.name, entries[0].lane)
        );
        assert!(
            matches!(refusal.reason, FieldAdmissionReason::ViewNotSelected { .. }),
            "selector {selector:02x}: {refusal:?}"
        );

        // The admitted entry: its own range planned from a view that
        // supplies none of its bytes, and its acquisition requirement.
        let admitted_entry = entries[1];
        assert!(
            plan.reads
                .iter()
                .any(|read| read.range.contains(admitted_entry.range())),
            "selector {selector:02x}: no read covers {:?}: {:?}",
            admitted_entry.range(),
            plan.reads
        );
        let requirement = sole_requirement(&plan);
        assert_eq!(
            (requirement.field, requirement.lane),
            (admitted_entry.row.name, admitted_entry.lane)
        );
        assert_eq!(requirement.selector_value, u64::from(selector));
    }
}

/// Correction (b): a supplied prerequisite whose kind carries no scalar
/// code is refused `PrerequisiteUndecodable` — never a fabricated actual
/// value, plain and content-multiplexed alike. Plain case: the SFF-8472
/// `optional_laser_temperature` row (a plain `Equals`, no content switch)
/// cloned (labelled synthetic) with its prerequisite kind replaced by
/// `Bytes`, bytes supplied. Content-multiplexed case: the real Page 14h
/// `media_side_snr` row cloned (labelled synthetic) with its prerequisite
/// kind replaced by `Bitmap` and `expected` set to `u64::MAX` — the value
/// the removed no-code sentinel fabricated — proving failed extraction
/// never admits the entry. Both: no data, no acquisition requirement.
#[test]
fn undecodable_prerequisite_is_typed_for_plain_and_content_switch() {
    // Plain: SFF-8472, compliance-admitting view, prerequisite bytes
    // supplied at the location the real row names (A0h byte 92).
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let base = resolve_field(
        rev,
        "optional_laser_temperature",
        "page_a2h_diagnostics_base",
    );
    assert_eq!(base.len(), 1);
    let mut prereq = match base[0].row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("optional_laser_temperature presence: {other:?}"),
    };
    prereq.kind = FieldKind::Bytes;
    let plain_name = prereq.field;
    let mut row = base[0].row.clone();
    row.presence = FieldPresence::Equals(prereq);
    let plain = FieldEntry {
        row: &row,
        rev: base[0].rev,
        lane: base[0].lane,
        bank: base[0].bank,
        offset: base[0].offset,
        bit_offset: base[0].bit_offset,
        revision_probe: base[0].revision_probe,
    };

    let snap = snapshot(&[sff8472_a0h(0x00, 0x0a)]);
    let plan = field_read_remaining(Family::Sff8472, rev, &[plain], &snap, GENERATION, EPOCH)
        .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!(
        (refusal.field, refusal.lane),
        (base[0].row.name, base[0].lane)
    );
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::PrerequisiteUndecodable {
            prerequisite: plain_name,
        }
    );
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );

    // Content-multiplexed: a Page 14h row whose selector prerequisite
    // kind is non-scalar, with expected = u64::MAX (the value the
    // removed sentinel would have fabricated as the actual code).
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    let mut prereq = match snr.row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("media_side_snr presence: {other:?}"),
    };
    prereq.kind = FieldKind::Bitmap;
    prereq.expected = u64::MAX;
    let muxed_name = prereq.field;
    let mut row = snr.row.clone();
    row.presence = FieldPresence::Equals(prereq);
    let muxed = FieldEntry {
        row: &row,
        rev: snr.rev,
        lane: snr.lane,
        bank: snr.bank,
        offset: snr.offset,
        bit_offset: snr.bit_offset,
        revision_probe: snr.revision_probe,
    };

    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x14, 128, vec![0x06]),
    ]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &[muxed],
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 1, "{:?}", plan.refusals);
    let refusal = &plan.refusals[0];
    assert_eq!((refusal.field, refusal.lane), (snr.row.name, snr.lane));
    assert_eq!(
        refusal.reason,
        FieldAdmissionReason::PrerequisiteUndecodable {
            prerequisite: muxed_name,
        }
    );
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (k) Malformed entries are plan-level input errors (R-B1) ──────

/// R-B1: structural validation of every entry — own range, `Equals`
/// prerequisite range, remaining-dependency (constants) range — happens
/// before the family gate and the admission walk in both rounds, so a
/// structurally malformed entry is a plan-level `FieldPlanError::Input`
/// (rule 8), never an admission refusal, whichever cheaper rule would
/// otherwise dispose of it first. Four malformed shapes, each a real
/// `media_side_snr` row cloned (labelled synthetic) with one invalid
/// range (`Range::validate`: address > 0x7f, len 0): the entry's own
/// range (address 0x80; zero length), the prerequisite's range
/// (address 0x80), the external-calibration constants' range
/// (address 0x80). Each shape is tested with its real `Equals`
/// presence against a view lacking the gate byte (which would otherwise
/// publish `PreludeMissing` for every entry before any per-entry
/// validation); the own-range and constants shapes additionally with a
/// synthetic `Unsupported` presence against a gate-supplying view
/// (which would otherwise publish `UnsupportedPresence` and skip every
/// later check) — a row with `Unsupported` presence carries no
/// prerequisite, so the prerequisite-malformed shape has no such
/// variant. The prelude is checked for every shape: it would otherwise
/// plan the gate byte alone. All Err.
#[test]
fn malformed_entries_are_plan_level_input_errors_before_gate_or_admission() {
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    let internal = match snr.row.scaling.clone() {
        Scaling::Scaled(lsb) => lsb,
        other => panic!("media_side_snr scaling: {other:?}"),
    };

    // Shape 1 — own range invalid: seven-bit address violation.
    let mut own_address = snr.row.clone();
    own_address.name = "media_side_snr (synthetic own-address-0x80 clone)";
    own_address.address = 0x80;

    // Shape 2 — own range invalid: zero length.
    let mut own_len = snr.row.clone();
    own_len.name = "media_side_snr (synthetic own-len-0 clone)";
    own_len.len = 0;

    // Shape 3 — `Equals` prerequisite range invalid: address 0x80 on the
    // prerequisite's own location (the row's presence is kept, so the
    // prelude's pass 1 sees the same malformed range).
    let mut prereq = match snr.row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("media_side_snr presence: {other:?}"),
    };
    prereq.loc.address = 0x80;
    let mut prereq_address = snr.row.clone();
    prereq_address.name = "media_side_snr (synthetic prerequisite-0x80 clone)";
    prereq_address.presence = FieldPresence::Equals(prereq);

    // Shape 4 — remaining-dependency range invalid: external-calibration
    // constants at address 0x80 (a plain `Scaling::Linear` shape the
    // remaining round would plan for an admitted entry).
    let mut constants = snr.row.clone();
    constants.name = "media_side_snr (synthetic constants-0x80 clone)";
    constants.scaling = Scaling::Linear {
        internal,
        external: Some(ExternalLinear {
            slope: FieldLoc {
                address: 0x80,
                page: 0x00,
                offset: 8,
                len: 2,
            },
            offset: FieldLoc {
                address: 0x80,
                page: 0x00,
                offset: 10,
                len: 2,
            },
        }),
    };

    let equals_rows = [own_address, own_len, prereq_address, constants];
    // The `Unsupported` variant exists only for the shapes whose malformed
    // range survives the presence replacement (indices 0, 1 and 3: the
    // own-range and constants shapes): a row with
    // `FieldPresence::Unsupported` carries no `Equals` prerequisite at
    // all, so the prerequisite-malformed shape has no Unsupported
    // variant to test — replacing the presence removes the malformed
    // range with it.
    let unsupported_rows: Vec<CatalogRow> = [0, 1, 3]
        .map(|shape| equals_rows[shape].clone())
        .into_iter()
        .map(|mut row: CatalogRow| {
            row.presence = FieldPresence::Unsupported {
                expr: "synthetic unsupported presence (never a generated row)",
            };
            row
        })
        .collect();
    fn entry_for<'a>(row: &'a CatalogRow, template: &FieldEntry<'_>) -> FieldEntry<'a> {
        FieldEntry {
            row,
            rev: template.rev,
            lane: template.lane,
            bank: 0,
            offset: row.offset,
            bit_offset: row.bit_offset,
            revision_probe: None,
        }
    }
    let equals_entries: Vec<FieldEntry<'_>> =
        equals_rows.iter().map(|row| entry_for(row, &snr)).collect();
    let unsupported_entries: Vec<FieldEntry<'_>> = unsupported_rows
        .iter()
        .map(|row| entry_for(row, &snr))
        .collect();

    // (a) A view lacking the gate byte: today the gate's early return
    // publishes `PreludeMissing` refusals for every entry before any
    // per-entry range is validated.
    let gate_absent = snapshot(&[(0x50, 0, 0x14, 128, vec![0x06])]);
    // (b) `FieldPresence::Unsupported` under a gate-supplying view: today
    // rule 2 refuses the entry before any range is validated.
    let gate_supplied = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x14, 128, vec![0x06]),
    ]);

    for entry in &equals_entries {
        let name = entry.row.name;
        let error = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, std::slice::from_ref(entry))
            .err()
            .unwrap_or_else(|| panic!("{name}: prelude accepted a malformed entry"));
        assert!(
            matches!(error, FieldPlanError::Input(..)),
            "{name}: prelude error {error:?}"
        );
        let error = field_read_remaining(
            Family::Cmis,
            Rev::CMIS_5_1,
            std::slice::from_ref(entry),
            &gate_absent,
            GENERATION,
            EPOCH,
        )
        .err()
        .unwrap_or_else(|| {
            panic!("{name}: remaining accepted a malformed entry under a gate-absent view")
        });
        assert!(
            matches!(error, FieldPlanError::Input(..)),
            "{name}: remaining error {error:?}"
        );
    }
    for entry in &unsupported_entries {
        let name = entry.row.name;
        let error = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, std::slice::from_ref(entry))
            .err()
            .unwrap_or_else(|| panic!("{name}: prelude accepted a malformed Unsupported entry"));
        assert!(
            matches!(error, FieldPlanError::Input(..)),
            "{name}: prelude error {error:?}"
        );
        let error = field_read_remaining(
            Family::Cmis,
            Rev::CMIS_5_1,
            std::slice::from_ref(entry),
            &gate_supplied,
            GENERATION,
            EPOCH,
        )
        .err()
        .unwrap_or_else(|| {
            panic!(
            "{name}: remaining accepted a malformed Unsupported entry under a gate-supplied view"
        )
        });
        assert!(
            matches!(error, FieldPlanError::Input(..)),
            "{name}: remaining error {error:?}"
        );
    }
}

/// N2 (parity with C `plan_row_valid`): the content-switch data window
/// is a structural location too. The remaining round publishes it as
/// `AcquisitionRequirement::data_range`, and C validates it alongside
/// every other prerequisite location (`prereq->has_content_switch &&
/// !plan_loc_valid(&prereq->content_switch.data)`); without the same
/// check here, a hand-built entry whose window runs past the page end
/// yields INVALID_ARGUMENT in C and an `Ok` plan carrying an invalid
/// `Range` as acquisition evidence in Rust. The real `media_side_snr`
/// row cloned (labelled synthetic) with its content-switch data window
/// at Page 14h offset 192 len 128 (end 320, past the 256-byte window):
/// `FieldPlanError::Input` in both rounds.
#[test]
fn content_switch_data_window_is_structurally_validated() {
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    let mut prereq = match snr.row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("media_side_snr presence: {other:?}"),
    };
    let mut switch = prereq
        .content_switch
        .clone()
        .expect("media_side_snr content switch");
    switch.data = FieldLoc {
        address: 0x50,
        page: 0x14,
        offset: 192,
        len: 128,
    };
    prereq.content_switch = Some(switch);
    let mut row = snr.row.clone();
    row.name = "media_side_snr (synthetic content-switch-window clone)";
    row.presence = FieldPresence::Equals(prereq);
    let entry = FieldEntry {
        row: &row,
        rev: snr.rev,
        lane: snr.lane,
        bank: 0,
        offset: row.offset,
        bit_offset: row.bit_offset,
        revision_probe: None,
    };

    // A gate-supplying, selector-satisfying view: without the
    // structural check the remaining round would admit the entry and
    // publish the invalid window as `data_range`.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x14, 128, vec![0x06]),
    ]);
    let error = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, std::slice::from_ref(&entry))
        .err()
        .unwrap_or_else(|| panic!("prelude accepted an invalid content-switch data window"));
    assert!(
        matches!(error, FieldPlanError::Input(..)),
        "prelude error {error:?}"
    );
    let error = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        std::slice::from_ref(&entry),
        &snap,
        GENERATION,
        EPOCH,
    )
    .err()
    .unwrap_or_else(|| panic!("remaining accepted an invalid content-switch data window"));
    assert!(
        matches!(error, FieldPlanError::Input(..)),
        "remaining error {error:?}"
    );
}

/// N8 (parity with C `plan_row_valid` via `row_applicable`): an entry
/// whose row does not apply at the entry's own revision — `since`
/// later than the revision — is a plan-level error in both rounds,
/// never an admitted or planned entry. `resolve` never returns rows
/// outside their applicability window, so only a hand-built entry can
/// carry one; `validate_field_entries` has already established
/// `entry.rev == revision` when this check runs, so the row-absent
/// variant names it (an entry-revision mismatch would be
/// self-contradictory: the two revisions are equal).
#[test]
fn rows_inapplicable_at_the_entry_revision_are_plan_level_errors() {
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    let mut row = snr.row.clone();
    row.name = "media_side_snr (synthetic since-6.0 clone)";
    row.since = Some(Rev {
        family: Family::Cmis,
        major: 6,
        minor: 0,
    });
    row.until = None;
    let entry = FieldEntry {
        row: &row,
        rev: snr.rev,
        lane: snr.lane,
        bank: 0,
        offset: row.offset,
        bit_offset: row.bit_offset,
        revision_probe: None,
    };

    // A gate-supplying view: the applicability check must fire before
    // any view logic, in both rounds.
    let snap = snapshot(&[
        (0x50, 0, 0x00, 1, vec![0x51]),
        (0x50, 0, 0x14, 128, vec![0x06]),
    ]);
    let error = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, std::slice::from_ref(&entry))
        .err()
        .unwrap_or_else(|| panic!("prelude accepted a row inapplicable at the entry revision"));
    let FieldPlanError::MonitorRowAbsent { page, name, rev } = error else {
        panic!("prelude error {error:?}")
    };
    assert_eq!((page, name, rev), (row.page_id, row.name, snr.rev));
    let error = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        std::slice::from_ref(&entry),
        &snap,
        GENERATION,
        EPOCH,
    )
    .err()
    .unwrap_or_else(|| panic!("remaining accepted a row inapplicable at the entry revision"));
    let FieldPlanError::MonitorRowAbsent { page, name, rev } = error else {
        panic!("remaining error {error:?}")
    };
    assert_eq!((page, name, rev), (row.page_id, row.name, snr.rev));
}

// ─── (l) Non-admitting SFF compliance codes refuse with the code (R-B2) ─

/// R-B2: a compliance byte `compliance_admits` rejects at the schema
/// revision refuses every entry with `ComplianceRefused` carrying that
/// byte, before any prerequisite is read — the SFF half of rule 1.
/// SFF-8472 at the 12.5 schema revision: 0x08 names Rev 12.3 and 0xff is
/// reserved — both refuse (only 0x0a names 12.5). SFF-8636 at 2.11: 0x08
/// names Rev 2.8-2.10 (no single revision) and 0x0a names Rev 2.12 —
/// both refuse (only 0x09 names 2.11). No reads, no acquisition
/// requirements, and the reported code is in the refusal.
#[test]
fn non_admitting_compliance_codes_refuse_with_the_code_in_the_reason() {
    // SFF-8472: `optional_laser_temperature`, A0h byte 94 as the gate.
    let rev8472 = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let entries = resolve_field(
        rev8472,
        "optional_laser_temperature",
        "page_a2h_diagnostics_base",
    );
    assert_eq!(entries.len(), 1);
    for code in [0x08u8, 0xff] {
        let snap = snapshot(&[sff8472_a0h(0x00, code)]);
        let plan =
            field_read_remaining(Family::Sff8472, rev8472, &entries, &snap, GENERATION, EPOCH)
                .unwrap_or_else(|e| panic!("code {code:02x}: {e:?}"));
        assert_eq!(
            plan.refusals.len(),
            1,
            "code {code:02x}: {:?}",
            plan.refusals
        );
        let refusal = &plan.refusals[0];
        assert_eq!(
            (refusal.field, refusal.lane),
            (entries[0].row.name, entries[0].lane)
        );
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::ComplianceRefused { code },
            "code {code:02x}"
        );
        assert!(plan.reads.is_empty(), "code {code:02x}: {:?}", plan.reads);
        assert!(
            plan.acquisition_required.is_empty(),
            "code {code:02x}: {:?}",
            plan.acquisition_required
        );
    }

    // SFF-8636: the lower-page `temperature`, byte 1 as the gate — the
    // gate is reachable with an ordinary resolved entry.
    let rev8636 = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let entries = resolve_field(rev8636, "temperature", "Sff8636LowerMemory");
    assert_eq!(entries.len(), 1);
    for code in [0x08u8, 0x09] {
        let snap = snapshot(&[(0x50, 0, 0x00, 0, vec![0x00, code])]);
        let plan =
            field_read_remaining(Family::Sff8636, rev8636, &entries, &snap, GENERATION, EPOCH)
                .unwrap_or_else(|e| panic!("code {code:02x}: {e:?}"));
        assert_eq!(
            plan.refusals.len(),
            1,
            "code {code:02x}: {:?}",
            plan.refusals
        );
        let refusal = &plan.refusals[0];
        assert_eq!(
            (refusal.field, refusal.lane),
            (entries[0].row.name, entries[0].lane)
        );
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::ComplianceRefused { code },
            "code {code:02x}"
        );
        assert!(plan.reads.is_empty(), "code {code:02x}: {:?}", plan.reads);
        assert!(
            plan.acquisition_required.is_empty(),
            "code {code:02x}: {:?}",
            plan.acquisition_required
        );
    }
}

/// N5: rule 12 (the compliance gate is checked before any entry's
/// prerequisite) and rule 13 (the refusal covers every entry): two
/// `optional_laser_temperature` entries under a refusing compliance
/// code with the prerequisite byte UNSATISFIED (`externally_calibrated`
/// set, A0h byte 92 bit 4) are both refused `ComplianceRefused`
/// carrying that code — never `PrerequisiteNotSatisfied` — and nothing
/// is planned or required. The second entry is a distinctly-named
/// clone of the first (labelled synthetic) so the every-entry mapping
/// is visible in the field names.
#[test]
fn compliance_refusal_outranks_the_prerequisite_for_every_entry() {
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let resolved = resolve_field(
        rev,
        "optional_laser_temperature",
        "page_a2h_diagnostics_base",
    );
    assert_eq!(resolved.len(), 1);
    let template = &resolved[0];
    let mut row_b = template.row.clone();
    row_b.name = "optional_laser_temperature (synthetic second entry)";
    let entries = [
        FieldEntry {
            row: template.row,
            rev: template.rev,
            lane: template.lane,
            bank: 0,
            offset: template.offset,
            bit_offset: template.bit_offset,
            revision_probe: None,
        },
        FieldEntry {
            row: &row_b,
            rev: template.rev,
            lane: template.lane,
            bank: 0,
            offset: row_b.offset,
            bit_offset: row_b.bit_offset,
            revision_probe: None,
        },
    ];
    let code = 0x08u8;
    let snap = snapshot(&[sff8472_a0h(0x10, code)]);
    let plan = field_read_remaining(Family::Sff8472, rev, &entries, &snap, GENERATION, EPOCH)
        .unwrap_or_else(|e| panic!("code {code:02x}: {e:?}"));
    assert_eq!(plan.refusals.len(), 2, "{:?}", plan.refusals);
    let refusal_fields: Vec<&str> = plan.refusals.iter().map(|r| r.field).collect();
    assert_eq!(
        refusal_fields,
        vec![template.row.name, row_b.name],
        "every entry refused, in input order"
    );
    for refusal in &plan.refusals {
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::ComplianceRefused { code },
            "the gate outranks the unsatisfied prerequisite: {:?}",
            refusal.reason
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}

// ─── (m) Prelude refusals in input-entry order (R-A1, rule 18) ────

/// R-A1: the prelude publishes its refusals in input-entry order
/// (rule 18), not in pass order. Mixed case: entry A (input 0) survives
/// pass 1 — its prerequisite (the Page 14h selector) is clean — and is
/// refused only by the effectful gate in pass 2; entry B (input 1) is
/// refused by its effectful prerequisite in pass 1. The gate is made
/// effectful through the public seam `family_gate` consults: entry A
/// carries a synthetic revision probe at the clear-on-read latched-flag
/// byte 8 (labelled synthetic; no generated probe points there — for
/// SFF families the gate is a fixed catalog row, passive in every
/// revision, so the mixed order is reachable only through this seam).
/// Refusals must publish [A, B] — input order — with nothing planned.
#[test]
fn prelude_refusals_are_published_in_input_entry_order() {
    let probe = RevisionProbe {
        field: "synthetic gate probe at a clear-on-read byte (never a generated row)",
        loc: FieldLoc {
            address: 0x50,
            page: 0x00,
            offset: 8,
            len: 1,
        },
    };
    // Entry A — input 0: the real `media_side_snr` lane 1 row (clean
    // selector prerequisite) carrying the synthetic gate probe.
    let snr = resolve_page14h("media_side_snr", Rev::CMIS_5_1)
        .into_iter()
        .find(|entry| entry.lane == Some(1))
        .expect("media_side_snr lane 1");
    let a = FieldEntry {
        row: snr.row,
        rev: snr.rev,
        lane: snr.lane,
        bank: 0,
        offset: snr.offset,
        bit_offset: snr.bit_offset,
        revision_probe: Some(&probe),
    };

    // Entry B — input 1: a renamed clone whose `Equals` prerequisite is
    // relocated to the same clear-on-read byte 8 (refused in pass 1).
    let mut prereq = match snr.row.presence.clone() {
        FieldPresence::Equals(prereq) => prereq,
        other => panic!("media_side_snr presence: {other:?}"),
    };
    prereq.loc = FieldLoc {
        address: 0x50,
        page: 0x00,
        offset: 8,
        len: 1,
    };
    prereq.banked = false;
    let mut row_b = snr.row.clone();
    row_b.name = "media_side_snr (synthetic effectful-prerequisite clone)";
    row_b.presence = FieldPresence::Equals(prereq);
    let b = FieldEntry {
        row: &row_b,
        rev: snr.rev,
        lane: snr.lane,
        bank: 0,
        offset: row_b.offset,
        bit_offset: row_b.bit_offset,
        revision_probe: None,
    };

    let entries = [a, b];
    let prelude = field_read_prelude(Family::Cmis, Rev::CMIS_5_1, &entries).expect("prelude");

    // Input order: A (gate effect, discovered in pass 2) publishes
    // before B (prerequisite effect, discovered in pass 1) — pass-2
    // refusals land at their entry's input position, not after every
    // pass-1 refusal.
    assert_eq!(prelude.refusals.len(), 2, "{:?}", prelude.refusals);
    let refusal_fields: Vec<&str> = prelude.refusals.iter().map(|r| r.field).collect();
    assert_eq!(
        refusal_fields,
        vec![snr.row.name, row_b.name],
        "refusals: {:?}",
        prelude.refusals
    );
    assert!(prelude
        .refusals
        .iter()
        .all(|r| r.reason == FieldPlanRefusalReason::ClearOnRead));
    // Nothing planned: B contributed nothing, A's contribution was
    // cleared by the gate refusal.
    assert!(prelude.reads.is_empty(), "reads: {:?}", prelude.reads);
}

// ─── (n) Instanced clear-on-read rows refuse every lane (P-B3) ────

/// P-B3 (parent-reproduced on the C port, same classifier here): a
/// row's effect footprint is the union of its own span and every
/// instance offset span, so per-lane clear-on-read bytes are refused at
/// every lane, not only at the row's base offset. CMIS 5.1
/// `tuning_complete_flag_tx` (Page 12h, clear-on-read, lane byte
/// offsets 231..238): lane 1 (the row's own span, byte 231) was refused
/// before and after the footprint fix alike; lanes 2..8 (bytes
/// 232..238, outside the row's own one-byte span) must refuse
/// `Effect(ClearOnRead)` too — nothing planned, no acquisition
/// requirements.
#[test]
fn remaining_refuses_instanced_clear_on_read_lanes() {
    let entries = resolve_page12h("tuning_complete_flag_tx", Rev::CMIS_5_1);
    assert_eq!(entries.len(), 8, "8 lanes");
    // N4: pin the geometry this oracle rests on — the row's own span is
    // one byte (231..232) and lanes 1..8 sit at exactly 231..=238 — so
    // a schema change to the row's length or the lane offsets fails
    // loudly instead of silently weakening the regression check.
    assert_eq!(
        entries[0].row.len, 1,
        "the P-B3 oracle needs a one-byte row span"
    );
    for (index, entry) in entries.iter().enumerate() {
        let lane = index as u16 + 1;
        assert_eq!(entry.lane, Some(lane), "lane order");
        assert_eq!(
            u32::from(entry.offset),
            231 + u32::from(lane - 1),
            "lane {lane} must sit exactly one byte past its predecessor"
        );
    }

    // The prelude's own bytes (the gate) are all the remaining round
    // needs to reach rule 4 for every lane.
    let snap = snapshot(&[(0x50, 0, 0x00, 1, vec![0x51])]);
    let plan = field_read_remaining(
        Family::Cmis,
        Rev::CMIS_5_1,
        &entries,
        &snap,
        GENERATION,
        EPOCH,
    )
    .expect("field_read_remaining");
    assert_eq!(plan.refusals.len(), 8, "{:?}", plan.refusals);
    for (entry, refusal) in entries.iter().zip(&plan.refusals) {
        assert_eq!((refusal.field, refusal.lane), (entry.row.name, entry.lane));
        assert_eq!(
            refusal.reason,
            FieldAdmissionReason::Effect(FieldPlanRefusalReason::ClearOnRead),
            "{} lane {:?}",
            entry.row.name,
            entry.lane
        );
    }
    assert!(plan.reads.is_empty(), "reads: {:?}", plan.reads);
    assert!(
        plan.acquisition_required.is_empty(),
        "acquisition_required: {:?}",
        plan.acquisition_required
    );
}
