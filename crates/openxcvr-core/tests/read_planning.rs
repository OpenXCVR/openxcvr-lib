// Red-first behaviour pins for the bounded R1 read-planning seam
// (`types::management::ReadFootprint`, `plan_observation`, `assemble_snapshot`).
//
// Scope: CMIS module memory at device address 0x50 — lower memory 0x00–0x7F
// and upper page 00h (bank 0) 0x80–0xFF — read-only, no selector writes.
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::*;

const GEN: ModuleGeneration = ModuleGeneration(7);
const EPOCH: CacheEpoch = CacheEpoch(9);

fn revision() -> Rev {
    Rev::CMIS_5_3
}

fn other_revision() -> Rev {
    Rev {
        family: Family::Cmis,
        major: 5,
        minor: 2,
    }
}

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

fn passive() -> ByteClassification {
    ByteClassification::from_masks(
        0xff,
        0x00,
        0x00,
        0x00,
        &[ReadEffect::None],
        ChangeSource::Static,
    )
}

fn passive_table() -> [ByteClassification; 128] {
    std::array::from_fn(|_| passive())
}

/// A footprint with `f` applied over the lower table, upper otherwise passive.
fn footprint_with(f: impl FnOnce(&mut [ByteClassification; 128])) -> ReadFootprint {
    let mut lower = passive_table();
    f(&mut lower);
    ReadFootprint {
        revision: revision(),
        address: 0x50,
        lower,
        upper_page00: passive_table(),
    }
}

fn footprint() -> ReadFootprint {
    footprint_with(|_| {})
}

fn unlimited() -> ReadLimits {
    ReadLimits {
        max_ranges: usize::MAX,
        max_bytes: u32::MAX,
        max_transfer_len: 128,
    }
}

fn request(ranges: Vec<Range>) -> PlanObservationInput {
    PlanObservationInput {
        revision: revision(),
        requester: ReadRequester::PassiveInspection,
        ranges,
        limits: unlimited(),
        selector: Some(Selection {
            address: 0x50,
            bank: 0,
            page: 0,
        }),
        flat_memory: None,
        generation: GEN,
        epoch: EPOCH,
    }
}

fn transfer(step_index: usize, bytes: Vec<u8>) -> TransferResult {
    let physical_range = range(0, bytes.len() as u16);
    transfer_at(step_index, physical_range, bytes)
}

fn transfer_at(step_index: usize, physical_range: Range, bytes: Vec<u8>) -> TransferResult {
    TransferResult {
        step_index,
        range: physical_range,
        generation: GEN,
        epoch: EPOCH,
        bytes,
    }
}

fn assemble(plan: &ReadPlan, results: &[TransferResult]) -> Result<Snapshot, AssembleRefusal> {
    assemble_snapshot(&AssembleInput {
        plan,
        results,
        generation: GEN,
        epoch: EPOCH,
        started_at: MonoTime(10),
        completed_at: MonoTime(11),
    })
}

// ── classification ──────────────────────────────────────────────────────────

#[test]
fn a_clear_on_read_byte_refuses_its_range_with_the_byte_named() {
    let footprint = footprint_with(|lower| {
        lower[2] = ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::ClearOnRead],
            ChangeSource::Dynamic,
        );
    });
    assert_eq!(
        footprint.classify_range(range(0, 4)),
        Ok(RangeClassified::NonPassive {
            byte: 2,
            reason: NonPassiveReason::ClearOnRead
        })
    );
    assert_eq!(
        plan_observation(&footprint, &request(vec![range(0, 4)])),
        Err(PlanRefusal::NonPassive {
            byte: 2,
            reason: NonPassiveReason::ClearOnRead
        })
    );
}

#[test]
fn an_unknown_access_byte_is_refused() {
    let footprint = footprint_with(|lower| {
        lower[3] =
            ByteClassification::from_masks(0x00, 0xff, 0x00, 0x00, &[], ChangeSource::Dynamic);
    });
    assert_eq!(
        plan_observation(&footprint, &request(vec![range(0, 8)])),
        Err(PlanRefusal::NonPassive {
            byte: 3,
            reason: NonPassiveReason::ReadEffectUnknown
        })
    );
}

