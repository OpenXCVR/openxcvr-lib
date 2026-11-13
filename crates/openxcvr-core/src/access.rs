// SPDX-License-Identifier: MIT
//! Supplied-peer access evaluation for passive reads and vendor programming.
//!
//! This pure gate owns no credentials, configuration, transport, or live state.
//! Reads need a resolved principal and the supplied read allow-list fact.
//! `get_operation` is read-only diagnostic access to an existing operation;
//! the daemon must still enforce requester ownership and secret-free views.
//! `get_audit` is read-only retrieval of secret-free durable audit metadata,
//! admitted to allow-listed readers by the daemon client contract. It has no
//! requester-only scope; the daemon remains responsible for secret-free views.
//! AccessFacts has no operation id or owner fact: this gate cannot decide
//! which record belongs to a requester and does not invent that authority.
//! `write_memory`, `program_fields` and `password_entry` need the explicit
//! `vendor_programming` write group. All return a write grant, never disguise
//! a write as `ReadOnly`.
//! The write grant does not admit ranges or standard RO writes: the CMIS
//! vendor-programming planner supplies that narrow validation and exact steps;
//! the daemon retains route receipts, exclusive execution, and durable audit.
//!
//! Passive read-effect admission remains in observation planning. Vendor
//! programming includes exact internal checksum/verify reads whose metadata
//! includes Unknown, so its access decision explicitly does NOT claim None.
//! Power, hold, readiness and CDB remain undelivered.
use crate::types::management::ReadEffect;

/// The delivered operations this evaluation may grant.
///
/// Every other operation name is refused with
/// [`AccessRefusal::OperationNotDelivered`]; admitting a name here is a
/// library delivery decision, never a caller's.
pub mod delivered {
    /// `list_modules`: enumerate daemon-held module instances.
    pub const LIST_MODULES: &str = "list_modules";
    /// `get_status`: one module's status.
    pub const GET_STATUS: &str = "get_status";
    /// `get_identity`: a decoded module identity.
    pub const GET_IDENTITY: &str = "get_identity";
    /// `get_capabilities`: a module's decoded capability set.
    pub const GET_CAPABILITIES: &str = "get_capabilities";
    /// `observe`: bounded passive observation over requested ranges.
    pub const OBSERVE: &str = "observe";
    /// `get_operation`: requester-scoped diagnostic metadata, no hardware act.
    pub const GET_OPERATION: &str = "get_operation";

    /// `get_audit`: secret-free durable audit metadata, no hardware act.
    pub const GET_AUDIT: &str = "get_audit";

    /// `write_memory`: only the separately admitted vendor-programming seam.
    pub const WRITE_MEMORY: &str = "write_memory";
    /// `program_fields`: named fields through the vendor-programming seam.
    pub const PROGRAM_FIELDS: &str = "program_fields";
    /// `password_entry`: standalone CMIS 4.0 entry, under the same explicit
    /// write group as `program_fields`, never a read-only grant.
    pub const PASSWORD_ENTRY: &str = "password_entry";
    /// The sole delivered write group (not a standard page-write permission).
    pub const VENDOR_PROGRAMMING: &str = "vendor_programming";

    /// Legacy delivered inventory, preserved in type and contents for consumers.
    /// The additive [`PASSWORD_ENTRY`] name is also evaluated, as a write.
    pub const ALL: [&str; 9] = [
        LIST_MODULES,
        GET_STATUS,
        GET_IDENTITY,
        GET_CAPABILITIES,
        OBSERVE,
        GET_OPERATION,
        GET_AUDIT,
        WRITE_MEMORY,
        PROGRAM_FIELDS,
    ];
}

/// The principal a platform credential resolved to, as a supplied fact.
///
/// A principal is a *fact the caller derived*, never a claim the wire
/// supplied: a peer whose credential did not resolve to a usable principal
/// is carried as `None` in [`AccessFacts`] rather than synthesized here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPrincipal {
    /// Resolved login name.
    pub user: String,
    /// Resolved primary group name.
    pub group: String,
}

