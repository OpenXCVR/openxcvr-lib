# Task cookbook

Copy-paste recipes for the jobs a consumer actually does. Rust is the
primary snippet. Python, Go, and TypeScript appear where the
[facade contract](../bindings/README.md#consumer-facade-contract)
implements the same operation. Architecture and error meanings are the
[consumer guide](consumer-guide.md) and the
[error catalog](error-catalog.md). Per-field tables are the generated
[schema reference](reference/index.md); this page does not restate them.
A zero-setup tour of one embedded sample is the
[quickstart](guides/quickstart.md).

Every byte below is hand-built. Nothing here is a vendor capture. The
identity strings match the committed synthetic CMIS 5.2 page described in
[`tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.md`](../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.md).
The library parses and decides; it does not open an I2C device. Snippets
that need a bus take [`BufTransport`](../crates/openxcvr-core/src/transport.rs)
or a transport the snippet itself implements.

Rust snippets are `fn main() -> Result<(), Box<dyn std::error::Error>>`
bodies. `openxcvr::prelude` covers the unified module, detection, errors,
revisions, and threshold decisions. Recipes that name `openxcvr_cmis`
depend on that crate as well (`crates/openxcvr-cmis`). Neither crate is
published; depend on it by path inside this workspace.

Every Rust block in a recipe is compiled and run by
`cargo test --doc -p openxcvr`; a recipe that stops matching the API fails
the workspace tests. The Python, Go and TypeScript blocks are not executed
by that check.

## Recipes

| Recipe | Goal |
| --- | --- |
| [Identify family and revision](cookbook/identify-family-and-revision.md) | Route the SFF-8024 identifier and read the family's revision byte, without assuming a later revision. |
| [Read vendor and serial](cookbook/read-vendor-and-serial.md) | Read the vendor name, part, and serial from page 00h. The spec revision stays on the module handle; `VendorInfo.rev` is the vendor's hardware-revision string, not the spec revision. |
| [Decode VDM](cookbook/decode-vdm.md) | List advertised VDM instances, decode one sample by observable type, and read the threshold quad for that instance. Group count comes from Page 2Fh byte 128 bits 1:0 (`0` means group 1 only). An inactive descriptor slot is type `0` and is skipped. |
| [Evaluate a threshold](cookbook/evaluate-a-threshold.md) | Turn one measurement and one threshold set into alarm/warning booleans. Bounds are inclusive. A `None` / nil bound does not trip. |
| [Read lane flags](cookbook/read-lane-flags.md) | Read per-lane Rx LOS and Tx fault. This recipe uses SFF-8636 lower memory, which is the family the facade exposes as its own lane-flag call. CMIS lane flags are `CmisModule::lanes` (Page 11h) and, on the facade, the `lanes` array inside `decode_cmis` when key `11h` is present. |
| [Plan a state transition](cookbook/plan-a-state-transition.md) | Ask the generated CMIS module state machine for the edges from LowPwr to Ready. The plan is data. Nothing here reads or writes a module. |
| [Issue a typed CDB command](cookbook/issue-a-typed-cdb-command.md) | Send CMIS CDB 0100h (Get Firmware Information) as a typed command. The snippet's transport is the stand-in for the caller's bus: it records the Page 9Fh write and returns a scripted reply. The reply bytes are synthetic, not a module. |
| [Explain a revision-gated field](cookbook/explain-a-revision-gated-field.md) | When a read says a field is not exposed, ask the catalog whether that name exists at another revision. The report does not touch the module. |
| [Handle a decode or unsupported error](cookbook/handle-a-decode-or-unsupported-error.md) | Branch on the error variant. Messages are for logs. |
| [Test without hardware](cookbook/test-without-hardware.md) | Exercise code that takes a [`Module`](../crates/openxcvr-core/src/module.rs) without a bus. [`MockModule`](../crates/openxcvr-core/src/testkit.rs) returns only stored values. An accessor left unset is `ModuleError::Unsupported`, not a zero reading. |
