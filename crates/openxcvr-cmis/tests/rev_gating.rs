//! Rev-gating contract tests for CMIS CDB commands (B9 Lane B + B6.5 land).
//!
//! Exercises the public [`CdbOp`] surface and asserts the in-code
//! `available_since` map stays in lockstep with the `available_since:`
//! annotations under `schemas/cmis/cdb-commands/` — the drift test.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use openxcvr_cmis::{CdbError, CdbOp, Rev};

#[test]
fn from_opcode_roundtrips_for_all_ops() {
    for op in CdbOp::ALL {
        assert_eq!(CdbOp::from_opcode(op.opcode()), Some(op));
    }
    assert_eq!(CdbOp::from_opcode(0xFFFF), None);
}

#[test]
fn available_since_matches_known_introduction_revs() {
    // The five commands added in CMIS 5.3.
    for op in [
        CdbOp::RmonStatistics,
        CdbOp::FecSymbolErrorHistogramControl,
        CdbOp::DeviceIdCertificateLpl,
        CdbOp::SetDigestToSignLpl,
        CdbOp::GetDigestSignatureLpl,
    ] {
        assert_eq!(op.available_since(), Rev::CMIS_5_3, "{}", op.name());
    }
    // A representative 4.0-era command.
    assert_eq!(CdbOp::ModuleFeatures.available_since(), Rev::CMIS_4_0);
}

#[test]
fn check_rev_gates_on_introduction() {
    // A 5.3-only command is refused on a 4.0 module, allowed on 5.3.
    assert!(matches!(
        CdbOp::RmonStatistics.check_rev(Rev::CMIS_4_0),
        Err(CdbError::UnsupportedInRev {
            op: CdbOp::RmonStatistics,
            since: Rev::CMIS_5_3,
            detected: Rev::CMIS_4_0,
        })
    ));
    assert!(CdbOp::RmonStatistics.check_rev(Rev::CMIS_5_3).is_ok());
    // A 4.0-era command is allowed on both 4.0 and 5.3.
    assert!(CdbOp::ModuleFeatures.check_rev(Rev::CMIS_4_0).is_ok());
    assert!(CdbOp::ModuleFeatures.check_rev(Rev::CMIS_5_3).is_ok());
    // A CMIS 5.4 command is refused on a 5.3 module, allowed on 5.4.
    assert!(matches!(
        CdbOp::GetModuleTime.check_rev(Rev::CMIS_5_3),
        Err(CdbError::UnsupportedInRev {
            op: CdbOp::GetModuleTime,
            since: Rev::CMIS_5_4,
            detected: Rev::CMIS_5_3,
        })
    ));
    assert!(CdbOp::GetModuleTime.check_rev(Rev::CMIS_5_4).is_ok());
    // A command removed by CMIS 5.4 is allowed through 5.3 and refused at 5.4
    // with the removal error, not the introduction error.
    assert!(CdbOp::DataMonitoringAdvertisements
        .check_rev(Rev::CMIS_5_3)
        .is_ok());
    assert!(matches!(
        CdbOp::DataMonitoringAdvertisements.check_rev(Rev::CMIS_5_4),
        Err(CdbError::RemovedInRev {
            op: CdbOp::DataMonitoringAdvertisements,
            until: Rev::CMIS_5_3,
            detected: Rev::CMIS_5_4,
        })
    ));
    // A 4.0-era command carried into 5.4 stays available at 5.4.
    assert!(CdbOp::ControlPmStatistics.check_rev(Rev::CMIS_4_0).is_ok());
    assert!(CdbOp::ControlPmStatistics.check_rev(Rev::CMIS_5_4).is_ok());
}

fn schema_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/cmis/cdb-commands")
}

