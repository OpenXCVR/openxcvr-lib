use openxcvr_cmis::{generated::v4_0::CmisLowerMemory, password_entry::*};
use openxcvr_core::{rev::Family, types::management::Invalidation, Rev};

const SENTINEL: [u8; 4] = *b"qZ9!";
fn prepare() -> PasswordEntryOperation {
    plan_password_entry(Family::Cmis, 0x40, Some(&SENTINEL)).unwrap()
}

#[test]
fn exact_one_write_with_generated_big_endian_encoding_and_nothing_else() {
    let operation = prepare();
    let plan = operation.plan();
    let write = plan.write();
    assert_eq!(
        (
            write.address,
            write.bank,
            write.page,
            write.offset,
            write.len
        ),
        (0x50, 0, 0, 122, 4)
    );
    assert_eq!(plan.revision(), Rev::CMIS_4_0);
    assert_eq!(plan.max_retries(), 0);
    assert_eq!(
        plan.unconfirmed_retry(),
        openxcvr_core::types::management::operations::RetryDecision::Terminal(
            openxcvr_core::types::management::ActuationOutcome::Uncertain {
                reason: "step-unconfirmed".into()
            }
        )
    );
    assert!(!plan.readback());
    assert!(!plan.probe());
    assert!(!plan.selector_change());
    assert!(!plan.checksum());
    let mut generated = [0; 128];
    CmisLowerMemory::set_password_entry_area(&mut generated, u32::from_be_bytes(SENTINEL));
    assert_eq!(operation.secret().as_bytes(), &generated[122..126]);
    assert_eq!(operation.secret().as_bytes(), &SENTINEL);
    assert_eq!(
        plan.invalidations(),
        &[Invalidation::Covered(write), Invalidation::Password]
    );
}

#[test]
fn only_reported_cmis40_is_admitted() {
    for revision in 0..=255 {
        let result = plan_password_entry(Family::Cmis, revision, Some(&SENTINEL));
        if revision == 0x40 {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.unwrap_err(),
                PasswordEntryRefusal::UnsupportedRevision(revision)
            );
        }
    }
    for family in [
        Family::Sff8472,
        Family::Sff8636,
        Family::CCmis,
        Family::Elsfp,
    ] {
        assert_eq!(
            plan_password_entry(family, 0x40, Some(&SENTINEL)).unwrap_err(),
            PasswordEntryRefusal::UnsupportedFamily(family)
        );
    }
}

#[test]
fn missing_or_wrong_width_refuses_before_any_plan() {
    assert_eq!(
        plan_password_entry(Family::Cmis, 0x40, None).unwrap_err(),
        PasswordEntryRefusal::MissingSecret
    );
    for len in [0, 1, 2, 3, 5, 6, 128] {
        let bytes = vec![81; len];
        assert_eq!(
            plan_password_entry(Family::Cmis, 0x40, Some(&bytes)).unwrap_err(),
            PasswordEntryRefusal::InvalidSecretWidth
        );
    }
}

#[test]
fn closed_outcome_projection_never_claims_verification_or_unlock() {
    for (fact, expected, possible, invalidate) in [
        (
            PasswordEntryFact::Admitted,
            PasswordEntryEvidence::Accepted,
            false,
            false,
        ),
        (
            PasswordEntryFact::Submitted,
            PasswordEntryEvidence::Accepted,
            true,
            true,
        ),
        (
            PasswordEntryFact::TransportCompleted,
            PasswordEntryEvidence::Completed,
            true,
            true,
        ),
        (
            PasswordEntryFact::ApplicationPossible,
            PasswordEntryEvidence::Uncertain,
            true,
            true,
        ),
        (
            PasswordEntryFact::Refused,
            PasswordEntryEvidence::Refused,
            false,
            false,
        ),
        (
            PasswordEntryFact::FailedBeforeSubmission,
            PasswordEntryEvidence::FailedBeforeSubmission,
            false,
            false,
        ),
    ] {
        let outcome = PasswordEntryOutcome::from(fact);
        assert_eq!(outcome.evidence(), expected);
        assert!(outcome.verification_not_performed());
        assert_eq!(outcome.possible_application(), possible);
        assert_eq!(!outcome.invalidations().is_empty(), invalidate);
        if invalidate {
            assert_eq!(outcome.invalidations(), prepare().plan().invalidations());
        }
        let text = format!("{outcome:?} {outcome}");
        for forbidden in [
            "stored_bytes_verified",
            "resulting_state_verified",
            "unlocked",
        ] {
            assert!(!text.contains(forbidden));
        }
    }
}

#[test]
fn every_diagnostic_surface_is_secret_free() {
    let operation = prepare();
    let text = format!(
        "{operation:?} {operation} {:?} {} {:?} {} {:?} {}",
        operation.secret(),
        operation.secret(),
        operation.plan(),
        operation.plan(),
        PasswordEntryRefusal::InvalidSecretWidth,
        PasswordEntryRefusal::InvalidSecretWidth
    );
    for sentinel in [
        "qZ9!",
        "715a3921",
        "0x71, 0x5a, 0x39, 0x21",
        "113, 90, 57, 33",
    ] {
        assert!(!text.contains(sentinel), "leaked {sentinel}");
        for fact in [
            PasswordEntryFact::Admitted,
            PasswordEntryFact::Submitted,
            PasswordEntryFact::TransportCompleted,
            PasswordEntryFact::ApplicationPossible,
            PasswordEntryFact::Refused,
            PasswordEntryFact::FailedBeforeSubmission,
        ] {
            let outcome = PasswordEntryOutcome::from(fact);
            assert!(!format!("{outcome:?} {outcome}").contains(sentinel));
        }
    }
}

#[test]
fn revision_typed_entry_admits_only_cmis40_and_matches_the_byte_form() {
    let by_rev = plan_password_entry_for_revision(Rev::CMIS_4_0, Some(&SENTINEL)).unwrap();
    let by_byte = prepare();
    assert_eq!(by_rev.plan(), by_byte.plan());
    assert_eq!(by_rev.secret().as_bytes(), by_byte.secret().as_bytes());
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        assert_eq!(
            plan_password_entry_for_revision(revision, Some(&SENTINEL)).unwrap_err(),
            PasswordEntryRefusal::UnsupportedRevisionOf(revision)
        );
    }
    let sff = Rev {
        family: Family::Sff8636,
        major: 4,
        minor: 0,
    };
    assert_eq!(
        plan_password_entry_for_revision(sff, Some(&SENTINEL)).unwrap_err(),
        PasswordEntryRefusal::UnsupportedFamily(Family::Sff8636)
    );
    assert_eq!(
        plan_password_entry_for_revision(Rev::CMIS_4_0, None).unwrap_err(),
        PasswordEntryRefusal::MissingSecret
    );
    assert_eq!(
        plan_password_entry_for_revision(Rev::CMIS_4_0, Some(&[0; 3])).unwrap_err(),
        PasswordEntryRefusal::InvalidSecretWidth
    );
}
