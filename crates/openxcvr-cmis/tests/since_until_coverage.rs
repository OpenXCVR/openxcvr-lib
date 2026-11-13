//! Since/until revision-window coverage enforcement.
//!
//! Enumerates EVERY `since` / `until` marking in `schemas/cmis` — page
//! fields (`fields/*.yaml`, through each page's `field_includes`) and state
//! machines (`state-machines/*.yaml`, per state and transition) — and fails
//! unless a generated-reader or state-machine assertion exercises it:
//!
//! * an emitted page field is exercised through the generated per-revision
//!   readers: it must decode as a struct member in exactly the revisions
//!   its window covers (`since` inclusive, `until` exclusive, matching the
//!   shared resolver semantics), and in no others;
//! * a metadata-only field (reserved-bit rows, `emit_value: false`) never
//!   decodes as a member in any revision, so its window is exercised
//!   through the generated field catalog: resolution must succeed in
//!   exactly the covered revisions;
//! * a state-machine marking is exercised through the generated behaviour
//!   tables: the state/transition must be present in `for_revision`'s table
//!   in exactly the covered revisions.
//!
//! Nothing here reads the revision matrix or any fixture: the inventory
//! comes from the schemas and the assertions from the generated code, so a
//! marking that lands without its generated surface (or a generated surface
//! that drifts from the schema window) fails this suite. A synthetic schema
//! fragment carrying one marking no assertion can exercise must make the
//! coverage check fail; the self-test asserts exactly that failure.

use std::path::{Path, PathBuf};

use openxcvr_cmis::generated::catalog::CATALOG;
use openxcvr_cmis::generated::fsm::{cmis_data_path_state_machine, cmis_module_state_machine};
use openxcvr_cmis::generated::{v3_0, v4_0, v5_0, v5_1, v5_2, v5_3, v5_4};
use openxcvr_core::field_catalog::{resolve, FieldQuery, LaneSelection};
use openxcvr_core::rev::Rev;

/// The complete CMIS catalog revision domain; every window is
/// exercised across all of it, in and out.
const REVS: [Rev; 7] = [
    Rev::CMIS_3_0,
    Rev::CMIS_4_0,
    Rev::CMIS_5_0,
    Rev::CMIS_5_1,
    Rev::CMIS_5_2,
    Rev::CMIS_5_3,
    Rev::CMIS_5_4,
];

fn schema_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis")
}

fn parse_yaml(path: &Path) -> Result<serde_norway::Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_norway::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))
}

/// `since` inclusive, `until` exclusive — the shared resolver semantics
/// (`openxcvr-codegen` ir/version.rs documents `until` as "field is absent
/// at and after this rev").
fn window_covers(since: Option<Rev>, until: Option<Rev>, rev: Rev) -> bool {
    since.is_none_or(|s| rev >= s) && until.is_none_or(|u| rev < u)
}

fn marking_rev(value: &serde_norway::Value, key: &str) -> Result<Option<Rev>, String> {
    match value.get(key) {
        None | Some(serde_norway::Value::Null) => Ok(None),
        Some(serde_norway::Value::String(rev)) => Rev::parse_cmis(rev)
            .map(Some)
            .ok_or_else(|| format!("unparseable {key} revision {rev:?}")),
        Some(other) => Err(format!("bad {key} value {other:?}")),
    }
}

// ─── inventory: every since/until marking in schemas/cmis ───────────────────

/// One page-field marking: the page schema id, the field's schema name,
/// the struct member names it can decode under (instance fields gain the
/// index suffix), and the declared window.
struct PageMarking {
    page_id: String,
    field: String,
    members: Vec<String>,
    since: Option<Rev>,
    until: Option<Rev>,
}

/// One state-machine marking: the machine schema id, the state or
/// transition id, and the declared window.
struct MachineMarking {
    machine: String,
    item: String,
    is_transition: bool,
    since: Option<Rev>,
    until: Option<Rev>,
}