/// The platform-verified peer facts one access evaluation is supplied with.
///
/// Every member is derived from a platform-verified credential and a
/// validated configuration by the caller. Membership in the configured
/// read allow-list is supplied as the fact it is: this evaluation invents
/// no roster, reads no configuration and resolves no names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessFacts {
    /// Numeric user id the platform reported.
    pub uid: u32,
    /// Numeric primary group id the platform reported.
    pub gid: u32,
    /// The principal the numeric identity resolved to, or `None` when it
    /// resolved to none.
    pub principal: Option<AccessPrincipal>,
    /// Whether the peer's identity is an entry in the configured read
    /// allow-list.
    pub read_allowed: bool,
    /// Explicit write groups derived by the daemon from verified credentials
    /// and configuration. An empty list grants no writes, even to a reader.
    pub write_groups: Vec<String>,
}

/// Why one access request was refused.
///
/// Each variant is typed and attributable; none is a bare boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRefusal {
    /// The peer facts are insufficient for a decision: the platform
    /// credential resolved to no usable principal.
    FactsInsufficient,
    /// The verified peer principal is not permitted: it is not an entry in
    /// the configured read allow-list.
    PrincipalNotAllowed,
    /// The peer lacks the operation's separately supplied write-group fact.
    WriteGroupNotAllowed,
    /// The named operation is outside the delivered subset, so no
    /// evaluation for it exists at this revision.
    OperationNotDelivered,
}

impl AccessRefusal {
    /// Stable identifier for events and typed refusals.
    pub const fn id(self) -> &'static str {
        match self {
            AccessRefusal::FactsInsufficient => "facts-insufficient",
            AccessRefusal::PrincipalNotAllowed => "principal-not-allowed",
            AccessRefusal::WriteGroupNotAllowed => "write-group-not-allowed",
            AccessRefusal::OperationNotDelivered => "operation-not-delivered",
        }
    }
}

/// The decision one access evaluation returned.
///
/// A read grant is zero-effect; a write grant explicitly names its delivered
/// group and conservative read-effect fact. Neither executes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    /// Permitted, and nothing executes: a zero-effect read-only grant.
    ReadOnly,
    /// Supplied peer facts permit this write group, subject to range/receipt
    /// admission. Unknown read effects are NOT passive-read authorization.
    Write {
        /// The permitted write group's name.
        group: &'static str,
        /// The read effect reading the group's bytes carries.
        read_effect: ReadEffect,
    },
    /// Denied, for the stated typed reason.
    Refuse(AccessRefusal),
}

