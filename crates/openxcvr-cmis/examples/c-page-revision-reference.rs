//! Test-only Rust reference for the C/Rust revision-checked page
//! differential (bindings/c/tests/checked_page_differential.py).
//!
//! NOT a distributed CLI: an `examples/` binary, invoked as
//!
//!     c-page-revision-reference <01h|11h> <rev-byte-decimal> <seed>
//!
//! with the same seed contract as `bindings/c/tests/checked_page_runner.c`
//! (decimal LCG seed, or `dpsm-<hex nibble>` mirroring the nibble into the
//! first four page bytes). Decoding goes through the generated per-revision
//! Rust decoders (`v3_0` .. `v5_3`) — never copied bit extraction — and the
//! output is the canonical projection the differential script compares:
//!
//!     page <01h|11h>
//!     rev <decimal>
//!     dump <single-line Debug of the decoded per-revision struct>
//!     interp dp_state_host_lane[<i>] <name|Unknown<n>>   (Page 11h only)
//!
//! The `dump` line is parsed by the differential script, which derives the
//! member-set and value comparison from the generated headers and enum
//! sources instead of a hand-maintained per-revision field list. For
//! `dp_state_host_lane`, each entry's known/reserved semantics come from
//! `CmisDataPathState::from_rev` (as baked in by the generated decoder) and
//! are printed through `name()`, matching the C side's
//! `openxcvr_cmis_data_path_state_for_revision` projection or
//! `Unknown<nibble>` when the projection has no name at that revision.

use std::process::ExitCode;

use openxcvr_cmis::generated::v3_0;
use openxcvr_cmis::generated::v4_0;
use openxcvr_cmis::generated::v5_0;
use openxcvr_cmis::generated::v5_1;
use openxcvr_cmis::generated::v5_2;
use openxcvr_cmis::generated::v5_3;

/// Identical LCG to the C runner so both sides read the same page bytes.
fn fill(seed: &str) -> Result<[u8; 128], String> {
    let mut x: u32 = if let Some(nibble) = seed.strip_prefix("dpsm-") {
        let v = u32::from_str_radix(nibble, 16).map_err(|e| e.to_string())?;
        if v > 15 {
            return Err(format!("dpsm nibble out of range: {seed}"));
        }
        v
    } else {
        seed.parse::<u32>().map_err(|e| e.to_string())?
    };
    let mut buf = [0u8; 128];
    for slot in &mut buf {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        *slot = (x >> 24) as u8;
    }
    if let Some(nibble) = seed.strip_prefix("dpsm-") {
        let v = u32::from_str_radix(nibble, 16).map_err(|e| e.to_string())? as u8;
        buf[..4].fill(v | (v << 4));
    }
    Ok(buf)
}

fn dump<T: std::fmt::Debug>(page: &str, rev: u8, value: &T) {
    println!("page {page}");
    println!("rev {}", rev as u32);
    println!("dump {value:?}");
}

fn interps(rev: u8, lanes: &[openxcvr_cmis::generated::CmisDataPathState; 8]) {
    for (i, state) in lanes.iter().enumerate() {
        println!("interp dp_state_host_lane[{i}] {}", state.name());
    }
    let _ = rev;
}

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args.next().expect("argv[0]");
    let (page, rev, seed) = match (args.next(), args.next(), args.next(), args.next()) {
        (Some(p), Some(r), Some(s), None) => (p, r, s),
        _ => {
            eprintln!("usage: {program:?} <01h|11h> <rev-byte-decimal> <seed>");
            return ExitCode::from(2);
        }
    };
    let page = match page.to_str() {
        Some(p @ ("01h" | "11h")) => p.to_string(),
        _ => return ExitCode::from(2),
    };
    let Some(rev) = rev
        .to_str()
        .and_then(|r| r.parse::<u8>().ok())
        .filter(|r| *r <= 0x53)
    else {
        return ExitCode::from(2);
    };
    let seed = match seed.to_str() {
        Some(s) => s.to_string(),
        None => return ExitCode::from(2),
    };
    let Ok(buf) = fill(&seed) else {
        return ExitCode::from(2);
    };
    match (page.as_str(), rev) {
        ("01h", 0x30) => dump("01h", rev, &v3_0::CmisPage01hAdvertising::decode(&buf)),
        ("01h", 0x40) => dump("01h", rev, &v4_0::CmisPage01hAdvertising::decode(&buf)),
        ("01h", 0x50) => dump("01h", rev, &v5_0::CmisPage01hAdvertising::decode(&buf)),
        ("01h", 0x51) => dump("01h", rev, &v5_1::CmisPage01hAdvertising::decode(&buf)),
        ("01h", 0x52) => dump("01h", rev, &v5_2::CmisPage01hAdvertising::decode(&buf)),
        ("01h", 0x53) => dump("01h", rev, &v5_3::CmisPage01hAdvertising::decode(&buf)),
        ("11h", 0x30) => {
            let v = v3_0::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        ("11h", 0x40) => {
            let v = v4_0::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        ("11h", 0x50) => {
            let v = v5_0::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        ("11h", 0x51) => {
            let v = v5_1::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        ("11h", 0x52) => {
            let v = v5_2::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        ("11h", 0x53) => {
            let v = v5_3::CmisPage11hLaneStatus::decode(&buf);
            dump("11h", rev, &v);
            interps(rev, &v.dp_state_host_lane);
        }
        _ => {
            eprintln!("revision byte {rev:#04x} is not in the CMIS catalog");
            return ExitCode::from(3);
        }
    }
    ExitCode::SUCCESS
}
