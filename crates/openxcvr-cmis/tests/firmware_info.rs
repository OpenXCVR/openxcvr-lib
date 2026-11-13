//! CMIS 5.3 §9.7.1 Table 9-19, PDF pp. 309–310; not another revision.
use openxcvr_cmis::{
    adapter::cdb::{CdbCommand, CmisCdbGetFirmwareInfo, CmisCdbGetFirmwareInfoResponse},
    firmware_info::{
        AdministrativeStatus, FirmwareInfo, FirmwareInfoError, OperationalStatus, ValidityStatus,
    },
    managed_cdb::{
        CdbEvidence, CdbInvalidReply, CdbOutcome, CdbRefusal, CdbStopReason, CdbTimeoutStage,
    },
};

fn reply() -> CmisCdbGetFirmwareInfoResponse {
    let mut lpl = [0u8; 110];
    // Table 9-19 p.309: FirmwareStatus 9Fh.136, ImageInformation 9Fh.137.
    lpl[0] = 0x53;
    lpl[1] = 7;
    // Table 9-19 pp.309–310: A major/minor 138/139, build 140–141,
    // extra 142–173; B 174/175, 176–177, 178–209; factory/boot
    // 210/211, 212–213, 214–245. §8.1.3.5 p.138: build is big endian.
    for (start, major, minor, build, extra) in [
        (2, 0x12, 0x34, [0x56, 0x78], b'A'),
        (38, 0x9a, 0xbc, [0xde, 0xf0], b'B'),
        (74, 0x23, 0x45, [0x67, 0x89], b'F'),
    ] {
        lpl[start] = major;
        lpl[start + 1] = minor;
        lpl[start + 2..start + 4].copy_from_slice(&build);
        lpl[start + 4..start + 36].fill(extra);
    }
    CmisCdbGetFirmwareInfo::decode_reply(1, &lpl).unwrap()
}

#[test]
fn distinct_versions_follow_physical_offsets_and_big_endian_builds() {
    let raw = reply();
    let before = raw.clone();
    let info = FirmwareInfo::try_from(&raw).unwrap();
    for (image, major, minor, build, extra) in [
        (info.image_a.unwrap(), 0x12, 0x34, 0x5678, b'A'),
        (info.image_b.unwrap(), 0x9a, 0xbc, 0xdef0, b'B'),
        (info.factory_boot.unwrap(), 0x23, 0x45, 0x6789, b'F'),
    ] {
        assert_eq!(
            (image.major, image.minor, image.build),
            (major, minor, build)
        );
        assert_eq!(image.extra, [extra; 32]);
    }
    assert_eq!(raw, before, "projection must not modify the raw reply");
}

#[test]
fn every_firmware_status_byte_including_reserved_bits() {
    let mut raw = reply();
    for byte in 0..=255u8 {
        raw.firmware_status[0] = byte;
        let status = FirmwareInfo::try_from(&raw).unwrap().status;
        // Table 9-19 p.309: A operational/admin/invalid bits 0/1/2;
        // B operational/admin/invalid bits 4/5/6; bits 3/7 reserved.
        for (image, shift) in [(status.image_a, 0), (status.image_b, 4)] {
            assert_eq!(
                image.operational,
                if byte & (1 << shift) != 0 {
                    OperationalStatus::Running
                } else {
                    OperationalStatus::NotRunning
                }
            );
            assert_eq!(
                image.administrative,
                if byte & (2 << shift) != 0 {
                    AdministrativeStatus::Committed
                } else {
                    AdministrativeStatus::Uncommitted
                }
            );
            assert_eq!(
                image.validity,
                if byte & (4 << shift) != 0 {
                    ValidityStatus::Invalid
                } else {
                    ValidityStatus::Valid
                }
            );
        }
        assert_eq!(status.reserved_bits, byte & 0x88);
    }
}

#[test]
fn every_availability_combination_with_nonzero_absent_image_bytes() {
    let mut raw = reply();
    // Table 9-19 p.309 byte 137: bits 0/1/2 gate A/B/factory information.
    // Sweep all eight combinations and all patterns in undefined bits 3–7.
    for byte in 0..=255u8 {
        raw.image_information[0] = byte;
        let info = FirmwareInfo::try_from(&raw).unwrap();
        assert_eq!(info.image_information.image_a, byte & 1 != 0);
        assert_eq!(info.image_information.image_b, byte & 2 != 0);
        assert_eq!(info.image_information.factory_boot, byte & 4 != 0);
        assert_eq!(info.image_information.reserved_bits, byte & 0xf8);
        assert_eq!(info.image_a.is_some(), byte & 1 != 0);
        assert_eq!(info.image_b.is_some(), byte & 2 != 0);
        assert_eq!(info.factory_boot.is_some(), byte & 4 != 0);
    }
}

