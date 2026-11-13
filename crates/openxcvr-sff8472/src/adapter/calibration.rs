use openxcvr_core::{error::ModuleError, transport::Transport};

pub(super) enum CalMode {
    Internal,
    External,
}

pub(super) fn cal_mode(t: &mut dyn Transport) -> Result<CalMode, ModuleError> {
    let mut buf = [0u8; 1];
    t.read(0xA0, 0, 0, 92, &mut buf)?;
    if buf[0] & 0x20 != 0 {
        Ok(CalMode::Internal)
    } else if buf[0] & 0x10 != 0 {
        Ok(CalMode::External)
    } else {
        Err(ModuleError::Decode(
            "A0h:92 - neither internal nor external calibration set".into(),
        ))
    }
}

/// Returns (slope_u16, offset_i16) pairs:
/// [0]=tx_bias, [1]=tx_power, [2]=temp, [3]=vcc
/// Layout: 4 pairs x 4 bytes = 16 bytes at A2h:76-91.
/// Each pair: slope (u16 BE 8.8 fixed-point) at [i*4, i*4+1]; offset (i16 BE) at [i*4+2, i*4+3].
pub(super) fn linear_constants(t: &mut dyn Transport) -> Result<[(u16, i16); 4], ModuleError> {
    let mut buf = [0u8; 16];
    t.read(0xA2, 0, 0, 76, &mut buf)?;
    let mut out = [(0u16, 0i16); 4];
    for i in 0..4 {
        let slope = u16::from_be_bytes([buf[i * 4], buf[i * 4 + 1]]);
        let offset = i16::from_be_bytes([buf[i * 4 + 2], buf[i * 4 + 3]]);
        out[i] = (slope, offset);
    }
    Ok(out)
}

pub(super) fn apply_linear(raw: u16, slope: u16, offset: i16) -> f32 {
    (slope as f32 / 256.0) * raw as f32 + offset as f32
}

/// Rx power polynomial: A2h bytes 56-75 (5 x 4-byte IEEE 754 f32 BE)
/// coefficients[0] = degree 4 term, coefficients[4] = degree 0 term.
pub(super) fn rx_power_polynomial(t: &mut dyn Transport, raw: u16) -> Result<f32, ModuleError> {
    let mut buf = [0u8; 20];
    t.read(0xA2, 0, 0, 56, &mut buf)?;
    let coeffs: [f32; 5] = std::array::from_fn(|i| {
        f32::from_be_bytes([buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]])
    });
    let r = raw as f32;
    Ok(coeffs[0] * r * r * r * r
        + coeffs[1] * r * r * r
        + coeffs[2] * r * r
        + coeffs[3] * r
        + coeffs[4])
}
