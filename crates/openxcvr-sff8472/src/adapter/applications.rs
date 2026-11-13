use openxcvr_core::{error::ModuleError, transport::Transport, types::Application};

/// SFF-8472 transceiver-compliance bit → name mapping.
/// Index = position in the `transceiver_compliance` bool array.
///
/// The array is populated from bytes 3-10 of the A0h page. Within each byte,
/// bit 7 (MSB) maps to index 0, bit 6 to index 1, …, bit 0 to index 7.
///
/// Per SFF-8472 v12.5a § 5.4 Table 5-3 (Transceiver Compliance Codes):
/// bytes 3-10 encode a 64-bit compliance bitmap where each bit position has
/// a specific spec meaning. Byte 3 bit 7 = 10GBASE-ER, byte 3 bit 4 =
/// 10GBASE-SR, etc. Indices 16, 20, 46, 47 and 54 are reserved and unnamed.
fn spec_bit_name(index: u8) -> Option<&'static str> {
    Some(match index {
        0 => "10GBASE-ER",
        1 => "10GBASE-LRM",
        2 => "10GBASE-LR",
        3 => "10GBASE-SR",
        4 => "1X-SX",
        5 => "1X-LX",
        6 => "1X-Copper-Active",
        7 => "1X-Copper-Passive",
        8 => "ESCON-MMF-1310nm-LED",
        9 => "ESCON-SMF-1310nm-Laser",
        10 => "OC-192-Short-Reach",
        11 => "SONET-Reach-Specifier-1",
        12 => "SONET-Reach-Specifier-2",
        13 => "OC-48-Long-Reach",
        14 => "OC-48-Intermediate-Reach",
        15 => "OC-48-Short-Reach",
        17 => "OC-12-SMF-Long-Reach",
        18 => "OC-12-SMF-Intermediate-Reach",
        19 => "OC-12-Short-Reach",
        21 => "OC-3-SMF-Long-Reach",
        22 => "OC-3-SMF-Intermediate-Reach",
        23 => "OC-3-Short-Reach",
        24 => "BASE-PX",
        25 => "BASE-BX10",
        26 => "100BASE-FX",
        27 => "100BASE-LX/LX10",
        28 => "1000BASE-T",
        29 => "1000BASE-CX",
        30 => "1000BASE-LX",
        31 => "1000BASE-SX",
        32 => "FC-LinkLength-V",
        33 => "FC-LinkLength-S",
        34 => "FC-LinkLength-I",
        35 => "FC-LinkLength-L",
        36 => "FC-LinkLength-M",
        37 => "FC-Tech-SA",
        38 => "FC-Tech-LC",
        39 => "FC-Tech-EL-Inter",
        40 => "FC-Tech-EL-Intra",
        41 => "FC-Tech-SN",
        42 => "FC-Tech-SL",
        43 => "FC-Tech-LL",
        44 => "SFP+-Active-Cable",
        45 => "SFP+-Passive-Cable",
        48 => "FC-Media-TW",
        49 => "FC-Media-TP",
        50 => "FC-Media-MI",
        51 => "FC-Media-TV",
        52 => "FC-Media-M6",
        53 => "FC-Media-M5/M5E",
        55 => "FC-Media-SM",
        56 => "FC-Speed-1200-MBps",
        57 => "FC-Speed-800-MBps",
        58 => "FC-Speed-1600-MBps",
        59 => "FC-Speed-400-MBps",
        60 => "FC-Speed-3200-MBps",
        61 => "FC-Speed-200-MBps",
        62 => "FC-Speed-Extended",
        63 => "FC-Speed-100-MBps",
        _ => return None,
    })
}