fn enumerate_page_markings(root: &Path) -> Result<Vec<PageMarking>, String> {
    let mut markings = Vec::new();
    let mut page_files = 0usize;
    for entry in std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))? {
        let path = entry
            .map_err(|e| format!("{}: {e}", root.display()))?
            .path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let doc = parse_yaml(&path)?;
        let Some(includes) = doc.get("field_includes").and_then(|v| v.as_sequence()) else {
            continue; // not a page document (index, README, ...)
        };
        page_files += 1;
        let page_id = doc
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: page document without id", path.display()))?
            .to_owned();
        for include in includes {
            let include = include
                .as_str()
                .ok_or_else(|| format!("{}: non-string field include", path.display()))?;
            let fields_path = root.join(include);
            let fields_doc = parse_yaml(&fields_path)?;
            let fields = fields_doc
                .get("fields")
                .and_then(|v| v.as_sequence())
                .ok_or_else(|| format!("{}: no fields list", fields_path.display()))?;
            for field in fields {
                let Some(name) = field.get("name").and_then(|v| v.as_str()) else {
                    return Err(format!("{}: field without a name", fields_path.display()));
                };
                if field.get("since").is_none() && field.get("until").is_none() {
                    continue;
                }
                let since = marking_rev(field, "since")?;
                let until = marking_rev(field, "until")?;
                let mut members = vec![name.to_owned()];
                if let Some(index) = field.get("instances").and_then(|v| v.get("index")) {
                    let index = index
                        .as_str()
                        .ok_or_else(|| format!("{name}: instances.index must be a name"))?;
                    members.push(format!("{name}_{index}"));
                }
                markings.push(PageMarking {
                    page_id: page_id.clone(),
                    field: name.to_owned(),
                    members,
                    since,
                    until,
                });
            }
        }
    }
    if page_files == 0 {
        return Err(format!("no page schemas under {}", root.display()));
    }
    Ok(markings)
}

fn enumerate_machine_markings(root: &Path) -> Result<Vec<MachineMarking>, String> {
    let dir = root.join("state-machines");
    let mut markings = Vec::new();
    let mut machine_files = 0usize;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        machine_files += 1;
        let doc = parse_yaml(&path)?;
        let machine = doc
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: machine without id", path.display()))?
            .to_owned();
        for (key, is_transition) in [("transitions", true), ("states", false)] {
            let Some(items) = doc.get(key).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for item in items {
                if item.get("since").is_none() && item.get("until").is_none() {
                    continue;
                }
                let Some(id) = item
                    .get("id")
                    .or_else(|| item.get("name"))
                    .and_then(|v| v.as_str())
                else {
                    return Err(format!("{}: {key} entry without an id", path.display()));
                };
                markings.push(MachineMarking {
                    machine: machine.clone(),
                    item: id.to_owned(),
                    is_transition,
                    since: marking_rev(item, "since")?,
                    until: marking_rev(item, "until")?,
                });
            }
        }
    }
    if machine_files == 0 {
        return Err(format!("no state machines under {}", dir.display()));
    }
    Ok(markings)
}

// ─── generated surfaces the markings are exercised through ─────────────────

macro_rules! page_debug_for_rev {
    ($page:ident, $rev:expr) => {
        match $rev {
            Rev::CMIS_3_0 => format!("{:?}", v3_0::$page::decode(&[0u8; 128])),
            Rev::CMIS_4_0 => format!("{:?}", v4_0::$page::decode(&[0u8; 128])),
            Rev::CMIS_5_0 => format!("{:?}", v5_0::$page::decode(&[0u8; 128])),
            Rev::CMIS_5_1 => format!("{:?}", v5_1::$page::decode(&[0u8; 128])),
            Rev::CMIS_5_2 => format!("{:?}", v5_2::$page::decode(&[0u8; 128])),
            Rev::CMIS_5_3 => format!("{:?}", v5_3::$page::decode(&[0u8; 128])),
            Rev::CMIS_5_4 => format!("{:?}", v5_4::$page::decode(&[0u8; 128])),
            _ => unreachable!("REVS is the complete CMIS catalog domain"),
        }
    };
}

