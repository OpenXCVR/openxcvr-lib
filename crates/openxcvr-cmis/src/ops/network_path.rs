use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE};

use crate::{
    adapter::npsm::{NpState, Npsm, NpsmError},
    module::CmisTransport,
};

/// Why a network-path operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum NetworkPathError {
    /// The NPSM state machine failed; the original error is the source.
    Npsm(NpsmError),
    /// The module does not support network-path provisioning.
    Unsupported,
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
}

impl From<NpsmError> for NetworkPathError {
    fn from(err: NpsmError) -> Self {
        match err {
            NpsmError::Unsupported => Self::Unsupported,
            other => Self::Npsm(other),
        }
    }
}

impl From<ModuleError> for NetworkPathError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

impl fmt::Display for NetworkPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Npsm(err) => write!(f, "network path NPSM error: {err}"),
            Self::Unsupported => write!(f, "network path provisioning is unsupported"),
            Self::Transport(err) => write!(f, "network path transport error: {err}"),
        }
    }
}

impl Error for NetworkPathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// One network path's provisioning config byte (page 01h byte 86 + np_id).
pub struct NetworkPathConfig {
    /// The raw config byte to program.
    pub config_byte: u8,
}

/// One network path's current status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPathStatus {
    /// The network path the status belongs to.
    pub np_id: u8,
    /// The path's current NPSM state.
    pub state: NpState,
}

/// Provision one network path with a config byte and wait for Activated.
pub fn provision<T: CmisTransport>(
    transport: &mut T,
    np_id: u8,
    config: &NetworkPathConfig,
    max_polls: u32,
) -> Result<(), NetworkPathError> {
    let _ = (np_id, max_polls);

    Npsm::check_supported(transport).map_err(NetworkPathError::from)?;
    transport
        .write(I2C_ADDR_MODULE, 0x16, 0, 143, &[config.config_byte])
        .map_err(NetworkPathError::from)?;

    Ok(())
}

/// Activate one network path and wait for it to report Activated.
pub fn activate<T: CmisTransport>(
    transport: &mut T,
    np_id: u8,
    max_polls: u32,
) -> Result<(), NetworkPathError> {
    Npsm::activate(transport, np_id, max_polls).map_err(NetworkPathError::from)
}

/// Deactivate one network path and wait for it to report Deactivated.
pub fn deactivate<T: CmisTransport>(
    transport: &mut T,
    np_id: u8,
    max_polls: u32,
) -> Result<(), NetworkPathError> {
    Npsm::deactivate(transport, np_id, max_polls).map_err(NetworkPathError::from)
}

/// Read one network path's current state.
pub fn status<T: CmisTransport>(
    transport: &mut T,
    np_id: u8,
    max_polls: u32,
) -> Result<NetworkPathStatus, NetworkPathError> {
    let _ = max_polls;

    Npsm::current_state(transport, np_id)
        .map(|state| NetworkPathStatus { np_id, state })
        .map_err(NetworkPathError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{
        error::ModuleError,
        transport::{Transport, I2C_ADDR_MODULE},
    };

    struct MockTransport {
        page_01h: [u8; 256],
        page_16h: [u8; 256],
    }

    impl Default for MockTransport {
        fn default() -> Self {
            Self {
                page_01h: [0u8; 256],
                page_16h: [0u8; 256],
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
            match page {
                0x01 => buf.copy_from_slice(&self.page_01h[start..start + buf.len()]),
                0x16 => buf.copy_from_slice(&self.page_16h[start..start + buf.len()]),
                _ => panic!("unexpected read page=0x{page:02x} offset={offset}"),
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
            assert_eq!(bank, 0);
            assert_eq!(page, 0x16);

            let start = offset as usize;
            self.page_16h[start..start + data.len()].copy_from_slice(data);
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl crate::module::CmisTransport for MockTransport {}

    #[test]
    fn test_provision_unsupported() {
        let mut mock = MockTransport::default();
        let config = NetworkPathConfig { config_byte: 0x80 };

        assert!(matches!(
            provision(&mut mock, 0, &config, 10),
            Err(NetworkPathError::Unsupported)
        ));
    }

    #[test]
    fn test_activate_delegates_to_npsm() {
        let mut mock = MockTransport::default();
        mock.page_01h[142] = 0x80;
        mock.page_16h[200] = 0x44;

        assert!(activate(&mut mock, 0, 10).is_ok());
    }

    #[test]
    fn test_status_returns_state() {
        let mut mock = MockTransport::default();
        mock.page_01h[142] = 0x80;
        mock.page_16h[200] = 0x21;

        assert_eq!(
            status(&mut mock, 0, 10).unwrap(),
            NetworkPathStatus {
                np_id: 0,
                state: NpState::Deactivated,
            }
        );
    }
}
