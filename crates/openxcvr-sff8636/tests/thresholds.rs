//! Supply-bytes threshold evaluation for SFF-8636: the Lower-memory
//! monitors evaluated against Upper Page 03h threshold sets (§ 6.6.1,
//! Table 6-28) through caller-supplied bytes — no page-select writes, on
//! fixtures (rev-detected and union paths) and synthetic in-test pages.

use openxcvr_core::rev::Rev;
use openxcvr_core::transport::Transport;
use openxcvr_sff8636::{adapter::Sff8636Module, mmap::Sff8636MmapTransport};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/sff-8636")
        .join(name)
}

fn fixture_transport(name: &str) -> Sff8636MmapTransport {
    Sff8636MmapTransport::from_files(
        fixture(&format!("{name}.lower.bin")),
        fixture(&format!("{name}.page-00h.bin")),
        fixture(&format!("{name}.page-01h.bin")),
        fixture(&format!("{name}.page-02h.bin")),
        fixture(&format!("{name}.page-03h.bin")),
    )
    .expect("fixture pages load")
}

fn page03h(name: &str) -> [u8; 128] {
    let bytes = std::fs::read(fixture(&format!("{name}.page-03h.bin"))).unwrap();
    let mut page = [0u8; 128];
    page.copy_from_slice(&bytes);
    page
}

/// Rev-detected path (Lower:1 = 0x07 → rev 2.5): the Finisar 100G SR4
/// fixture's monitors sit below its low bias/power thresholds with
/// temperature and supply well inside their bounds.
#[test]
fn evaluates_rev_detected_fixture_page_03h() {
    let t = fixture_transport("finisar-ftlc9551repm-100g-sr4");
    let module = Sff8636Module::open(t).expect("rev 2.5 detected");
    assert_eq!(module.detected_rev(), Some(Rev::SFF8636_2_5));

    let e = module
        .evaluate_thresholds(&page03h("finisar-ftlc9551repm-100g-sr4"))
        .expect("evaluation");
    // Monitors from the same fixture: temp 19.14 °C vs (75, -5, 70, 0),
    // vcc 3.2861 V vs (3.63, 2.97, 3.465, 3.135) — both within bounds.
    assert!(e.temperature.is_within_bounds());
    assert!(e.supply_voltage.is_within_bounds());
    // Rx power 0.0001 mW on every lane vs (2.1877, 0.0446, 1.7378,
    // 0.1122): below the low-alarm bound.
    for lane in e.rx_power {
        assert!(lane.low_alarm);
        assert!(lane.low_warning);
        assert!(!lane.high_alarm);
        assert!(!lane.high_warning);
    }
    // Bias 0-0.002 mA vs (15, 2, 14, 3): below the low-alarm bound.
    for lane in e.tx_bias {
        assert!(lane.low_alarm);
        assert!(lane.low_warning);
        assert!(!lane.high_alarm);
        assert!(!lane.high_warning);
    }
    // Tx power 0.0001 mW vs (1.5848, 0.0692, 0.7943, 0.1737).
    for lane in e.tx_power {
        assert!(lane.low_alarm);
        assert!(lane.low_warning);
    }
}

/// Union path (Lower:1 = 0x00, `Sff8636Module::new`): the 40G SR4
/// fixture's lanes 3-4 bias (15.224 / 18.304 mA) exceed its 15 mA
/// high-alarm bound while lanes 1-2 stay within.
#[test]
fn evaluates_union_path_fixture_page_03h() {
    let t = fixture_transport("finisar-ftl410qe3c-40g-sr4");
    let module = Sff8636Module::new(t);
    let e = module
        .evaluate_thresholds(&page03h("finisar-ftl410qe3c-40g-sr4"))
        .expect("evaluation");

    assert!(e.temperature.is_within_bounds()); // 43.36 °C
    assert!(e.supply_voltage.is_within_bounds()); // 3.2689 V
    for lane in e.rx_power {
        assert!(lane.is_within_bounds(), "rx within");
    }
    // Bias monitors (6.24-7.61 mA) vs (15, 2, 14, 3) mA: all within.
    for lane in e.tx_bias {
        assert!(lane.is_within_bounds(), "bias within");
    }
    // Tx power lane 2 (0.9152 mW) exceeds the 0.7943 mW high-warning
    // bound without reaching the 1.5848 mW high alarm; the other lanes
    // (0.736-0.7849 mW) stay within.
    for (n, lane) in e.tx_power.iter().enumerate() {
        if n == 1 {
            assert!(lane.high_warning);
            assert!(!lane.high_alarm);
            assert!(!lane.low_alarm);
        } else {
            assert!(lane.is_within_bounds(), "tx lane {} within", n + 1);
        }
    }
}