/// Evaluate one access request over the supplied peer facts.
///
/// The evaluation order is fixed and total:
///
/// 1. An operation name outside [`delivered::ALL`] and not
///    [`delivered::PASSWORD_ENTRY`] is refused
///    ([`AccessRefusal::OperationNotDelivered`]) before any fact is
///    consulted, so an undelivered operation discloses nothing about the
///    peer.
/// 2. An unresolved principal is refused ([`AccessRefusal::FactsInsufficient`]):
///    an identity that resolved to no usable principal is insufficient
///    facts, never an implicit grant or denial by numeric id.
/// 3. A resolved principal outside the configured read allow-list is
///    refused ([`AccessRefusal::PrincipalNotAllowed`]).
/// 4. A write additionally requires its named write group; absence is refused
///    ([`AccessRefusal::WriteGroupNotAllowed`]).
/// 5. Otherwise return the read-only or explicit write-group grant.
///
/// The grant is never unconditional: it requires both a resolved principal
/// and the supplied allow-list fact, so a different configuration or an
/// unresolved credential yields the typed refusal, not the grant.
pub fn evaluate_access(facts: &AccessFacts, operation: &str) -> AccessDecision {
    if operation != delivered::PASSWORD_ENTRY && !delivered::ALL.contains(&operation) {
        return AccessDecision::Refuse(AccessRefusal::OperationNotDelivered);
    }
    if facts.principal.is_none() {
        return AccessDecision::Refuse(AccessRefusal::FactsInsufficient);
    }
    if !facts.read_allowed {
        return AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed);
    }
    if operation == delivered::WRITE_MEMORY
        || operation == delivered::PROGRAM_FIELDS
        || operation == delivered::PASSWORD_ENTRY
    {
        if !facts
            .write_groups
            .iter()
            .any(|group| group == delivered::VENDOR_PROGRAMMING)
        {
            return AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed);
        }
        return AccessDecision::Write {
            group: delivered::VENDOR_PROGRAMMING,
            read_effect: ReadEffect::Unknown,
        };
    }
    AccessDecision::ReadOnly
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(principal: Option<AccessPrincipal>, read_allowed: bool) -> AccessFacts {
        AccessFacts {
            uid: 1000,
            gid: 1000,
            principal,
            read_allowed,
            write_groups: vec![],
        }
    }

    fn resolved() -> Option<AccessPrincipal> {
        Some(AccessPrincipal {
            user: "bench".to_string(),
            group: "bench".to_string(),
        })
    }

    #[test]
    fn a_delivered_read_with_a_resolved_permitted_principal_is_read_only() {
        for operation in delivered::ALL
            .into_iter()
            .filter(|op| *op != delivered::WRITE_MEMORY && *op != delivered::PROGRAM_FIELDS)
        {
            assert_eq!(
                evaluate_access(&facts(resolved(), true), operation),
                AccessDecision::ReadOnly
            );
        }
    }

    #[test]
    fn vendor_programming_grant_requires_its_explicit_write_group() {
        let mut facts = facts(resolved(), true);
        assert_eq!(
            evaluate_access(&facts, delivered::WRITE_MEMORY),
            AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed)
        );
        facts.write_groups.push("other".into());
        assert_eq!(
            evaluate_access(&facts, delivered::WRITE_MEMORY),
            AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed)
        );
        facts
            .write_groups
            .push(delivered::VENDOR_PROGRAMMING.into());
        assert_eq!(
            evaluate_access(&facts, delivered::WRITE_MEMORY),
            AccessDecision::Write {
                group: "vendor_programming",
                read_effect: ReadEffect::Unknown,
            }
        );
    }

    #[test]
    fn an_operation_outside_the_delivered_subset_is_refused_before_any_fact() {
        for operation in [
            "set_power",
            "set_lowpower_hold",
            "wait_manageable",
            "run_cdb",
        ] {
            assert_eq!(
                evaluate_access(&facts(resolved(), true), operation),
                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
            );
            // The refusal precedes fact inspection: unresolved facts do not
            // change it.
            assert_eq!(
                evaluate_access(&facts(None, false), operation),
                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
            );
        }
    }

    #[test]
    fn an_unresolved_principal_is_insufficient_facts() {
        for operation in delivered::ALL {
            assert_eq!(
                evaluate_access(&facts(None, true), operation),
                AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
            );
        }
    }

    #[test]
    fn a_principal_outside_the_read_allow_list_is_not_permitted() {
        for operation in delivered::ALL {
            assert_eq!(
                evaluate_access(&facts(resolved(), false), operation),
                AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed)
            );
        }
    }

    #[test]
    fn the_grant_is_never_unconditional() {
        // Both facts are required: removing either yields the typed refusal.
        assert_ne!(
            evaluate_access(&facts(resolved(), true), delivered::GET_IDENTITY),
            AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
        );
        assert_eq!(
            evaluate_access(&facts(None, true), delivered::GET_IDENTITY),
            AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
        );
        assert_eq!(
            evaluate_access(&facts(resolved(), false), delivered::GET_IDENTITY),
            AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed)
        );
    }

    #[test]
    fn get_operation_is_read_only_and_peer_facts_are_independent() {
        for principal in [None, resolved()] {
            for read_allowed in [false, true] {
                for groups in [
                    vec![],
                    vec!["other".into()],
                    vec!["vendor_programming".into()],
                ] {
                    let mut peer = facts(principal.clone(), read_allowed);
                    peer.write_groups = groups;
                    // Numeric root credentials alone do not repair missing facts.
                    for (uid, gid) in [(0, 0), (1000, 1000)] {
                        peer.uid = uid;
                        peer.gid = gid;
                        let expected = if principal.is_none() {
                            AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
                        } else if !read_allowed {
                            AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed)
                        } else {
                            AccessDecision::ReadOnly
                        };
                        assert_eq!(evaluate_access(&peer, "get_operation"), expected);
                        for unknown in [
                            "",
                            "GET_OPERATION",
                            "get_operation_other",
                            "cancel_operation",
                            "arbitrary",
                        ] {
                            assert_eq!(
                                evaluate_access(&peer, unknown),
                                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
                            );
                        }
                    }
                }
            }
        }
        let peer = facts(resolved(), true);
        assert_eq!(
            evaluate_access(&peer, "get_operation"),
            AccessDecision::ReadOnly
        );
        assert_eq!(
            evaluate_access(&peer, "write_memory"),
            AccessDecision::Refuse(AccessRefusal::WriteGroupNotAllowed)
        );
        assert!(!delivered::ALL
            .iter()
            .enumerate()
            .any(|(i, name)| delivered::ALL[..i].contains(name)));
    }

    #[test]
    fn get_audit_has_the_same_read_only_peer_rules_as_get_operation() {
        for principal in [None, resolved()] {
            for read_allowed in [false, true] {
                for write_groups in [vec![], vec!["vendor_programming".into()]] {
                    let mut peer = facts(principal.clone(), read_allowed);
                    peer.write_groups = write_groups;
                    assert_eq!(
                        evaluate_access(&peer, "get_audit"),
                        evaluate_access(&peer, "get_operation")
                    );
                    assert_eq!(
                        evaluate_access(&peer, "get_audit_unknown"),
                        AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
                    );
                }
            }
        }
    }

    #[test]
    fn refusal_identifiers_are_stable_and_distinct() {
        let ids = [
            AccessRefusal::WriteGroupNotAllowed.id(),
            AccessRefusal::FactsInsufficient.id(),
            AccessRefusal::PrincipalNotAllowed.id(),
            AccessRefusal::OperationNotDelivered.id(),
        ];
        for (index, id) in ids.iter().enumerate() {
            assert!(!id.is_empty());
            assert!(!ids[..index].contains(id));
        }
    }

    #[test]
    fn observe_is_granted_under_the_same_conditions_as_get_identity() {
        assert_eq!(
            evaluate_access(&facts(resolved(), true), "observe"),
            AccessDecision::ReadOnly
        );
        assert_eq!(
            evaluate_access(&facts(resolved(), true), "observe"),
            evaluate_access(&facts(resolved(), true), delivered::GET_IDENTITY)
        );
    }

    #[test]
    fn observe_is_refused_when_principal_is_unresolved() {
        assert_eq!(
            evaluate_access(&facts(None, true), "observe"),
            AccessDecision::Refuse(AccessRefusal::FactsInsufficient)
        );
        assert_eq!(
            evaluate_access(&facts(None, true), "observe"),
            evaluate_access(&facts(None, true), delivered::GET_IDENTITY)
        );
    }

    #[test]
    fn observe_is_refused_when_allow_list_fact_is_absent() {
        assert_eq!(
            evaluate_access(&facts(resolved(), false), "observe"),
            AccessDecision::Refuse(AccessRefusal::PrincipalNotAllowed)
        );
        assert_eq!(
            evaluate_access(&facts(resolved(), false), "observe"),
            evaluate_access(&facts(resolved(), false), delivered::GET_IDENTITY)
        );
    }

    #[test]
    fn power_and_other_operations_stay_operation_not_delivered() {
        for operation in [
            "set_power",
            "set_lowpower_hold",
            "request_low_power",
            "release_low_power",
            "reset",
            "wait_manageable",
            "run_cdb",
            "select_application",
        ] {
            assert_eq!(
                evaluate_access(&facts(resolved(), true), operation),
                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
            );
            assert_eq!(
                evaluate_access(&facts(None, false), operation),
                AccessDecision::Refuse(AccessRefusal::OperationNotDelivered)
            );
        }
    }
}
