//! The unified entry surface: any-version family detection
//! ([`detect`], [`dispatch_identifier`]) and the spec-agnostic
//! [`UnifiedModule`] handle with its mmap and custom-transport constructors.

use openxcvr_cmis::{CmisMmapTransport, CmisModule};
use openxcvr_core::{
    error::{IoError, ModuleError},
    module::Module,
    revision_report::{self, RevisionReport, RevisionReportError},
    types::{Application, ChecksumStatus, LaneStatus, ModuleFlags, ModuleState, VendorInfo},
};
use openxcvr_sff8472::{adapter::Sff8472Module, mmap::Sff8472MmapTransport};
use openxcvr_sff8636::{adapter::Sff8636Module, mmap::Sff8636MmapTransport};

/// Spec-specific module wrapper. Variants hold concrete adapter types so consumers can
/// pattern-match on the underlying spec family when needed; for the common case, the
/// [`Module`] trait is implemented on `UnifiedModule` itself and dispatches per-variant.
pub enum UnifiedModule {
    /// SFP/SFP+/SFP28 module over the SFF-8472 A0h/A2h map (mmap captures).
    Sff8472(Sff8472Module<Sff8472MmapTransport>),
    /// QSFP+/QSFP28 module over the SFF-8636 paged map (mmap captures).
    Sff8636(Sff8636Module<Sff8636MmapTransport>),
    /// CMIS module (QSFP-DD/OSFP/DSFP/pure-CMIS families) over unbanked
    /// Pages 00h-05h (mmap captures).
    Cmis(Box<CmisModule<CmisMmapTransport>>),
    /// A caller-supplied [`Module`] implementation — typically a family adapter
    /// over a custom [`Transport`](openxcvr_core::transport::Transport). Unified
    /// reads dispatch through the trait object; capabilities beyond the
    /// [`Module`] surface stay reachable only through the caller's own handle,
    /// which remains the owner of the transport.
    Custom(Box<dyn Module>),
}

/// The result of the unified any-version detection pipeline: the family
/// routed from the SFF-8024 identifier byte, plus the spec compliance
/// revision proven from the family's revision byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Detected {
    /// The spec family routed from the SFF-8024 identifier byte.
    pub kind: SpecKind,
    /// The spec compliance revision proven from the family's revision byte.
    pub rev: openxcvr_core::Rev,
}

/// Pure any-version detection over a transport: read byte 0 (SFF-8024
/// identifier), route the family through [`dispatch_identifier`], then read
/// the family's revision byte (CMIS/SFF-8636: Lower:1, packed-BCD /
/// compliance code; SFF-8472: A0h byte 94, Table 8-8) and map it to a
/// [`Rev`](openxcvr_core::Rev).
///
/// Fail closed, never assume-latest: an unknown identifier fails with
/// [`ModuleError::UnknownIdentifier`], and an unknown/unspecified revision
/// byte fails with [`ModuleError::Detection`] carrying
/// [`openxcvr_core::DetectError::UnsupportedRev`] naming the detected byte
/// and the family's known-revision domain. Detection reads two bytes and
/// holds no state — the transport stays the caller's.
pub fn detect(
    transport: &mut dyn openxcvr_core::transport::Transport,
) -> Result<Detected, ModuleError> {
    use openxcvr_core::transport::{I2C_ADDR_MODULE, I2C_ADDR_SFF8472_BASE};

    let mut head = [0u8; 2];
    transport.read(I2C_ADDR_MODULE, 0x00, 0, 0, &mut head)?;
    let kind = dispatch_identifier(head[0], head[1])?;

    let rev = match kind {
        // CMIS and SFF-8636 both carry their revision at Lower:1 (CMIS
        // packed-BCD major byte; SFF-8636 Table 6-4 compliance code).
        SpecKind::Cmis => openxcvr_core::Rev::from_major_byte_cmis(head[1]).ok_or(
            openxcvr_core::DetectError::UnsupportedRev {
                detected_byte: head[1],
                known_revs: openxcvr_core::Rev::known_for(openxcvr_core::Family::Cmis),
            },
        )?,
        SpecKind::Sff8636 => openxcvr_core::Rev::from_compliance_byte_sff8636(head[1]).ok_or(
            openxcvr_core::DetectError::UnsupportedRev {
                detected_byte: head[1],
                known_revs: openxcvr_core::Rev::known_for(openxcvr_core::Family::Sff8636),
            },
        )?,
        // SFF-8472 keeps its compliance code in the A0h base memory at
        // byte 94.
        SpecKind::Sff8472 => {
            let mut b = [0u8; 1];
            transport.read(I2C_ADDR_SFF8472_BASE, 0, 0, 94, &mut b)?;
            openxcvr_core::Rev::from_compliance_byte_sff8472(b[0]).ok_or(
                openxcvr_core::DetectError::UnsupportedRev {
                    detected_byte: b[0],
                    known_revs: openxcvr_core::Rev::known_for(openxcvr_core::Family::Sff8472),
                },
            )?
        }
    };
    Ok(Detected { kind, rev })
}