/// A module that does not implement Page 03h (the synthetic DAC fixture's
/// threshold sets are all zero) stays within bounds on every monitor —
/// never a spurious alarm from a zero high-alarm bound.
#[test]
fn all_zero_threshold_sets_stay_within_bounds() {
    let t = fixture_transport("synthetic-qsfp-dac-40g-passive-cu");
    let module = Sff8636Module::new(t);
    let e = module
        .evaluate_thresholds(&page03h("synthetic-qsfp-dac-40g-passive-cu"))
        .expect("evaluation");
    assert!(e.temperature.is_within_bounds());
    assert!(e.supply_voltage.is_within_bounds());
    for lane in e
        .rx_power
        .iter()
        .chain(e.tx_bias.iter())
        .chain(e.tx_power.iter())
    {
        assert!(lane.is_within_bounds());
    }
}

/// The synthetic decision table: hand-built Lower page and Page 03h
/// bytes, every severity direction exercised per monitor.
#[test]
fn synthetic_page_03h_decisions_are_pure_and_exact() {
    struct Flat {
        lower: [u8; 128],
    }
    impl Transport for Flat {
        fn read(
            &mut self,
            _a: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            out: &mut [u8],
        ) -> Result<(), openxcvr_core::error::ModuleError> {
            out.copy_from_slice(&self.lower[..out.len()]);
            Ok(())
        }
        fn write(
            &mut self,
            _a: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            _data: &[u8],
        ) -> Result<(), openxcvr_core::error::ModuleError> {
            Err(openxcvr_core::error::ModuleError::Io(
                "the read footprint must stay write-free".into(),
            ))
        }
        fn probe(&mut self) -> Result<(), openxcvr_core::error::ModuleError> {
            Ok(())
        }
    }

    let mut lower = [0u8; 128];
    lower[1] = 0x0A; // rev 2.12 (Table 6-4)
                     // Monitors (Lower:22-57): temp 30 °C, vcc 3.3 V.
    lower[22..24].copy_from_slice(&(30 * 256i16).to_be_bytes());
    lower[26..28].copy_from_slice(&33000u16.to_be_bytes());
    // Lane monitors (Lower:34-57; Table 6-10): rx power at 34+2(lane-1),
    // bias at 42+2(lane-1), tx power at 50+2(lane-1).
    // Lane 1 rx power 2.5 mW (above every high bound), lanes 2-4 1.0 mW.
    lower[34..36].copy_from_slice(&25000u16.to_be_bytes());
    for lane in 1..4 {
        lower[34 + 2 * lane..36 + 2 * lane].copy_from_slice(&10000u16.to_be_bytes());
    }
    // Bias: lane 1 = 1 mA (below the 2 mA low-alarm bound), lanes 2-4 6 mA.
    lower[42..44].copy_from_slice(&500u16.to_be_bytes());
    for lane in 1..4 {
        lower[42 + 2 * lane..44 + 2 * lane].copy_from_slice(&3000u16.to_be_bytes());
    }
    // Tx power: lane 1 = 2.0 mW (high alarm at 1.8), lane 2 = 1.75 mW
    // (high warning at 1.7), lanes 3-4 = 1.0 mW (within).
    lower[50..52].copy_from_slice(&20000u16.to_be_bytes());
    lower[52..54].copy_from_slice(&17500u16.to_be_bytes());
    lower[54..56].copy_from_slice(&10000u16.to_be_bytes());
    lower[56..58].copy_from_slice(&10000u16.to_be_bytes());

    let mut page03h = [0u8; 128];
    // Temp thresholds (page-relative 0-7): 75/-5/70/0 °C.
    page03h[0..2].copy_from_slice(&(75 * 256i16).to_be_bytes());
    page03h[2..4].copy_from_slice(&(-5 * 256i16).to_be_bytes());
    page03h[4..6].copy_from_slice(&(70 * 256i16).to_be_bytes());
    page03h[6..8].copy_from_slice(&0i16.to_be_bytes()); // low warning 0 °C
                                                        // Vcc thresholds (16-23): 3.63/2.97/3.465/3.135 V.
    page03h[16..18].copy_from_slice(&36300u16.to_be_bytes());
    page03h[18..20].copy_from_slice(&29700u16.to_be_bytes());
    page03h[20..22].copy_from_slice(&34650u16.to_be_bytes());
    page03h[22..24].copy_from_slice(&31350u16.to_be_bytes());
    // Rx power thresholds (48-55): 2.0/0.5/1.8/0.6 mW.
    page03h[48..50].copy_from_slice(&20000u16.to_be_bytes());
    page03h[50..52].copy_from_slice(&5000u16.to_be_bytes());
    page03h[52..54].copy_from_slice(&18000u16.to_be_bytes());
    page03h[54..56].copy_from_slice(&6000u16.to_be_bytes());
    // Bias thresholds (56-63): 15/2/14/3 mA.
    page03h[56..58].copy_from_slice(&7500u16.to_be_bytes());
    page03h[58..60].copy_from_slice(&1000u16.to_be_bytes());
    page03h[60..62].copy_from_slice(&7000u16.to_be_bytes());
    page03h[62..64].copy_from_slice(&1500u16.to_be_bytes());
    // Tx power thresholds (64-71): 1.8/0.07/1.7/0.17 mW.
    page03h[64..66].copy_from_slice(&18000u16.to_be_bytes());
    page03h[66..68].copy_from_slice(&700u16.to_be_bytes());
    page03h[68..70].copy_from_slice(&17000u16.to_be_bytes());
    page03h[70..72].copy_from_slice(&1700u16.to_be_bytes());

    let transport = Flat { lower };
    let module = Sff8636Module::open(transport).expect("rev 2.12 detected");
    let e = module.evaluate_thresholds(&page03h).expect("evaluation");

    assert!(e.temperature.is_within_bounds(), "30 °C within");
    assert!(e.supply_voltage.is_within_bounds(), "3.3 V within");
    // Rx: lane 1 above the 2.0 mW high alarm, lanes 2-4 within.
    assert!(e.rx_power[0].high_alarm && e.rx_power[0].high_warning);
    assert!(e.rx_power[1].is_within_bounds());
    // Bias: lane 1 below the 2 mA low alarm, lanes 2-4 within.
    assert!(e.tx_bias[0].low_alarm && e.tx_bias[0].low_warning);
    assert!(e.tx_bias[1].is_within_bounds());
    // Tx power severities split across lanes: alarm / warning / within.
    assert!(e.tx_power[0].high_alarm && e.tx_power[0].high_warning);
    assert!(!e.tx_power[1].high_alarm && e.tx_power[1].high_warning);
    assert!(e.tx_power[2].is_within_bounds());

    // Pure over its inputs: same bytes, same decisions.
    let again = module.evaluate_thresholds(&page03h).expect("again");
    assert_eq!(again, e);
}

