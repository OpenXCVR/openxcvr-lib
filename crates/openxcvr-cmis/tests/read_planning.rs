//! Real schema/fixture refusal proof: fixture bytes are not hardware reads,
//! and a successful decode cannot manufacture read-effect authority.
use openxcvr_cmis::read_planning::CmisReadPlanner;
use openxcvr_core::{rev::Rev, types::management::*};
use std::path::Path;

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}
fn request(revision: Rev, ranges: Vec<Range>) -> PlanObservationInput {
    PlanObservationInput {
        revision,
        requester: ReadRequester::PassiveInspection,
        ranges,
        limits: ReadLimits {
            max_ranges: 4,
            max_bytes: 256,
            max_transfer_len: 16,
        },
        selector: Some(Selection {
            address: 0x50,
            bank: 0,
            page: 0,
        }),
        flat_memory: Some(false),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
    }
}

#[test]
fn fixture_advertised_53_uses_real_generated_metadata_and_refuses_unmarked_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cmis");
    let lower = std::fs::read(root.join("fboss-cmis-5-3-osfp-800g-zr.lower.bin")).unwrap();
    let upper = std::fs::read(root.join("fboss-cmis-5-3-osfp-800g-zr.page-00h.bin")).unwrap();
    let revision = Rev::from_major_byte_cmis(lower[1]).unwrap();
    assert_eq!(revision, Rev::CMIS_5_3);
    assert_eq!(lower.len(), 128);
    assert_eq!(upper.len(), 128);
    let planner = CmisReadPlanner::cmis_5_3();
    for requested in [range(14, 1), range(128, 16)] {
        assert_eq!(
            planner.classify_range(revision, requested),
            Ok(RangeClassified::NonPassive {
                byte: requested.offset,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
        assert_eq!(
            planner.plan_observation(&request(revision, vec![requested])),
            Err(PlanRefusal::NonPassive {
                byte: requested.offset,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
    }
    assert_eq!(
        planner.classify_range(revision, range(9, 3)),
        Ok(RangeClassified::NonPassive {
            byte: 9,
            reason: NonPassiveReason::ClearOnRead
        })
    );
    for requested in [
        Range {
            page: 1,
            ..range(128, 1)
        },
        Range {
            bank: 1,
            ..range(128, 1)
        },
        Range {
            address: 0x51,
            ..range(0, 1)
        },
    ] {
        assert_eq!(
            planner.classify_range(revision, requested),
            Ok(RangeClassified::Uncovered)
        );
    }
}

#[test]
fn fixture_advertised_51_is_refused_instead_of_decoded_as_53() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cmis");
    let lower = std::fs::read(root.join("fboss-cmis-5-1-osfp-credo-800g-aec.lower.bin")).unwrap();
    let revision = Rev::from_major_byte_cmis(lower[1]).unwrap();
    assert_ne!(revision, Rev::CMIS_5_3);
    let planner = CmisReadPlanner::cmis_5_3();
    assert_eq!(
        planner.classify_range(revision, range(0, 1)),
        Err(InputError::UnsupportedRevision(revision))
    );
    assert_eq!(
        planner.plan_observation(&request(revision, vec![range(0, 1)])),
        Err(PlanRefusal::UnsupportedRevision(revision))
    );
}

#[test]
fn fixture_bytes_flow_through_split_transfers_with_explicit_synthetic_authority() {
    // This is a transport/assembly test, NOT schema admission or a hardware read.
    // The synthetic explicit-None footprint is deliberately separate from the
    // real generated CMIS metadata exercised above, which admits only identity bytes.
    let passive =
        || ByteClassification::from_masks(0xff, 0, 0, 0, &[ReadEffect::None], ChangeSource::Static);
    let footprint = ReadFootprint {
        revision: Rev::CMIS_5_3,
        address: 0x50,
        lower: std::array::from_fn(|_| passive()),
        upper_page00: std::array::from_fn(|_| passive()),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/cmis");
    let lower = std::fs::read(root.join("fboss-cmis-5-3-osfp-800g-zr.lower.bin")).unwrap();
    let upper = std::fs::read(root.join("fboss-cmis-5-3-osfp-800g-zr.page-00h.bin")).unwrap();
    let revision = Rev::from_major_byte_cmis(lower[1]).unwrap();
    let bytes = [lower, upper].concat();
    let plan = plan_observation(&footprint, &request(revision, vec![range(0, 256)])).unwrap();
    let results: Vec<_> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(step_index, step)| {
            let physical = step.physical_range;
            TransferResult {
                step_index,
                range: physical,
                generation: ModuleGeneration(1),
                epoch: CacheEpoch(2),
                bytes: bytes[usize::from(physical.offset)..physical.end() as usize].to_vec(),
            }
        })
        .rev()
        .collect(); // backend completion ordering cannot reorder bytes
    let snapshot = assemble_snapshot(&AssembleInput {
        plan: &plan,
        results: &results,
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
    })
    .unwrap();
    assert_eq!(snapshot.ranges[0].bytes(), Some(bytes.as_slice()));
    snapshot.validate().unwrap();
}

#[test]
fn actual_metadata_gaps_are_enumerated_from_generated_tables() {
    use openxcvr_cmis::generated::union::{CmisLowerMemory, CmisPage00hAdministrative};
    let table: Vec<_> = CmisLowerMemory::read_classifications()
        .into_iter()
        .chain(CmisPage00hAdministrative::read_classifications())
        .collect();
    let mut start = 0;
    while start < table.len() {
        let class = &table[start];
        let mut end = start + 1;
        while end < table.len() && table[end] == *class {
            end += 1;
        }
        println!("{start:#04x}..={:#04x}: readable={:#04x} unknown={:#04x} uncovered={:#04x} write_only={:#04x} effects={:?} change={:?}", end - 1,
            class.readable_mask, class.unknown_mask, class.uncovered_mask, class.write_only_mask, class.known_effects, class.change_source);
        for (offset, classification) in table.iter().enumerate().take(end).skip(start) {
            // CMIS 5.3 passive set at this revision: lower bytes 0-2 (Table 8-5
            // identity bytes), 3 (Table 8-6 global status; Reserved bits are
            // nonfunctional per §8.1.3.1), 37 (dynamic CdbStatus1, § 8.2.8 Tables 8-13/14,
            // p.151; plain RO per Table 8-3, p.138),
            // 39-40 (active firmware, § 8.2.9 Table 8-15),
            // 85-117 (application advertising, § 8.2.13),
            // and 126-127 (BankSelect/PageSelect, § 8.2.15
            // Table 8-25 RW/RWW: Table 8-3 gives a read of RW/RWW no effect), plus
            // upper page-00h 129-189 (Table 8-28 vendor identity bytes).
            let expected_passive = offset <= 3
                || offset == 37
                || (39..=40).contains(&offset)
                || (85..=117).contains(&offset)
                || (0x7e..=0x7f).contains(&offset)
                || (0x81..=0xbd).contains(&offset);
            assert_eq!(
                classification.is_qualified_none(),
                expected_passive,
                "byte {offset:#04x}"
            );
            // CdbStatus1 is module-updated. BankSelect/PageSelect also stay
            // dynamic (PageMapping Validity clears PageSelect): read effect
            // None is not evidence of a static change source.
            let expected_change = if offset == 3 || offset == 37 || (0x7e..=0x7f).contains(&offset)
            {
                ChangeSource::Dynamic
            } else {
                ChangeSource::Static
            };
            let expected = if expected_passive {
                RangeClassified::Passive {
                    change_source: expected_change,
                }
            } else {
                let reason = match offset {
                    // Table 8-9 (p.146) and Table 8-3 (p.138) qualify the
                    // full byte-8 COR effect, including Reserved bits 5:3.
                    0x08..=0x0b => NonPassiveReason::ClearOnRead,
                    0x04..=0x07
                    | 0x0c..=0x0d
                    | 0x1a..=0x1f
                    | 0x23..=0x24
                    | 0x2a..=0x37
                    | 0x3a..=0x54 => NonPassiveReason::MetadataAbsent,
                    0x76..=0x7d => NonPassiveReason::WriteOnly,
                    _ => NonPassiveReason::ReadEffectUnknown,
                };
                RangeClassified::NonPassive {
                    byte: offset as u8,
                    reason,
                }
            };
            assert_eq!(
                CmisReadPlanner::cmis_5_3().classify_range(Rev::CMIS_5_3, range(offset as u8, 1)),
                Ok(expected)
            );
        }
        start = end;
    }
}

#[test]
fn identity_footprint_plans_but_adjacent_bytes_are_refused() {
    let planner = CmisReadPlanner::cmis_5_3();
    let input = request(Rev::CMIS_5_3, vec![range(0, 3), range(0x81, 61)]);
    let plan = planner.plan_observation(&input).unwrap();
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| s.physical_range)
            .collect::<Vec<_>>(),
        vec![
            range(0, 3),
            range(0x81, 16),
            range(0x91, 16),
            range(0xa1, 16),
            range(0xb1, 13)
        ]
    );
    assert!(plan
        .steps
        .iter()
        .flat_map(|s| &s.classifications)
        .all(ByteClassification::is_qualified_none));
    for (requested, byte) in [(range(0x80, 62), 0x80), (range(0x81, 62), 0xbe)] {
        assert_eq!(
            planner.plan_observation(&request(Rev::CMIS_5_3, vec![requested])),
            Err(PlanRefusal::NonPassive {
                byte,
                reason: NonPassiveReason::ReadEffectUnknown
            })
        );
    }
}

#[test]
fn revision_40_admits_the_shared_footprint_and_keeps_other_revisions_closed() {
    let v4 = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    let v5 = CmisReadPlanner::cmis_5_3();
    for r in [range(0, 2), range(126, 2), range(129, 61)] {
        assert_eq!(
            v4.plan_observation(&request(Rev::CMIS_4_0, vec![r]))
                .unwrap()
                .steps,
            v5.plan_observation(&request(Rev::CMIS_5_3, vec![r]))
                .unwrap()
                .steps
        );
    }
    assert_eq!(
        v4.classify_range(Rev::CMIS_4_0, range(223, 33)),
        v5.classify_range(Rev::CMIS_5_3, range(223, 33))
    );
    for revision in [
        Rev::CMIS_3_0,
        Rev {
            family: openxcvr_core::rev::Family::Sff8636,
            major: 4,
            minor: 0,
        },
    ] {
        assert!(
            matches!(CmisReadPlanner::for_revision(revision), Err(InputError::UnsupportedRevision(r)) if r == revision)
        );
    }
    assert_eq!(
        v4.classify_range(Rev::CMIS_5_3, range(0, 1)),
        Err(InputError::UnsupportedRevision(Rev::CMIS_5_3))
    );
}

#[test]
fn generated_40_and_53_lower_layouts_keep_revision_specific_byte2_semantics() {
    use openxcvr_cmis::generated::{union, v4_0};
    // Sweep each byte value through the emitted decoders; no handwritten bit map.
    for value in 0..=255 {
        let bytes = [value; 128];
        let v4 = v4_0::CmisLowerMemory::decode(&bytes);
        let v5 = union::CmisLowerMemory::decode(&bytes);
        assert_eq!(v4.sff8024_identifier, v5.sff8024_identifier);
        assert_eq!(v4.cmis_revision, v5.cmis_revision);
        assert_eq!(v4.memory_model, v5.memory_model);
        assert_eq!(v4.twi_max_speed_cmis4, v5.mci_max_speed & 3);
        assert_eq!(v4.bank_select, v5.bank_select);
        assert_eq!(v4.page_select, v5.page_select);
    }
    let v4 = v4_0::CmisLowerMemory::read_classifications();
    let v5 = union::CmisLowerMemory::read_classifications();
    for offset in [0, 1, 126, 127] {
        assert_eq!(v4[offset], v5[offset]);
    }
    assert_eq!(v4[2].uncovered_mask, 0);
    assert!(v4[2].is_qualified_none());
    assert_eq!(v5[2].uncovered_mask, 0);
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    assert!(planner
        .plan_observation(&request(Rev::CMIS_4_0, vec![range(0, 3)]))
        .is_ok());
    // Independently sourced page-00h read effects are revision-specific;
    // 5.3 and custom bytes 223..255 remain unchanged.
}

#[test]
fn cmis4_d4_whole_status_byte_is_dynamic_and_does_not_acquire_flags() {
    let planner = CmisReadPlanner::for_revision(Rev::CMIS_4_0).unwrap();
    assert_eq!(
        planner.classify_range(Rev::CMIS_4_0, range(3, 1)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        })
    );
    let plan = planner
        .plan_observation(&request(Rev::CMIS_4_0, vec![range(3, 1)]))
        .unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].physical_range, range(3, 1));
    assert_eq!(plan.steps[0].declared_effects, vec![ReadEffect::None]);
    assert_eq!(
        plan.steps[0].classifications[0].change_source,
        ChangeSource::Dynamic
    );
    // Byte 8 already has COR but uncovered reserved bits: keep the stricter
    // existing MetadataAbsent refusal rather than qualifying unrelated bits.
    for byte in 4..=11 {
        assert_eq!(
            planner.classify_range(Rev::CMIS_4_0, range(byte, 1)),
            Ok(RangeClassified::NonPassive {
                byte,
                reason: if byte <= 8 {
                    NonPassiveReason::MetadataAbsent
                } else {
                    NonPassiveReason::ClearOnRead
                }
            })
        );
    }
    assert_eq!(
        CmisReadPlanner::cmis_5_3().classify_range(Rev::CMIS_5_3, range(3, 1)),
        Ok(RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        })
    );
}