impl UnifiedModule {
    /// Unified any-version construction over mmap captures: detect the
    /// family from byte 0, detect the revision from the family's revision
    /// byte, and open the matching adapter through its detect-once
    /// constructor so every read decodes through the revision-specialised
    /// generated reader. Fails closed on unknown identifiers and
    /// unknown/unspecified revisions — never an assumed latest revision.
    ///
    /// The parameter list is the union of the per-family mmap layouts
    /// (`open_sff8636_mmap`, `open_sff8472_mmap`, `open_cmis_mmap`); the
    /// unused paths per family are simply not opened.
    #[allow(clippy::too_many_arguments)]
    pub fn open_auto_mmap(
        lower: impl AsRef<std::path::Path>,
        page_00h: impl AsRef<std::path::Path>,
        page_01h: impl AsRef<std::path::Path>,
        page_02h: impl AsRef<std::path::Path>,
        page_03h: impl AsRef<std::path::Path>,
        page_04h: impl AsRef<std::path::Path>,
        page_05h: impl AsRef<std::path::Path>,
        a0h: impl AsRef<std::path::Path>,
        a2h: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        // Probe byte 0/1 from the lower capture to route the family.
        let lower_bytes = std::fs::read(lower.as_ref()).map_err(|e| {
            ModuleError::Io(IoError::with_source(
                format!("{}: {e}", lower.as_ref().display()),
                e,
            ))
        })?;
        if lower_bytes.len() < 2 {
            return Err(ModuleError::Io(
                format!(
                    "{}: lower capture shorter than the 2 detection bytes",
                    lower.as_ref().display()
                )
                .into(),
            ));
        }
        let kind = dispatch_identifier(lower_bytes[0], lower_bytes[1])?;

        match kind {
            SpecKind::Sff8636 => {
                let t = Sff8636MmapTransport::from_files(
                    lower, page_00h, page_01h, page_02h, page_03h,
                )?;
                let module = Sff8636Module::open(t).map_err(ModuleError::from)?;
                Ok(Self::Sff8636(module))
            }
            SpecKind::Sff8472 => {
                let t = Sff8472MmapTransport::from_files(a0h, a2h)?;
                let module = Sff8472Module::open(t).map_err(ModuleError::from)?;
                Ok(Self::Sff8472(module))
            }
            SpecKind::Cmis => {
                let t = CmisMmapTransport::from_files(
                    lower, page_00h, page_01h, page_02h, page_03h, page_04h, page_05h,
                )
                .map_err(|e| ModuleError::Io(IoError::with_source(e.to_string(), e)))?;
                let module = CmisModule::new(t).map_err(ModuleError::from)?;
                Ok(Self::Cmis(Box::new(module)))
            }
        }
    }

    /// The detected spec revision when this handle was opened through
    /// [`Self::open_auto_mmap`] (CMIS/SFF-8636/SFF-8472 all expose their
    /// detect-once revision); `None` for the per-family constructors that
    /// do not detect, and for [`Self::Custom`] — a custom module's revision
    /// context is its caller's, never guessed here.
    pub fn detected_rev(&self) -> Option<openxcvr_core::Rev> {
        match self {
            Self::Sff8472(m) => m.detected_rev(),
            Self::Sff8636(m) => m.detected_rev(),
            Self::Cmis(m) => Some(m.detected_rev()),
            Self::Custom(_) => None,
        }
    }

    /// Schema-driven field availability at the revision this handle detected.
    ///
    /// Lists every field in the family's generated catalog as available or
    /// revision-gated at [`Self::detected_rev`]. Windows are the schema
    /// `since`/`until` spans ([`revision_report::field_availability`]); this
    /// method does not keep a hand-written field list and does not read the
    /// module again. A field gated out of this revision stays in the report.
    ///
    /// Fails closed. Rev-agnostic constructors and [`Self::Custom`] have no
    /// detected revision and return
    /// [`RevisionReportError::RevisionNotDetected`] rather than an assumed
    /// one. A family or revision the schemas do not catalog is refused the
    /// same way. Callers who already hold a [`Rev`](openxcvr_core::Rev) and
    /// the catalog can call [`revision_report::field_availability`] directly.
    pub fn revision_report(&self) -> Result<RevisionReport, RevisionReportError> {
        let rev = self
            .detected_rev()
            .ok_or(RevisionReportError::RevisionNotDetected)?;
        let catalog = crate::catalog::family_catalog(rev.family)
            .ok_or(RevisionReportError::NoCatalog { family: rev.family })?;
        revision_report::field_availability(catalog, rev)
    }

