//! Library-owned read plan for arbitrary catalog-resolved fields: which
//! exact byte ranges a caller must acquire so `observe_fields` decodes
//! everything a full snapshot would, with clear-on-read bytes refused
//! before any transfer. Synthetic snapshots only; no captures, no I/O.
use std::cell::RefCell;
use std::collections::BTreeSet;

use openxcvr::catalog::{resolve, CatalogError, FieldEntry, FieldQuery, LaneSelection};
use openxcvr::observation::{
    field_read_plan, field_read_plan_with_limits, observe_fields, FieldPlanError, FieldPlanLimits,
    FieldPlanRefusalReason, FieldReadPlan, ObservationState, ObserveOptions,
    FIELD_PLAN_DEFAULT_LIMITS,
};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{
    supplied_bytes, ByteView, CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource,
    ObservationStamp, Range, RangeObservation, RangeOutcome, ReadRequester, Snapshot,
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

// ─── Recording view: every byte the decoders touch ─────────────────────

struct RecordingView<'a> {
    snapshot: &'a Snapshot,
    touched: RefCell<BTreeSet<(u8, u8, u8, u8)>>,
}

impl<'a> RecordingView<'a> {
    fn new(snapshot: &'a Snapshot) -> Self {
        Self {
            snapshot,
            touched: RefCell::new(BTreeSet::new()),
        }
    }

    fn touched(&self) -> BTreeSet<(u8, u8, u8, u8)> {
        self.touched.borrow().clone()
    }
}

impl ByteView for RecordingView<'_> {
    fn get(&self, range: Range) -> Option<&RangeObservation> {
        for offset in u32::from(range.offset)..range.end() {
            self.touched
                .borrow_mut()
                .insert((range.address, range.bank, range.page, offset as u8));
        }
        self.snapshot.get(range)
    }
}

fn planned_bytes(plan: &FieldReadPlan) -> BTreeSet<(u8, u8, u8, u8)> {
    plan.reads
        .iter()
        .flat_map(|read| {
            (u32::from(read.range.offset)..read.range.end()).map(move |offset| {
                (
                    read.range.address,
                    read.range.bank,
                    read.range.page,
                    offset as u8,
                )
            })
        })
        .collect()
}

fn shape(plan: &FieldReadPlan) -> Vec<(Range, &'static str, bool)> {
    plan.reads
        .iter()
        .map(|r| (r.range, r.purpose, r.optional))
        .collect()
}

// ─── Fixtures (mirror obs_basic_read_plan.rs) ──────────────────────────

fn cmis_full_snapshot(revision_byte: u8, with_optional_pages: bool) -> Snapshot {
    let mut lower = vec![0u8; 18];
    lower[0] = 0x1e; // SFF-8024 identifier: QSFP+ with CMIS
    lower[1] = revision_byte;
    lower[14..16].copy_from_slice(&0x1000u16.to_be_bytes());
    lower[16..18].copy_from_slice(&33000u16.to_be_bytes());
    let mut parts = vec![(0x50u8, 0u8, 0u8, 0u8, lower)];
    if with_optional_pages {
        let mut page11 = vec![0u8; 128];
        for lane in 0..8u16 {
            page11[154 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(1000 + lane).to_be_bytes());
            page11[170 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(2500 + 10 * lane).to_be_bytes());
            page11[186 - 128 + 2 * usize::from(lane)..][..2]
                .copy_from_slice(&(3000 + lane).to_be_bytes());
        }
        parts.push((0x50, 0, 0x11, 128, page11));
        parts.push((0x50, 0, 0x01, 160, vec![0x00]));
    }
    snapshot(&parts)
}

fn sff8636_full_snapshot(identifier: u8, compliance: u8) -> Snapshot {
    let mut lower = vec![0u8; 128];
    lower[0] = identifier;
    lower[1] = compliance;
    lower[22..24].copy_from_slice(&(-768i16).to_be_bytes());
    lower[26..28].copy_from_slice(&33000u16.to_be_bytes());
    for lane in 0..4u16 {
        let lane = usize::from(lane);
        lower[34 + 2 * lane..][..2].copy_from_slice(&(3000 + lane as u16).to_be_bytes());
        lower[42 + 2 * lane..][..2].copy_from_slice(&(2500 + 10 * lane as u16).to_be_bytes());
        lower[50 + 2 * lane..][..2].copy_from_slice(&(1000 + lane as u16).to_be_bytes());
    }
    snapshot(&[(0x50, 0, 0, 0, lower)])
}

