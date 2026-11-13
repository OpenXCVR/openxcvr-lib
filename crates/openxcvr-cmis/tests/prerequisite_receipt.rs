//! The library-issued prerequisite-I/O receipt: the committed file is the
//! generator's own byte-for-byte output, no footprint whose window covers
//! a non-passive byte is ever qualified, and a granted plan may only read
//! bytes the library classifies as passive — the three unsupported
//! prerequisite steps appear as explicit denials, never as grants naming
//! unqualified footprints.
//!
//! Nothing here is a hardware read: the qualification is the library's own
//! `classify_range` answer at this revision, and the committed receipt is a
//! draft record, not a transferred sample.
use openxcvr_cmis::prerequisite_receipt::{
    qualification_for_ranges, receipt_json, PlannedRead, ReceiptError, ReceiptRefusal,
};
use openxcvr_core::types::management::Range;
use std::path::Path;

/// The committed receipt this crate's generator owns.
const COMMITTED_RECEIPT: &str =
    "crates/openxcvr-cmis/tests/data/cmis-5.3-prerequisite-io-receipt.json";

/// One module-memory range at the CMIS management address.
fn range(page: u8, offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page,
        offset,
        len,
    }
}

/// The identity footprint the qualified grants read through.
const IDENTITY_FOOTPRINT: &str = "cmis-5.3-lower-page00h-identity";

/// One planned memory read of a grant under test, naming its footprint.
fn planned_read(footprint: &'static str, window: Range) -> PlannedRead {
    PlannedRead {
        footprint,
        range: window,
    }
}

#[test]
fn a_grant_plan_over_non_passive_bytes_is_refused() {
    // The wave-2 defect this unit fixes: a single 0..128 lower read carries
    // the clear-on-read range between the passive bytes, so the generator
    // refuses to emit it as a grant — the daemon's own gate would refuse it
    // as reading outside its footprint's ranges; the library refuses first,
    // at its own classification.
    let refused = openxcvr_cmis::prerequisite_receipt::read_grant(
        "detection",
        4,
        &[planned_read(IDENTITY_FOOTPRINT, range(0, 0, 128))],
    )
    .expect_err("a plan over non-passive bytes is never emitted as a grant");
    assert!(
        matches!(
            &refused,
            ReceiptError::Grant(ReceiptRefusal::NonPassivePlanRange { step, .. })
                if *step == "detection"
        ),
        "the refusal names the non-passive plan range: {refused:?}"
    );
    // The two passive reads the daemon's § 12 window rule accepts are the
    // one two-step detection grant the generator emits.
    let granted = openxcvr_cmis::prerequisite_receipt::read_grant(
        "detection",
        4,
        &[
            planned_read(IDENTITY_FOOTPRINT, range(0, 0, 3)),
            planned_read(IDENTITY_FOOTPRINT, range(0, 126, 2)),
        ],
    )
    .expect("the passive detection windows are granted");
    assert_eq!(granted.steps.len(), 2);

    // A plan naming a footprint the receipt does not carry is refused, as is
    // one naming a footprint the library does not qualify.
    let uncarried = openxcvr_cmis::prerequisite_receipt::read_grant(
        "pre-identification",
        4,
        &[planned_read("no-such-footprint", range(0, 0, 3))],
    )
    .expect_err("an uncarried footprint is never granted");
    assert!(
        matches!(
            &refused,
            ReceiptError::Grant(ReceiptRefusal::NonPassivePlanRange { .. })
        ),
        "the earlier refusal is unchanged: {refused:?}"
    );
    assert!(
        matches!(
            &uncarried,
            ReceiptError::Grant(ReceiptRefusal::UncarriedFootprint { footprint, .. })
                if footprint == &"no-such-footprint"
        ),
        "the refusal names the uncarried footprint: {uncarried:?}"
    );
    let unqualified = openxcvr_cmis::prerequisite_receipt::read_grant(
        "duration-acquisition",
        8,
        &[planned_read("cmis-page-01h-duration", range(1, 166, 2))],
    )
    .expect_err("an unqualified footprint is never granted");
    assert!(
        matches!(
            &unqualified,
            ReceiptError::Grant(ReceiptRefusal::UnqualifiedFootprint { footprint, .. })
                if footprint == &"cmis-page-01h-duration"
        ),
        "the refusal names the unqualified footprint: {unqualified:?}"
    );
    // A window the library's tables do not cover at all is refused too: its
    // passivity is unproven, so no grant may read it.
    let uncovered = openxcvr_cmis::prerequisite_receipt::read_grant(
        "duration-acquisition",
        6,
        &[planned_read(IDENTITY_FOOTPRINT, range(1, 166, 2))],
    )
    .expect_err("a window the tables do not cover is never granted");
    assert!(
        matches!(
            &uncovered,
            ReceiptError::Grant(ReceiptRefusal::UncoveredPlanRange { step, .. })
                if *step == "duration-acquisition"
        ),
        "the refusal names the uncovered plan range: {uncovered:?}"
    );
}

