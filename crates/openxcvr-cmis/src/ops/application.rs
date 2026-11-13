use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE, Rev};

use crate::{
    adapter::dpsm::{DpState, Dpsm, DpsmError},
    generated::Sff8024HostElectricalInterface,
    module::CmisTransport,
};

const STAGED_CONTROL_PAGE: u8 = 0x10;
/// SCS0::ApplyDPInitLane trigger (CMIS 5.3 § 8.9.3.1, Table 8-70).
const SCS0_APPLY_DP_INIT_OFFSET: u8 = 143;
/// SCS0::DPConfigLane<lane> staged AppSelCode/DataPathID registers, one byte
/// per in-bank lane starting at 145 (CMIS 5.3 § 8.9.3.2, Table 8-72).
const SCS0_DP_CONFIG_FIRST_OFFSET: u8 = 145;
const MAX_HOST_LANES: u8 = 8;

/// Why an application configuration failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum AppError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The AppSel code is not one of the advertised descriptors.
    AppCodeNotAdvertised(u8),
    /// The requested lane count exceeds the data path's host lanes.
    InvalidLaneRange {
        /// The data path that was configured.
        dp_id: u8,
        /// The lane count that exceeded it.
        lane_count: u8,
    },
    /// The data-path state machine failed; the original error is the source.
    DpsmFailed(DpsmError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "application transport error: {err}"),
            Self::AppCodeNotAdvertised(app_sel_code) => {
                write!(f, "application code 0x{app_sel_code:02x} is not advertised")
            }
            Self::InvalidLaneRange { dp_id, lane_count } => write!(
                f,
                "data path at host lane {dp_id} with {lane_count} lanes exceeds the 8-lane bank"
            ),
            Self::DpsmFailed(err) => write!(f, "application DPSM activation failed: {err}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<DpsmError> for AppError {
    fn from(err: DpsmError) -> Self {
        Self::DpsmFailed(err)
    }
}

impl From<ModuleError> for AppError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// An application advertised by the module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    /// The AppSel code identifying this application: the 1-based sequence
    /// number of its Application Descriptor slot (CMIS 5.3 § 6.2.5.1 — the
    /// AppSel code is the descriptor number, NOT the SFF-8024 host-interface
    /// ID carried inside the descriptor).
    pub app_sel_code: u8,
    /// SFF-8024 host electrical interface code advertised in the descriptor.
    pub host_interface_code: u8,
    /// SFF-8024 media interface code advertised in the descriptor.
    pub media_interface_code: u8,
    /// Host lanes the application uses.
    pub host_lane_count: u8,
    /// Media lanes the application uses.
    pub media_lane_count: u8,
    /// Permitted first host lanes, as assignment bits.
    pub host_lane_assignment_options: u8,
}

impl Application {
    /// Build an `Application` from one Page 01h application-descriptor slot.
    /// The descriptor's base fields (host interface id, host/media lane counts)
    /// are rev-invariant, so the caller passes the values it already extracted
    /// from the rev-specific `CmisPage01hAdvertising`; the remaining two bytes
    /// are read straight from the raw page buffer. Each 4-byte descriptor
    /// starts at buf[95 + 4*idx] per CMIS 5.3 § 8.4.7.
    fn from_slot(
        host_if: Sff8024HostElectricalInterface,
        host_lane_count: u8,
        media_lane_count: u8,
        buf: &[u8; 128],
        idx: usize,
    ) -> Option<Self> {
        if host_if == Sff8024HostElectricalInterface::Undefined {
            return None;
        }
        Some(Self {
            app_sel_code: (idx + 1) as u8,
            host_interface_code: host_if.repr(),
            // media_interface_id_app and host_lane_assignment_options_app are
            // variable-length/skipped by codegen; read directly from the raw buf.
            media_interface_code: buf[96 + 4 * idx],
            host_lane_count,
            media_lane_count,
            host_lane_assignment_options: buf[98 + 4 * idx],
        })
    }
}

/// Decode Page 01h's application-descriptor arrays through the per-rev
/// generated module set selected by `rev`.
/// Returns the `(host_interface_id, host_lane_count, media_lane_count)` arrays
/// for the seven descriptor slots.
///
/// The descriptor base fields consumed here are rev-invariant across
/// CMIS 3.0–5.3 (CMIS 5.3 § 8.4.7), so every arm extracts the identical triple
/// and this dispatch has no behavioral divergence today. It exists to honor the
/// ADR-0029 contract that decode flows through the detected-rev module set, so
/// that a future rev which *does* relocate these descriptors decodes correctly
/// without touching this call site.
fn decode_app_descriptors(
    rev: Rev,
    buf: &[u8; 128],
) -> ([Sff8024HostElectricalInterface; 7], [u8; 7], [u8; 7]) {
    use crate::generated::{v3_0, v4_0, v5_0, v5_1, v5_2, v5_3};
    macro_rules! extract {
        ($m:ident) => {{
            let p = $m::CmisPage01hAdvertising::decode(buf);
            (
                p.host_interface_id_app,
                p.host_lane_count_app,
                p.media_lane_count_app,
            )
        }};
    }
    match rev {
        r if r == Rev::CMIS_3_0 => extract!(v3_0),
        r if r == Rev::CMIS_4_0 => extract!(v4_0),
        r if r == Rev::CMIS_5_0 => extract!(v5_0),
        r if r == Rev::CMIS_5_1 => extract!(v5_1),
        r if r == Rev::CMIS_5_2 => extract!(v5_2),
        _ => extract!(v5_3),
    }
}

