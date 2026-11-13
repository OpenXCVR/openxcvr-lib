//! Any-version pipeline for SFF-8636: detect-once (Lower:1, Table 6-4),
//! per-revision compiled dispatch, and the canonical projection — with
//! fail-closed admission. Mirrors the CMIS dispatch_by_rev contract.

use openxcvr_core::rev::Rev;
use openxcvr_core::{module::Module, DetectError};
use openxcvr_sff8636::{adapter::Sff8636Module, ctx::Ctx};

/// Flat memory layout transport, addressable for tests: bytes 0..128 are
/// the Lower page; bytes 128..256 Upper Page 00h (the layout
/// `openxcvr_core::transport::BufTransport` uses, but writable so the
/// control surface can be exercised end to end).
struct RwTransport {
    buf: Vec<u8>,
}

impl RwTransport {
    fn with_lower(lower: [u8; 128], page_00h: [u8; 128]) -> Self {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&lower);
        buf.extend_from_slice(&page_00h);
        Self { buf }
    }
}

impl openxcvr_core::transport::Transport for RwTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        out: &mut [u8],
    ) -> Result<(), openxcvr_core::error::ModuleError> {
        let start = match (page, offset) {
            (0x00, 0..=127) => offset as usize,
            (0x00, 128..=255) => offset as usize,
            _ => {
                return Err(openxcvr_core::error::ModuleError::Io(
                    format!("unsupported read page/offset 0x{page:02x}/{offset}").into(),
                ))
            }
        };
        let end = start + out.len();
        if end > self.buf.len() {
            return Err(openxcvr_core::error::ModuleError::Io("short read".into()));
        }
        out.copy_from_slice(&self.buf[start..end]);
        Ok(())
    }

    fn write(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), openxcvr_core::error::ModuleError> {
        let start = match (page, offset) {
            (0x00, 0..=127) => offset as usize,
            _ => {
                return Err(openxcvr_core::error::ModuleError::Io(
                    format!("unsupported write page/offset 0x{page:02x}/{offset}").into(),
                ))
            }
        };
        let end = start + data.len();
        if end > self.buf.len() {
            return Err(openxcvr_core::error::ModuleError::Io("short write".into()));
        }
        self.buf[start..end].copy_from_slice(data);
        Ok(())
    }

    fn probe(&mut self) -> Result<(), openxcvr_core::error::ModuleError> {
        Ok(())
    }
}

fn lower_with_rev(compliance: u8) -> [u8; 128] {
    let mut lower = [0u8; 128];
    lower[0] = 0x0D; // QSFP+ identifier
    lower[1] = compliance;
    // Channel monitor interrupt flags: lane 2 rx power high alarm.
    // Flattened bit = 4 * (lane - 1) + severity; bit 4 (lane 2, high alarm)
    // decodes at offset 9 + 4/8 = 9, bit_offset 4*(1-((4/4)%2)) + (3-0) = 3,
    // i.e. mask 0x08 of byte 9.
    lower[9] = 0b0000_1000;
    lower
}

#[test]
fn open_detects_each_admitted_revision_from_table_6_4() {
    for (byte, rev) in [
        (0x03u8, Rev::SFF8636_1_3),
        (0x04, Rev::SFF8636_1_4),
        (0x05, Rev::SFF8636_1_5),
        (0x06, Rev::SFF8636_2_0),
        (0x07, Rev::SFF8636_2_5),
        (0x08, Rev::SFF8636_2_8),
        (0x09, Rev::SFF8636_2_11),
        (0x0A, Rev::SFF8636_2_12),
    ] {
        let module = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(byte), [0; 128]))
            .expect("admitted compliance code opens");
        assert_eq!(module.detected_rev(), Some(rev), "byte {byte:#04x}");
    }
}

#[test]
fn open_fails_closed_on_unspecified_and_legacy_codes() {
    for byte in [0x00u8, 0x01, 0x02, 0x0B, 0xFF] {
        let err = match Sff8636Module::open(RwTransport::with_lower(lower_with_rev(byte), [0; 128]))
        {
            Err(e) => e,
            Ok(_) => panic!("byte {byte:#04x} must fail closed"),
        };
        match err {
            DetectError::UnsupportedRev {
                detected_byte,
                known_revs,
            } => {
                assert_eq!(detected_byte, byte);
                assert_eq!(
                    known_revs,
                    Rev::known_for(openxcvr_core::rev::Family::Sff8636)
                );
            }
            other => panic!("unexpected error for byte {byte:#04x}: {other:?}"),
        }
    }
}