#[test]
fn regenerated_receipt_matches_the_committed_file_byte_for_byte() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(COMMITTED_RECEIPT);
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the committed receipt {path:?} must be readable: {error}"));
    let pins: serde_json::Value =
        serde_json::from_str(&committed).expect("the committed receipt is one JSON object");
    let library_revision = pins["library_revision"]
        .as_str()
        .expect("the committed receipt names its library revision");
    let connector_revision = pins["connector_revision"]
        .as_str()
        .expect("the committed receipt names its connector revision");
    let regenerated = match pins["review_reference"].as_str() {
        // A draft omits the member entirely; a delivered receipt records it.
        Some(reference) => receipt_json(library_revision, connector_revision, Some(reference)),
        None => receipt_json(library_revision, connector_revision, None),
    }
    .expect("the committed pins classify every footprint range");
    assert_eq!(regenerated, committed, "the committed receipt must be the generator's exact output for the pins it names; regenerate it with crates/openxcvr-cmis/src/bin/cmis-prerequisite-receipt.rs and commit the result");
}

#[test]
fn the_committed_receipt_is_an_explicit_draft() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(COMMITTED_RECEIPT);
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the committed receipt {path:?} must be readable: {error}"));
    let draft: serde_json::Value =
        serde_json::from_str(&committed).expect("the committed receipt is one JSON object");
    // A committed receipt is a draft, explicitly undelivered: `delivered` is
    // false, there is no `review_reference` placeholder at all, and
    // `library_revision` is a real revision that is not the committing
    // commit's own OID (a commit cannot name itself — the production receipt
    // is regenerated from the landed commit, after this work lands).
    assert_eq!(
        draft["delivered"], false,
        "a committed receipt is a draft, explicitly undelivered"
    );
    assert!(
        draft.get("review_reference").is_none(),
        "a draft carries no review_reference placeholder; the review reference \
         is supplied when the delivered receipt is generated"
    );
    for pin in ["library_revision", "connector_revision"] {
        let revision = draft[pin]
            .as_str()
            .unwrap_or_else(|| panic!("the draft names its {pin}"));
        assert_eq!(
            revision.len(),
            40,
            "the {pin} is a full commit OID, never a placeholder"
        );
        assert!(
            revision.chars().all(|c| c.is_ascii_hexdigit()),
            "the {pin} is a full commit OID: {revision}"
        );
    }
}

#[test]
fn a_delivered_receipt_requires_a_non_empty_review_reference() {
    // Without a review reference the generator emits a draft.
    let draft: serde_json::Value =
        serde_json::from_str(&receipt_json("library-pin", "connector-pin", None).unwrap())
            .expect("the draft receipt is one JSON object");
    assert_eq!(draft["delivered"], false);
    assert!(draft.get("review_reference").is_none());
    // With one, the receipt is delivered and records it verbatim.
    let delivered: serde_json::Value = serde_json::from_str(
        &receipt_json(
            "library-pin",
            "connector-pin",
            Some("review-instance-prerequisite-io"),
        )
        .unwrap(),
    )
    .expect("the delivered receipt is one JSON object");
    assert_eq!(delivered["delivered"], true);
    assert_eq!(
        delivered["review_reference"],
        "review-instance-prerequisite-io"
    );
    // A blank review reference is refused: the generator never emits a
    // delivered receipt without a real review.
    let refused = receipt_json("library-pin", "connector-pin", Some("   "))
        .expect_err("a blank review reference never delivers a receipt");
    assert!(
        matches!(refused, ReceiptError::BlankReviewReference),
        "the refusal names the blank review reference: {refused:?}"
    );
}

