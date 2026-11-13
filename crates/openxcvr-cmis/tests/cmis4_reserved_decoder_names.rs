//! Narrow value-name regression, not qualification of the whole 4.0 decoder.
//! CMIS 4.0 Table 8-21 reserves 208–209; Table 8-15 reserves 213–220.
use openxcvr_cmis::generated::v4_0::CmisPage00hAdministrative;

const V4_SOURCE: &str = include_str!("../src/generated/v4_0/CmisPage00hAdministrative.rs");
const LATER_NAMES: [&str; 4] = [
    "attenuation_at_53p1ghz",
    "mci_flow_control_duration_encoding",
    "mci_flow_control_duration",
    "reserved_mci_related_advertisement",
];

#[test]
fn generated_40_declarations_and_accessors_do_not_advertise_later_meanings() {
    for name in LATER_NAMES {
        assert!(!V4_SOURCE.contains(name), "later name in 4.0: {name}");
    }
    for name in [
        "reserved_administrative_208_cmis4",
        "reserved_administrative_213_bit7_cmis4",
        "reserved_administrative_213_bits6_0_cmis4",
        "reserved_administrative_214_cmis4",
    ] {
        assert!(V4_SOURCE.contains(&format!("pub {name}:")), "{name}");
    }
}

#[test]
fn generated_40_values_are_raw_reserved_slices_not_attenuation_or_duration() {
    // Nonzero synthetic reserved values deliberately expose interpretation bugs;
    // this neither asserts conformant module contents nor authorizes a read.
    for value in 0..=255u8 {
        let mut bytes = [0u8; 128];
        bytes[208 - 128] = value;
        bytes[209 - 128] = value.wrapping_add(1);
        bytes[213 - 128] = value;
        bytes[214 - 128] = !value;
        bytes[215 - 128..=220 - 128].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        let decoded = CmisPage00hAdministrative::decode(&bytes);
        assert_eq!(decoded.reserved_administrative_208_cmis4, value);
        assert_eq!(
            decoded.reserved_administrative_213_bit7_cmis4,
            value & 0x80 != 0
        );
        assert_eq!(
            decoded.reserved_administrative_213_bits6_0_cmis4,
            value & 0x7f
        );
        assert_eq!(decoded.reserved_administrative_214_cmis4, [!value]);
        // Debug is the generated struct's actual public value surface. Keep
        // this runtime assertion red-capable even before reserved fields exist.
        let output = format!("{decoded:?}");
        for expected in [
            format!("reserved_administrative_208_cmis4: {value},"),
            format!(
                "reserved_administrative_213_bit7_cmis4: {},",
                value & 0x80 != 0
            ),
            format!(
                "reserved_administrative_213_bits6_0_cmis4: {},",
                value & 0x7f
            ),
            format!("reserved_administrative_214_cmis4: [{}],", !value),
        ] {
            assert!(output.contains(&expected), "missing {expected}: {output}");
        }
        for name in LATER_NAMES {
            assert!(!output.contains(name), "later value name: {output}");
        }
        assert_eq!(
            decoded.reserved_copper_cable_attenuation,
            [value.wrapping_add(1)]
        );
        assert_eq!(decoded.reserved_administrative_215_220, [1, 2, 3, 4, 5, 6]);
    }
}

#[test]
fn renamed_raw_buffer_accessors_preserve_the_existing_storage_behavior() {
    // These are legacy raw encoders, NOT programming permission. The vendor
    // programming allowlist is unchanged and does not admit these locations.
    for value in 0..=255u8 {
        let mut bytes = [0xa5u8; 128];
        CmisPage00hAdministrative::set_reserved_administrative_208_cmis4(&mut bytes, value);
        CmisPage00hAdministrative::set_reserved_administrative_213_bit7_cmis4(
            &mut bytes,
            value & 0x80 != 0,
        );
        assert_eq!(bytes[213 - 128] & 0x7f, 0xa5 & 0x7f);
        CmisPage00hAdministrative::set_reserved_administrative_213_bits6_0_cmis4(&mut bytes, value);
        let mut expected = [0xa5u8; 128];
        expected[208 - 128] = value;
        expected[213 - 128] = value;
        assert_eq!(bytes, expected);
    }
}
