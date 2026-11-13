//! Page 00h application-compliance projection, dispatched per detected
//! revision.
//!
//! `compliance_extended` (byte 3 bit 7) carries a revision window: the
//! schema records it `since: "1.4"`, so the v1_3 reader does not decode it
//! at all. The projection therefore carries it as an `Option<bool>`: `None`
//! at 1.3 (the bit did not exist with this meaning), `Some(bit)` at every
//! later revision — never a borrowed later-revision meaning.

use openxcvr_core::rev::Rev;
use openxcvr_core::{error::ModuleError, transport::Transport, types::Application};

use crate::ctx::Ctx;

/// Private common projection of the Page 00h application fields.
#[derive(Debug)]
pub(super) struct AppCommon {
    pub compliance_extended: Option<bool>,
    pub ext_spec_name: String,
    pub compliance_10gbase_sr: bool,
    pub compliance_10gbase_lr: bool,
    pub compliance_10gbase_lrm: bool,
    pub compliance_40gbase_sr4: bool,
    pub compliance_40gbase_lr4: bool,
    pub compliance_40gbase_cr4: bool,
    pub compliance_40g_active_cable: bool,
    pub compliance_sas_rate: [bool; 4],
    pub compliance_oc48_short_reach: bool,
    pub compliance_oc48_intermediate_reach: bool,
    pub compliance_oc48_long_reach: bool,
    pub compliance_1000base: [bool; 4],
    pub compliance_fibre_channel_link_length: [bool; 5],
    pub compliance_fc_shortwave_laser_without_ofc: bool,
    pub compliance_fc_shortwave_laser_with_ofc: bool,
    pub compliance_fc_electrical_intra_enclosure: bool,
    pub compliance_fc_electrical_inter_enclosure: bool,
    pub compliance_fc_longwave_laser: bool,
    pub nominal_signaling_rate: u8,
    pub connector_name: String,
}

/// Project the application fields from a revision-specialised generated
/// page. `$compliance_extended` is spliced textually inside the expansion,
/// where `page` is in scope: `Some(page.compliance_extended)` for revisions
/// that decode the bit, `None` for v1_3 where the bit is not modeled.
macro_rules! project_page00_app {
    ($buf:expr, $page:path, $compliance_extended:expr) => {{
        let page = <$page>::decode($buf);
        AppCommon {
            compliance_extended: $compliance_extended(&page),
            ext_spec_name: page.ext_spec_compliance.name(),
            compliance_10gbase_sr: page.compliance_10gbase_sr,
            compliance_10gbase_lr: page.compliance_10gbase_lr,
            compliance_10gbase_lrm: page.compliance_10gbase_lrm,
            compliance_40gbase_sr4: page.compliance_40gbase_sr4,
            compliance_40gbase_lr4: page.compliance_40gbase_lr4,
            compliance_40gbase_cr4: page.compliance_40gbase_cr4,
            compliance_40g_active_cable: page.compliance_40g_active_cable,
            compliance_sas_rate: page.compliance_sas_rate,
            compliance_oc48_short_reach: page.compliance_oc48_short_reach,
            compliance_oc48_intermediate_reach: page.compliance_oc48_intermediate_reach,
            compliance_oc48_long_reach: page.compliance_oc48_long_reach,
            compliance_1000base: page.compliance_1000base,
            compliance_fibre_channel_link_length: page.compliance_fibre_channel_link_length,
            compliance_fc_shortwave_laser_without_ofc: page
                .compliance_fc_shortwave_laser_without_ofc,
            compliance_fc_shortwave_laser_with_ofc: page.compliance_fc_shortwave_laser_with_ofc,
            compliance_fc_electrical_intra_enclosure: page.compliance_fc_electrical_intra_enclosure,
            compliance_fc_electrical_inter_enclosure: page.compliance_fc_electrical_inter_enclosure,
            compliance_fc_longwave_laser: page.compliance_fc_longwave_laser,
            nominal_signaling_rate: page.nominal_signaling_rate,
            connector_name: page.connector_type.name(),
        }
    }};
}

