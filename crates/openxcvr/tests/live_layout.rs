use openxcvr::core::{
    rev::{Family, Rev},
    types::management::*,
};
use openxcvr::live::{discover_layout, discover_layout_supplied, LayoutDiscoveryError};
use openxcvr_transports::{
    bus::{Address7, I2cBus, Limits},
    paged::Layout,
    Result,
};
use std::time::{Duration, Instant};

struct Identity {
    prefix: [u8; 3],
    options: u8,
    reads: Vec<(u8, u8, usize)>,
}
impl I2cBus for Identity {
    fn limits(&self) -> Limits {
        Limits {
            write: 60,
            read: 256,
            prefix: 1,
        }
    }
    fn write(&mut self, _: Address7, _: &[u8], _: Instant) -> Result<()> {
        panic!("discovery must not select pages")
    }
    fn write_read(&mut self, a: Address7, prefix: &[u8], out: &mut [u8], _: Instant) -> Result<()> {
        assert_eq!(a.get(), 0x50);
        assert_eq!(prefix.len(), 1);
        self.reads.push((a.get(), prefix[0], out.len()));
        match prefix[0] {
            0 => out.copy_from_slice(&self.prefix[..out.len()]),
            2 => out.copy_from_slice(&self.prefix[2..3]),
            64 => out.copy_from_slice(&[self.options]),
            _ => panic!("unexpected register read"),
        }
        Ok(())
    }
}

#[test]
fn discovers_qualified_cmis_layout_without_alarm_padding() {
    for revision in [0x40, 0x50, 0x51, 0x52, 0x53] {
        for flat in [false, true] {
            let mut bus = Identity {
                prefix: [0x19, revision, if flat { 0x80 } else { 0 }],
                options: 0x20,
                reads: vec![],
            };
            let layout =
                discover_layout(&mut bus, Instant::now() + Duration::from_secs(1)).unwrap();
            assert!(matches!(layout, Layout::Cmis { flat: value } if value == flat));
            assert_eq!(bus.reads, vec![(0x50, 0, 2), (0x50, 2, 1)]);
        }
    }
}

#[test]
fn refuses_unqualified_sff8636_and_unsupported_cmis_before_byte2() {
    for prefix in [
        [0x11, 8, 4],
        [0x18, 0x30, 4],
        [0x19, 0x30, 4],
        [0x1e, 0, 0],
        [0x1e, 0x55, 0],
        [0x1e, 0x5f, 0],
        [0x1e, 0x60, 0],
        [0x1e, 0x30, 0],
        [0xff, 0, 0],
    ] {
        let mut bus = Identity {
            prefix,
            options: 0x20,
            reads: vec![],
        };
        assert!(discover_layout(&mut bus, Instant::now() + Duration::from_secs(1)).is_err());
        assert_eq!(bus.reads, vec![(0x50, 0, 2)]);
        assert!(bus
            .reads
            .iter()
            .all(|(_, offset, len)| usize::from(*offset) + len <= 2));
    }
}

#[test]
fn discovers_sff8472_options_without_padding() {
    for options in [0, 0x20] {
        let mut bus = Identity {
            prefix: [0x03, 0, 0],
            options,
            reads: vec![],
        };
        let layout = discover_layout(&mut bus, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(
            matches!(layout, Layout::Sff8472 { paged_diagnostics } if paged_diagnostics == (options != 0))
        );
        assert_eq!(bus.reads, vec![(0x50, 0, 2), (0x50, 64, 1)]);
    }
}

fn supplied(prefix: &[u8], extras: &[(u8, u8)]) -> Snapshot {
    let stamp = ObservationStamp {
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    };
    let range = |offset, len| Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    };
    let mut ranges = vec![RangeObservation {
        range: range(0, prefix.len() as u16),
        outcome: RangeOutcome::Bytes(prefix.to_vec()),
        stamp: stamp.clone(),
        requester: ReadRequester::PassiveInspection,
    }];
    for (offset, byte) in extras {
        ranges.push(RangeObservation {
            range: range(*offset, 1),
            outcome: RangeOutcome::Bytes(vec![*byte]),
            stamp: stamp.clone(),
            requester: ReadRequester::Initialization,
        });
    }
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        ranges,
    }
}