#[test]
fn a_footprint_covering_any_non_passive_byte_is_never_qualified() {
    // 0x80 (upper 128, SFF8024IdentifierCopy) and 0xBE (upper 190, CLEICode)
    // carry no read-effect-none assertion; 0x04 has incomplete field metadata.
    // None of these non-passive ranges may ever reach "qualified".
    for (window, expected) in [
        (range(0, 0x80, 1), "unresolved"),
        (range(0, 0xbe, 1), "unresolved"),
        (range(0, 0x04, 1), "uncovered"),
        // The mapping's other typed answers, pinned so no value is invented.
        (range(0, 0x09, 1), "effectful"),
        (range(0, 0x76, 1), "ambiguous"),
        (range(1, 166, 2), "uncovered"),
    ] {
        assert_eq!(
            qualification_for_ranges(&[window]).unwrap(),
            expected,
            "window {window:?}"
        );
        assert_ne!(
            qualification_for_ranges(&[window]).unwrap(),
            "qualified",
            "window {window:?}"
        );
    }
    // A footprint mixing a passive range with any non-passive byte stays
    // unqualified: no qualified byte can erase another byte's restriction.
    for non_passive in [range(0, 0x80, 1), range(0, 0xbe, 1), range(0, 0x04, 1)] {
        let mixed = [range(0, 0, 3), non_passive];
        assert_ne!(
            qualification_for_ranges(&mixed).unwrap(),
            "qualified",
            "mixed window {mixed:?}"
        );
    }
    // The identity window remains a qualified footprint: lower 0-2,
    // lower 126-127 (the paged readback) and upper page-00h 129-189.
    assert_eq!(
        qualification_for_ranges(&[range(0, 0, 3), range(0, 126, 2), range(0, 129, 61)]).unwrap(),
        "qualified"
    );
}

/// The window one planned read covers, as `(page, offset, length)`.
fn read_window(planned: &serde_json::Value) -> (u8, u16, u16) {
    let read = &planned["plan"]["read"];
    let address = &read["address"];
    (
        address["page"].as_u64().expect("page") as u8,
        address["offset"].as_u64().expect("offset") as u16,
        read["length"].as_u64().expect("length") as u16,
    )
}

