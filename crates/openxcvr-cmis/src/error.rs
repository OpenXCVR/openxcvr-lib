use std::{error::Error, fmt};

use crate::adapter::{CdbError, DpsmError, MsmError, NpsmError};

/// The CMIS adapter's unified error type.
#[derive(Debug)]
#[non_exhaustive]
pub enum CmisError {
    /// A core transport/observation error.
    Core(openxcvr_core::error::ModuleError),
    /// A CDB framing error.
    Cdb(CdbError),
    /// A data-path state-machine error.
    Dpsm(DpsmError),
    /// A module state-machine error.
    Msm(MsmError),
    /// A network-path state-machine error.
    Npsm(NpsmError),
    /// The request is outside the current version's delivered scope.
    NotInV01Scope,
}

impl From<openxcvr_core::error::ModuleError> for CmisError {
    fn from(err: openxcvr_core::error::ModuleError) -> Self {
        Self::Core(err)
    }
}

impl From<CdbError> for CmisError {
    fn from(err: CdbError) -> Self {
        Self::Cdb(err)
    }
}

impl From<DpsmError> for CmisError {
    fn from(err: DpsmError) -> Self {
        Self::Dpsm(err)
    }
}

impl From<MsmError> for CmisError {
    fn from(err: MsmError) -> Self {
        Self::Msm(err)
    }
}

impl From<NpsmError> for CmisError {
    fn from(err: NpsmError) -> Self {
        Self::Npsm(err)
    }
}

impl fmt::Display for CmisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(err) => write!(f, "{err}"),
            Self::Cdb(err) => write!(f, "{err}"),
            Self::Dpsm(err) => write!(f, "{err}"),
            Self::Msm(err) => write!(f, "{err}"),
            Self::Npsm(err) => write!(f, "{err}"),
            Self::NotInV01Scope => write!(f, "field not in CMIS v0.1 scope"),
        }
    }
}

impl Error for CmisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Core(err) => Some(err),
            Self::Cdb(err) => Some(err),
            Self::Dpsm(err) => Some(err),
            Self::Msm(err) => Some(err),
            Self::Npsm(err) => Some(err),
            Self::NotInV01Scope => None,
        }
    }
}
