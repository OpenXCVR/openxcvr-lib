//! Explore a transceiver capture offline: list the pages that were supplied
//! and decode a page or a field on demand.
//!
//! No hardware. With no path, the embedded synthetic CMIS 5.2 sample is
//! used (see `docs/guides/quickstart.md`). A path loads that capture instead.
//!
//! ```sh
//! cargo run -p openxcvr --example explore-module
//! cargo run -p openxcvr --example explore-module -- --tour
//! cargo run -p openxcvr --example explore-module -- path/to/module.lower.bin
//! ```
//!
//! A terminal gets a small command loop (`help`, `vendor`, `pages`,
//! `page <label>`, `field [page] <name>`, `quit`). A pipe runs those
//! commands until EOF. With no commands, or with `--tour`, a guided tour
//! is printed and the process exits. Field decode uses the revision proven
//! from lower memory and refuses an unknown revision; bytes that were not
//! supplied are reported missing rather than treated as zero.

#[path = "support/sample.rs"]
mod sample;

use openxcvr::catalog::{self, FieldEntry, FieldKind, FieldQuery, LaneSelection};
use openxcvr::core::rev::Rev;
use openxcvr::core::transport::I2C_ADDR_MODULE;
use openxcvr::core::types::management::{
    CacheEpoch, Freshness, ModuleGeneration, MonoTime, ObservationSource, ObservationStamp, Range,
    RangeObservation, RangeOutcome, ReadRequester, Snapshot,
};
use openxcvr::observation::{
    observe_field, FieldObservation, FieldValue, ObservationState, ObserveOptions,
};
use openxcvr::{CmisModule, Module};
use sample::{Capture, SampleError};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const GENERATION: ModuleGeneration = ModuleGeneration(1);
const EPOCH: CacheEpoch = CacheEpoch(1);

const HELP: &str = "\
commands:
  help                  show this list
  vendor                vendor identity and the proven spec revision
  pages                 list pages that were actually supplied
  page <label>          hex-dump a page and decode its fields
  field [page] <name>   decode one field (page label disambiguates)
  quit                  exit";

struct Args {
    tour: bool,
    path: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let capture = match &args.path {
        Some(path) => Capture::load(path)?,
        None => Capture::embedded(),
    };
    if args.tour {
        tour(&capture)?;
        return Ok(());
    }
    if io::stdin().is_terminal() {
        println!("source: {}", capture.source);
        println!("{HELP}");
        repl(&capture)?;
        return Ok(());
    }
    let commands = read_piped_commands()?;
    if commands.is_empty() {
        tour(&capture)?;
        return Ok(());
    }
    for line in commands {
        if !dispatch(&capture, &line)? {
            break;
        }
    }
    Ok(())
}

