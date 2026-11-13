use super::lower::LowerCommon;

pub(super) fn temperature_celsius(lower: &LowerCommon) -> Option<f32> {
    Some(lower.temperature as f32 / 256.0)
}

pub(super) fn supply_voltage_v(lower: &LowerCommon) -> Option<f32> {
    Some(lower.supply_voltage as f32 * 0.0001)
}
