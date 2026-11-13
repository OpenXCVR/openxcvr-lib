//! Cross-rev parity runner. Reads tests/parity/cmis/*.yaml; for each listed
//! fixture, decodes through the rev-transparent `CmisModule` API and compares
//! the canonical fields named in `canonical_assertions` against the fixture's
//! committed golden JSON.
//!
//! Fail-closed: a listed fixture whose `.lower.bin` / `.page-00h.bin` / golden
//! is missing, whose detected rev disagrees with the spec's `rev` (or with the
//! golden's `_metadata.cmis_version`), or naming an unknown assertion path is
//! a hard failure — never a skip. Empty fixture lists remain SKIPPED so a
//! parity spec can be authored before its fixtures land.

use std::path::{Path, PathBuf};

use openxcvr_cmis::generated::union::CmisLowerMemory;
use openxcvr_cmis::{module::CmisTransport, CmisModule, Rev};
use openxcvr_core::{error::ModuleError, module::Module, transport::Transport};

#[derive(serde::Deserialize)]
struct ParitySpec {
    schema: String,
    kind: String,
    group: String,
    #[serde(default)]
    fixtures: Vec<FixtureEntry>,
    #[serde(default)]
    canonical_assertions: Vec<String>,
}

#[derive(serde::Deserialize)]
struct FixtureEntry {
    rev: String,
    path: PathBuf,
    golden: PathBuf,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// In-memory transport over the two pages the canonical identity/module-state
/// surface is decoded from: Lower memory (wire 0..127) and upper Page 00h
/// (wire 128..255). Any other page/offset is a hard I/O error so a decode
/// that silently reaches beyond the parity surface fails instead of
/// zero-reading.
struct LowerPlusPage00h {
    lower: [u8; 128],
    page_00h: [u8; 128],
}

impl Transport for LowerPlusPage00h {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x00, 128..=255) => &self.page_00h[(offset as usize - 128)..],
            _ => {
                return Err(ModuleError::Io(
                    format!("parity transport: unsupported page/offset 0x{page:02x}/{offset}")
                        .into(),
                ))
            }
        };
        if buf.len() > src.len() {
            return Err(ModuleError::Io(
                format!(
                    "parity transport: read overflow page=0x{page:02x} offset={offset} len={}",
                    buf.len()
                )
                .into(),
            ));
        }
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }
    fn write(&mut self, _: u8, _: u8, _: u8, _: u8, _: &[u8]) -> Result<(), ModuleError> {
        Err(ModuleError::Io("read-only parity transport".into()))
    }
    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
impl CmisTransport for LowerPlusPage00h {}

fn read_128(path: &Path) -> [u8; 128] {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("could not read fixture {path:?}: {e}"));
    assert_eq!(
        bytes.len(),
        128,
        "{path:?}: expected 128 bytes, got {}",
        bytes.len()
    );
    bytes.try_into().unwrap()
}

/// Canonical string form: the goldens carry raw fixed-width fields (NUL/space
/// padding included) while the adapter's `ascii_trim` reports the same field
/// trimmed. Trim both sides to the canonical form before comparing so the
/// assertion is about the field's content, not its padding.
fn canon(s: &str) -> &str {
    s.trim_end_matches(['\0', ' '])
}

/// Project one canonical assertion path to (decoded, golden) string pairs.
/// Unknown paths are a hard failure: a spec that names a field the runner
/// cannot project is a spec bug, not a pass.
fn project(
    assertion: &str,
    module: &CmisModule<LowerPlusPage00h>,
    lower: &[u8; 128],
    golden: &serde_json::Value,
) -> (String, String) {
    let golden_str = |ptr: &str| -> String {
        golden
            .pointer(ptr)
            .unwrap_or_else(|| panic!("golden missing {ptr}"))
            .as_str()
            .unwrap_or_else(|| panic!("golden {ptr} is not a string"))
            .to_owned()
    };
    match assertion {
        "identity.vendor_name" => {
            let info = module.vendor_info().expect("vendor_info");
            (info.name, golden_str("/identity/vendor_name"))
        }
        "identity.vendor_oui" => {
            let info = module.vendor_info().expect("vendor_info");
            (
                format!(
                    "{:02X}:{:02X}:{:02X}",
                    info.oui[0], info.oui[1], info.oui[2]
                ),
                golden_str("/identity/vendor_oui"),
            )
        }
        // The canonical accessor is identity.part_number; the golden's
        // identity block keys the same field vendor_pn.
        "identity.part_number" => {
            let info = module.vendor_info().expect("vendor_info");
            (info.part_number, golden_str("/identity/vendor_pn"))
        }
        "module_state" => {
            let decoded = CmisLowerMemory::decode(lower);
            (
                decoded.module_state.name(),
                golden_str("/module/module_state"),
            )
        }
        other => panic!("unknown canonical assertion path {other:?}"),
    }
}

