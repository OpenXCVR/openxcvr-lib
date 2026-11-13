use openxcvr_transports::{
    bus::{Address7, I2cBus, Limits},
    paged::{Layout, Memory, Timing},
    Error, ErrorKind, Result,
};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Bus {
    bank: u8,
    page: u8,
    writes: Vec<(u8, Vec<u8>)>,
    reads: usize,
    read_limit: usize,
    read_ranges: Vec<(u8, usize)>,
    fail_read: Option<usize>,
    disconnect_read: Option<usize>,
    fail_write: Option<usize>,
    disconnect_write: Option<usize>,
}
impl I2cBus for Bus {
    fn limits(&self) -> Limits {
        Limits {
            write: 60,
            read: if self.read_limit == 0 {
                8
            } else {
                self.read_limit
            },
            prefix: 1,
        }
    }
    fn write(&mut self, addr: Address7, bytes: &[u8], _: Instant) -> Result<()> {
        self.writes.push((addr.get(), bytes.to_vec()));
        if self.fail_write == Some(self.writes.len()) {
            return Err(Error::new(ErrorKind::Timeout, "lost completion").uncertain());
        }
        if self.disconnect_write == Some(self.writes.len()) && !matches!(bytes[0], 126 | 127) {
            // A replacement module is now selected at its power-on page. Only a
            // data write triggers this, never a selector write (bytes 126/127).
            self.page = 0;
            self.bank = 0;
            return Err(Error::new(ErrorKind::Disconnected, "module replaced"));
        }
        if bytes[0] == 126 {
            self.bank = bytes[1];
        }
        if bytes[0] == 127 {
            self.page = bytes[1];
        }
        Ok(())
    }
    fn write_read(
        &mut self,
        addr: Address7,
        prefix: &[u8],
        out: &mut [u8],
        _: Instant,
    ) -> Result<()> {
        self.reads += 1;
        self.read_ranges.push((prefix[0], out.len()));
        if self.fail_read == Some(self.reads) {
            return Err(Error::new(ErrorKind::Nack, "missing module"));
        }
        if self.disconnect_read == Some(self.reads) {
            // A replacement module is now selected at its power-on page.
            self.page = 0;
            self.bank = 0;
            return Err(Error::new(ErrorKind::Disconnected, "module replaced"));
        }
        for (i, b) in out.iter_mut().enumerate() {
            *b = match usize::from(prefix[0]) + i {
                126 => self.bank,
                127 => self.page,
                128.. => self
                    .bank
                    .wrapping_mul(32)
                    .wrapping_add(self.page)
                    .wrapping_add(addr.get()),
                _ => addr.get(),
            };
        }
        Ok(())
    }
}
fn timing() -> Timing {
    Timing {
        write_block: 8,
        write_settle: Duration::ZERO,
        select_settle: Duration::ZERO,
        timeout: Duration::from_secs(1),
    }
}

#[test]
fn c_window_selects_before_lower_fragment_and_keeps_banks_distinct() {
    let mut m = Memory::new(Bus::default(), Layout::Cmis { flat: false }, timing()).unwrap();
    let mut a = [0; 16];
    m.read_window(0x50, 0x11, 1, 120, &mut a).unwrap();
    assert_eq!(&a[..6], &[0x50; 6]);
    assert_eq!(&a[6..8], &[1, 0x11]);
    assert_eq!(&a[8..], &[0x81; 8]);
    let mut b = [0; 8];
    m.read_memory(0x50, 0x11, 2, 128, &mut b).unwrap();
    assert_eq!(b, [0xa1; 8]);
}

#[test]
fn wide_bus_still_splits_at_the_module_window_without_reselecting() {
    let mut memory = Memory::new(
        Bus {
            read_limit: 65535,
            ..Bus::default()
        },
        Layout::Cmis { flat: false },
        timing(),
    )
    .unwrap();
    let mut out = [0; 16];
    memory.read_window(0x50, 0x11, 1, 120, &mut out).unwrap();
    assert_eq!(&out[6..8], &[1, 0x11]);
    assert_eq!(&out[8..], &[0x81; 8]);
    let bus = memory.into_inner();
    assert_eq!(bus.read_ranges, vec![(120, 8), (128, 8)]);
    assert_eq!(
        bus.writes,
        vec![(0x50, vec![126, 1]), (0x50, vec![127, 0x11])]
    );
}

