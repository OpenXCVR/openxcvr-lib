//! Property suite for the CMIS CDB reply parser (brainstorm item: arbitrary
//! reply buffers never panic; checksum/length validation behaves per spec).
//!
//! The reply path is exercised end to end through [`Cdb::send`] against a
//! mock transport, so the pure framing checks (reply length, reply checksum,
//! failure status) run exactly as they do against a real module, and the
//! per-command `decode_reply` is additionally proven total on arbitrary
//! `(status, lpl)` pairs.
//!
//! Spec anchors (CMIS 5.3):
//! * Table 8-179: `RPLLength` counts reply body bytes from Page 9Fh byte 136;
//!   `RPLChkCode` is the one's complement of the sum of the reply body bytes
//!   and is 0 only when there is no reply body.
//! * A reply longer than the 120-byte LPL window is refused (`LplTooLong`);
//!   a reply shorter than the command's fixed layout is refused
//!   (`TruncatedReply`) rather than zero-padded into a response.

use openxcvr_cmis::adapter::cdb::{
    CdbCommand, CdbError, CmisCdbFirmwareManagementFeatures, CmisCdbGetFirmwareInfo,
    CmisCdbLoopbacks, CmisCdbModuleFeatures, CmisCdbModuleFeaturesRequest, CmisCdbRmonStatistics,
};
use openxcvr_cmis::module::CmisTransport;
use openxcvr_cmis::Cdb;
use openxcvr_core::error::ModuleError;
use proptest::prelude::*;

/// Reply framing constants mirrored from the adapter (private there); they
/// are the Page 9Fh/00h offsets `Cdb::send` reads.
const STATUS: (u8, u8) = (0x00, 37);
const REPLY_HEADER: (u8, u8) = (0x9F, 134);
const LPL: (u8, u8) = (0x9F, 136);

/// Mock module: every read is answered from a scripted status byte, reply
/// header (length, checksum) and reply LPL. Reads anywhere else are served
/// deterministically from `fill` so the total-input property drives every
/// framing branch even where the script does not line up.
struct ScriptedModule {
    status: u8,
    header: [u8; 2],
    lpl: Vec<u8>,
    fill: u8,
}

impl openxcvr_core::transport::Transport for ScriptedModule {
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }

    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let o = offset;
        let src: &[u8] = match (page, o) {
            (p, 37) if p == STATUS.0 => {
                buf[0] = self.status;
                return Ok(());
            }
            (p, 134) if p == REPLY_HEADER.0 => {
                buf.copy_from_slice(&self.header[..buf.len().min(2)]);
                return Ok(());
            }
            (p, o) if p == LPL.0 && o >= LPL.1 => &self.lpl[(o - LPL.1) as usize..],
            _ => {
                for (i, slot) in buf.iter_mut().enumerate() {
                    *slot = self.fill.wrapping_add(o).rotate_left(i as u32 % 8);
                }
                return Ok(());
            }
        };
        let n = buf.len().min(src.len());
        buf[..n].copy_from_slice(&src[..n]);
        for slot in buf[n..].iter_mut() {
            *slot = self.fill;
        }
        Ok(())
    }

    fn write(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        _data: &[u8],
    ) -> Result<(), ModuleError> {
        Ok(())
    }
}

impl CmisTransport for ScriptedModule {}