/// The read footprint stays caller-directed: evaluation issues no
/// writes at all (no page-select to fetch Page 03h — the bytes were
/// supplied), verified through a shared write-counting transport.
#[test]
fn evaluation_issues_no_writes() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Clone)]
    struct Counting {
        inner: Rc<(RefCell<[u8; 128]>, Cell<u32>)>,
    }
    impl Transport for Counting {
        fn read(
            &mut self,
            _a: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            out: &mut [u8],
        ) -> Result<(), openxcvr_core::error::ModuleError> {
            out.copy_from_slice(&self.inner.0.borrow()[..out.len()]);
            Ok(())
        }
        fn write(
            &mut self,
            _a: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            _data: &[u8],
        ) -> Result<(), openxcvr_core::error::ModuleError> {
            self.inner.1.set(self.inner.1.get() + 1);
            Ok(())
        }
        fn probe(&mut self) -> Result<(), openxcvr_core::error::ModuleError> {
            Ok(())
        }
    }

    let mut lower = [0u8; 128];
    lower[1] = 0x0A; // rev 2.12 (Table 6-4)
    lower[22..24].copy_from_slice(&(30 * 256i16).to_be_bytes());
    let shared = Rc::new((RefCell::new(lower), Cell::new(0u32)));
    let module = Sff8636Module::new(Counting {
        inner: Rc::clone(&shared),
    });
    let page = [0u8; 128]; // all-zero thresholds: every decision disabled
    let e = module.evaluate_thresholds(&page).expect("zero thresholds");
    assert!(e.temperature.is_within_bounds());
    assert_eq!(shared.1.get(), 0, "evaluate_thresholds never writes");
}
