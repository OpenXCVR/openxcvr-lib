//! Embedded synthetic CMIS sample and offline capture loading.
//!
//! The embedded bytes are the committed output of
//! `tools/build-cmis-5-2-synthetic.py` (hand-authored CMIS 5.2 QSFP-DD
//! placeholders). They are not a vendor capture. `tests/fixtures/cmis/ATTRIBUTIONS.md`
//! covers the fboss-derived fixtures in that directory; those files are not
//! embedded here. `include_bytes!` compiles the synthetic pages into the
//! example, so a run needs no path and no particular working directory.

// Each example binary links this module and uses a different subset.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

const LOWER: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.lower.bin");
const PAGE_00H: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.page-00h.bin");
const PAGE_01H: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.page-01h.bin");
const PAGE_02H: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.page-02h.bin");
const PAGE_10H: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.page-10h.bin");
const PAGE_11H: &[u8] =
    include_bytes!("../../../../tests/fixtures/cmis/synthetic-cmis-5-2-qsfp-dd.page-11h.bin");

/// One supplied 128-byte window. Absent windows are not represented — callers
/// must not treat a missing page as a zero page.
#[derive(Debug, Clone)]
pub struct PageBytes {
    /// `lower`, or an upper-page label such as `00h`.
    pub label: String,
    /// `None` is lower memory. `Some(page)` is that upper page.
    pub upper: Option<u8>,
    /// The supplied bytes. Exactly 128; shorter or longer inputs are refused.
    pub bytes: Vec<u8>,
}

/// A module image assembled only from pages the caller actually supplied.
#[derive(Debug, Clone)]
pub struct Capture {
    /// Where the bytes came from, for the example's own output.
    pub source: String,
    pages: Vec<PageBytes>,
}

/// Why a capture could not be assembled. Refuses rather than padding,
/// trimming, or inventing a page.
#[derive(Debug)]
pub struct SampleError(pub String);

impl std::fmt::Display for SampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SampleError {}

impl Capture {
    /// The embedded synthetic CMIS 5.2 QSFP-DD sample.
    pub fn embedded() -> Self {
        let pages = [
            ("lower", None, LOWER),
            ("00h", Some(0x00), PAGE_00H),
            ("01h", Some(0x01), PAGE_01H),
            ("02h", Some(0x02), PAGE_02H),
            ("10h", Some(0x10), PAGE_10H),
            ("11h", Some(0x11), PAGE_11H),
        ];
        Self {
            source: "embedded synthetic CMIS 5.2 QSFP-DD (tools/build-cmis-5-2-synthetic.py)"
                .to_string(),
            pages: pages
                .into_iter()
                .map(|(label, upper, bytes)| PageBytes {
                    label: label.to_string(),
                    upper,
                    bytes: bytes.to_vec(),
                })
                .collect(),
        }
    }

    /// Load a capture from `path`.
    ///
    /// - A directory must contain exactly one `*.lower.bin`; sibling
    ///   `<stem>.page-XXh.bin` files are loaded with it.
    /// - A `*.lower.bin` or `*.page-XXh.bin` file loads that stem from the
    ///   same directory.
    /// - Any other file whose length is a positive multiple of 128 bytes is a
    ///   dense image: the first 128 bytes are lower memory and each following
    ///   128 bytes is the next upper page, starting at `00h`.
    ///
    /// Each named page file must be exactly 128 bytes.
    pub fn load(path: &Path) -> Result<Self, SampleError> {
        if path.is_dir() {
            return load_dir(path);
        }
        if !path.is_file() {
            return Err(SampleError(format!(
                "capture path does not exist: {}",
                path.display()
            )));
        }
        let name = file_name(path)?;
        if let Some((stem, _)) = classify(&name) {
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            return load_stem(dir, &stem, &format!("capture {}", path.display()));
        }
        load_dense(path)
    }

    /// Pages in display order: lower memory, then upper pages by selector.
    pub fn pages(&self) -> &[PageBytes] {
        &self.pages
    }

    /// The page with this label (`lower`, `00h`, …), if it was supplied.
    pub fn page(&self, label: &str) -> Option<&PageBytes> {
        let want = normalize_label(label);
        self.pages.iter().find(|page| page.label == want)
    }

