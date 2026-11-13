//! Per-lane signal flags for SFF-8472: the Adaptive Input EQ Fail Flag
//! (A2h:118.4, Table 10-1) and the revision window it carries
//! (`since: "12.5"`), decoded through the revision-specialised generated
//! readers.

use openxcvr_core::{error::ModuleError, module::Module, transport::Transport};
use openxcvr_sff8472::adapter::Sff8472Module;

/// Two-page in-memory transport (A0h base, A2h diagnostics).
struct Pages {
    a0h: [u8; 256],
    a2h: [u8; 256],
}

impl Pages {
    /// A0h byte 94 carries the compliance revision (Table 8-8): 0x0A is
    /// rev 12.5, 0x01 is rev 9.3. Byte 92 bit 5 declares internal
    /// calibration, which the lanes() monitors require.
    fn with_compliance(compliance: u8, a2h: [u8; 256]) -> Self {
        let mut a0h = [0u8; 256];
        a0h[0] = 0x03; // SFP/SFP+/SFP28 identifier
        a0h[92] = 0b0010_0000; // internally calibrated
        a0h[94] = compliance;
        Self { a0h, a2h }
    }
}

impl Transport for Pages {
    fn read(
        &mut self,
        address: u8,
        _page: u8,
        _bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match address {
            0xA0 => &self.a0h[offset as usize..],
            0xA2 => &self.a2h[offset as usize..],
            _ => return Err(ModuleError::Io("bad address".into())),
        };
        out.copy_from_slice(&src[..out.len()]);
        Ok(())
    }
    fn write(
        &mut self,
        _address: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        _data: &[u8],
    ) -> Result<(), ModuleError> {
        Err(ModuleError::Io("immutable test pages".into()))
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}

#[test]
fn lanes_carry_adaptive_input_eq_fail_flag_at_12_5() {
    let mut a2h = [0u8; 256];
    a2h[118] = 0b0001_0000; // bit 4: Adaptive Input EQ Fail Flag
    let module = Sff8472Module::open(Pages::with_compliance(0x0A, a2h)).expect("rev 12.5 opens");
    let lanes = module.lanes().expect("lanes");
    assert_eq!(lanes.len(), 1, "single channel");
    assert_eq!(lanes[0].tx_adaptive_eq_fault, Some(true));

    // Bit clear reads Some(false) at 12.5 — the monitor exists.
    a2h[118] = 0;
    let module = Sff8472Module::open(Pages::with_compliance(0x0A, a2h)).unwrap();
    assert_eq!(module.lanes().unwrap()[0].tx_adaptive_eq_fault, Some(false));
}

#[test]
fn pre_12_5_revisions_do_not_borrow_the_adaptive_eq_flag() {
    // Same bit pattern at rev 9.3 (A0h:94 = 0x01): the flag bit is not
    // modeled there, so it reads `None` — never a borrowed 12.5 meaning.
    let mut a2h = [0u8; 256];
    a2h[118] = 0b0001_0000;
    let module = Sff8472Module::open(Pages::with_compliance(0x01, a2h)).expect("rev 9.3 opens");
    let lanes = module.lanes().expect("lanes");
    assert_eq!(lanes[0].tx_adaptive_eq_fault, None);

    // The undetected union path decodes the union of revisions.
    let module = Sff8472Module::new(Pages::with_compliance(0x01, a2h));
    assert_eq!(
        module.lanes().unwrap()[0].tx_adaptive_eq_fault,
        Some(true),
        "union view decodes the union of revisions"
    );
}

#[test]
fn lanes_report_no_tx_los_or_cdr_lol() {
    // SFF-8472 defines no Tx LOS and no CDR LOL flags (they arrived with
    // the SFF-8636/8079 parallel-module maps), so the unified vocabulary
    // reports them as not-defined, whatever the bytes hold.
    let mut a2h = [0u8; 256];
    a2h[3] = 0xFF; // threshold bytes, irrelevant to signal flags
    a2h[110] = 0xFF; // every byte-110 state bit set
    let module = Sff8472Module::open(Pages::with_compliance(0x0A, a2h)).unwrap();
    let lane = &module.lanes().unwrap()[0];
    assert_eq!(lane.tx_los, None);
    assert_eq!(lane.tx_cdr_lol, None);
    assert_eq!(lane.rx_cdr_lol, None);
    // The byte-110 mirrors the spec does define still decode.
    assert_eq!(lane.rx_los, Some(true));
    assert_eq!(lane.tx_fault, Some(true));
}