/// The rev-aware path reports the detected spec revision in
/// `vendor_info().spec_rev` while `.rev` stays the (empty) vendor hardware
/// string; the undetected union path reports `spec_rev: None`, never a
/// guess.
#[test]
fn vendor_info_carries_spec_rev_and_keeping_hardware_rev_separate() {
    let module = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(0x09), [0; 128]))
        .expect("2.11 opens");
    let info = module.vendor_info().expect("vendor_info");
    assert_eq!(info.spec_rev, Some(Rev::SFF8636_2_11));
    assert_eq!(info.rev, "", "hardware ASCII rev stays its own field");

    let union = Sff8636Module::new(RwTransport::with_lower(lower_with_rev(0x09), [0; 128]));
    let info = union.vendor_info().expect("vendor_info");
    assert_eq!(info.spec_rev, None, "undetected path never guesses");
}

/// A v1_3 module never borrows the post-1.4 extended-compliance bit: the
/// applications projection reports `None` there and `Some(bit)` later.
#[test]
fn applications_do_not_borrow_post_1_4_compliance_bit_at_v1_3() {
    let mut page_00h = [0u8; 128];
    page_00h[3] = 0x80; // byte 131 flat = page offset 3: extended bit set

    let v1_3 = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(0x03), page_00h))
        .expect("1.3 opens");
    let apps_v1_3 = v1_3.applications().expect("apps");
    // The bit is not modeled at 1.3; the primary compliance bits stand and
    // no extended name is borrowed.
    assert!(
        !apps_v1_3[0].spec_bits.iter().any(|b| b.contains("GBASE")),
        "v1_3 must not borrow the extended bit: {:?}",
        apps_v1_3[0].spec_bits
    );

    let v2_11 = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(0x09), page_00h))
        .expect("2.11 opens");
    let apps = v2_11.applications().expect("apps");
    assert!(
        !apps[0].spec_bits.iter().any(|b| b.contains("GBASE")),
        "with the extended bit set, the primary bits are invalid and only the extended code names the app: {:?}",
        apps[0].spec_bits
    );
}

/// Parity fix: per-lane alarm/warning flags from Lower bytes 9-15 are no
/// longer dropped.
#[test]
fn module_flags_carry_per_lane_alarm_flags() {
    let module = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(0x09), [0; 128]))
        .expect("opens");
    let flags = module.flags().expect("flags");
    assert_eq!(flags.lanes.len(), 4, "four lanes of flags");
    // lower[9] = 0b0000_0100: lane 2 high alarm at flattened bit 4.
    assert!(flags.lanes[1].rx_power_high_alarm, "lane 2 rx high alarm");
    assert!(!flags.lanes[0].rx_power_high_alarm);
    assert!(!flags.lanes[1].rx_power_low_alarm);
}

/// Control surface: Tx disable and rate select write the schema-declared
/// control bytes read-modify-write.
#[test]
fn control_writes_tx_disable_and_rate_select() {
    let module = Sff8636Module::open(RwTransport::with_lower(lower_with_rev(0x09), [0; 128]))
        .expect("opens");
    module.set_tx_disable(2, true).expect("disable lane 2");
    module.set_tx_rate_select_raw(3, 0b10).expect("tx rate sel");
    module.set_rx_rate_select_raw(3, 0b01).expect("rx rate sel");
    let lanes = module.lanes().expect("lanes");
    assert_eq!(lanes[1].tx_disable, Some(true));
    assert_eq!(lanes[2].tx_disable, Some(false));
    assert_eq!(
        lanes[2].rx_los,
        Some(false),
        "sibling bits untouched by RMW"
    );

    // Out-of-range lanes and codes fail closed.
    assert!(module.set_tx_disable(0, true).is_err());
    assert!(module.set_tx_disable(5, true).is_err());
    assert!(module.set_tx_rate_select_raw(1, 4).is_err());
}