#[test]
fn the_receipt_is_the_admissions_prerequisite_member_over_computed_qualifications() {
    let receipt: serde_json::Value =
        serde_json::from_str(&receipt_json("library-pin", "connector-pin", None).unwrap())
            .expect("the receipt is one JSON object");
    // The closed member set of the daemon's `prerequisite` member, exactly —
    // as the committed draft emits it: no `review_reference` placeholder.
    let mut members: Vec<&str> = receipt
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    members.sort_unstable();
    assert_eq!(
        members,
        [
            "capability",
            "connector_revision",
            "delivered",
            "footprints",
            "grants",
            "kind",
            "library_revision",
            "owner",
            "repository",
            "version",
        ]
    );
    assert_eq!(receipt["kind"], "prerequisite-io-contract");
    assert_eq!(receipt["version"], "1");
    assert_eq!(receipt["capability"], "instance-prerequisite-io");
    assert_eq!(receipt["delivered"], false);
    assert_eq!(receipt["library_revision"], "library-pin");
    assert_eq!(receipt["connector_revision"], "connector-pin");

    // Every footprint carries the byte ranges its qualification is computed
    // over, except the per-bit marking identifier, which is not a byte
    // window. A `ranges` member, when present, names at least one range.
    let footprint = |id: &str| -> serde_json::Value {
        receipt["footprints"]
            .as_array()
            .expect("footprints is an array")
            .iter()
            .find(|entry| entry["footprint"] == id)
            .unwrap_or_else(|| panic!("the receipt carries footprint {id}"))
            .clone()
    };
    let ranges_of = |id: &str| -> Vec<(u8, u16, u16)> {
        footprint(id)["ranges"]
            .as_array()
            .unwrap_or_else(|| panic!("footprint {id} carries its ranges"))
            .iter()
            .map(|range| {
                (
                    range["page"].as_u64().expect("page") as u8,
                    range["offset"].as_u64().expect("offset") as u16,
                    range["length"].as_u64().expect("length") as u16,
                )
            })
            .collect()
    };
    let qualification = |id: &str| -> String {
        footprint(id)["qualification"]
            .as_str()
            .expect("qualification is a string")
            .to_string()
    };
    // The identity footprint covers exactly the bytes its plans read: lower
    // 0-2, the 126/127 paged readback and upper page-00h 129-189, all
    // computed passive by `classify_range`.
    assert_eq!(
        ranges_of(IDENTITY_FOOTPRINT),
        [(0, 0, 3), (0, 126, 2), (0, 129, 61)]
    );
    assert_eq!(qualification(IDENTITY_FOOTPRINT), "qualified");
    assert_eq!(ranges_of("cmis-5.3-lower-module-state"), [(0, 3, 1)]);
    assert_eq!(qualification("cmis-5.3-lower-module-state"), "qualified");
    assert_eq!(ranges_of("cmis-page-01h-duration"), [(1, 166, 2)]);
    assert_eq!(qualification("cmis-page-01h-duration"), "uncovered");
    // C2.1 is a marking identifier, not a byte window: no ranges member.
    assert!(footprint("C2.1").get("ranges").is_none());
    assert_eq!(qualification("C2.1"), "unresolved");
    // Each range-carrying footprint's qualification is the library's answer
    // over exactly the ranges it carries.
    assert_eq!(
        qualification_for_ranges(&[range(0, 0, 3), range(0, 126, 2), range(0, 129, 61)]).unwrap(),
        qualification(IDENTITY_FOOTPRINT)
    );

    // Exactly the five prerequisite steps, each named exactly once, either as
    // a grant object or as an explicit denial with no other member.
    let grants = receipt["grants"].as_array().expect("grants is an array");
    let answer_of = |step: &str| -> &serde_json::Value {
        grants
            .iter()
            .find(|answer| answer["step"] == step)
            .unwrap_or_else(|| panic!("the contract names step {step}"))
    };
    assert_eq!(grants.len(), 5, "exactly the five prerequisite steps");
    let mut steps: Vec<&str> = grants
        .iter()
        .map(|answer| answer["step"].as_str().expect("step name"))
        .collect();
    steps.sort_unstable();
    assert_eq!(
        steps,
        [
            "detection",
            "duration-acquisition",
            "pre-identification",
            "readiness-bootstrap",
            "startup-power-hold"
        ]
    );

    // The two granted steps the daemon's identity read uses: read-only plans
    // over the qualified identity footprint, with finite budgets.
    let detection = answer_of("detection");
    assert_eq!(detection["transfers_per_generation"], 4);
    assert_eq!(detection["authority"], "privileged");
    // Detection stays within its existing identity footprint: the identifier
    // read 0..3 and paged readback 126..128. Byte 3 is now passive but remains
    // outside this footprint; the intervening window still includes COR flags.
    let detection_steps = detection["steps"].as_array().expect("detection steps");
    assert!(
        detection_steps.len() == 1 || detection_steps.len() == 2,
        "an unpaged detection grant names one or two steps"
    );
    let mut windows: Vec<(u8, u16, u16)> = Vec::new();
    for planned in detection_steps {
        assert_eq!(planned["footprint"], IDENTITY_FOOTPRINT);
        assert_eq!(planned["effect"], "memory");
        assert_eq!(planned["declared"], "read");
        let window = read_window(planned);
        assert!(
            window == (0, 0, 3) || window == (0, 126, 2),
            "detection reads only the passive windows 0..3 and 126..128, never \
             lower offsets 3..125: {window:?}"
        );
        windows.push(window);
    }
    assert_eq!(
        windows.first().copied(),
        Some((0, 0, 3)),
        "the first detection step is the identifier read"
    );
    if windows.len() == 2 {
        assert_eq!(
            windows[1],
            (0, 126, 2),
            "the second detection step is the paged readback"
        );
    }

    let pre_identification = answer_of("pre-identification");
    assert_eq!(pre_identification["transfers_per_generation"], 4);
    let pre_steps = pre_identification["steps"]
        .as_array()
        .expect("pre-id steps");
    assert_eq!(
        pre_steps.len(),
        1,
        "an unpaged grant names exactly one step"
    );
    assert_eq!(pre_steps[0]["footprint"], IDENTITY_FOOTPRINT);
    assert_eq!(read_window(&pre_steps[0]), (0, 129, 61));

    // The three steps the library does not support appear as explicit
    // denials: exactly the step and the non-empty reason, with no footprint,
    // no plan and no budget — never a grant naming an unqualified footprint.
    for step in [
        "startup-power-hold",
        "readiness-bootstrap",
        "duration-acquisition",
    ] {
        let denial = answer_of(step);
        let mut members: Vec<&str> = denial
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        members.sort_unstable();
        assert_eq!(
            members,
            ["denied", "step"],
            "a denial carries exactly the step and the reason: {step}"
        );
        let reason = denial["denied"].as_str().unwrap_or_default();
        assert!(
            !reason.trim().is_empty(),
            "the denial of {step} carries the library's non-empty reason"
        );
    }
    // The denial reasons name the library's own computed answers.
    assert!(answer_of("startup-power-hold")["denied"]
        .as_str()
        .unwrap()
        .contains("C2.1"));
    assert!(answer_of("readiness-bootstrap")["denied"]
        .as_str()
        .unwrap()
        .contains("cmis-5.3-lower-module-state"));
    assert!(answer_of("duration-acquisition")["denied"]
        .as_str()
        .unwrap()
        .contains("cmis-page-01h-duration"));

    // Every granted step's footprint is one the footprints list carries, and
    // every budget is finite.
    for answer in grants {
        if answer.get("denied").is_some() {
            continue;
        }
        assert!(
            answer["transfers_per_generation"].as_u64().unwrap_or(0) > 0,
            "a granted step carries a finite budget"
        );
        for planned in answer["steps"].as_array().expect("grant steps") {
            let footprint = planned["footprint"].as_str().unwrap();
            assert!(
                receipt["footprints"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["footprint"] == footprint),
                "step footprint {footprint} must be carried by the footprints list"
            );
        }
    }
}

