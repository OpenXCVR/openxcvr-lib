//! Single-field observation decoding from supplied byte snapshots.
//! Synthetic rows and synthetic bytes only; no I/O, no captures.
use openxcvr_core::field_catalog::{
    AccessKind, CatalogRow, ExternalLinear, ExternalPoly, FieldEntry, FieldKind, FieldLoc,
    InstanceDimension, ScaledLsb, Scaling, Unit, VariantDiscriminator,
};
use openxcvr_core::observation::{
    observe_field, FieldObservation, FieldValue, ObservationState, ObserveOptions,
};
use openxcvr_core::rev::Rev;
use openxcvr_core::types::management::{
    ByteView, CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource,
    ObservationStamp, Range, RangeObservation, RangeOutcome, ReadEffect, ReadRequester, Snapshot,
};

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(2);

fn stamp() -> ObservationStamp {
    ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(3),
        completed_at: MonoTime(4),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: vec![],
    }
}

fn snapshot(parts: &[(u8, u8, u8, u8, Vec<u8>)]) -> Snapshot {
    Snapshot {
        validity_interval: (MonoTime(3), MonoTime(4)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges: parts
            .iter()
            .map(
                |&(address, bank, page, offset, ref bytes)| RangeObservation {
                    range: Range {
                        address,
                        bank,
                        page,
                        offset,
                        len: bytes.len() as u16,
                    },
                    outcome: RangeOutcome::Bytes(bytes.clone()),
                    stamp: stamp(),
                    requester: ReadRequester::PassiveInspection,
                },
            )
            .collect(),
    }
}

fn entry(
    row: &'static CatalogRow,
    offset: u16,
    bit_offset: u8,
    lane: Option<u16>,
) -> FieldEntry<'static> {
    FieldEntry {
        row,
        rev: Rev::CMIS_5_3,
        lane,
        bank: 0,
        offset,
        bit_offset,
        revision_probe: None,
    }
}

const fn scaled(signed: bool, num: u32, den: u32, unit: Unit) -> ScaledLsb {
    ScaledLsb {
        signed,
        num,
        den,
        unit,
        note: None,
    }
}

const fn base_row(name: &'static str) -> CatalogRow {
    CatalogRow {
        name,
        display_name: name,
        page_id: "SyntheticLower",
        page_label: "lower",
        address: 0x50,
        page: 0,
        banked: false,
        offset: 14,
        len: 2,
        bit_offset: 0,
        bit_width: 16,
        kind: FieldKind::I16Be,
        enum_ref: None,
        scaling: Scaling::None,
        access: AccessKind::Ro,
        read_effects: &[ReadEffect::None],
        change_source: None,
        instances: None,
        since: None,
        until: None,
        renamed_from: None,
        aliased_to: &[],
        tags: &[],
        category: None,
        unit: None,
        discriminator: None,
        presence: FieldPresence::Unconditional,
    }
}

