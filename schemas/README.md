# Schemas

Authoritative YAML source for transceiver memory maps and shared registries.

## Layout

- `sff-8024/` — shared enum registries imported by the memory-map specs. One directory for SFF-8024 revision 4.14.
- `cmis/` — OIF-CMIS memory pages, CDB commands, and state machines. Revisions 3.0, 4.0, 5.0, 5.1, 5.2, 5.3, and 5.4 are revision windows in this one directory. The index document revision is 5.4 (`docs/stds/cmis/OIF-CMIS-05.4.pdf`). A page's `spec.revision` is the document that page's provenance cites, not the only revision the page models.
- `c-cmis-1.0/` — OIF-C-CMIS-01.0 (Coherent CMIS), one directory for that revision. Its normative CMIS base is Revision 4.0 (§ 8.1). It imports the windowed `cmis/` schemas.
- `c-cmis-1.1/` — OIF-C-CMIS-01.1 (Coherent CMIS), one directory for that revision. Its normative CMIS base is Revision 4.0 (§ 8.1). It imports the windowed `cmis/` schemas.
- `c-cmis-1.2/` — OIF-C-CMIS-01.2 (Coherent CMIS), one directory for that revision. Its normative CMIS base is Revision 4.0 (§ 8.1). It imports the windowed `cmis/` schemas.
- `c-cmis-1.3/` — OIF-C-CMIS-01.3 (Coherent CMIS), one directory for that revision. Its normative CMIS base is Revision 5.2 (§ 8.1). It imports the windowed `cmis/` schemas.
- `c-cmis-1.4/` — OIF-C-CMIS-01.4 (Coherent CMIS), one directory for that revision. Its normative CMIS reference is Revision 5.3 (OIF-CMIS-05.3). It imports the windowed `cmis/` schemas.
- `c-cmis-1.5/` — OIF-C-CMIS-01.5 (Coherent CMIS), one directory for that revision, companion of c-cmis-1.4. Its normative CMIS reference is Revision 5.4 (OIF-CMIS-05.4). It imports the windowed `cmis/` schemas.
- `cmis-ff-1.0/` — OIF-CMIS-FF-01.0 (Formfactor Specific Hardware Management) page 05h, one directory for that revision. The document's normative references are CMIS 5.2 and a CMIS 5.3 editor draft. It imports the windowed `cmis/` schemas.
- `elsfp-1.0/` — OIF-ELSFP-CMIS-01.0 (External Laser SFP CMIS), one directory for that revision. Its normative CMIS reference is Revision 5.3; the firmware section also cites OIF-CMIS-05.2 Table 9-14. It imports the windowed `cmis/` schemas.
- `cmis-lt-1.0/` — OIF-CMIS-LT-01.0 (CMIS Out-of-Band Link Training) pages 08h and 50h-53h, one directory for that revision. Its normative reference is CMIS 5.4, and it requires CMIS 5.3 or a later revision for the support advertisement. It imports the windowed `cmis/` schemas.
- `cmis-vcs-1.0/` — OIF-CMIS-VCS-01.0 (CMIS Versatile Control Set) VCS spaces, parameter registry and code tables, one directory for that revision. Its CMIS base revision is Rev 5.3 (OIF-CMIS-05.3), cited by § 7.1 source [1] and § 6. It imports the windowed `cmis/` schemas.
- `cmis-vcs-1.1/` — OIF-CMIS-VCS-01.1 (CMIS Versatile Control Set) VCS spaces, parameter registry and code tables, one directory for that revision. Its reference is CMIS Rev 5.3; the document also says backwards compatibility is within CMIS 5.x and copies some controls from CMIS 5.2. It imports the windowed `cmis/` schemas.
- `sff-8636/` — SFF-8636 QSFP+/QSFP28 memory pages. Revisions 1.3, 1.4, 1.5, 2.0, 2.5, 2.8, 2.11, and 2.12 are revision windows in this one directory (`Rev::known_for` in `crates/openxcvr-core/src/rev.rs`; 2.5 is the baseline of 2.5–2.7 and 2.8 of 2.8–2.10). Cached documents are `docs/stds/sff-8636/SFF-8636-Rev2.11.pdf` and `SFF-8636-Rev2.12.pdf`. The index document revision is 2.12. Revisions 1.3, 1.4, 1.5, 2.0, 2.5, and 2.8 have no cached document; their windows are derived from the Rev 2.12 change history.
- `sff-8472/` — SFF-8472 SFP/SFP+ memory pages and TOM messages. Runtime revisions 9.3, 9.5, 10.2, 10.4, 11.0, 11.3, 11.4, 12.3, 12.4, and 12.5 are revision windows in this one directory. There is no 12.0 compliance code. The index document revision is 12.5a (`docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf`). The runtime revision is 12.5; `12.5a` is the cited document's label and is not a runtime revision. Cached documents are `SFF-8472-Rev12.3.pdf`, `SFF-8472-Rev12.4.pdf`, and `SFF-8472-Rev12.5a.pdf`. Revisions 9.3, 9.5, 10.2, 10.4, 11.0, 11.3, and 11.4 have no cached document; their windows are derived from the later documents' change history.