/// Parse `(cmd_id, available_since, available_until)` from every command
/// schema. Hard-fails if the directory is unreadable or a file is malformed —
/// there is intentionally no skip-on-missing branch, so a relocated schema
/// tree surfaces as a test failure rather than a silent pass. Files with no
/// top-level `cmd_id:` line (e.g. `_index.yaml`) are skipped; `README.md` is
/// filtered by extension.
fn schema_available_since() -> BTreeMap<u16, (Rev, Option<Rev>)> {
    let dir = schema_dir();
    let entries = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read schema dir {}: {e}", dir.display()));

    let mut map = BTreeMap::new();
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        // Top-level `cmd_id:` only (column 0). Absent on `_index.yaml` and any
        // non-command schema, which we skip.
        let cmd_id = text.lines().find_map(|l| {
            l.strip_prefix("cmd_id:").map(|v| {
                let v = v.trim();
                let hex = v.strip_prefix("0x").unwrap_or(v);
                u16::from_str_radix(hex, 16)
                    .unwrap_or_else(|e| panic!("bad cmd_id {v:?} in {}: {e}", path.display()))
            })
        });
        let cmd_id = match cmd_id {
            Some(c) => c,
            None => continue,
        };

        // Top-level `available_since:` only (column 0), matching the schema's
        // command-level key. A nested occurrence (e.g. under a per-field block)
        // is intentionally ignored — these schemas carry the rev at the command
        // level, and a future per-field `since:` would use that key, not this.
        let since = text
            .lines()
            .find_map(|l| l.strip_prefix("available_since:"))
            .unwrap_or_else(|| panic!("missing available_since: in {}", path.display()));
        let since = since.trim().trim_matches('"');
        let rev = Rev::parse_cmis(since)
            .unwrap_or_else(|| panic!("bad available_since {since:?} in {}", path.display()));

        // Top-level `available_until:` only (column 0), mirroring the command
        // level key. Absent means the command is still defined in the newest
        // modeled revision.
        let until = text
            .lines()
            .find_map(|l| l.strip_prefix("available_until:"))
            .map(|u| {
                let u = u.trim().trim_matches('"');
                Rev::parse_cmis(u)
                    .unwrap_or_else(|| panic!("bad available_until {u:?} in {}", path.display()))
            });

        if let Some(prev) = map.insert(cmd_id, (rev, until)) {
            panic!(
                "duplicate cmd_id 0x{cmd_id:04x} (was {prev:?}) in {}",
                path.display()
            );
        }
    }
    assert!(
        !map.is_empty(),
        "no command schemas found in {}",
        dir.display()
    );
    map
}

#[test]
fn cdb_op_available_since_matches_schema_annotations() {
    let schema = schema_available_since();
    // 0041h is qualified from the 5.3 table only, without earlier-layout inference.
    assert_eq!(schema.get(&0x0041), Some(&(Rev::CMIS_5_3, None)));
    assert_eq!(
        CdbOp::from_opcode(0x0041),
        Some(CdbOp::FirmwareManagementFeatures)
    );

    // 0281h is the removed-command window carrier: defined 4.0..=5.3 only.
    assert_eq!(
        schema.get(&0x0281),
        Some(&(Rev::CMIS_4_0, Some(Rev::CMIS_5_3)))
    );

    // Every schema command maps to a CdbOp with the same since and until...
    for (&cmd_id, &(rev, until)) in &schema {
        let op = CdbOp::from_opcode(cmd_id)
            .unwrap_or_else(|| panic!("schema cmd_id 0x{cmd_id:04x} has no CdbOp variant"));
        assert_eq!(
            op.available_since(),
            rev,
            "available_since drift for {} (0x{cmd_id:04x}): code={:?} schema={:?}",
            op.name(),
            op.available_since(),
            rev
        );
        assert_eq!(
            op.available_until(),
            until,
            "available_until drift for {} (0x{cmd_id:04x}): code={:?} schema={:?}",
            op.name(),
            op.available_until(),
            until
        );
    }

    // ...and every CdbOp has a backing schema command (bijection).
    for op in CdbOp::ALL {
        assert!(
            schema.contains_key(&op.opcode()),
            "CdbOp {} (0x{:04x}) has no schema under schemas/cmis/cdb-commands/",
            op.name(),
            op.opcode()
        );
    }

    // Belt-and-suspenders: identical cardinality means no silently-extra rows
    // on either side.
    assert_eq!(
        schema.len(),
        CdbOp::ALL.len(),
        "schema command count {} != CdbOp variant count {}",
        schema.len(),
        CdbOp::ALL.len()
    );
}
