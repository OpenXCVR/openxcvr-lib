/// Application descriptor decoding and data-path configuration.
pub mod application;
/// Device attestation (certificate and digest signature) commands.
pub mod attestation;
/// Firmware download commands.
pub mod firmware;
/// Loopback engagement over the CDB Loopbacks command.
pub mod loopback;
/// Network-path provisioning over NPSM.
pub mod network_path;
/// RMON statistics over the CDB 0220h command.
pub mod rmon;
/// Tunable laser retuning.
pub mod tunable_laser;
/// Vendor-defined monitor (VDM) telemetry over Pages 20h-2Fh.
pub mod vdm;

pub use application::AppError;
