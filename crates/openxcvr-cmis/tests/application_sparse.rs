//! Sparse dependency coverage, not transport authority. Synthetic CMIS fixtures.
use openxcvr_cmis::{
    application_records::{
        application_records, application_records_footprint, application_records_from_ranges,
        ApplicationRange, SlotState,
    },
    generated::catalog::CATALOG,
    Rev,
};
use openxcvr_core::{
    field_catalog::{resolve, FieldQuery, LaneSelection},
    types::management::Range,
};

const REVS: [Rev; 5] = [
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
];
fn range(page: u8, offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page,
        offset,
        len,
    }
}
fn fixture(rev: Rev, media: u8, scenario: u8) -> ([u8; 128], [u8; 128]) {
    let mut lo = [0; 128];
    let mut hi = [0; 128];
    lo[1] = rev.major << 4 | rev.minor;
    lo[85] = media;
    for i in 0..8 {
        lo[86 + 4 * i..90 + 4 * i].copy_from_slice(&[0x1a, 1, 0x44, 0x11]);
    }
    for i in 0..7 {
        hi[95 + 4 * i..99 + 4 * i].copy_from_slice(&[0x1a, 1, 0x22, 0x55]);
    }
    match scenario {
        1 => lo[114] = 255,
        2 => hi[95] = 255,
        5 => {
            lo[86] = 254;
            lo[87] = 254;
        }
        6 => {
            lo[86] = 0;
            lo[87] = 0;
        }
        7 => {
            lo[88] = 9;
            lo[89] = 255;
        }
        8 => lo[1] = 0,
        11 => hi[119] = 255,
        _ => (),
    }
    (lo, hi)
}
fn subset<'a>(rev: Rev, lo: &'a [u8], hi: &'a [u8]) -> Vec<ApplicationRange<'a>> {
    application_records_footprint(rev)
        .unwrap()
        .ranges
        .into_iter()
        .map(|range| {
            let (bytes, base) = if range.page == 0 { (lo, 0) } else { (hi, 128) };
            ApplicationRange {
                range,
                bytes: &bytes[usize::from(range.offset) - base..range.end() as usize - base],
            }
        })
        .collect()
}
#[test]
fn exact_schema_derived_footprint_and_passive_lower_bytes() {
    for rev in REVS {
        let fp = application_records_footprint(rev).unwrap();
        assert_eq!(fp.revision, rev);
        assert_eq!(
            fp.ranges,
            vec![range(0, 1, 1), range(0, 85, 33), range(1, 223, 28)]
        );
        // Check all constituents, not just one row chosen for a shared byte.
        let mut covered = [0u8; 128];
        for row in CATALOG.rows.iter().filter(|r| r.page_label == "lower") {
            let entries = resolve(
                &CATALOG,
                rev,
                &FieldQuery {
                    pattern: row.name,
                    page: Some("lower"),
                    lane: if row.instances.is_some() {
                        LaneSelection::All
                    } else {
                        LaneSelection::Unspecified
                    },
                    bank: 0,
                },
            );
            if let Ok(entries) = entries {
                for entry in entries {
                    if fp.ranges.iter().any(|r| r.contains(entry.range())) {
                        assert_eq!(
                            entry.row.read_effects,
                            &[openxcvr_core::types::management::ReadEffect::None]
                        );
                        assert_eq!(
                            entry.row.access,
                            openxcvr_core::field_catalog::AccessKind::Ro
                        );
                        covered[entry.offset as usize] |=
                            (((1u16 << entry.width_bits()) - 1) << entry.bit_offset) as u8;
                    }
                }
            }
        }
        for r in fp.ranges.iter().filter(|r| r.page == 0) {
            for (b, mask) in covered
                .iter()
                .enumerate()
                .take(r.end() as usize)
                .skip(r.offset as usize)
            {
                assert_eq!(*mask, 0xff, "{rev:?} byte {b} must be fully qualified");
                if let Ok(planner) =
                    openxcvr_cmis::read_planning::CmisReadPlanner::for_revision(rev)
                {
                    assert!(matches!(
                        planner.classify_range(rev, range(0, b as u8, 1)),
                        Ok(openxcvr_core::types::management::RangeClassified::Passive { .. })
                    ));
                }
            }
        }
    }
    assert!(application_records_footprint(Rev::CMIS_3_0).is_err());
}
#[test]
fn footprint_subset_equals_full_for_existing_fixture_scenarios() {
    for rev in REVS {
        for media in [0, 1, 2, 3, 4, 5, 0x40] {
            for scenario in [0, 1, 2, 5, 6, 7, 8, 11] {
                let (lo, hi) = fixture(rev, media, scenario);
                assert_eq!(
                    application_records_from_ranges(rev, &subset(rev, &lo, &hi)).unwrap(),
                    application_records(rev, Some(&lo), Some(&hi))
                );
            }
        }
    }
}
#[test]
fn every_consumed_byte_must_be_supplied_before_slot_decoding() {
    for rev in REVS {
        let (lo, hi) = fixture(rev, 1, 0);
        let fp = application_records_footprint(rev).unwrap();
        for omitted in fp
            .ranges
            .iter()
            .flat_map(|r| (u16::from(r.offset)..r.end() as u16).map(move |b| (r.page, b)))
        {
            let mut supplied = Vec::new();
            for r in &fp.ranges {
                for b in u16::from(r.offset)..r.end() as u16 {
                    if (r.page, b) != omitted {
                        let index = usize::from(b);
                        supplied.push(ApplicationRange {
                            range: range(r.page, b as u8, 1),
                            bytes: if r.page == 0 {
                                &lo[index..index + 1]
                            } else {
                                &hi[index - 128..index - 127]
                            },
                        });
                    }
                }
            }
            let decoded = application_records_from_ranges(rev, &supplied).unwrap();
            let affected = if omitted.0 == 0 && (omitted.1 == 1 || omitted.1 == 85) {
                None
            } else if omitted.0 == 0 {
                Some((omitted.1 - 86) / 4)
            } else {
                Some(8 + (omitted.1 - 223) / 4)
            };
            for (i, s) in decoded.slots.iter().enumerate() {
                if affected.is_none() || affected == Some(i as u16) {
                    assert_eq!(s.state, SlotState::ShortData, "{rev:?} {omitted:?} {i}");
                    assert!(s.application.is_none());
                } else {
                    assert_eq!(s.state, SlotState::Present);
                }
            }
        }
    }
}
#[test]
fn missing_upper_is_unavailable_even_after_lower_ff_and_missing_lower_is_explicit() {
    for rev in REVS {
        let (lo, hi) = fixture(rev, 1, 1);
        let mut supplied = subset(rev, &lo, &hi);
        supplied.retain(|r| r.range.page == 0);
        let r = application_records_from_ranges(rev, &supplied).unwrap();
        assert!(r.slots[8..]
            .iter()
            .all(|s| s.state == SlotState::Unavailable));
        assert!(application_records_from_ranges(rev, &[])
            .unwrap()
            .slots
            .iter()
            .all(|s| s.state == SlotState::Unavailable));
        supplied.retain(|r| r.range.offset != 1);
        assert!(application_records_from_ranges(rev, &supplied)
            .unwrap()
            .slots
            .iter()
            .all(|s| s.state == SlotState::ShortData));
    }
}
#[test]
fn unsupplied_backing_bytes_cannot_influence_results_and_fragmentation_is_supported() {
    for rev in REVS {
        let (lo, hi) = fixture(rev, 1, 0);
        let expected = application_records_from_ranges(rev, &subset(rev, &lo, &hi)).unwrap();
        for pattern in [0x55, 0xaa] {
            let mut a = [pattern; 128];
            let mut b = [pattern; 128];
            for r in application_records_footprint(rev).unwrap().ranges {
                for o in usize::from(r.offset)..r.end() as usize {
                    if r.page == 0 {
                        a[o] = lo[o]
                    } else {
                        b[o - 128] = hi[o - 128]
                    }
                }
            }
            assert_eq!(
                application_records_from_ranges(rev, &subset(rev, &a, &b)).unwrap(),
                expected
            );
        }
        let mut one_byte = Vec::new();
        for s in subset(rev, &lo, &hi) {
            for (i, byte) in s.bytes.iter().enumerate() {
                one_byte.push(ApplicationRange {
                    range: range(s.range.page, s.range.offset + i as u8, 1),
                    bytes: std::slice::from_ref(byte),
                });
            }
        }
        one_byte.reverse();
        assert_eq!(
            application_records_from_ranges(rev, &one_byte).unwrap(),
            expected
        );
    }
}
#[test]
fn malformed_or_overlapping_ranges_refuse_and_unsupported_revision_fails_closed() {
    let byte = [0x40];
    let s = ApplicationRange {
        range: range(0, 1, 1),
        bytes: &byte,
    };
    assert!(application_records_from_ranges(Rev::CMIS_4_0, &[s, s]).is_err());
    assert!(application_records_from_ranges(
        Rev::CMIS_4_0,
        &[ApplicationRange {
            range: range(0, 1, 2),
            bytes: &byte
        }]
    )
    .is_err());
    assert!(application_records_from_ranges(
        Rev::CMIS_4_0,
        &[ApplicationRange {
            range: range(0, 127, 2),
            bytes: &[0, 0]
        }]
    )
    .is_err());
    assert!(application_records_from_ranges(Rev::CMIS_3_0, &[])
        .unwrap()
        .slots
        .iter()
        .all(|r| r.state == SlotState::UnsupportedRevision));
}
