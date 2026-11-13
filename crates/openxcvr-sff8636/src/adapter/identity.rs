//! Page 00h (Upper Administrative) per-revision dispatch and the identity
//! projection consumed by [`openxcvr_core::module::Module::vendor_info`].

use openxcvr_core::rev::Rev;
use openxcvr_core::{error::ModuleError, transport::Transport, types::VendorInfo};

use crate::ctx::Ctx;

/// Private common projection of Upper Page 00h: the identity fields this
/// module's `vendor_info` consumes — all present at every admitted
/// revision, so the projection compiles against every per-revision reader
/// and the union view alike.
#[derive(Debug)]
pub(super) struct Page00Common {
    pub vendor_name: [u8; 16],
    pub vendor_oui: [u8; 3],
    pub vendor_pn: [u8; 16],
    pub vendor_rev: [u8; 2],
    pub vendor_sn: [u8; 16],
    pub date_code_year: [u8; 2],
    pub date_code_month: [u8; 2],
    pub date_code_day: [u8; 2],
    pub date_code_lot_code: [u8; 2],
}

/// Project the identity fields from a revision-specialised generated
/// `Sff8636Page00hUpperAdministrative` page; field access stays
/// compiler-checked against the concrete type.
macro_rules! project_page00 {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        crate::adapter::record_decoded_page00_type(std::any::type_name_of_val(&page));
        Page00Common {
            vendor_name: page.vendor_name,
            vendor_oui: page.vendor_oui,
            vendor_pn: page.vendor_pn,
            vendor_rev: page.vendor_rev,
            vendor_sn: page.vendor_sn,
            date_code_year: page.date_code_year,
            date_code_month: page.date_code_month,
            date_code_day: page.date_code_day,
            date_code_lot_code: page.date_code_lot_code,
        }
    }};
}

/// Read and decode Upper Page 00h through the revision-specialised reader
/// selected by the frozen `ctx` (`None` keeps the union view — see
/// [`super::lower::decode_lower`]).
pub(super) fn read_page00h(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<Page00Common, ModuleError> {
    let mut buf = [0u8; 128];
    t.read(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        128,
        &mut buf,
    )?;
    use crate::generated as g;
    match ctx {
        None => Ok(project_page00!(
            &buf,
            g::union::Sff8636Page00hUpperAdministrative
        )),
        Some(ctx) => match ctx.detected_rev {
            Rev::SFF8636_1_3 => Ok(project_page00!(
                &buf,
                g::v1_3::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_1_4 => Ok(project_page00!(
                &buf,
                g::v1_4::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_1_5 => Ok(project_page00!(
                &buf,
                g::v1_5::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_2_0 => Ok(project_page00!(
                &buf,
                g::v2_0::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_2_5 => Ok(project_page00!(
                &buf,
                g::v2_5::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_2_8 => Ok(project_page00!(
                &buf,
                g::v2_8::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_2_11 => Ok(project_page00!(
                &buf,
                g::v2_11::Sff8636Page00hUpperAdministrative
            )),
            Rev::SFF8636_2_12 => Ok(project_page00!(
                &buf,
                g::v2_12::Sff8636Page00hUpperAdministrative
            )),
            _ => Err(ModuleError::Unsupported),
        },
    }
}

pub(super) fn identifier(t: &mut dyn Transport) -> Result<u8, ModuleError> {
    let mut buf = [0u8; 1];
    t.read(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        0,
        &mut buf,
    )?;
    Ok(buf[0])
}

pub(super) fn vendor_info(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<VendorInfo, ModuleError> {
    let page = read_page00h(t, ctx)?;
    let mut date_bytes = [0u8; 8];
    date_bytes[0..2].copy_from_slice(&page.date_code_year);
    date_bytes[2..4].copy_from_slice(&page.date_code_month);
    date_bytes[4..6].copy_from_slice(&page.date_code_day);
    date_bytes[6..8].copy_from_slice(&page.date_code_lot_code);
    Ok(VendorInfo {
        name: ascii_trim(&page.vendor_name),
        oui: page.vendor_oui,
        part_number: ascii_trim(&page.vendor_pn),
        // Hardware revision ASCII string (Table 6-16) — never the spec
        // compliance revision.
        rev: ascii_trim(&page.vendor_rev),
        serial_number: ascii_trim(&page.vendor_sn),
        date_code: ascii_trim(&date_bytes),
        // Byte 0 is not part of this read; see VendorInfo::identifier.
        identifier: None,
        // Spec compliance revision proven from Lower:1 by the detect-once
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