/// The decoded page's Debug rendering at `rev`: its struct members are the
/// generated reader surface. An unknown page id is an error, never a
/// silent pass — a new marked page forces its dispatch here.
fn page_debug(page_id: &str, rev: Rev) -> Result<String, String> {
    match page_id {
        "CmisLowerMemory" => Ok(page_debug_for_rev!(CmisLowerMemory, rev)),
        "CmisPage00hAdministrative" => Ok(page_debug_for_rev!(CmisPage00hAdministrative, rev)),
        "CmisPage01hAdvertising" => Ok(page_debug_for_rev!(CmisPage01hAdvertising, rev)),
        "CmisPage04hLaserCapabilities" => {
            Ok(page_debug_for_rev!(CmisPage04hLaserCapabilities, rev))
        }
        "CmisPage0ChModuleManagement" => Ok(page_debug_for_rev!(CmisPage0ChModuleManagement, rev)),
        "CmisPage0DhFirmwareManagement" => {
            Ok(page_debug_for_rev!(CmisPage0DhFirmwareManagement, rev))
        }
        "CmisPage11hLaneStatus" => Ok(page_debug_for_rev!(CmisPage11hLaneStatus, rev)),
        "CmisPage12hTunableLaser" => Ok(page_debug_for_rev!(CmisPage12hTunableLaser, rev)),
        "CmisPage14hDiagnosticsResults" => {
            Ok(page_debug_for_rev!(CmisPage14hDiagnosticsResults, rev))
        }
        "CmisPage1DhHostLaneSwitching" => {
            Ok(page_debug_for_rev!(CmisPage1DhHostLaneSwitching, rev))
        }
        "CmisPage60hLaneDatapathManagementExtensions" => Ok(page_debug_for_rev!(
            CmisPage60hLaneDatapathManagementExtensions,
            rev
        )),
        "CmisPage61hLaneDatapathMonitoring" => {
            Ok(page_debug_for_rev!(CmisPage61hLaneDatapathMonitoring, rev))
        }
        "CmisPage62hLaneSupervisionThresholds" => Ok(page_debug_for_rev!(
            CmisPage62hLaneSupervisionThresholds,
            rev
        )),
        "CmisPage6DhMediaLaneSwitching" => {
            Ok(page_debug_for_rev!(CmisPage6DhMediaLaneSwitching, rev))
        }
        other => Err(format!(
            "page {other} carries a since/until marking but has no generated-reader \
             dispatch in this suite; add it"
        )),
    }
}

/// Whether the field decodes as a struct member at `rev` (any of its
/// possible member names).
fn member_present(marking: &PageMarking, rev: Rev) -> Result<bool, String> {
    let debug = page_debug(&marking.page_id, rev)?;
    Ok(marking
        .members
        .iter()
        .any(|m| debug.contains(&format!("{m}:"))))
}

/// Whether the generated field catalog resolves the field at `rev` (the
/// metadata-only surface: reserved-bit rows never decode as members).
fn catalog_present(marking: &PageMarking, rev: Rev) -> bool {
    resolve(
        &CATALOG,
        rev,
        &FieldQuery {
            pattern: &marking.field,
            lane: LaneSelection::Unspecified,
            page: Some(&marking.page_id),
            bank: 0,
        },
    )
    .is_ok()
}

/// Whether the machine's generated table carries the state/transition at
/// `rev`. An unknown machine (or an undispatched marking kind) is an
/// error, never a silent pass.
fn machine_item_present(marking: &MachineMarking, rev: Rev) -> Result<bool, String> {
    let unknown = |machine: &str| {
        format!(
            "state machine {machine} carries a since/until marking but has no generated \
             table dispatch in this suite; add it"
        )
    };
    match marking.machine.as_str() {
        "CmisModuleStateMachine" => {
            let table = cmis_module_state_machine::for_revision(rev)
                .ok_or_else(|| format!("{rev:?} outside the machine's catalog domain"))?;
            if marking.is_transition {
                Ok(table
                    .transitions
                    .iter()
                    .any(|transition| transition.id == marking.item))
            } else {
                Ok(table
                    .states
                    .iter()
                    .any(|state| state.state.schema_name() == marking.item))
            }
        }
        "CmisDataPathStateMachine" => {
            let table = cmis_data_path_state_machine::for_revision(rev)
                .ok_or_else(|| format!("{rev:?} outside the machine's catalog domain"))?;
            if marking.is_transition {
                Ok(table
                    .transitions
                    .iter()
                    .any(|transition| transition.id == marking.item))
            } else {
                // The v1 DPSM schema names states in PascalCase the plain
                // generated enum cannot key back; no state-level marking
                // exists today, so one landing is an explicit extension
                // demand, not a silent pass.
                Err(format!(
                    "state-level since/until on {}::{} has no name keying in this \
                     suite; add it",
                    marking.machine, marking.item
                ))
            }
        }
        other => Err(unknown(other)),
    }
}

