//! Exactly lower-memory 85–117; no page-01h or general RO read authority.
use openxcvr_cmis::read_planning::CmisReadPlanner;
use openxcvr_core::{types::management::*, Rev};

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

#[test]
fn application_advertising_is_passive_at_every_delivered_planner_revision() {
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let planner = CmisReadPlanner::for_revision(rev).unwrap();
        for r in std::iter::once(range(85, 33)).chain((85..=117).map(|b| range(b, 1))) {
            assert_eq!(
                planner.classify_range(rev, r),
                Ok(RangeClassified::Passive {
                    change_source: ChangeSource::Static
                }),
                "{rev:?} {r:?}"
            );
        }
    }
}

#[test]
fn clear_on_read_and_unqualified_neighbors_are_not_widened() {
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let planner = CmisReadPlanner::for_revision(rev).unwrap();
        for byte in 9..=11 {
            assert_eq!(
                planner.classify_range(rev, range(byte, 1)),
                Ok(RangeClassified::NonPassive {
                    byte,
                    reason: NonPassiveReason::ClearOnRead
                })
            );
        }
        for r in [
            range(8, 1),
            range(12, 2),
            range(84, 34),
            range(85, 34),
            range(8, 110),
        ] {
            assert!(matches!(
                planner.classify_range(rev, r),
                Ok(RangeClassified::NonPassive { .. })
            ));
        }
    }
}
