//! Detect-once revision context.
//!
//! `Ctx` freezes the module's detected CMIS revision for the lifetime of a
//! `CmisModule` handle. It is built once from the 128-byte Lower memory page
//! at construction time and read by the rev-aware adapter dispatch in
//! `adapter::dispatch`.

use openxcvr_core::Rev;

/// A frozen detect-once revision context.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// The revision detected at construction; never re-read.
    pub detected_rev: Rev,
}

impl Ctx {
    /// Parse the detected revision from Lower memory. CMIS 5.3 § 8.2.1 places
    /// the MajorRevision compliance byte at Lower:1 (packed-BCD, e.g. 0x52 =
    /// CMIS 5.2). Returns [`openxcvr_core::DetectError::UnsupportedRev`] when the byte is
    /// missing or does not map to a known [`Rev`].
    pub fn from_lower_memory(buf: &[u8]) -> Result<Self, openxcvr_core::DetectError> {
        let b = *buf
            .get(1)
            .ok_or(openxcvr_core::DetectError::UnsupportedRev {
                detected_byte: 0,
                known_revs: &[
                    Rev::CMIS_3_0,
                    Rev::CMIS_4_0,
                    Rev::CMIS_5_0,
                    Rev::CMIS_5_1,
                    Rev::CMIS_5_2,
                    Rev::CMIS_5_3,
                    Rev::CMIS_5_4,
                ],
            })?;
        let rev =
            Rev::from_major_byte_cmis(b).ok_or(openxcvr_core::DetectError::UnsupportedRev {
                detected_byte: b,
                known_revs: &[
                    Rev::CMIS_3_0,
                    Rev::CMIS_4_0,
                    Rev::CMIS_5_0,
                    Rev::CMIS_5_1,
                    Rev::CMIS_5_2,
                    Rev::CMIS_5_3,
                    Rev::CMIS_5_4,
                ],
            })?;
        Ok(Ctx { detected_rev: rev })
    }
}