#[test]
fn an_absent_metadata_byte_is_refused() {
    let footprint = footprint_with(|lower| {
        lower[10] =
            ByteClassification::from_masks(0x00, 0x00, 0xff, 0x00, &[], ChangeSource::Static);
    });
    assert_eq!(
        plan_observation(&footprint, &request(vec![range(8, 8)])),
        Err(PlanRefusal::NonPassive {
            byte: 10,
            reason: NonPassiveReason::MetadataAbsent
        })
    );
}

#[test]
fn a_conflicting_metadata_byte_is_refused() {
    let footprint = footprint_with(|lower| {
        // Two fields disagree about the same byte's read effect.
        lower[5] = ByteClassification::from_masks(
            0xff,
            0x00,
            0x00,
            0x00,
            &[ReadEffect::None, ReadEffect::ClearOnRead],
            ChangeSource::Dynamic,
        );
    });
    assert_eq!(
        plan_observation(&footprint, &request(vec![range(0, 8)])),
        Err(PlanRefusal::NonPassive {
            byte: 5,
            reason: NonPassiveReason::Conflict
        })
    );
}

#[test]
fn a_fully_passive_lower_range_classifies_passive() {
    assert_eq!(
        footprint().classify_range(range(0, 128)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Static
        })
    );
}

// ── footprint coverage and revision ─────────────────────────────────────────

#[test]
fn a_range_beyond_page_00h_is_refused() {
    let mut beyond_page = range(0x90, 8);
    beyond_page.page = 1;
    assert_eq!(
        plan_observation(&footprint(), &request(vec![beyond_page])),
        Err(PlanRefusal::Uncovered { range: beyond_page })
    );
    let mut beyond_bank = range(0x90, 8);
    beyond_bank.bank = 1;
    assert_eq!(
        plan_observation(&footprint(), &request(vec![beyond_bank])),
        Err(PlanRefusal::Uncovered { range: beyond_bank })
    );
    let mut other_address = range(0, 8);
    other_address.address = 0x51;
    assert_eq!(
        plan_observation(&footprint(), &request(vec![other_address])),
        Err(PlanRefusal::Uncovered {
            range: other_address
        })
    );
}

#[test]
fn an_unsupported_revision_is_refused() {
    let mut input = request(vec![range(0, 8)]);
    input.revision = other_revision();
    assert_eq!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::UnsupportedRevision(other_revision()))
    );
}

// ── selector policy ──────────────────────────────────────────────────────────

#[test]
fn an_upper_page00_range_is_planned_with_a_bank0_page0_selector_statement() {
    let mut input = request(vec![range(0x90, 16)]);
    input.selector = Some(Selection {
        address: 0x50,
        bank: 0,
        page: 0,
    });
    let plan = plan_observation(&footprint(), &input).unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].physical_range, range(0x90, 16));
    assert_eq!(
        plan.result_mapping,
        vec![ResultMapping {
            requested_range: range(0x90, 16),
            step_index: 0,
            offset_in_step: 0,
        }]
    );
    // Without any statement the range is refused: plans never contain a
    // selector write.
    let mut unstated = request(vec![range(0x90, 16)]);
    unstated.selector = None;
    assert_eq!(
        plan_observation(&footprint(), &unstated),
        Err(PlanRefusal::SelectorChangeRequired { bank: 0, page: 0 })
    );
    // A statement naming another selector is equally refused.
    let mut elsewhere = request(vec![range(0x90, 16)]);
    elsewhere.selector = Some(Selection {
        address: 0x50,
        bank: 1,
        page: 1,
    });
    assert_eq!(
        plan_observation(&footprint(), &elsewhere),
        Err(PlanRefusal::SelectorChangeRequired { bank: 0, page: 0 })
    );
    // A module reporting flat memory needs no selector statement.
    let mut flat = request(vec![range(0x90, 16)]);
    flat.flat_memory = Some(true);
    flat.selector = None;
    assert!(plan_observation(&footprint(), &flat).is_ok());
}