#[test]
fn sff_addresses_do_not_alias_or_touch_selectors() {
    let mut m = Memory::new(
        Bus::default(),
        Layout::Sff8472 {
            paged_diagnostics: false,
        },
        timing(),
    )
    .unwrap();
    let mut a = [0];
    let mut b = [0];
    m.read_memory(0xa0, 0, 0, 16, &mut a).unwrap();
    m.read_memory(0xa2, 0, 0, 16, &mut b).unwrap();
    assert_eq!(a, [0x50]);
    assert_eq!(b, [0x51]);
    assert!(m.into_inner().writes.is_empty());
    assert!(Address7::from_legacy(0xfe).is_err());
}

#[test]
fn full_window_and_single_window_bounds_are_distinct() {
    let mut m = Memory::new(Bus::default(), Layout::Cmis { flat: true }, timing()).unwrap();
    m.read_window(0x50, 0, 0, 0, &mut [0; 256]).unwrap();
    m.read_window(0x50, 0, 0, 255, &mut [0; 1]).unwrap();
    assert!(m.read_window(0x50, 0, 0, 255, &mut [0; 2]).is_err());
    assert!(m.read_window(0x50, 0, 0, 0, &mut []).is_err());
    assert!(m.read_memory(0x50, 0, 0, 127, &mut [0; 2]).is_err());
    assert!(m.read_memory(0x50, 1, 0, 128, &mut [0]).is_err());
    assert!(m.into_inner().writes.is_empty());
}

#[test]
fn sff8636_does_not_write_reserved_bank_byte() {
    let mut m = Memory::new(Bus::default(), Layout::Sff8636 { flat: false }, timing()).unwrap();
    m.read_memory(0x50, 3, 0, 128, &mut [0]).unwrap();
    assert!(m.read_memory(0x50, 3, 1, 128, &mut [0]).is_err());
    assert_eq!(m.into_inner().writes, vec![(0x50, vec![127, 3])]);
}

#[test]
fn late_read_error_does_not_publish_partial_output() {
    let mut m = Memory::new(
        Bus {
            fail_read: Some(2),
            ..Bus::default()
        },
        Layout::Cmis { flat: true },
        timing(),
    )
    .unwrap();
    let mut out = [0xcc; 16];
    assert_eq!(
        m.read_memory(0x50, 0, 0, 0, &mut out).unwrap_err().kind,
        ErrorKind::Nack
    );
    assert_eq!(out, [0xcc; 16]);
}

#[test]
fn disconnect_forgets_confirmed_page_before_next_read_or_write() {
    for write_next in [false, true] {
        let mut m = Memory::new(
            Bus {
                disconnect_read: Some(2),
                ..Bus::default()
            },
            Layout::Sff8636 { flat: false },
            timing(),
        )
        .unwrap();
        let mut out = [0xcc];
        m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
        out = [0xcc];
        assert_eq!(
            m.read_memory(0x50, 3, 0, 128, &mut out).unwrap_err().kind,
            ErrorKind::Disconnected
        );
        assert_eq!(out, [0xcc]);
        if write_next {
            m.write_memory(0x50, 3, 0, 128, &[7]).unwrap();
        } else {
            m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
            assert_eq!(out, [0x53]);
        }
        assert_eq!(
            m.into_inner()
                .writes
                .iter()
                .filter(|(_, bytes)| bytes.as_slice() == [127, 3])
                .count(),
            2
        );
    }
}

