//! Library-owned read plan for the basic observation set: which exact byte
//! ranges a caller must acquire so `observe_basic` decodes everything a
//! full snapshot would. Synthetic snapshots only; no captures, no I/O.
//! The C parity test (bindings/c/tests/catalog_observation.c) mirrors the
//! same plans and expectations.
use std::cell::RefCell;
use std::collections::BTreeSet;

use openxcvr::catalog::{family_catalog, FieldQuery, LaneSelection};
use openxcvr::observation::{
    basic_read_prelude, basic_read_remaining, observe_basic, BasicPlanError, ObservationState,
    ObserveOptions, PlannedRead,
};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::ReadEffect;
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

/// Wraps a snapshot and records every byte any decoder asks for.
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

fn planned_bytes(reads: &[PlannedRead]) -> BTreeSet<(u8, u8, u8, u8)> {
    reads
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

fn read(range: Range, purpose: &'static str, optional: bool) -> PlannedRead {
    PlannedRead {
        range,
        optional,
        purpose,
    }
}

fn shape(reads: &[PlannedRead]) -> Vec<(Range, &'static str, bool)> {
    reads
        .iter()
        .map(|r| (r.range, r.purpose, r.optional))
        .collect()
}

// ─── Fixtures (mirror obs_basic_catalog.rs) ────────────────────────────

fn cmis_full_snapshot(revision_byte: u8, with_optional_pages: bool) -> Snapshot {
    let mut lower = vec![0u8; 18];
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

fn sff8636_full_snapshot(compliance: u8) -> Snapshot {
    let mut lower = vec![0u8; 128];
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

/// SFF-8472 fixture with A0h (92/94), A2h diagnostics and — when
/// `with_constants` — the external-calibration constants.
fn sff8472_full_snapshot(dmt_byte: u8, compliance: u8, with_constants: bool) -> Snapshot {
    let mut a0 = vec![0u8; 96];
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

// ─── Prelude shapes ────────────────────────────────────────────────────

#[test]
fn prelude_shapes_are_catalog_derived() {
    // CMIS: the lower-memory revision byte (cmis_revision row), for every
    // supported revision.
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let prelude = basic_read_prelude(Family::Cmis, revision).unwrap();
        assert_eq!(
            shape(&prelude),
            vec![(
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 1,
                    len: 1
                },
                "cmis_revision",
                false
            )],
            "{revision:?}"
        );
    }

    // SFF-8636: the lower-page revision-compliance byte.
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    let prelude = basic_read_prelude(Family::Sff8636, rev).unwrap();
    assert_eq!(
        shape(&prelude),
        vec![(
            Range {
                address: 0x50,
                bank: 0,
                page: 0,
                offset: 1,
                len: 1
            },
            "revision_compliance",
            false
        )]
    );

    // SFF-8472: the A0h compliance byte, then the external-calibration
    // discriminator (the monitor rows' VariantDiscriminator loc, A0h byte
    // 92 bit 4 `externally_calibrated`) — both from the catalog.
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let prelude = basic_read_prelude(Family::Sff8472, rev).unwrap();
    assert_eq!(
        shape(&prelude),
        vec![
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
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 92,
                    len: 1
                },
                "page_a0h_base_memory.externally_calibrated",
                false
            ),
        ]
    );
}

#[test]
fn prelude_refuses_unsupported_family_and_revision() {
    assert!(matches!(
        basic_read_prelude(Family::Elsfp, Rev::CMIS_5_3),
        Err(BasicPlanError::UnsupportedFamily(_))
    ));
    // SFF-8636 has exactly one supported revision.
    let bad = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 10,
    };
    assert!(matches!(
        basic_read_prelude(Family::Sff8636, bad),
        Err(BasicPlanError::UnsupportedRevision { rev }) if rev == bad
    ));
    // A CMIS revision requested for the SFF-8472 family is refused by the
    // family's own table, not decoded cross-family.
    assert!(matches!(
        basic_read_prelude(Family::Sff8472, Rev::CMIS_5_3),
        Err(BasicPlanError::UnsupportedRevision { .. })
    ));
}

