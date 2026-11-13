//! Per-revision dispatch and common projections for the SFF-8472 pages
//! this crate's public accessors consume.
//!
//! Every projected field is present at EVERY admitted revision, so the
//! projections compile against each per-revision generated reader
//! (`generated::v9_3` … `generated::v12_5`) and against the rev-agnostic
//! union view alike. Revision window differences (e.g. laser temperature
//! since 11.4, Page 02h tunability since 12.5) live in the per-revision
//! readers; these projections deliberately re-derive none of them.

use openxcvr_core::rev::Rev;
use openxcvr_core::{error::ModuleError, transport::Transport, DetectError};

use crate::ctx::Ctx;

/// Private common projection of the A0h base memory page.
#[derive(Debug)]
pub(super) struct A0hCommon {
    pub vendor_name: [u8; 16],
    pub vendor_oui: [u8; 3],
    pub vendor_pn: [u8; 16],
    pub vendor_rev: [u8; 4],
    pub vendor_sn: [u8; 16],
    pub date_code: [u8; 8],
    pub extended_spec_name: String,
    pub transceiver_compliance: [bool; 64],
    pub signaling_rate_nominal: u8,
    pub connector_name: String,
    pub digital_diagnostics_implemented: bool,
    pub options: [bool; 16],
}

/// Private common projection of the A2h diagnostics page (the fields the
/// flags and lanes accessors consume).
#[derive(Debug)]
pub(super) struct A2hCommon {
    pub alarm_warning_flag: [bool; 32],
    /// A2h bytes 0-39 alarm/warning threshold sets (Table 9-5), severity
    /// order high_alarm, low_alarm, high_warning, low_warning, in the
    /// spec's parameter LSB units. Consumed by the threshold-evaluation
    /// surface only, so the optional laser-temperature and TEC-current
    /// threshold sets stay out of this projection.
    pub temperature_threshold: [i16; 4],
    pub supply_voltage_threshold: [u16; 4],
    pub tx_bias_threshold: [u16; 4],
    pub tx_power_threshold: [u16; 4],
    pub rx_power_threshold: [u16; 4],
    /// A2h byte 110 per-bit fields (Table 9-16) that the status accessors
    /// consume, present at every admitted revision. The writable control
    /// bits (110.6/110.3, and the revision-windowed 118.3 since 11.4) are
    /// reached by the control surface's raw read-modify-write, not held
    /// here; the full bit decode stays available on the generated pages.
    pub tx_disable_state: bool,
    pub tx_fault_state: bool,
    pub rx_los_state: bool,
    pub data_not_ready: bool,
    /// A2h:118.4 Adaptive Input EQ Fail Flag (Table 10-1), the one
    /// per-lane signal flag beyond Rx LOS / Tx fault this spec defines —
    /// since rev 12.4, so earlier revisions carry it as `None` (the bit
    /// is not modeled there), never a borrowed later-revision reading.
    pub adaptive_input_eq_fail_flag: Option<bool>,
}

macro_rules! project_a0h {
    ($buf:expr, $page:path) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        crate::adapter::record_decoded_a0h_type(std::any::type_name_of_val(&page));
        A0hCommon {
            vendor_name: page.vendor_name,
            vendor_oui: page.vendor_oui,
            vendor_pn: page.vendor_pn,
            vendor_rev: page.vendor_rev,
            vendor_sn: page.vendor_sn,
            date_code: page.date_code,
            extended_spec_name: page.extended_specification_compliance.name(),
            transceiver_compliance: page.transceiver_compliance,
            signaling_rate_nominal: page.signaling_rate_nominal,
            connector_name: page.connector_type.name(),
            digital_diagnostics_implemented: page.digital_diagnostics_implemented,
            options: page.options,
        }
    }};
}

macro_rules! project_a2h {
    ($buf:expr, $page:path, $adaptive_eq:expr) => {{
        let page = <$page>::decode($buf);
        #[cfg(test)]
        crate::adapter::record_decoded_a2h_type(std::any::type_name_of_val(&page));
        A2hCommon {
            alarm_warning_flag: page.alarm_warning_flag,
            temperature_threshold: page.temperature_threshold,
            supply_voltage_threshold: page.supply_voltage_threshold,
            tx_bias_threshold: page.tx_bias_threshold,
            tx_power_threshold: page.tx_power_threshold,
            rx_power_threshold: page.rx_power_threshold,
            tx_disable_state: page.tx_disable_state,
            tx_fault_state: page.tx_fault_state,
            rx_los_state: page.rx_los_state,
            data_not_ready: page.data_not_ready,
            // `$adaptive_eq` is spliced inside the expansion, where `page`
            // is in scope: `Some(page.adaptive_input_eq_fail_flag)` at
            // revisions that decode A2h:118.4 (schema `since: "12.4"`,
            // Table 10-1), `None` where the bit is not modeled — never a
            // borrowed later-revision reading.
            adaptive_input_eq_fail_flag: $adaptive_eq(&page),
        }
    }};
}

