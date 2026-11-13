# Live monitoring

Poll a live module through a transport you own, starting from the
[documentation map](../README.md).

## When this is your situation

- You hold the bus: Linux `/dev/i2c-N`, a BMC's I2C master, a switch
  ASIC SDK, a test rig.
- You want identity read once, telemetry and flags polled on an interval
  you choose, and thresholds evaluated on the fresh readings.
- A single process owns one module; no scheduler, cache or epoch bookkeeping
  is needed. When it is, that is the
  [managed operations](managed-operations.md) path.

## Shape of the solution

1. Implement the `Transport` trait: `read`, `write`, `probe`. One call
   addresses (`slave_addr`, `page`, `bank`, `offset`). Addresses are
   `0x50` for CMIS and SFF-8636, `0xA0`/`0xA2` for SFF-8472 base and
   diagnostics. Lower memory is `page 0`, offsets 0–127; upper pages are
   `page NN`, offsets 128–255. Your transport performs page selection
   when the hardware needs an explicit select write.
2. Call `detect(&mut transport)` once. It reads byte 0 and the family's
   revision byte and returns `Detected { kind, rev }` — or an error; it
   never assumes a revision.
3. Open the family adapter over your transport: `Sff8472Module::open`,
   `Sff8636Module::open` (detect-once) or `CmisModule::new`. Keep the
   concrete handle when you need per-spec methods; box it into
   `UnifiedModule::open_custom` when the `Module` surface is enough.
4. Poll on your schedule: `vendor_info` once, then `lanes()`,
   `flags()`, `temperature_celsius()`, `supply_voltage_v()` as needed.
   Every call reads through your transport when it is made.