fn sff8472_full_snapshot(
    identifier: u8,
    dmt_byte: u8,
    compliance: u8,
    with_constants: bool,
) -> Snapshot {
    let mut a0 = vec![0u8; 96];
    a0[0] = identifier;
    a0[92] = dmt_byte;
    a0[94] = compliance;
    let mut a2 = vec![0u8; 120];
    a2[96..98].copy_from_slice(&(-768i16).to_be_bytes());
    a2[98..100].copy_from_slice(&33000u16.to_be_bytes());
    a2[100..102].copy_from_slice(&2500u16.to_be_bytes());
    a2[102..104].copy_from_slice(&1000u16.to_be_bytes());
    a2[104..106].copy_from_slice(&3000u16.to_be_bytes());
    if with_constants {
        a2[76..78].copy_from_slice(&0x0100u16.to_be_bytes());
        a2[78..80].copy_from_slice(&0i16.to_be_bytes());
        a2[80..82].copy_from_slice(&0x0100u16.to_be_bytes());
        a2[82..84].copy_from_slice(&0i16.to_be_bytes());
        a2[84..86].copy_from_slice(&0x0200u16.to_be_bytes());
        a2[86..88].copy_from_slice(&(-256i16).to_be_bytes());
        a2[88..90].copy_from_slice(&0x0100u16.to_be_bytes());
        a2[90..92].copy_from_slice(&0i16.to_be_bytes());
        a2[68..72].copy_from_slice(&1.0f32.to_be_bytes());
    }
    snapshot(&[(0x50, 0, 0, 0, a0), (0x51, 0, 0, 0, a2)])
}

// ─── Entry helpers: the daemon's own resolution path ───────────────────

fn query<'a>(pattern: &'a str, page: Option<&'a str>, lane: LaneSelection) -> FieldQuery<'a> {
    FieldQuery {
        pattern,
        lane,
        page,
        bank: 0,
    }
}

fn resolve_entries(
    revision: Rev,
    pattern: &str,
    page: Option<&str>,
    lane: LaneSelection,
) -> Vec<FieldEntry<'static>> {
    resolve(revision, &query(pattern, page, lane)).unwrap()
}

fn monitor_entries(family: Family, revision: Rev) -> Vec<FieldEntry<'static>> {
    let (temperature, supply, tx_bias, tx_power, rx_power) = match family {
        Family::Cmis => (
            ("CmisLowerMemory", "temp_mon_value"),
            ("CmisLowerMemory", "vcc_mon_voltage"),
            ("CmisPage11hLaneStatus", "laser_bias_tx"),
            ("CmisPage11hLaneStatus", "optical_power_tx"),
            ("CmisPage11hLaneStatus", "optical_power_rx"),
        ),
        Family::Sff8636 => (
            ("Sff8636LowerMemory", "temperature"),
            ("Sff8636LowerMemory", "supply_voltage"),
            ("Sff8636LowerMemory", "tx_bias_lane"),
            ("Sff8636LowerMemory", "tx_power_lane"),
            ("Sff8636LowerMemory", "rx_power_lane"),
        ),
        Family::Sff8472 => (
            ("page_a2h_diagnostics_base", "temperature"),
            ("page_a2h_diagnostics_base", "supply_voltage"),
            ("page_a2h_diagnostics_base", "tx_bias"),
            ("page_a2h_diagnostics_base", "tx_power"),
            ("page_a2h_diagnostics_base", "rx_power"),
        ),
        _ => unreachable!(),
    };
    let mut entries = Vec::new();
    for (page, name) in [temperature, supply, tx_bias, tx_power, rx_power] {
        // Lane-instanced fields expand with All; single-instance fields stay
        // Unspecified (a lane request on them is a typed NotInstanced error).
        let all = query(name, Some(page), LaneSelection::All);
        let selected = if matches!(
            resolve(revision, &all),
            Err(CatalogError::NotInstanced { .. })
        ) {
            LaneSelection::Unspecified
        } else {
            LaneSelection::All
        };
        entries.extend(resolve(revision, &query(name, Some(page), selected)).unwrap());
    }
    entries
}

// ─── Plan shapes ───────────────────────────────────────────────────────

#[test]
fn cmis_plan_coalesces_adjacent_ranges_and_marks_upper_pages_optional() {
    for revision in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let entries = monitor_entries(Family::Cmis, revision);
        let plan = field_read_plan(Family::Cmis, revision, &entries).unwrap();
        // Coalescing within one selection: temperature (14..15) and supply
        // voltage (16..17) merge into one lower-page range; the three page
        // 11h lane monitors (154..169, 170..185, 186..201) merge into one
        // 48-byte range whose purpose is the first (lowest-offset)
        // contributor. The page-01h bias advertisement rides along as the
        // laser-bias entries' dependency. Upper pages are optional exactly
        // as basic_read_remaining marks them (flat-memory modules do not
        // implement them; the decode yields NotSupplied).
        assert_eq!(
            shape(&plan),
            vec![
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0,
                        offset: 1,
                        len: 1
                    },
                    "cmis_revision",
                    false
                ),
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0,
                        offset: 14,
                        len: 4
                    },
                    "temp_mon_value",
                    false
                ),
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0x01,
                        offset: 160,
                        len: 1
                    },
                    "tx_bias_current_scaling_factor",
                    true
                ),
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0x11,
                        offset: 154,
                        len: 48
                    },
                    "optical_power_tx",
                    true
                ),
            ],
            "{revision:?}"
        );
        assert!(plan.refusals.is_empty(), "{revision:?}");
    }
}