    /// Bytes of one catalog range, if the covering page was supplied.
    ///
    /// `address` is the seven-bit I2C address. Upper-page window offsets are
    /// 128..256; the stored page slice is the 128-byte upper half.
    pub fn range_bytes(&self, address: u8, page: u8, offset: u8, len: u16) -> Option<&[u8]> {
        if address != openxcvr_core::transport::I2C_ADDR_MODULE {
            return None;
        }
        let end = u32::from(offset) + u32::from(len);
        if end > 256 {
            return None;
        }
        let (upper, index) = if page == 0 && offset < 128 {
            if end > 128 {
                return None;
            }
            (None, usize::from(offset))
        } else if offset >= 128 {
            (Some(page), usize::from(offset) - 128)
        } else {
            return None;
        };
        let supplied = self
            .pages
            .iter()
            .find(|candidate| candidate.upper == upper)?;
        let index_end = index + usize::from(len);
        supplied.bytes.get(index..index_end)
    }

    /// Dense EEPROM image for [`openxcvr_cmis::CmisModule::from_bytes`].
    ///
    /// Layout matches `BufTransport`: bytes 0..128 are lower memory, and
    /// upper page `P` occupies `128 + P*128`. Pages that were not supplied
    /// stay absent from [`Self::pages`]; the dense image still has zeros in
    /// those gaps because the transport is a single buffer. Callers must not
    /// decode a gap. This returns an error when lower memory is missing or
    /// any page is not 128 bytes — nothing is padded or trimmed.
    pub fn flat_eeprom(&self) -> Result<Vec<u8>, SampleError> {
        if self.page("lower").is_none() {
            return Err(SampleError(
                "capture has no lower memory page; refusing to invent one".to_string(),
            ));
        }
        let mut end = 128usize;
        for page in &self.pages {
            if page.bytes.len() != 128 {
                return Err(SampleError(format!(
                    "page {} is {} bytes; expected 128 (refusing to pad or trim)",
                    page.label,
                    page.bytes.len()
                )));
            }
            if let Some(selector) = page.upper {
                end = end.max(128 + (usize::from(selector) + 1) * 128);
            }
        }
        let mut buf = vec![0u8; end];
        let mut filled = vec![false; end];
        for page in &self.pages {
            let start = match page.upper {
                None => 0,
                Some(selector) => 128 + usize::from(selector) * 128,
            };
            if filled[start..start + 128].contains(&true) {
                return Err(SampleError(format!(
                    "duplicate bytes for page {}",
                    page.label
                )));
            }
            buf[start..start + 128].copy_from_slice(&page.bytes);
            filled[start..start + 128].fill(true);
        }
        Ok(buf)
    }
}

/// Accept `lower`, `00h`, `0x11`, or a bare hex page number.
pub fn normalize_label(label: &str) -> String {
    let text = label.trim().to_ascii_lowercase();
    if text == "lower" {
        return text;
    }
    let hex = text
        .strip_prefix("0x")
        .unwrap_or(&text)
        .strip_suffix('h')
        .unwrap_or(&text);
    if !hex.is_empty() && hex.len() <= 2 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(page) = u8::from_str_radix(hex, 16) {
            return format!("{page:02x}h");
        }
    }
    text
}

fn file_name(path: &Path) -> Result<String, SampleError> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| SampleError(format!("capture path has no file name: {}", path.display())))
}

enum Kind {
    Lower,
    Upper(u8),
}

fn classify(file_name: &str) -> Option<(String, Kind)> {
    if let Some(stem) = file_name.strip_suffix(".lower.bin") {
        if stem.is_empty() {
            return None;
        }
        return Some((stem.to_string(), Kind::Lower));
    }
    let (stem, rest) = file_name.rsplit_once(".page-")?;
    if stem.is_empty() {
        return None;
    }
    let hex = rest.strip_suffix("h.bin")?;
    if hex.is_empty() || hex.len() > 2 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let page = u8::from_str_radix(hex, 16).ok()?;
    Some((stem.to_string(), Kind::Upper(page)))
}

