//! Quickstart example demonstrating UnifiedModule usage on transceiver fixtures.
//!
//! Run from the repository root (uses default fixture paths):
//! ```sh
//! cargo run -p openxcvr --example quickstart
//! ```
//! Or pass custom A0h and A2h fixture paths (e.g. when run from another directory):
//! ```sh
//! cargo run -p openxcvr --example quickstart -- <path/to/a0h.bin> <path/to/a2h.bin>
//! ```

use openxcvr::{dispatch_identifier, Module, SpecKind, UnifiedModule};
use std::env;
use std::path::{Path, PathBuf};

fn resolve_paths() -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.len() {
        0 => {
            // Default: repo-root-relative fixture paths
            let a0h =
                PathBuf::from("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin");
            let a2h =
                PathBuf::from("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin");
            if !a0h.exists() {
                return Err(format!(
                    "Default fixture file not found at '{}'.\n\
                     Note: default paths are relative to the repository root.\n\
                     Run from repo root: `cargo run -p openxcvr --example quickstart`\n\
                     Or pass paths: `cargo run -p openxcvr --example quickstart -- <a0h.bin> <a2h.bin>`",
                    a0h.display()
                )
                .into());
            }
            Ok((a0h, a2h))
        }
        1 => {
            let p = Path::new(&args[0]);
            if p.is_dir() {
                let a0h = p.join("finisar-ftlx8571d3bcl-mup0wb0.a0h.bin");
                let a2h = p.join("finisar-ftlx8571d3bcl-mup0wb0.a2h.bin");
                Ok((a0h, a2h))
            } else {
                let s = args[0].as_str();
                if let Some(stem) = s.strip_suffix(".a0h.bin") {
                    let a2h = format!("{stem}.a2h.bin");
                    Ok((PathBuf::from(s), PathBuf::from(a2h)))
                } else {
                    Err("Single argument must be a directory or end in '.a0h.bin'. \
                         Otherwise pass both <a0h> and <a2h> fixture paths."
                        .into())
                }
            }
        }
        2 => Ok((PathBuf::from(&args[0]), PathBuf::from(&args[1]))),
        _ => Err("Usage: quickstart [<a0h.bin> <a2h.bin> | <fixtures-dir>]".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (a0h_path, a2h_path) = resolve_paths()?;

    let sff8472 = UnifiedModule::open_sff8472_mmap(&a0h_path, &a2h_path)?;
    let vendor = sff8472.vendor_info()?;
    println!("Opened SFF-8472 module from {}", a0h_path.display());
    println!("  Vendor: {}", vendor.name.trim_end());
    println!("  Part:   {}", vendor.part_number.trim_end());
    println!("  Serial: {}", vendor.serial_number.trim_end());

    // Also verify SFF-8636 fixture if running from repo root where it exists
    let sff8636_lower = Path::new("tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.lower.bin");
    if sff8636_lower.exists() {
        let sff8636 = UnifiedModule::open_sff8636_mmap(
            sff8636_lower,
            "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-00h.bin",
            "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-01h.bin",
            "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-02h.bin",
            "tests/fixtures/sff-8636/finisar-ftl410qe3c-40g-sr4.page-03h.bin",
        )?;
        let vendor8636 = sff8636.vendor_info()?;
        println!("Opened SFF-8636 module from {}", sff8636_lower.display());
        println!("  Vendor: {}", vendor8636.name.trim_end());
        println!("  Part:   {}", vendor8636.part_number.trim_end());
        println!("  Serial: {}", vendor8636.serial_number.trim_end());
    }

    // Demonstrate SFF-8024 identifier routing
    assert_eq!(dispatch_identifier(0x03, 0x00)?, SpecKind::Sff8472);
    assert_eq!(dispatch_identifier(0x11, 0x00)?, SpecKind::Sff8636);
    assert_eq!(dispatch_identifier(0x18, 0x50)?, SpecKind::Cmis);
    println!("Identifier dispatch checks passed.");

    Ok(())
}
