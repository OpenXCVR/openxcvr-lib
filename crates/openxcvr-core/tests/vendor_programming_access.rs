use openxcvr_core::{access::*, types::management::ReadEffect};

fn facts() -> AccessFacts {
    AccessFacts {
        uid: 1000,
        gid: 1000,
        principal: Some(AccessPrincipal {
            user: "bench".into(),
            group: "bench".into(),
        }),
        read_allowed: true,
        write_groups: vec!["vendor_programming".into()],
    }
}

#[test]
fn vendor_programming_requires_a_separate_supplied_write_group() {
    let mut f = facts();
    assert_eq!(
        evaluate_access(&f, "write_memory"),
        AccessDecision::Write {
            group: "vendor_programming",
            read_effect: ReadEffect::Unknown
        }
    );
    f.write_groups.clear();
    assert_eq!(
        evaluate_access(&f, "write_memory"),
        AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed)
    );
    f.write_groups.push("other".into());
    assert_eq!(
        evaluate_access(&f, "write_memory"),
        AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed)
    );
    f = facts();
    f.principal = None;
    assert_eq!(
        evaluate_access(&f, "write_memory"),
        AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
    );
    f = facts();
    f.read_allowed = false;
    assert_eq!(
        evaluate_access(&f, "write_memory"),
        AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed)
    );
    for operation in ["set_power", "set_lowpower_hold"] {
        assert_eq!(
            evaluate_access(&facts(), operation),
            AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
        );
    }
}