## Coverage

One row per modelled revision. "Windows in one set" means `since` / `until` annotations inside the named directory. "One directory" means that revision is a separate namespace, not a window. The document column is the specification the schema was checked against. SFF-8636 revisions 1.3, 1.4, 1.5, 2.0, 2.5, and 2.8, and SFF-8472 revisions 9.3, 9.5, 10.2, 10.4, 11.0, 11.3, and 11.4, have no cached specification file; their windows are derived from the later document's change history and were not checked against that revision's own tables. SFF-8636 2.11 and 2.12, and SFF-8472 12.3, 12.4, and 12.5, name the cached document for that revision.

| Family | Revision | Modelled as | Document checked against |
|---|---|---|---|
| CMIS | 3.0 | windows in `cmis/` | `docs/stds/cmis/CMIS3p0_Third_Party_Spec.pdf` |
| CMIS | 4.0 | windows in `cmis/` | `docs/stds/cmis/CMIS4p0_Third_Party_Spec.pdf` |
| CMIS | 5.0 | windows in `cmis/` | `docs/stds/cmis/CMIS5p0_Third_Party_Spec.pdf` |
| CMIS | 5.1 | windows in `cmis/` | `docs/stds/cmis/CMIS5p1_Third_Party_Spec.pdf` |
| CMIS | 5.2 | windows in `cmis/` | `docs/stds/cmis/OIF-CMIS-05.2.pdf` |
| CMIS | 5.3 | windows in `cmis/` | `docs/stds/cmis/OIF-CMIS-05.3.pdf` |
| CMIS | 5.4 | windows in `cmis/` | `docs/stds/cmis/OIF-CMIS-05.4.pdf` |
| C-CMIS | 01.0 | one directory, `c-cmis-1.0/` | `docs/stds/cmis/OIF-C-CMIS-01.0.pdf` (normative CMIS base: Revision 4.0) |
| C-CMIS | 01.1 | one directory, `c-cmis-1.1/` | `docs/stds/cmis/OIF-C-CMIS-01.1.pdf` (normative CMIS base: Revision 4.0) |
| C-CMIS | 01.2 | one directory, `c-cmis-1.2/` | `docs/stds/cmis/OIF-C-CMIS-01.2.pdf` (normative CMIS base: Revision 4.0) |
| C-CMIS | 01.3 | one directory, `c-cmis-1.3/` | `docs/stds/cmis/OIF-C-CMIS-01.3.pdf` (normative CMIS base: Revision 5.2) |
| C-CMIS | 01.4 | one directory, `c-cmis-1.4/` | `docs/stds/cmis/OIF-C-CMIS-01.4_IA.pdf` (normative CMIS reference: Revision 5.3) |
| C-CMIS | 01.5 | one directory, `c-cmis-1.5/` | `docs/stds/cmis/OIF-C-CMIS-01.5.pdf` (normative CMIS reference: Revision 5.4) |
| CMIS-FF | 01.0 | one directory, `cmis-ff-1.0/` | `docs/stds/cmis/OIF-CMIS-FF-01.0.pdf` (normative references: CMIS 5.2 and a CMIS 5.3 editor draft) |
| CMIS-LT | 01.0 | one directory, `cmis-lt-1.0/` | `docs/stds/cmis/OIF-CMIS-LT-01.0.pdf` (normative reference: CMIS 5.4; requires CMIS 5.3 or later for advertisement) |
| CMIS-VCS | 01.0 | one directory, `cmis-vcs-1.0/` | `docs/stds/cmis/OIF-CMIS-VCS-01.0.pdf` (CMIS base revision: Rev 5.3, OIF-CMIS-05.3) |
| CMIS-VCS | 01.1 | one directory, `cmis-vcs-1.1/` | `docs/stds/cmis/OIF-CMIS-VCS-01.1.pdf` (reference: CMIS Rev 5.3; also CMIS 5.x and controls copied from CMIS 5.2) |
| ELSFP | 01.0 | one directory, `elsfp-1.0/` | `docs/stds/cmis/OIF-ELSFP-CMIS-01.0.pdf` (normative CMIS reference: Revision 5.3; firmware commands also cite OIF-CMIS-05.2 Table 9-14) |
| SFF-8636 | 1.3 | windows in `sff-8636/` | no cached document; windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 1.4 | windows in `sff-8636/` | no cached document; windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 1.5 | windows in `sff-8636/` | no cached document; windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 2.0 | windows in `sff-8636/` | no cached document; windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 2.5 | windows in `sff-8636/` | no cached document; windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 2.8 | windows in `sff-8636/` | no cached document (baseline of 2.8–2.10); windows derived from the change history of `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8636 | 2.11 | windows in `sff-8636/` | `docs/stds/sff-8636/SFF-8636-Rev2.11.pdf` |
| SFF-8636 | 2.12 | windows in `sff-8636/` | `docs/stds/sff-8636/SFF-8636-Rev2.12.pdf` |
| SFF-8472 | 9.3 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 9.5 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 10.2 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 10.4 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 11.0 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 11.3 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 11.4 | windows in `sff-8472/` | no cached document; windows derived from the change history of `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` |
| SFF-8472 | 12.3 | windows in `sff-8472/` | `docs/stds/sff-8472/SFF-8472-Rev12.3.pdf` |
| SFF-8472 | 12.4 | windows in `sff-8472/` | `docs/stds/sff-8472/SFF-8472-Rev12.4.pdf` |
| SFF-8472 | 12.5 | windows in `sff-8472/` | `docs/stds/sff-8472/SFF-8472-Rev12.5a.pdf` (document revision 12.5a; runtime revision 12.5) |
| SFF-8024 | 4.14 | one directory, `sff-8024/` | `docs/stds/sff-8024/SFF-8024-Rev4.14.pdf` |

## Authoring Rules

Transcribe each field from the specification into the schema file, and review
the file against that specification before treating it as done. Catalogs kept
outside this repository are not required to build or test the tree.

Generated snippets are drafts only. A completed schema has no `REVIEW`, `TODO`,
or `TBD` markers and passes both `tools/validate-schemas.py` and the Rust vocabulary
guard. `tools/validate-schemas.py` checks structure and repository hygiene (draft
markers `REVIEW`/`TODO`/`TBD`, `cdb_commands`/`state_machines`/`tom_messages` index target
paths, checksum self-exclusion, import cycle diagnosis) and does not check key
vocabulary; key vocabulary of every schema document, fragment, index and
template is enforced by the Rust guard, run with
`cargo test -p openxcvr-codegen --test strict_schema_vocabulary` (CI runs it via
`cargo test --workspace`). A schema edit is complete only when both pass.

## Metadata-only fields

`emit_value: false` is an optional field-level generation control. It omits a
field from generated decoded members, getters/setters, automatic serialization,
and JSON Schema value properties in every value emitter. Omitted or `true`
preserves legacy emission, including historical Reserved bindings. It uses the
existing metadata inheritance/resolution rules for templates and revisions.

This is not a physical-read permission or an effect assertion. The field stays
in the public catalog, documentation, and complete bit-classification inputs;
its type, category, tags, revision window and read-effect authority are unchanged.
Use it for explicitly metadata-only rows, not to infer safety from `access: ro`
or to suppress historical APIs by category.
