//! Linux `/dev/i2c-N` usage example.
//!
//! Opens a Linux i2c bus, reads byte 0 + byte 1 of the module memory at
//! the standard transceiver address (`0x50`), and dispatches to the
//! correct spec family via [`openxcvr::unified::dispatch_identifier`].
//!
//! ## Build
//!
//! ```bash
//! cargo build --example linux-i2c -p openxcvr --features linux-i2c
//! ```
//!
//! ## Run
//!
//! Set `LINUX_I2C_BUS` to the bus that the transceiver is wired to (the
//! kernel auto-creates one `/dev/i2c-N` per i2c controller).
//!
//! ```bash
//! LINUX_I2C_BUS=/dev/i2c-1 cargo run --example linux-i2c \
//!     -p openxcvr --features linux-i2c
//! ```
//!
//! ## What this demonstrates
//!
//! - Opening [`LinuxI2cTransport`] against a real bus.
//! - Reading the SFF-8024 identifier byte (lower-memory offset 0), which
//!   does NOT require page selection — the lower memory is always
//!   present regardless of the active page.
//! - Dispatching to `SpecKind` so the caller knows whether to drive the
//!   module via the SFF-8472, SFF-8636, or CMIS adapter.
//!
//! Reading vendor identity strings (which live in Upper Page 00h for
//! both SFF-8636 and CMIS) requires writing the page-select byte first
//! and then calling [`page_select_hold`] — see the SFF-8636 / SFF-8472
//! adapters for the full sequence.
//!
//! [`LinuxI2cTransport`]: openxcvr_core::transports::LinuxI2cTransport
//! [`page_select_hold`]: openxcvr_core::transports::linux_i2c::page_select_hold

#[cfg(all(feature = "linux-i2c", target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use openxcvr::unified::{dispatch_identifier, SpecKind};
    use openxcvr_core::transport::{Transport, I2C_ADDR_MODULE};
    use openxcvr_core::transports::LinuxI2cTransport;

    let bus = std::env::var("LINUX_I2C_BUS").unwrap_or_else(|_| "/dev/i2c-1".to_string());

    let mut transport = LinuxI2cTransport::new(&bus)?;
    transport.probe()?;

    // Read SFF-8024 identifier (byte 0) + extended identifier byte (byte 1).
    // Both live in lower memory which is page-independent.
    let mut id = [0u8; 2];
    transport.read(I2C_ADDR_MODULE, 0, 0, 0, &mut id)?;

    let kind = dispatch_identifier(id[0], id[1])?;
    let kind_str = match kind {
        SpecKind::Sff8472 => "SFF-8472 (SFP / SFP+ / SFP28)",
        SpecKind::Sff8636 => "SFF-8636 (QSFP+ / QSFP28 / etc.)",
        SpecKind::Cmis => "CMIS",
    };

    println!("bus:        {bus}");
    println!("byte0:      0x{:02X}", id[0]);
    println!("byte1:      0x{:02X}", id[1]);
    println!("management: {kind_str}");

    Ok(())
}

// On macOS / Windows / when the feature is off, print a friendlier
// message instead of silently building an empty binary.
#[cfg(not(all(feature = "linux-i2c", target_os = "linux")))]
fn main() {
    eprintln!(
        "this example requires `--features linux-i2c` on Linux. \
         current build excludes the LinuxI2cTransport module."
    );
}
