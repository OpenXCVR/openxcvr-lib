# CMIS 4.0 supplied identity and operation-diagnostic access

Technical support note for consumers of `openxcvr::live` and
`openxcvr_core::access`. This is offline library evidence, not programming or
hardware qualification. No runtime owner is added.

## Source-bound identity reuse

The reported target revision is **CMIS 4.0**, not 5.3. The implementation uses
the existing shared page-00h metadata, not a renamed 5.3 page decoder or a new
revision resolver.

Reuse inspection found versioned `generated::v4_0::CmisLowerMemory`, but **no
versioned page-00h identity module**. The page schema
`schemas/cmis/page-00h-administrative.yaml` is unversioned and cites CMIS 5.3
§8.3, PDF pp.161–167. Its identity fields in
`schemas/cmis/fields/page-00h-administrative-part-01.yaml` cite 5.3 §§8.3.2.1–6,
Tables 8-28/8-29. Those later citations alone do not establish 4.0 support.

The 4.0 authority is CMIS Rev 4.0, PDF pages 94–95 (printed pages 92–93), §8.3:
“Upper page 00h contains static read-only module identification information” and
“shall be implemented for both paged and flat memory implementations”.
Table 8-15 and §§8.3.2–7 establish exactly the fields below:

| Identity field | 4.0 bytes | 4.0 authority / meaning | Existing generated metadata |
|---|---|---|---|
| Vendor name | 129–144 | §8.3.2, required, 16 ASCII characters, left aligned / space padded | `VENDOR_NAME_RANGE = (129,16)` |
| OUI | 145–147 | §8.3.3, required, 3 raw IEEE company-ID bytes, all-zero unspecified | `VENDOR_OUI_RANGE = (145,3)` |
| Part number | 148–163 | §8.3.4, required, 16 ASCII characters, space padded, all-zero unspecified | `VENDOR_PN_RANGE = (148,16)` |
| Revision | 164–165 | §8.3.5, required, 2 ASCII characters, space padded, all-zero unspecified | `VENDOR_REV_RANGE = (164,2)` |
| Serial | 166–181 | §8.3.6, required, 16 ASCII characters, space padded, all-zero unspecified | `VENDOR_SN_RANGE = (166,16)` |
| Date | 182–189 | §8.3.7 / Table 8-17, mandatory 8 ASCII bytes | four generated date subspans |

Table 8-17 gives year 182–183, month 184–185, day 186–187, optional lot
188–189 (may be blank). The generated `DATE_CODE_*_RANGE` constants are
`(182,2), (184,2), (186,2), (188,2)`: contiguous, summing to eight bytes.
`cmis_identity_layout` derives the six ranges from these existing generated
constants; the regression independently compares them to the actual 4.0 table.
The revision remains 4.0 throughout. No schema, generator, generated file,
fixture or golden is changed. No broader page compatibility is inferred.

`decode_cmis_identity_supplied` reuses core `decode_identity`: supplied ASCII
text is trimmed at its edges for spaces/NULs; OUI stays raw. This remains a
permissive extraction API, not ASCII/date-format validation. It has no bus,
selector, module-state or power handle, never constructs a full page and never
fills missing data with zero. Only supplied field spans are needed, including
split observations. Required-input checks still apply to all eight date bytes;
“optional lot” permits blank bytes, not silently fabricated missing bytes.

The pure supplied-identity surface admits six exact revisions (3.0, 4.0, 5.0, 5.1,
5.2, 5.3) through closed match arms in `live::cmis_identity_layout`, core
`decode_identity`, and C `openxcvr_identity_required_ranges` / `openxcvr_decode_identity_ranges`.
Each revision's identical six identity ranges ([129, 145), [145, 148), [148, 164),
[164, 166), [166, 182), [182, 190)) are source-proved by their primary specification
tables:
- CMIS 3.0: `CMIS3p0_Third_Party_Spec.pdf` §1.7.3 Tables 27–29 (PDF pp. 58–60)
- CMIS 4.0: `CMIS4p0_Third_Party_Spec.pdf` §8.3 Tables 8-15–17 (PDF pp. 94–95)
- CMIS 5.0: `CMIS5p0_Third_Party_Spec.pdf` §8.3 Tables 8-22–25 (PDF pp. 122–123)
- CMIS 5.1: `CMIS5p1_Third_Party_Spec.pdf` §8.3 Tables 8-22–25 (PDF pp. 138–139)
- CMIS 5.2: `OIF-CMIS-05.2.pdf` §8.3 Tables 8-23–26 (PDF pp. 141–142)
- CMIS 5.3: `OIF-CMIS-05.3.pdf` §8.3 Tables 8-26–29 (PDF pp. 161–162)

In Rust, `cmis_identity_layout` resolves the ranges through the matching generated
`generated::v3_0..v5_3::CmisPage00hAdministrative` type constants. Other revisions (e.g.
3.1, 4.1, 5.4) and other families remain strictly refused. Revision is a **caller-supplied
fact**: this API does not read or verify lower byte 1. Consumers must bind it to the
current detected module revision, generation and epoch. This is decoding support, not
permission to acquire bytes; live I/O gates (such as 4.0 and 5.3 runtime access) remain
strictly separate and are not relabeled.

## Diagnostic access boundary

`delivered::GET_OPERATION` joins the existing exact-name delivered set.
`evaluate_access` returns `ReadOnly` only for a resolved principal plus
`read_allowed`. Unresolved principal and absent read authorization retain their
existing typed refusals; numeric uid/gid (including zero) cannot repair them.
Write groups neither authorize this diagnostic nor become granted by it.
`write_memory` still independently requires `vendor_programming`, and power,
hold, CDB, cancellation and unknown operation names remain undelivered.

`AccessFacts` is a trusted supplied-fact boundary, **not credential verification**.
It has no authentication attestation, operation id, requester id or record-owner
fact. The daemon must derive principal/read authorization/write membership from
verified platform credentials and configuration, retain requester-scoped lookup
of the existing operation owner, and return only secret-free diagnostic metadata.
The library cannot enforce per-record ownership with this input contract and
does not pretend it can. No ownership or hardware permission is added here.

## What the tests cover

Focused tests cover both 4.0 and unchanged 5.3: schema-derived ranges,
actual-table payloads, reverse-ordered single-byte assembly, unspecified part
number and OUI, a blank lot code, and each missing, short, absent, failed,
uncertain or conflicting field. Fixtures added for this note are synthetic.
The decode does not qualify hardware and does not authorize acquiring the
identity ranges.
