//! cargo-fuzz target: CMIS CDB reply parsing.
//!
//! Drives `Cdb::send` (0x0040 Module Features) end to end over a scripted
//! transport built from the fuzz input — status byte, reply header
//! (length, checksum) and reply LPL all come from the byte stream — and
//! additionally sweeps every command's `decode_reply` on the remaining
//! bytes. No reachable state may panic: every outcome is a decoded
//! response or a typed `CdbError`.
//!
//! Mirrors `crates/openxcvr-cmis/tests/property_cdb_reply.rs`, which runs
//! the same properties on bounded random input inside `cargo test`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use openxcvr_cmis::adapter::cdb::{
    CdbCommand, CmisCdbFirmwareManagementFeatures, CmisCdbGetFirmwareInfo, CmisCdbLoopbacks,
    CmisCdbModuleFeatures, CmisCdbModuleFeaturesRequest, CmisCdbRmonStatistics,
};
use openxcvr_cmis::module::CmisTransport;
use openxcvr_cmis::Cdb;
use openxcvr_core::error::ModuleError;

struct ScriptedModule {
    status: u8,
    header: [u8; 2],
    lpl: Vec<u8>,
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
        let src: &[u8] = match (page, offset) {
            (0x00, 37) => {
                buf[0] = self.status;
                return Ok(());
            }
            (0x9F, 134) => {
                buf.copy_from_slice(&self.header[..buf.len().min(2)]);
                return Ok(());
            }
            (0x9F, o) if o >= 136 => &self.lpl[(o - 136) as usize..],
            _ => return Ok(()),
        };
        let n = buf.len().min(src.len());
        buf[..n].copy_from_slice(&src[..n]);
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

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let mut m = ScriptedModule {
        status: data[0],
        header: [data[1], data[2]],
        lpl: data[3..].to_vec(),
    };
    let _ = Cdb::send::<CmisCdbModuleFeatures, _>(&mut m, &CmisCdbModuleFeaturesRequest, 3);

    let status = data[0];
    let lpl = &data[1..];
    let _ = <CmisCdbModuleFeatures as CdbCommand>::decode_reply(status, lpl);
    let _ = <CmisCdbFirmwareManagementFeatures as CdbCommand>::decode_reply(status, lpl);
    let _ = <CmisCdbGetFirmwareInfo as CdbCommand>::decode_reply(status, lpl);
    let _ = <CmisCdbLoopbacks as CdbCommand>::decode_reply(status, lpl);
    let _ = <CmisCdbRmonStatistics as CdbCommand>::decode_reply(status, lpl);
});