// ── transfer bounds ──────────────────────────────────────────────────────────

#[test]
fn a_range_spanning_the_half_boundary_never_spans_one_transfer() {
    // 0x70..0x90 straddles the 128-byte boundary between lower memory and
    // upper page 00h: two transfers, each inside one half.
    let plan = plan_observation(&footprint(), &request(vec![range(0x70, 0x20)])).unwrap();
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| s.physical_range)
            .collect::<Vec<_>>(),
        vec![range(0x70, 16), range(0x80, 16)]
    );
}

#[test]
fn the_input_transfer_limit_bounds_every_transfer() {
    let mut input = request(vec![range(0, 40)]);
    input.limits.max_transfer_len = 16;
    let plan = plan_observation(&footprint(), &input).unwrap();
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| (s.physical_range.offset, s.physical_range.len))
            .collect::<Vec<_>>(),
        vec![(0, 16), (16, 16), (32, 8)]
    );
}

#[test]
fn the_input_range_and_byte_limits_are_enforced() {
    let mut input = request(vec![range(0, 8), range(16, 8)]);
    input.limits.max_ranges = 1;
    assert_eq!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::TooManyRanges {
            requested: 2,
            limit: 1
        })
    );
    let mut input = request(vec![range(0, 8), range(16, 8)]);
    input.limits.max_bytes = 15;
    assert_eq!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::TooManyBytes {
            requested: 16,
            limit: 15
        })
    );
}

#[test]
fn malformed_overlapping_and_empty_requests_are_refused() {
    let malformed = Range {
        len: 0,
        ..range(0, 8)
    };
    assert_eq!(
        plan_observation(&footprint(), &request(vec![malformed])),
        Err(PlanRefusal::MalformedRange(malformed))
    );
    assert_eq!(
        plan_observation(&footprint(), &request(vec![range(16, 32), range(0, 17)])),
        Err(PlanRefusal::OverlappingRanges)
    );
    assert_eq!(
        plan_observation(&footprint(), &request(vec![])),
        Err(PlanRefusal::NoRanges)
    );
}

#[test]
fn the_plan_carries_the_input_generation_and_epoch() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    assert_eq!(plan.required_generation, GEN);
    assert_eq!(plan.required_epoch, EPOCH);
    assert_eq!(plan.steps[0].requester, ReadRequester::PassiveInspection);
    assert_eq!(plan.steps[0].declared_effects, vec![ReadEffect::None]);
    assert!(plan.steps[0]
        .classifications
        .iter()
        .all(ByteClassification::is_qualified_none));
}

// ── assembly ─────────────────────────────────────────────────────────────────

#[test]
fn a_fully_passive_lower_range_is_planned_and_assembled() {
    let mut lower_only = request(vec![range(0x10, 16)]);
    lower_only.selector = None;
    let plan = plan_observation(&footprint(), &lower_only).unwrap();
    let bytes: Vec<u8> = (0x10u8..0x20).collect();
    let snapshot = assemble(&plan, &[transfer_at(0, range(0x10, 16), bytes.clone())]).unwrap();
    assert_eq!(snapshot.ranges.len(), 1);
    assert_eq!(snapshot.ranges[0].range, range(0x10, 16));
    assert_eq!(snapshot.ranges[0].outcome, RangeOutcome::Bytes(bytes));
    assert_eq!(snapshot.generation, GEN);
    assert_eq!(snapshot.epoch, EPOCH);
    assert!(snapshot.validate().is_ok());
}

