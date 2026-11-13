//! Family-qualified Rev type. Each family's known-revision domain is
//! enumerated by that family's constants plus [`Rev::known_for`], and each
//! family decodes its hardware revision byte through its own parser
//! ([`Rev::from_major_byte_cmis`], [`Rev::from_compliance_byte_sff8636`],
//! [`Rev::from_compliance_byte_sff8472`]).

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
/// A transceiver management family.
pub enum Family {
    /// CMIS (QSFP-DD/OSFP/DSFP/COBO) — the Common Management Interface
    /// Specification.
    Cmis,
    /// SFF-8472 (SFP/SFP+/SFP28) — the two-address diagnostic interface.
    Sff8472,
    /// SFF-8636 (QSFP+/QSFP28) — the paged management interface.
    Sff8636,
    /// C-CMIS (CMIS over copper) — not yet modeled; no known revisions.
    CCmis,
    /// ELSFP — not yet modeled; no known revisions.
    Elsfp,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
/// A family-qualified specification revision (`family`, `major.minor`).
pub struct Rev {
    /// The family the revision belongs to.
    pub family: Family,
    /// Major revision number.
    pub major: u8,
    /// Minor revision number.
    pub minor: u8,
}

impl Rev {
    /// CMIS revision 3.0.
    pub const CMIS_3_0: Rev = Rev {
        family: Family::Cmis,
        major: 3,
        minor: 0,
    };
    /// CMIS revision 4.0.
    pub const CMIS_4_0: Rev = Rev {
        family: Family::Cmis,
        major: 4,
        minor: 0,
    };
    /// CMIS revision 5.0.
    pub const CMIS_5_0: Rev = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 0,
    };
    /// CMIS revision 5.1.
    pub const CMIS_5_1: Rev = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 1,
    };
    /// CMIS revision 5.2.
    pub const CMIS_5_2: Rev = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 2,
    };
    /// CMIS revision 5.3.
    pub const CMIS_5_3: Rev = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 3,
    };
    /// CMIS revision 5.4.
    pub const CMIS_5_4: Rev = Rev {
        family: Family::Cmis,
        major: 5,
        minor: 4,
    };

    // SFF-8636 compliance codes (Table 6-4); admission domain of the
    // family: a compliance byte maps to the earliest baseline it can
    // prove (0x07 covers 2.5–2.7, 0x08 covers 2.8–2.10).
    /// SFF-8636 revision 1.3 (compliance byte `0x03`).
    pub const SFF8636_1_3: Rev = Rev {
        family: Family::Sff8636,
        major: 1,
        minor: 3,
    };
    /// SFF-8636 revision 1.4 (compliance byte `0x04`).
    pub const SFF8636_1_4: Rev = Rev {
        family: Family::Sff8636,
        major: 1,
        minor: 4,
    };
    /// SFF-8636 revision 1.5 (compliance byte `0x05`).
    pub const SFF8636_1_5: Rev = Rev {
        family: Family::Sff8636,
        major: 1,
        minor: 5,
    };
    /// SFF-8636 revision 2.0 (compliance byte `0x06`).
    pub const SFF8636_2_0: Rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 0,
    };
    /// SFF-8636 revision 2.5, the earliest baseline of the `0x07` range
    /// (2.5–2.7).
    pub const SFF8636_2_5: Rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 5,
    };
    /// SFF-8636 revision 2.8, the earliest baseline of the `0x08` range
    /// (2.8–2.10).
    pub const SFF8636_2_8: Rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 8,
    };
    /// SFF-8636 revision 2.11 (compliance byte `0x09`).
    pub const SFF8636_2_11: Rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 11,
    };
    /// SFF-8636 revision 2.12 (compliance byte `0x0A`).
    pub const SFF8636_2_12: Rev = Rev {
        family: Family::Sff8636,
        major: 2,
        minor: 12,
    };

    // SFF-8472 compliance codes (Table 8-8, A0h byte 94).
    /// SFF-8472 revision 9.3 (A0h byte 94 code `0x01`).
    pub const SFF8472_9_3: Rev = Rev {
        family: Family::Sff8472,
        major: 9,
        minor: 3,
    };
    /// SFF-8472 revision 9.5 (A0h byte 94 code `0x02`).
    pub const SFF8472_9_5: Rev = Rev {
        family: Family::Sff8472,
        major: 9,
        minor: 5,
    };
    /// SFF-8472 revision 10.2 (A0h byte 94 code `0x03`).
    pub const SFF8472_10_2: Rev = Rev {
        family: Family::Sff8472,
        major: 10,
        minor: 2,
    };
    /// SFF-8472 revision 10.4 (A0h byte 94 code `0x04`).
    pub const SFF8472_10_4: Rev = Rev {
        family: Family::Sff8472,
        major: 10,
        minor: 4,
    };
    /// SFF-8472 revision 11.0 (A0h byte 94 code `0x05`).
    pub const SFF8472_11_0: Rev = Rev {
        family: Family::Sff8472,
        major: 11,
        minor: 0,
    };
    /// SFF-8472 revision 11.3 (A0h byte 94 code `0x06`).
    pub const SFF8472_11_3: Rev = Rev {
        family: Family::Sff8472,
        major: 11,
        minor: 3,
    };
    /// SFF-8472 revision 11.4 (A0h byte 94 code `0x07`).
    pub const SFF8472_11_4: Rev = Rev {
        family: Family::Sff8472,
        major: 11,
        minor: 4,
    };
    /// SFF-8472 revision 12.3 (A0h byte 94 code `0x08`). Linux
    /// `include/linux/sfp.h` names this code `SFP_SFF8472_COMPLIANCE_REV12_0`;
    /// the SFF-8472 Rev 12.3, 12.4 and 12.5a documents label it Rev 12.3.
    pub const SFF8472_12_3: Rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 3,
    };
    /// SFF-8472 revision 12.4 (A0h byte 94 code `0x09`).
    pub const SFF8472_12_4: Rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 4,
    };
    /// SFF-8472 revision 12.5 (A0h byte 94 code `0x0A`; the schemas track
    /// 12.5a).
    pub const SFF8472_12_5: Rev = Rev {
        family: Family::Sff8472,
        major: 12,
        minor: 5,
    };

    /// The known-revision domain of `family`, in chronological order.
    /// Families without a modeled revision domain return an empty slice —
    /// callers fail closed on them rather than assuming a latest revision.
    pub fn known_for(family: Family) -> &'static [Rev] {
        match family {
            Family::Cmis => &[
                Self::CMIS_3_0,
                Self::CMIS_4_0,
                Self::CMIS_5_0,
                Self::CMIS_5_1,
                Self::CMIS_5_2,
                Self::CMIS_5_3,
                Self::CMIS_5_4,
            ],
            Family::Sff8636 => &[
                Self::SFF8636_1_3,
                Self::SFF8636_1_4,
                Self::SFF8636_1_5,
                Self::SFF8636_2_0,
                Self::SFF8636_2_5,
                Self::SFF8636_2_8,
                Self::SFF8636_2_11,
                Self::SFF8636_2_12,
            ],
            Family::Sff8472 => &[
                Self::SFF8472_9_3,
                Self::SFF8472_9_5,
                Self::SFF8472_10_2,
                Self::SFF8472_10_4,
                Self::SFF8472_11_0,
                Self::SFF8472_11_3,
                Self::SFF8472_11_4,
                Self::SFF8472_12_3,
                Self::SFF8472_12_4,
                Self::SFF8472_12_5,
            ],
            Family::CCmis | Family::Elsfp => &[],
        }
    }

    /// Canonical dotted revision string, e.g. `"2.11"`.
    pub fn as_str(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// Parse a dotted CMIS revision string like `"5.3"`; `None` on any
    /// malformed input.
    pub fn parse_cmis(s: &str) -> Option<Rev> {
        let (maj, min) = s.split_once('.')?;
        Some(Rev {
            family: Family::Cmis,
            major: maj.parse().ok()?,
            minor: min.parse().ok()?,
        })
    }

    /// CMIS lower-memory byte 1 (BCD major/minor in the nibbles):
    /// `0x53` → 5.3, and so on; `None` outside the known domain.
    pub fn from_major_byte_cmis(b: u8) -> Option<Rev> {
        match b {
            0x30 => Some(Self::CMIS_3_0),
            0x40 => Some(Self::CMIS_4_0),
            0x50 => Some(Self::CMIS_5_0),
            0x51 => Some(Self::CMIS_5_1),
            0x52 => Some(Self::CMIS_5_2),
            0x53 => Some(Self::CMIS_5_3),
            0x54 => Some(Self::CMIS_5_4),
            _ => None,
        }
    }

    /// SFF-8636 Lower Memory byte 1 (revision compliance, Table 6-4).
    /// `0x00` (not specified) and `0x01`/`0x02` (SFF-8436 legacy) decode to
    /// `None` — the caller fails closed, never assuming a baseline. The
    /// range codes `0x07` (2.5–2.7) and `0x08` (2.8–2.10) map to their
    /// earliest baseline, the only map the code can prove.
    pub fn from_compliance_byte_sff8636(b: u8) -> Option<Rev> {
        match b {
            0x03 => Some(Self::SFF8636_1_3),
            0x04 => Some(Self::SFF8636_1_4),
            0x05 => Some(Self::SFF8636_1_5),
            0x06 => Some(Self::SFF8636_2_0),
            0x07 => Some(Self::SFF8636_2_5),
            0x08 => Some(Self::SFF8636_2_8),
            0x09 => Some(Self::SFF8636_2_11),
            0x0A => Some(Self::SFF8636_2_12),
            _ => None,
        }
    }

    /// SFF-8472 A0h byte 94 (SFF-8472 compliance code, Table 8-8).
    /// `0x00` (unspecified) decodes to `None` — fail closed.
    pub fn from_compliance_byte_sff8472(b: u8) -> Option<Rev> {
        match b {
            0x01 => Some(Self::SFF8472_9_3),
            0x02 => Some(Self::SFF8472_9_5),
            0x03 => Some(Self::SFF8472_10_2),
            0x04 => Some(Self::SFF8472_10_4),
            0x05 => Some(Self::SFF8472_11_0),
            0x06 => Some(Self::SFF8472_11_3),
            0x07 => Some(Self::SFF8472_11_4),
            0x08 => Some(Self::SFF8472_12_3),
            0x09 => Some(Self::SFF8472_12_4),
            0x0A => Some(Self::SFF8472_12_5),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cmis_roundtrip() {
        assert_eq!(Rev::parse_cmis("3.0"), Some(Rev::CMIS_3_0));
        assert_eq!(Rev::parse_cmis("5.3"), Some(Rev::CMIS_5_3));
        assert_eq!(Rev::parse_cmis("nonsense"), None);
    }

    #[test]
    fn cmis_ordering_is_chronological() {
        assert!(Rev::CMIS_3_0 < Rev::CMIS_4_0);
        assert!(Rev::CMIS_5_1 < Rev::CMIS_5_3);
    }

    #[test]
    fn major_byte_maps_per_cmis_spec() {
        // CMIS 5.3 § 8.2.1 — MajorRevision byte uses BCD-like nibble layout.
        assert_eq!(Rev::from_major_byte_cmis(0x53), Some(Rev::CMIS_5_3));
        assert_eq!(Rev::from_major_byte_cmis(0x30), Some(Rev::CMIS_3_0));
        assert_eq!(Rev::from_major_byte_cmis(0xFF), None);
    }

    #[test]
    fn known_for_lists_every_family_domain_chronologically() {
        let cmis = Rev::known_for(Family::Cmis);
        assert_eq!(cmis.first(), Some(&Rev::CMIS_3_0));
        assert_eq!(cmis.last(), Some(&Rev::CMIS_5_4));
        assert!(cmis.windows(2).all(|w| w[0] < w[1]));

        let sff8636 = Rev::known_for(Family::Sff8636);
        assert_eq!(
            sff8636,
            &[
                Rev::SFF8636_1_3,
                Rev::SFF8636_1_4,
                Rev::SFF8636_1_5,
                Rev::SFF8636_2_0,
                Rev::SFF8636_2_5,
                Rev::SFF8636_2_8,
                Rev::SFF8636_2_11,
                Rev::SFF8636_2_12,
            ]
        );
        assert!(sff8636.windows(2).all(|w| w[0] < w[1]));

        let sff8472 = Rev::known_for(Family::Sff8472);
        assert_eq!(
            sff8472,
            &[
                Rev::SFF8472_9_3,
                Rev::SFF8472_9_5,
                Rev::SFF8472_10_2,
                Rev::SFF8472_10_4,
                Rev::SFF8472_11_0,
                Rev::SFF8472_11_3,
                Rev::SFF8472_11_4,
                Rev::SFF8472_12_3,
                Rev::SFF8472_12_4,
                Rev::SFF8472_12_5,
            ]
        );
        assert!(sff8472.windows(2).all(|w| w[0] < w[1]));

        // Families without a modeled domain report none — callers fail
        // closed on them rather than assuming a latest revision.
        assert!(Rev::known_for(Family::CCmis).is_empty());
        assert!(Rev::known_for(Family::Elsfp).is_empty());
    }

    /// Table 6-4 (SFF-8636 Rev 2.12 § 6.2.2): every defined code maps to
    /// its revision; unspecified and SFF-8436 legacy codes fail closed.
    #[test]
    fn sff8636_compliance_byte_maps_table_6_4() {
        let cases = [
            (0x03u8, Rev::SFF8636_1_3),
            (0x04, Rev::SFF8636_1_4),
            (0x05, Rev::SFF8636_1_5),
            (0x06, Rev::SFF8636_2_0),
            // Range codes map to their earliest provable baseline.
            (0x07, Rev::SFF8636_2_5),
            (0x08, Rev::SFF8636_2_8),
            (0x09, Rev::SFF8636_2_11),
            (0x0A, Rev::SFF8636_2_12),
        ];
        for (byte, rev) in cases {
            assert_eq!(Rev::from_compliance_byte_sff8636(byte), Some(rev));
        }
        // Unspecified and SFF-8436 legacy codes never admit a baseline.
        for byte in [0x00u8, 0x01, 0x02, 0x0B, 0xFF] {
            assert_eq!(Rev::from_compliance_byte_sff8636(byte), None);
        }
    }

    /// Table 8-8 (SFF-8472 Rev 12.5a § 8.11): A0h byte 94 codes. Codes 08h
    /// and 09h name Rev 12.3 and Rev 12.4 (identical in the Rev 12.3, 12.4
    /// and 12.5a documents); none of them labels a code "12.0".
    #[test]
    fn sff8472_compliance_byte_maps_table_8_8() {
        let cases = [
            (0x01u8, Rev::SFF8472_9_3),
            (0x02, Rev::SFF8472_9_5),
            (0x03, Rev::SFF8472_10_2),
            (0x04, Rev::SFF8472_10_4),
            (0x05, Rev::SFF8472_11_0),
            (0x06, Rev::SFF8472_11_3),
            (0x07, Rev::SFF8472_11_4),
            (0x08, Rev::SFF8472_12_3),
            (0x09, Rev::SFF8472_12_4),
            (0x0A, Rev::SFF8472_12_5),
        ];
        for (byte, rev) in cases {
            assert_eq!(Rev::from_compliance_byte_sff8472(byte), Some(rev));
        }
        // Unspecified and reserved codes never admit a baseline.
        for byte in [0x00u8, 0x0B, 0xFF] {
            assert_eq!(Rev::from_compliance_byte_sff8472(byte), None);
        }
    }

    /// Both compliance parsers stay inside their family's known domain.
    #[test]
    fn compliance_parsers_admit_only_known_domain_revs() {
        for byte in 0u8..=255 {
            if let Some(rev) = Rev::from_compliance_byte_sff8636(byte) {
                assert!(Rev::known_for(Family::Sff8636).contains(&rev));
            }
            if let Some(rev) = Rev::from_compliance_byte_sff8472(byte) {
                assert!(Rev::known_for(Family::Sff8472).contains(&rev));
            }
        }
    }
}