// ─── Remaining shapes ──────────────────────────────────────────────────

#[test]
fn cmis_remaining_shape_all_supported_revisions() {
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let byte = match revision {
            Rev::CMIS_3_0 => 0x30,
            Rev::CMIS_4_0 => 0x40,
            Rev::CMIS_5_0 => 0x50,
            Rev::CMIS_5_1 => 0x51,
            Rev::CMIS_5_2 => 0x52,
            Rev::CMIS_5_3 => 0x53,
            _ => unreachable!(),
        };
        let view = snapshot(&[(0x50, 0, 0, 1, vec![byte])]);
        let remaining = basic_read_remaining(Family::Cmis, revision, &view, GENERATION, EPOCH)
            .unwrap_or_else(|e| panic!("{revision:?}: {e:?}"));
        assert_eq!(
            shape(&remaining),
            vec![
                // Lower page: module-scope monitors, mandatory.
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
                ),
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0,
                        offset: 16,
                        len: 2
                    },
                    "vcc_mon_voltage",
                    false
                ),
                // Page 11h bank 0: lane monitors, optional — a flat-memory
                // module (memory_model, CMIS lower byte 2 bit 7 = 1b, "Flat
                // memory (Page 00h supported only)") implements neither
                // page 01h nor 11h, and the decode yields NotSupplied.
                // Order follows monitors(): temp, supply, bias, tx, rx.
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0x11,
                        offset: 170,
                        len: 16
                    },
                    "laser_bias_tx",
                    true
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
                ),
                (
                    Range {
                        address: 0x50,
                        bank: 0,
                        page: 0x11,
                        offset: 186,
                        len: 16
                    },
                    "optical_power_rx",
                    true
                ),
                // Page 01h byte 160: TxBiasCurrentScalingFactor
                // advertisement, optional for the same flat-memory reason.
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
            ],
            "{revision:?}"
        );
    }
}

#[test]
fn sff8636_remaining_shape() {
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    // All basic-set bytes live in the lower page, which every module has
    // (SFF-8636 flat_memory, byte 2 bit 2, still guarantees the lower page).
    let view = sff8636_full_snapshot(0x0a);
    let remaining = basic_read_remaining(Family::Sff8636, rev, &view, GENERATION, EPOCH).unwrap();
    assert_eq!(
        shape(&remaining),
        shape(&[
            read(
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
            read(
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
            read(
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 42,
                    len: 8
                },
                "tx_bias_lane",
                false
            ),
            read(
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 50,
                    len: 8
                },
                "tx_power_lane",
                false
            ),
            read(
                Range {
                    address: 0x50,
                    bank: 0,
                    page: 0,
                    offset: 34,
                    len: 8
                },
                "rx_power_lane",
                false
            ),
        ])
    );
}