#[test]
fn pure_companion_uses_only_supplied_current_observations() {
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    let cmis = supplied(&[0x19, 0x53], &[(2, 0x80)]);
    assert!(matches!(
        discover_layout_supplied(&cmis, Rev::CMIS_5_3, generation, epoch),
        Ok(Layout::Cmis { flat: true })
    ));
    let sff8636 = supplied(&[0x11, 0x0A], &[(2, 4)]);
    let sff8636_rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };
    assert!(matches!(
        discover_layout_supplied(&sff8636, sff8636_rev, generation, epoch),
        Ok(Layout::Sff8636 { flat: true })
    ));
    let sff8472 = supplied(&[0x03, 0], &[(94, 0x0A), (64, 0x20)]);
    let sff8472_rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };
    assert!(matches!(
        discover_layout_supplied(&sff8472, sff8472_rev, generation, epoch),
        Ok(Layout::Sff8472 {
            paged_diagnostics: true
        })
    ));
    for view in [
        supplied(&[0x19], &[]),
        supplied(&[0x19, 0x53], &[]),
        supplied(&[0x11, 0x0A], &[]),
        supplied(&[0x03, 0], &[]),
        supplied(&[0x03, 0], &[(64, 0x20)]),
    ] {
        assert!(discover_layout_supplied(&view, Rev::CMIS_5_3, generation, epoch).is_err());
    }
    for byte in [0, 0x55, 0x5f, 0x60] {
        assert!(
            matches!(discover_layout_supplied(&supplied(&[0x1e, byte], &[]), Rev::CMIS_5_3, generation, epoch), Err(LayoutDiscoveryError::UnsupportedRevision(value)) if value == byte)
        );
    }
    assert!(matches!(
        discover_layout_supplied(&cmis, Rev::CMIS_5_3, ModuleGeneration(9), epoch),
        Err(LayoutDiscoveryError::Input(InputError::StaleObservation))
    ));
    assert!(discover_layout_supplied(&cmis, Rev::CMIS_5_3, generation, CacheEpoch(9)).is_err());
    let mut absent = cmis.clone();
    absent.ranges[1].outcome = RangeOutcome::Absent;
    assert!(matches!(
        discover_layout_supplied(&absent, Rev::CMIS_5_3, generation, epoch),
        Err(LayoutDiscoveryError::Input(
            InputError::InsufficientInput { .. }
        ))
    ));
    assert!(discover_layout_supplied(
        &sff8636,
        Rev {
            minor: 13,
            ..sff8636_rev
        },
        generation,
        epoch
    )
    .is_err());
}

#[test]
fn supplied_cmis_3_0_qsfpdd_and_osfp_route_to_cmis_layout() {
    // CMIS 3.0 reports byte1 = 0x30 (the synthetic CMIS 3.0 fixtures carry
    // exactly this prefix). After the family-routing repair, the supplied
    // companion decodes these modules through the CMIS seam when the caller
    // declares Rev::CMIS_3_0; the declared revision must still match the
    // observed revision byte, and byte 2 must still be supplied.
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    for identifier in [0x18, 0x19] {
        let view = supplied(&[identifier, 0x30], &[(2, 0x80)]);
        assert!(matches!(
            discover_layout_supplied(&view, Rev::CMIS_3_0, generation, epoch),
            Ok(Layout::Cmis { flat: true })
        ));
        let flat = supplied(&[identifier, 0x30], &[(2, 0x00)]);
        assert!(matches!(
            discover_layout_supplied(&flat, Rev::CMIS_3_0, generation, epoch),
            Ok(Layout::Cmis { flat: false })
        ));
        // A declared revision that disagrees with the observed byte is refused.
        assert!(matches!(
            discover_layout_supplied(&view, Rev::CMIS_5_3, generation, epoch),
            Err(LayoutDiscoveryError::Input(InputError::UnsupportedRevision(rev)))
                if rev == Rev::CMIS_5_3
        ));
        // Without the capability byte the decode still fails closed.
        assert!(discover_layout_supplied(
            &supplied(&[identifier, 0x30], &[]),
            Rev::CMIS_3_0,
            generation,
            epoch
        )
        .is_err());
    }
}