#[test]
fn internal_d4_receipt_uses_actual_revision_and_only_scoped_grants() {
    use openxcvr_cmis::prerequisite_receipt::{
        qualification_for_ranges_for_revision, receipt_json_for_revision,
    };
    use openxcvr_core::Rev;
    for rev in [Rev::CMIS_4_0, Rev::CMIS_5_3] {
        let text = receipt_json_for_revision(rev, "test-library", "test-connector", None).unwrap();
        let receipt: serde_json::Value = serde_json::from_str(&text).unwrap();
        let grants = receipt["grants"].as_array().unwrap();
        assert_eq!(grants.len(), 5);
        for name in [
            "detection",
            "pre-identification",
            "startup-power-hold",
            "readiness-bootstrap",
            "duration-acquisition",
        ] {
            assert_eq!(grants.iter().filter(|g| g["step"] == name).count(), 1);
        }
        let bootstrap = grants
            .iter()
            .find(|g| g["step"] == "readiness-bootstrap")
            .unwrap();
        if rev == Rev::CMIS_4_0 {
            assert!(bootstrap.get("denied").is_none());
            assert_eq!(bootstrap["transfers_per_generation"], 1);
            assert_eq!(bootstrap["authority"], "privileged");
            assert_eq!(bootstrap["steps"].as_array().unwrap().len(), 1);
            let step = &bootstrap["steps"][0];
            assert_eq!(step["footprint"], "cmis-4.0-lower-module-state");
            assert_eq!(step["plan"]["read"]["address"]["offset"], 3);
            assert_eq!(step["plan"]["read"]["length"], 1);
            assert!(grants
                .iter()
                .find(|g| g["step"] == "detection")
                .unwrap()
                .get("denied")
                .is_none());
            assert!(grants
                .iter()
                .find(|g| g["step"] == "pre-identification")
                .unwrap()
                .get("denied")
                .is_none());
            assert_eq!(
                qualification_for_ranges_for_revision(rev, &[range(0, 0, 3)]).unwrap(),
                "qualified"
            );
            let detection = grants.iter().find(|g| g["step"] == "detection").unwrap();
            assert_eq!(detection["transfers_per_generation"], 4);
            let reads = detection["steps"].as_array().unwrap();
            assert_eq!(reads.len(), 2);
            assert_eq!(read_window(&reads[0]), (0, 0, 3));
            assert_eq!(read_window(&reads[1]), (0, 126, 2));
            for read in reads {
                assert_eq!(read["footprint"], "cmis-4.0-lower-page00h-identity");
                assert!(read["plan"].get("write").is_none());
                assert!(read["plan"].get("control").is_none());
            }
        } else {
            assert!(bootstrap.get("denied").is_some());
        }
        for name in ["startup-power-hold", "duration-acquisition"] {
            assert!(grants
                .iter()
                .find(|g| g["step"] == name)
                .unwrap()
                .get("denied")
                .is_some());
        }
        assert_eq!(receipt["delivered"], false);
    }
}