#[test]
fn sff8472_remaining_internal_and_external() {
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    let monitors = |address: u8| {
        vec![
            read(
                Range {
                    address,
                    bank: 0,
                    page: 0,
                    offset: 96,
                    len: 2,
                },
                "temperature",
                false,
            ),
            read(
                Range {
                    address,
                    bank: 0,
                    page: 0,
                    offset: 98,
                    len: 2,
                },
                "supply_voltage",
                false,
            ),
            read(
                Range {
                    address,
                    bank: 0,
                    page: 0,
                    offset: 100,
                    len: 2,
                },
                "tx_bias",
                false,
            ),
            read(
                Range {
                    address,
                    bank: 0,
                    page: 0,
                    offset: 102,
                    len: 2,
                },
                "tx_power",
                false,
            ),
            read(
                Range {
                    address,
                    bank: 0,
                    page: 0,
                    offset: 104,
                    len: 2,
                },
                "rx_power",
                false,
            ),
        ]
    };

    // Internal calibration (bit 4 clear): the A2h monitors only.
    let view = sff8472_full_snapshot(0x40, 0x0a, false);
    let remaining = basic_read_remaining(Family::Sff8472, rev, &view, GENERATION, EPOCH).unwrap();
    assert_eq!(shape(&remaining), shape(&monitors(0x51)));

    // External calibration (bit 4 set): the monitors plus the declared
    // external-calibration constant locations, per field, in declaration
    // order (slope/offset pairs; the polynomial's five coefficients). The
    // purpose names the field the constants serve (the catalog row's name).
    let view = sff8472_full_snapshot(0x10, 0x0a, true);
    let remaining = basic_read_remaining(Family::Sff8472, rev, &view, GENERATION, EPOCH).unwrap();
    // Each field is followed immediately by its own constants, in the
    // declaration order the schema gives (slope then offset; the
    // polynomial's five coefficients in array order).
    let field = |address: u8, offset: u8, len: u16, purpose: &'static str| {
        read(
            Range {
                address,
                bank: 0,
                page: 0,
                offset,
                len,
            },
            purpose,
            false,
        )
    };
    let expected = vec![
        field(0x51, 96, 2, "temperature"),
        field(0x51, 84, 2, "temperature"),
        field(0x51, 86, 2, "temperature"),
        field(0x51, 98, 2, "supply_voltage"),
        field(0x51, 88, 2, "supply_voltage"),
        field(0x51, 90, 2, "supply_voltage"),
        field(0x51, 100, 2, "tx_bias"),
        field(0x51, 76, 2, "tx_bias"),
        field(0x51, 78, 2, "tx_bias"),
        field(0x51, 102, 2, "tx_power"),
        field(0x51, 80, 2, "tx_power"),
        field(0x51, 82, 2, "tx_power"),
        field(0x51, 104, 2, "rx_power"),
        field(0x51, 72, 4, "rx_power"),
        field(0x51, 68, 4, "rx_power"),
        field(0x51, 64, 4, "rx_power"),
        field(0x51, 60, 4, "rx_power"),
        field(0x51, 56, 4, "rx_power"),
    ];
    assert_eq!(shape(&remaining), shape(&expected));
}

// ─── Minimality and completeness against a recording view ──────────────

/// The contract, for one full snapshot: (a) a view containing only the
/// planned bytes decodes identically to the full snapshot; (b) every byte
/// the decoders touch is inside the plan; and — when everything the plan
/// can request is present — the plan is byte-exact (no byte the decoders
/// never touch).
fn assert_plan_covers(family: Family, revision: Rev, full: &Snapshot, byte_exact: bool) {
    let prelude = basic_read_prelude(family, revision).unwrap();
    let remaining =
        basic_read_remaining(family, revision, full, GENERATION, EPOCH).unwrap_or_else(|e| {
            panic!("{family:?} {revision:?}: remaining refused: {e:?}");
        });
    let mut plan: Vec<PlannedRead> = prelude.clone();
    plan.extend(remaining);
    let plan_bytes = planned_bytes(&plan);

    // (b) completeness: record every byte the decoders ask for — the
    // observe_basic set, and the observe_field path over the same five
    // monitors (every lane), which observe_basic drives internally but
    // is also a public entry point of its own.
    let recording = RecordingView::new(full);
    let full_result = observe_basic(
        family,
        revision,
        &recording,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    let field_recording = RecordingView::new(full);
    for (page, name) in monitor_name_pairs(family) {
        let mut query = FieldQuery {
            pattern: name,
            lane: LaneSelection::All,
            page: Some(page),
            bank: 0,
        };
        // SFF-8472's monitors are single-lane (no instance dimension):
        // the whole field is the entry.
        if matches!(
            openxcvr::catalog::resolve(revision, &query),
            Err(openxcvr_core::field_catalog::CatalogError::NotInstanced { .. })
        ) {
            query.lane = LaneSelection::Unspecified;
        }
        for entry in openxcvr::catalog::resolve(revision, &query).unwrap() {
            let _ = openxcvr::observation::observe_field(
                &entry,
                &field_recording,
                GENERATION,
                EPOCH,
                &ObserveOptions::default(),
            );
        }
    }
    let mut touched = recording.touched();
    touched.extend(field_recording.touched());
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

    // (a) sufficiency: slice a snapshot with only the planned bytes.
    let planned_parts: Vec<(u8, u8, u8, u8, Vec<u8>)> = plan
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
    let planned_result = observe_basic(
        family,
        revision,
        &planned_view,
        GENERATION,
        EPOCH,
        ObserveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        full_result, planned_result,
        "{family:?} {revision:?}: planned-only decode differs from the full snapshot"
    );
}

#[test]
fn planned_bytes_suffice_and_cover_every_supported_family() {
    // CMIS: two supported revisions, everything present -> byte-exact.
    assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_4_0,
        &cmis_full_snapshot(0x40, true),
        true,
    );
    assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_5_3,
        &cmis_full_snapshot(0x53, true),
        true,
    );
    // CMIS flat-memory shape (no page 01h/11h): the plan's optional ranges
    // are absent from the snapshot; the decode must still match.
    assert_plan_covers(
        Family::Cmis,
        Rev::CMIS_5_3,
        &cmis_full_snapshot(0x53, false),
        false,
    );
    // SFF-8636 0Ah (Rev 2.12): byte-exact.
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    assert_plan_covers(Family::Sff8636, rev, &sff8636_full_snapshot(0x0a), true);
    // SFF-8472 0Ah internal and external calibration: byte-exact.
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    assert_plan_covers(
        Family::Sff8472,
        rev,
        &sff8472_full_snapshot(0x40, 0x0a, false),
        true,
    );
    assert_plan_covers(
        Family::Sff8472,
        rev,
        &sff8472_full_snapshot(0x10, 0x0a, true),
        true,
    );
}

