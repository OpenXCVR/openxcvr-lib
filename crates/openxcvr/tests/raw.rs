//! Pure R1 contract tests: no transport, execution, checksum repair or hardware.
use openxcvr::{
    core::{
        rev::{Family, Rev},
        types::management::{Range, ReadEffect, ReadRequester, Selection},
    },
    raw::{
        classify_target, preflight, support_cell, AddressModel, ByteClass, Purpose, RawRegion,
        RawRequest, RawTarget, RefusalReason, UnsupportedCell, WriteBytes, MAX_BYTES, MAX_RANGES,
    },
};

fn target(offset: u8, len: u16) -> RawTarget {
    RawTarget {
        model: AddressModel::Cmis,
        range: Range {
            address: 0x50,
            bank: 0,
            page: 0,
            offset,
            len,
        },
    }
}

fn check(
    targets: &[RawTarget],
    purpose: Purpose,
) -> Result<openxcvr::raw::RawDescription, openxcvr::raw::RawRefusal> {
    let payload = [0xa5; 256];
    let regions: Vec<_> = targets
        .iter()
        .map(|&target| RawRegion {
            target,
            write_bytes: (purpose == Purpose::Write)
                .then(|| WriteBytes::new(&payload[..usize::from(target.range.len).min(256)])),
        })
        .collect();
    preflight(&RawRequest {
        revision: Rev::CMIS_4_0,
        purpose,
        regions: &regions,
    })
}

fn reason(target: RawTarget, purpose: Purpose) -> RefusalReason {
    check(&[target], purpose).unwrap_err().reason()
}

#[test]
fn legacy_a0_is_canonical_seven_bit_module_memory_not_a2() {
    let mut a0 = target(129, 16);
    a0.range.address = 0xa0;
    let description = check(&[a0], Purpose::Read).unwrap();
    assert_eq!(description.targets()[0].target(), target(129, 16));
    assert_eq!(
        description.targets()[0].target().selection(),
        Selection {
            address: 0x50,
            bank: 0,
            page: 0
        }
    );
    assert_eq!(a0.canonical().unwrap().range.address, 0x50);
    let mut a2 = a0;
    a2.model = AddressModel::Sff8472A2;
    a2.range.address = 0xa2;
    assert_eq!(a2.canonical().unwrap().range.address, 0x51);
    let mut sfp_a0 = a0;
    sfp_a0.model = AddressModel::Sff8472A0;
    assert_eq!(sfp_a0.canonical().unwrap().range.address, 0x50);
    assert_ne!(sfp_a0.canonical().unwrap(), a2.canonical().unwrap());
}

#[test]
fn addresses_cannot_silently_change_model_identity() {
    for address in [0x51, 0xa2] {
        let mut t = target(0, 1);
        t.range.address = address;
        assert_eq!(
            reason(t, Purpose::Read),
            RefusalReason::AddressModelMismatch
        );
    }
    for address in [0x80, 0xa1, 0xff] {
        let mut t = target(0, 1);
        t.range.address = address;
        assert_eq!(reason(t, Purpose::Read), RefusalReason::InvalidAddress);
    }
}

