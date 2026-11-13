use openxcvr_core::{
    rev::{Family, Rev},
    types::management::*,
};

fn range(offset: u8, len: u16) -> Range {
    Range {
        address: 0x50,
        bank: 0,
        page: 0,
        offset,
        len,
    }
}
fn observation(range: Range, outcome: RangeOutcome) -> RangeObservation {
    RangeObservation {
        range,
        outcome,
        stamp: ObservationStamp {
            generation: ModuleGeneration(1),
            epoch: CacheEpoch(2),
            started_at: MonoTime(3),
            completed_at: MonoTime(4),
            source: ObservationSource::DaemonObserved,
            freshness: Freshness::Fresh,
            consumed_effects: vec![],
        },
        requester: ReadRequester::ExplicitClient,
    }
}
fn snapshot(ranges: Vec<RangeObservation>) -> Snapshot {
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        ranges,
    }
}

#[test]
fn checked_ranges_and_common_origin_times() {
    assert!(range(0, 256).validate().is_ok());
    assert!(range(255, 1).validate().is_ok());
    assert!(range(255, 2).validate().is_err());
    assert!(range(0, 0).validate().is_err());
    assert!(range(0, u16::MAX).validate().is_err());
    assert!(Range {
        address: 0x80,
        ..range(0, 1)
    }
    .validate()
    .is_err());
    assert_eq!(MonoTime(1).checked_add(MonoDuration(2)), Ok(MonoTime(3)));
    assert_eq!(
        MonoTime(3).checked_duration_since(MonoTime(1)),
        Ok(MonoDuration(2))
    );
    assert_eq!(
        MonoTime(u64::MAX).checked_add(MonoDuration(1)),
        Err(InputError::TimeOverflow)
    );
    assert_eq!(
        MonoTime(1).checked_duration_since(MonoTime(2)),
        Err(InputError::ReversedTime)
    );
    assert!(MonoDuration(u64::MAX).checked_add(MonoDuration(1)).is_err());
    assert!(MonoDuration(0).checked_sub(MonoDuration(1)).is_err());
}

#[test]
fn absent_failed_and_stale_are_not_bytes() {
    for outcome in [
        RangeOutcome::Absent,
        RangeOutcome::Failed {
            reason: "failed".into(),
        },
        RangeOutcome::Uncertain {
            reason: "uncertain".into(),
        },
    ] {
        let view = snapshot(vec![observation(range(1, 1), outcome)]);
        assert!(view.get(range(1, 1)).unwrap().bytes().is_none());
        assert!(matches!(
            supplied_bytes(&view, range(1, 1), ModuleGeneration(1), CacheEpoch(2)),
            Err(InputError::InsufficientInput { .. })
        ));
    }
    let view = snapshot(vec![observation(range(1, 1), RangeOutcome::Bytes(vec![0]))]);
    assert_eq!(
        supplied_bytes(&view, range(1, 1), ModuleGeneration(1), CacheEpoch(2)).unwrap(),
        vec![0]
    );
    assert_eq!(
        supplied_bytes(&view, range(1, 1), ModuleGeneration(2), CacheEpoch(2)),
        Err(InputError::StaleObservation)
    );
    assert_eq!(
        supplied_bytes(&view, range(1, 1), ModuleGeneration(1), CacheEpoch(3)),
        Err(InputError::StaleObservation)
    );
    let mut crossed = view.clone();
    crossed.ranges[0].stamp.generation = ModuleGeneration(2);
    assert_eq!(crossed.validate(), Err(InputError::StaleObservation));
    let mut reversed = view.clone();
    reversed.ranges[0].stamp.started_at = MonoTime(5);
    assert_eq!(reversed.validate(), Err(InputError::ReversedTime));
    let overlap = snapshot(vec![view.ranges[0].clone(), view.ranges[0].clone()]);
    assert_eq!(overlap.validate(), Err(InputError::OverlappingRanges));
    let short = snapshot(vec![observation(range(1, 2), RangeOutcome::Bytes(vec![0]))]);
    assert!(matches!(
        short.validate(),
        Err(InputError::InvalidLength(_))
    ));
}

#[test]
fn consumed_history_preserves_old_provenance_and_mixed_effects() {
    let mut original = observation(range(8, 1), RangeOutcome::Bytes(vec![1]));
    original.stamp.consumed_effects = vec![ReadEffect::ClearOnRead];
    original.stamp.freshness = Freshness::Historical;
    let history = HistoryEntry {
        observation: original.clone(),
        sequence: EventSequence(99),
    };
    assert_eq!(history.observation.stamp, original.stamp);
    assert!(supplied_bytes(
        &snapshot(vec![original]),
        range(8, 1),
        ModuleGeneration(1),
        CacheEpoch(2)
    )
    .is_err());
    let mut class = ByteClassification {
        known_effects: vec![ReadEffect::AcknowledgeStartupInterrupt],
        readable_mask: 7,
        unknown_mask: 0xf8,
        uncovered_mask: 0,
        write_only_mask: 0,
        change_source: ChangeSource::Dynamic,
    };
    assert!(!class.is_qualified_none());
    class.known_effects = vec![ReadEffect::None];
    assert!(!class.is_qualified_none());
    class.readable_mask = 0xff;
    class.unknown_mask = 0;
    assert!(class.is_qualified_none());
    class.known_effects.clear();
    assert!(!class.is_qualified_none());
}