5. Evaluate thresholds with `evaluate(measurement, &Thresholds)` (the
   SFF-8636 and SFF-8472 adapters also offer `evaluate_thresholds` with
   the module's stored threshold bytes), and classify failures with
   `openxcvr::classify::is_retryable` / `is_timeout`. The library never
   retries, sleeps or keeps a cache; the poll loop and its policy are
   yours.

The stand-in bus below serves three pages from memory. It is the same
trait a hardware backend implements:

```rust
use openxcvr::core::transport::Transport;
use openxcvr::prelude::*;
use openxcvr::CmisModule;
use openxcvr_cmis::module::CmisTransport;

struct ScriptedBus {
    lower: [u8; 128],
    page00: [u8; 128],
    page11: [u8; 128],
}

impl Transport for ScriptedBus {
    fn read(&mut self, _addr: u8, page: u8, _bank: u8, offset: u8, buf: &mut [u8])
        -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x00, 128..=255) => &self.page00[(offset - 128) as usize..],
            (0x11, 128..=255) => &self.page11[(offset - 128) as usize..],
            _ => {
                return Err(ModuleError::Io(
                    format!("unread page {page:#04x} offset {offset}").into(),
                ))
            }
        };
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }
    fn write(&mut self, _a: u8, _p: u8, _b: u8, _o: u8, _d: &[u8])
        -> Result<(), ModuleError> {
        Err(ModuleError::Io("monitoring bus is read-only".into()))
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
impl CmisTransport for ScriptedBus {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lower = [0u8; 128];
    lower[0] = 0x18; // QSFP-DD
    lower[1] = 0x52; // CMIS 5.2
    lower[14..16].copy_from_slice(&0x1000u16.to_be_bytes()); // 16.0 C
    let mut page00 = [0u8; 128];
    page00[1..17].copy_from_slice(b"ACME PHOTONICS  "); // VendorName
    let mut bus = ScriptedBus { lower, page00, page11: [0u8; 128] };

    let detected = detect(&mut bus)?;
    assert_eq!((detected.kind, detected.rev), (SpecKind::Cmis, Rev::CMIS_5_2));
    let module = CmisModule::new(bus)?; // re-reads lower, freezes the rev
    println!("vendor {}", module.vendor_info()?.name); // Page 00h
    println!("temp {:?}", module.temperature_celsius()); // lower memory
    assert!(!module.flags()?.temp_high_alarm); // lower + Page 11h
    Ok(())
}
```

A poll that fails classifies before it retries. `is_retryable` is true
only for `Busy`, `Timeout` and `Nack`, and only when a write is known
not to have applied; delay and budget stay with you:

```rust
use openxcvr::prelude::*;
use openxcvr_transports::{Error, ErrorKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let thresholds = Thresholds {
        high_alarm: Some(80.0),
        low_alarm: Some(-5.0),
        high_warning: Some(75.0),
        low_warning: None,
    };
    // A good tick: evaluate the fresh reading against stored bounds.
    let decision = evaluate(78.0, &thresholds);
    assert!(decision.high_warning);
    // A failed tick: classify the ModuleError the read returned.
    let timeout = Error::new(ErrorKind::Timeout, "deadline");
    let failed = ModuleError::Io(IoError::with_source(timeout.to_string(), timeout));
    assert!(openxcvr::classify::is_retryable(&failed));
    assert!(openxcvr::classify::is_timeout(&failed));
    // A write that may have applied is never replayed; re-read instead.
    let uncertain = Error::new(ErrorKind::Timeout, "deadline").uncertain();
    let written = ModuleError::Io(IoError::with_source(uncertain.to_string(), uncertain));
    assert!(!openxcvr::classify::is_retryable(&written));
    Ok(())
}
```

When a poll must read exactly the bytes a decode needs — a slow bus, a
rate-limited rig — `observation::basic_read_prelude` and
`basic_read_remaining` plan the acquisition and `observe_basic` decodes
it. The plan names the gate byte first (CMIS lower byte 1), then, given
that byte, the monitors' exact ranges; upper pages are marked optional
because flat-memory modules do not implement them:

```rust
use openxcvr::observation::{basic_read_prelude, basic_read_remaining, observe_basic};
use openxcvr::prelude::*;
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp,
    Range, RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (gen, epoch) = (ModuleGeneration(1), CacheEpoch(1));
    let stamp = ObservationStamp {
        generation: gen,
        epoch,
        started_at: MonoTime(0),
        completed_at: MonoTime(1),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: Vec::new(),
    };
    let mut snapshot = |ranges: Vec<(u8, u8, u8, u8, Vec<u8>)>| Snapshot {
        validity_interval: (MonoTime(0), MonoTime(1)),
        generation: gen,
        epoch,
        ranges: ranges
            .into_iter()
            .map(|(page, offset, bank, address, bytes)| RangeObservation {
                range: Range { address, bank, page, offset, len: bytes.len() as u16 },
                outcome: RangeOutcome::Bytes(bytes),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    };

    // Round 1: acquire the bytes that decide admission.
    let mut lower = vec![0u8; 18];
    lower[1] = 0x52; // CMIS 5.2
    lower[14..18].copy_from_slice(&[0x10, 0x00, 0x80, 0xe8]); // 16 C, 3.3 V
    let prelude = basic_read_prelude(Family::Cmis, Rev::CMIS_5_2)?;
    let view = snapshot(vec![(0, 1, 0, 0x50, vec![0x52])]);
    // Round 2: given the prelude bytes, the exact remaining ranges.
    let remaining = basic_read_remaining(Family::Cmis, Rev::CMIS_5_2, &view, gen, epoch)?;
    let mut acquired = vec![(0, 1, 0, 0x50, vec![0x52])]; // the gate byte we hold
    for read in &remaining {
        if read.optional {
            continue; // upper pages: a flat-memory module does not implement them
        }
        let start = read.range.offset as usize;
        let bytes = lower[start..start + read.range.len as usize].to_vec();
        acquired.push((
            read.range.page,
            read.range.offset,
            read.range.bank,
            read.range.address,
            bytes,
        ));
    }
    let view = snapshot(acquired);
    let observed = observe_basic(Family::Cmis, Rev::CMIS_5_2, &view, gen, epoch, Default::default())?;
    assert!(format!("{:?}", observed.temperature).contains("Valid"));
    assert!(format!("{:?}", observed.supply_voltage).contains("Valid"));
    println!("prelude byte: {}", prelude[0].purpose);
    Ok(())
}
```

## What the library does not do for you

- It does not open a device on its own initiative. Your transport owns the
  bus, the handle, and page selection; `probe` is yours to implement. The
  optional `linux-i2c` transport is a mechanism you direct: it opens the
  i2c-dev path you pass to `LinuxI2cTransport::new`, and its page-select
  hold and post-write acknowledge poll sleep for a bounded interval.
- It does not retry, schedule or keep a cache. Every `Module` call reads
  the transport when it is made; a poll loop is caller code.
- It does not write. Polling is read-only; writes go through the planned
  paths in [managed operations](managed-operations.md).
- It does not decide what a threshold crossing means for your system.
  `evaluate` returns booleans; alarming, logging and mitigation are yours.

## Limits today

- The unified handle covers CMIS through unbanked Pages 00h–05h (plus
  Page 11h when attached); banked CMIS pages are not implemented.
- There is no CMIS threshold-evaluation entry point; CMIS Page 02h
  monitors are ordinary decoded fields. SFF-8636 and SFF-8472 have
  `evaluate_thresholds`.
- The basic-observation plan and decode have C mirrors
  (`openxcvr_basic_read_prelude` / `openxcvr_basic_read_remaining`), so a
  C poll loop can share the same acquisition plan.
- Observation decoding is split between `openxcvr-core` and `openxcvr`
  with near-identical module descriptions — a known seam.

## Next

- Recipes: [read lane flags](../cookbook/read-lane-flags.md),
  [evaluate a threshold](../cookbook/evaluate-a-threshold.md),
  [handle a decode or unsupported error](../cookbook/handle-a-decode-or-unsupported-error.md).
- [API guide](api-guide.md) for the `Transport` contract and the
  adapters; [error handling](error-handling.md) for the retry rules.
- The `linux-i2c` feature supplies `LinuxI2cTransport` for `/dev/i2c-N`:
  `cargo run -p openxcvr --features linux-i2c --example linux-i2c`
  ([installation](installation.md)).