#[test]
fn a_genuinely_supplied_zero_byte_is_not_a_missing_byte() {
    // Two ranges, one whose transfer supplies a real 0x00 byte and one whose
    // transfer was never reported: served zero stays served, missing stays
    // missing, and nothing is zero-filled.
    let plan = plan_observation(&footprint(), &request(vec![range(0, 1), range(8, 1)])).unwrap();
    let snapshot = assemble(&plan, &[transfer(0, vec![0x00])]).unwrap();
    assert_eq!(snapshot.ranges[0].outcome, RangeOutcome::Bytes(vec![0x00]));
    assert_eq!(snapshot.ranges[1].outcome, RangeOutcome::Absent);
}

#[test]
fn a_short_transfer_is_a_typed_refusal() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 16)])).unwrap();
    assert_eq!(
        assemble(&plan, &[transfer(0, vec![0u8; 15])]),
        Err(AssembleRefusal::TransferLength {
            index: 0,
            expected: 16,
            supplied: 15
        })
    );
    // A transfer longer than its step is equally mismatched.
    assert_eq!(
        assemble(&plan, &[transfer(0, vec![0u8; 17])]),
        Err(AssembleRefusal::TransferLength {
            index: 0,
            expected: 16,
            supplied: 17
        })
    );
}

#[test]
fn cross_generation_and_cross_epoch_assembly_are_refused() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    let mut next_generation = transfer(0, vec![0u8; 8]);
    next_generation.generation = ModuleGeneration(8);
    assert_eq!(
        assemble(&plan, &[next_generation]),
        Err(AssembleRefusal::StampMismatch { index: 0 })
    );
    let mut next_epoch = transfer(0, vec![0u8; 8]);
    next_epoch.epoch = CacheEpoch(10);
    assert_eq!(
        assemble(&plan, &[next_epoch]),
        Err(AssembleRefusal::StampMismatch { index: 0 })
    );
}

#[test]
fn results_outside_the_plan_are_refused() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    assert_eq!(
        assemble(&plan, &[transfer(1, vec![0u8; 8])]),
        Err(AssembleRefusal::UnknownStep { index: 1 })
    );
    assert_eq!(
        assemble(
            &plan,
            &[transfer(0, vec![0u8; 8]), transfer(0, vec![0u8; 8])]
        ),
        Err(AssembleRefusal::DuplicateStep { index: 0 })
    );
}

#[test]
fn a_split_range_assembles_from_its_own_transfers_in_order() {
    // 0x70..0x90 plans two transfers; supplying only one leaves the range
    // missing, supplying both serves it end to end.
    let plan = plan_observation(&footprint(), &request(vec![range(0x70, 0x20)])).unwrap();
    let lower: Vec<u8> = (0x70u8..0x80).collect();
    let upper: Vec<u8> = (0x00u8..0x10).collect();
    let partial = assemble(&plan, &[transfer_at(0, range(0x70, 16), lower.clone())]).unwrap();
    assert_eq!(partial.ranges[0].outcome, RangeOutcome::Absent);
    let full = assemble(
        &plan,
        &[
            transfer_at(0, range(0x70, 16), lower.clone()),
            transfer_at(1, range(0x80, 16), upper.clone()),
        ],
    )
    .unwrap();
    assert_eq!(
        full.ranges[0].outcome,
        RangeOutcome::Bytes([lower, upper].concat())
    );
}

#[test]
fn a_same_length_result_from_another_range_is_refused() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    let mut result = transfer(0, vec![1; 8]);
    result.range = range(8, 8);
    assert_eq!(
        assemble(&plan, &[result]),
        Err(AssembleRefusal::RangeMismatch { index: 0 })
    );
}

#[test]
fn a_retired_plan_stamp_is_refused_even_if_its_transfers_match() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    let results = [transfer(0, vec![1; 8])];
    let mut input = AssembleInput {
        plan: &plan,
        results: &results,
        generation: ModuleGeneration(8),
        epoch: EPOCH,
        started_at: MonoTime(10),
        completed_at: MonoTime(11),
    };
    assert_eq!(
        assemble_snapshot(&input),
        Err(AssembleRefusal::PlanStampMismatch)
    );
    input.generation = GEN;
    input.epoch = CacheEpoch(10);
    assert_eq!(
        assemble_snapshot(&input),
        Err(AssembleRefusal::PlanStampMismatch)
    );
}