/// Ctx is directly constructible from a Lower page slice for callers that
/// already hold the bytes (the unified factory does this).
#[test]
fn ctx_parses_lower_memory_slice() {
    let ctx = Ctx::from_lower_memory(&[0x0D, 0x0A, 0x00]).expect("2.12");
    assert_eq!(ctx.detected_rev, Rev::SFF8636_2_12);
}

/// Per-lane latched signal flags (Table 6-5, Lower bytes 3-5): Tx LOS and
/// Rx LOS at byte 3, Tx Adaptive EQ fault and Tx fault at byte 4, Tx/Rx
/// CDR LOL at byte 5. Lane n sits at bit 3+n (high nibble) or n-1 (low
/// nibble) of its byte, per the schema's `bit_offset_expr`.
#[test]
fn lanes_carry_per_lane_signal_flags() {
    let mut lower = lower_with_rev(0x0A); // rev 2.12
    lower[3] = 0b0001_0001; // Tx LOS lane 1 (bit 4), Rx LOS lane 1 (bit 0)
    lower[4] = 0b1000_0010; // EQ fault lane 4 (bit 7), Tx fault lane 2 (bit 1)
    lower[5] = 0b0100_0010; // Tx CDR LOL lane 3 (bit 6), Rx CDR LOL lane 2 (bit 1)
    let module =
        Sff8636Module::open(RwTransport::with_lower(lower, [0; 128])).expect("admitted rev opens");
    let lanes = module.lanes().expect("lanes");
    assert_eq!(lanes.len(), 4);

    assert_eq!(lanes[0].tx_los, Some(true), "lane 1 Tx LOS");
    assert_eq!(lanes[1].tx_los, Some(false), "lane 2 Tx LOS clear");
    assert_eq!(lanes[0].rx_los, Some(true), "lane 1 Rx LOS");
    assert_eq!(lanes[3].rx_los, Some(false), "lane 4 Rx LOS clear");

    assert_eq!(lanes[3].tx_adaptive_eq_fault, Some(true), "lane 4 EQ fault");
    assert_eq!(lanes[0].tx_adaptive_eq_fault, Some(false));
    assert_eq!(lanes[1].tx_fault, Some(true), "lane 2 Tx fault");
    assert_eq!(lanes[0].tx_fault, Some(false));

    assert_eq!(lanes[2].tx_cdr_lol, Some(true), "lane 3 Tx CDR LOL");
    assert_eq!(lanes[1].tx_cdr_lol, Some(false));
    assert_eq!(lanes[1].rx_cdr_lol, Some(true), "lane 2 Rx CDR LOL");
    assert_eq!(lanes[0].rx_cdr_lol, Some(false));
}

/// The Tx Adaptive EQ fault bits carry a revision window (schema
/// `since: "2.0"`): a pre-2.0 module never borrows a later-revision
/// reading of the reserved bits — the flag reads `None`, not false.
#[test]
fn lanes_do_not_borrow_pre_2_0_adaptive_eq_flag() {
    let mut lower = lower_with_rev(0x03); // rev 1.3
    lower[4] = 0b1000_0000; // bit 7 — reserved at this revision
    let module =
        Sff8636Module::open(RwTransport::with_lower(lower, [0; 128])).expect("admitted rev opens");
    for lane in module.lanes().expect("lanes") {
        assert_eq!(lane.tx_adaptive_eq_fault, None, "pre-2.0: not decoded");
    }

    // The same bytes at rev 2.12 (and on the undetected union path) decode
    // the flag.
    let mut lower212 = lower_with_rev(0x0A);
    lower212[4] = 0b1000_0000;
    let m212 = Sff8636Module::open(RwTransport::with_lower(lower212, [0; 128]))
        .expect("admitted rev opens");
    assert_eq!(
        m212.lanes().expect("lanes")[3].tx_adaptive_eq_fault,
        Some(true)
    );

    let mut union_lower = lower_with_rev(0x03);
    union_lower[4] = 0b1000_0000;
    let m_union = Sff8636Module::new(RwTransport::with_lower(union_lower, [0; 128]));
    assert_eq!(
        m_union.lanes().expect("lanes")[3].tx_adaptive_eq_fault,
        Some(true),
        "union view decodes the union of revisions"
    );
}
