//! Demonstrate SFF-8024 byte-0 / byte-1 dispatch across all three spec families.
//!
//! Run: `cargo run --example dispatch-across-specs`
//!
//! `dispatch_identifier` is the routing function the unified API uses to decide
//! which adapter to instantiate. It takes byte 0 (SFF-8024 module identifier)
//! plus byte 1 (CMIS revision tie-break for QSFP-DD/OSFP) and returns a
//! `SpecKind` discriminant.

use openxcvr::{dispatch_identifier, SpecKind};

fn main() {
    let cases = [
        (0x03, 0x00, "SFP+ (SFF-8472 management)"),
        (0x11, 0x00, "QSFP28 (SFF-8636 management)"),
        (
            0x18,
            0x00,
            "QSFP-DD pre-CMIS legacy (SFF-8636-compatible map)",
        ),
        (0x18, 0x50, "QSFP-DD CMIS 5.0 (CMIS management)"),
        (0x19, 0x40, "OSFP CMIS 4.0"),
        (0x1B, 0x00, "DSFP (pure-CMIS)"),
        (0x22, 0x00, "OIF-ELSFP (pure-CMIS)"),
    ];

    for (byte0, byte1, label) in cases {
        match dispatch_identifier(byte0, byte1) {
            Ok(SpecKind::Sff8472) => println!("0x{byte0:02X}/0x{byte1:02X} → SFF-8472   — {label}"),
            Ok(SpecKind::Sff8636) => println!("0x{byte0:02X}/0x{byte1:02X} → SFF-8636   — {label}"),
            Ok(SpecKind::Cmis) => println!("0x{byte0:02X}/0x{byte1:02X} → CMIS       — {label}"),
            Err(e) => println!("0x{byte0:02X}/0x{byte1:02X} → unknown    — {e}"),
        }
    }
}