#[test]
fn disconnect_on_a_data_write_forgets_the_page_and_the_next_access_reselects() {
    // A confirmed page-3 selection is established first (a selector write of
    // byte 127). The following data write is byte 128, not a selector, and the
    // bus reports it Disconnected: the module was replaced and now sits at its
    // power-on page 0, so the cached page-3 selection is stale and must be
    // dropped before the next access trusts it.
    let mut m = Memory::new(
        Bus {
            disconnect_write: Some(2),
            ..Bus::default()
        },
        Layout::Sff8636 { flat: false },
        timing(),
    )
    .unwrap();
    let mut out = [0xcc; 2];
    m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
    assert_eq!(out, [0x53; 2]);
    let err = m.write_memory(0x50, 3, 0, 128, &[7]).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Disconnected);
    // The failed packet was the first fragment of the data write, so no earlier
    // data applied and the error is not marked uncertain.
    assert!(!err.write_may_have_applied);
    // The selection was forgotten, so the next access re-selects page 3 and
    // returns the requested page rather than the replacement module's page 0.
    out = [0xcc; 2];
    m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
    assert_eq!(out, [0x53; 2]);
    let bus = m.into_inner();
    assert_eq!(
        bus.writes,
        vec![
            (0x50, vec![127, 3]),
            (0x50, vec![128, 7]),
            (0x50, vec![127, 3]),
        ]
    );
}

#[test]
fn writes_respect_boundaries_and_never_retry_uncertain_data() {
    let mut m = Memory::new(
        Bus {
            fail_write: Some(2),
            ..Bus::default()
        },
        Layout::Cmis { flat: true },
        timing(),
    )
    .unwrap();
    let err = m.write_memory(0x50, 0, 0, 6, &[1; 12]).unwrap_err();
    assert!(err.write_may_have_applied);
    assert_eq!(
        m.into_inner().writes,
        vec![
            (0x50, vec![6, 1, 1]),
            (0x50, vec![8, 1, 1, 1, 1, 1, 1, 1, 1])
        ]
    );
}

#[test]
fn optional_sff8472_pages_have_their_own_selector() {
    let mut m = Memory::new(
        Bus::default(),
        Layout::Sff8472 {
            paged_diagnostics: true,
        },
        timing(),
    )
    .unwrap();
    m.read_memory(0xa2, 2, 0, 128, &mut [0]).unwrap();
    assert!(m.read_memory(0xa0, 2, 0, 128, &mut [0]).is_err());
    assert_eq!(m.into_inner().writes, vec![(0x51, vec![127, 2])]);
}

#[test]
fn failed_bank_select_is_uncertain_and_the_next_access_reselects() {
    // Write 3 is the bank-select write for bank 2; writes 1 and 2 selected
    // page 0x10 in bank 1 successfully first.
    let mut m = Memory::new(
        Bus {
            fail_write: Some(3),
            ..Bus::default()
        },
        Layout::Cmis { flat: false },
        timing(),
    )
    .unwrap();
    read_ok(&mut m, 0x10, 1);
    let mut out = [0xcc; 2];
    let err = m.read_window(0x50, 0x10, 2, 0, &mut out).unwrap_err();
    assert!(err.write_may_have_applied);
    // A failed select publishes no partial output either.
    assert_eq!(out, [0xcc; 2]);
    // The device may already be on bank 2, so the bank-1 selection cached
    // before the failure is gone and this access re-selects it.
    read_ok(&mut m, 0x10, 1);
    let bus = m.into_inner();
    assert_eq!(
        bus.writes,
        vec![
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
            (0x50, vec![126, 2]),
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
        ]
    );
    assert_eq!(bus.reads, 2);
}

#[test]
fn failed_page_select_is_uncertain_and_the_next_access_reselects() {
    // SFF-8636 paging writes byte 127 only: byte 126 is reserved and never a
    // selector here, on success or on failure.
    let mut m = Memory::new(
        Bus {
            fail_write: Some(2),
            ..Bus::default()
        },
        Layout::Sff8636 { flat: false },
        timing(),
    )
    .unwrap();
    let mut out = [0; 2];
    m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
    let err = m.read_memory(0x50, 4, 0, 128, &mut out).unwrap_err();
    assert!(err.write_may_have_applied);
    m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
    assert_eq!(
        m.into_inner().writes,
        vec![
            (0x50, vec![127, 3]),
            (0x50, vec![127, 4]),
            (0x50, vec![127, 3]),
        ]
    );
}