#[test]
fn support_matrix_explicitly_refuses_other_models_families_and_revisions() {
    assert_eq!(
        support_cell(Rev::CMIS_4_0, AddressModel::Cmis, 0, 0),
        Ok(())
    );
    for family in [
        Family::Sff8472,
        Family::Sff8636,
        Family::CCmis,
        Family::Elsfp,
    ] {
        let rev = Rev {
            family,
            major: 4,
            minor: 0,
        };
        assert_eq!(
            support_cell(rev, AddressModel::Cmis, 0, 0),
            Err(UnsupportedCell::Family)
        );
        let regions = [RawRegion {
            target: target(0, 1),
            write_bytes: None,
        }];
        assert_eq!(
            preflight(&RawRequest {
                revision: rev,
                purpose: Purpose::Read,
                regions: &regions
            })
            .unwrap_err()
            .reason(),
            RefusalReason::Unsupported(UnsupportedCell::Family)
        );
    }
    for rev in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
        Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        },
    ] {
        assert_eq!(
            support_cell(rev, AddressModel::Cmis, 0, 0),
            Err(UnsupportedCell::Revision)
        );
        assert!(classify_target(rev, target(129, 1))
            .unwrap()
            .bytes()
            .iter()
            .all(|b| b.class == ByteClass::Unknown));
    }
    for model in [
        AddressModel::Sff8636,
        AddressModel::Sff8472A0,
        AddressModel::Sff8472A2,
        AddressModel::Unknown(65535),
    ] {
        assert_eq!(
            support_cell(Rev::CMIS_4_0, model, 0, 0),
            Err(UnsupportedCell::AddressModel)
        );
        let mut t = target(129, 1);
        t.model = model;
        if model == AddressModel::Sff8472A2 {
            t.range.address = 0x51;
        }
        assert_eq!(
            reason(t, Purpose::Read),
            RefusalReason::Unsupported(UnsupportedCell::AddressModel)
        );
    }
}

#[test]
fn every_other_bank_page_cell_is_unsupported_including_cdb() {
    for bank in 0..=u8::MAX {
        for page in 0..=u8::MAX {
            let cell = support_cell(Rev::CMIS_4_0, AddressModel::Cmis, bank, page);
            assert_eq!(cell.is_ok(), bank == 0 && page == 0);
        }
    }
    for (bank, page) in [(1, 0), (255, 0), (0, 1), (0, 0x9f), (0, 255)] {
        let mut t = target(128, 1);
        t.range.bank = bank;
        t.range.page = page;
        assert!(check(&[t], Purpose::Read).is_err());
        assert!(check(&[t], Purpose::Write).is_err());
    }
}

#[test]
fn zero_length_and_all_overflowing_lengths_refuse_without_wrapping() {
    for (offset, len) in [(0, 0), (255, 2), (128, 129), (1, u16::MAX), (255, u16::MAX)] {
        assert_eq!(
            reason(target(offset, len), Purpose::Read),
            RefusalReason::InvalidRange
        );
    }
    assert!(classify_target(Rev::CMIS_4_0, target(255, 1)).is_ok());
}

#[test]
fn lower_upper_window_crossings_and_noncanonical_lower_coordinates_refuse() {
    for (offset, len) in [(127, 2), (0, 256), (126, 3)] {
        assert_eq!(
            reason(target(offset, len), Purpose::Read),
            RefusalReason::WindowCrossing
        );
    }
    for (bank, page) in [(1, 0), (0, 1), (255, 255)] {
        let mut t = target(0, 1);
        t.range.bank = bank;
        t.range.page = page;
        assert_eq!(reason(t, Purpose::Read), RefusalReason::NonCanonicalLower);
    }
}

#[test]
fn empty_and_over_bound_requests_refuse_before_allocating_a_description() {
    assert_eq!(
        check(&[], Purpose::Read).unwrap_err().reason(),
        RefusalReason::NoRanges
    );
    assert_eq!(
        check(&[target(0, 1); MAX_RANGES + 1], Purpose::Read)
            .unwrap_err()
            .reason(),
        RefusalReason::TooManyRanges
    );
    assert_eq!(MAX_BYTES, MAX_RANGES * 128);
    assert!(check(&[target(0, 1); MAX_RANGES], Purpose::Read).is_ok());
}

