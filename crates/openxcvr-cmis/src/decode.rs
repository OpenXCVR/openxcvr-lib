/// Decode CMIS 5.3 F16, a decimal floating-point format encoded as
/// `m * 10^(s - 24)` with a 5-bit exponent `s` and 11-bit unsigned mantissa.
pub fn cmis_f16be(bytes: [u8; 2]) -> f32 {
    let s = (bytes[0] >> 3) as i32;
    let m = (((bytes[0] & 0x07) as u16) << 8) | bytes[1] as u16;
    (m as f32) * 10f32.powi(s - 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_zero() {
        assert_eq!(cmis_f16be([0x00, 0x00]), 0.0);
    }

    #[test]
    fn decodes_unit_value() {
        assert_eq!(cmis_f16be([0xc0, 0x01]), 1.0);
    }

    #[test]
    fn decodes_decimal_scaled_value() {
        let decoded = cmis_f16be([0xab, 0xe8]);
        assert!((decoded - 1.0).abs() < 0.000_001);
    }
}
