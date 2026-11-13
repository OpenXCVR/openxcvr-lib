# Glossary

The terms the openxcvr-lib documentation uses, one definition each. Where a
term has a longer treatment, the entry links to it; the model as a whole is in
[concepts](guides/concepts.md).

## Transceiver and specification terms

| Term | Meaning |
| --- | --- |
| A0h, A2h | The two two-wire addresses of an SFF-8472 module: A0h carries identity, A2h carries diagnostics. |
| CDB | Command Data Block. The CMIS command-and-reply mechanism, used for operations such as firmware information queries. |
| CMIS | Common Management Interface Specification. The management memory map for QSFP-DD, OSFP, DSFP and related modules. |
| Compliance code | The byte in which an SFF-8636 or SFF-8472 module reports the specification revision it implements. |
| Identifier | Byte 0 of a module's memory: an SFF-8024 code naming the form factor. The library uses it to choose a family. |
| Lower memory | Bytes 0–127 of a paged memory map. Always addressable, whichever page is selected. |
| Page | One 128-byte window of upper memory (bytes 128–255), written as a hex number such as 00h or 11h. |
| SFF-8024 | The registry of identifier and other code values shared by the memory-map specifications. |
| SFF-8472 | The management interface for SFP, SFP+ and SFP28 modules. |
| SFF-8636 | The management interface for QSFP+ and QSFP28 modules. |
| VDM | Versatile Diagnostics Monitoring. CMIS's extensible set of monitored observables and their thresholds. |

## Library terms

| Term | Meaning |
| --- | --- |
| Adapter | The per-family type that implements `Module` over a `Transport`: `CmisModule`, `Sff8636Module`, `Sff8472Module`. |
| Binding | A generated surface for a language other than Rust, under `bindings/`. |
| Catalog | The schema-derived table of a family's fields, used to resolve names and revision windows. |
| Detect-once | Opening a module through detection, so later reads use the revision the module reported. See [concepts](guides/concepts.md). |
| Facade | The small hand-written API each binding offers on top of its generated code. The shared contract is in [bindings/README.md](../bindings/README.md). |
| Fail closed | Refusing to proceed on an unknown identifier or revision instead of guessing. |
| Family | One of the specification families the library routes to: CMIS, SFF-8636 or SFF-8472. |
| Field availability | Whether a schema field exists at a given revision. A field outside its window is *gated*. |
| Fixture | A committed memory image used as test input, with recorded provenance. See the [fixture corpus](../tests/fixtures/README.md). |
| Golden | A committed expected decode output that a fixture's decode is compared against. |
| `Module` | The trait of spec-agnostic questions (identity, lanes, flags) every adapter answers. |
| Prelude | `openxcvr::prelude`, the set of re-exports a consumer imports. |
| `Rev` | A family-qualified specification revision, such as `Rev::CMIS_5_2`. |
| Rev-agnostic | Opened without detection. Reads decode through the union of generated fields and no revision report is available. |
| Revision report | The list of schema fields available or gated at the revision detection proved. |
| Schema | The authored YAML description of a memory map: the source of every generated surface. |
| Supplied input | Bytes the caller has already read and hands to a pure decode function. No transport is involved. |
| `Transport` | The trait through which the library reads and writes module memory. The caller provides the implementation. |
| `UnifiedModule` | The spec-agnostic handle that wraps whichever adapter the identifier routed to. |
