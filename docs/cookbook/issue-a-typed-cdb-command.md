# Issue a typed CDB command

A recipe from the [task cookbook](../cookbook.md), which states the
conventions every recipe shares.

**Goal.** Send CMIS CDB 0100h (Get Firmware Information) as a typed
command. The snippet's transport is the stand-in for the caller's bus: it
records the Page 9Fh write and returns a scripted reply. The reply bytes
are synthetic, not a module.

```rust
use openxcvr::core::transport::{Transport, I2C_ADDR_MODULE};
use openxcvr::prelude::*;
use openxcvr_cmis::adapter::{CmisCdbGetFirmwareInfo, CmisCdbGetFirmwareInfoRequest};
use openxcvr_cmis::module::CmisTransport;
use openxcvr_cmis::Cdb;
use openxcvr_cmis::CmisModule;

struct ScriptedCdb {
    lower: [u8; 128],
    page9f: [u8; 256],
    wrote: Vec<u8>,
}

impl ScriptedCdb {
    fn new() -> Self {
        let mut lower = [0u8; 128];
        lower[0] = 0x18;
        lower[1] = 0x52;
        lower[37] = 0x01; // CDB status: not busy, success (below 0x40)
        let mut lpl = [0u8; 110];
        lpl[0] = 0x01; // firmware status
        lpl[1] = 0x01; // image A advertised
        lpl[2..13].copy_from_slice(b"1.0.0-synth");
        let sum = lpl.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        let mut page9f = [0u8; 256];
        page9f[134] = lpl.len() as u8;
        page9f[135] = !sum; // one's-complement checksum of the reply LPL
        page9f[136..136 + lpl.len()].copy_from_slice(&lpl);
        Self { lower, page9f, wrote: Vec::new() }
    }
}

impl Transport for ScriptedCdb {
    fn read(&mut self, _: u8, page: u8, _: u8, offset: u8, buf: &mut [u8]) -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x9F, _) => &self.page9f[offset as usize..],
            _ => return Err(ModuleError::Io(format!("unread page {page:#04x}").into())),
        };
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }
    fn write(&mut self, slave: u8, page: u8, _: u8, offset: u8, data: &[u8]) -> Result<(), ModuleError> {
        if slave != I2C_ADDR_MODULE || page != 0x9F || offset != 128 {
            return Err(ModuleError::Io(format!("unexpected write {page:#04x}:{offset}").into()));
        }
        self.wrote = data.to_vec();
        println!("cdb write opcode 0x{:02X}{:02X} lpl_len {}", data[0], data[1], data[4]);
        Ok(())
    }
    fn probe(&mut self) -> Result<(), ModuleError> { Ok(()) }
}
impl CmisTransport for ScriptedCdb {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = CmisModule::new(ScriptedCdb::new())?;
    let reply = module.cdb_send::<CmisCdbGetFirmwareInfo>(&CmisCdbGetFirmwareInfoRequest, 2)?;
    let image_a = String::from_utf8_lossy(&reply.image_a_firmware);
    println!(
        "0100h firmware_status {:02x} image_information {:02x} image_a {}",
        reply.firmware_status[0], reply.image_information[0], image_a.trim_end_matches('\0')
    );

    // 0100h is defined from CMIS 4.0. A 3.0 context is refused before any write.
    let mut early = ScriptedCdb::new();
    let refused = Cdb::send_checked::<CmisCdbGetFirmwareInfo, _>(
        &mut early, &CmisCdbGetFirmwareInfoRequest, Rev::CMIS_3_0, 1,
    );
    println!("cmis 3.0 wrote {} err {refused:?}", early.wrote.len());
    Ok(())
}
```

Expected output:

```text
cdb write opcode 0x0100 lpl_len 0
0100h firmware_status 01 image_information 01 image_a 1.0.0-synth
cmis 3.0 wrote 0 err Err(UnsupportedInRev { op: GetFirmwareInfo, since: Rev { family: Cmis, major: 4, minor: 0 }, detected: Rev { family: Cmis, major: 3, minor: 0 } })
```

**Notes.** `CmisModule::cdb_send` is `Cdb::send_checked` with the revision
frozen at `CmisModule::new`. `max_polls` is the caller's poll budget; this
library does not sleep. The facade has no typed CDB command.