#[test]
fn all_portable_plan_and_facts_types_are_downstream_importable() {
    let _: Selection = Selection {
        address: 0x50,
        bank: 0,
        page: 0,
    };
    let _: OperationId = OperationId(1);
    let _: PowerGeneration = PowerGeneration(1);
    let intent = ReadIntent {
        requester: ReadRequester::Initialization,
        exact_ranges: vec![range(2, 1)],
        allowed_effects: vec![ReadEffect::AcknowledgeStartupInterrupt],
    };
    let step = ReadStep {
        physical_range: range(2, 1),
        requester: intent.requester,
        classifications: vec![],
        declared_effects: intent.allowed_effects,
    };
    let _: ReadPlan = ReadPlan {
        steps: vec![step],
        result_mapping: vec![ResultMapping {
            requested_range: range(2, 1),
            step_index: 0,
            offset_in_step: 0,
        }],
        required_generation: ModuleGeneration(1),
        required_epoch: CacheEpoch(2),
    };
    let _: AccessFacts = AccessFacts {
        generation: ModuleGeneration(1),
        epoch: CacheEpoch(2),
        revision: Some(Rev::CMIS_5_3),
        capabilities: Capabilities::default(),
        selection: None,
        mutation_excluded: false,
    };
    let _ = [
        ActuationOutcome::Confirmed,
        ActuationOutcome::Failed {
            reason: "failed".into(),
        },
        ActuationOutcome::Uncertain {
            reason: "uncertain".into(),
        },
    ];
    let _ = [ReadRequester::PassiveInspection, ReadRequester::Monitoring];
    let _ = [
        ReadEffect::Unknown,
        ReadEffect::WriteOnly,
        ReadEffect::Uncovered,
    ];
    let _ = ChangeSource::Static;
}

#[test]
fn supplied_identity_layout_decodes_without_family_dependencies() {
    // Synthetic layout exercises the portable extraction seam, not schema authority.
    let mut layout = IdentityLayout {
        revision: Rev::CMIS_5_3,
        vendor_name: range(10, 4),
        vendor_pn: range(14, 4),
        vendor_rev: range(18, 2),
        vendor_sn: range(20, 4),
        date_code: range(24, 8),
        oui: range(32, 3),
        identifier: None,
    };
    let data = b"NAMEPARTA1SN  261006  \x01\x02\x03".to_vec();
    let view = snapshot(vec![observation(range(10, 25), RangeOutcome::Bytes(data))]);
    let info = decode_identity(&layout, &view, ModuleGeneration(1), CacheEpoch(2)).unwrap();
    assert_eq!(info.name, "NAME");
    assert_eq!(info.serial_number, "SN");
    assert_eq!(info.oui, [1, 2, 3]);
    for revision in [
        Rev::CMIS_3_0,
        Rev::CMIS_4_0,
        Rev::CMIS_5_0,
        Rev::CMIS_5_1,
        Rev::CMIS_5_2,
        Rev::CMIS_5_3,
        Rev {
            family: Family::Sff8472,
            major: 12,
            minor: 5,
        },
        Rev {
            family: Family::Sff8636,
            major: 2,
            minor: 12,
        },
    ] {
        layout.revision = revision;
        assert!(decode_identity(&layout, &view, ModuleGeneration(1), CacheEpoch(2)).is_ok());
    }
    struct PanickingByteView;
    impl ByteView for PanickingByteView {
        fn get(&self, _range: Range) -> Option<&RangeObservation> {
            panic!("ByteView must not be accessed for unsupported revisions");
        }
    }
    for (major, minor) in [(0, 0), (3, 1), (4, 1), (5, 5), (5, 15), (6, 0), (21, 3)] {
        layout.revision = Rev {
            family: Family::Cmis,
            major,
            minor,
        };
        assert!(matches!(
            decode_identity(
                &layout,
                &PanickingByteView,
                ModuleGeneration(1),
                CacheEpoch(2)
            ),
            Err(InputError::UnsupportedRevision(_))
        ));
    }
    layout.revision = Rev::CMIS_5_3;
    assert!(matches!(
        decode_identity(
            &layout,
            &snapshot(vec![]),
            ModuleGeneration(1),
            CacheEpoch(2)
        ),
        Err(InputError::InsufficientInput { .. })
    ));
}