static TEMP_ROW: CatalogRow = CatalogRow {
    scaling: Scaling::Scaled(scaled(true, 1, 256, Unit::Celsius)),
    ..base_row("temp_mon_value")
};
static VCC_ROW: CatalogRow = CatalogRow {
    offset: 16,
    kind: FieldKind::U16Be,
    scaling: Scaling::Scaled(scaled(false, 1, 10_000, Unit::Volt)),
    ..base_row("vcc_mon_voltage")
};
static COR_FLAG_ROW: CatalogRow = CatalogRow {
    offset: 8,
    len: 1,
    bit_offset: 7,
    bit_width: 1,
    kind: FieldKind::Bool,
    read_effects: &[ReadEffect::ClearOnRead],
    ..base_row("temp_mon_high_alarm_flag")
};
static CHECKSUM_ROW: CatalogRow = CatalogRow {
    scaling: Scaling::Other { kind: "checksum" },
    ..base_row("cc_base")
};
static ASCII_ROW: CatalogRow = CatalogRow {
    kind: FieldKind::Ascii,
    len: 16,
    bit_width: 128,
    ..base_row("vendor_name")
};
static LINEAR_VARIANT_ROW: CatalogRow = CatalogRow {
    address: 0x51,
    offset: 96,
    scaling: Scaling::Linear {
        internal: scaled(true, 1, 256, Unit::Celsius),
        external: Some(ExternalLinear {
            slope: FieldLoc {
                address: 0x51,
                page: 0,
                offset: 84,
                len: 2,
            },
            offset: FieldLoc {
                address: 0x51,
                page: 0,
                offset: 86,
                len: 2,
            },
        }),
    },
    discriminator: Some(VariantDiscriminator {
        field: "page_a0h_base_memory.externally_calibrated",
        loc: FieldLoc {
            address: 0x50,
            page: 0,
            offset: 92,
            len: 1,
        },
        bit: 4,
    }),
    ..base_row("temperature")
};
static POLY_VARIANT_ROW: CatalogRow = CatalogRow {
    address: 0x51,
    offset: 104,
    kind: FieldKind::U16Be,
    scaling: Scaling::Polynomial {
        internal: scaled(false, 1, 10_000_000, Unit::Watt),
        external: Some(ExternalPoly {
            coefficients: [
                FieldLoc {
                    address: 0x51,
                    page: 0,
                    offset: 72,
                    len: 4,
                },
                FieldLoc {
                    address: 0x51,
                    page: 0,
                    offset: 68,
                    len: 4,
                },
                FieldLoc {
                    address: 0x51,
                    page: 0,
                    offset: 64,
                    len: 4,
                },
                FieldLoc {
                    address: 0x51,
                    page: 0,
                    offset: 60,
                    len: 4,
                },
                FieldLoc {
                    address: 0x51,
                    page: 0,
                    offset: 56,
                    len: 4,
                },
            ],
        }),
    },
    discriminator: Some(VariantDiscriminator {
        field: "page_a0h_base_memory.externally_calibrated",
        loc: FieldLoc {
            address: 0x50,
            page: 0,
            offset: 92,
            len: 1,
        },
        bit: 4,
    }),
    ..base_row("rx_power")
};
static LANE_ROW: CatalogRow = CatalogRow {
    page: 0x11,
    offset: 154,
    kind: FieldKind::U16Be,
    scaling: Scaling::Scaled(scaled(false, 1, 10_000_000, Unit::Watt)),
    instances: Some(InstanceDimension {
        index: "lane",
        first: 1,
        last: 8,
        offsets: &[154, 156, 158, 160, 162, 164, 166, 168],
        bit_offsets: &[0; 8],
    }),
    ..base_row("optical_power_tx")
};
static BIAS_ROW: CatalogRow = CatalogRow {
    page: 0x11,
    offset: 170,
    scaling: Scaling::Scaled(ScaledLsb {
        signed: false,
        num: 1,
        den: 500_000,
        unit: Unit::Ampere,
        note: Some("times TxBiasCurrentScalingFactor (Page 01h:160)"),
    }),
    kind: FieldKind::U16Be,
    ..base_row("laser_bias_tx")
};

fn observe<'a>(
    entry: &FieldEntry<'a>,
    view: &impl ByteView,
    options: &ObserveOptions,
) -> FieldObservation {
    observe_field(entry, view, GENERATION, EPOCH, options)
}

fn scaled_value(observation: &FieldObservation) -> (f64, Unit) {
    match observation.value.as_ref() {
        Some(FieldValue::Scaled { value, unit }) => (*value, *unit),
        other => panic!("expected scaled value, got {other:?}"),
    }
}