fn parse_args() -> Result<Args, SampleError> {
    let mut tour = false;
    let mut path = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--tour" => tour = true,
            "--help" | "-h" => {
                println!(
                    "explore-module [--tour] [CAPTURE]\n\n\
                     CAPTURE is optional. Without it, the embedded synthetic\n\
                     CMIS 5.2 sample is used. See docs/guides/quickstart.md.\n\n\
                     {HELP}"
                );
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(SampleError(format!("unknown argument {other}; try --help")));
            }
            other => {
                if path.is_some() {
                    return Err(SampleError("pass at most one capture path".to_string()));
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    Ok(Args { tour, path })
}

fn read_piped_commands() -> Result<Vec<String>, SampleError> {
    let stdin = io::stdin();
    let mut lines = Vec::new();
    for line in stdin.lock().lines() {
        let line = line.map_err(|err| SampleError(format!("stdin: {err}")))?;
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    Ok(lines)
}

fn repl(capture: &Capture) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut locked = stdin.lock();
    let mut line = String::new();
    loop {
        print!("explore> ");
        io::stdout().flush()?;
        line.clear();
        if locked.read_line(&mut line)? == 0 {
            println!();
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        match dispatch(capture, &line) {
            Ok(true) => {}
            Ok(false) => break,
            Err(err) => eprintln!("error: {err}"),
        }
    }
    Ok(())
}

fn tour(capture: &Capture) -> Result<(), Box<dyn std::error::Error>> {
    println!("source: {}", capture.source);
    println!("guided tour (pass --tour, or run from a terminal for the command loop)");
    print_pages(capture);
    println!();
    if let Err(err) = print_vendor(capture) {
        println!("revision/vendor refused: {err}");
        println!("field decode stays closed until a known revision is proven");
        return Ok(());
    }
    println!();
    let rev = detect(capture)?;
    for (page, name) in [
        ("lower", "sff8024_identifier"),
        ("lower", "cmis_revision"),
        ("00h", "vendor_name"),
        ("lower", "temp_mon_value"),
        ("lower", "vcc_mon_voltage"),
    ] {
        if capture.page(page).is_none() {
            println!("{name}: page {page} was not supplied");
            continue;
        }
        decode_field(capture, rev, Some(page), name)?;
    }
    Ok(())
}

/// `Ok(true)` continues, `Ok(false)` quits.
fn dispatch(capture: &Capture, line: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let Some(command) = parts.first().copied() else {
        return Ok(true);
    };
    match command {
        "help" | "?" => println!("{HELP}"),
        "quit" | "exit" | "q" => return Ok(false),
        "pages" => print_pages(capture),
        "vendor" => print_vendor(capture)?,
        "page" => {
            let Some(label) = parts.get(1) else {
                return Err(SampleError("usage: page <label>".to_string()).into());
            };
            print_page(capture, label)?;
        }
        "field" => {
            let (page, name) = match parts.as_slice() {
                [_, name] => (None, *name),
                [_, page, name] => (Some(*page), *name),
                _ => {
                    return Err(SampleError("usage: field [page] <name>".to_string()).into());
                }
            };
            decode_field(capture, detect(capture)?, page, name)?;
        }
        other => {
            return Err(SampleError(format!("unknown command {other}; try help")).into());
        }
    }
    Ok(true)
}

fn print_pages(capture: &Capture) {
    println!("pages supplied: {}", capture.pages().len());
    for page in capture.pages() {
        println!("  {:<8} {} bytes", page.label, page.bytes.len());
    }
}

fn print_vendor(capture: &Capture) -> Result<(), Box<dyn std::error::Error>> {
    let module = open_lower(capture)?;
    let rev = module.detected_rev();
    println!(
        "spec revision: CMIS {} (lower byte 1 = 0x{:02X})",
        rev.as_str(),
        module.cmis_revision()?
    );
    println!("identifier: 0x{:02X}", module.identifier()?);
    if capture.page("00h").is_none() {
        println!("page 00h was not supplied; vendor strings were not decoded");
        return Ok(());
    }
    let info = open_flat(capture)?.vendor_info()?;
    println!("vendor: {}", info.name);
    println!("part: {}", info.part_number);
    println!("serial: {}", info.serial_number);
    println!("vendor rev: {}", info.rev);
    Ok(())
}

fn print_page(capture: &Capture, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (page_label, bytes) = match capture.page(label) {
        Some(page) => (page.label.clone(), page.bytes.clone()),
        None => {
            return Err(SampleError(format!(
                "page {} was not supplied",
                sample::normalize_label(label)
            ))
            .into());
        }
    };
    println!(
        "page {page_label} ({} bytes; hex is page-relative, field offsets are window offsets)",
        bytes.len()
    );
    hex_dump(&bytes);
    println!();
    let rev = match detect(capture) {
        Ok(rev) => rev,
        Err(err) => {
            println!("field decode refused: {err}");
            return Ok(());
        }
    };
    let entries = resolve_page(rev, &page_label)?;
    let view = snapshot(capture);
    println!("fields at CMIS {}: {}", rev.as_str(), entries.len());
    for entry in &entries {
        print_entry(capture, &view, entry);
    }
    Ok(())
}

fn decode_field(
    capture: &Capture,
    rev: Rev,
    page: Option<&str>,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let page_owned = page.map(sample::normalize_label);
    let query = FieldQuery {
        pattern: name,
        lane: LaneSelection::Unspecified,
        page: page_owned.as_deref(),
        bank: 0,
    };
    let entries = catalog::resolve(rev, &query)?;
    let view = snapshot(capture);
    for entry in &entries {
        print_entry(capture, &view, entry);
    }
    Ok(())
}

fn resolve_page(
    rev: Rev,
    label: &str,
) -> Result<Vec<FieldEntry<'static>>, Box<dyn std::error::Error>> {
    let query = FieldQuery {
        pattern: "*",
        lane: LaneSelection::Unspecified,
        page: Some(label),
        bank: 0,
    };
    Ok(catalog::resolve(rev, &query)?)
}

fn print_entry(capture: &Capture, view: &Snapshot, entry: &FieldEntry<'_>) {
    let lane = match entry.lane {
        Some(lane) => format!(" lane {lane}"),
        None => String::new(),
    };
    let loc = format!(
        "{}:{} len {}{lane}",
        entry.row.page_label, entry.offset, entry.row.len
    );
    let range = entry.range();
    let raw = capture.range_bytes(range.address, range.page, range.offset, range.len);
    let Some(raw) = raw else {
        println!("  {}  {loc}  not supplied", entry.name());
        return;
    };
    match entry.row.kind {
        FieldKind::Ascii => {
            let text = String::from_utf8_lossy(raw);
            let text = text.trim_matches([' ', '\0']);
            println!("  {}  {loc}  ascii {text:?}", entry.name());
        }
        FieldKind::Bytes => {
            let hex = raw
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  {}  {loc}  bytes {hex}", entry.name());
        }
        _ => {
            let observed =
                observe_field(entry, view, GENERATION, EPOCH, &ObserveOptions::default());
            let rendered = format_observation(&observed);
            if observed.state == ObservationState::Valid {
                println!("  {}  {loc}  {rendered}", entry.name());
            } else {
                let hex = raw
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  {}  {loc}  {rendered}  raw {hex}", entry.name());
            }
        }
    }
}

fn format_observation(observed: &FieldObservation) -> String {
    let rendered = match observed.value {
        Some(FieldValue::Scaled { value, unit }) => format!("{value} {}", unit.name()),
        Some(FieldValue::Raw(raw)) => format!("0x{raw:X}"),
        None => "no value".to_string(),
    };
    let note = observed
        .note
        .map(|note| format!(" ({note})"))
        .unwrap_or_default();
    format!("{:?} {rendered}{note}", observed.state)
}

fn hex_dump(bytes: &[u8]) {
    for (index, chunk) in bytes.chunks(16).enumerate() {
        let offset = index * 16;
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = chunk
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    *byte as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("  {offset:04x}  {hex:<47}  {ascii}");
    }
}

fn detect(capture: &Capture) -> Result<Rev, Box<dyn std::error::Error>> {
    Ok(open_lower(capture)?.detected_rev())
}

fn open_lower(
    capture: &Capture,
) -> Result<CmisModule<openxcvr::core::transport::BufTransport>, Box<dyn std::error::Error>> {
    let lower = capture.page("lower").ok_or_else(|| {
        SampleError("lower memory was not supplied; revision is not assumed".to_string())
    })?;
    Ok(CmisModule::from_bytes(&lower.bytes)?)
}

fn open_flat(
    capture: &Capture,
) -> Result<CmisModule<openxcvr::core::transport::BufTransport>, Box<dyn std::error::Error>> {
    Ok(CmisModule::from_bytes(&capture.flat_eeprom()?)?)
}

fn snapshot(capture: &Capture) -> Snapshot {
    let stamp = ObservationStamp {
        generation: GENERATION,
        epoch: EPOCH,
        started_at: MonoTime(0),
        completed_at: MonoTime(1),
        source: ObservationSource::Supplied,
        freshness: Freshness::Fresh,
        consumed_effects: Vec::new(),
    };
    let ranges = capture
        .pages()
        .iter()
        .map(|page| {
            let (selector, offset) = match page.upper {
                None => (0, 0),
                Some(selector) => (selector, 128),
            };
            RangeObservation {
                range: Range {
                    address: I2C_ADDR_MODULE,
                    bank: 0,
                    page: selector,
                    offset,
                    len: u16::try_from(page.bytes.len()).expect("page length fits in a range"),
                },
                outcome: RangeOutcome::Bytes(page.bytes.clone()),
                stamp: stamp.clone(),
                requester: ReadRequester::PassiveInspection,
            }
        })
        .collect();
    Snapshot {
        validity_interval: (MonoTime(0), MonoTime(1)),
        generation: GENERATION,
        epoch: EPOCH,
        ranges,
    }
}