#[test]
fn failed_select_settle_is_uncertain_after_the_selector_was_written() {
    let mut m = Memory::new(
        Bus::default(),
        Layout::Sff8636 { flat: false },
        Timing {
            select_settle: Duration::from_secs(2),
            timeout: Duration::from_secs(1),
            ..timing()
        },
    )
    .unwrap();
    let mut out = [0; 2];
    let err = m.read_memory(0x50, 3, 0, 128, &mut out).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    // The page selector was already written when the settle deadline expired.
    assert!(err.write_may_have_applied);
    let bus = m.into_inner();
    assert_eq!(bus.writes, vec![(0x50, vec![127, 3])]);
    assert_eq!(bus.reads, 0);
}

#[test]
fn failed_select_on_the_write_path_is_uncertain_and_the_next_write_reselects() {
    let mut m = Memory::new(
        Bus {
            fail_write: Some(3),
            ..Bus::default()
        },
        Layout::Cmis { flat: false },
        timing(),
    )
    .unwrap();
    read_ok(&mut m, 0x10, 1);
    let err = m.write_memory(0x50, 0x10, 2, 128, &[7]).unwrap_err();
    assert!(err.write_may_have_applied);
    m.write_memory(0x50, 0x10, 1, 128, &[7]).unwrap();
    assert_eq!(
        m.into_inner().writes,
        vec![
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
            (0x50, vec![126, 2]),
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
            (0x50, vec![128, 7]),
        ]
    );
}

#[test]
fn window_write_over_selector_bytes_invalidates_the_selection() {
    let mut m = Memory::new(Bus::default(), Layout::Cmis { flat: false }, timing()).unwrap();
    read_ok(&mut m, 0x10, 1);
    // A window write may cover bytes 126/127, which are the module's own
    // selector registers, so the selection stops being known before the write.
    m.write_window(0x50, 0x10, 1, 120, &[0; 16]).unwrap();
    read_ok(&mut m, 0x10, 1);
    assert_eq!(
        m.into_inner().writes,
        vec![
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
            (0x50, vec![120, 0, 0, 0, 0, 0, 0, 0, 0]),
            (0x50, vec![128, 0, 0, 0, 0, 0, 0, 0, 0]),
            (0x50, vec![126, 1]),
            (0x50, vec![127, 0x10]),
        ]
    );
}

#[test]
fn confirmed_selection_is_not_rewritten_for_the_same_page_and_bank() {
    let mut m = Memory::new(Bus::default(), Layout::Cmis { flat: false }, timing()).unwrap();
    read_ok(&mut m, 0x10, 1);
    read_ok(&mut m, 0x10, 1);
    let bus = m.into_inner();
    assert_eq!(
        bus.writes,
        vec![(0x50, vec![126, 1]), (0x50, vec![127, 0x10])]
    );
    assert_eq!(bus.reads, 2);
}

/// Reads two bytes of the selected window, asserting the selection was published.
fn read_ok<B: I2cBus>(memory: &mut Memory<B>, page: u8, bank: u8) {
    let mut out = [0; 2];
    memory.read_window(0x50, page, bank, 0, &mut out).unwrap();
}

#[test]
fn a_failed_select_is_forgotten_so_retrying_the_same_page_reselects() {
    // Retrying the page that just failed must rewrite the selector: remembering
    // the attempted page after a failure would skip it and read whatever page
    // the module actually holds.
    let mut m = Memory::new(
        Bus {
            fail_write: Some(2),
            ..Bus::default()
        },
        Layout::Sff8636 { flat: false },
        timing(),
    )
    .unwrap();
    let mut out = [0; 2];
    m.read_memory(0x50, 3, 0, 128, &mut out).unwrap();
    let err = m.read_memory(0x50, 4, 0, 128, &mut out).unwrap_err();
    assert!(err.write_may_have_applied);
    m.read_memory(0x50, 4, 0, 128, &mut out).unwrap();
    assert_eq!(
        m.into_inner().writes,
        vec![
            (0x50, vec![127, 3]),
            (0x50, vec![127, 4]),
            (0x50, vec![127, 4]),
        ]
    );
}