/// Maps the projected SFF-8636 compliance fields to spec bit names.
fn spec_bits(page: &AppCommon) -> Vec<String> {
    let mut bits = Vec::new();
    // When compliance_extended is set, the primary compliance bits are
    // invalid — the spec compliance comes from ext_spec_compliance instead.
    // At rev 1.3 the bit is not modeled (None) and the primary bits stand.
    if page.compliance_extended == Some(true) {
        if page.ext_spec_name != "Unspecified" {
            bits.push(page.ext_spec_name.clone());
        }
        return bits;
    }
    if page.compliance_10gbase_sr {
        bits.push("10GBASE-SR".into());
    }
    if page.compliance_10gbase_lr {
        bits.push("10GBASE-LR".into());
    }
    if page.compliance_10gbase_lrm {
        bits.push("10GBASE-LRM".into());
    }
    if page.compliance_40gbase_sr4 {
        bits.push("40GBASE-SR4".into());
    }
    if page.compliance_40gbase_lr4 {
        bits.push("40GBASE-LR4".into());
    }
    if page.compliance_40gbase_cr4 {
        bits.push("40GBASE-CR4".into());
    }
    if page.compliance_40g_active_cable {
        bits.push("40G-Active-Cable".into());
    }
    for (i, &set) in page.compliance_sas_rate.iter().enumerate() {
        if set {
            bits.push(format!("SAS-{:?}-Gbps", i + 1));
        }
    }
    if page.compliance_oc48_short_reach {
        bits.push("OC48-Short-Reach".into());
    }
    if page.compliance_oc48_intermediate_reach {
        bits.push("OC48-Intermediate-Reach".into());
    }
    if page.compliance_oc48_long_reach {
        bits.push("OC48-Long-Reach".into());
    }
    for (set, name) in page.compliance_1000base.iter().zip([
        "1000BASE-T",
        "1000BASE-CX",
        "1000BASE-LX",
        "1000BASE-SX",
    ]) {
        if *set {
            bits.push(name.into());
        }
    }
    for (i, &set) in page.compliance_fibre_channel_link_length.iter().enumerate() {
        if set {
            bits.push(format!("FC-LinkLength-{:#x}", i));
        }
    }
    if page.compliance_fc_shortwave_laser_without_ofc {
        bits.push("FC-SW-Without-OCF".into());
    }
    if page.compliance_fc_shortwave_laser_with_ofc {
        bits.push("FC-SW-With-OCF".into());
    }
    if page.compliance_fc_electrical_intra_enclosure {
        bits.push("FC-Electrical-Intra".into());
    }
    if page.compliance_fc_electrical_inter_enclosure {
        bits.push("FC-Electrical-Inter".into());
    }
    if page.compliance_fc_longwave_laser {
        bits.push("FC-LW".into());
    }
    bits
}

fn read_page00h_app(t: &mut dyn Transport, ctx: Option<Ctx>) -> Result<AppCommon, ModuleError> {
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
        None => Ok(project_page00_app!(
            &buf,
            g::union::Sff8636Page00hUpperAdministrative,
            |p: &g::union::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
        )),
        Some(ctx) => match ctx.detected_rev {
            // The extended-compliance bit is not modeled at 1.3 (since 1.4):
            // None, never a borrowed later-revision meaning.
            Rev::SFF8636_1_3 => Ok(project_page00_app!(
                &buf,
                g::v1_3::Sff8636Page00hUpperAdministrative,
                |_: &g::v1_3::Sff8636Page00hUpperAdministrative| None
            )),
            Rev::SFF8636_1_4 => Ok(project_page00_app!(
                &buf,
                g::v1_4::Sff8636Page00hUpperAdministrative,
                |p: &g::v1_4::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_1_5 => Ok(project_page00_app!(
                &buf,
                g::v1_5::Sff8636Page00hUpperAdministrative,
                |p: &g::v1_5::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_2_0 => Ok(project_page00_app!(
                &buf,
                g::v2_0::Sff8636Page00hUpperAdministrative,
                |p: &g::v2_0::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_2_5 => Ok(project_page00_app!(
                &buf,
                g::v2_5::Sff8636Page00hUpperAdministrative,
                |p: &g::v2_5::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_2_8 => Ok(project_page00_app!(
                &buf,
                g::v2_8::Sff8636Page00hUpperAdministrative,
                |p: &g::v2_8::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_2_11 => Ok(project_page00_app!(
                &buf,
                g::v2_11::Sff8636Page00hUpperAdministrative,
                |p: &g::v2_11::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            Rev::SFF8636_2_12 => Ok(project_page00_app!(
                &buf,
                g::v2_12::Sff8636Page00hUpperAdministrative,
                |p: &g::v2_12::Sff8636Page00hUpperAdministrative| Some(p.compliance_extended)
            )),
            _ => Err(ModuleError::Unsupported),
        },
    }
}

pub(super) fn applications(
    t: &mut dyn Transport,
    ctx: Option<Ctx>,
) -> Result<Application, ModuleError> {
    let page = read_page00h_app(t, ctx)?;
    Ok(Application {
        extended_spec: page.ext_spec_name.clone(),
        spec_bits: spec_bits(&page),
        rate_gt: if page.nominal_signaling_rate == 0 {
            None
        } else {
            // nominal_signaling_rate is in 100 MBd units (0.1 GT/s per unit)
            Some(page.nominal_signaling_rate as f32 * 0.1)
        },
        connector: page.connector_name.clone(),
    })
}
