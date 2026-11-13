//! Pure threshold evaluation for SFF-8472: the calibrated base monitors
//! against their A2h Table 9-5 threshold sets, through
//! `openxcvr_core::threshold::evaluate`.

use openxcvr_core::error::ModuleError;
use openxcvr_core::transport::Transport;
use openxcvr_sff8472::{adapter::Sff8472Module, mmap::Sff8472MmapTransport};
use std::cell::RefCell;
use std::rc::Rc;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/sff-8472")
        .join(name)
}

/// In-memory transport whose pages the test retains a handle to.
#[derive(Clone)]
struct SharedTransport {
    pages: Rc<RefCell<([u8; 256], [u8; 256])>>, // (A0h, A2h)
}

impl SharedTransport {
    fn from_fixture(a0h: &str, a2h: &str) -> Self {
        let a0 = std::fs::read(fixture(a0h)).unwrap();
        let a2 = std::fs::read(fixture(a2h)).unwrap();
        let mut pages = ([0u8; 256], [0u8; 256]);
        assert!(a0.len() <= 256 && a2.len() == 256);
        pages.0[..a0.len()].copy_from_slice(&a0);
        pages.1.copy_from_slice(&a2);
        Self {
            pages: Rc::new(RefCell::new(pages)),
        }
    }
}

impl Transport for SharedTransport {
    fn read(
        &mut self,
        address: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> Result<(), ModuleError> {
        let pages = self.pages.borrow();
        let src: &[u8] = match address {
            0xA0 => &pages.0[offset as usize..],
            0xA2 => &pages.1[offset as usize..],
            _ => return Err(ModuleError::Io("bad address".into())),
        };
        out.copy_from_slice(&src[..out.len()]);
        Ok(())
    }
    fn write(
        &mut self,
        address: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), ModuleError> {
        let mut pages = self.pages.borrow_mut();
        let dst: &mut [u8] = match address {
            0xA0 => &mut pages.0[offset as usize..],
            0xA2 => &mut pages.1[offset as usize..],
            _ => return Err(ModuleError::Io("bad address".into())),
        };
        dst[..data.len()].copy_from_slice(data);
        Ok(())
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}

/// The Finisar capture carries real threshold sets (A2h bytes 0-39) and
/// monitor readings; its received power is 0.0 mW with a non-zero RX
/// low-alarm/low-warning bound, so RX power must evaluate as a genuine
/// low alarm while every other monitor stays within bounds.
#[test]
fn evaluates_the_finisar_capture_against_its_threshold_sets() {
    let module = Sff8472Module::open(SharedTransport::from_fixture(
        "finisar-ftlx8571d3bcl-mup0wb0.a0h.bin",
        "finisar-ftlx8571d3bcl-mup0wb0.a2h.bin",
    ))
    .unwrap();
    let e = module.evaluate_thresholds().unwrap();

    // temp 10.10 °C vs bounds 78/-13/73/-8: within.
    assert!(e.temperature.is_within_bounds(), "temp {e:?}");
    // vcc 3.3162 V vs 3.7/2.9/3.6/3.0: within.
    assert!(e.supply_voltage.is_within_bounds(), "vcc {e:?}");
    // bias 7.176 mA vs 13.2/4.0/12.6/5.0: within.
    assert!(e.tx_bias.is_within_bounds(), "bias {e:?}");
    // tx power 0.5846 mW vs 1.0/0.2512/0.7943/0.3162: within.
    assert!(e.tx_power.is_within_bounds(), "tx {e:?}");
    // rx power 0.0 mW vs 1.0/0.01/0.7943/0.0158: low alarm AND low warning.
    assert!(e.rx_power.low_alarm, "rx low alarm must trip at 0 mW");
    assert!(e.rx_power.low_warning, "rx low warning must trip at 0 mW");
    assert!(!e.rx_power.high_alarm);
    assert!(!e.rx_power.high_warning);
}

/// The synthetic captures leave every threshold byte zero (the
/// not-implemented encoding): nothing may trip, and in particular no
/// zero high-alarm bound may spuriously alarm a zero reading.
#[test]
fn zero_threshold_sets_disable_the_decisions() {
    for name in ["synthetic-extcal-sfp10g-sr", "synthetic-sfp28-25g-sr"] {
        let module = Sff8472Module::open(SharedTransport::from_fixture(
            &format!("{name}.a0h.bin"),
            &format!("{name}.a2h.bin"),
        ))
        .unwrap();
        let e = module.evaluate_thresholds().unwrap();
        assert!(e.temperature.is_within_bounds(), "{name} temp {e:?}");
        assert!(e.supply_voltage.is_within_bounds(), "{name} vcc {e:?}");
        assert!(e.tx_bias.is_within_bounds(), "{name} bias {e:?}");
        assert!(e.tx_power.is_within_bounds(), "{name} tx {e:?}");
        assert!(e.rx_power.is_within_bounds(), "{name} rx {e:?}");
    }
}

/// The decision is pure: the same capture evaluates identically through
/// the mmap transport and the shared transport, detected or not.
#[test]
fn evaluation_is_independent_of_the_transport_and_detection() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let t = Sff8472MmapTransport::from_files(
        root.join("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a0h.bin"),
        root.join("tests/fixtures/sff-8472/finisar-ftlx8571d3bcl-mup0wb0.a2h.bin"),
    )
    .unwrap();
    let undetected = Sff8472Module::new(t).evaluate_thresholds().unwrap();
    let detected = Sff8472Module::open(SharedTransport::from_fixture(
        "finisar-ftlx8571d3bcl-mup0wb0.a0h.bin",
        "finisar-ftlx8571d3bcl-mup0wb0.a2h.bin",
    ))
    .unwrap()
    .evaluate_thresholds()
    .unwrap();
    assert_eq!(undetected, detected);
}
