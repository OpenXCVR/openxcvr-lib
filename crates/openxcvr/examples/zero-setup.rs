//! Decode an embedded synthetic CMIS sample and print its vendor identity
//! and spec revision.
//!
//! The sample is compiled in with `include_bytes!`. Nothing is read from
//! the working directory and no hardware is touched, so this runs from any
//! directory:
//!
//! ```sh
//! cargo run -p openxcvr --example zero-setup
//! ```
//!
//! See `docs/guides/quickstart.md`. The bytes are the hand-authored synthetic
//! CMIS 5.2 QSFP-DD fixture emitted by `tools/build-cmis-5-2-synthetic.py`,
//! not a vendor capture. An unknown revision fails closed inside
//! `CmisModule::from_bytes`; this example does not guess one.

#[path = "support/sample.rs"]
mod sample;

use openxcvr::{CmisModule, Module};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let capture = sample::Capture::embedded();
    let module = CmisModule::from_bytes(&capture.flat_eeprom()?)?;
    let info = module.vendor_info()?;
    let rev = module.detected_rev();
    println!("source: {}", capture.source);
    println!("vendor: {}", info.name);
    println!("part: {}", info.part_number);
    println!("serial: {}", info.serial_number);
    println!("vendor rev: {}", info.rev);
    println!(
        "spec revision: CMIS {} (lower byte 1 = 0x{:02X})",
        rev.as_str(),
        module.cmis_revision()?
    );
    println!("identifier: 0x{:02X}", module.identifier()?);
    Ok(())
}
