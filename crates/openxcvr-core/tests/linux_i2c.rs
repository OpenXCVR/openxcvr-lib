//! Integration tests for [`LinuxI2cTransport`] against the Linux
//! `i2c-stub` kernel module.
//!
//! All tests are `#[ignore]` because they require:
//!
//! 1. Running on Linux with `/dev/i2c-N` character devices.
//! 2. The `i2c-stub` kernel module loaded with at least one chip
//!    address. Single-addr tests need `0x50`; the dual-address test
//!    additionally needs `0x51` (= 7-bit form of SFF-8472 `A2h`).
//! 3. The `LINUX_I2C_BUS` environment variable pointing at the
//!    i2c-stub bus (e.g. `/dev/i2c-12`).
//! 4. The `LINUX_I2C_DUAL_ADDR=1` environment variable for the
//!    dual-address test (which requires `chip_addr=0x50,0x51`).
//!
//! ## Running locally
//!
//! ```bash
//! sudo modprobe i2c-stub chip_addr=0x50,0x51
//! # find the bus number that i2c-stub created:
//! i2cdetect -l | grep stub
//! # → assume `i2c-12` for this example
//! LINUX_I2C_BUS=/dev/i2c-12 LINUX_I2C_DUAL_ADDR=1 \
//!     cargo test --features linux-i2c -p openxcvr-core -- --ignored
//! ```
//!
//! In default `cargo test` runs (no `--ignored`), every test in this
//! file is skipped, so the absence of i2c-stub is not a build break.

#![cfg(all(feature = "linux-i2c", target_os = "linux"))]

use openxcvr_core::transport::{
    Transport, I2C_ADDR_MODULE, I2C_ADDR_SFF8472_BASE, I2C_ADDR_SFF8472_DIAG,
};
use openxcvr_core::transports::LinuxI2cTransport;

/// Read `LINUX_I2C_BUS` and skip with a clear notice if unset. Returning
/// `None` lets each test no-op gracefully when the var is absent — the
/// test still "passes" because `--ignored` runs are best-effort against
/// whatever local kernel state exists.
fn bus_path_or_skip() -> Option<String> {
    match std::env::var("LINUX_I2C_BUS") {
        Ok(path) => Some(path),
        Err(_) => {
            eprintln!("skipping: set LINUX_I2C_BUS=/dev/i2c-N (i2c-stub bus) to run this test");
            None
        }
    }
}

#[test]
#[ignore]
fn round_trip_single_byte_at_module_addr() {
    let Some(bus) = bus_path_or_skip() else {
        return;
    };
    let mut t = LinuxI2cTransport::new(&bus).expect("open i2c bus");

    let written = [0xDEu8];
    t.write(I2C_ADDR_MODULE, 0, 0, 0, &written)
        .expect("write byte 0");

    let mut buf = [0u8; 1];
    t.read(I2C_ADDR_MODULE, 0, 0, 0, &mut buf)
        .expect("read byte 0");

    assert_eq!(buf, written, "round-trip mismatch at module addr");
}

#[test]
#[ignore]
fn round_trip_four_byte_pattern() {
    let Some(bus) = bus_path_or_skip() else {
        return;
    };
    let mut t = LinuxI2cTransport::new(&bus).expect("open i2c bus");

    let pattern = [0xDEu8, 0xAD, 0xBE, 0xEF];
    t.write(I2C_ADDR_MODULE, 0, 0, 0x10, &pattern)
        .expect("write 4-byte pattern");

    let mut buf = [0u8; 4];
    t.read(I2C_ADDR_MODULE, 0, 0, 0x10, &mut buf)
        .expect("read 4-byte pattern");

    assert_eq!(buf, pattern, "round-trip mismatch for 4-byte pattern");
}

#[test]
#[ignore]
fn round_trip_chunked_read_across_smbus_cap() {
    // 64 bytes forces two 32-byte SMBus block-read transactions, exercising
    // the inter-chunk TBUF_STANDARD sleep path.
    let Some(bus) = bus_path_or_skip() else {
        return;
    };
    let mut t = LinuxI2cTransport::new(&bus).expect("open i2c bus");

    let pattern: Vec<u8> = (0u8..64).collect();
    t.write(I2C_ADDR_MODULE, 0, 0, 0, &pattern)
        .expect("write 64-byte pattern");

    let mut buf = [0u8; 64];
    t.read(I2C_ADDR_MODULE, 0, 0, 0, &mut buf)
        .expect("read 64-byte pattern");

    assert_eq!(&buf[..], &pattern[..], "chunked-read mismatch");
}

#[test]
#[ignore]
fn dual_address_a0_a2_round_trip() {
    // i2c-stub keeps separate per-chip-addr buffers, so writing distinct
    // patterns at A0h and A2h then reading them back proves slave-addr
    // selection actually re-aims the I2C_SLAVE ioctl.
    if std::env::var("LINUX_I2C_DUAL_ADDR").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping: set LINUX_I2C_DUAL_ADDR=1 (and modprobe i2c-stub with chip_addr=0x50,0x51)"
        );
        return;
    }
    let Some(bus) = bus_path_or_skip() else {
        return;
    };
    let mut t = LinuxI2cTransport::new(&bus).expect("open i2c bus");

    let pat_a0 = [0xA0u8, 0x11, 0x22, 0x33];
    let pat_a2 = [0xA2u8, 0x44, 0x55, 0x66];

    t.write(I2C_ADDR_SFF8472_BASE, 0, 0, 0, &pat_a0)
        .expect("write A0h pattern");
    t.write(I2C_ADDR_SFF8472_DIAG, 0, 0, 0, &pat_a2)
        .expect("write A2h pattern");

    let mut buf_a0 = [0u8; 4];
    let mut buf_a2 = [0u8; 4];
    t.read(I2C_ADDR_SFF8472_BASE, 0, 0, 0, &mut buf_a0)
        .expect("read A0h pattern");
    t.read(I2C_ADDR_SFF8472_DIAG, 0, 0, 0, &mut buf_a2)
        .expect("read A2h pattern");

    assert_eq!(buf_a0, pat_a0, "A0h slave-addr round-trip mismatch");
    assert_eq!(buf_a2, pat_a2, "A2h slave-addr round-trip mismatch");
}

#[test]
#[ignore]
fn probe_succeeds_at_module_addr() {
    let Some(bus) = bus_path_or_skip() else {
        return;
    };
    let mut t = LinuxI2cTransport::new(&bus).expect("open i2c bus");
    t.probe()
        .expect("probe should succeed against i2c-stub at 0x50");
}
