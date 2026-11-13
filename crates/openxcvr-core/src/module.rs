use crate::error::ModuleError;
use crate::types::{Application, ChecksumStatus, LaneStatus, ModuleFlags, ModuleState, VendorInfo};

/// The canonical L2 module surface: one transceiver, read through the
/// unified vocabulary (per), independently of the governing spec.
/// Per-spec adapters implement it over any
/// [`Transport`](crate::transport::Transport); readers never see the spec's
/// native byte locations.
///
/// Every read is caller-directed and stateless beyond the implementor's own
/// handle: the trait performs no I/O of its own initiative and holds no
/// clocks or resident state. Optional capabilities are expressed as default
/// methods returning [`ModuleError::Unsupported`], so a family that lacks a
/// capability reports it instead of guessing.
pub trait Module {
    /// Vendor identity block (name, part number, hardware revision, serial,
    /// date code, OUI), decoded from the spec's identity byte ranges.
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError>;
    /// The SFF-8024 identifier byte (byte 0 of the base/lower page).
    fn identifier(&self) -> Result<u8, ModuleError>;
    /// Case temperature in °C, when the module implements the monitor.
    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError>;
    /// Supply voltage in volts, when the module implements the monitor.
    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError>;
    /// Number of lanes the module exposes.
    fn lane_count(&self) -> Result<u8, ModuleError>;
    /// Per-lane status (RX/TX power, bias, signal flags; CMIS data-path
    /// state via [`LaneStatus::dp_state`]).
    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError>;
    /// Module-level status and alarm/warning flags.
    fn flags(&self) -> Result<ModuleFlags, ModuleError>;
    /// Checksum validation for the code blocks the spec defines.
    fn checksums(&self) -> Result<ChecksumStatus, ModuleError>;

    /// Application descriptors decoded from the spec-compliance bytes.
    /// Defaults to [`ModuleError::Unsupported`] for families without the
    /// concept.
    fn applications(&self) -> Result<Vec<Application>, ModuleError> {
        Err(ModuleError::Unsupported)
    }
    /// Synthesized module operational state (status, data-not-ready,
    /// interrupt, temp/VCC alarms and warnings). Defaults to
    /// [`ModuleError::Unsupported`] for families without status bytes.
    fn module_state(&self) -> Result<ModuleState, ModuleError> {
        Err(ModuleError::Unsupported)
    }
}