#[test]
fn cross_rev_parity_specs_decode_and_compare() {
    let parity_dir = repo_root().join("tests/parity/cmis");
    let mut ran_group = false;
    for entry in std::fs::read_dir(&parity_dir).expect("parity dir") {
        let p = entry.expect("entry").path();
        if p.file_name().and_then(|n| n.to_str()) == Some("_shape.yaml") {
            continue;
        }
        let body = std::fs::read_to_string(&p).expect("read parity spec");
        let spec: ParitySpec =
            serde_norway::from_str(&body).unwrap_or_else(|e| panic!("parse {p:?}: {e}"));
        assert_eq!(spec.schema, "openxcvr.parity.v1");
        assert_eq!(spec.kind, "parity_spec");
        if spec.fixtures.is_empty() {
            eprintln!("SKIP {} (no fixtures yet)", spec.group);
            continue;
        }
        assert!(
            !spec.canonical_assertions.is_empty(),
            "{}: fixtures listed but no canonical_assertions — nothing would be compared",
            spec.group
        );
        ran_group = true;
        for f in &spec.fixtures {
            let root = repo_root();
            let lower_path = root.join(&f.path);
            let lower = read_128(&lower_path);
            let page00_path = f
                .path
                .to_str()
                .and_then(|s| s.strip_suffix(".lower.bin"))
                .map(|stem| root.join(format!("{stem}.page-00h.bin")))
                .unwrap_or_else(|| {
                    panic!(
                        "{}: fixture path {:?} must end in .lower.bin",
                        spec.group, f.path
                    )
                });
            let page_00h = read_128(&page00_path);

            let spec_rev = Rev::parse_cmis(&f.rev)
                .unwrap_or_else(|| panic!("{}: bad rev {:?}", spec.group, f.rev));
            let module =
                CmisModule::new(LowerPlusPage00h { lower, page_00h }).expect("CMIS detect");
            assert_eq!(
                module.detected_rev(),
                spec_rev,
                "{}: {:?} detected rev mismatch",
                spec.group,
                f.path
            );

            let golden_path = root.join(&f.golden);
            let golden_raw = std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|e| panic!("{}: read golden {golden_path:?}: {e}", spec.group));
            let golden: serde_json::Value = serde_json::from_str(&golden_raw)
                .unwrap_or_else(|e| panic!("{}: parse golden {golden_path:?}: {e}", spec.group));
            let golden_rev = golden
                .pointer("/_metadata/cmis_version")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "{}: golden {golden_path:?} lacks _metadata.cmis_version",
                        spec.group
                    )
                });
            assert_eq!(
                Rev::parse_cmis(golden_rev),
                Some(spec_rev),
                "{}: golden cmis_version {golden_rev} != spec rev {}",
                spec.group,
                f.rev
            );

            for a in &spec.canonical_assertions {
                let (decoded, want) = project(a, &module, &lower, &golden);
                assert_eq!(
                    canon(&decoded),
                    canon(&want),
                    "{}: {:?} assertion {a}: decoded {decoded:?} != golden {want:?}",
                    spec.group,
                    f.path
                );
            }
        }
    }
    assert!(
        ran_group,
        "no parity spec with fixtures found under {parity_dir:?} — the parity runner would be vacuous"
    );
}
