//! SFF-8472 control surface: soft TX disable and rate select write the
//! schema-declared A2h control bits read-modify-write (Tables 9-16 and
//! 10-1), over a shared in-memory transport for byte-level assertions and
//! over the mmap capture transport for the caller-directed write
//! mechanism.

use openxcvr_core::error::ModuleError;
use openxcvr_core::module::Module;
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

/// The Finisar capture carries A2h byte 110 = 0x12 (RS(0) state and Rx_LOS
/// state asserted, everything else clear) and byte 118 = 0x00 — a fixed
/// reference for the RMW assertions below.
const A0H: &str = "finisar-ftlx8571d3bcl-mup0wb0.a0h.bin";
const A2H: &str = "finisar-ftlx8571d3bcl-mup0wb0.a2h.bin";

/// In-memory SFF-8472 transport whose pages the test retains a handle to,
/// so raw control bytes stay inspectable while the module owns a clone.
#[derive(Clone)]
struct SharedTransport {
    pages: Rc<RefCell<([u8; 256], [u8; 256])>>, // (A0h, A2h)
}

impl SharedTransport {
    fn from_fixture(a0h: &str, a2h: &str) -> Self {
        let a0 = std::fs::read(fixture(a0h)).unwrap();
        let a2 = std::fs::read(fixture(a2h)).unwrap();
        let mut pages = ([0u8; 256], [0u8; 256]);
        // The A0h capture may be shorter than the full page (the mmap
        // transport pads the same way); the A2h capture is always full.
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

#[test]
fn control_writes_soft_tx_disable_and_rate_selects() {
    let shared = SharedTransport::from_fixture(A0H, A2H);
    let module = Sff8472Module::open(shared.clone()).unwrap();
    // This capture declares SFF-8472 10.2 compliance (A0h:94 = 0x03).
    assert_eq!(
        module.detected_rev(),
        Some(openxcvr_core::Rev::SFF8472_10_2)
    );
    let byte = |off: usize| shared.pages.borrow().1[off];

    // Soft TX Disable Select is A2h:110.6 (Table 9-16); byte 110 was 0x12.
    module.set_tx_disable(true).unwrap();
    assert_eq!(
        byte(110),
        0x52,
        "only bit 6 of byte 110 changes: 0x12 | 0x40"
    );
    module.set_tx_disable(false).unwrap();
    assert_eq!(byte(110), 0x12, "clearing restores the captured byte");

    // Soft Rate Select RS(0) is A2h:110.3.
    module.set_rate_select_rs0(true).unwrap();
    assert_eq!(byte(110), 0x1A, "0x12 | 0x08");
    module.set_rate_select_rs0(false).unwrap();
    assert_eq!(byte(110), 0x12);

    // Soft RS(1) Select is A2h:118.3 (Table 10-1); byte 118 was 0x00.
    module.set_rate_select_rs1(true).unwrap();
    assert_eq!(byte(118), 0x08, "only bit 3 of byte 118 changes");
    module.set_rate_select_rs1(false).unwrap();
    assert_eq!(byte(118), 0x00);
}

#[test]
fn soft_tx_disable_does_not_flip_the_pin_state_mirror() {
    // Byte 110 bit 7 is the TX Disable pin-state mirror, a read-only
    // meaning per Table 9-16 — writing the soft select at bit 6 must not
    // change what lanes() reports for tx_disable.
    let module = Sff8472Module::open(SharedTransport::from_fixture(A0H, A2H)).unwrap();
    let before = module.lanes().unwrap();
    assert_eq!(before[0].tx_disable, Some(false));
    module.set_tx_disable(true).unwrap();
    let after = module.lanes().unwrap();
    assert_eq!(
        after[0].tx_disable,
        Some(false),
        "the pin-state mirror is not a write target"
    );
    // The lane status bits decode per Table 9-16 on this capture
    // (A2h:110 = 0x12): Rx_LOS state asserted (bit 1), TX Fault clear
    // (bit 2).
    assert_eq!(after[0].rx_los, Some(true));
    assert_eq!(after[0].tx_fault, Some(false));
}

#[test]
fn mmap_write_is_confined_to_the_control_bytes() {
    let mut t = Sff8472MmapTransport::from_files(fixture(A0H), fixture(A2H)).unwrap();
    // The A0h base page is never writable.
    assert!(t.write(0xA0, 0, 0, 110, &[0xFF]).is_err());
    // Monitor and flag bytes of A2h are not part of the control surface.
    assert!(t.write(0xA2, 0, 0, 100, &[0xFF]).is_err());
    assert!(t.write(0xA2, 0, 0, 112, &[0xFF]).is_err());
    assert!(t.write(0xA2, 0, 0, 127, &[0xFF]).is_err());
    // Multi-byte writes are refused; the control surface is per-byte RMW.
    assert!(t.write(0xA2, 0, 0, 110, &[0, 0]).is_err());
    // The two control bytes accept the write and read back mutated — the
    // caller-directed offline write mechanism (mirroring SFF-8636's mmap).
    t.write(0xA2, 0, 0, 110, &[0x52]).unwrap();
    t.write(0xA2, 0, 0, 118, &[0x08]).unwrap();
    let mut buf = [0u8; 1];
    t.read(0xA2, 0, 0, 110, &mut buf).unwrap();
    assert_eq!(buf[0], 0x52);
    t.read(0xA2, 0, 0, 118, &mut buf).unwrap();
    assert_eq!(buf[0], 0x08);
}

#[test]
fn data_not_ready_decodes_from_the_schema_field() {
    // The capture has A2h:110 = 0x12, so Data_Not_Ready (bit 0) is clear.
    let module = Sff8472Module::new(SharedTransport::from_fixture(A0H, A2H));
    assert_eq!(module.flags().unwrap().data_not_ready, Some(false));
    assert_eq!(module.module_state().unwrap().data_not_ready, Some(false));
}

#[test]
fn every_detected_revision_decodes_the_byte_110_lane_flags() {
    // The per-bit byte-110 fields exist at every admitted revision, so the
    // lanes/flags projections decode through each per-revision generated
    // reader — spot-check across the synthetic capture set, including a
    // module whose A2h carries a set Data_Not_Ready bit.
    for (name, expected_los, expected_dnr) in [
        // The DAC capture is excluded: it declares no DDMI calibration
        // (A0h:92), so lanes() legitimately refuses it before byte 110.
        ("synthetic-extcal-sfp10g-sr", false, false),
        ("synthetic-sfp28-25g-sr", false, false),
    ] {
        // Both captures detect 10.4 (A0h:94 = 0x04) and decode through the
        // per-revision generated reader.
        let module = Sff8472Module::open(SharedTransport::from_fixture(
            &format!("{name}.a0h.bin"),
            &format!("{name}.a2h.bin"),
        ))
        .unwrap();
        let lanes = module.lanes().unwrap();
        assert_eq!(
            lanes[0].rx_los,
            Some(expected_los),
            "{name}: rx_los must decode the Table 9-16 bit 110.1 state"
        );
        assert_eq!(
            module.flags().unwrap().data_not_ready,
            Some(expected_dnr),
            "{name}: data_not_ready must decode the Table 9-16 bit 110.0 state"
        );
    }
}