#[test]
fn sff8636_plan_includes_the_compliance_gate_the_decode_reads() {
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let entries = monitor_entries(Family::Sff8636, rev);
    let plan = field_read_plan(Family::Sff8636, rev, &entries).unwrap();
    // observe_field gates every SFF decode on the snapshot's own
    // revision-compliance byte, so the plan must acquire it; the monitors
    // themselves are lower-page (never optional). Bytes 24..25 belong to
    // fields nobody asked for: coalescing never reads them. The three
    // lane monitors are contiguous (rx 34..41, bias 42..49, tx 50..57),
    // so they coalesce into one 24-byte range whose purpose is the
    // lowest-offset contributor.
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "revision_compliance",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 22,
                    len: 2
                },
                "temperature",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 26,
                    len: 2
                },
                "supply_voltage",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 34,
                    len: 24
                },
                "rx_power_lane",
                false
            ),
        ]
    );
    assert!(plan.refusals.is_empty());
}

#[test]
fn sff8472_plan_spans_a0h_and_a2h_addressing() {
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    // One telemetry field is enough to prove the addressing: the A0h
    // compliance gate and the external-calibration discriminator (byte 92),
    // the A2h field bytes, and the declared external-calibration constants
    // (slope 84..85, offset 86..87 — coalesced into one 4-byte range).
    let entries = resolve_entries(
        rev,
        "temperature",
        Some("page_a2h_diagnostics_base"),
        LaneSelection::Unspecified,
    );
    let plan = field_read_plan(Family::Sff8472, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 92,
                    len: 1
                },
                "page_a0h_base_memory.externally_calibrated",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 94,
                    len: 1
                },
                "sff8472_compliance",
                false
            ),
            (
                Range {
                    address: 0x51,
                    bank: 0,
                    page: 0,
                    offset: 84,
                    len: 4
                },
                "temperature",
                false
            ),
            (
                Range {
                    address: 0x51,
                    bank: 0,
                    page: 0,
                    offset: 96,
                    len: 2
                },
                "temperature",
                false
            ),
        ]
    );
    assert!(plan.refusals.is_empty());
}

#[test]
fn empty_entry_set_yields_an_empty_plan() {
    let plan = field_read_plan(Family::Cmis, Rev::CMIS_5_3, &[]).unwrap();
    assert!(plan.reads.is_empty());
    assert!(plan.refusals.is_empty());
    // UpTo(0) resolves to zero entries: same empty plan, no gate, no
    // advertisement, nothing acquired.
    let entries = resolve_entries(
        Rev::CMIS_5_3,
        "laser_bias_tx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::UpTo(0),
    );
    assert!(entries.is_empty());
    let plan = field_read_plan(Family::Cmis, Rev::CMIS_5_3, &entries).unwrap();
    assert!(plan.reads.is_empty());
}

// ─── Resolution surfaces: exact, glob, ambiguous, lanes, unknown ───────

#[test]
fn glob_matches_and_lane_selections_shape_the_plan() {
    let rev = Rev::CMIS_5_3;
    // Glob with the ? wildcard, page-qualified: the tx monitor's eight
    // instances. Its span (154..169) stops where the unrequested laser
    // bias starts, so nothing is coalesced across it and no unrequested
    // byte is read. (A bare `optical_power_*` glob also matches the
    // latched flags and thresholds; those surface as refusals or their
    // own ranges, which the oversize and refusal tests cover.)
    let entries = resolve_entries(
        rev,
        "optical_power_t?",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::All,
    );
    assert_eq!(entries.len(), 8);
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x11,
                    offset: 154,
                    len: 16
                },
                "optical_power_tx",
                true
            )
        ]
    );
    assert!(plan.refusals.is_empty());

    // Two exact-name resolutions (what a caller with two patterns gets):
    // tx (154..169) and rx (186..201) are separated by the laser-bias
    // span nobody asked for — two ranges, never merged across the gap.
    let mut entries = resolve_entries(
        rev,
        "optical_power_tx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::All,
    );
    entries.extend(resolve_entries(
        rev,
        "optical_power_rx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::All,
    ));
    assert_eq!(entries.len(), 16);
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    let ranges: Vec<Range> = plan.reads.iter().map(|read| read.range).collect();
    assert_eq!(
        ranges,
        vec![
            Range {
                address: 0x50,
                bank: 0,
                page: 0,
                offset: 1,
                len: 1
            },
            Range {
                address: 0x50,
                bank: 0,
                page: 0x11,
                offset: 154,
                len: 16
            },
            Range {
                address: 0x50,
                bank: 0,
                page: 0x11,
                offset: 186,
                len: 16
            },
        ]
    );

    // One lane: the instance's own bytes plus the bias advertisement.
    let entries = resolve_entries(
        rev,
        "laser_bias_tx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::One(3),
    );
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x01,
                    offset: 160,
                    len: 1
                },
                "tx_bias_current_scaling_factor",
                true
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x11,
                    offset: 174,
                    len: 2
                },
                "laser_bias_tx",
                true
            ),
        ]
    );

    // UpTo(2): the first two instances coalesce into one range.
    let entries = resolve_entries(
        rev,
        "laser_bias_tx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::UpTo(2),
    );
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x01,
                    offset: 160,
                    len: 1
                },
                "tx_bias_current_scaling_factor",
                true
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x11,
                    offset: 170,
                    len: 4
                },
                "laser_bias_tx",
                true
            ),
        ]
    );
}

