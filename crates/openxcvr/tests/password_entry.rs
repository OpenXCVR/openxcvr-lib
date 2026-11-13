use openxcvr::core::rev::Family;
use openxcvr::programming::{
    plan_password_entry, PasswordEntryEvidence, PasswordEntryFact, PasswordEntryOutcome,
};

#[test]
fn facade_exposes_the_standalone_contract() {
    let operation = plan_password_entry(Family::Cmis, 0x40, Some(&[1, 2, 3, 4])).unwrap();
    assert_eq!(operation.plan().write().offset, 122);
    assert_eq!(
        PasswordEntryOutcome::from(PasswordEntryFact::TransportCompleted).evidence(),
        PasswordEntryEvidence::Completed
    );
}

#[test]
fn facade_exposes_the_revision_typed_entry() {
    use openxcvr::core::Rev;
    use openxcvr::programming::plan_password_entry_for_revision;
    let operation = plan_password_entry_for_revision(Rev::CMIS_4_0, Some(&[1, 2, 3, 4])).unwrap();
    assert_eq!(operation.plan().write().offset, 122);
    assert!(plan_password_entry_for_revision(Rev::CMIS_5_3, Some(&[1, 2, 3, 4])).is_err());
}
