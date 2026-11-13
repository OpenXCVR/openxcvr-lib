use openxcvr::{
    core::{rev::Family, types::management::Range, Rev},
    programming::{plan_named_vendor_programming, FieldValue, ProgrammingRefusal as Refusal},
};
use openxcvr_cmis::vendor_programming::{plan_vendor_programming, validate_write, WriteRequest};

fn plan(
    fields: &[(&str, FieldValue<'_>)],
) -> Result<openxcvr::programming::NamedProgrammingPlan, Refusal> {
    plan_named_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[0xa5; 128], fields)
}

#[test]
fn resolves_names_and_matches_the_raw_plan_authority() {
    let named = plan(&[("vendor_oui", FieldValue::Bytes(&[0x48, 0xb0, 0x2d]))]).unwrap();
    let r = Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset: 145,
        len: 3,
    };
    assert_eq!(named.fields[0].name, "vendor_oui");
    assert_eq!(named.fields[0].range, r);
    let ranges = vec![(r, vec![0x48, 0xb0, 0x2d])];
    assert_eq!(
        named.plan,
        plan_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &ranges).unwrap()
    );
    assert_eq!(
        named.plan,
        validate_write(
            &WriteRequest::VendorProgramming {
                password: [0; 4],
                ranges
            },
            Rev::CMIS_4_0,
            &Default::default()
        )
        .unwrap()
    );
}

#[test]
fn duplicate_unknown_and_nonexact_names_are_typed_refusals() {
    assert!(matches!(
        plan(&[
            ("vendor_oui", FieldValue::Bytes(&[0; 3])),
            ("vendor_oui", FieldValue::Bytes(&[1; 3]))
        ]),
        Err(Refusal::DuplicateField { .. })
    ));
    assert!(matches!(
        plan(&[("nonexistent", FieldValue::Text("x"))]),
        Err(Refusal::UnknownField { .. })
    ));
    assert!(matches!(
        plan(&[("page_checksum", FieldValue::Bytes(&[1]))]),
        Err(Refusal::AmbiguousField { .. })
    ));
    assert!(matches!(
        plan(&[("vendor_*", FieldValue::Text("x"))]),
        Err(Refusal::NonExactName { .. })
    ));
}

#[test]
fn read_only_and_nonadmitted_fields_never_gain_standard_write_permission() {
    assert!(matches!(
        plan(&[("clei_code", FieldValue::Text("0123456789"))]),
        Err(Refusal::ReadOnlyField { .. })
    ));
    assert!(matches!(
        plan(&[("low_pwr_request_sw", FieldValue::Bytes(&[1]))]),
        Err(Refusal::NotAdmitted { .. })
    ));
}

#[test]
fn malformed_and_out_of_range_values_are_refused_not_truncated_or_padded() {
    assert!(matches!(
        plan(&[("vendor_oui", FieldValue::Bytes(&[0; 4]))]),
        Err(Refusal::OutOfRange { .. })
    ));
    assert!(matches!(
        plan(&[("vendor_oui", FieldValue::Text("abc"))]),
        Err(Refusal::MalformedValue { .. })
    ));
    for text in ["é", "\n\n"] {
        assert!(matches!(
            plan(&[("vendor_rev", FieldValue::Text(text))]),
            Err(Refusal::MalformedValue { .. })
        ));
    }
    assert!(matches!(
        plan(&[("vendor_rev", FieldValue::Text("A"))]),
        Err(Refusal::OutOfRange { .. })
    ));
}

#[test]
fn bad_pair_refuses_the_whole_request_without_modifying_current_bytes() {
    let current = [0xa5; 128];
    let result = plan_named_vendor_programming(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &current,
        &[
            ("vendor_oui", FieldValue::Bytes(&[1; 3])),
            ("vendor_rev", FieldValue::Text("too long")),
        ],
    );
    assert!(matches!(result, Err(Refusal::OutOfRange { .. })));
    assert_eq!(current, [0xa5; 128]);
}