/// Read Page 01h upper page and return all non-sentinel application descriptors,
/// decoding through the module's detected CMIS revision.
pub fn list_advertised<T: CmisTransport>(
    transport: &mut T,
    rev: Rev,
) -> Result<Vec<Application>, AppError> {
    let mut buf = [0u8; 128];
    transport.read(I2C_ADDR_MODULE, 0x01, 0, 128, &mut buf)?;

    let (host_if, host_lanes, media_lanes) = decode_app_descriptors(rev, &buf);
    Ok((0..7)
        .filter_map(|i| Application::from_slot(host_if[i], host_lanes[i], media_lanes[i], &buf, i))
        .collect())
}

/// Activate the application identified by `app_sel_code` (a Page 01h
/// Application Descriptor number, 1-based) on the data path whose first host
/// lane is `dp_id` (0-based).
///
/// Stages SCS0 DPConfigLane (AppSelCode in bits 7-4, DataPathID in bits 3-1,
/// ExplicitControl preserved) for every host lane of the data path, then
/// triggers SCS0::ApplyDPInitLane with the full lane mask — CMIS requires the
/// Apply trigger to hit all lanes of a data path simultaneously (CMIS 5.3
/// § 6.3.2.1) — and waits for the DPSM to reach DPInitialized.
pub fn activate<T: CmisTransport>(
    transport: &mut T,
    rev: Rev,
    app_sel_code: u8,
    dp_id: u8,
    max_polls: u32,
) -> Result<(), AppError> {
    let app = list_advertised(transport, rev)?
        .into_iter()
        .find(|app| app.app_sel_code == app_sel_code)
        .ok_or(AppError::AppCodeNotAdvertised(app_sel_code))?;

    let lane_count = app.host_lane_count;
    if lane_count == 0 || dp_id >= MAX_HOST_LANES || lane_count > MAX_HOST_LANES - dp_id {
        return Err(AppError::InvalidLaneRange { dp_id, lane_count });
    }

    // Pages 10h are lane-banked 8 lanes per bank (CMIS 5.3 § 8.2); the lane
    // range guard above confines this data path to bank 0.
    let lane_mask = (((1u16 << lane_count) - 1) << dp_id) as u8;
    for lane in dp_id..dp_id + lane_count {
        let offset = SCS0_DP_CONFIG_FIRST_OFFSET + lane;
        let mut config = [0u8; 1];
        transport.read(I2C_ADDR_MODULE, STAGED_CONTROL_PAGE, 0, offset, &mut config)?;
        config[0] = (config[0] & 0x01) | (app_sel_code << 4) | (dp_id << 1);
        transport.write(I2C_ADDR_MODULE, STAGED_CONTROL_PAGE, 0, offset, &config)?;
    }

    transport.write(
        I2C_ADDR_MODULE,
        STAGED_CONTROL_PAGE,
        0,
        SCS0_APPLY_DP_INIT_OFFSET,
        &[lane_mask],
    )?;

    Dpsm::wait_for(transport, dp_id, DpState::Initialized, max_polls)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{error::ModuleError, transport::Transport};

    const PAGE_01H: u8 = 0x01;
    const PAGE_10H: u8 = 0x10;
    const PAGE_11H: u8 = 0x11;

    struct MockTransport {
        page_01h: [u8; 128],
        page_10h: [u8; 256],
        page_10h_writes: Vec<(u8, u8, Vec<u8>)>,
        page_11h_state: [u8; 4],
    }

    impl Default for MockTransport {
        fn default() -> Self {
            Self {
                page_01h: [0u8; 128],
                page_10h: [0u8; 256],
                page_10h_writes: Vec::new(),
                page_11h_state: [0u8; 4],
            }
        }
    }

    impl MockTransport {
        fn with_application(host_interface_code: u8) -> Self {
            let mut transport = Self::default();
            // CMIS 5.3 § 8.4.7: descriptor slot 1 at Page 01h byte 223 = buf[95]
            transport.page_01h[95..99].copy_from_slice(&[
                host_interface_code, // host_interface_id (SFF-8024 code)
                0x01,                // media_interface_code
                0x44,                // host_lane_count=4 | media_lane_count=4
                0x01,                // host_lane_assignment_options
            ]);
            transport
        }

        fn with_second_application(mut self, host_interface_code: u8) -> Self {
            // Descriptor slot 2 at Page 01h byte 227 = buf[99].
            self.page_01h[99..103].copy_from_slice(&[
                host_interface_code,
                0x01,
                0x22, // host_lane_count=2 | media_lane_count=2
                0x03,
            ]);
            self
        }

        fn with_initialized_dpsm(mut self) -> Self {
            self.page_11h_state[0] = 0x07;
            self
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            match (page, bank, offset, buf.len()) {
                (PAGE_01H, 0, 128, 128) => buf.copy_from_slice(&self.page_01h),
                (PAGE_10H, 0, offset, 1) => buf[0] = self.page_10h[offset as usize],
                (PAGE_11H, 0, 128, 4) => buf.copy_from_slice(&self.page_11h_state),
                _ => panic!(
                    "unexpected read page=0x{page:02x} bank={bank} offset={offset} len={}",
                    buf.len()
                ),
            }
            Ok(())
        }

        fn write(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            data: &[u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(page, PAGE_10H);
            self.page_10h[offset as usize..offset as usize + data.len()].copy_from_slice(data);
            self.page_10h_writes.push((bank, offset, data.to_vec()));
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_list_advertised() {
        let mut transport = MockTransport::with_application(0x1A);

        assert_eq!(
            list_advertised(&mut transport, Rev::CMIS_5_3).unwrap(),
            vec![Application {
                // AppSelCode is the 1-based descriptor slot number, not the
                // 0x1A SFF-8024 host-interface ID inside the descriptor.
                app_sel_code: 1,
                host_interface_code: 0x1A,
                media_interface_code: 0x01,
                host_lane_count: 4,
                media_lane_count: 4,
                host_lane_assignment_options: 0x01,
            }]
        );
    }

    /// The Page 01h application descriptors are rev-invariant (CMIS 5.3
    /// § 8.4.7), so decoding the same buffer through every per-rev module set
    /// must yield identical results. This guards the rev-dispatch
    /// wiring in `decode_app_descriptors`.
    #[test]
    fn test_list_advertised_all_revs() {
        let expected = vec![Application {
            app_sel_code: 1,
            host_interface_code: 0x1A,
            media_interface_code: 0x01,
            host_lane_count: 4,
            media_lane_count: 4,
            host_lane_assignment_options: 0x01,
        }];
        for rev in [
            Rev::CMIS_3_0,
            Rev::CMIS_4_0,
            Rev::CMIS_5_0,
            Rev::CMIS_5_1,
            Rev::CMIS_5_2,
            Rev::CMIS_5_3,
        ] {
            let mut transport = MockTransport::with_application(0x1A);
            assert_eq!(
                list_advertised(&mut transport, rev).unwrap(),
                expected.clone(),
                "rev {rev:?} dispatch diverged"
            );
        }
    }

    #[test]
    fn test_activate_found() {
        let mut transport = MockTransport::with_application(0x1A).with_initialized_dpsm();

        assert!(activate(&mut transport, Rev::CMIS_5_3, 1, 0, 100).is_ok());

        // Slot 1 (AppSelCode 0x1) staged on lanes 0-3 at 145-148 with
        // DataPathID 0, then ApplyDPInit triggered for all four lanes at once.
        assert_eq!(
            transport.page_10h_writes,
            vec![
                (0, 145, vec![0x10]),
                (0, 146, vec![0x10]),
                (0, 147, vec![0x10]),
                (0, 148, vec![0x10]),
                (0, 143, vec![0x0F]),
            ]
        );
    }

    #[test]
    fn test_activate_stages_slot_code_and_data_path_id_per_lane() {
        let mut transport = MockTransport::with_application(0x1A)
            .with_second_application(0x21)
            .with_initialized_dpsm();
        // dp 2 initialized: nibble 2 = low nibble of byte 131 (buf[1]).
        transport.page_11h_state[1] = 0x07;

        assert!(activate(&mut transport, Rev::CMIS_5_3, 2, 2, 100).is_ok());

        // AppSelCode 0x2 (slot) | DataPathID 0x2 (first lane, 0-based).
        assert_eq!(
            transport.page_10h_writes,
            vec![
                (0, 147, vec![0x24]),
                (0, 148, vec![0x24]),
                (0, 143, vec![0x0C]),
            ]
        );
    }

    #[test]
    fn test_activate_rejects_lane_range_beyond_bank() {
        let mut transport = MockTransport::with_application(0x1A).with_initialized_dpsm();

        assert!(matches!(
            activate(&mut transport, Rev::CMIS_5_3, 1, 6, 100),
            Err(AppError::InvalidLaneRange {
                dp_id: 6,
                lane_count: 4
            })
        ));
        assert!(transport.page_10h_writes.is_empty());
    }

    #[test]
    fn test_activate_not_found() {
        let mut transport = MockTransport::with_application(0x1A).with_initialized_dpsm();

        assert!(matches!(
            activate(&mut transport, Rev::CMIS_5_3, 0xFF, 0, 100),
            Err(AppError::AppCodeNotAdvertised(0xFF))
        ));
    }
}
