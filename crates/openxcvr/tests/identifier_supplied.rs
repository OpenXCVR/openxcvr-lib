//! SFF-8024 module identifier decode from supplied bytes, for every
//! cataloged family, and its additive presence in the CMIS identity
//! decode. Synthetic snapshots only; no captures, no I/O. The identifier
//! location always comes from the family's catalog row (byte 0 of the
//! A0h / lower page); there is no second offset table here.
use openxcvr::live::{
    cmis_identity_layout, decode_cmis_identity_supplied, decode_identifier_supplied,
    IdentifierError, ModuleIdentifier,
};
use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, InputError, ModuleGeneration, MonoTime, ObservationSource,
    ObservationStamp, Range, RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(2);

fn range(address: u8, offset: u8, len: u16) -> Range {
    Range {
        address,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}

fn snapshot(parts: &[(u8, u8, Vec<u8>)]) -> Snapshot {
    let stamp = ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    };
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: parts
            .iter()
            .map(|&(address, offset, ref bytes)| RangeObservation {
                range: range(address, offset, bytes.len() as u16),
                outcome: RangeOutcome::Bytes(bytes.clone()),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    }
}

fn identifier_snapshot(code: u8) -> Snapshot {
    snapshot(&[(0x50, 0, vec![code])])
}

const SFF8636_2_11: Rev = Rev {
    family: Family::Sff8636,
    major: 2,
    minor: 12,
};
const SFF8472_12_5: Rev = Rev {
    family: Family::Sff8472,
    major: 12,
    minor: 5,
};

// Re-export check: the facade root (which the USB connector re-exports with
// `pub use openxcvr`) exposes the new type as `openxcvr::ModuleIdentifier`.
const _: Option<openxcvr::ModuleIdentifier> = None;

#[test]
fn identifier_decodes_each_family_from_the_generated_enum() {
    // The code and its meaning both come from the family's generated
    // Sff8024Identifier enum; the location from the catalog row.
    let cases: [(Family, Rev, u8, u8, &str); 5] = [
        (Family::Cmis, Rev::CMIS_4_0, 0x1e, 0x1e, "QsfpPlusCmis"),
        (Family::Cmis, Rev::CMIS_5_3, 0x18, 0x18, "QsfpDd"),
        (Family::Cmis, Rev::CMIS_5_3, 0x19, 0x19, "Osfp"),
        (Family::Sff8636, SFF8636_2_11, 0x11, 0x11, "Qsfp28Sff8636"),
        (Family::Sff8472, SFF8472_12_5, 0x03, 0x03, "SfpSff8472"),
    ];
    for (family, revision, code, expected_code, expected_name) in cases {
        let view = identifier_snapshot(code);
        let identifier = decode_identifier_supplied(&view, revision, GENERATION, EPOCH)
            .unwrap_or_else(|e| {
                panic!("{family:?} {revision:?} code {code:#04x}: {e:?}");
            });
        assert_eq!(
            identifier,
            ModuleIdentifier {
                code: expected_code,
                name: expected_name.to_string(),
                known: true,
            },
            "{family:?} {revision:?}"
        );
    }
}

#[test]
fn reserved_codes_are_reported_as_such_never_guessed() {
    // 0x00 is a named code ("Unknown or unspecified"), not a reserved one.
    let view = identifier_snapshot(0x00);
    let identifier = decode_identifier_supplied(&view, Rev::CMIS_5_3, GENERATION, EPOCH).unwrap();
    assert_eq!(identifier.code, 0x00);
    assert_eq!(identifier.name, "UnknownOrUnspecified");
    assert!(identifier.known, "0x00 is the declared 'unspecified' code");

    // Everything the schema leaves unassigned (0x27..=0xFF here) reports
    // its raw code, is marked unknown, and is never mapped to a name.
    for (family, revision) in [
        (Family::Cmis, Rev::CMIS_5_3),
        (Family::Sff8636, SFF8636_2_11),
        (Family::Sff8472, SFF8472_12_5),
    ] {
        for code in [0x27u8, 0x7f, 0xff] {
            let view = identifier_snapshot(code);
            let identifier =
                decode_identifier_supplied(&view, revision, GENERATION, EPOCH).unwrap();
            assert_eq!(identifier.code, code, "{family:?}");
            assert!(
                !identifier.known,
                "{family:?} {code:#04x} must not be guessed"
            );
            assert!(
                identifier.name.contains("Unknown"),
                "{family:?} {code:#04x}: {}",
                identifier.name
            );
        }
    }
}

#[test]
fn identifier_byte_is_read_only_from_the_catalog_row_location() {
    // Missing byte: typed InsufficientInput naming the catalog location.
    let empty = snapshot(&[]);
    assert!(matches!(
        decode_identifier_supplied(&empty, Rev::CMIS_5_3, GENERATION, EPOCH),
        Err(IdentifierError::Input(
            InputError::InsufficientInput { range: missing }
        )) if missing == range(0x50, 0, 1)
    ));
    // A byte at the wrong address is not the identifier.
    let wrong_address = snapshot(&[(0x51, 0, vec![0x18])]);
    assert!(matches!(
        decode_identifier_supplied(&wrong_address, SFF8472_12_5, GENERATION, EPOCH),
        Err(IdentifierError::Input(InputError::InsufficientInput { .. }))
    ));
    // Stale provenance propagates.
    let mut stale = identifier_snapshot(0x18);
    stale.ranges[0].stamp.generation = ModuleGeneration(9);
    assert!(matches!(
        decode_identifier_supplied(&stale, Rev::CMIS_5_3, GENERATION, EPOCH),
        Err(IdentifierError::Input(InputError::StaleObservation))
    ));
}

#[test]
fn identifier_refuses_unsupported_family_and_revision() {
    let cmis_5_5 = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 5,
    };
    assert!(matches!(
        decode_identifier_supplied(&identifier_snapshot(0x18), cmis_5_5, GENERATION, EPOCH),
        Err(IdentifierError::UnsupportedRevision { rev }) if rev == cmis_5_5
    ));
    let bad = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 10,
    };
    assert!(matches!(
        decode_identifier_supplied(&identifier_snapshot(0x11), bad, GENERATION, EPOCH),
        Err(IdentifierError::UnsupportedRevision { rev }) if rev == bad
    ));
    assert!(matches!(
        decode_identifier_supplied(
            &identifier_snapshot(0x01),
            Rev {
                family: Family::Elsfp,
                major: 1,
                minor: 0,
            },
            GENERATION,
            EPOCH
        ),
        Err(IdentifierError::UnsupportedFamily(Family::Elsfp))
    ));
}