#[test]
fn available_zero_versions_are_not_unavailable() {
    let mut raw = reply();
    raw.image_a_firmware.fill(0);
    raw.image_b_firmware.fill(0);
    raw.factory_boot_firmware.fill(0);
    let info = FirmwareInfo::try_from(&raw).unwrap();
    for image in [info.image_a, info.image_b, info.factory_boot] {
        let image = image.unwrap();
        assert_eq!((image.major, image.minor, image.build), (0, 0, 0));
        assert_eq!(image.extra, [0; 32]);
    }
}

#[test]
fn extra_information_is_lossless_not_trimmed_or_nul_terminated() {
    let mut raw = reply();
    let mut embedded_nul = [b'x'; 32];
    embedded_nul[1] = 0;
    let mut non_printable = [0xff; 32];
    non_printable[..4].copy_from_slice(&[0, 0x1f, 0x7f, 0x80]);
    // Table 9-19 p.310 extra fields: 142–173, 178–209, 214–245.
    // Only "Additional information" is defined: no charset/padding contract.
    for extra in [[0; 32], [b' '; 32], embedded_nul, non_printable] {
        raw.image_a_firmware[4..].copy_from_slice(&extra);
        raw.image_b_firmware[4..].copy_from_slice(&extra);
        raw.factory_boot_firmware[4..].copy_from_slice(&extra);
        let info = FirmwareInfo::try_from(&raw).unwrap();
        for image in [info.image_a, info.image_b, info.factory_boot] {
            assert_eq!(image.unwrap().extra, extra);
        }
    }
}

#[test]
fn malformed_field_lengths_are_errors_even_for_unavailable_images() {
    for index in 0..5 {
        for length in 0..=120 {
            let expected = if index < 2 { 1 } else { 36 };
            if length == expected {
                continue;
            }
            let mut raw = reply();
            raw.image_information[0] = 0;
            let (field, name) = match index {
                0 => (&mut raw.firmware_status, "firmware_status"),
                1 => (&mut raw.image_information, "image_information"),
                2 => (&mut raw.image_a_firmware, "image_a_firmware"),
                3 => (&mut raw.image_b_firmware, "image_b_firmware"),
                _ => (&mut raw.factory_boot_firmware, "factory_boot_firmware"),
            };
            field.resize(length, 0xff);
            let error = FirmwareInfoError {
                field: name,
                expected,
                actual: length,
            };
            assert_eq!(FirmwareInfo::try_from(&raw), Err(error));
            let outcome = CdbOutcome::Completed {
                reply: raw,
                evidence: CdbEvidence::default(),
            };
            assert_eq!(outcome.firmware_info(), Err(error));
        }
    }
}

#[test]
fn completed_projection_preserves_raw_images_and_separate_command_evidence() {
    let outcome = CdbOutcome::Completed {
        reply: reply(),
        evidence: CdbEvidence {
            raw_status: Some(1),
            ..CdbEvidence::default()
        },
    };
    let before = outcome.clone();
    let info = outcome.firmware_info().unwrap().unwrap();
    assert_eq!(info.image_a.unwrap().build, 0x5678);
    // FirmwareStatus=0x53 differs from command CdbStatus=0x01.
    assert_eq!(info.status.image_b.validity, ValidityStatus::Invalid);
    assert_eq!(info.status.image_a.operational, OperationalStatus::Running);
    let raw_images = outcome.firmware_images().unwrap();
    assert_eq!(raw_images.image_a.unwrap()[..4], [0x12, 0x34, 0x56, 0x78]);
    assert_eq!(outcome, before);
}

#[test]
fn every_non_completed_outcome_has_no_firmware_info() {
    let evidence = CdbEvidence::default();
    let outcomes = [
        CdbOutcome::Failed {
            raw_status: 0x40,
            result: 0,
            evidence: evidence.clone(),
        },
        CdbOutcome::BusyTimeout {
            stage: CdbTimeoutStage::AfterTrigger,
            evidence: evidence.clone(),
        },
        CdbOutcome::AmbiguousAfterTrigger {
            reason: CdbStopReason::Boundary,
            evidence: evidence.clone(),
        },
        CdbOutcome::RefusedBeforeIo {
            reason: CdbRefusal::Qualification,
        },
        CdbOutcome::StoppedBeforeTrigger {
            reason: CdbStopReason::Cancelled,
            staged_effects: evidence.clone(),
        },
        CdbOutcome::InvalidReply {
            reason: CdbInvalidReply::UnexpectedLength,
            evidence,
        },
    ];
    for outcome in outcomes {
        assert_eq!(outcome.firmware_info(), Ok(None));
        assert_eq!(outcome.firmware_images(), None);
    }
}
