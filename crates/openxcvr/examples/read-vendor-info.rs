//! Open an SFF-8472 transceiver fixture and print its vendor identity.
//!
//! Run: `cargo run --example read-vendor-info`
//!
//! The fixture pair below is a Finisar FTLX8571D3BCL 10G-SR SFP+ captured by the
//! `tools/optoe-to-bin.py` converter. Replace with your own A0h/A2h `.bin` pair
//! to read a different module.

use openxcvr::{Module, UnifiedModule};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = UnifiedModule::open_sff8472_mmap(
        "tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin",
        "tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin",
    )?;

    let info = module.vendor_info()?;
    println!("vendor name : {}", info.name.trim_end());
    println!(
        "vendor OUI  : {:02X}-{:02X}-{:02X}",
        info.oui[0], info.oui[1], info.oui[2]
    );
    println!("part number : {}", info.part_number.trim_end());
    println!("serial      : {}", info.serial_number.trim_end());
    println!("date code   : {}", info.date_code.trim_end());

    if let Some(t) = module.temperature_celsius()? {
        println!("temperature : {t:.2} °C");
    }
    if let Some(v) = module.supply_voltage_v()? {
        println!("supply Vcc  : {v:.3} V");
    }

    Ok(())
}