/// One's complement of the sum of the bytes, as the module computes it.
fn chkcode(lpl: &[u8]) -> u8 {
    !lpl.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

fn send(
    script: &mut ScriptedModule,
) -> Result<<CmisCdbModuleFeatures as CdbCommand>::Response, CdbError> {
    Cdb::send::<CmisCdbModuleFeatures, _>(script, &CmisCdbModuleFeaturesRequest, 3)
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(64))]

    /// Arbitrary status/header/LPL bytes never panic: every outcome is a
    /// decoded response or a typed `CdbError`.
    #[test]
    fn arbitrary_reply_buffers_never_panic(
        status in any::<u8>(),
        header in any::<[u8; 2]>(),
        lpl in prop::collection::vec(any::<u8>(), 0..=130),
        fill in any::<u8>(),
    ) {
        let mut m = ScriptedModule { status, header, lpl, fill };
        match send(&mut m) {
            Ok(_) => {}
            Err(e) => drop(e), // typed error; a panic fails the property
        }
    }

    /// Busy-forever status yields `Timeout`; failure statuses yield
    /// `Failed { status }`; completion statuses proceed to the reply.
    #[test]
    fn status_semantics_per_spec(
        lpl in prop::collection::vec(any::<u8>(), 36..=120),
    ) {
        let header = [lpl.len() as u8, chkcode(&lpl)];
        // Busy never clears -> Timeout (0x80 set on every poll).
        let mut m = ScriptedModule { status: 0x80, header, lpl: lpl.clone(), fill: 0 };
        prop_assert!(matches!(send(&mut m), Err(CdbError::Timeout)));
        // 0x40..=0x7F is the failure range once not busy.
        for status in [0x40u8, 0x41, 0x55, 0x7F] {
            let mut m = ScriptedModule { status, header, lpl: lpl.clone(), fill: 0 };
            prop_assert!(matches!(send(&mut m), Err(CdbError::Failed { .. })), "status {status:02x}");
        }
        // A completion status below 0x40 decodes the reply.
        for status in [0x00u8, 0x01, 0x3F] {
            let mut m = ScriptedModule { status, header, lpl: lpl.clone(), fill: 0 };
            prop_assert!(send(&mut m).is_ok(), "status {status:02x}");
        }
    }

    /// Table 8-179: a present reply body must carry the one's-complement
    /// checksum; a mismatch is refused; an absent body is not verified.
    #[test]
    fn reply_checksum_per_spec(
        lpl in prop::collection::vec(any::<u8>(), 36..=120),
    ) {
        let len = lpl.len() as u8;
        let good = chkcode(&lpl);
        let mut m = ScriptedModule { status: 0x01, header: [len, good], lpl: lpl.clone(), fill: 0 };
        prop_assert!(send(&mut m).is_ok());
        let bad = good.wrapping_add(1);
        let mut m = ScriptedModule { status: 0x01, header: [len, bad], lpl: lpl.clone(), fill: 0 };
        prop_assert!(matches!(send(&mut m), Err(CdbError::ChecksumMismatch)));
        // Empty body: RPLChkCode 0 is accepted (nothing to verify).
        let mut m = ScriptedModule { status: 0x01, header: [0, 0], lpl: vec![], fill: 0 };
        // 0x0040 requires 36 reply bytes, so a declared-empty reply is a
        // truncation, not a checksum case; use it to pin that ordering too.
        let r = send(&mut m);
        assert!(matches!(r, Err(CdbError::TruncatedReply { expected: 36, actual: 0 })));
    }

    /// RPLLength bounds: > 120 is refused outright; below the command's
    /// fixed reply layout (36 bytes for 0x0040) is a truncation; any
    /// 36..=120 with a matching checksum decodes.
    #[test]
    fn reply_length_per_spec(
        lpl in prop::collection::vec(any::<u8>(), 36..=120),
    ) {
        let mut m = ScriptedModule { status: 0x01, header: [121, 0], lpl: vec![], fill: 0 };
        prop_assert!(matches!(send(&mut m), Err(CdbError::LplTooLong)));
        for short in [0u8, 1, 17, 35] {
            let mut m = ScriptedModule { status: 0x01, header: [short, 0], lpl: vec![0; short as usize], fill: 0 };
            prop_assert!(
                matches!(send(&mut m), Err(CdbError::TruncatedReply { expected: 36, actual }) if actual == short as usize),
                "short {short}"
            );
        }
        let header = [lpl.len() as u8, chkcode(&lpl)];
        let mut m = ScriptedModule { status: 0x01, header, lpl, fill: 0 };
        prop_assert!(send(&mut m).is_ok());
    }

    /// Every command's `decode_reply` is total on arbitrary status and
    /// body bytes: it returns a response or a typed error, never panics,
    /// for bodies far shorter than and longer than the declared layout.
    #[test]
    fn decode_reply_total_for_every_command(
        status in any::<u8>(),
        lpl in prop::collection::vec(any::<u8>(), 0..=130),
    ) {
        let _ = <CmisCdbModuleFeatures as CdbCommand>::decode_reply(status, &lpl);
        let _ = <CmisCdbFirmwareManagementFeatures as CdbCommand>::decode_reply(status, &lpl);
        let _ = <CmisCdbGetFirmwareInfo as CdbCommand>::decode_reply(status, &lpl);
        let _ = <CmisCdbLoopbacks as CdbCommand>::decode_reply(status, &lpl);
        let _ = <CmisCdbRmonStatistics as CdbCommand>::decode_reply(status, &lpl);
    }
}