#[test]
fn ambiguous_names_need_the_page_qualifier() {
    let rev = Rev::CMIS_5_3;
    // `page_checksum` exists on five pages: the exact name is ambiguous.
    let err = resolve(
        rev,
        &query("page_checksum", None, LaneSelection::Unspecified),
    )
    .unwrap_err();
    match &err {
        CatalogError::Ambiguous {
            pattern,
            candidates,
            ..
        } => {
            assert_eq!(pattern, "page_checksum");
            assert_eq!(candidates.len(), 5);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    // Qualified by page id, the plan is one optional upper-page byte.
    let entries = resolve_entries(
        rev,
        "page_checksum",
        Some("CmisPage02hThresholds"),
        LaneSelection::Unspecified,
    );
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x02,
                    offset: 255,
                    len: 1
                },
                "page_checksum",
                true
            )
        ]
    );
}

#[test]
fn unknown_names_and_out_of_range_lanes_refuse_typed() {
    let rev = Rev::CMIS_5_3;
    assert!(matches!(
        resolve(
            rev,
            &query("no_such_field", None, LaneSelection::Unspecified)
        ),
        Err(CatalogError::NoMatch { .. })
    ));
    assert!(matches!(
        resolve(
            rev,
            &query(
                "laser_bias_tx",
                Some("CmisPage11hLaneStatus"),
                LaneSelection::One(9)
            )
        ),
        Err(CatalogError::LaneOutOfRange {
            lane: 9,
            first: 1,
            last: 8,
            ..
        })
    ));
    // A lane request on a field without an instance dimension is typed too.
    assert!(matches!(
        resolve(
            rev,
            &query(
                "temp_mon_value",
                Some("CmisLowerMemory"),
                LaneSelection::One(1)
            )
        ),
        Err(CatalogError::NotInstanced { .. })
    ));
}

// ─── Clear-on-read refusal before any transfer ─────────────────────────

#[test]
fn clear_on_read_field_is_refused_and_never_planned() {
    let rev = Rev::CMIS_5_3;
    // A lower-page latched flag: reading its byte clears the latch.
    let entries = resolve_entries(
        rev,
        "temp_mon_high_alarm_flag",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert!(plan.reads.is_empty());
    assert_eq!(
        plan.refusals,
        vec![openxcvr::observation::FieldPlanRefusal {
            field: "temp_mon_high_alarm_flag",
            lane: None,
            reason: FieldPlanRefusalReason::ClearOnRead,
        }]
    );

    // Mixed query: the clean field is planned, the COR field refused, and
    // no planned range covers the COR byte (lower page byte 9).
    let mut entries = resolve_entries(
        rev,
        "temp_mon_value",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    entries.extend(resolve_entries(
        rev,
        "temp_mon_high_alarm_flag",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 14,
                    len: 2
                },
                "temp_mon_value",
                false
            )
        ]
    );
    assert_eq!(plan.refusals.len(), 1);
    let bytes = planned_bytes(&plan);
    assert!(!bytes.contains(&(0x50, 0, 0, 9)));
}

#[test]
fn refused_bytes_never_coalesce_into_neighbouring_ranges() {
    let rev = Rev::CMIS_5_3;
    // los_flag_rx occupies page 11h bytes 147..153 (clear on read);
    // optical_power_tx starts at 154. The refused entry's bytes must not
    // be absorbed into the monitor's coalesced range.
    let mut entries = resolve_entries(
        rev,
        "optical_power_tx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::All,
    );
    entries.extend(resolve_entries(
        rev,
        "los_flag_rx",
        Some("CmisPage11hLaneStatus"),
        LaneSelection::All,
    ));
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x11,
                    offset: 154,
                    len: 16
                },
                "optical_power_tx",
                true
            )
        ]
    );
    // Every refused lane is named, in entry order.
    assert_eq!(plan.refusals.len(), 8);
    for (index, refusal) in plan.refusals.iter().enumerate() {
        assert_eq!(refusal.field, "los_flag_rx");
        assert_eq!(refusal.lane, Some(index as u16 + 1));
        assert_eq!(refusal.reason, FieldPlanRefusalReason::ClearOnRead);
    }
    let bytes = planned_bytes(&plan);
    for offset in 147u8..=153u8 {
        assert!(
            !bytes.contains(&(0x50, 0, 0x11, offset)),
            "byte {offset} planned"
        );
    }
}

#[test]
fn sff8636_latched_flags_are_refused_like_cmis() {
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let entries = resolve_entries(
        rev,
        "tx_los_flag",
        Some("Sff8636LowerMemory"),
        LaneSelection::All,
    );
    let plan = field_read_plan(Family::Sff8636, rev, &entries).unwrap();
    assert!(
        plan.reads.is_empty(),
        "no byte may be planned for a latched flag"
    );
    assert_eq!(plan.refusals.len(), 4);
    assert!(plan
        .refusals
        .iter()
        .all(|r| r.reason == FieldPlanRefusalReason::ClearOnRead));
}

// ─── Write-only refusal before any transfer ───────────────────────────

