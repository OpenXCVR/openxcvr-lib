use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE};

use crate::module::CmisTransport;

const PAGE_04H: u8 = 0x04;
const PAGE_12H: u8 = 0x12;
// Page 04h is read-only capability advertisement: GridSupported<rate> flags
// live in byte 128 (CMIS 5.3 § 8.7); the writable controls are per-lane on
// Page 12h.
const CAPABILITY_OFFSET: u8 = 128;
const GRID_OFFSET: u8 = 128;
const CHANNEL_OFFSET: u8 = 136;
const FINE_TUNING_OFFSET: u8 = 152;
const CURRENT_FREQUENCY_OFFSET: u8 = 168;

/// The tunable laser's channel grid spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridSpacing {
    /// 100 GHz spacing.
    Ghz100,
    /// 50 GHz spacing.
    Ghz50,
    /// 75 GHz spacing.
    Ghz75,
}

/// Why a tunable-laser operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum TunableLaserError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The requested frequency does not land on the selected grid.
    InvalidFrequency,
    /// The requested channel number is outside the advertised range.
    InvalidChannel,
}

impl fmt::Display for TunableLaserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "tunable laser transport error: {err}"),
            Self::InvalidFrequency => write!(f, "tunable laser frequency is out of range"),
            Self::InvalidChannel => write!(f, "tunable laser channel is out of range"),
        }
    }
}

impl Error for TunableLaserError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ModuleError> for TunableLaserError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// Retune the laser to a frequency on the advertised grid (THz).
pub fn set_frequency<T: CmisTransport>(
    transport: &mut T,
    grid: GridSpacing,
    freq_thz: f64,
) -> Result<(), TunableLaserError> {
    if !(186.0..=201.0).contains(&freq_thz) {
        return Err(TunableLaserError::InvalidFrequency);
    }

    let step_ghz = grid.step_ghz();
    let channel = (((freq_thz - 193.1) * 1000.0) / step_ghz).round() as i32 + 1;
    if !(1..=4096).contains(&channel) {
        return Err(TunableLaserError::InvalidChannel);
    }

    let channel = channel as i16;
    transport.write(
        I2C_ADDR_MODULE,
        PAGE_12H,
        0,
        GRID_OFFSET,
        &[grid.code() << 4],
    )?;
    transport.write(
        I2C_ADDR_MODULE,
        PAGE_12H,
        0,
        CHANNEL_OFFSET,
        &channel.to_be_bytes(),
    )?;
    transport.write(I2C_ADDR_MODULE, PAGE_12H, 0, FINE_TUNING_OFFSET, &[0, 0])?;

    Ok(())
}

/// Read the laser's current frequency (THz, decoded from bytes 168-170).
pub fn current_frequency<T: CmisTransport>(transport: &mut T) -> Result<f64, TunableLaserError> {
    let mut frequency = [0u8; 4];
    transport.read(
        I2C_ADDR_MODULE,
        PAGE_12H,
        0,
        CURRENT_FREQUENCY_OFFSET,
        &mut frequency,
    )?;
    Ok(u32::from_be_bytes(frequency) as f64 / 1_000_000.0)
}

/// Read the grid spacings the module advertises (Page 01h byte 128).
pub fn supported_grids<T: CmisTransport>(
    transport: &mut T,
) -> Result<Vec<GridSpacing>, TunableLaserError> {
    let mut capability = [0u8; 1];
    transport.read(
        I2C_ADDR_MODULE,
        PAGE_04H,
        0,
        CAPABILITY_OFFSET,
        &mut capability,
    )?;

    // GridSupported flags in Page 04h byte 128: bit 7 = 75 GHz, bit 5 =
    // 100 GHz, bit 4 = 50 GHz.
    let mut grids = Vec::new();
    if capability[0] & (1 << 5) != 0 {
        grids.push(GridSpacing::Ghz100);
    }
    if capability[0] & (1 << 4) != 0 {
        grids.push(GridSpacing::Ghz50);
    }
    if capability[0] & (1 << 7) != 0 {
        grids.push(GridSpacing::Ghz75);
    }
    Ok(grids)
}

impl GridSpacing {
    fn code(self) -> u8 {
        match self {
            Self::Ghz100 => 0x05,
            Self::Ghz50 => 0x04,
            Self::Ghz75 => 0x07,
        }
    }

    fn step_ghz(self) -> f64 {
        match self {
            Self::Ghz100 => 100.0,
            Self::Ghz50 => 50.0,
            Self::Ghz75 => 75.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{
        error::ModuleError,
        transport::{Transport, I2C_ADDR_MODULE},
    };

    struct MockTransport {
        page_04h: [u8; 256],
        page_12h: [u8; 256],
    }

    impl Default for MockTransport {
        fn default() -> Self {
            Self {
                page_04h: [0u8; 256],
                page_12h: [0u8; 256],
            }
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
            assert_eq!(bank, 0);
            let start = offset as usize;
            let source = if page == PAGE_04H {
                &self.page_04h
            } else {
                assert_eq!(page, PAGE_12H);
                &self.page_12h
            };
            buf.copy_from_slice(&source[start..start + buf.len()]);
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
            assert_eq!(page, PAGE_12H);
            assert_eq!(bank, 0);

            let start = offset as usize;
            self.page_12h[start..start + data.len()].copy_from_slice(data);
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    #[test]
    fn test_set_frequency_100g_grid_writes_channel() {
        let mut transport = MockTransport::default();

        set_frequency(&mut transport, GridSpacing::Ghz100, 193.2).unwrap();

        assert_eq!(transport.page_12h[128], 0x50);
        assert_eq!(&transport.page_12h[136..138], &[0x00, 0x02]);
        assert_eq!(&transport.page_12h[152..154], &[0, 0]);
    }

    #[test]
    fn test_set_frequency_rejects_out_of_range() {
        let mut transport = MockTransport::default();

        assert!(matches!(
            set_frequency(&mut transport, GridSpacing::Ghz100, 185.9),
            Err(TunableLaserError::InvalidFrequency)
        ));
    }

    #[test]
    fn test_supported_grids_reads_page_04h_capability_byte() {
        let mut transport = MockTransport::default();
        // Page 04h:128 bits: 7=75GHz, 5=100GHz, 4=50GHz (bit 0 = 3.125GHz is
        // not representable in this enum and must be ignored).
        transport.page_04h[128] = (1 << 7) | (1 << 5) | (1 << 4) | 1;

        assert_eq!(
            supported_grids(&mut transport).unwrap(),
            vec![GridSpacing::Ghz100, GridSpacing::Ghz50, GridSpacing::Ghz75]
        );
    }

    #[test]
    fn test_current_frequency_reads_grid_and_channel() {
        let mut transport = MockTransport::default();
        transport.page_12h[168..172].copy_from_slice(&193_200_000u32.to_be_bytes());

        assert_eq!(current_frequency(&mut transport).unwrap(), 193.2);
    }
}
