# Managed operations

A daemon or controller that plans every read, state transition and
command with the library and executes the plans itself, starting from
the [documentation map](../README.md).

## When this is your situation

- One process manages many modules: it owns admission, scheduling,
  fencing, a cache with epochs and generations, and the transport.
- Reads must be minimal and batched: the daemon wants the exact byte
  ranges a decode needs, not a full page sweep.
- Writes and commands must be checked before anything moves: raw targets,
  vendor programming and CDB commands are planned, then executed by the
  daemon's own write owner.

The library parses and decides, and never acts on a module on its own
initiative. Every routine below returns data — ranges, plans,
refusals, observations — and the caller executes.

## Shape of the solution

1. Resolve the fields you want at the revision your detection holds:
   `catalog::resolve(rev, &FieldQuery { pattern, lane, page, bank })`
   returns catalog entries with their physical ranges.
2. Ask for the read plan: `observation::field_read_plan(family, rev,
   &entries)`. It is pure — no module byte is read; the ranges come back
   sorted, coalesced and effect-checked (see Limits).
3. Acquire the planned ranges through your own transport, assemble a
   `Snapshot` stamped with your generation and epoch, and decode with
   `observation::observe_fields`. When a plan must be staged in two
   rounds — acquire the deciding bytes first — use
   `observation::field_read_prelude` and `observation::field_read_remaining`:
   the prelude plans the family gate byte plus every entry's
   presence-prerequisite bytes; the remaining round, given those bytes,
   admits or refuses each entry with a typed `FieldAdmissionRefusal` and
   plans exactly the admitted entries' ranges and decode dependencies,
   never re-listing the gate or prerequisite bytes.
4. Plan state transitions with the generated CMIS state machine:
   `Msm::plan_transition_for_rev` returns the edges as data, guards and
   side effects attached; the daemon executes them and re-checks each
   guard live (the recipe is
   [plan a state transition](../cookbook/plan-a-state-transition.md)).
5. Send CMIS commands through the typed CDB surface
   (`CmisModule::cdb_send`), which refuses a command newer than the
   module's revision before any write; the poll budget is the caller's
   (the recipe is
   [issue a typed CDB command](../cookbook/issue-a-typed-cdb-command.md)).
6. Preflight raw access and vendor programming before executing writes:
   `raw::preflight`, `raw_write`'s plan, and
   `programming::plan_named_vendor_programming` below.

The read-planning core, in miniature. The "bus" is a map of ranges the
daemon already holds; nothing here opens a device:

```rust
use openxcvr::catalog::{resolve, FieldQuery, LaneSelection};
use openxcvr::observation::{field_read_plan, observe_fields, ObservationState, ObserveOptions};
use openxcvr::prelude::*;
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp,
    RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The daemon's own detection produced this revision; nothing re-reads it.
    let rev = Rev::CMIS_5_2;
    let mut entries = resolve(
        rev,
        &FieldQuery { pattern: "temp_mon_value", lane: LaneSelection::Unspecified,
                      page: Some("CmisLowerMemory"), bank: 0 },
    )?;
    entries.extend(resolve(
        rev,
        &FieldQuery { pattern: "vcc_mon_voltage", lane: LaneSelection::Unspecified,
                      page: Some("CmisLowerMemory"), bank: 0 },
    )?);

    // Pure plan: the gate byte, then the monitors' coalesced range.
    let plan = field_read_plan(Family::Cmis, rev, &entries)?;
    assert!(plan.refusals.is_empty());
    let mut lower = vec![0u8; 18];
    lower[1] = 0x52; // the plan's gate byte: the module reports CMIS 5.2
    lower[14..18].copy_from_slice(&[0x10, 0x00, 0x80, 0xe8]); // 16 C, 3.3 V

    // The daemon acquires the planned ranges however it chooses, then
    // stamps them with its own generation and epoch.
    let (gen, epoch) = (ModuleGeneration(7), CacheEpoch(2));
    let stamp = ObservationStamp {
        generation: gen,
        epoch,
        started_at: MonoTime(10),
        completed_at: MonoTime(11),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: Vec::new(),
    };
    let snapshot = Snapshot {
        validity_interval: (MonoTime(10), MonoTime(11)),
        generation: gen,
        epoch,
        ranges: plan
            .reads
            .iter()
            .map(|read| RangeObservation {
                range: read.range,
                outcome: RangeOutcome::Bytes(
                    lower[read.range.offset as usize
                        ..(read.range.offset as usize + read.range.len as usize)]
                        .to_vec(),
                ),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            })
            .collect(),
    };
    let observed = observe_fields(
        Family::Cmis, rev, &entries, &snapshot, gen, epoch, &ObserveOptions::default(),
    )?;
    assert!(observed.iter().all(|o| o.state == ObservationState::Valid));
    println!("{} of {} fields decoded", observed.len(), entries.len());
    Ok(())
}
```