    /// Open an SFF-8636 module over captured memory images (rev-agnostic:
    /// decodes through the union view of every generated field). Callers
    /// that can read Lower:1 should prefer
    /// [`Sff8636Module::open`](openxcvr_sff8636::adapter::Sff8636Module::open)
    /// via [`Self::open_custom`].
    pub fn open_sff8636_mmap(
        lower: impl AsRef<std::path::Path>,
        page_00h: impl AsRef<std::path::Path>,
        page_01h: impl AsRef<std::path::Path>,
        page_02h: impl AsRef<std::path::Path>,
        page_03h: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        let t = Sff8636MmapTransport::from_files(lower, page_00h, page_01h, page_02h, page_03h)?;
        Ok(Self::Sff8636(Sff8636Module::new(t)))
    }

    /// Open an SFF-8472 module over captured A0h/A2h images (rev-agnostic:
    /// decodes through the union view of every generated field). Callers
    /// that can read A0h:94 should prefer
    /// [`Sff8472Module::open`](openxcvr_sff8472::adapter::Sff8472Module::open)
    /// via [`Self::open_custom`].
    pub fn open_sff8472_mmap(
        a0h: impl AsRef<std::path::Path>,
        a2h: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        let t = Sff8472MmapTransport::from_files(a0h, a2h)?;
        Ok(Self::Sff8472(Sff8472Module::new(t)))
    }

    /// Open a CMIS module over captured lower/Pages 00h-05h images. Detects
    /// the revision once through the CMIS packed-BCD byte and decodes every
    /// read through the revision-specialised generated reader; unknown
    /// revisions fail closed.
    pub fn open_cmis_mmap(
        lower: impl AsRef<std::path::Path>,
        page_00h: impl AsRef<std::path::Path>,
        page_01h: impl AsRef<std::path::Path>,
        page_02h: impl AsRef<std::path::Path>,
        page_03h: impl AsRef<std::path::Path>,
        page_04h: impl AsRef<std::path::Path>,
        page_05h: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        let t = CmisMmapTransport::from_files(
            lower, page_00h, page_01h, page_02h, page_03h, page_04h, page_05h,
        )
        .map_err(|e| ModuleError::Io(IoError::with_source(e.to_string(), e)))?;
        let module = CmisModule::new(t).map_err(ModuleError::from)?;
        Ok(Self::Cmis(Box::new(module)))
    }

    /// Wrap any [`Module`] implementation — in particular a family adapter
    /// constructed over a custom [`Transport`](openxcvr_core::transport::Transport)
    /// (an `Sff8472Module<T>`, `Sff8636Module<T>` or `CmisModule<T>` built by
    /// the caller's own transport code, or any other module type) — so its
    /// reads dispatch through the unified [`Module`] surface. The boxed
    /// module owns its transport; this handle adds no I/O of its own.
    ///
    /// Family detection over a custom transport is available separately
    /// through [`detect`], which is pure and leaves the transport with the
    /// caller; per-spec capabilities beyond the [`Module`] trait stay on the
    /// caller's concrete handle.
    pub fn open_custom(module: impl Module + 'static) -> Self {
        Self::Custom(Box::new(module))
    }

    /// Attach a bank-0 Page 11h capture to an already opened CMIS mmap module.
    pub fn with_cmis_page_11h(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        match self {
            Self::Cmis(module) => {
                let module = module.with_page_11h(path)?;
                Ok(Self::Cmis(Box::new(module)))
            }
            _ => Err(ModuleError::Unsupported),
        }
    }
}