// ─── Refusals ──────────────────────────────────────────────────────────

fn monitor_name_pairs(family: Family) -> &'static [(&'static str, &'static str)] {
    match family {
        Family::Cmis => &[
            ("CmisLowerMemory", "temp_mon_value"),
            ("CmisLowerMemory", "vcc_mon_voltage"),
            ("CmisPage11hLaneStatus", "laser_bias_tx"),
            ("CmisPage11hLaneStatus", "optical_power_tx"),
            ("CmisPage11hLaneStatus", "optical_power_rx"),
        ],
        Family::Sff8636 => &[
            ("Sff8636LowerMemory", "temperature"),
            ("Sff8636LowerMemory", "supply_voltage"),
            ("Sff8636LowerMemory", "tx_bias_lane"),
            ("Sff8636LowerMemory", "tx_power_lane"),
            ("Sff8636LowerMemory", "rx_power_lane"),
        ],
        Family::Sff8472 => &[
            ("page_a2h_diagnostics_base", "temperature"),
            ("page_a2h_diagnostics_base", "supply_voltage"),
            ("page_a2h_diagnostics_base", "tx_bias"),
            ("page_a2h_diagnostics_base", "tx_power"),
            ("page_a2h_diagnostics_base", "rx_power"),
        ],
        _ => unreachable!(),
    }
}

fn monitor_field_ranges(family: Family, revision: Rev) -> Vec<Range> {
    // Resolve each of the five monitor names through the public resolver
    // (the same rows observe_basic uses) and take every instance's range.
    let mut ranges = Vec::new();
    for (page, name) in monitor_name_pairs(family) {
        let mut query = FieldQuery {
            pattern: name,
            lane: LaneSelection::All,
            page: Some(page),
            bank: 0,
        };
        // SFF-8472's monitors are single-lane (no instance dimension):
        // the whole field is its range.
        if matches!(
            openxcvr::catalog::resolve(revision, &query),
            Err(openxcvr_core::field_catalog::CatalogError::NotInstanced { .. })
        ) {
            query.lane = LaneSelection::Unspecified;
        }
        for entry in openxcvr::catalog::resolve(revision, &query).unwrap() {
            ranges.push(entry.range());
        }
    }
    ranges
}

fn ranges_intersect(a: Range, b: Range) -> bool {
    a.address == b.address
        && a.bank == b.bank
        && a.page == b.page
        && u32::from(a.offset) < b.end()
        && u32::from(b.offset) < a.end()
}