Two-round admission fails closed when the module is not what the plan
assumed. Here the gate byte reports a revision the library does not map,
so no data range is planned at all — the daemon stops before acquiring:

```rust
use openxcvr::catalog::{resolve, FieldQuery, LaneSelection};
use openxcvr::observation::{field_read_prelude, field_read_remaining};
use openxcvr::prelude::*;
use openxcvr_core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp,
    Range, RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rev = Rev::CMIS_5_2;
    let entries = resolve(
        rev,
        &FieldQuery { pattern: "temp_mon_value", lane: LaneSelection::Unspecified,
                      page: Some("CmisLowerMemory"), bank: 0 },
    )?;
    let prelude = field_read_prelude(Family::Cmis, rev, &entries)?;
    assert_eq!(prelude.reads[0].purpose, "cmis_revision");

    // The acquired gate byte reports 0x99 — not a revision this library maps.
    let (gen, epoch) = (ModuleGeneration(1), CacheEpoch(1));
    let gate = prelude.reads[0].range;
    let stamp = ObservationStamp {
        generation: gen, epoch,
        started_at: MonoTime(0), completed_at: MonoTime(1),
        source: ObservationSource::Supplied, freshness: Freshness::Fresh,
        consumed_effects: Vec::new(),
    };
    let view = Snapshot {
        validity_interval: (MonoTime(0), MonoTime(1)),
        generation: gen, epoch,
        ranges: vec![RangeObservation {
            range: gate,
            outcome: RangeOutcome::Bytes(vec![0x99]),
            stamp,
            requester: ReadRequester::PassiveInspection,
        }],
    };
    let remaining = field_read_remaining(Family::Cmis, rev, &entries, &view, gen, epoch)?;
    assert!(remaining.reads.is_empty(), "no data range after a refusal");
    println!("refused: {:?}", remaining.refusals[0].reason);
    Ok(())
}
```

Writes are preflighted before anything moves. `raw::preflight`
classifies every requested byte against the CMIS 4.0 catalog; a
successful description is still not write permission, and a refusal
redacts every coordinate. Vendor programming has its own named preflight:

```rust
use openxcvr::prelude::*;
use openxcvr::programming::{plan_named_vendor_programming, FieldValue};
use openxcvr::raw::{preflight, AddressModel, Purpose, RawRegion, RawRequest, RawTarget, WriteBytes};
use openxcvr_core::types::management::Range;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = |offset: u8, len: u16| RawTarget {
        model: AddressModel::Cmis,
        range: Range { address: 0x50, bank: 0, page: 0, offset, len },
    };
    // A raw write of Page 00h vendor-name bytes: classified and admitted.
    let regions = [RawRegion {
        target: target(129, 2),
        write_bytes: Some(WriteBytes::new(b"OK")),
    }];
    let request = RawRequest { revision: Rev::CMIS_4_0, purpose: Purpose::Write, regions: &regions };
    let description = preflight(&request)?;
    assert_eq!(description.targets().len(), 1);

    // The password area (lower byte 118) is refused, whole-range.
    let secret = [RawRegion {
        target: target(118, 1),
        write_bytes: Some(WriteBytes::new(b"x")),
    }];
    let refused = preflight(&RawRequest {
        revision: Rev::CMIS_4_0, purpose: Purpose::Write, regions: &secret,
    })
    .unwrap_err();
    println!("raw refusal: {:?}", refused.reason());

    // Named vendor programming: exact names, exact-width values, one revision.
    let plan = plan_named_vendor_programming(
        Rev::CMIS_4_0,
        Some(&[0; 4]),
        &[0u8; 128],
        &[("vendor_name", FieldValue::Text("ACME PHOTONICS  "))],
    )?;
    assert_eq!(plan.fields[0].name, "vendor_name");
    assert_eq!(plan.fields[0].range.offset, 129);
    Ok(())
}
```

