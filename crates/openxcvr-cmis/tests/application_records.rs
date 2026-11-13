//! Synthetic table fixtures, not captures: CMIS 4.0 Tables 8-12/13/39 and
//! CMIS 5.3 Tables 8-20/22/23/59. FF is the list terminator, not zero.
use openxcvr_cmis::{
    application_records::{application_records, InterfaceMeaning, LaneCount, SlotState},
    Rev,
};

fn fixture(rev: Rev, media: u8) -> ([u8; 128], [u8; 128]) {
    let mut lower = [0; 128];
    let mut upper = [0; 128];
    lower[1] = rev.major << 4 | rev.minor;
    lower[85] = media;
    for i in 0..8 {
        lower[86 + 4 * i..90 + 4 * i].copy_from_slice(&[0x1a, 1, 0x44, 0x11]);
    }
    for i in 0..7 {
        upper[95 + 4 * i..99 + 4 * i].copy_from_slice(&[0x1a, 1, 0x22, 0x55]);
    }
    (lower, upper)
}

#[test]
fn all_slots_and_boundaries_are_appsel_not_lanes() {
    for rev in [
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let (lower, upper) = fixture(rev, 1);
        let result = application_records(rev, Some(&lower), Some(&upper));
        assert_eq!(result.revision, rev);
        assert_eq!(result.slots.len(), 15);
        for slot in [8, 9, 15] {
            let record = &result.slots[slot - 1];
            assert_eq!(record.slot as usize, slot);
            assert_eq!(record.state, SlotState::Present);
            let app = record.application.as_ref().unwrap();
            assert_eq!(app.app_sel_code as usize, slot);
            assert_eq!(app.host_lane_count, if slot == 8 { 4 } else { 2 });
            assert_eq!(
                app.host_lane_assignment_options,
                if slot == 8 { 0x11 } else { 0x55 }
            );
            assert_eq!(record.host.meaning().as_deref(), Some("Base100gCr4"));
        }
    }
}

#[test]
fn sentinel_terminates_across_pages_without_decoding_trailing_garbage() {
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let (mut lower, upper) = fixture(rev, 1);
        lower[114] = 0xff; // slot 8
        let r = application_records(rev, Some(&lower), Some(&upper));
        assert_eq!(r.slots[7].state, SlotState::EndOfList);
        assert!(r.slots[8..]
            .iter()
            .all(|r| r.state == SlotState::Absent && r.application.is_none()));
        lower[114] = 0x1a;
        let mut upper = upper;
        upper[95] = 0xff; // slot 9
        let r = application_records(rev, Some(&lower), Some(&upper));
        assert_eq!(r.slots[8].state, SlotState::EndOfList);
        assert!(r.slots[9..].iter().all(|r| r.state == SlotState::Absent));
    }
}

#[test]
fn media_table_selection_reuses_generated_enums() {
    let expected = [
        "Base10gSw",
        "Base10gLw",
        "CopperCable",
        "ActiveCableBer1e12",
        "Base1000T",
    ];
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        for (media, expected) in (1..=5).zip(expected) {
            let (lower, upper) = fixture(rev, media);
            let r = application_records(rev, Some(&lower), Some(&upper));
            assert_eq!(r.slots[0].media.meaning().as_deref(), Some(expected));
        }
    }
}

#[test]
fn undefined_and_unknown_are_not_terminators_or_zero_success() {
    let (mut lower, upper) = fixture(Rev::CMIS_4_0, 1);
    lower[86] = 0;
    lower[87] = 0;
    let r = application_records(Rev::CMIS_4_0, Some(&lower), Some(&upper));
    assert_eq!(r.slots[0].state, SlotState::Present);
    assert_eq!(r.slots[0].host, InterfaceMeaning::Undefined);
    assert_eq!(r.slots[0].media, InterfaceMeaning::Undefined);
    lower[86] = 0xfe;
    lower[87] = 0xfe;
    let r = application_records(Rev::CMIS_4_0, Some(&lower), Some(&upper));
    assert_eq!(r.slots[0].host, InterfaceMeaning::UnknownCode(0xfe));
    assert_eq!(r.slots[0].media, InterfaceMeaning::UnknownCode(0xfe));
    lower[85] = 0x40;
    let r = application_records(Rev::CMIS_4_0, Some(&lower), Some(&upper));
    assert_eq!(
        r.slots[0].media,
        InterfaceMeaning::UnsupportedMediaType(0x40)
    );
}

#[test]
fn missing_and_short_pages_are_explicit_even_after_a_lower_sentinel() {
    let (mut lower, upper) = fixture(Rev::CMIS_5_3, 1);
    lower[90] = 0xff;
    let r = application_records(Rev::CMIS_5_3, Some(&lower), None);
    assert!(r.slots[8..]
        .iter()
        .all(|r| r.state == SlotState::Unavailable));
    let r = application_records(Rev::CMIS_5_3, Some(&lower), Some(&upper[..122]));
    assert!(r.slots[8..].iter().all(|r| r.state == SlotState::ShortData));
    let r = application_records(Rev::CMIS_5_3, None, Some(&upper));
    assert!(r.slots.iter().all(|r| r.state == SlotState::Unavailable));
    let r = application_records(Rev::CMIS_5_3, Some(&lower[..117]), Some(&upper));
    assert!(r.slots.iter().all(|r| r.state == SlotState::ShortData));
}

#[test]
fn revision_mismatch_and_unsupported_revision_are_not_newest_fallbacks() {
    let (lower, upper) = fixture(Rev::CMIS_4_0, 1);
    let r = application_records(Rev::CMIS_5_3, Some(&lower), Some(&upper));
    assert!(r
        .slots
        .iter()
        .all(|r| r.state == SlotState::RevisionMismatch));
    for rev in [
        Rev::CMIS_3_0,
        Rev {
            major: 5,
            minor: 4,
            ..Rev::CMIS_5_3
        },
        Rev {
            family: openxcvr_core::rev::Family::Sff8636,
            ..Rev::CMIS_4_0
        },
    ] {
        let r = application_records(rev, Some(&lower), Some(&upper));
        assert!(r
            .slots
            .iter()
            .all(|r| r.state == SlotState::UnsupportedRevision));
    }
}

#[test]
fn counts_preserve_implicit_and_reserved_encodings() {
    let (mut lower, upper) = fixture(Rev::CMIS_4_0, 3);
    lower[88] = 0x09;
    lower[89] = 0xff;
    let r = application_records(Rev::CMIS_4_0, Some(&lower), Some(&upper));
    assert_eq!(r.slots[0].host_lanes, Some(LaneCount::DefinedByInterface));
    assert_eq!(r.slots[0].media_lanes, Some(LaneCount::Reserved(9)));
    assert_eq!(
        r.slots[0]
            .application
            .as_ref()
            .unwrap()
            .host_lane_assignment_options,
        0xff
    );
}
