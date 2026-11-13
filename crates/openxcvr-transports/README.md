# OpenXCVR transports

Transport building blocks: the backend-agnostic I2C bus contract, memory-window (page) selection, a HID backend (`hid` feature), a Linux i2c-dev backend (`linux-i2c` feature) and transport error classification. Device protocols belong to the family adapters, not here.

Most consumers depend on the [`openxcvr`](../openxcvr/README.md) crate, which re-exports this one; depend on this crate directly only when you need it alone.

Use this crate for generic HID report I/O and addressed module memory access.
Device opcodes, USB identifiers and board wiring belong to device adapters.

`I2cBus` provides synchronous `write` and repeated-start `write_read` operations.
`Memory<B>` implements the existing `openxcvr_core::Transport` interface over
that bus. Supply the module's `Layout` and verified write boundaries/timing.
The `openxcvr::live::discover_layout` helper reuses the existing family dispatcher
and generated capability decoders without reading alarm registers.

- Rust logical reads stay within one lower/upper window. `read_window` and
  `write_window` support the C API's complete 256-byte window with selection
  applied before the first fragment.
- A0/A2 aliases normalize to seven-bit 0x50/0x51. Bank/page dimensions are never
  silently discarded. Supplied layout determines family-level compatibility;
  callers remain responsible for module-specific advertised page availability.
- Reads publish output only on success. Writes may partially apply on error;
  inspect `Error::write_may_have_applied`. The legacy `ModuleError` adapter retains
  detail as text; typed callers use `read_memory`/`write_memory` directly.
- No selector cache or implicit write replay is used. The owner must serialize
  the complete memory operation and other access to the underlying bus.

Enable `hid` for HIDAPI-backed Windows HID, macOS IOHIDManager and Linux hidraw.
Without this feature, offline consumers need no native USB dependencies. Linux
builds need libudev headers; installed access needs appropriate device permissions.
`NativeHid::open_path` takes an exact path from `enumerate`, whose filters are
supplied by the device adapter. Paths are opaque and may change after reconnect.

`NativeHid::held_info(&self)` returns owned `HeldInfo` metadata from that same
held handle, without opening another device, enumerating, escaping a handle or
performing input/output/feature report I/O. It retains the advisory lock and
uses the existing typed error mapping: a failed metadata call fails the whole
inspection. `HeldBusType` is crate-owned. `HeldPlatformFacts::Macos` carries the
held handle's location ID and exclusive-open state; other platforms return
`NotObservable`, not a claim of shared/exclusive access. Metadata retains
hidapi 2.6.7's cache/unknown-field semantics and is not independent identity or
liveness proof. This API does not itself qualify or authorize device access.

Output report IDs are separate from payloads; returned output counts exclude the
ID. Input counts follow HIDAPI: numbered input reports include an ID, unnumbered
ones do not. Report sizes are device-specific. HID writes have no portable hard
cancellation deadline; the owner cannot start another transaction while one is
still outstanding.

Advisory device locks (`DeviceLock`) coordinate cooperating processes per
device node or path. On Unix, lock lifetime follows the underlying open file
description: the lock persists while any descriptor refers to that description,
including one duplicated or inherited by a forked child. Dropping a
`DeviceLock` closes only that handle's descriptor. If another thread concurrently
forks a child (or uses Linux `clone` without `CLONE_FILES`), the child inherits a
descriptor that remains open until successful `execve` (close-on-exec is set)
or exit. The parent can therefore observe transient `Busy` when immediately
re-acquiring the same key during that pre-exec window. No explicit unlock is
issued on drop, and no parent-only release or fork-refusal guarantee is provided.

Run `cargo test -p openxcvr-transports --features hid` from the library root.

## See also

- [`openxcvr` crate](../openxcvr/README.md): the facade most consumers use
- [Architecture](../../docs/architecture.md): how the crates fit together
- [Documentation map](../../docs/README.md)
- [Glossary](../../docs/glossary.md)