#[test]
fn only_catalog_whole_byte_fields_are_admitted_and_all_other_bits_are_preserved() {
    let named = plan(&[("vendor_rev", FieldValue::Text("A1"))]).unwrap();
    let mut expected = [0xa5; 128];
    expected[36..38].copy_from_slice(b"A1");
    assert_eq!(named.encoded_page, expected);
    // CMIS4 shared-byte fields start beyond the existing admitted 129..190
    // half-open range. A success here would silently broaden vendor authority.
    for field in ["module_power_class", "length_multiplier", "base_length"] {
        assert!(matches!(
            plan(&[(field, FieldValue::Bytes(&[1]))]),
            Err(Refusal::ReadOnlyField { .. })
        ));
    }
}

#[test]
fn every_admitted_generated_setter_is_reachable_without_range_widening() {
    for (name, value, offset, len) in [
        ("vendor_name", FieldValue::Text("0123456789abcdef"), 129, 16),
        ("vendor_oui", FieldValue::Bytes(&[1, 2, 3]), 145, 3),
        ("vendor_pn", FieldValue::Text("0123456789abcdef"), 148, 16),
        ("vendor_rev", FieldValue::Text("A1"), 164, 2),
        ("vendor_sn", FieldValue::Text("0123456789abcdef"), 166, 16),
        ("date_code_year", FieldValue::Text("26"), 182, 2),
        ("date_code_month", FieldValue::Text("10"), 184, 2),
        ("date_code_day_of_month", FieldValue::Text("20"), 186, 2),
        ("date_code_lot_code", FieldValue::Text("AB"), 188, 2),
    ] {
        let result = plan(&[(name, value)]).unwrap();
        assert_eq!(result.fields[0].range.offset, offset);
        assert_eq!(result.fields[0].range.len, len);
        assert_eq!(result.ranges[0].0, result.fields[0].range);
    }
}

#[test]
fn unknown_revision_missing_password_empty_request_and_short_current_refuse() {
    let fields = [("vendor_oui", FieldValue::Bytes(&[0; 3]))];
    for rev in [
        Rev::CMIS_5_3,
        Rev {
            family: Family::Cmis,
            major: 9,
            minor: 9,
        },
    ] {
        assert_eq!(
            plan_named_vendor_programming(rev, Some(&[0; 4]), &[0; 128], &fields).unwrap_err(),
            Refusal::UnsupportedRevision(rev)
        );
    }
    assert!(plan_named_vendor_programming(Rev::CMIS_4_0, None, &[0; 128], &fields).is_err());
    assert!(plan(&[]).is_err());
    assert_eq!(
        plan_named_vendor_programming(Rev::CMIS_4_0, Some(&[0; 4]), &[0; 127], &fields)
            .unwrap_err(),
        Refusal::InvalidCurrentLength
    );
}

#[test]
fn named_policy_defaults_to_maintenance_and_never_uses_encoding_bytes_as_fresh_evidence() {
    use openxcvr::core::types::management::{CacheEpoch, ModuleGeneration, MonoTime, OperationId};
    use openxcvr::programming::{
        ProgrammingDisposition, VendorProgrammingOptions, VendorProgrammingScope,
    };
    let named = plan(&[("vendor_oui", FieldValue::Bytes(&[0xa5; 3]))]).unwrap();
    let scope = VendorProgrammingScope {
        operation: OperationId(1),
        generation: ModuleGeneration(2),
        epoch: CacheEpoch(3),
        exclusive: true,
        started_at: MonoTime(10),
        now: MonoTime(20),
    };
    let policy = named
        .with_options(
            Some(&[0; 4]),
            VendorProgrammingOptions::default(),
            scope,
            None,
        )
        .unwrap();
    assert_eq!(policy.plan, named.plan);
    assert_eq!(
        policy.disposition,
        ProgrammingDisposition::RequiresExecution
    );
    assert_eq!(policy.finish_reads[0].physical_range.offset, 128);
    assert_eq!(policy.finish_reads[0].physical_range.len, 95);
}