// ─── the coverage check itself ──────────────────────────────────────────────

/// Every enumerated marking must be exercised by a generated surface, in
/// and out of its window. Returns the collected violations as an error —
/// the real run expects none, the self-test expects one.
fn check_coverage(root: &Path) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();

    let pages = enumerate_page_markings(root)?;
    if pages.is_empty() {
        return Err(format!(
            "no since/until markings enumerated from pages under {}",
            root.display()
        ));
    }
    for marking in &pages {
        let member_revs: Vec<Rev> = REVS
            .iter()
            .copied()
            .filter(|&rev| {
                member_present(marking, rev).unwrap_or_else(|e| {
                    errors.push(format!("{}::{}: {e}", marking.page_id, marking.field));
                    false
                })
            })
            .collect();
        if member_revs.is_empty() {
            // Metadata-only field: the catalog is its generated surface.
            for rev in REVS {
                let expected = window_covers(marking.since, marking.until, rev);
                if catalog_present(marking, rev) != expected {
                    errors.push(format!(
                        "{}::{}: catalog resolution at {rev:?} disagrees with the window \
                         (since {:?}, until {:?})",
                        marking.page_id, marking.field, marking.since, marking.until
                    ));
                }
            }
        } else {
            for rev in REVS {
                let expected = window_covers(marking.since, marking.until, rev);
                let present = member_revs.contains(&rev);
                if present != expected {
                    errors.push(format!(
                        "{}::{}: generated reader presence at {rev:?} disagrees with the \
                         window (since {:?}, until {:?}): present={present}",
                        marking.page_id, marking.field, marking.since, marking.until
                    ));
                }
            }
        }
    }

    let machines = enumerate_machine_markings(root)?;
    if machines.is_empty() {
        return Err(format!(
            "no since/until markings enumerated from state machines under {}",
            root.display()
        ));
    }
    for marking in &machines {
        for rev in REVS {
            let expected = window_covers(marking.since, marking.until, rev);
            let present = machine_item_present(marking, rev).unwrap_or_else(|e| {
                errors.push(format!("{}::{}: {e}", marking.machine, marking.item));
                false
            });
            if present != expected {
                errors.push(format!(
                    "{}::{}: generated table presence at {rev:?} disagrees with the window \
                     (since {:?}, until {:?})",
                    marking.machine, marking.item, marking.since, marking.until
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} since/until coverage violation(s):\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        ))
    }
}

#[test]
fn every_since_and_until_marking_in_schemas_cmis_is_exercised() {
    check_coverage(&schema_root()).expect(
        "every since/until marking in schemas/cmis (pages and state machines) must be \
         exercised by the generated readers, the field catalog or the behaviour tables",
    );
}

#[test]
fn a_synthetic_unexercised_marking_makes_the_check_fail() {
    let tmp = TempDirGuard::new();
    let root = tmp.path().to_path_buf();
    // A synthetic page carrying one ghost marking: the field is in no
    // generated reader and no catalog row, so nothing can exercise it.
    std::fs::write(
        root.join("lower-memory.yaml"),
        "schema: openxcvr.page.v2\nkind: page\nid: CmisLowerMemory\nfield_includes:\n  \
         - fields/ghost.yaml\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("fields")).unwrap();
    std::fs::write(
        root.join("fields/ghost.yaml"),
        "fields:\n  - name: ghost_field_never_modeled\n    since: \"5.2\"\n",
    )
    .unwrap();
    // A synthetic machine marking with the same property.
    std::fs::create_dir(root.join("state-machines")).unwrap();
    std::fs::write(
        root.join("state-machines/ghost-machine.yaml"),
        [
            "id: CmisModuleStateMachine",
            "transitions:",
            "  - id: ghost_transition_never_modeled",
            "    since: \"4.0\"",
            "states: []",
        ]
        .join("\n"),
    )
    .unwrap();
    let err = check_coverage(tmp.path()).expect_err("the ghost markings must fail the check");
    assert!(
        err.contains("ghost_field_never_modeled"),
        "the failure must name the unexercised page marking: {err}"
    );
    assert!(
        err.contains("ghost_transition_never_modeled"),
        "the failure must name the unexercised machine marking: {err}"
    );
}

/// A throwaway directory removed on drop (scratch never persists).
struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!(
            "since-until-coverage-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
