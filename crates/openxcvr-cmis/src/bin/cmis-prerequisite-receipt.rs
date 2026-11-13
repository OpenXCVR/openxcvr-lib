//! Generate the library-issued prerequisite-I/O receipt (default CMIS 5.3).
//! `--cmis-revision 4.0` selects only the scoped internal-D4 addition.
//!
//! Usage:
//!
//! ```sh
//! # the explicitly undelivered draft a commit may carry:
//! cargo run -q -p openxcvr-cmis --bin cmis-prerequisite-receipt -- \
//!     --library-revision <oid> --connector-revision <oid> [--out <path>]
//!
//! # the delivered receipt, only from the landed revision:
//! cargo run -q -p openxcvr-cmis --bin cmis-prerequisite-receipt -- \
//!     --library-revision <oid> --connector-revision <oid> \
//!     --review-reference <non-empty reference> [--out <path>]
//! ```
//!
//! The revision pins are the consumer's compiled pins, supplied by the
//! caller and recorded verbatim; they are never guessed. Without
//! `--review-reference` the receipt is a **draft**: `delivered: false` and
//! no `review_reference` member at all — a commit cannot name its own OID,
//! so a committed receipt is always such a draft. The generator refuses to
//! emit a delivered receipt unless `--library-revision` equals the `HEAD`
//! of a clean checkout it is run from and `--review-reference` is
//! non-empty: the delivered receipt is regenerated from the landed commit,
//! after the work lands, for the consumer's verified pins. Without `--out`
//! the receipt is printed to stdout. Missing or malformed arguments refuse
//! with usage on stderr and a nonzero exit; nothing is written partially.

use std::io::Write;
use std::process::Command;

/// The checkout this generator was built from: the delivered-receipt check
/// pins `--library-revision` to its `HEAD`.
const CHECKOUT: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    let mut revision = openxcvr_core::Rev::CMIS_5_3;
    let mut library_revision: Option<String> = None;
    let mut connector_revision: Option<String> = None;
    let mut review_reference: Option<String> = None;
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage(&[&arg]));
        match arg.as_str() {
            "--cmis-revision" => {
                revision = match value().as_str() {
                    "4.0" => openxcvr_core::Rev::CMIS_4_0,
                    "5.3" => openxcvr_core::Rev::CMIS_5_3,
                    _ => usage(&["--cmis-revision 4.0|5.3"]),
                }
            }
            "--library-revision" => library_revision = Some(value()),
            "--connector-revision" => connector_revision = Some(value()),
            "--review-reference" => review_reference = Some(value()),
            "--out" => out = Some(value()),
            _ => usage(&[&arg]),
        }
    }
    let library_revision = library_revision.unwrap_or_else(|| usage(&["--library-revision <oid>"]));
    let connector_revision =
        connector_revision.unwrap_or_else(|| usage(&["--connector-revision <oid>"]));
    if library_revision.trim().is_empty() || connector_revision.trim().is_empty() {
        usage(&["the revision pins must be non-empty"]);
    }
    // A delivered receipt is bound to the landed revision: the checkout the
    // generator runs from must be clean, and its `HEAD` must be exactly the
    // `--library-revision` the caller named. A draft names its pins the same
    // way but is explicitly undelivered, so no delivery check applies.
    if let Some(reference) = review_reference.as_deref() {
        if reference.trim().is_empty() {
            refuse("a delivered receipt requires a non-empty --review-reference; omit it to emit an explicitly undelivered draft");
        }
        let head = match Command::new("git")
            .args(["-C", CHECKOUT, "rev-parse", "--verify", "HEAD"])
            .output()
        {
            Ok(output) => output,
            Err(error) => refuse(&format!("cannot run git in {CHECKOUT}: {error}")),
        };
        if !head.status.success() {
            refuse(&format!("{CHECKOUT} is not a git checkout with a HEAD"));
        }
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
        if head != library_revision.trim() {
            refuse(&format!(
                "a delivered receipt binds --library-revision to the HEAD of the clean \
                 checkout it is run from: HEAD is {head}, named {library_revision}"
            ));
        }
        let status = match Command::new("git")
            .args(["-C", CHECKOUT, "status", "--porcelain"])
            .output()
        {
            Ok(output) => output,
            Err(error) => refuse(&format!("cannot run git in {CHECKOUT}: {error}")),
        };
        if !status.status.success() || !status.stdout.is_empty() {
            refuse(&format!(
                "a delivered receipt is generated from a clean checkout: {CHECKOUT} has \
                 uncommitted or untracked work"
            ));
        }
    }
    let receipt = match openxcvr_cmis::prerequisite_receipt::receipt_json_for_revision(
        revision,
        &library_revision,
        &connector_revision,
        review_reference.as_deref(),
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("the library refused to emit the receipt: {error}");
            std::process::exit(1);
        }
    };
    match out {
        Some(path) => {
            if let Err(error) = std::fs::write(&path, receipt) {
                eprintln!("cannot write {path}: {error}");
                std::process::exit(1);
            }
        }
        None => {
            let stdout = std::io::stdout();
            let mut locked = stdout.lock();
            if let Err(error) = locked.write_all(receipt.as_bytes()) {
                eprintln!("cannot write the receipt: {error}");
                std::process::exit(1);
            }
        }
    }
}

/// Print the usage line and refuse: missing pins never guess a default.
fn usage(missing: &[&str]) -> ! {
    eprintln!(
        "usage: cmis-prerequisite-receipt --library-revision <oid> --connector-revision <oid> \
         [--cmis-revision 4.0|5.3] [--review-reference <reference>] [--out <path>]\n  missing or unusable: {missing:?}"
    );
    std::process::exit(2);
}

/// Print a delivery-check refusal and exit: a delivered receipt is never
/// emitted on a dirty checkout or a revision that is not its own `HEAD`.
fn refuse(reason: &str) -> ! {
    eprintln!("the generator refuses to emit a delivered receipt: {reason}");
    std::process::exit(1);
}