/// Maps the SFF-8472 64-bit compliance bitmap to a list of spec names.
/// Bits are ordered MSB-first within byte 3, then byte 4, etc.
fn spec_bits_from_bitmap(bitmap: &[bool; 64]) -> Vec<String> {
    bitmap
        .iter()
        .enumerate()
        .filter_map(|(i, &set)| {
            if set {
                spec_bit_name(i as u8).map(String::from)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn applications(
    t: &mut dyn Transport,
    ctx: Option<crate::ctx::Ctx>,
) -> Result<Application, ModuleError> {
    let page = super::pages::read_a0h(t, ctx)?;
    Ok(Application {
        extended_spec: page.extended_spec_name,
        spec_bits: spec_bits_from_bitmap(&page.transceiver_compliance),
        rate_gt: {
            let rate = page.signaling_rate_nominal;
            if rate == 0 {
                None
            } else {
                // signaling_rate_nominal is in 100 MBd units (0.1 GT/s per unit)
                Some(rate as f32 * 0.1)
            }
        },
        connector: page.connector_name,
    })
}

#[cfg(test)]
mod tests {
    use super::spec_bits_from_bitmap;
    use crate::generated::union::PageA0hBaseMemory;

    /// Decodes a raw A0h page with a single compliance bit set and returns
    /// the resulting spec-bit labels. `index` is the position in the
    /// `transceiver_compliance` array: byte `3 + index / 8`, MSB-first.
    fn labels_for_single_bit(index: u8) -> Vec<String> {
        let mut buf = [0u8; 256];
        buf[3 + (index / 8) as usize] = 1 << (7 - (index % 8));
        let page = PageA0hBaseMemory::decode(&buf);
        spec_bits_from_bitmap(&page.transceiver_compliance)
    }

    #[test]
    fn every_defined_compliance_bit_has_its_exact_label() {
        let cases = [
            (0, "10GBASE-ER"),
            (1, "10GBASE-LRM"),
            (2, "10GBASE-LR"),
            (3, "10GBASE-SR"),
            (4, "1X-SX"),
            (5, "1X-LX"),
            (6, "1X-Copper-Active"),
            (7, "1X-Copper-Passive"),
            (8, "ESCON-MMF-1310nm-LED"),
            (9, "ESCON-SMF-1310nm-Laser"),
            (10, "OC-192-Short-Reach"),
            (11, "SONET-Reach-Specifier-1"),
            (12, "SONET-Reach-Specifier-2"),
            (13, "OC-48-Long-Reach"),
            (14, "OC-48-Intermediate-Reach"),
            (15, "OC-48-Short-Reach"),
            (17, "OC-12-SMF-Long-Reach"),
            (18, "OC-12-SMF-Intermediate-Reach"),
            (19, "OC-12-Short-Reach"),
            (21, "OC-3-SMF-Long-Reach"),
            (22, "OC-3-SMF-Intermediate-Reach"),
            (23, "OC-3-Short-Reach"),
            (24, "BASE-PX"),
            (25, "BASE-BX10"),
            (26, "100BASE-FX"),
            (27, "100BASE-LX/LX10"),
            (28, "1000BASE-T"),
            (29, "1000BASE-CX"),
            (30, "1000BASE-LX"),
            (31, "1000BASE-SX"),
            (32, "FC-LinkLength-V"),
            (33, "FC-LinkLength-S"),
            (34, "FC-LinkLength-I"),
            (35, "FC-LinkLength-L"),
            (36, "FC-LinkLength-M"),
            (37, "FC-Tech-SA"),
            (38, "FC-Tech-LC"),
            (39, "FC-Tech-EL-Inter"),
            (40, "FC-Tech-EL-Intra"),
            (41, "FC-Tech-SN"),
            (42, "FC-Tech-SL"),
            (43, "FC-Tech-LL"),
            (44, "SFP+-Active-Cable"),
            (45, "SFP+-Passive-Cable"),
            (48, "FC-Media-TW"),
            (49, "FC-Media-TP"),
            (50, "FC-Media-MI"),
            (51, "FC-Media-TV"),
            (52, "FC-Media-M6"),
            (53, "FC-Media-M5/M5E"),
            (55, "FC-Media-SM"),
            (56, "FC-Speed-1200-MBps"),
            (57, "FC-Speed-800-MBps"),
            (58, "FC-Speed-1600-MBps"),
            (59, "FC-Speed-400-MBps"),
            (60, "FC-Speed-3200-MBps"),
            (61, "FC-Speed-200-MBps"),
            (62, "FC-Speed-Extended"),
            (63, "FC-Speed-100-MBps"),
        ];
        for (index, expected) in cases {
            assert_eq!(
                labels_for_single_bit(index),
                [expected],
                "compliance index {index}"
            );
        }
    }

    #[test]
    fn reserved_compliance_bits_have_no_label() {
        // Byte 5 bits 7 and 3, byte 8 bits 1-0 and byte 9 bit 1 are
        // reserved per SFF-8472 v12.5a § 5.4 Table 5-3.
        for index in [16u8, 20, 46, 47, 54] {
            assert!(
                labels_for_single_bit(index).is_empty(),
                "reserved compliance index {index}"
            );
        }
    }
}
