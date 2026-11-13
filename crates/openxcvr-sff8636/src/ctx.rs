//! Detect-once revision context for SFF-8636.
//!
//! `Ctx` freezes the module's detected SFF-8636 revision for the lifetime of
//! an [`crate::adapter::Sff8636Module`] handle opened through
//! [`crate::adapter::Sff8636Module::open`]. It is built once from the Lower
//! memory page at construction time and read by the rev-aware adapter
//! dispatch.

use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::DetectError;

/// The frozen detect-once context (see the module docs): the revision proven
/// from Lower:1 at construction time, held for the module handle's lifetime.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// The SFF-8636 compliance revision proven from Lower:1.
    pub detected_rev: Rev,
}

impl Ctx {
    /// Parse the detected revision from the Lower memory page. SFF-8636
    /// Rev 2.12 § 6.2.2 (Table 6-4) places the revision-compliance code at
    /// Lower:1. Returns [`DetectError::UnsupportedRev`] when the byte is
    /// missing or does not map to a known [`Rev`] — the unspecified (0x00)
    /// and SFF-8436 legacy (0x01/0x02) codes fail closed, and the range
    /// codes map to their earliest provable baseline (0x07 → 2.5, 0x08 →
    /// 2.8).
    pub fn from_lower_memory(buf: &[u8]) -> Result<Self, DetectError> {
        let known_revs = Rev::known_for(Family::Sff8636);
        let b = *buf.get(1).ok_or(DetectError::UnsupportedRev {
            detected_byte: 0,
            known_revs,
        })?;
        let rev = Rev::from_compliance_byte_sff8636(b).ok_or(DetectError::UnsupportedRev {
            detected_byte: b,
            known_revs,
        })?;
        Ok(Ctx { detected_rev: rev })
    }

    /// Directory label of the frozen revision's generated module tree
    /// (`"v1_3"` … `"v2_12"`), or `None` for a revision this crate does not
    /// compile a decoder for (never — every known family revision is
    /// emitted; the `Option` keeps the dispatch total and fail-closed).
    pub fn dispatch_label(rev: Rev) -> Option<&'static str> {
        match rev {
            Rev::SFF8636_1_3 => Some("v1_3"),
            Rev::SFF8636_1_4 => Some("v1_4"),
            Rev::SFF8636_1_5 => Some("v1_5"),
            Rev::SFF8636_2_0 => Some("v2_0"),
            Rev::SFF8636_2_5 => Some("v2_5"),
            Rev::SFF8636_2_8 => Some("v2_8"),
            Rev::SFF8636_2_11 => Some("v2_11"),
            Rev::SFF8636_2_12 => Some("v2_12"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_byte_1_maps_through_table_6_4() {
        assert_eq!(
            Ctx::from_lower_memory(&[0x0D, 0x0A, 0])
                .unwrap()
                .detected_rev,
            Rev::SFF8636_2_12
        );
        assert_eq!(
            Ctx::from_lower_memory(&[0x11, 0x07, 0])
                .unwrap()
                .detected_rev,
            Rev::SFF8636_2_5
        );
    }

    #[test]
    fn unspecified_and_legacy_codes_fail_closed() {
        for byte in [0x00u8, 0x01, 0x02] {
            let err = Ctx::from_lower_memory(&[0x0D, byte, 0]).unwrap_err();
            match err {
                DetectError::UnsupportedRev {
                    detected_byte,
                    known_revs,
                } => {
                    assert_eq!(detected_byte, byte);
                    assert_eq!(known_revs, Rev::known_for(Family::Sff8636));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn short_buffer_fails_closed() {
        let err = Ctx::from_lower_memory(&[0x0D]).unwrap_err();
        assert!(matches!(err, DetectError::UnsupportedRev { .. }));
    }

    #[test]
    fn every_known_family_rev_has_a_dispatch_label() {
        for rev in Rev::known_for(Family::Sff8636) {
            assert!(Ctx::dispatch_label(*rev).is_some(), "{rev:?}");
        }
    }
}
