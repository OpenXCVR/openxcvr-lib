use super::pages::read_a0h;
use openxcvr_core::{error::ModuleError, transport::Transport, types::VendorInfo};

use crate::ctx::Ctx;

pub(super) fn identifier(t: &mut dyn Transport) -> Result<u8, ModuleError> {
    let mut buf = [0u8; 1];
    t.read(0xA0, 0, 0, 0, &mut buf)?;
    Ok(buf[0])
}

pub(super) fn vendor_info(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<VendorInfo, ModuleError> {
    let a0h = read_a0h(t, ctx)?;
    Ok(VendorInfo {
        name: ascii_trim(&a0h.vendor_name),
        oui: a0h.vendor_oui,
        part_number: ascii_trim(&a0h.vendor_pn),
        // Hardware revision ASCII string (Table 8-2) - never the spec
        // compliance revision.
        rev: ascii_trim(&a0h.vendor_rev),
        serial_number: ascii_trim(&a0h.vendor_sn),
        date_code: ascii_trim(&a0h.date_code),
        // Filled by the supplied identity decode, which reads byte 0.
        identifier: None,
        // Spec compliance revision proven from A0h:94 by the detect-once
        // ctx; None on the undetected union path, never guessed.
        spec_rev: ctx.map(|c| c.detected_rev),
    })
}

fn ascii_trim(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0x00)
        .map(|i| i + 1)
        .unwrap_or(0);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