#[test]
fn malformed_public_plans_are_refused_not_panicked_or_zero_filled() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 8)])).unwrap();
    for mutate in [0, 1, 2, 3] {
        let mut bad = plan.clone();
        match mutate {
            0 => bad.result_mapping[0].step_index = 4,
            1 => bad.steps[0].physical_range = range(4, 8),
            2 => bad.result_mapping.push(bad.result_mapping[0].clone()),
            _ => bad.steps[0].classifications[0].unknown_mask = 0xff,
        }
        assert_eq!(assemble(&bad, &[]), Err(AssembleRefusal::InvalidPlan));
    }
}

#[test]
fn zero_is_a_bound_not_an_implicit_unlimited_grant() {
    let mut input = request(vec![range(0, 8)]);
    input.limits.max_ranges = 0;
    assert!(matches!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::TooManyRanges { limit: 0, .. })
    ));
    input.limits.max_ranges = 1;
    input.limits.max_bytes = 0;
    assert!(matches!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::TooManyBytes { limit: 0, .. })
    ));
    input.limits.max_bytes = 8;
    input.limits.max_transfer_len = 0;
    assert_eq!(
        plan_observation(&footprint(), &input),
        Err(PlanRefusal::InvalidTransferLimit)
    );
}

#[test]
fn passive_range_preserves_dynamic_change_source_for_the_consumer() {
    let mut footprint = footprint();
    footprint.lower[1].change_source = ChangeSource::Dynamic;
    assert_eq!(
        footprint.classify_range(range(0, 2)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        })
    );
    assert_eq!(
        footprint.classify_range(range(4, 2)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Static
        })
    );
}

#[test]
fn write_only_and_startup_acknowledgement_refuse_the_named_byte() {
    for (class, reason) in [
        (
            ByteClassification::from_masks(0, 0, 0, 0xff, &[], ChangeSource::Static),
            NonPassiveReason::WriteOnly,
        ),
        (
            ByteClassification::from_masks(
                0xff,
                0,
                0,
                0,
                &[ReadEffect::AcknowledgeStartupInterrupt],
                ChangeSource::Dynamic,
            ),
            NonPassiveReason::AcknowledgeStartupInterrupt,
        ),
    ] {
        let footprint = footprint_with(|lower| lower[2] = class);
        assert_eq!(
            footprint.classify_range(range(0, 4)),
            Ok(RangeClassified::NonPassive { byte: 2, reason })
        );
        assert_eq!(
            plan_observation(&footprint, &request(vec![range(0, 4)])),
            Err(PlanRefusal::NonPassive { byte: 2, reason })
        );
    }
}

#[test]
fn reversed_assembly_interval_is_a_typed_refusal() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 1)])).unwrap();
    assert_eq!(
        assemble_snapshot(&AssembleInput {
            plan: &plan,
            results: &[transfer(0, vec![0])],
            generation: GEN,
            epoch: EPOCH,
            started_at: MonoTime(11),
            completed_at: MonoTime(10),
        }),
        Err(AssembleRefusal::InvalidInterval(InputError::ReversedTime))
    );
}

#[test]
fn assembled_snapshot_rejects_out_of_interval_stamps() {
    let plan = plan_observation(&footprint(), &request(vec![range(0, 1)])).unwrap();
    let snapshot = assemble(&plan, &[transfer(0, vec![0])]).unwrap();
    for (started_at, completed_at) in [(MonoTime(9), MonoTime(11)), (MonoTime(10), MonoTime(12))] {
        let mut bad = snapshot.clone();
        bad.ranges[0].stamp.started_at = started_at;
        bad.ranges[0].stamp.completed_at = completed_at;
        assert_eq!(bad.validate(), Err(InputError::StaleObservation));
    }
}