#[test]
fn d4_grant_requires_nonzero_budget_carried_footprint_and_exact_passive_window() {
    use openxcvr_cmis::prerequisite_receipt::read_grant_for_revision;
    use openxcvr_core::Rev;
    let reads = [planned_read("cmis-4.0-lower-module-state", range(0, 3, 1))];
    assert!(matches!(
        read_grant_for_revision(Rev::CMIS_4_0, "readiness-bootstrap", 0, &reads),
        Err(ReceiptError::Grant(ReceiptRefusal::InvalidReadGrant { .. }))
    ));
    assert!(matches!(
        read_grant_for_revision(Rev::CMIS_4_0, "readiness-bootstrap", 1, &[]),
        Err(ReceiptError::Grant(ReceiptRefusal::InvalidReadGrant { .. }))
    ));
    let invalid = [planned_read("cmis-4.0-lower-module-state", range(0, 0, 1))];
    assert!(matches!(
        read_grant_for_revision(Rev::CMIS_4_0, "readiness-bootstrap", 1, &invalid),
        Err(ReceiptError::Grant(ReceiptRefusal::OutsideFootprint { .. }))
    ));
    for len in [2, 6, 9] {
        let too_wide = [planned_read(
            "cmis-4.0-lower-module-state",
            range(0, 3, len),
        )];
        assert!(matches!(
            read_grant_for_revision(Rev::CMIS_4_0, "readiness-bootstrap", 1, &too_wide),
            Err(ReceiptError::Grant(
                ReceiptRefusal::NonPassivePlanRange { .. }
            ))
        ));
    }
    let grant = read_grant_for_revision(Rev::CMIS_4_0, "readiness-bootstrap", 7, &reads).unwrap();
    assert_eq!(grant.transfers_per_generation, 7); // supplied finite budget, not a default
    assert_eq!(grant.steps.len(), 1);
}

#[test]
fn byte3_is_qualified_without_changing_admission_or_detection_footprints() {
    use openxcvr_cmis::read_planning::CmisReadPlanner;
    use openxcvr_core::{
        rev::Rev,
        types::management::{ChangeSource, NonPassiveReason, RangeClassified},
    };
    let planner = CmisReadPlanner::cmis_5_3();
    assert_eq!(
        planner
            .classify_range(Rev::CMIS_5_3, range(0, 3, 1))
            .unwrap(),
        RangeClassified::Passive {
            change_source: ChangeSource::Dynamic
        }
    );
    assert_eq!(
        planner
            .classify_range(Rev::CMIS_5_3, range(0, 4, 1))
            .unwrap(),
        RangeClassified::NonPassive {
            byte: 4,
            reason: NonPassiveReason::MetadataAbsent
        }
    );
    assert_eq!(
        qualification_for_ranges(&[range(0, 3, 1)]).unwrap(),
        "qualified"
    );
    let granted = openxcvr_cmis::prerequisite_receipt::read_grant(
        "readiness-bootstrap",
        8,
        &[planned_read("cmis-5.3-lower-module-state", range(0, 3, 1))],
    )
    .expect("an explicitly requested passive module-state plan qualifies");
    assert_eq!(granted.steps.len(), 1);
    let outside = openxcvr_cmis::prerequisite_receipt::read_grant(
        "detection",
        4,
        &[planned_read(IDENTITY_FOOTPRINT, range(0, 3, 1))],
    )
    .expect_err("passivity does not expand the detection footprint");
    assert!(matches!(
        outside,
        ReceiptError::Grant(ReceiptRefusal::OutsideFootprint { .. })
    ));
}