/// Read and decode the A0h base memory page through the
/// revision-specialised reader selected by the frozen `ctx` (`None` keeps
/// the union view — the historical undetected path).
pub(super) fn read_a0h(t: &mut dyn Transport, ctx: Option<Ctx>) -> Result<A0hCommon, ModuleError> {
    let mut buf = [0u8; 256];
    t.read(
        openxcvr_core::transport::I2C_ADDR_SFF8472_BASE,
        0,
        0,
        0,
        &mut buf,
    )?;
    use crate::generated as g;
    match ctx {
        None => Ok(project_a0h!(&buf, g::union::PageA0hBaseMemory)),
        Some(ctx) => match ctx.detected_rev {
            Rev::SFF8472_9_3 => Ok(project_a0h!(&buf, g::v9_3::PageA0hBaseMemory)),
            Rev::SFF8472_9_5 => Ok(project_a0h!(&buf, g::v9_5::PageA0hBaseMemory)),
            Rev::SFF8472_10_2 => Ok(project_a0h!(&buf, g::v10_2::PageA0hBaseMemory)),
            Rev::SFF8472_10_4 => Ok(project_a0h!(&buf, g::v10_4::PageA0hBaseMemory)),
            Rev::SFF8472_11_0 => Ok(project_a0h!(&buf, g::v11_0::PageA0hBaseMemory)),
            Rev::SFF8472_11_3 => Ok(project_a0h!(&buf, g::v11_3::PageA0hBaseMemory)),
            Rev::SFF8472_11_4 => Ok(project_a0h!(&buf, g::v11_4::PageA0hBaseMemory)),
            Rev::SFF8472_12_3 => Ok(project_a0h!(&buf, g::v12_3::PageA0hBaseMemory)),
            Rev::SFF8472_12_4 => Ok(project_a0h!(&buf, g::v12_4::PageA0hBaseMemory)),
            Rev::SFF8472_12_5 => Ok(project_a0h!(&buf, g::v12_5::PageA0hBaseMemory)),
            // Fail closed: never a nearest-latest guess.
            _ => Err(ModuleError::Unsupported),
        },
    }
}

/// Read and decode the A2h diagnostics base page through the
/// revision-specialised reader selected by the frozen `ctx`.
pub(super) fn read_a2h(t: &mut dyn Transport, ctx: Option<Ctx>) -> Result<A2hCommon, ModuleError> {
    let mut buf = [0u8; 256];
    t.read(
        openxcvr_core::transport::I2C_ADDR_SFF8472_DIAG,
        0,
        0,
        0,
        &mut buf,
    )?;
    use crate::generated as g;
    match ctx {
        None => Ok(project_a2h!(
            &buf,
            g::union::PageA2hDiagnosticsBase,
            |page: &g::union::PageA2hDiagnosticsBase| Some(page.adaptive_input_eq_fail_flag)
        )),
        Some(ctx) => match ctx.detected_rev {
            Rev::SFF8472_9_3 => Ok(project_a2h!(
                &buf,
                g::v9_3::PageA2hDiagnosticsBase,
                |_: &g::v9_3::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_9_5 => Ok(project_a2h!(
                &buf,
                g::v9_5::PageA2hDiagnosticsBase,
                |_: &g::v9_5::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_10_2 => Ok(project_a2h!(
                &buf,
                g::v10_2::PageA2hDiagnosticsBase,
                |_: &g::v10_2::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_10_4 => Ok(project_a2h!(
                &buf,
                g::v10_4::PageA2hDiagnosticsBase,
                |_: &g::v10_4::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_11_0 => Ok(project_a2h!(
                &buf,
                g::v11_0::PageA2hDiagnosticsBase,
                |_: &g::v11_0::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_11_3 => Ok(project_a2h!(
                &buf,
                g::v11_3::PageA2hDiagnosticsBase,
                |_: &g::v11_3::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_11_4 => Ok(project_a2h!(
                &buf,
                g::v11_4::PageA2hDiagnosticsBase,
                |_: &g::v11_4::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_12_3 => Ok(project_a2h!(
                &buf,
                g::v12_3::PageA2hDiagnosticsBase,
                |_: &g::v12_3::PageA2hDiagnosticsBase| None
            )),
            Rev::SFF8472_12_4 => Ok(project_a2h!(
                &buf,
                g::v12_4::PageA2hDiagnosticsBase,
                |page: &g::v12_4::PageA2hDiagnosticsBase| Some(page.adaptive_input_eq_fail_flag)
            )),
            Rev::SFF8472_12_5 => Ok(project_a2h!(
                &buf,
                g::v12_5::PageA2hDiagnosticsBase,
                |page: &g::v12_5::PageA2hDiagnosticsBase| Some(page.adaptive_input_eq_fail_flag)
            )),
            _ => Err(ModuleError::Unsupported),
        },
    }
}

/// Fail-closed detection over the transport: read A0h byte 94 and freeze
/// the detected revision. Shared by `Sff8472Module::open` and the unified
/// auto-detect factory.
pub(crate) fn detect(t: &mut dyn Transport) -> Result<Ctx, DetectError> {
    let mut buf = [0u8; 1];
    t.read(
        openxcvr_core::transport::I2C_ADDR_SFF8472_BASE,
        0,
        0,
        94,
        &mut buf,
    )
    .map_err(DetectError::from)?;
    let mut page = [0u8; 256];
    page[94] = buf[0];
    Ctx::from_a0h(&page)
}
