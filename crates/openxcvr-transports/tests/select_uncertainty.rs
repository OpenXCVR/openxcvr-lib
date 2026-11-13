//! Select-write uncertainty through the public `paged::Memory` path.
//!
//! A page- or bank-select write is a write boundary in its own right: once it
//! has been submitted, a failure may have left the device on a different
//! selection, so the error it returns carries `write_may_have_applied` and the
//! next access re-selects instead of trusting the cached selection. A failure
//! the bus reports as *not sent* was never submitted, so its flag must stay
//! false. Both runs use [`Layout::Cmis{flat:false}`], whose selector writes are
//! byte 126 (bank) and byte 127 (page).
//!
//! The fake bus is copied from `tests/memory.rs` and scripts one failing write,
//! flagged submitted or not sent. Every scenario runs once per adapter profile
//! so the paged path's select semantics are shown independent of which adapter
//! identity is in play.

use openxcvr_transports::{
    bus::{Address7, I2cBus, Limits},
    paged::{Layout, Memory, Timing},
    Error, ErrorKind, Result,
};
use std::time::{Duration, Instant};

#[allow(dead_code)]
mod support;

/// The `tests/memory.rs` fake, with the failing write scripting whether the
/// bus submitted it before failing.
#[derive(Default)]
struct Bus {
    bank: u8,
    page: u8,
    writes: Vec<(u8, Vec<u8>)>,
    fail_write: Option<(usize, bool)>,
}
impl I2cBus for Bus {
    fn limits(&self) -> Limits {
        Limits {
            write: 60,
            read: 8,
            prefix: 1,
        }
    }
    fn write(&mut self, addr: Address7, bytes: &[u8], _: Instant) -> Result<()> {
        self.writes.push((addr.get(), bytes.to_vec()));
        if let Some((nth, submitted)) = self.fail_write {
            if nth == self.writes.len() {
                let error = Error::new(ErrorKind::Timeout, "lost completion");
                return Err(if submitted { error.uncertain() } else { error });
            }
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

/// The bank-select write (byte 126) fails after submission: the error is
/// uncertain and the next access re-selects bank and page.
#[test]
fn bank_select_failure_after_submission_is_uncertain_and_the_next_access_reselects() {
    for profile in support::PROFILES {
        let mut m = Memory::new(
            Bus {
                fail_write: Some((1, true)),
                ..Bus::default()
            },
            Layout::Cmis { flat: false },
            timing(),
        )
        .unwrap();
        let mut out = [0xcc; 2];
        let err = m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap_err();
        assert!(
            err.write_may_have_applied,
            "{}: a bank select submitted before failing is uncertain",
            profile.kernel_driver
        );
        // A failed select publishes no partial output.
        assert_eq!(out, [0xcc; 2]);
        // The device may already be on bank 2, so the cached selection is gone
        // and this access re-selects bank and page.
        m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap();
        let bus = m.into_inner();
        assert_eq!(
            bus.writes,
            vec![
                (0x50, vec![126, 1]),
                (0x50, vec![126, 1]),
                (0x50, vec![127, 0x10]),
            ],
            "{}: the select write is issued again after a failed bank select",
            profile.kernel_driver
        );
    }
}

/// The page-select write (byte 127) fails after submission: the error is
/// uncertain and the next access re-selects.
#[test]
fn page_select_failure_after_submission_is_uncertain_and_the_next_access_reselects() {
    for profile in support::PROFILES {
        let mut m = Memory::new(
            Bus {
                fail_write: Some((2, true)),
                ..Bus::default()
            },
            Layout::Cmis { flat: false },
            timing(),
        )
        .unwrap();
        let mut out = [0xcc; 2];
        let err = m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap_err();
        assert!(
            err.write_may_have_applied,
            "{}: a page select submitted before failing is uncertain",
            profile.kernel_driver
        );
        m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap();
        let bus = m.into_inner();
        assert_eq!(
            bus.writes,
            vec![
                (0x50, vec![126, 1]),
                (0x50, vec![127, 0x10]),
                (0x50, vec![126, 1]),
                (0x50, vec![127, 0x10]),
            ],
            "{}: the select write is issued again after a failed page select",
            profile.kernel_driver
        );
    }
}

/// A write failure the bus reports as not sent was never submitted, so the
/// error crossing the memory layer must keep `write_may_have_applied == false`.
/// Scripted here on the first data write, where no boundary was crossed.
#[test]
fn not_sent_failure_on_the_first_data_write_stays_false() {
    for profile in support::PROFILES {
        let mut m = Memory::new(
            Bus {
                fail_write: Some((1, false)),
                ..Bus::default()
            },
            Layout::Cmis { flat: false },
            timing(),
        )
        .unwrap();
        let err = m.write_memory(0x50, 0, 0, 6, &[0xaa; 4]).unwrap_err();
        assert!(
            !err.write_may_have_applied,
            "{}: a write the bus reports as not sent must stay false",
            profile.kernel_driver
        );
    }
}

/// A failed select write is marked `write_may_have_applied` even when the
/// bus reports it as not sent: the spec's error model marks every failed page
/// or bank select write, because the selector register's state is then
/// unknown (spec.md, "Uncertainty at every write boundary"). The selection
/// is also dropped, so the next access re-selects.
#[test]
fn failed_select_write_is_uncertain_even_when_reported_not_sent() {
    // Write 1 is the bank select (126), write 2 the page select (127).
    for (nth, which) in [(1, "bank"), (2, "page")] {
        for profile in support::PROFILES {
            let mut m = Memory::new(
                Bus {
                    fail_write: Some((nth, false)),
                    ..Bus::default()
                },
                Layout::Cmis { flat: false },
                timing(),
            )
            .unwrap();
            let mut out = [0xcc; 2];
            let err = m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap_err();
            assert!(
                err.write_may_have_applied,
                "{}: a failed {which} select write is always marked",
                profile.kernel_driver
            );
            m.read_window(0x50, 0x10, 1, 0, &mut out).unwrap();
            let mut expected = vec![(0x50, vec![126, 1])];
            if nth == 2 {
                expected.push((0x50, vec![127, 0x10]));
            }
            expected.extend([(0x50, vec![126, 1]), (0x50, vec![127, 0x10])]);
            assert_eq!(
                m.into_inner().writes,
                expected,
                "{}: after a failed {which} select the next access re-selects bank and page",
                profile.kernel_driver
            );
        }
    }
}
