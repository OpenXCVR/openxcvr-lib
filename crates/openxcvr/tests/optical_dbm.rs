//! Shared mathematical contract, exercised through the public facade.
use openxcvr::observation::{
    optical_power_dbm, FieldObservation, FieldValue, ObservationState, OpticalDbmValue,
};
use openxcvr_core::field_catalog::Unit;
use openxcvr_core::rev::Rev;

fn state(code: u8) -> ObservationState {
    match code {
        0 => ObservationState::Valid,
        1 => ObservationState::NotSupplied,
        2 => ObservationState::Uncalibrated,
        3 => ObservationState::UnsupportedRevision(Rev::CMIS_4_0),
        4 => ObservationState::ClearOnReadRefused,
        5 => ObservationState::Invalid,
        6 => ObservationState::UnsupportedField,
        7 => ObservationState::WriteOnlyRefused,
        _ => panic!("unknown fixture state"),
    }
}

#[test]
fn shared_rust_c_vectors() {
    let fixture = include_str!("../../../tests/fixtures/optical-dbm.tsv");
    let mut count = 0;
    for line in fixture.lines().filter(|l| !l.starts_with('#')) {
        let row: Vec<_> = line.split_whitespace().collect();
        assert_eq!(row.len(), 7);
        let watts: f64 = row[3].parse().unwrap();
        let value = match row[2] {
            "0" => Some(FieldValue::Scaled {
                value: watts,
                unit: Unit::Watt,
            }),
            "1" => Some(FieldValue::Scaled {
                value: watts,
                unit: Unit::Volt,
            }),
            "2" => Some(FieldValue::Raw(42)),
            "3" => None,
            _ => panic!("unknown fixture value kind"),
        };
        let source = FieldObservation {
            field: "optical_power_rx",
            lane: Some(3),
            state: state(row[1].parse().unwrap()),
            value,
            note: Some("retained"),
        };
        let before = format!("{source:?}");
        let result = optical_power_dbm(&source);
        assert!(std::ptr::eq(result.source, &source));
        assert_eq!(format!("{source:?}"), before);
        assert_eq!(result.state, state(row[5].parse().unwrap()), "{}", row[0]);
        let (kind, number) = match result.value {
            OpticalDbmValue::Value(dbm) => {
                assert!(dbm.is_finite());
                let expected: f64 = row[6].parse().unwrap();
                assert!(
                    (dbm - expected).abs() <= 1e-10,
                    "{}: {dbm} != {expected}",
                    row[0]
                );
                (0, dbm)
            }
            OpticalDbmValue::Unavailable => (1, 0.0),
            OpticalDbmValue::NotConvertible => (2, 0.0),
            OpticalDbmValue::NoPower => (3, 0.0),
        };
        assert_eq!(kind.to_string(), row[4], "{}", row[0]);
        println!(
            "PARITY {} {} {} {:016x}",
            row[0],
            row[5],
            kind,
            number.to_bits()
        );
        count += 1;
    }
    assert_eq!(count, 26);
}

#[test]
fn all_other_units_are_not_convertible() {
    for unit in [
        Unit::Celsius,
        Unit::Volt,
        Unit::Ampere,
        Unit::Decibel,
        Unit::DecibelMilliwatt,
        Unit::Hertz,
        Unit::Meter,
        Unit::Second,
        Unit::Baud,
        Unit::Count,
        Unit::Ratio,
    ] {
        let source = FieldObservation {
            field: "not_watts",
            lane: None,
            state: ObservationState::Valid,
            value: Some(FieldValue::Scaled { value: 0.001, unit }),
            note: None,
        };
        let result = optical_power_dbm(&source);
        assert_eq!(result.state, ObservationState::Valid);
        assert_eq!(result.value, OpticalDbmValue::NotConvertible);
    }
}

#[test]
fn non_valid_state_takes_precedence_over_value_shape() {
    for code in 1..=7 {
        for value in [
            None,
            Some(FieldValue::Raw(1)),
            Some(FieldValue::Scaled {
                value: 0.001,
                unit: Unit::Watt,
            }),
        ] {
            let source = FieldObservation {
                field: "tx_power",
                lane: Some(1),
                state: state(code),
                value,
                note: None,
            };
            let result = optical_power_dbm(&source);
            assert_eq!(result.state, state(code));
            assert_eq!(result.value, OpticalDbmValue::Unavailable);
        }
    }
}