impl Module for UnifiedModule {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        match self {
            Self::Sff8472(m) => m.vendor_info(),
            Self::Sff8636(m) => m.vendor_info(),
            Self::Cmis(m) => m.vendor_info(),
            Self::Custom(m) => m.vendor_info(),
        }
    }
    fn identifier(&self) -> Result<u8, ModuleError> {
        match self {
            Self::Sff8472(m) => m.identifier(),
            Self::Sff8636(m) => m.identifier(),
            Self::Cmis(m) => m.identifier(),
            Self::Custom(m) => m.identifier(),
        }
    }
    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        match self {
            Self::Sff8472(m) => m.temperature_celsius(),
            Self::Sff8636(m) => m.temperature_celsius(),
            Self::Cmis(m) => m.temperature_celsius(),
            Self::Custom(m) => m.temperature_celsius(),
        }
    }
    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        match self {
            Self::Sff8472(m) => m.supply_voltage_v(),
            Self::Sff8636(m) => m.supply_voltage_v(),
            Self::Cmis(m) => m.supply_voltage_v(),
            Self::Custom(m) => m.supply_voltage_v(),
        }
    }
    fn lane_count(&self) -> Result<u8, ModuleError> {
        match self {
            Self::Sff8472(m) => m.lane_count(),
            Self::Sff8636(m) => m.lane_count(),
            Self::Cmis(m) => m.lane_count(),
            Self::Custom(m) => m.lane_count(),
        }
    }
    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        match self {
            Self::Sff8472(m) => m.lanes(),
            Self::Sff8636(m) => m.lanes(),
            Self::Cmis(m) => m.lanes(),
            Self::Custom(m) => m.lanes(),
        }
    }
    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        match self {
            Self::Sff8472(m) => m.flags(),
            Self::Sff8636(m) => m.flags(),
            Self::Cmis(m) => m.flags(),
            Self::Custom(m) => m.flags(),
        }
    }
    fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
        match self {
            Self::Sff8472(m) => m.checksums(),
            Self::Sff8636(m) => m.checksums(),
            Self::Cmis(m) => m.checksums(),
            Self::Custom(m) => m.checksums(),
        }
    }
    fn applications(&self) -> Result<Vec<Application>, ModuleError> {
        match self {
            Self::Sff8472(m) => m.applications(),
            Self::Sff8636(m) => m.applications(),
            Self::Cmis(m) => m.applications(),
            Self::Custom(m) => m.applications(),
        }
    }
    fn module_state(&self) -> Result<ModuleState, ModuleError> {
        match self {
            Self::Sff8472(m) => m.module_state(),
            Self::Sff8636(m) => m.module_state(),
            Self::Cmis(m) => m.module_state(),
            Self::Custom(m) => m.module_state(),
        }
    }
}

/// SFF-8024 byte-0 / byte-1 dispatch result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecKind {
    /// SFF-8472 management (SFP/SFP+/SFP28, soldered variants).
    Sff8472,
    /// SFF-8636 management (QSFP/INF-8438, QSFP+, QSFP28, microQSFP, and
    /// the legacy pre-CMIS QSFP-DD/OSFP register maps).
    Sff8636,
    /// QSFP-DD / OSFP with CMIS rev 0x30 (CMIS 3.0) or ≥ 0x40 (CMIS 4.0+),
    /// plus pure-CMIS families (DSFP, ELSFP, CDFP, etc.).
    /// v0.1 dispatcher routes these to [`UnifiedModule::Cmis`] backed by the [`CmisModule`]
    /// adapter (unbanked Pages 00h–05h supported, plus typed CDB commands for the
    /// qualified subset; banked pages deferred to v0.2+).
    Cmis,
}

