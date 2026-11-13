use super::lower::LowerCommon;
use openxcvr_core::types::LaneStatus;

pub(super) fn lanes(
    lower: &LowerCommon,
) -> Result<Vec<LaneStatus>, openxcvr_core::error::ModuleError> {
    Ok((0..4)
        .map(|n| {
            LaneStatus {
                index: (n + 1) as u8,
                rx_power_mw: Some(lower.rx_power_lane[n] as f32 / 10000.0),
                tx_power_mw: Some(lower.tx_power_lane[n] as f32 / 10000.0),
                tx_bias_ma: Some(lower.tx_bias_lane[n] as f32 * 0.002),
                rx_los: Some(lower.rx_los_flag[n]),
                tx_disable: Some(lower.tx_disable_lane[n]),
                tx_fault: Some(lower.tx_fault_flag[n]),
                // Latched per-lane signal flags, Lower bytes 3-5 (Table
                // 6-5): Tx LOS (3.7-4), Tx CDR LOL (5.7-4), Rx CDR LOL
                // (5.3-0). The Tx Adaptive EQ fault (4.7-4) carries a
                // revision window — `None` before rev 2.0, where the bits
                // are reserved and the pre-2.0 readers do not decode them.
                tx_los: Some(lower.tx_los_flag[n]),
                tx_cdr_lol: Some(lower.tx_cdr_lol_flag[n]),
                rx_cdr_lol: Some(lower.rx_cdr_lol_flag[n]),
                tx_adaptive_eq_fault: lower.tx_adaptive_eq_fault_flag.map(|f| f[n]),
                // SFF-8636 has no data-path state machine (CMIS-only field).
                dp_state: None,
            }
        })
        .collect())
}