#[test]
fn counted_payload_is_exact_and_read_requests_cannot_carry_it() {
    for bytes in [&[][..], &[1][..], &[1, 2, 3][..]] {
        let regions = [RawRegion {
            target: target(164, 2),
            write_bytes: Some(WriteBytes::new(bytes)),
        }];
        assert_eq!(
            preflight(&RawRequest {
                revision: Rev::CMIS_4_0,
                purpose: Purpose::Write,
                regions: &regions
            })
            .unwrap_err()
            .reason(),
            RefusalReason::PayloadLength
        );
        assert_eq!(
            preflight(&RawRequest {
                revision: Rev::CMIS_4_0,
                purpose: Purpose::Read,
                regions: &regions
            })
            .unwrap_err()
            .reason(),
            RefusalReason::UnexpectedPayload
        );
    }
    let regions = [RawRegion {
        target: target(164, 2),
        write_bytes: None,
    }];
    assert_eq!(
        preflight(&RawRequest {
            revision: Rev::CMIS_4_0,
            purpose: Purpose::Write,
            regions: &regions
        })
        .unwrap_err()
        .reason(),
        RefusalReason::MissingPayload
    );
    let bytes = WriteBytes::new(b"ok");
    assert_eq!(bytes.len(), 2);
    assert!(!bytes.is_empty());
    assert_eq!(bytes.expose(), b"ok");
}

#[test]
fn each_byte_of_both_password_regions_is_secret_not_just_the_first_byte() {
    // CMIS 4.0 §8.2.10 Table 8-14; catalog spans must cover all 8 physical bytes.
    for offset in 118..=125 {
        let t = target(offset, 1);
        let classified = classify_target(Rev::CMIS_4_0, t).unwrap();
        assert_eq!(
            classified.bytes()[0].class,
            ByteClass::SecretOrWriteOnly,
            "{offset}"
        );
        for purpose in [Purpose::Read, Purpose::Write] {
            assert_eq!(
                reason(t, purpose),
                RefusalReason::SecretOrWriteOnly,
                "{offset}"
            );
        }
    }
}

#[test]
fn every_partial_password_overlap_refuses_the_whole_range() {
    for start in 117..=127 {
        for end in (start + 1)..=128 {
            if start < 126 && end > 118 {
                for purpose in [Purpose::Read, Purpose::Write] {
                    assert_eq!(
                        reason(target(start, u16::from(end - start)), purpose),
                        RefusalReason::SecretOrWriteOnly,
                        "{start}..{end}"
                    );
                }
            }
        }
    }
}

#[test]
fn selector_bytes_are_control_even_though_their_read_effect_is_none() {
    for offset in [126, 127] {
        let t = target(offset, 1);
        let classified = classify_target(Rev::CMIS_4_0, t).unwrap();
        assert_eq!(classified.bytes()[0].class, ByteClass::ControlOrCommand);
        assert!(classified.bytes()[0]
            .read
            .as_ref()
            .unwrap()
            .is_qualified_none());
        assert_eq!(reason(t, Purpose::Write), RefusalReason::ControlOrCommand);
        assert!(check(&[t], Purpose::Read).is_ok());
    }
    assert_eq!(
        reason(target(126, 2), Purpose::Write),
        RefusalReason::ControlOrCommand
    );
    assert_eq!(
        reason(target(125, 2), Purpose::Write),
        RefusalReason::SecretOrWriteOnly
    );
}

#[test]
fn other_controls_and_write_only_triggers_do_not_become_storage() {
    // CMIS 4.0 controls: bytes 26, 31, 32 have control/mask fields.
    // Byte 27 is Reserved in CMIS 4.0 (Table 8-7 p87 `27-28 All Reserved`) and
    // only gains MciSpeedConfiguration (Control) at CMIS 5.3 (Table 8-11 p149).
    for offset in [26, 31, 32] {
        let class = classify_target(Rev::CMIS_4_0, target(offset, 1))
            .unwrap()
            .bytes()[0]
            .class;
        assert!(matches!(
            class,
            ByteClass::ControlOrCommand | ByteClass::SecretOrWriteOnly
        ));
        assert!(check(&[target(offset, 1)], Purpose::Write).is_err());
    }
    // CMIS 4.0 byte 27 is Reserved -> Unknown coverage, write refused.
    assert_eq!(
        classify_target(Rev::CMIS_4_0, target(27, 1))
            .unwrap()
            .bytes()[0]
            .class,
        ByteClass::Unknown
    );
    assert_eq!(
        reason(target(27, 1), Purpose::Write),
        RefusalReason::UnknownCoverage
    );
    assert_eq!(
        reason(target(26, 1), Purpose::Read),
        RefusalReason::SecretOrWriteOnly
    );
}