#[test]
fn write_only_fields_are_refused_and_never_planned() {
    let rev = Rev::CMIS_5_3;
    // Write-only locations: CMIS software_reset (lower byte 26) and the
    // password change/entry areas (lower bytes 118..=125). A read of a
    // write-only location returns undefined or vendor-specific bytes, so
    // a decoded value would report a guess as a fact: no byte may be
    // planned for them.
    for name in [
        "software_reset",
        "password_change_entry_area",
        "password_entry_area",
    ] {
        let entries = resolve_entries(
            rev,
            name,
            Some("CmisLowerMemory"),
            LaneSelection::Unspecified,
        );
        let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
        assert!(plan.reads.is_empty(), "{name}: no byte may be planned");
        assert_eq!(
            plan.refusals,
            vec![openxcvr::observation::FieldPlanRefusal {
                field: name,
                lane: None,
                reason: FieldPlanRefusalReason::WriteOnly,
            }],
            "{name}: refused WriteOnly"
        );
        let bytes = planned_bytes(&plan);
        for offset in [26u8, 118, 119, 120, 121, 122, 123, 124, 125] {
            assert!(
                !bytes.contains(&(0x50, 0, 0, offset)),
                "{name}: write-only byte {offset} planned"
            );
        }
    }
}

#[test]
fn write_only_bytes_never_coalesce_into_neighbouring_ranges() {
    let rev = Rev::CMIS_5_3;
    // bank_select (lower byte 126) neighbours password_entry_area
    // (122..=125) without sharing a byte: it stays planned on exactly
    // its own byte — the refused bytes are absorbed into no merged
    // range and taint no surviving neighbour.
    let mut entries = resolve_entries(
        rev,
        "password_entry_area",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    entries.extend(resolve_entries(
        rev,
        "bank_select",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 126,
                    len: 1
                },
                "bank_select",
                false
            )
        ]
    );
    assert_eq!(
        plan.refusals,
        vec![openxcvr::observation::FieldPlanRefusal {
            field: "password_entry_area",
            lane: None,
            reason: FieldPlanRefusalReason::WriteOnly,
        }]
    );
    let bytes = planned_bytes(&plan);
    for offset in 118u8..=125u8 {
        assert!(
            !bytes.contains(&(0x50, 0, 0, offset)),
            "write-only byte {offset} planned"
        );
    }
}

#[test]
fn a_field_sharing_a_byte_with_a_write_only_row_is_refused() {
    let rev = Rev::CMIS_5_3;
    // low_pwr_request_sw is a readable row on its own bits but shares
    // lower byte 26 with software_reset: the byte its decode would read
    // is a write-only byte. The classification is by byte overlap,
    // exactly as the clear-on-read classification matches rows.
    let mut entries = resolve_entries(
        rev,
        "software_reset",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    entries.extend(resolve_entries(
        rev,
        "low_pwr_request_sw",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert!(plan.reads.is_empty(), "byte 26 is write-only; plan nothing");
    assert_eq!(
        plan.refusals,
        vec![
            openxcvr::observation::FieldPlanRefusal {
                field: "software_reset",
                lane: None,
                reason: FieldPlanRefusalReason::WriteOnly,
            },
            openxcvr::observation::FieldPlanRefusal {
                field: "low_pwr_request_sw",
                lane: None,
                reason: FieldPlanRefusalReason::WriteOnly,
            },
        ]
    );
    let bytes = planned_bytes(&plan);
    assert!(!bytes.contains(&(0x50, 0, 0, 26)));
}

#[test]
fn a_glob_plans_only_the_readable_matches() {
    let rev = Rev::CMIS_5_3;
    // "password*" matches three readable rows (password_cmd_result,
    // password_entry_supported, password_entry_result_supported — the
    // last two share page-01h byte 251, so they coalesce) and two
    // write-only ones: only the readable matches are planned, the rest
    // are refused.
    let entries = resolve_entries(rev, "password*", None, LaneSelection::Unspecified);
    assert_eq!(entries.len(), 5);
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert_eq!(
        shape(&plan),
        vec![
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 42,
                    len: 1
                },
                "password_cmd_result",
                false
            ),
            (
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0x01,
                    offset: 251,
                    len: 1
                },
                "password_entry_supported",
                true
            ),
        ]
    );
    assert_eq!(
        plan.refusals,
        vec![
            openxcvr::observation::FieldPlanRefusal {
                field: "password_change_entry_area",
                lane: None,
                reason: FieldPlanRefusalReason::WriteOnly,
            },
            openxcvr::observation::FieldPlanRefusal {
                field: "password_entry_area",
                lane: None,
                reason: FieldPlanRefusalReason::WriteOnly,
            },
        ]
    );
}