#[test]
fn supplied_sff_layout_decodes_every_known_revision() {
    // SFF-8636 Table 6-4 compliance codes at Lower:1; every code the
    // detection machinery admits decodes through the union view when the
    // caller declares the detected revision. Range codes 0x07/0x08 decode
    // at their earliest provable baseline.
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    for (byte, rev) in [
        (0x03, Rev::SFF8636_1_3),
        (0x04, Rev::SFF8636_1_4),
        (0x05, Rev::SFF8636_1_5),
        (0x06, Rev::SFF8636_2_0),
        (0x07, Rev::SFF8636_2_5),
        (0x08, Rev::SFF8636_2_8),
        (0x09, Rev::SFF8636_2_11),
        (0x0A, Rev::SFF8636_2_12),
    ] {
        for capability in [0x00, 0x04] {
            let view = supplied(&[0x11, byte], &[(2, capability)]);
            assert!(
                matches!(discover_layout_supplied(&view, rev, generation, epoch),
                    Ok(Layout::Sff8636 { flat }) if flat == (capability == 0x04)),
                "SFF-8636 compliance 0x{byte:02x} at {rev:?} must decode"
            );
        }
    }
    // SFF-8472 Table 8-8 compliance codes at A0h byte 94.
    for (byte, rev) in [
        (0x01, Rev::SFF8472_9_3),
        (0x02, Rev::SFF8472_9_5),
        (0x03, Rev::SFF8472_10_2),
        (0x04, Rev::SFF8472_10_4),
        (0x05, Rev::SFF8472_11_0),
        (0x06, Rev::SFF8472_11_3),
        (0x07, Rev::SFF8472_11_4),
        (0x08, Rev::SFF8472_12_3),
        (0x09, Rev::SFF8472_12_4),
        (0x0A, Rev::SFF8472_12_5),
    ] {
        let view = supplied(&[0x03, 0], &[(94, byte), (64, 0x20)]);
        assert!(
            matches!(
                discover_layout_supplied(&view, rev, generation, epoch),
                Ok(Layout::Sff8472 {
                    paged_diagnostics: true
                })
            ),
            "SFF-8472 compliance 0x{byte:02x} at {rev:?} must decode"
        );
    }
}

#[test]
fn supplied_sff_layout_fails_closed_on_unknown_revisions() {
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    // Unspecified (0x00), legacy SFF-8436 (0x01/0x02) and unassigned codes
    // never fall back to a latest revision.
    for byte in [0x00, 0x01, 0x02, 0x0B, 0x7F, 0xFF] {
        let view = supplied(&[0x11, byte], &[(2, 0x00)]);
        assert!(
            matches!(discover_layout_supplied(&view, Rev::SFF8636_2_12, generation, epoch),
                Err(LayoutDiscoveryError::UnsupportedRevision(value)) if value == byte),
            "SFF-8636 compliance 0x{byte:02x} must fail closed"
        );
    }
    for byte in [0x00, 0x0B, 0x7F, 0xFF] {
        let view = supplied(&[0x03, 0], &[(94, byte), (64, 0x00)]);
        assert!(
            matches!(discover_layout_supplied(&view, Rev::SFF8472_12_5, generation, epoch),
                Err(LayoutDiscoveryError::UnsupportedRevision(value)) if value == byte),
            "SFF-8472 compliance 0x{byte:02x} must fail closed"
        );
    }
    // A caller-declared revision that disagrees with the detected byte is
    // refused, never reconciled.
    let mismatch = supplied(&[0x11, 0x09], &[(2, 0x00)]);
    assert!(matches!(
        discover_layout_supplied(&mismatch, Rev::SFF8636_2_12, generation, epoch),
        Err(LayoutDiscoveryError::Input(InputError::UnsupportedRevision(rev)))
            if rev == Rev::SFF8636_2_12
    ));
    let mismatch = supplied(&[0x03, 0], &[(94, 0x09), (64, 0x00)]);
    assert!(matches!(
        discover_layout_supplied(&mismatch, Rev::SFF8472_12_5, generation, epoch),
        Err(LayoutDiscoveryError::Input(InputError::UnsupportedRevision(rev)))
            if rev == Rev::SFF8472_12_5
    ));
    // Without the compliance byte the revision is unknown, not assumed.
    let missing = supplied(&[0x03, 0], &[(64, 0x20)]);
    assert!(matches!(
        discover_layout_supplied(&missing, Rev::SFF8472_12_5, generation, epoch),
        Err(LayoutDiscoveryError::Input(
            InputError::InsufficientInput { .. }
        ))
    ));
}

