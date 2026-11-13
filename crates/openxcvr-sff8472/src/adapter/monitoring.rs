use super::calibration::{apply_linear, cal_mode, linear_constants, CalMode};
use openxcvr_core::{error::ModuleError, transport::Transport};

pub(super) fn temperature_celsius(t: &mut dyn Transport) -> Result<Option<f32>, ModuleError> {
    let mut buf = [0u8; 2];
    t.read(0xA2, 0, 0, 96, &mut buf)?;
    let raw = i16::from_be_bytes(buf);
    match cal_mode(t)? {
        CalMode::Internal => Ok(Some(raw as f32 / 256.0)),
        CalMode::External => {
            let c = linear_constants(t)?;
            // Temperature raw and offset are signed and the calibrated result
            // remains in 1/256 degree-C parameter LSBs.
            let calibrated_lsb = (c[2].0 as f32 / 256.0) * raw as f32 + c[2].1 as f32;
            Ok(Some(calibrated_lsb / 256.0))
        }
    }
}

pub(super) fn supply_voltage_v(t: &mut dyn Transport) -> Result<Option<f32>, ModuleError> {
    let mut buf = [0u8; 2];
    t.read(0xA2, 0, 0, 98, &mut buf)?;
    let raw = u16::from_be_bytes(buf);
    match cal_mode(t)? {
        CalMode::Internal => Ok(Some(raw as f32 * 0.0001)),
        CalMode::External => {
            let c = linear_constants(t)?;
            Ok(Some(apply_linear(raw, c[3].0, c[3].1) * 0.0001))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::temperature_celsius;
    use openxcvr_core::{error::ModuleError, transport::Transport};

    struct MemoryTransport {
        a0: [u8; 256],
        a2: [u8; 256],
    }

    impl Transport for MemoryTransport {
        fn read(
            &mut self,
            address: u8,
            _page: u8,
            _bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            let memory = match address {
                0xA0 => &self.a0,
                0xA2 => &self.a2,
                _ => return Err(ModuleError::Io("bad address".into())),
            };
            buf.copy_from_slice(&memory[offset as usize..offset as usize + buf.len()]);
            Ok(())
        }
        fn write(
            &mut self,
            _address: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            _data: &[u8],
        ) -> Result<(), ModuleError> {
            unreachable!()
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    #[test]
    fn externally_calibrated_negative_temperature_uses_parameter_lsb_units() {
        let mut transport = MemoryTransport {
            a0: [0; 256],
            a2: [0; 256],
        };
        transport.a0[92] = 0x10;
        transport.a2[96..98].copy_from_slice(&(-512i16).to_be_bytes());
        transport.a2[84..86].copy_from_slice(&0x0100u16.to_be_bytes());
        transport.a2[86..88].copy_from_slice(&(-256i16).to_be_bytes());
        assert_eq!(temperature_celsius(&mut transport).unwrap(), Some(-3.0));
    }
}