#[test]
fn scaled_signed_temperature_decodes_from_supplied_bytes() {
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x10, 0x00])]);
    let observation = observe(
        &entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    assert_eq!(observation.field, "temp_mon_value");
    let (value, unit) = scaled_value(&observation);
    assert!((value - 16.0).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
}

#[test]
fn negative_scaled_temperature_is_not_clamped_to_zero() {
    let view = snapshot(&[(0x50, 0, 0, 14, (-768i16).to_be_bytes().to_vec())]);
    let observation = observe(
        &entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, _) = scaled_value(&observation);
    assert!((value - (-3.0)).abs() < 1e-9, "value {value}");
}

#[test]
fn scaled_unsigned_voltage_decodes() {
    let view = snapshot(&[(0x50, 0, 0, 16, 33000u16.to_be_bytes().to_vec())]);
    let observation = observe(
        &entry(&VCC_ROW, 16, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 3.3).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Volt);
}

#[test]
fn missing_bytes_are_not_supplied_never_zero() {
    let view = snapshot(&[(0x50, 0, 0, 0, vec![0x1b, 0x53])]);
    let observation = observe(
        &entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert!(observation.value.is_none());
    assert_eq!(observation.field, "temp_mon_value");
}

#[test]
fn clear_on_read_field_is_refused_without_authorization() {
    let view = snapshot(&[(0x50, 0, 0, 8, vec![0x80])]);
    let refused = observe(
        &entry(&COR_FLAG_ROW, 8, 7, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(refused.state, ObservationState::ClearOnReadRefused);
    assert!(refused.value.is_none());
    assert_eq!(refused.field, "temp_mon_high_alarm_flag");

    let authorized = observe(
        &entry(&COR_FLAG_ROW, 8, 7, None),
        &view,
        &ObserveOptions {
            authorize_clear_on_read: true,
        },
    );
    assert_eq!(authorized.state, ObservationState::Valid);
    assert_eq!(authorized.value, Some(FieldValue::Raw(1)));
}

#[test]
fn non_numeric_encoding_is_an_unsupported_field() {
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x00, 0xa2])]);
    let observation = observe(
        &entry(&CHECKSUM_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::UnsupportedField);
    assert!(observation.value.is_none());
    assert_eq!(observation.note, Some("checksum"));
}

#[test]
fn variable_length_field_kind_is_an_unsupported_field() {
    let view = snapshot(&[(0x50, 0, 0, 20, vec![0x41; 16])]);
    let observation = observe(
        &entry(&ASCII_ROW, 20, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::UnsupportedField);
    assert!(observation.value.is_none());
}

#[test]
fn internal_calibration_variant_decodes_with_fixed_scaling() {
    // A0h:92 bit 5 set (internally calibrated), bit 4 clear.
    let view = snapshot(&[
        (0x50, 0, 0, 92, vec![0x20]),
        (0x51, 0, 0, 96, 0x0800i16.to_be_bytes().to_vec()),
    ]);
    let observation = observe(
        &entry(&LINEAR_VARIANT_ROW, 96, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 8.0).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
}

#[test]
fn external_linear_calibration_applies_slope_and_offset() {
    // A0h:92 bit 4 set (externally calibrated); slope = 0x0200 (2.0),
    // offset = -256 (in 1/256 degC LSBs); raw = -512.
    let view = snapshot(&[
        (0x50, 0, 0, 92, vec![0x10]),
        (0x51, 0, 0, 84, 0x0200u16.to_be_bytes().to_vec()),
        (0x51, 0, 0, 86, (-256i16).to_be_bytes().to_vec()),
        (0x51, 0, 0, 96, (-512i16).to_be_bytes().to_vec()),
    ]);
    let observation = observe(
        &entry(&LINEAR_VARIANT_ROW, 96, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    // (2.0 * -512 + -256) = -1280 parameter LSBs -> -5.0 degC.
    let (value, unit) = scaled_value(&observation);
    assert!((value - (-5.0)).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
}

#[test]
fn external_calibration_with_missing_constants_is_uncalibrated() {
    // Externally calibrated, but A2h constants are not supplied.
    let view = snapshot(&[
        (0x50, 0, 0, 92, vec![0x10]),
        (0x51, 0, 0, 96, 0x0800i16.to_be_bytes().to_vec()),
    ]);
    let observation = observe(
        &entry(&LINEAR_VARIANT_ROW, 96, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Uncalibrated);
    assert!(observation.value.is_none());
}

#[test]
fn missing_discriminator_byte_is_not_supplied() {
    let view = snapshot(&[(0x51, 0, 0, 96, 0x0800i16.to_be_bytes().to_vec())]);
    let observation = observe(
        &entry(&LINEAR_VARIANT_ROW, 96, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert!(observation.value.is_none());
}

#[test]
fn external_polynomial_calibration_decodes_in_parameter_lsbs() {
    // Coefficients: degree 1 = 1.0 (bytes 68-71), others zero; raw = 3000
    // -> 3000 parameter LSBs of 0.1 uW = 3.0e-4 W.
    let mut degree1 = [0u8; 4];
    degree1.copy_from_slice(&1.0f32.to_be_bytes());
    let view = snapshot(&[
        (0x50, 0, 0, 92, vec![0x10]),
        (0x51, 0, 0, 56, vec![0; 4]),
        (0x51, 0, 0, 60, vec![0; 4]),
        (0x51, 0, 0, 64, vec![0; 4]),
        (0x51, 0, 0, 68, degree1.to_vec()),
        (0x51, 0, 0, 72, vec![0; 4]),
        (0x51, 0, 0, 104, 3000u16.to_be_bytes().to_vec()),
    ]);
    let observation = observe(
        &entry(&POLY_VARIANT_ROW, 104, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 3.0e-4).abs() < 1e-12, "value {value}");
    assert_eq!(unit, Unit::Watt);
}

#[test]
fn lane_entry_decodes_at_its_own_offset() {
    let view = snapshot(&[(0x50, 0, 0x11, 158, 1000u16.to_be_bytes().to_vec())]);
    let e = entry(&LANE_ROW, 158, 0, Some(3));
    let observation = observe(&e, &view, &ObserveOptions::default());
    assert_eq!(observation.state, ObservationState::Valid);
    assert_eq!(observation.lane, Some(3));
    let (value, unit) = scaled_value(&observation);
    assert!((value - 1.0e-4).abs() < 1e-12, "value {value}");
    assert_eq!(unit, Unit::Watt);
}

#[test]
fn unscaled_scalar_field_decodes_as_raw() {
    static ROW: CatalogRow = CatalogRow {
        kind: FieldKind::U4,
        offset: 2,
        bit_offset: 2,
        bit_width: 4,
        len: 1,
        ..base_row("mci_max_speed")
    };
    let row = &ROW;
    let view = snapshot(&[(0x50, 0, 0, 2, vec![0b0001_0100])]);
    let observation = observe(&entry(row, 2, 2, None), &view, &ObserveOptions::default());
    assert_eq!(observation.state, ObservationState::Valid);
    assert_eq!(observation.value, Some(FieldValue::Raw(5)));
}

#[test]
fn scaled_lsb_note_is_preserved_on_the_row() {
    let view = snapshot(&[(0x50, 0, 0x11, 170, 2500u16.to_be_bytes().to_vec())]);
    let observation = observe(
        &entry(&BIAS_ROW, 170, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 5.0e-3).abs() < 1e-12, "value {value}");
    assert_eq!(unit, Unit::Ampere);
    match &BIAS_ROW.scaling {
        Scaling::Scaled(lsb) => {
            assert_eq!(
                lsb.note,
                Some("times TxBiasCurrentScalingFactor (Page 01h:160)")
            );
        }
        other => panic!("expected scaled, got {other:?}"),
    }
}

// ─── Stage 1 admission: presence prerequisites, evidence, revision probe ──
//
// Synthetic rows only; the real generated catalogs are exercised by the
// facade tests. These rows pin the core decoder's admission contract.

use openxcvr_core::field_catalog::{
    ContentSwitch, FieldPresence, PrerequisiteEquals, RevisionProbe, SettlingKind, SwitchBound,
};
use openxcvr_core::observation::{observe_field_with, AcquisitionContext, SelectorAcquisition};

#[allow(clippy::too_many_arguments)]
const fn equals_prereq(
    field: &'static str,
    address: u8,
    page: u8,
    offset: u16,
    banked: bool,
    bit_offset: u8,
    bit_width: u16,
    expected: u64,
    content_switch: Option<ContentSwitch>,
) -> PrerequisiteEquals {
    PrerequisiteEquals {
        field,
        loc: FieldLoc {
            address,
            page,
            offset,
            len: if bit_width > 8 { 2 } else { 1 },
        },
        banked,
        kind: FieldKind::Bool,
        bit_offset,
        bit_width,
        expected,
        content_switch,
    }
}

/// CMIS-style reported-revision probe: lower memory byte 1, BCD
/// major/minor (Rev::from_major_byte_cmis).
static SYNTH_PROBE: RevisionProbe = RevisionProbe {
    field: "synth_revision",
    loc: FieldLoc {
        address: 0x50,
        page: 0,
        offset: 1,
        len: 1,
    },
};

static CAPABILITY_GATED_ROW: CatalogRow = CatalogRow {
    scaling: Scaling::Scaled(scaled(true, 1, 256, Unit::Celsius)),
    presence: FieldPresence::Equals(equals_prereq(
        "capability_bit",
        0x50,
        0,
        8,
        false,
        7,
        1,
        1,
        None,
    )),
    ..base_row("gated_temp")
};

static UNSUPPORTED_PRESENCE_ROW: CatalogRow = CatalogRow {
    presence: FieldPresence::Unsupported {
        expr: "bank_count > 0",
    },
    ..base_row("banked_field")
};

static SWITCH_BOUNDS: &[SwitchBound] = &[
    SwitchBound {
        since: Some(Rev::CMIS_4_0),
        settling: SettlingKind::TnackAdvertised,
    },
    SwitchBound {
        since: Some(Rev::CMIS_5_0),
        settling: SettlingKind::Tddcs,
    },
];

static MULTIPLEXED_ROW: CatalogRow = CatalogRow {
    page: 0x14,
    banked: true,
    offset: 208,
    kind: FieldKind::U16Le,
    bit_width: 16,
    scaling: Scaling::Scaled(scaled(false, 1, 256, Unit::Decibel)),
    presence: FieldPresence::Equals(PrerequisiteEquals {
        field: "diagnostics_selector",
        loc: FieldLoc {
            address: 0x50,
            page: 0x14,
            offset: 128,
            len: 1,
        },
        banked: true,
        kind: FieldKind::EnumRef,
        bit_offset: 0,
        bit_width: 8,
        expected: 6,
        content_switch: Some(ContentSwitch {
            data: FieldLoc {
                address: 0x50,
                page: 0x14,
                offset: 192,
                len: 64,
            },
            bound: SWITCH_BOUNDS,
        }),
    }),
    ..base_row("snr_lane")
};

fn probe_entry(
    row: &'static CatalogRow,
    offset: u16,
    bit_offset: u8,
    lane: Option<u16>,
) -> FieldEntry<'static> {
    FieldEntry {
        row,
        rev: Rev::CMIS_5_1,
        lane,
        bank: 0,
        offset,
        bit_offset,
        revision_probe: Some(&SYNTH_PROBE),
    }
}

fn acquisition(selector_value: u8, data_offset: u8, data_len: u16) -> SelectorAcquisition {
    SelectorAcquisition {
        generation: GENERATION,
        epoch: EPOCH,
        address: 0x50,
        bank: 0,
        page: 0x14,
        selector_offset: 128,
        selector_value,
        data_range: Range {
            address: 0x50,
            bank: 0,
            page: 0x14,
            offset: data_offset,
            len: data_len,
        },
    }
}

#[test]
fn unsupported_presence_yields_unsupported_field_with_the_expression_as_note() {
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x10, 0x00])]);
    let observation = observe(
        &entry(&UNSUPPORTED_PRESENCE_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::UnsupportedField);
    assert_eq!(observation.note, Some("bank_count > 0"));
    assert!(observation.value.is_none());
}

#[test]
fn plain_equals_prerequisite_absent_is_not_supplied() {
    // The row's own bytes are supplied; the prerequisite byte is not.
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x10, 0x00])]);
    let observation = observe(
        &entry(&CAPABILITY_GATED_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert!(observation.value.is_none());
}

#[test]
fn plain_equals_prerequisite_mismatch_is_invalid() {
    let view = snapshot(&[
        (0x50, 0, 0, 8, vec![0x00]),
        (0x50, 0, 0, 14, vec![0x10, 0x00]),
    ]);
    let observation = observe(
        &entry(&CAPABILITY_GATED_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Invalid);
    assert!(observation.value.is_none());
}

#[test]
fn plain_equals_prerequisite_match_decodes() {
    let view = snapshot(&[
        (0x50, 0, 0, 8, vec![0x80]),
        (0x50, 0, 0, 14, vec![0x10, 0x00]),
    ]);
    let observation = observe(
        &entry(&CAPABILITY_GATED_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 16.0).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Celsius);
}

#[test]
fn multiplexed_row_without_evidence_is_not_supplied_with_note() {
    // Selector matches the row's expectation; no acquisition evidence.
    let view = snapshot(&[
        (0x50, 0, 0x14, 128, vec![0x06]),
        (0x50, 0, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    let observation = observe(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert_eq!(
        observation.note,
        Some("selector acquisition evidence not supplied")
    );
    assert!(observation.value.is_none());
}

#[test]
fn multiplexed_row_with_matching_evidence_decodes() {
    let view = snapshot(&[
        (0x50, 0, 0x14, 128, vec![0x06]),
        (0x50, 0, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    let evidence = [acquisition(0x06, 192, 64)];
    let observation = observe_field_with(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::Valid);
    let (value, unit) = scaled_value(&observation);
    assert!((value - 8.97265625).abs() < 1e-9, "value {value}");
    assert_eq!(unit, Unit::Decibel);
}

#[test]
fn multiplexed_row_contradictory_evidence_is_invalid() {
    // Snapshot selector is 06h; the evidence claims 01h.
    let view = snapshot(&[
        (0x50, 0, 0x14, 128, vec![0x06]),
        (0x50, 0, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    let evidence = [acquisition(0x01, 192, 64)];
    let observation = observe_field_with(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::Invalid);
    assert_eq!(
        observation.note,
        Some("selector evidence contradicts snapshot")
    );
    assert!(observation.value.is_none());
}

#[test]
fn multiplexed_row_view_not_selected_is_invalid() {
    // Evidence matches the snapshot (01h) but the row wants 06h.
    let view = snapshot(&[
        (0x50, 0, 0x14, 128, vec![0x01]),
        (0x50, 0, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    let evidence = [acquisition(0x01, 192, 64)];
    let observation = observe_field_with(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::Invalid);
    assert_eq!(observation.note, Some("view not selected by selector"));
    assert!(observation.value.is_none());
}

#[test]
fn multiplexed_row_selector_absent_with_evidence_is_not_supplied() {
    // Evidence exists, but the snapshot never supplied the selector byte.
    let view = snapshot(&[(0x50, 0, 0x14, 208, vec![0xf9, 0x08])]);
    let evidence = [acquisition(0x06, 192, 64)];
    let observation = observe_field_with(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert!(observation.value.is_none());
}

#[test]
fn multiplexed_row_evidence_range_not_covering_is_not_supplied() {
    let view = snapshot(&[
        (0x50, 0, 0x14, 128, vec![0x06]),
        (0x50, 0, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    // Covers bytes 192-207 only; the row's bytes are 208-209.
    let evidence = [acquisition(0x06, 192, 16)];
    let observation = observe_field_with(
        &entry(&MULTIPLEXED_ROW, 208, 0, Some(1)),
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert_eq!(
        observation.note,
        Some("selector acquisition evidence not supplied")
    );
}

#[test]
fn multiplexed_prerequisite_follows_the_entry_bank() {
    // Banked prerequisite page: the selector is read at the entry's bank.
    let view = snapshot(&[
        (0x50, 1, 0x14, 128, vec![0x06]),
        (0x50, 1, 0x14, 208, vec![0xf9, 0x08]),
    ]);
    let mut banked_entry = entry(&MULTIPLEXED_ROW, 208, 0, Some(1));
    banked_entry.bank = 1;
    let evidence = [SelectorAcquisition {
        bank: 1,
        data_range: Range {
            bank: 1,
            ..acquisition(0x06, 192, 64).data_range
        },
        ..acquisition(0x06, 192, 64)
    }];
    let observation = observe_field_with(
        &banked_entry,
        &view,
        GENERATION,
        EPOCH,
        &ObserveOptions::default(),
        &AcquisitionContext {
            acquisitions: &evidence,
        },
    );
    assert_eq!(observation.state, ObservationState::Valid);
}

#[test]
fn revision_probe_absent_byte_is_not_supplied() {
    // Row bytes supplied, probe byte (lower memory byte 1) absent.
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x10, 0x00])]);
    let observation = observe(
        &probe_entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::NotSupplied);
    assert_eq!(
        observation.note,
        Some("CMIS revision byte not supplied; revision undetermined")
    );
    assert!(observation.value.is_none());
}

#[test]
fn revision_probe_mismatch_is_unsupported_revision_reporting_the_byte() {
    let view = snapshot(&[
        (0x50, 0, 0, 1, vec![0x30]),
        (0x50, 0, 0, 14, vec![0x10, 0x00]),
    ]);
    let observation = observe(
        &probe_entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(
        observation.state,
        ObservationState::UnsupportedRevision(Rev::CMIS_3_0)
    );
    assert!(observation.value.is_none());
}

#[test]
fn revision_probe_unknown_byte_is_unsupported_revision() {
    let view = snapshot(&[
        (0x50, 0, 0, 1, vec![0xff]),
        (0x50, 0, 0, 14, vec![0x10, 0x00]),
    ]);
    let observation = observe(
        &probe_entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(
        observation.state,
        ObservationState::UnsupportedRevision(Rev {
            family: rev_family(),
            major: 15,
            minor: 15,
        })
    );
}

#[test]
fn revision_probe_match_decodes_normally() {
    let view = snapshot(&[
        (0x50, 0, 0, 1, vec![0x51]),
        (0x50, 0, 0, 14, vec![0x10, 0x00]),
    ]);
    let observation = observe(
        &probe_entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
}

#[test]
fn entries_without_a_probe_skip_revision_admission() {
    // Synthetic seam: a hand-built entry with no probe (no schema family
    // behind it) is not revision-gated; generated catalogs always carry
    // the family probe.
    let view = snapshot(&[(0x50, 0, 0, 14, vec![0x10, 0x00])]);
    let observation = observe(
        &entry(&TEMP_ROW, 14, 0, None),
        &view,
        &ObserveOptions::default(),
    );
    assert_eq!(observation.state, ObservationState::Valid);
}

#[test]
fn switch_bounds_are_revision_qualified() {
    assert_eq!(
        ContentSwitch {
            data: FieldLoc {
                address: 0x50,
                page: 0x14,
                offset: 192,
                len: 64,
            },
            bound: SWITCH_BOUNDS,
        }
        .settling_at(Rev::CMIS_4_0),
        Some(SettlingKind::TnackAdvertised)
    );
    assert_eq!(
        ContentSwitch {
            data: FieldLoc {
                address: 0x50,
                page: 0x14,
                offset: 192,
                len: 64,
            },
            bound: SWITCH_BOUNDS,
        }
        .settling_at(Rev::CMIS_5_1),
        Some(SettlingKind::Tddcs)
    );
}

fn rev_family() -> openxcvr_core::rev::Family {
    openxcvr_core::rev::Family::Cmis
}