## What the library does not do for you

- It does not acquire, execute, schedule or fence. Plans name ranges;
  your transport reads them. Live module instances, history, cache
  storage, epochs, clocks, admission and execution belong to the caller.
  Poll budgets, retries and
  timeouts are caller policy.
- It does not keep secrets. Raw preflight never hashes, copies or logs
  payloads; the write owner retains responsibility for their lifetime.

## Limits today

- `observation::field_read_plan` is pure — no module bytes; ranges come
  back sorted and coalesced, with upper pages marked optional exactly as
  the basic plan marks them; clear-on-read, acknowledge-on-read and
  write-only entries are excluded with typed `FieldPlanRefusal`s before
  any transfer; output is bounded and oversize queries are refused typed,
  never trimmed. `observation::observe_fields` returns one
  `FieldObservation` per entry, under the same per-entry family rules
  `observe_field` applies. The Rust daemon path is the consumer; there
  is deliberately no C mirror (the daemon is Rust) — the C binding
  exposes the basic-observation plan pair only.
- Stage 1 admission applies on every supplied-input decode before any
  value: the snapshot's reported CMIS revision (absent → `NotSupplied`,
  different or unknown → `UnsupportedRevision`; the plan carries the
  revision byte as a dependency — flat-superset planning, which proves
  nothing about acquisition ordering; the pre-I/O admission is the
  separately staged prelude/remaining pair), presence prerequisites from
  the schemas' field-level `present_when` conditions (SFF-8636 and
  SFF-8472 capability-gated rows included — a disclosed behaviour
  correction: these rows used to decode unconditionally; absent byte →
  `NotSupplied`, value mismatch → `Invalid`, unsupported condition
  syntax → `UnsupportedField`), and selector-content evidence.
- Content-multiplexed rows (Page 14h diagnostics, bytes 192–255) decode
  only through `observe_field_with` / `observe_fields_with` with a
  caller-asserted `SelectorAcquisition` whose coordinates match the
  admitted `AcquisitionRequirement`; without it every such row is
  `NotSupplied`, and a contradicting or non-selecting assertion is
  `Invalid`. The evidence's generation, epoch and settling provenance
  are caller-asserted — the library performs no write, wait or timing
  check. Page-level `present_when` is not represented by this stage.
- The prelude may itself read prerequisite bytes (possibly on an upper
  page) before revision proof; the guarantee is only that the remaining
  round plans no dependent range after a refusal. A refusal names its
  reason typed: missing prelude byte, unsupported reported revision,
  refused compliance, unsatisfied prerequisite, view not selected,
  unsupported presence, read effect.
- The context-free entry points behave exactly as with an empty context.
- Raw preflight supports CMIS 4.0, address model `Cmis`, bank 0, page
  00h only; every other cell is a typed refusal. Vendor programming is
  CMIS 4.0 only.
- `live::cmis_identity_layout` / `live::decode_cmis_identity_supplied`
  decode CMIS 3.0 through 5.3 identity from supplied bytes without I/O
  or page padding
  ([CMIS 4 runtime
  support](../../crates/openxcvr/docs/cmis4-runtime-support.md)); that
  support decodes the supplied bytes and does not qualify hardware or
  authorize acquiring the identity ranges.
  `live::decode_identifier_supplied` decodes the SFF-8024 identifier for
  every cataloged family, reporting unassigned codes as unknown, never
  guessing; the identity decodes carry it as `VendorInfo::identifier`,
  `None` when the snapshot has no byte 0.

## Next

- Recipes: [plan a state transition](../cookbook/plan-a-state-transition.md),
  [issue a typed CDB command](../cookbook/issue-a-typed-cdb-command.md),
  [explain a revision-gated field](../cookbook/explain-a-revision-gated-field.md).
- [API guide](api-guide.md) for the supplied-input entry points and the
  revision report; [error handling](error-handling.md) for refusals.
- [Architecture](../architecture.md) for the crate layering behind the
  plan/execute split.