#[test]
fn observe_fields_reports_write_only_entries_without_a_value() {
    let rev = Rev::CMIS_5_3;
    let mut entries = resolve_entries(
        rev,
        "software_reset",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    entries.extend(resolve_entries(
        rev,
        "low_pwr_request_sw",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    entries.extend(resolve_entries(
        rev,
        "bank_select",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    // Undefined write-only readback bytes are present (byte 26 = 0x08:
    // software_reset bit set, low_pwr_request_sw bit clear) alongside a
    // readable neighbour (byte 126 = 0x03): a decode that ignored
    // write-only-ness would report the garbage as facts. The refused
    // entries must surface as refused with no value, exactly as
    // clear-on-read refusals surface; the readable neighbour decodes.
    // Byte 1 supplies the snapshot's reported revision (0x53 = 5.3) —
    // the decode's Stage 1 admission prerequisite.
    let view = snapshot(&[
        (0x50, 0, 0, 1, vec![0x53]),
        (0x50, 0, 0, 26, vec![0x08]),
        (0x50, 0, 0, 126, vec![0x03]),
    ]);
    let observed = observe_fields(
        Family::Cmis,
        rev,
        &entries,
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.len(), 3);
    assert_eq!(observed[0].field, "software_reset");
    assert_eq!(observed[1].field, "low_pwr_request_sw");
    assert_eq!(observed[2].field, "bank_select");
    assert_eq!(observed[0].state, ObservationState::WriteOnlyRefused);
    assert!(observed[0].value.is_none(), "no value for software_reset");
    assert_eq!(observed[1].state, ObservationState::WriteOnlyRefused);
    assert!(
        observed[1].value.is_none(),
        "no value for the byte-26 neighbour"
    );
    assert_eq!(observed[2].state, ObservationState::Valid);
    assert_eq!(
        observed[2].value,
        Some(openxcvr_core::observation::FieldValue::Raw(3))
    );
}

// ─── Bounds ────────────────────────────────────────────────────────────

#[test]
fn oversize_queries_are_refused_typed() {
    let rev = Rev::CMIS_5_3;
    // A whole-catalog glob resolves far more entries than the default
    // bound admits.
    let entries = resolve_entries(rev, "*", None, LaneSelection::Unspecified);
    assert!(entries.len() > FIELD_PLAN_DEFAULT_LIMITS.max_entries);
    assert!(matches!(
        field_read_plan(Family::Cmis, rev, &entries),
        Err(FieldPlanError::TooManyEntries { count, limit })
            if count == entries.len() && limit == FIELD_PLAN_DEFAULT_LIMITS.max_entries
    ));

    // Caller-supplied limits refuse the same query earlier.
    let monitors = monitor_entries(Family::Cmis, rev);
    let small = FieldPlanLimits {
        max_entries: 4,
        max_ranges: 64,
    };
    assert!(matches!(
        field_read_plan_with_limits(Family::Cmis, rev, &monitors, small),
        Err(FieldPlanError::TooManyEntries { count, limit }) if count == monitors.len() && limit == 4
    ));

    // The ranges bound binds after coalescing: two non-adjacent ranges
    // with a one-range allowance.
    let mut entries = resolve_entries(
        rev,
        "temp_mon_value",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    entries.extend(resolve_entries(
        rev,
        "cmis_revision",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    ));
    let one_range = FieldPlanLimits {
        max_entries: 8,
        max_ranges: 1,
    };
    assert!(matches!(
        field_read_plan_with_limits(Family::Cmis, rev, &entries, one_range),
        Err(FieldPlanError::TooManyRanges { count: 2, limit: 1 })
    ));

    // Zero limits admit nothing — no omitted/unlimited grant.
    let nothing = FieldPlanLimits {
        max_entries: 0,
        max_ranges: 0,
    };
    let one = resolve_entries(
        rev,
        "temp_mon_value",
        Some("CmisLowerMemory"),
        LaneSelection::Unspecified,
    );
    assert!(matches!(
        field_read_plan_with_limits(Family::Cmis, rev, &one, nothing),
        Err(FieldPlanError::TooManyEntries { count: 1, limit: 0 })
    ));
}

// ─── Unsupported family / revision / entry consistency ─────────────────

#[test]
fn unsupported_family_revision_and_entry_mismatches_refuse() {
    assert!(matches!(
        field_read_plan(Family::Elsfp, Rev::CMIS_5_3, &[]),
        Err(FieldPlanError::UnsupportedFamily(Family::Elsfp))
    ));
    let bad = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 10,
    };
    assert!(matches!(
        field_read_plan(Family::Sff8636, bad, &[]),
        Err(FieldPlanError::UnsupportedRevision { rev }) if rev == bad
    ));

    // Entries resolved at another revision are refused, not decoded
    // cross-revision.
    let entries = monitor_entries(Family::Cmis, Rev::CMIS_5_3);
    assert!(matches!(
        field_read_plan(Family::Cmis, Rev::CMIS_4_0, &entries),
        Err(FieldPlanError::EntryRevisionMismatch { .. })
    ));
    // And cross-family: an SFF entry under a CMIS plan.
    let sff = monitor_entries(
        Family::Sff8636,
        Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        },
    );
    assert!(matches!(
        field_read_plan(Family::Cmis, Rev::CMIS_5_3, &sff),
        Err(FieldPlanError::EntryRevisionMismatch { .. })
    ));

    // observe_fields applies the same gates.
    assert!(matches!(
        observe_fields(
            Family::Cmis,
            Rev::CMIS_5_3,
            &sff,
            &snapshot(&[]),
            GENERATION,
            EPOCH,
            &ObserveOptions::default()
        ),
        Err(FieldPlanError::EntryRevisionMismatch { .. })
    ));
}

// ─── Minimality and completeness against a recording view ──────────────

/// The contract, for one full snapshot: (a) a view containing only the
/// planned bytes decodes identically to the full snapshot; (b) every byte
/// observe_fields touches is inside the plan; and — where the plan has no
/// unknowable variant decision — (c) the plan is byte-exact (no byte the
/// decoders never touch).
fn assert_plan_covers(
    family: Family,
    revision: Rev,
    full: &Snapshot,
    byte_exact: bool,
) -> Vec<openxcvr_core::observation::FieldObservation> {
    let entries = monitor_entries(family, revision);
    let plan = field_read_plan(family, revision, &entries).unwrap();
    let plan_bytes = planned_bytes(&plan);

    let recording = RecordingView::new(full);
    let full_result = observe_fields(
        family,
        revision,
        &entries,
        &recording,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    let touched = recording.touched();
    let outside: Vec<_> = touched.difference(&plan_bytes).collect();
    assert!(
        outside.is_empty(),
        "{family:?} {revision:?}: bytes read outside the plan: {outside:?}"
    );

    if byte_exact {
        let extra: Vec<_> = plan_bytes.difference(&touched).collect();
        assert!(
            extra.is_empty(),
            "{family:?} {revision:?}: plan bytes never read: {extra:?}"
        );
    }

    let planned_parts: Vec<(u8, u8, u8, u8, Vec<u8>)> = plan
        .reads
        .iter()
        .filter_map(|read| {
            supplied_bytes(full, read.range, GENERATION, EPOCH)
                .ok()
                .map(|bytes| {
                    (
                        read.range.address,
                        read.range.bank,
                        read.range.page,
                        read.range.offset,
                        bytes,
                    )
                })
        })
        .collect();
    let planned_view = snapshot(&planned_parts);
    let planned_result = observe_fields(
        family,
        revision,
        &entries,
        &planned_view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        full_result, planned_result,
        "{family:?} {revision:?}: planned-only decode differs from the full snapshot"
    );
    full_result
}

#[test]
fn planned_bytes_suffice_and_cover_every_supported_family() {
    // CMIS 4.0 / 5.3, everything present: byte-exact.
    assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_4_0,
        &cmis_full_snapshot(0x40, true),
        true,
    );
    let full = assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_5_3,
        &cmis_full_snapshot(0x53, true),
        true,
    );
    // Sanity: the paged decode is valid and lane-aligned with the entries.
    assert_eq!(full.len(), 26);
    assert!(full.iter().all(|o| o.state == ObservationState::Valid));

    // CMIS flat-memory shape (no page 01h/11h): the plan's optional ranges
    // are absent from the snapshot; the decode must still match, with the
    // lane monitors NotSupplied — exactly what observe_basic yields for a
    // flat module.
    let flat = assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_5_3,
        &cmis_full_snapshot(0x53, false),
        false,
    );
    assert!(flat
        .iter()
        .filter(|o| o.lane.is_some())
        .all(|o| o.state == ObservationState::NotSupplied));
    assert!(flat
        .iter()
        .filter(|o| o.lane.is_none())
        .all(|o| o.state == ObservationState::Valid));

    // SFF-8636 0Ah (Rev 2.12): byte-exact.
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    assert_plan_covers(
        Family::Sff8636,
        rev,
        &sff8636_full_snapshot(0x0d, 0x0a),
        true,
    );

    // SFF-8472 external calibration: byte-exact (constants are read).
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    assert_plan_covers(
        Family::Sff8472,
        rev,
        &sff8472_full_snapshot(0x03, 0x10, 0x0a, true),
        true,
    );
    // SFF-8472 internal calibration: sufficient and covering, but not
    // byte-exact by design — the pure plan cannot know the discriminator
    // byte's value, so it always acquires the external-calibration
    // constants (passive static bytes the internal decode never reads).
    assert_plan_covers(
        Family::Sff8472,
        rev,
        &sff8472_full_snapshot(0x03, 0x40, 0x0a, false),
        false,
    );
}

#[test]
fn sff8636_compliance_gate_admits_0ah_and_refuses_08h_like_basic() {
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let entries = monitor_entries(Family::Sff8636, rev);

    // 0Ah: the decode is admitted.
    let view = sff8636_full_snapshot(0x0d, 0x0a);
    let observed = observe_fields(
        Family::Sff8636,
        rev,
        &entries,
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    assert!(observed.iter().all(|o| o.state == ObservationState::Valid));

    // 08h: every observation refuses with the schema revision, exactly as
    // observe_basic does for the same snapshot.
    let view = sff8636_full_snapshot(0x0d, 0x08);
    let observed = observe_fields(
        Family::Sff8636,
        rev,
        &entries,
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.len(), entries.len());
    assert!(observed
        .iter()
        .all(|o| o.state == ObservationState::UnsupportedRevision(rev)));
    assert!(observed.iter().all(|o| o.value.is_none()));
}

#[test]
fn observe_fields_results_align_with_the_entries() {
    let rev = Rev::CMIS_5_3;
    let entries = monitor_entries(Family::Cmis, rev);
    let view = cmis_full_snapshot(0x53, true);
    let observed = observe_fields(
        Family::Cmis,
        rev,
        &entries,
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(observed.len(), entries.len());
    for (entry, observation) in entries.iter().zip(&observed) {
        assert_eq!(observation.field, entry.row.name);
        assert_eq!(observation.lane, entry.lane);
    }
    // The bias advertisement is applied on the field path: lane 1's
    // 2500-count laser bias with the x1 advertisement decodes 5000 uA...
    // (fixture: 2500 counts, 2 uA LSB, advertisement 0x00 = x1).
    let bias = observed
        .iter()
        .find(|o| o.field == "laser_bias_tx" && o.lane == Some(1))
        .unwrap();
    match &bias.value {
        Some(openxcvr_core::observation::FieldValue::Scaled { value, unit }) => {
            assert!((value - 2500.0 * 2e-6).abs() < 1e-12);
            assert_eq!(unit, &openxcvr::catalog::Unit::Ampere);
        }
        other => panic!("laser bias lane 1 did not decode scaled: {other:?}"),
    }
}

#[test]
fn plan_never_plans_a_range_covering_clear_on_read_rows() {
    // Row-level invariant, every family and supported revision: no planned
    // range of the worst-case monitor plan covers a row carrying
    // ClearOnRead (the CMIS lower-page flag bytes 8..=11 among them).
    let families = [
        (
            Family::Cmis,
            vec![
                Rev::CMIS_3_0,
                Rev::CMIS_4_0,
                Rev::CMIS_5_0,
                Rev::CMIS_5_1,
                Rev::CMIS_5_2,
                Rev::CMIS_5_3,
            ],
        ),
        (
            Family::Sff8636,
            vec![Rev {
                family: Family::Sff8636,
                major: 2,
                minor: 12,
            }],
        ),
        (
            Family::Sff8472,
            vec![Rev {
                family: Family::Sff8472,
                major: 12,
                minor: 5,
            }],
        ),
    ];
    for (family, revisions) in families {
        let catalog = openxcvr::catalog::family_catalog(family).unwrap();
        for revision in revisions {
            let entries = monitor_entries(family, revision);
            let plan = field_read_plan(family, revision, &entries).unwrap();
            for planned in &plan.reads {
                for row in catalog.rows.iter() {
                    let applicable = row.since.is_none_or(|since| revision >= since)
                        && row.until.is_none_or(|until| revision < until);
                    if !applicable
                        || row.address != planned.range.address
                        || row.page != planned.range.page
                        || !row
                            .read_effects
                            .contains(&openxcvr_core::types::management::ReadEffect::ClearOnRead)
                    {
                        continue;
                    }
                    let row_start = u32::from(row.offset);
                    let row_end = row_start + u32::from(row.len);
                    let range_start = u32::from(planned.range.offset);
                    let range_end = planned.range.end();
                    assert!(
                        row_start >= range_end || range_start >= row_end,
                        "{family:?} {revision:?}: planned {:?} covers clear-on-read row {} ({}..{})",
                        planned.range,
                        row.name,
                        row_start,
                        row_end
                    );
                }
                if family == Family::Cmis {
                    assert!(
                        planned.range.page != 0
                            || planned.range.end() <= 8
                            || planned.range.offset >= 12,
                        "CMIS plan touches lower-page flag bytes: {:?}",
                        planned.range
                    );
                }
            }
        }
    }
}

#[test]
fn instanced_clear_on_read_rows_refuse_every_lane() {
    let rev = Rev::CMIS_5_1;
    // P-B3 (parent-reproduced on the C port, same classifier here):
    // `tuning_complete_flag_tx` is a per-lane clear-on-read flag whose
    // instance byte offsets (231..238) run past the row's own one-byte
    // span (231..232). Lane 1 — inside the row's own span — was refused
    // before the footprint fix alike; lanes 2..8 sit outside it and must
    // refuse ClearOnRead too: a row's effect footprint is the union of
    // its own span and every instance offset span.
    let entries = resolve_entries(
        rev,
        "tuning_complete_flag_tx",
        Some("CmisPage12hTunableLaser"),
        LaneSelection::All,
    );
    assert_eq!(entries.len(), 8);
    // N4: pin the geometry the refusal walk rests on — the row's own
    // span is one byte (231..232) and lanes 1..8 sit at exactly
    // 231..=238 — so a schema change to the row's length or the lane
    // offsets fails loudly instead of silently weakening the oracle.
    assert_eq!(
        entries[0].row.len, 1,
        "the P-B3 oracle needs a one-byte row span"
    );
    for (index, entry) in entries.iter().enumerate() {
        let lane = index as u16 + 1;
        assert_eq!(entry.lane, Some(lane), "lane order");
        assert_eq!(
            u32::from(entry.offset),
            231 + index as u32,
            "lane {lane} must sit exactly one byte past its predecessor"
        );
    }
    let plan = field_read_plan(Family::Cmis, rev, &entries).unwrap();
    assert!(
        plan.reads.is_empty(),
        "no byte may be planned for a latched flag: {:?}",
        plan.reads
    );
    assert_eq!(plan.refusals.len(), 8, "{:?}", plan.refusals);
    for (index, refusal) in plan.refusals.iter().enumerate() {
        assert_eq!(refusal.field, "tuning_complete_flag_tx");
        assert_eq!(refusal.lane, Some(index as u16 + 1));
        assert_eq!(refusal.reason, FieldPlanRefusalReason::ClearOnRead);
    }
}