#[test]
fn cmis4_cdb_trigger_staging_and_epl_are_deny_only_not_raw_storage() {
    // CMIS 4.0 §§8.13–8.14, Tables 8-106/107: all staging is denied,
    // including a request merely overlapping the command LSB at byte 129.
    for page in [0x9f, 0xa0, 0xaf] {
        for (offset, len) in [(128, 1), (128, 2), (129, 1), (130, 3), (135, 2), (255, 1)] {
            let mut t = target(offset, len);
            t.range.page = page;
            let classified = classify_target(Rev::CMIS_4_0, t).unwrap();
            assert_eq!(classified.bytes().len(), usize::from(len));
            assert!(classified
                .bytes()
                .iter()
                .all(|b| b.class == ByteClass::ControlOrCommand));
            assert_eq!(
                reason(t, Purpose::Write),
                RefusalReason::Unsupported(UnsupportedCell::Page)
            );
            assert!(check(&[t], Purpose::Read).is_err());
            assert!(classify_target(Rev::CMIS_5_3, t)
                .unwrap()
                .bytes()
                .iter()
                .all(|b| b.class == ByteClass::Unknown));
        }
    }
}

#[test]
fn missing_vendor_and_reserved_semantics_are_unknown_even_when_reads_have_no_effect() {
    for offset in [5, 63, 209, 215, 221, 223, 255] {
        let classified = classify_target(Rev::CMIS_4_0, target(offset, 1)).unwrap();
        assert_eq!(classified.bytes()[0].class, ByteClass::Unknown, "{offset}");
        for purpose in [Purpose::Read, Purpose::Write] {
            assert_eq!(
                reason(target(offset, 1), purpose),
                RefusalReason::UnknownCoverage
            );
        }
    }
    // Generated read metadata says None for 221, but its VendorSpecific tag
    // does NOT prove absence of secrets or trigger semantics.
    assert!(classify_target(Rev::CMIS_4_0, target(221, 1))
        .unwrap()
        .bytes()[0]
        .read
        .as_ref()
        .unwrap()
        .is_qualified_none());
}

#[test]
fn read_effects_are_not_silently_consumed() {
    let t = target(9, 1);
    let classified = classify_target(Rev::CMIS_4_0, t).unwrap();
    assert_eq!(classified.bytes()[0].class, ByteClass::OrdinaryReadable);
    assert!(classified.bytes()[0]
        .read
        .as_ref()
        .unwrap()
        .known_effects
        .contains(&ReadEffect::ClearOnRead));
    for purpose in [Purpose::Read, Purpose::Write] {
        assert_eq!(reason(t, purpose), RefusalReason::ReadEffectNotAdmitted);
        // A Monitor tag proves semantics, not the missing v4_0 read effect.
        assert_eq!(
            reason(target(14, 2), purpose),
            RefusalReason::ReadEffectNotAdmitted
        );
    }
}

#[test]
fn admitted_description_preserves_all_ranges_and_carries_only_exact_read_intent() {
    let targets = [target(0, 2), target(129, 16), target(222, 1)];
    let description = check(&targets, Purpose::Read).unwrap();
    assert_eq!(
        description
            .targets()
            .iter()
            .map(|t| t.target())
            .collect::<Vec<_>>(),
        targets
    );
    for d in description.targets() {
        assert_eq!(d.bytes().len(), usize::from(d.target().range.len));
        assert!(d
            .bytes()
            .iter()
            .all(|b| b.class == ByteClass::OrdinaryReadable));
    }
    let intent = description.read_intent().unwrap();
    assert_eq!(intent.exact_ranges, targets.map(|t| t.range));
    assert_eq!(intent.requester, ReadRequester::ExplicitClient);
    assert_eq!(intent.allowed_effects, [ReadEffect::None]);
    assert_eq!(description.audit().targets().unwrap(), targets);
}

