//! Lower-memory per-revision dispatch and common projection.
//!
//! [`LowerCommon`] carries only the raw Lower-memory fields this crate's
//! public accessors consume — fields present at EVERY admitted revision, so
//! the projection compiles against each per-revision generated reader
//! (`generated::v1_3` … `generated::v2_12`) and against the rev-agnostic
//! union view (`generated::union`) without conditional field access. The
//! revision window differences (e.g. `revision_compliance` since 1.4,
//! `init_complete_flag` since 2.0) live in the per-revision readers; this
//! projection deliberately re-derives none of them.

use openxcvr_core::rev::Rev;
use openxcvr_core::{error::ModuleError, DetectError};

use crate::ctx::Ctx;

/// Private common projection of the Lower-memory page: the raw fields the
/// lane/flag/state/monitor accessors consume. Field names and Rust types are
/// identical across all admitted revision readers — implementation/type
/// compatibility, NOT a claim of identical protocol semantics across
/// revisions.
#[derive(Debug)]
pub(super) struct LowerCommon {
    pub interrupt_deasserted: bool,
    pub data_not_ready: bool,
    pub tx_los_flag: [bool; 4],
    pub rx_los_flag: [bool; 4],
    pub tx_adaptive_eq_fault_flag: Option<[bool; 4]>,
    pub tx_fault_flag: [bool; 4],
    pub tx_cdr_lol_flag: [bool; 4],
    pub rx_cdr_lol_flag: [bool; 4],
    pub temp_mon_alarm_warning_flag: [bool; 4],
    pub vcc_mon_alarm_warning_flag: [bool; 4],
    pub rx_power_alarm_warning_flag: [bool; 16],
    pub tx_bias_alarm_warning_flag: [bool; 16],
    pub tx_power_alarm_warning_flag: [bool; 16],
    pub temperature: i16,
    pub supply_voltage: u16,
    pub rx_power_lane: [u16; 4],
    pub tx_bias_lane: [u16; 4],
    pub tx_power_lane: [u16; 4],
    pub tx_disable_lane: [bool; 4],
}

/// Project the consumed Lower-memory fields from a revision-specialised
/// generated `Sff8636LowerMemory` page. `$page` names the concrete generated
/// type (e.g. `generated::v2_11::Sff8636LowerMemory`); field access stays
/// compiler-checked against that type. `$tx_adaptive_eq` is spliced inside
/// the expansion, where `page` is in scope: `Some(page.tx_adaptive_eq_fault_flag)`
/// for revisions that decode the flag (schema `since: "2.0"`, Table 6-5),
/// `None` for the pre-2.0 readers where the bits are reserved — never a
/// borrowed later-revision meaning.
macro_rules! project_lower {
    ($buf:expr, $page:path, $tx_adaptive_eq:expr) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        crate::adapter::record_decoded_lower_type(std::any::type_name_of_val(&page));
        LowerCommon {
            interrupt_deasserted: page.interrupt_deasserted,
            data_not_ready: page.data_not_ready,
            tx_los_flag: page.tx_los_flag,
            rx_los_flag: page.rx_los_flag,
            tx_adaptive_eq_fault_flag: $tx_adaptive_eq(&page),
            tx_fault_flag: page.tx_fault_flag,
            tx_cdr_lol_flag: page.tx_cdr_lol_flag,
            rx_cdr_lol_flag: page.rx_cdr_lol_flag,
            temp_mon_alarm_warning_flag: page.temp_mon_alarm_warning_flag,
            vcc_mon_alarm_warning_flag: page.vcc_mon_alarm_warning_flag,
            rx_power_alarm_warning_flag: page.rx_power_alarm_warning_flag,
            tx_bias_alarm_warning_flag: page.tx_bias_alarm_warning_flag,
            tx_power_alarm_warning_flag: page.tx_power_alarm_warning_flag,
            temperature: page.temperature,
            supply_voltage: page.supply_voltage,
            rx_power_lane: page.rx_power_lane,
            tx_bias_lane: page.tx_bias_lane,
            tx_power_lane: page.tx_power_lane,
            tx_disable_lane: page.tx_disable_lane,
        }
    }};
}

