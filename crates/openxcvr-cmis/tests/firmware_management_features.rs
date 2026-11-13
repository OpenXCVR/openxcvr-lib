//! CMIS 5.3 Table 9-9, pp.296–297; byte order §8.1.3.5 p.138.
use openxcvr_cmis::{
    adapter::cdb::{
        CdbCommand, CmisCdbFirmwareManagementFeatures as Command,
        CmisCdbFirmwareManagementFeaturesRequest, CmisCdbFirmwareManagementFeaturesResponse,
    },
    firmware_management_features::{FirmwareManagementFeatures, HitlessRestart, Mechanism},
    CdbOp, Rev,
};

fn project(bytes: &[u8]) -> FirmwareManagementFeatures {
    FirmwareManagementFeatures::from(&Command::decode_reply(1, bytes).unwrap())
}

#[test]
fn raw_codec_and_revision_are_exact() {
    assert_eq!(Command::CMD_ID, 0x0041);
    assert_eq!(Command::MIN_REPLY_LPL_LEN, 18);
    assert!(Command::encode_lpl(&CmisCdbFirmwareManagementFeaturesRequest).is_empty());
    let bytes: Vec<_> = (0..18).map(|n| n * 13).collect();
    assert_eq!(
        Command::decode_reply(1, &bytes).unwrap().bytes,
        bytes.as_slice()
    );
    assert_eq!(
        CdbOp::from_opcode(0x41),
        Some(CdbOp::FirmwareManagementFeatures)
    );
    assert_eq!(
        CdbOp::FirmwareManagementFeatures.available_since(),
        Rev::CMIS_5_3
    );
    assert!(CdbOp::FirmwareManagementFeatures
        .check_rev(Rev::CMIS_5_2)
        .is_err());
}

#[test]
fn all_256_capability_bytes_preserve_reserved_bits_and_multiplier() {
    for bits in 0..=255 {
        let mut bytes = [0; 18];
        bytes[0] = 0xa6; // reserved byte 136, do not zero it
        bytes[1] = bits;
        let p = project(&bytes);
        assert_eq!(p.reserved_136, 0xa6);
        assert_eq!(p.capabilities_raw, bits);
        assert_eq!(p.reserved_137_bits, bits & 0x70);
        assert_eq!(p.image_readback, bits & 0x80 != 0);
        assert_eq!(p.max_duration_coding, bits & 8 != 0);
        assert_eq!(p.duration_multiplier, if bits & 8 == 0 { 1 } else { 10 });
        assert_eq!(p.skipping_erased_blocks, bits & 4 != 0);
        assert_eq!(p.copy_cmd, bits & 2 != 0);
        assert_eq!(p.abort_cmd, bits & 1 != 0);
    }
}

#[test]
fn literal_mechanisms_and_hitless_codes_never_use_bitmask_guessing() {
    for raw in 0..=255 {
        let mut bytes = [0; 18];
        bytes[5] = raw;
        bytes[6] = raw;
        bytes[7] = raw;
        let p = project(&bytes);
        let expected = match raw {
            0 => Some(Mechanism::None),
            1 => Some(Mechanism::Lpl),
            0x10 => Some(Mechanism::Epl),
            0x11 => Some(Mechanism::LplAndEpl),
            _ => None,
        };
        assert_eq!(p.write_mechanism_raw, raw);
        assert_eq!(p.read_mechanism_raw, raw);
        assert_eq!(p.write_mechanism(), expected);
        assert_eq!(p.read_mechanism(), expected);
        assert_eq!(p.hitless_restart_raw, raw);
        assert_eq!(
            p.hitless_restart(),
            match raw {
                0 => Some(HitlessRestart::Reset),
                1 => Some(HitlessRestart::BestEffort),
                _ => None,
            }
        );
    }
}

#[test]
fn duration_counts_are_big_endian_at_all_five_offsets_not_scaled() {
    for coding in [0, 8] {
        let bytes = [
            0xaa, coding, 0xff, 0x7e, 0xff, 0x10, 0x11, 1, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
            0xde, 0xf0, 0x24, 0x68,
        ];
        let p = project(&bytes);
        assert_eq!(p.max_duration_start, 0x1234); // 144–145
        assert_eq!(p.max_duration_abort, 0x5678); // 146–147
        assert_eq!(p.max_duration_write, 0x9abc); // 148–149
        assert_eq!(p.max_duration_complete, 0xdef0); // 150–151
        assert_eq!(p.max_duration_copy, 0x2468); // 152–153
        assert_eq!(p.duration_multiplier, if coding == 0 { 1 } else { 10 });
    }
}

#[test]
fn byte_scalars_and_exact_length_extensions_cover_all_u8_values() {
    for raw in 0..=255 {
        let mut bytes = [0; 18];
        bytes[2] = raw;
        bytes[3] = raw;
        bytes[4] = raw;
        let p = project(&bytes);
        assert_eq!(p.start_cmd_payload_size, raw);
        assert_eq!(p.erased_byte, raw);
        assert_eq!(p.read_write_length_ext, raw);
        assert_eq!(p.max_lpl_length(), 8 * (1 + u16::from(raw.min(15))));
        assert_eq!(p.max_epl_length(), 8 * (1 + u16::from(raw)));
    }
}

#[test]
fn malformed_wire_lengths_are_errors_never_padded_or_truncated() {
    for len in 0..=256 {
        if len != 18 {
            assert!(
                Command::decode_reply(1, &vec![0; len]).is_err(),
                "length {len}"
            );
        }
    }
    // Public raw type has exactly 18 bytes; no malformed projection can be constructed.
    let raw = CmisCdbFirmwareManagementFeaturesResponse { bytes: [0; 18] };
    assert_eq!(
        FirmwareManagementFeatures::from(&raw).duration_multiplier,
        1
    );
}