#[test]
fn write_description_is_not_a_write_authorization_or_checksum_plan() {
    // Even RO identity bytes can pass this CONFIDENTIALITY gate; R2/task64
    // must separately establish access, transfer, freshness and readback.
    let targets = [target(129, 16), target(222, 1)];
    let description = check(&targets, Purpose::Write).unwrap();
    assert_eq!(
        description
            .targets()
            .iter()
            .map(|t| t.target())
            .collect::<Vec<_>>(),
        targets
    );
    assert!(description.read_intent().is_none());
    assert_eq!(description.audit().targets().unwrap(), targets);
}

#[test]
fn bad_final_range_never_returns_a_partial_description_or_audit_coordinates() {
    for bad in [
        target(118, 1),
        target(126, 1),
        target(221, 1),
        target(255, 2),
    ] {
        let refusal = check(&[target(129, 16), bad], Purpose::Write).unwrap_err();
        assert!(refusal.audit().targets().is_none());
        assert_eq!(refusal.audit().range_count(), 2);
    }
}

#[test]
fn payload_sentinel_is_absent_from_debug_display_errors_and_audit() {
    let sentinel = b"SENTINEL_SECRET!";
    let write_bytes = WriteBytes::new(sentinel);
    let regions = [RawRegion {
        target: target(129, sentinel.len() as u16),
        write_bytes: Some(write_bytes),
    }];
    let request = RawRequest {
        revision: Rev::CMIS_4_0,
        purpose: Purpose::Write,
        regions: &regions,
    };
    let description = preflight(&request).unwrap();
    let mut diagnostics = format!(
        "{write_bytes:?} {write_bytes} {regions:?} {request:?} {description:?} {:?}",
        description.audit()
    );
    for offset in [118, 126, 221] {
        let bad_regions = [RawRegion {
            target: target(offset, 1),
            write_bytes: Some(WriteBytes::new(&sentinel[..1])),
        }];
        let bad = RawRequest {
            regions: &bad_regions,
            ..request
        };
        let refusal = preflight(&bad).unwrap_err();
        diagnostics += &format!("{bad:?} {refusal:?} {refusal} {:?}", refusal.audit());
    }
    for representation in [
        String::from_utf8_lossy(sentinel).into_owned(),
        format!("{sentinel:?}"),
        sentinel.iter().map(|b| format!("{b:02x}")).collect(),
    ] {
        assert!(
            !diagnostics.contains(&representation),
            "payload leaked: {diagnostics}"
        );
    }
    assert!(!format!("{:?}", regions[0]).contains("offset"));
}

#[test]
fn description_and_audit_are_deterministic_and_independent_of_payload() {
    let a = [RawRegion {
        target: target(129, 16),
        write_bytes: Some(WriteBytes::new(b"SECRET_VALUE_ONE")),
    }];
    let b = [RawRegion {
        target: target(129, 16),
        write_bytes: Some(WriteBytes::new(b"OTHER_VALUE_TWO!")),
    }];
    let request = RawRequest {
        revision: Rev::CMIS_4_0,
        purpose: Purpose::Write,
        regions: &a,
    };
    let first = preflight(&request).unwrap();
    let second = preflight(&RawRequest {
        regions: &b,
        ..request
    })
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.audit(), second.audit());
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(first, preflight(&request).unwrap());
}

#[test]
fn every_physically_overlapping_generated_password_span_is_covered() {
    let catalog = openxcvr::catalog::family_catalog(Family::Cmis).unwrap();
    let mut covered = [false; 128];
    let mut spans = 0;
    for row in catalog
        .rows
        .iter()
        .filter(|r| r.page_label == "lower" && r.tags.contains(&"BlindControl::Password"))
    {
        spans += 1;
        for offset in row.offset..row.offset + row.len {
            covered[usize::from(offset)] = true;
            assert_eq!(
                classify_target(Rev::CMIS_4_0, target(offset as u8, 1))
                    .unwrap()
                    .bytes()[0]
                    .class,
                ByteClass::SecretOrWriteOnly
            );
        }
    }
    assert_eq!(spans, 2);
    assert_eq!(covered.iter().filter(|&&b| b).count(), 8);
    assert!(covered[118..126].iter().all(|&b| b));
}