fn load_dir(dir: &Path) -> Result<Capture, SampleError> {
    let mut stems = Vec::new();
    for (path, stem, kind) in read_classified(dir)? {
        if matches!(kind, Kind::Lower) {
            stems.push((stem, path));
        }
    }
    stems.sort_by(|a, b| a.0.cmp(&b.0));
    stems.dedup_by(|a, b| a.0 == b.0);
    match stems.as_slice() {
        [(stem, _)] => load_stem(dir, stem, &format!("capture {}", dir.display())),
        [] => Err(SampleError(format!(
            "{} contains no *.lower.bin; pass a fixture file or a dense EEPROM image",
            dir.display()
        ))),
        _ => {
            let listed = stems
                .iter()
                .map(|(stem, _)| stem.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(SampleError(format!(
                "{} contains more than one module ({listed}); pass one *.lower.bin",
                dir.display()
            )))
        }
    }
}

fn load_stem(dir: &Path, stem: &str, source: &str) -> Result<Capture, SampleError> {
    let mut pages = Vec::new();
    for (path, found, kind) in read_classified(dir)? {
        if found != stem {
            continue;
        }
        let bytes = read_page(&path)?;
        let (label, upper) = match kind {
            Kind::Lower => ("lower".to_string(), None),
            Kind::Upper(page) => (format!("{page:02x}h"), Some(page)),
        };
        if pages
            .iter()
            .any(|existing: &PageBytes| existing.label == label)
        {
            return Err(SampleError(format!(
                "duplicate page {label} in {}",
                dir.display()
            )));
        }
        pages.push(PageBytes {
            label,
            upper,
            bytes,
        });
    }
    if pages.is_empty() {
        return Err(SampleError(format!(
            "no pages for stem {stem} in {}",
            dir.display()
        )));
    }
    pages.sort_by_key(|page| (page.upper.is_some(), page.upper));
    Ok(Capture {
        source: source.to_string(),
        pages,
    })
}

fn read_classified(dir: &Path) -> Result<Vec<(PathBuf, String, Kind)>, SampleError> {
    let entries = fs::read_dir(dir)
        .map_err(|err| SampleError(format!("cannot read {}: {err}", dir.display())))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|err| SampleError(format!("cannot read {}: {err}", dir.display())))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = file_name(&path)?;
        if let Some((stem, kind)) = classify(&name) {
            found.push((path, stem, kind));
        }
    }
    Ok(found)
}

fn read_page(path: &Path) -> Result<Vec<u8>, SampleError> {
    let bytes = fs::read(path)
        .map_err(|err| SampleError(format!("cannot read {}: {err}", path.display())))?;
    if bytes.len() != 128 {
        return Err(SampleError(format!(
            "{} is {} bytes; expected 128 (refusing to pad or trim)",
            path.display(),
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn load_dense(path: &Path) -> Result<Capture, SampleError> {
    let bytes = fs::read(path)
        .map_err(|err| SampleError(format!("cannot read {}: {err}", path.display())))?;
    if bytes.is_empty() || bytes.len() % 128 != 0 {
        return Err(SampleError(format!(
            "{} is not a *.lower.bin, *.page-XXh.bin, or a dense image whose length is a multiple of 128 (got {} bytes)",
            path.display(),
            bytes.len()
        )));
    }
    // One lower window plus upper pages 00h..=FFh.
    if bytes.len() / 128 > 257 {
        return Err(SampleError(format!(
            "{} has more than 256 upper pages",
            path.display()
        )));
    }
    let mut pages = Vec::new();
    pages.push(PageBytes {
        label: "lower".to_string(),
        upper: None,
        bytes: bytes[..128].to_vec(),
    });
    for (index, chunk) in bytes[128..].chunks(128).enumerate() {
        let page = u8::try_from(index).map_err(|_| {
            SampleError(format!("{} has more than 255 upper pages", path.display()))
        })?;
        pages.push(PageBytes {
            label: format!("{page:02x}h"),
            upper: Some(page),
            bytes: chunk.to_vec(),
        });
    }
    Ok(Capture {
        source: format!("dense image {}", path.display()),
        pages,
    })
}
