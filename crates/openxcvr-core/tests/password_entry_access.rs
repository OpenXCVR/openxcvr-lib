// SPDX-License-Identifier: MIT
use openxcvr_core::{access::*, types::management::ReadEffect};

const INSUFFICIENT: AccessDecision = AccessDecision::Refuse(AccessRefusal::FactsInsufficient);
const NOT_ALLOWED: AccessDecision = AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed);
const NO_WRITE_GROUP: AccessDecision = AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed);
const READ: AccessDecision = AccessDecision::ReadOnly;
const WRITE: AccessDecision = AccessDecision::Write {
    group: "vendor_programming",
    read_effect: ReadEffect::Unknown,
};

const NONE: &[&str] = &[];
const VENDOR: &[&str] = &["vendor_programming"];
const OTHER: &[&str] = &["other"];
const BOTH: &[&str] = &["other", "vendor_programming"];

struct Case {
    resolved: bool,
    read_allowed: bool,
    groups: &'static [&'static str],
    read: AccessDecision,
    write: AccessDecision,
}

fn cases() -> impl Iterator<Item = Case> {
    // Explicit decision table: principal x read allow-list x write groups.
    [
        (false, false, NONE, INSUFFICIENT, INSUFFICIENT),
        (false, false, VENDOR, INSUFFICIENT, INSUFFICIENT),
        (false, false, OTHER, INSUFFICIENT, INSUFFICIENT),
        (false, false, BOTH, INSUFFICIENT, INSUFFICIENT),
        (false, true, NONE, INSUFFICIENT, INSUFFICIENT),
        (false, true, VENDOR, INSUFFICIENT, INSUFFICIENT),
        (false, true, OTHER, INSUFFICIENT, INSUFFICIENT),
        (false, true, BOTH, INSUFFICIENT, INSUFFICIENT),
        (true, false, NONE, NOT_ALLOWED, NOT_ALLOWED),
        (true, false, VENDOR, NOT_ALLOWED, NOT_ALLOWED),
        (true, false, OTHER, NOT_ALLOWED, NOT_ALLOWED),
        (true, false, BOTH, NOT_ALLOWED, NOT_ALLOWED),
        (true, true, NONE, READ, NO_WRITE_GROUP),
        (true, true, VENDOR, READ, WRITE),
        (true, true, OTHER, READ, NO_WRITE_GROUP),
        (true, true, BOTH, READ, WRITE),
    ]
    .into_iter()
    .map(|(resolved, read_allowed, groups, read, write)| Case {
        resolved,
        read_allowed,
        groups,
        read,
        write,
    })
}

impl Case {
    fn facts(&self) -> AccessFacts {
        AccessFacts {
            uid: 1000,
            gid: 1000,
            principal: self.resolved.then(|| AccessPrincipal {
                user: "bench".into(),
                group: "bench".into(),
            }),
            read_allowed: self.read_allowed,
            write_groups: self.groups.iter().map(|group| (*group).into()).collect(),
        }
    }
}

#[test]
fn password_entry_matches_write_memory_for_the_complete_permission_matrix() {
    for (index, case) in cases().enumerate() {
        let facts = case.facts();
        let legacy = evaluate_access(&facts, "write_memory");
        assert_eq!(legacy, case.write, "write_memory case {index}: {facts:?}");
        assert_eq!(
            evaluate_access(&facts, "password_entry"),
            legacy,
            "password_entry case {index}: {facts:?}"
        );
    }
}

#[test]
fn every_base_operation_retains_its_explicit_expected_decisions() {
    for (index, case) in cases().enumerate() {
        let facts = case.facts();
        // List all nine base operations explicitly: neither ALL nor another
        // evaluator's output is the compatibility oracle.
        for (operation, expected) in [
            ("list_modules", case.read),
            ("get_status", case.read),
            ("get_identity", case.read),
            ("get_capabilities", case.read),
            ("observe", case.read),
            ("get_operation", case.read),
            ("get_audit", case.read),
            ("write_memory", case.write),
            ("program_fields", case.write),
        ] {
            assert_eq!(
                evaluate_access(&facts, operation),
                expected,
                "{operation} case {index}: {facts:?}"
            );
        }
    }
}

#[test]
fn unknown_names_remain_undelivered_before_any_peer_fact() {
    for (index, case) in cases().enumerate() {
        let facts = case.facts();
        for operation in [
            "",
            "PASSWORD_ENTRY",
            "password_entry_other",
            "password_entry ",
            "vendor_programming",
            "set_power",
            "set_lowpower_hold",
            "wait_manageable",
            "run_cdb",
            "arbitrary",
        ] {
            assert_eq!(
                evaluate_access(&facts, operation),
                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered),
                "{operation:?} case {index}: {facts:?}"
            );
        }
    }
}

#[test]
fn additive_name_preserves_the_legacy_inventory_type_and_contents() {
    let _: [&str; 9] = delivered::ALL;
    assert_eq!(delivered::PASSWORD_ENTRY, "password_entry");
    assert!(!delivered::ALL.contains(&delivered::PASSWORD_ENTRY));
}
