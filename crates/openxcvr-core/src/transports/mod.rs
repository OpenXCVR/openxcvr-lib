//! Transport implementations gated behind opt-in features.

#[cfg(all(feature = "linux-i2c", target_os = "linux"))]
pub mod linux_i2c;

#[cfg(all(feature = "linux-i2c", target_os = "linux"))]
pub use linux_i2c::{I2cDev, LinuxI2cTransport};