/// Decode the Lower-memory page through the revision-specialised reader
/// selected by the frozen detect-once `ctx`:
///
/// - `Some(ctx)`: the concrete `generated::v{rev}` reader for
///   `ctx.detected_rev` — never the union, never an assumed latest revision.
///   An out-of-family revision exits fail-closed with
///   [`ModuleError::Unsupported`].
/// - `None` (a module constructed without detection): the rev-agnostic
///   union view, the historical behavior, which the caller has opted into by
///   constructing via [`crate::adapter::Sff8636Module::new`].
pub(super) fn decode_lower(ctx: Option<Ctx>, buf: &[u8; 128]) -> Result<LowerCommon, ModuleError> {
    use crate::generated as g;
    match ctx {
        None => Ok(project_lower!(
            &buf,
            g::union::Sff8636LowerMemory,
            |page: &g::union::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
        )),
        Some(ctx) => match ctx.detected_rev {
            Rev::SFF8636_1_3 => Ok(project_lower!(
                &buf,
                g::v1_3::Sff8636LowerMemory,
                |_: &g::v1_3::Sff8636LowerMemory| None
            )),
            Rev::SFF8636_1_4 => Ok(project_lower!(
                &buf,
                g::v1_4::Sff8636LowerMemory,
                |_: &g::v1_4::Sff8636LowerMemory| None
            )),
            Rev::SFF8636_1_5 => Ok(project_lower!(
                &buf,
                g::v1_5::Sff8636LowerMemory,
                |_: &g::v1_5::Sff8636LowerMemory| None
            )),
            Rev::SFF8636_2_0 => Ok(project_lower!(
                &buf,
                g::v2_0::Sff8636LowerMemory,
                |page: &g::v2_0::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
            )),
            Rev::SFF8636_2_5 => Ok(project_lower!(
                &buf,
                g::v2_5::Sff8636LowerMemory,
                |page: &g::v2_5::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
            )),
            Rev::SFF8636_2_8 => Ok(project_lower!(
                &buf,
                g::v2_8::Sff8636LowerMemory,
                |page: &g::v2_8::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
            )),
            Rev::SFF8636_2_11 => Ok(project_lower!(
                &buf,
                g::v2_11::Sff8636LowerMemory,
                |page: &g::v2_11::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
            )),
            Rev::SFF8636_2_12 => Ok(project_lower!(
                &buf,
                g::v2_12::Sff8636LowerMemory,
                |page: &g::v2_12::Sff8636LowerMemory| Some(page.tx_adaptive_eq_fault_flag)
            )),
            // Fail closed: a detected revision this crate compiles no
            // decoder for is never decoded through a nearest-latest guess.
            other => {
                let _ = other;
                Err(ModuleError::Unsupported)
            }
        },
    }
}

/// Read and decode the Lower-memory page in one step.
pub(super) fn read_lower(
    t: &mut dyn openxcvr_core::transport::Transport,
    ctx: Option<Ctx>,
) -> Result<LowerCommon, ModuleError> {
    let mut buf = [0u8; 128];
    t.read(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        0,
        &mut buf,
    )?;
    decode_lower(ctx, &buf)
}

/// Fail-closed detection over the transport: read Lower:1 and freeze the
/// detected revision. Shared by `Sff8636Module::open` and the unified
/// auto-detect factory.
pub(crate) fn detect(t: &mut dyn openxcvr_core::transport::Transport) -> Result<Ctx, DetectError> {
    let mut buf = [0u8; 2];
    t.read(
        openxcvr_core::transport::I2C_ADDR_MODULE,
        0x00,
        0,
        0,
        &mut buf,
    )
    .map_err(DetectError::from)?;
    Ctx::from_lower_memory(&buf)
}