#[test]
fn refused_compliance_plans_no_monitor_bytes() {
    let rev8636 = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    for code in [0x08u8, 0x07, 0x00, 0x09, 0x0b, 0xff] {
        let view = sff8636_full_snapshot(code);
        let err =
            basic_read_remaining(Family::Sff8636, rev8636, &view, GENERATION, EPOCH).unwrap_err();
        assert!(
            matches!(&err, BasicPlanError::ComplianceRefused { code: c, .. } if *c == code),
            "code {code:#04x}: {err:?}"
        );
        // The total plan (prelude; the remainder refused) names no monitor
        // byte: only the compliance byte is needed to report the refusal.
        let prelude = basic_read_prelude(Family::Sff8636, rev8636).unwrap();
        for monitor in monitor_field_ranges(Family::Sff8636, rev8636) {
            for planned in &prelude {
                assert!(
                    !ranges_intersect(planned.range, monitor),
                    "code {code:#04x}: prelude {planned:?} overlaps monitor {monitor:?}"
                );
            }
        }
        // Consistency: observe_basic on the same bytes refuses the decode
        // (no values) yet still reports the code and its meaning.
        let observed = observe_basic(
            Family::Sff8636,
            rev8636,
            &view,
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap();
        assert_eq!(
            observed.temperature.state,
            ObservationState::UnsupportedRevision(rev8636),
            "code {code:#04x}"
        );
        assert_eq!(observed.reported_compliance.map(|c| c.code), Some(code));
    }

    let rev8472 = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    for code in [0x09u8, 0x08, 0x00, 0x0b, 0xff] {
        let view = sff8472_full_snapshot(0x40, code, false);
        let err =
            basic_read_remaining(Family::Sff8472, rev8472, &view, GENERATION, EPOCH).unwrap_err();
        assert!(
            matches!(&err, BasicPlanError::ComplianceRefused { code: c, .. } if *c == code),
            "code {code:#04x}: {err:?}"
        );
        let prelude = basic_read_prelude(Family::Sff8472, rev8472).unwrap();
        for monitor in monitor_field_ranges(Family::Sff8472, rev8472) {
            for planned in &prelude {
                assert!(
                    !ranges_intersect(planned.range, monitor),
                    "code {code:#04x}: prelude {planned:?} overlaps monitor {monitor:?}"
                );
            }
        }
        let observed = observe_basic(
            Family::Sff8472,
            rev8472,
            &view,
            GENERATION,
            EPOCH,
            ObserveOptions::default(),
        )
        .unwrap();
        assert_eq!(
            observed.temperature.state,
            ObservationState::UnsupportedRevision(rev8472),
            "code {code:#04x}"
        );
    }
}

#[test]
fn missing_prelude_refuses_with_the_missing_range() {
    // CMIS: nothing at byte 1.
    let view = snapshot(&[(0x50, 0, 0, 2, vec![0u8; 16])]);
    assert!(matches!(
        basic_read_remaining(Family::Cmis, Rev::CMIS_5_3, &view, GENERATION, EPOCH),
        Err(BasicPlanError::PreludeMissing { range }) if range.offset == 1
    ));

    // SFF-8636: compliance byte absent.
    let view = snapshot(&[(0x50, 0, 0, 2, vec![0u8; 126])]);
    let rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    assert!(matches!(
        basic_read_remaining(Family::Sff8636, rev, &view, GENERATION, EPOCH),
        Err(BasicPlanError::PreludeMissing { range }) if range.offset == 1
    ));

    // SFF-8472: compliance byte present, discriminator byte absent.
    let mut a0 = vec![0u8; 94];
    a0[94 - 1] = 0x0a; // byte 93 is not byte 94; the view stops at 93
    a0[92] = 0x40;
    let view = snapshot(&[(0x50, 0, 0, 0, a0)]);
    let rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    assert!(matches!(
        basic_read_remaining(Family::Sff8472, rev, &view, GENERATION, EPOCH),
        Err(BasicPlanError::PreludeMissing { range }) if range.offset == 94
    ));
    // Byte 94 present, byte 92 absent.
    let mut a0 = [0u8; 96];
    a0[92] = 0;
    a0[94] = 0x0a;
    let view = snapshot(&[(0x50, 0, 0, 93, a0[93..].to_vec())]);
    assert!(matches!(
        basic_read_remaining(Family::Sff8472, rev, &view, GENERATION, EPOCH),
        Err(BasicPlanError::PreludeMissing { range }) if range.offset == 92
    ));
}

#[test]
fn cmis_revision_byte_gates_remaining_like_observe_basic() {
    // Unsupported future revision byte.
    let view = cmis_full_snapshot(0x55, true);
    assert!(matches!(
        basic_read_remaining(Family::Cmis, Rev::CMIS_5_3, &view, GENERATION, EPOCH),
        Err(BasicPlanError::UnsupportedRevision { .. })
    ));
    // Byte disagreeing with the requested revision.
    let view = cmis_full_snapshot(0x40, true);
    assert!(matches!(
        basic_read_remaining(Family::Cmis, Rev::CMIS_5_3, &view, GENERATION, EPOCH),
        Err(BasicPlanError::RevisionMismatch { .. })
    ));
}

// ─── Clear-on-read exclusion, by construction and by test ──────────────

#[test]
fn plan_never_includes_clear_on_read_bytes_for_any_supported_revision() {
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
        let catalog = family_catalog(family).unwrap();
        for revision in revisions {
            // The worst case plan: external calibration for SFF-8472 (the
            // largest footprint), everything else's admitted shape.
            let prelude = basic_read_prelude(family, revision).unwrap();
            let remaining = match family {
                Family::Cmis => basic_read_remaining(
                    family,
                    revision,
                    &snapshot(&[(0x50, 0, 0, 1, vec![cmis_rev_byte(revision)])]),
                    GENERATION,
                    EPOCH,
                )
                .unwrap(),
                Family::Sff8636 => basic_read_remaining(
                    family,
                    revision,
                    &sff8636_full_snapshot(0x0a),
                    GENERATION,
                    EPOCH,
                )
                .unwrap(),
                _ => basic_read_remaining(
                    family,
                    revision,
                    &sff8472_full_snapshot(0x10, 0x0a, true),
                    GENERATION,
                    EPOCH,
                )
                .unwrap(),
            };
            let plan: Vec<&PlannedRead> = prelude.iter().chain(remaining.iter()).collect();

            // Row-level: no row the plan derives a range from is COR.
            for planned in &plan {
                let covered = catalog
                    .rows
                    .iter()
                    .filter(|row| applicable(row, revision))
                    .filter(|row| {
                        row.address == planned.range.address && row.page == planned.range.page
                    })
                    .filter(|row| overlaps(row, planned.range))
                    .any(|row| row.read_effects.contains(&ReadEffect::ClearOnRead));
                assert!(
                    !covered,
                    "{family:?} {revision:?}: planned range {:?} covers a clear-on-read row",
                    planned.range
                );
            }

            // Byte-level: none of the plan's bytes is a COR byte — the CMIS
            // lower-page flag bytes 8..=11 in particular.
            if family == Family::Cmis {
                for planned in &plan {
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

fn cmis_rev_byte(revision: Rev) -> u8 {
    match revision {
        Rev::CMIS_3_0 => 0x30,
        Rev::CMIS_4_0 => 0x40,
        Rev::CMIS_5_0 => 0x50,
        Rev::CMIS_5_1 => 0x51,
        Rev::CMIS_5_2 => 0x52,
        Rev::CMIS_5_3 => 0x53,
        _ => unreachable!(),
    }
}

fn applicable(row: &openxcvr_core::field_catalog::CatalogRow, revision: Rev) -> bool {
    row.since.is_none_or(|since| revision >= since)
        && row.until.is_none_or(|until| revision < until)
}

fn overlaps(row: &openxcvr_core::field_catalog::CatalogRow, range: Range) -> bool {
    // One byte of overlap is enough; banked pages ride bank 0 here.
    let row_start = u32::from(row.offset);
    let row_end = row_start + u32::from(row.len);
    let range_start = u32::from(range.offset);
    let range_end = range.end();
    row_start < range_end && range_start < row_end
}