#[test]
fn layout_discovery_error_preserves_its_source_chain() {
    use std::error::Error;
    let generation = ModuleGeneration(1);
    let epoch = CacheEpoch(2);
    let stale = supplied(&[0x19, 0x53], &[(2, 0x80)]);
    let error =
        discover_layout_supplied(&stale, Rev::CMIS_5_3, ModuleGeneration(9), epoch).unwrap_err();
    assert!(matches!(error, LayoutDiscoveryError::Input(_)));
    assert!(error.source().is_some(), "Input must chain its source");
    let span = discover_layout_supplied(
        &supplied(&[0x19, 0x53], &[]),
        Rev::CMIS_5_3,
        generation,
        epoch,
    )
    .unwrap_err();
    // The missing capability byte surfaces as an Input error (range
    // admission), and its own source chain stays reachable.
    assert!(matches!(span, LayoutDiscoveryError::Input(_)));
    assert!(span.source().is_some());
    let unknown = discover_layout_supplied(
        &supplied(&[0x1e, 0x60], &[]),
        Rev::CMIS_5_3,
        generation,
        epoch,
    )
    .unwrap_err();
    assert!(unknown.source().is_none());
}

#[test]
fn supplied_cmis_identity_is_schema_derived_and_revision_gated() {
    use openxcvr::live::{cmis_identity_layout, decode_cmis_identity_supplied};
    let layout = cmis_identity_layout(Rev::CMIS_5_3).unwrap();
    assert_eq!(
        (layout.vendor_name.offset, layout.vendor_name.len),
        (129, 16)
    );
    let mut view = supplied(&[0x19, 0x53], &[]);
    let stamp = view.ranges[0].stamp.clone();
    for (range, bytes) in [
        (layout.vendor_name, b"EXAMPLE         ".to_vec()),
        (layout.vendor_pn, b"PART            ".to_vec()),
        (layout.vendor_rev, b"A1".to_vec()),
        (layout.vendor_sn, b"SERIAL          ".to_vec()),
        (layout.date_code, b"261006  ".to_vec()),
        (layout.oui, vec![0, 2, 3]),
    ] {
        view.ranges.push(RangeObservation {
            range,
            outcome: RangeOutcome::Bytes(bytes),
            stamp: stamp.clone(),
            requester: ReadRequester::PassiveInspection,
        });
    }
    for rev in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
    ] {
        let layout = cmis_identity_layout(rev).unwrap();
        assert_eq!(layout.revision, rev);
        let info =
            decode_cmis_identity_supplied(&view, rev, ModuleGeneration(1), CacheEpoch(2)).unwrap();
        assert_eq!(info.name, "EXAMPLE");
        assert_eq!(info.oui, [0, 2, 3]);
    }
    for rev in [
        Rev {
            family: Family::Cmis,
            major: 3,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 4,
            minor: 1,
        },
        Rev {
            family: Family::Cmis,
            major: 5,
            minor: 4,
        },
    ] {
        assert_eq!(
            cmis_identity_layout(rev),
            Err(InputError::UnsupportedRevision(rev))
        );
        assert!(matches!(
            decode_cmis_identity_supplied(&view, rev, ModuleGeneration(1), CacheEpoch(2)),
            Err(InputError::UnsupportedRevision(rejected)) if rejected == rev
        ));
    }
    view.ranges.pop();
    assert!(matches!(
        decode_cmis_identity_supplied(&view, Rev::CMIS_5_3, ModuleGeneration(1), CacheEpoch(2)),
        Err(InputError::InsufficientInput { .. })
    ));
}

#[test]
fn generated_cmis_fields_decode_only_the_supplied_span() {
    use openxcvr::core::supplied::SpanError;
    use openxcvr_cmis::generated::union::CmisPage00hAdministrative as Page;
    let name = *b"EXAMPLE         ";
    assert_eq!(Page::decode_vendor_name_supplied(&name, 129).unwrap(), name);
    assert_eq!(
        Page::decode_vendor_name_supplied(&name[..15], 129),
        Err(SpanError::InsufficientInput {
            offset: 129,
            len: 16
        })
    );
    assert!(matches!(
        Page::decode_vendor_pn_supplied(&name, 129),
        Err(SpanError::InsufficientInput { .. })
    ));
    assert!(matches!(
        Page::decode_vendor_name_supplied(&name, 130),
        Err(SpanError::InsufficientInput { .. })
    ));
    assert_eq!(
        Page::decode_vendor_oui_supplied(&[0, 0, 0], 145).unwrap(),
        [0, 0, 0]
    );
    assert_eq!(
        Page::decode_vendor_oui_supplied(&[], 145),
        Err(SpanError::InsufficientInput {
            offset: 145,
            len: 3
        })
    );
    assert!(matches!(
        Page::decode_vendor_oui_supplied(&[0; 3], usize::MAX),
        Err(SpanError::InvalidSpan { .. })
    ));
}