/// Dispatch byte-0 (SFF-8024 identifier) to a `SpecKind`.
///
/// SFF-8472 identifiers: 0x02 (soldered/SFF-8472), 0x03 (SFP/SFP+/SFP28).
///
/// SFF-8636 identifiers: 0x0C (QSFP/INF-8438, backward-compat), 0x0D (QSFP+), 0x11 (QSFP28),
/// 0x17 (microQSFP, uses SFF-8636 management per SFF-8024 §4.1).
///
/// QSFP-DD (0x18) and OSFP (0x19) use byte-1 as a tie-break: CMIS 3.0 modules
/// (byte1 = 0x30, the BCD-like major revision `Rev::from_major_byte_cmis` maps
/// to `Rev::CMIS_3_0`) and CMIS 4.0+ modules (byte1 ≥ 0x40) use the CMIS
/// adapter; every other byte-1 value (< 0x40 except 0x30, e.g. the legacy 0x00
/// and 0x10 values) used an SFF-8636-compatible register map. Classification is
/// family routing only — it is not supported-revision admission.
///
/// Pure-CMIS families never had a legacy SFF-8636 variant:
/// 0x1B (DSFP), 0x1E–0x25 (all `*_cmis` named per SFF-8024 Table 4-1).
///
/// Everything else returns `Err(UnknownIdentifier)`.
pub fn dispatch_identifier(byte0: u8, byte1: u8) -> Result<SpecKind, ModuleError> {
    match byte0 {
        // SFF-8472 management
        0x02 | 0x03 => Ok(SpecKind::Sff8472),
        // SFF-8636 management: 0x0C QSFP/INF-8438, 0x0D QSFP+, 0x11 QSFP28, 0x17 microQSFP
        0x0C | 0x0D | 0x11 | 0x17 => Ok(SpecKind::Sff8636),
        // QSFP-DD / OSFP: CMIS 3.0 (byte1 = 0x30, Rev::CMIS_3_0) and CMIS 4.0+
        // (byte1 >= 0x40) → CMIS; every other sub-0x40 byte is the legacy
        // pre-CMIS SFF-8636-compatible map.
        0x18 | 0x19 => {
            if byte1 >= 0x40 || byte1 == 0x30 {
                Ok(SpecKind::Cmis)
            } else {
                Ok(SpecKind::Sff8636)
            }
        }
        // DSFP (0x1B) uses CMIS management per SFF-8024
        // Pure-CMIS families (no legacy SFF-8636 variant): 0x1E–0x25 (*_cmis per schema)
        0x1B | 0x1E | 0x1F | 0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 => Ok(SpecKind::Cmis),
        other => Err(ModuleError::UnknownIdentifier(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory transport for the pure detection pipeline: 0x50 carries the
    /// lower page, 0xA0 the SFF-8472 base page.
    struct DetectTransport {
        lower: [u8; 128],
        a0: [u8; 256],
    }

    impl openxcvr_core::transport::Transport for DetectTransport {
        fn read(
            &mut self,
            address: u8,
            _page: u8,
            _bank: u8,
            offset: u8,
            out: &mut [u8],
        ) -> Result<(), ModuleError> {
            let src = match address {
                0x50 => &self.lower[offset as usize..],
                0xA0 => &self.a0[offset as usize..],
                _ => return Err(ModuleError::Io("bad address".into())),
            };
            out.copy_from_slice(&src[..out.len()]);
            Ok(())
        }
        fn write(&mut self, _a: u8, _p: u8, _b: u8, _o: u8, _d: &[u8]) -> Result<(), ModuleError> {
            Err(ModuleError::Io("read-only".into()))
        }
        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    /// Detection routes each family from byte 0 and proves the revision from
    /// the family's own revision byte — CMIS packed-BCD at Lower:1,
    /// SFF-8636 compliance code at Lower:1, SFF-8472 compliance code at
    /// A0h:94.
    #[test]
    fn detect_routes_family_and_revision_from_the_actual_bytes() {
        // SFF-8636 QSFP+ (0x0D) reporting 2.11 (0x09).
        let mut t = DetectTransport {
            lower: [0; 128],
            a0: [0; 256],
        };
        t.lower[0] = 0x0D;
        t.lower[1] = 0x09;
        assert_eq!(
            detect(&mut t).unwrap(),
            Detected {
                kind: SpecKind::Sff8636,
                rev: openxcvr_core::Rev::SFF8636_2_11
            }
        );

        // SFF-8472 SFP+ (0x03) reporting 12.5 (0x0A) at A0h:94.
        t.lower[0] = 0x03;
        t.a0[94] = 0x0A;
        assert_eq!(
            detect(&mut t).unwrap(),
            Detected {
                kind: SpecKind::Sff8472,
                rev: openxcvr_core::Rev::SFF8472_12_5
            }
        );

        // QSFP-DD (0x18) reporting CMIS 5.3 (0x53).
        t.lower[0] = 0x18;
        t.lower[1] = 0x53;
        assert_eq!(
            detect(&mut t).unwrap(),
            Detected {
                kind: SpecKind::Cmis,
                rev: openxcvr_core::Rev::CMIS_5_3
            }
        );
    }

    /// Detection fails closed on unknown identifiers, and on
    /// unknown/unspecified revision bytes — never an assumed latest
    /// revision. The error names the detected byte and the family's known
    /// revision domain.
    #[test]
    fn detect_fails_closed_never_assume_latest() {
        // Unknown identifier.
        let mut t = DetectTransport {
            lower: [0; 128],
            a0: [0; 256],
        };
        t.lower[0] = 0xFF;
        assert!(matches!(
            detect(&mut t),
            Err(ModuleError::UnknownIdentifier(0xFF))
        ));

        // SFF-8636 with the unspecified code 0x00.
        t.lower = [0; 128];
        t.lower[0] = 0x0D;
        match detect(&mut t) {
            Err(ModuleError::Detection(e)) => match *e {
                openxcvr_core::DetectError::UnsupportedRev {
                    detected_byte,
                    known_revs,
                } => {
                    assert_eq!(detected_byte, 0x00);
                    assert_eq!(
                        known_revs,
                        openxcvr_core::Rev::known_for(openxcvr_core::Family::Sff8636)
                    );
                }
                other => panic!("unexpected detect error: {other:?}"),
            },
            other => panic!("must fail closed, got {other:?}"),
        }

        // SFF-8636 with a reserved code beyond the table.
        t.lower[1] = 0x0B;
        assert!(matches!(detect(&mut t), Err(ModuleError::Detection(_))));

        // SFF-8472 with the unspecified code 0x00 at A0h:94.
        t.lower[0] = 0x03;
        t.a0[94] = 0x00;
        assert!(matches!(detect(&mut t), Err(ModuleError::Detection(_))));

        // CMIS with an unassigned major byte.
        t.lower[0] = 0x18;
        t.lower[1] = 0x21;
        assert!(matches!(detect(&mut t), Err(ModuleError::Detection(_))));
    }

    #[test]
    fn dispatch_soldered_sff8472() {
        // 0x02 = module/connector soldered to motherboard using SFF-8472
        assert_eq!(dispatch_identifier(0x02, 0x00).unwrap(), SpecKind::Sff8472);
    }
    #[test]
    fn dispatch_sfp_plus() {
        assert_eq!(dispatch_identifier(0x03, 0x00).unwrap(), SpecKind::Sff8472);
    }
    #[test]
    fn dispatch_micro_qsfp() {
        // 0x17 = microQSFP, uses SFF-8636 management interface
        assert_eq!(dispatch_identifier(0x17, 0x00).unwrap(), SpecKind::Sff8636);
    }
    #[test]
    fn dispatch_qsfp28() {
        assert_eq!(dispatch_identifier(0x11, 0x00).unwrap(), SpecKind::Sff8636);
    }
    #[test]
    fn dispatch_dsfp_cmis() {
        // 0x1B = DSFP (Dual SFP), uses CMIS management per SFF-8024
        assert_eq!(dispatch_identifier(0x1B, 0x00).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_qsfpdd_cmis_4() {
        assert_eq!(dispatch_identifier(0x18, 0x40).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_qsfpdd_cmis_5() {
        assert_eq!(dispatch_identifier(0x18, 0x50).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_qsfpdd_legacy_pre_cmis() {
        assert_eq!(dispatch_identifier(0x18, 0x00).unwrap(), SpecKind::Sff8636);
    }
    #[test]
    fn dispatch_osfp_legacy() {
        assert_eq!(dispatch_identifier(0x19, 0x10).unwrap(), SpecKind::Sff8636);
    }
    #[test]
    fn dispatch_qsfp_plus_cmis() {
        // 0x1E = QSFP+ with CMIS (pure-CMIS, no legacy SFF-8636 variant)
        assert_eq!(dispatch_identifier(0x1E, 0x00).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_sfp_dd_cmis() {
        // 0x1F = SFP-DD with CMIS (pure-CMIS, no byte1 tie-break)
        assert_eq!(dispatch_identifier(0x1F, 0x00).unwrap(), SpecKind::Cmis);
        assert_eq!(dispatch_identifier(0x1F, 0x50).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_dwdm_sfp_unknown() {
        // 0x0B = DWDM-SFP "not using SFF-8472" — different management spec
        assert!(dispatch_identifier(0x0B, 0x00).is_err());
    }
    #[test]
    fn dispatch_sfp_dd_mgmt_unknown() {
        // 0x1A = SFP-DD with SFP-DD management interface (not SFF-8472)
        assert!(dispatch_identifier(0x1A, 0x00).is_err());
    }
    #[test]
    fn dispatch_minilink_unknown() {
        // 0x1C = x4 MiniLink/OcuLink — PCIe storage connector, not a transceiver
        assert!(dispatch_identifier(0x1C, 0x00).is_err());
    }
    #[test]
    fn dispatch_oif_elsfp_cmis() {
        // 0x22 = OIF-ELSFP with CMIS
        assert_eq!(dispatch_identifier(0x22, 0x00).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_cdfp_pcie_cmis() {
        // 0x23/0x24/0x25 = CDFP x4/x8/x16 PCIe with CMIS
        assert_eq!(dispatch_identifier(0x23, 0x00).unwrap(), SpecKind::Cmis);
        assert_eq!(dispatch_identifier(0x24, 0x00).unwrap(), SpecKind::Cmis);
        assert_eq!(dispatch_identifier(0x25, 0x00).unwrap(), SpecKind::Cmis);
    }
    #[test]
    fn dispatch_unknown() {
        assert!(dispatch_identifier(0xFF, 0x00).is_err());
    }

    #[test]
    fn dispatch_cmis_3_0_qsfpdd_and_osfp_route_to_cmis() {
        // Grounding, independent of the dispatcher: CMIS 3.0 reports its major
        // revision as 0x30 (BCD-like nibble layout, CMIS 5.3 § 8.2.1; the
        // synthetic CMIS 3.0 fixtures carry byte1 = 0x30).
        assert_eq!(
            openxcvr_core::rev::Rev::from_major_byte_cmis(0x30),
            Some(openxcvr_core::rev::Rev::CMIS_3_0)
        );
        assert_eq!(dispatch_identifier(0x18, 0x30).unwrap(), SpecKind::Cmis);
        assert_eq!(dispatch_identifier(0x19, 0x30).unwrap(), SpecKind::Cmis);
        // Sub-0x40 bytes other than 0x30 keep the legacy pre-CMIS map.
        assert_eq!(dispatch_identifier(0x18, 0x31).unwrap(), SpecKind::Sff8636);
        assert_eq!(dispatch_identifier(0x19, 0x2F).unwrap(), SpecKind::Sff8636);
        assert_eq!(dispatch_identifier(0x19, 0x3F).unwrap(), SpecKind::Sff8636);
        // The >= 0x40 boundary is not lowered.
        assert_eq!(dispatch_identifier(0x18, 0x40).unwrap(), SpecKind::Cmis);
        // Pure-CMIS families ignore byte 1 entirely.
        assert_eq!(dispatch_identifier(0x1B, 0x30).unwrap(), SpecKind::Cmis);
    }

    /// Collapse a dispatch result to a comparable tag. `dispatch_identifier`
    /// only fails with `UnknownIdentifier`, so the byte is a faithful tag.
    fn dispatch_tag(byte0: u8, byte1: u8) -> Result<Option<SpecKind>, u8> {
        match dispatch_identifier(byte0, byte1) {
            Ok(kind) => Ok(Some(kind)),
            Err(ModuleError::UnknownIdentifier(byte)) => Err(byte),
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    /// Snapshot of the trunk rule at 42efcae, embedded verbatim as the
    /// old-vs-new compatibility baseline (not regenerated from anything).
    fn trunk_reference(byte0: u8, byte1: u8) -> Result<Option<SpecKind>, u8> {
        match byte0 {
            0x02 | 0x03 => Ok(Some(SpecKind::Sff8472)),
            0x0C | 0x0D | 0x11 | 0x17 => Ok(Some(SpecKind::Sff8636)),
            0x18 | 0x19 => Ok(Some(if byte1 >= 0x40 {
                SpecKind::Cmis
            } else {
                SpecKind::Sff8636
            })),
            0x1B | 0x1E..=0x25 => Ok(Some(SpecKind::Cmis)),
            other => Err(other),
        }
    }

    /// Expected behavior authored from the settled contract, independently of
    /// the dispatcher under test and of the shared dispatch table: identifiers
    /// 0x18/0x19 route to CMIS exactly at CMIS 3.0's revision byte 0x30 and at
    /// every byte >= 0x40; every other outcome matches the trunk snapshot.
    fn expected_new(byte0: u8, byte1: u8) -> Result<Option<SpecKind>, u8> {
        match byte0 {
            0x02 | 0x03 => Ok(Some(SpecKind::Sff8472)),
            0x0C | 0x0D | 0x11 | 0x17 => Ok(Some(SpecKind::Sff8636)),
            0x18 | 0x19 => Ok(Some(if byte1 >= 0x40 || byte1 == 0x30 {
                SpecKind::Cmis
            } else {
                SpecKind::Sff8636
            })),
            0x1B | 0x1E..=0x25 => Ok(Some(SpecKind::Cmis)),
            other => Err(other),
        }
    }

    #[test]
    fn dispatch_matches_authored_contract_over_all_65536_pairs() {
        for byte0 in 0u16..=255 {
            for byte1 in 0u16..=255 {
                let (byte0, byte1) = (byte0 as u8, byte1 as u8);
                assert_eq!(
                    dispatch_tag(byte0, byte1),
                    expected_new(byte0, byte1),
                    "pair ({byte0:#04x}, {byte1:#04x})"
                );
            }
        }
    }

    #[test]
    fn dispatch_changes_exactly_two_pairs_against_trunk_snapshot() {
        let mut deltas = Vec::new();
        for byte0 in 0u16..=255 {
            for byte1 in 0u16..=255 {
                let (byte0, byte1) = (byte0 as u8, byte1 as u8);
                let old = trunk_reference(byte0, byte1);
                let new = dispatch_tag(byte0, byte1);
                if old != new {
                    deltas.push((byte0, byte1, old, new));
                }
            }
        }
        assert_eq!(
            deltas.len(),
            2,
            "exactly (0x18,0x30) and (0x19,0x30) may change"
        );
        for (byte0, byte1, old, new) in deltas {
            assert!((byte0 == 0x18 || byte0 == 0x19) && byte1 == 0x30);
            assert_eq!(old, Ok(Some(SpecKind::Sff8636)));
            assert_eq!(new, Ok(Some(SpecKind::Cmis)));
        }
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;
    use openxcvr_core::types::{ChecksumStatus, LaneStatus, ModuleFlags, VendorInfo};

    /// Minimal Module over a custom (here: no) transport: proves the boxed
    /// `dyn Module` route dispatches the unified surface without any mmap
    /// transport underneath, and that revision context is never guessed.
    struct StubModule {
        ident: u8,
    }

    impl Module for StubModule {
        fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
            Ok(VendorInfo::default())
        }
        fn identifier(&self) -> Result<u8, ModuleError> {
            Ok(self.ident)
        }
        fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
            Ok(Some(30.5))
        }
        fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
            Ok(Some(3.3))
        }
        fn lane_count(&self) -> Result<u8, ModuleError> {
            Ok(4)
        }
        fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
            Ok(Vec::new())
        }
        fn flags(&self) -> Result<ModuleFlags, ModuleError> {
            Ok(ModuleFlags::default())
        }
        fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
            Ok(ChecksumStatus::default())
        }
    }

    #[test]
    fn custom_module_dispatches_through_the_unified_surface() {
        let m = UnifiedModule::open_custom(StubModule { ident: 0x0D });
        assert_eq!(m.identifier().unwrap(), 0x0D);
        assert_eq!(m.temperature_celsius().unwrap(), Some(30.5));
        // Defaults still flow through the trait object.
        assert!(matches!(m.applications(), Err(ModuleError::Unsupported)));
        let unsupported = m.applications().unwrap_err();
        let text = unsupported.to_string();
        assert!(
            text.contains("not exposed by this module family/revision"),
            "{text}"
        );
        assert!(text.contains("detected_rev()"), "{text}");
        // No detect-once revision context: never guessed.
        assert_eq!(m.detected_rev(), None);
        assert!(matches!(
            m.revision_report(),
            Err(RevisionReportError::RevisionNotDetected)
        ));
        let page = match UnifiedModule::open_custom(StubModule { ident: 0x0D })
            .with_cmis_page_11h("not-read")
        {
            Err(error) => error,
            Ok(_) => panic!("page 11h on a non-CMIS module must be unsupported"),
        };
        assert!(matches!(page, ModuleError::Unsupported));
        assert!(page.to_string().contains("detected_rev()"));
    }
}

#[cfg(test)]
mod revision_report_tests {
    use super::*;
    use openxcvr_core::revision_report::field_availability;
    use openxcvr_core::{Family, Rev};

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn cmis_catalog_reports_a_later_field_as_gated_not_absent() {
        let catalog = crate::catalog::family_catalog(Family::Cmis).expect("cmis catalog");
        let early = field_availability(catalog, Rev::CMIS_3_0).expect("3.0 is catalogued");
        let later = early
            .field("auto_commissioning")
            .expect("schema field must be listed, not dropped");
        assert!(
            later.is_gated(),
            "a field outside its since/until window must be gated, not absent: {later}"
        );
        assert!(
            later
                .windows()
                .iter()
                .all(|window| !window.contains(early.rev)),
            "gated windows must not contain CMIS 3.0"
        );
        assert!(early.field("sff8024_identifier").unwrap().is_available());
        assert!(
            early.field("not_a_schema_field").is_none(),
            "a name the schema never declared is absence, not a gate"
        );

        let at_4 = field_availability(catalog, Rev::CMIS_4_0).unwrap();
        assert!(
            at_4.field("mci_max_speed")
                .expect("renamed field stays listed")
                .is_gated(),
            "mci_max_speed at 4.0 is outside its spans"
        );
        assert!(at_4.field("twi_max_speed_cmis4").unwrap().is_available());

        let at_53 = field_availability(catalog, Rev::CMIS_5_3).unwrap();
        assert!(at_53.field("auto_commissioning").unwrap().is_available());
    }

    #[test]
    fn opened_synthetic_cmis_3_0_report_uses_the_detected_revision() {
        let root = repo_root().join("tests/fixtures/cmis");
        let module = UnifiedModule::open_cmis_mmap(
            root.join("synthetic-cmis-3-0-qsfp-dd.lower.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-00h.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-01h.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-02h.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-02h.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-02h.bin"),
            root.join("synthetic-cmis-3-0-qsfp-dd.page-02h.bin"),
        )
        .expect("synthetic CMIS 3.0 capture");
        assert_eq!(module.detected_rev(), Some(Rev::CMIS_3_0));
        let report = module.revision_report().expect("detected revision");
        assert_eq!(report.rev, Rev::CMIS_3_0);
        assert!(report.field("auto_commissioning").unwrap().is_gated());
        let text = report.to_string();
        assert!(text.contains("CMIS 3.0"), "{text}");
        assert!(text.contains("revision-gated"), "{text}");
    }
}