// ─── The identity layout and decode carry the identifier additively ────

fn identity_parts(with_identifier: bool) -> Vec<(u8, u8, Vec<u8>)> {
    // CMIS 4.0 Table 8-15 / §§8.3.2–8.3.7 vendor spans, plus byte 0.
    let mut parts = vec![
        (0x50, 129, b"EXAMPLE         ".to_vec()),
        (0x50, 145, vec![0x48, 0xb0, 0x2d]),
        (0x50, 148, b"PART            ".to_vec()),
        (0x50, 164, b"A1".to_vec()),
        (0x50, 166, b"SERIAL          ".to_vec()),
        (0x50, 182, b"261006L1".to_vec()),
    ];
    if with_identifier {
        parts.push((0x50, 0, vec![0x1e]));
    }
    parts
}

#[test]
fn identity_layout_carries_the_identifier_range() {
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let layout = cmis_identity_layout(revision).unwrap();
        assert_eq!(layout.identifier, Some(range(0x50, 0, 1)), "{revision:?}");
        // The existing fields are unchanged.
        assert_eq!(
            (
                layout.vendor_name,
                layout.oui,
                layout.vendor_pn,
                layout.vendor_rev,
                layout.vendor_sn,
                layout.date_code
            ),
            (
                range(0x50, 129, 16),
                range(0x50, 145, 3),
                range(0x50, 148, 16),
                range(0x50, 164, 2),
                range(0x50, 166, 16),
                range(0x50, 182, 8)
            ),
            "{revision:?}"
        );
    }
}

#[test]
fn cmis_identity_decode_includes_the_identifier_when_supplied() {
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let view = snapshot(&identity_parts(true));
        let info = decode_cmis_identity_supplied(&view, revision, GENERATION, EPOCH).unwrap();
        // Existing fields first, unchanged in name and value.
        assert_eq!(info.name, "EXAMPLE");
        assert_eq!(info.part_number, "PART");
        assert_eq!(info.rev, "A1");
        assert_eq!(info.serial_number, "SERIAL");
        assert_eq!(info.date_code, "261006L1");
        assert_eq!(info.oui, [0x48, 0xb0, 0x2d]);
        // The identifier is additive.
        assert_eq!(
            info.identifier,
            Some(ModuleIdentifier {
                code: 0x1e,
                name: "QsfpPlusCmis".to_string(),
                known: true,
            }),
            "{revision:?}"
        );
    }
}

#[test]
fn cmis_identity_decode_without_the_identifier_byte_decodes_as_before() {
    // The daemon acquires the identity layout's vendor ranges field by
    // field; a snapshot without byte 0 decodes exactly as it always has,
    // with the identifier simply absent. This keeps the additive field
    // from becoming a new hard requirement on existing callers.
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let view = snapshot(&identity_parts(false));
        let info = decode_cmis_identity_supplied(&view, revision, GENERATION, EPOCH).unwrap();
        assert_eq!(info.name, "EXAMPLE");
        assert_eq!(info.part_number, "PART");
        assert_eq!(info.oui, [0x48, 0xb0, 0x2d]);
        assert_eq!(info.identifier, None, "{revision:?}");
    }
}

#[test]
fn serialized_identity_keeps_the_existing_field_order() {
    // serde_json::to_string emits struct fields in declaration order, so
    // the exact string pins both the existing order and the additive
    // field's trailing position.
    let view = snapshot(&identity_parts(false));
    let info = decode_cmis_identity_supplied(&view, Rev::CMIS_5_3, GENERATION, EPOCH).unwrap();
    assert_eq!(
        serde_json::to_string(&info).unwrap(),
        "{\"name\":\"EXAMPLE\",\"part_number\":\"PART\",\"rev\":\"A1\",\
\"serial_number\":\"SERIAL\",\"date_code\":\"261006L1\",\"oui\":[72,176,45]}"
    );
    let view = snapshot(&identity_parts(true));
    let info = decode_cmis_identity_supplied(&view, Rev::CMIS_5_3, GENERATION, EPOCH).unwrap();
    assert_eq!(
        serde_json::to_string(&info).unwrap(),
        "{\"name\":\"EXAMPLE\",\"part_number\":\"PART\",\"rev\":\"A1\",\
\"serial_number\":\"SERIAL\",\"date_code\":\"261006L1\",\"oui\":[72,176,45],\
\"identifier\":{\"code\":30,\"name\":\"QsfpPlusCmis\",\"known\":true}}"
    );
}
