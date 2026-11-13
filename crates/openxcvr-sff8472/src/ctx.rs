//! Detect-once revision context for SFF-8472.
//!
//! `Ctx` freezes the module's detected SFF-8472 revision for the lifetime of
//! an [`crate::adapter::Sff8472Module`] handle opened through
//! [`crate::adapter::Sff8472Module::open`]. It is built once from the A0h
//! base memory page at construction time and read by the rev-aware adapter
//! dispatch.

use openxcvr_core::rev::{Family, Rev};
use openxcvr_core::DetectError;

/// The frozen detect-once context (see the module docs): the revision proven
/// from A0h byte 94 at construction time, held for the module handle's
/// lifetime.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// The SFF-8472 compliance revision proven from A0h:94.
    pub detected_rev: Rev,
}

impl Ctx {
    /// Parse the detected revision from the A0h base memory page. SFF-8472
    /// Rev 12.5a § 8.11 (Table 8-8) places the SFF-8472 compliance code at
    /// A0h byte 94. Returns [`DetectError::UnsupportedRev`] when the byte is
    /// missing or does not map to a known [`Rev`] — the unspecified (0x00)
    /// and reserved codes fail closed, never a latest-revision fallback.
    pub fn from_a0h(buf: &[u8]) -> Result<Self, DetectError> {
        let known_revs = Rev::known_for(Family::Sff8472);
        let b = *buf.get(94).ok_or(DetectError::UnsupportedRev {
            detected_byte: 0,
            known_revs,
        })?;
        let rev = Rev::from_compliance_byte_sff8472(b).ok_or(DetectError::UnsupportedRev {
            detected_byte: b,
            known_revs,
        })?;
        Ok(Ctx { detected_rev: rev })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a0h_with(compliance: u8) -> [u8; 256] {
        let mut buf = [0u8; 256];
        buf[0] = 0x03; // SFP/SFP+/SFP28
        buf[94] = compliance;
        buf
    }

    #[test]
    fn a0h_byte_94_maps_through_table_8_8() {
        assert_eq!(
            Ctx::from_a0h(&a0h_with(0x0A)).unwrap().detected_rev,
            Rev::SFF8472_12_5
        );
        assert_eq!(
            Ctx::from_a0h(&a0h_with(0x01)).unwrap().detected_rev,
            Rev::SFF8472_9_3
        );
    }

    #[test]
    fn unspecified_and_reserved_codes_fail_closed() {
        for byte in [0x00u8, 0x0B, 0xFF] {
            let err = Ctx::from_a0h(&a0h_with(byte)).unwrap_err();
            match err {
                DetectError::UnsupportedRev {
                    detected_byte,
                    known_revs,
                } => {
                    assert_eq!(detected_byte, byte);
                    assert_eq!(known_revs, Rev::known_for(Family::Sff8472));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn short_buffer_fails_closed() {
        let buf = [0u8; 94];
        assert!(matches!(
            Ctx::from_a0h(&buf).unwrap_err(),
            DetectError::UnsupportedRev { .. }
        ));
    }
}
