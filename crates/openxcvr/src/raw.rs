//! Pure raw target/address/confidentiality preflight (R1). No I/O or execution.
//!
//! A successful description is **not write permission**: even RO information
//! bytes can pass this confidentiality gate. Access/writability, selector
//! acquisition, transfer limits, freshness, exclusion, audit persistence and
//! write/readback evidence remain the execution planner/owner's obligations.
//! Nothing here repairs checksums, widens a range, interprets a payload, or
//! supplies an executable write action. A request describes one operation;
//! no partial description escapes when any of its ranges fails.
//!
//! # Published support matrix
//!
//! `support_cell` is the total, machine-readable version of this matrix. A
//! supported window still requires byte-level classification and passive read
//! qualification (also for a write, whose later readback must be possible).
//!
//! | Family / revision | Address model | Bank / page | R1 window support |
//! |---|---|---|---|
//! | CMIS 4.0 | `Cmis` (lower AND upper at seven-bit 0x50) | 0 / 00h | Qualified bytes only |
//! | CMIS 4.0 | `Cmis` | Any other bank or page | Unsupported |
//! | CMIS 4.0 | SFF-8636, SFF-8472 A0h, SFF-8472 A2h, unknown | Any | Unsupported model |
//! | CMIS, any revision except exactly 4.0 | Any | Any | Unsupported revision |
//! | SFF-8472, SFF-8636, C-CMIS, ELSFP (every revision) | Any | Any | Unsupported family |
//!
//! A0h/A2h are historical *address labels*, not page numbers. Canonicalization
//! uses `openxcvr_transports::bus::Address7::from_legacy`: A0h -> 0x50,
//! A2h -> 0x51. CMIS upper memory remains at 0x50, never at SFF-8472 A2h.
//! Coordinates use the existing core `Range` and `Selection`; each range
//! stays in one 128-byte lower/upper window, in caller order.
//!
//! # Classification authority and limits
//!
//! * Generated CMIS catalog rows at the selected revision supply physical
//!   spans, access, tags and presence. Password (`BlindControl::Password`),
//!   WO/WO-SC and `WriteOnly` rows taint **their entire byte spans**, not just
//!   their first byte or readable-bit masks. In CMIS 4.0 this covers both
//!   password areas, bytes 118-121 and 122-125: CMIS 4.0 §8.2.10, Table 8-14.
//! * Control tags and RW/RWW access deny raw writes; bank/page selectors are
//!   catalog fields, not a second selector table. Unknown, reserved, custom,
//!   vendor-specific and conditional semantics cannot establish nonsecret
//!   storage. Every bit needs positive semantic coverage, independently of
//!   read effects. In particular, generated `ReadEffect::None` is NOT proof
//!   of nonsecret memory (e.g. upper byte 221 is VendorSpecific).
//! * Read effects come only from generated **v4_0** lower/page-00h byte
//!   classifications, not the newest union projection. Unknown effects and
//!   clear/acknowledge-on-read are refused, not silently consumed.
//! * Deny-only CMIS 4.0 CDB coverage: §8.13 / Table 8-106 describes page 9Fh
//!   command/header/LPL, including triggering on byte 129; §8.14 / Table
//!   8-107 describes EPL pages A0h-AFh. Their upper windows in banks 0/1 are
//!   command staging, not raw storage. They remain unsupported for reads AND
//!   writes; no CDB capability, command safety or later-revision semantics
//!   are inferred. Source: parsed `cmis__CMIS4p0_Third_Party_Spec.md`.
//!
//! Only admitted, nonsecret coordinates enter the audit projection. A refusal
//! redacts ALL coordinates (including any good prefix), retaining only the
//! reported revision, purpose and range count. Payloads are borrowed, opaque,
//! and never hashed, decoded, compared or copied here. Their owner retains
//! responsibility for secret lifetime/erasure; Debug/Display expose counts
//! only, and descriptions/refusals never retain payloads.
use crate::catalog::{self, AccessKind, CatalogRow};
use openxcvr_cmis::generated::v4_0::{CmisLowerMemory, CmisPage00hAdministrative};
use openxcvr_core::{
    field_catalog::FieldPresence,
    rev::{Family, Rev},
    types::management::{
        ByteClassification, Range, ReadEffect, ReadIntent, ReadRequester, Selection,
    },
};
use openxcvr_transports::bus::Address7;
use std::fmt;

/// R1 API capacity, not a hardware transfer limit or a standards maximum.
/// Sixteen single-window ranges bound both classification work and output.
pub const MAX_RANGES: usize = 16;
/// Derived from `MAX_RANGES` and the single-window coordinate contract.
pub const MAX_BYTES: usize = MAX_RANGES * 128;

/// Explicit memory model identity, independent of historical address notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressModel {
    /// CMIS single-address module memory (0x50).
    Cmis,
    /// SFF-8636 QSFP memory (0x50).
    Sff8636,
    /// SFF-8472 A0h address (0x50).
    Sff8472A0,
    /// SFF-8472 A2h address (0x51).
    Sff8472A2,
    /// Unrecognized caller identifier, always unsupported.
    Unknown(u16),
}

impl AddressModel {
    fn address(self) -> Option<u8> {
        match self {
            Self::Cmis | Self::Sff8636 | Self::Sff8472A0 => Some(0x50),
            Self::Sff8472A2 => Some(0x51),
            Self::Unknown(_) => None,
        }
    }
}

/// Exact requested coordinates; only `address` may be canonicalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawTarget {
    /// The explicit memory-model identity of the target.
    pub model: AddressModel,
    /// The exact requested byte range.
    pub range: Range,
}

impl RawTarget {
    /// Address normalization/identity and core bounds only, NOT admission.
    /// In particular, canonicalizing A2h never grants SFF-8472 support.
    pub fn canonical(self) -> Result<Self, RefusalReason> {
        let address = Address7::from_legacy(self.range.address)
            .map_err(|_| RefusalReason::InvalidAddress)?
            .get();
        if self
            .model
            .address()
            .is_some_and(|expected| expected != address)
        {
            return Err(RefusalReason::AddressModelMismatch);
        }
        let range = Range {
            address,
            ..self.range
        };
        range.validate().map_err(|_| RefusalReason::InvalidRange)?;
        Ok(Self { range, ..self })
    }

    /// For a canonical target this is a canonical selector description only.
    /// It neither asserts current selection nor authorizes a selector write.
    pub fn selection(self) -> Selection {
        Selection {
            address: self.range.address,
            bank: self.range.bank,
            page: self.range.page,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Whether a raw request reads or writes.
pub enum Purpose {
    /// A raw read request.
    Read,
    /// A raw write request.
    Write,
}

/// Counted, borrowed opaque write material. No automatic payload diagnostics,
/// equality, identifiers or owned copies. `expose` is an explicit future
/// execution handoff, never called by this preflight.
#[derive(Clone, Copy)]
pub struct WriteBytes<'a>(&'a [u8]);

impl<'a> WriteBytes<'a> {
    /// Borrow the write material.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
    /// The material's byte count.
    pub fn len(self) -> usize {
        self.0.len()
    }
    /// Whether the material is empty.
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
    /// Expose the material (dispatch only, never for logs).
    pub fn expose(self) -> &'a [u8] {
        self.0
    }
}

impl fmt::Debug for WriteBytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WriteBytes")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for WriteBytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<opaque write bytes: {}>", self.len())
    }
}

#[derive(Clone, Copy)]
/// One region of a raw request: a target plus write material.
pub struct RawRegion<'a> {
    /// The region's target.
    pub target: RawTarget,
    /// Exactly `target.range.len` bytes for a write; absent for a read.
    pub write_bytes: Option<WriteBytes<'a>>,
}
impl fmt::Debug for RawRegion<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Before classification even coordinates may name a secret region.
        f.debug_struct("RawRegion")
            .field("write_bytes", &self.write_bytes)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
/// One raw request: reported revision, purpose and regions.
pub struct RawRequest<'a> {
    /// Caller-supplied reported revision. This API does not detect a module.
    pub revision: Rev,
    /// Whether the request reads or writes.
    pub purpose: Purpose,
    /// Borrowed input is checked against MAX_RANGES before any traversal or
    /// output allocation; a description contains at most MAX_BYTES bytes.
    pub regions: &'a [RawRegion<'a>],
}
impl fmt::Debug for RawRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawRequest")
            .field("revision", &self.revision)
            .field("purpose", &self.purpose)
            .field("range_count", &self.regions.len())
            .finish_non_exhaustive()
    }
}

/// Explicit reason a matrix cell has no positive qualification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedCell {
    /// The family has no raw surface.
    Family,
    /// The revision has no raw surface.
    Revision,
    /// The address model is not supported.
    AddressModel,
    /// The bank coordinate is not supported.
    Bank,
    /// The page coordinate is not supported.
    Page,
}

/// Total support query: every non-CMIS4/model-Cmis/bank0/page00 cell is
/// explicitly unsupported. No implicit/default family or revision allowance.
pub fn support_cell(
    revision: Rev,
    model: AddressModel,
    bank: u8,
    page: u8,
) -> Result<(), UnsupportedCell> {
    if revision.family != Family::Cmis {
        return Err(UnsupportedCell::Family);
    }
    if revision != Rev::CMIS_4_0 {
        return Err(UnsupportedCell::Revision);
    }
    if model != AddressModel::Cmis {
        return Err(UnsupportedCell::AddressModel);
    }
    if bank != 0 {
        return Err(UnsupportedCell::Bank);
    }
    if page != 0 {
        return Err(UnsupportedCell::Page);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The confidentiality/coverage class of one requested byte.
pub enum ByteClass {
    /// Positive catalog semantic coverage; this says nothing about write
    /// permission. Read effects are independently qualified below.
    OrdinaryReadable,
    /// Secret or write-only memory: no honest value exists.
    SecretOrWriteOnly,
    /// Selector/control/command memory the request must not touch.
    ControlOrCommand,
    /// No catalog coverage at all; explicitly unknown.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One requested byte's classification entry.
pub struct RawByte {
    /// The byte's confidentiality class.
    pub class: ByteClass,
    /// The selected revision's generated effect metadata, if delivered.
    pub read: Option<ByteClassification>,
    nonsecret_coverage: bool,
}

impl RawByte {
    fn passive(&self) -> bool {
        self.nonsecret_coverage
            && self
                .read
                .as_ref()
                .is_some_and(ByteClassification::is_qualified_none)
    }
}

/// One byte entry per requested byte, in ascending physical offset order.
/// Classification alone is not admission; unsupported cells can provide
/// deny-only classifications and unknown bytes remain explicitly unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDescription {
    target: RawTarget,
    bytes: Vec<RawByte>,
}
impl TargetDescription {
    /// The described target.
    pub fn target(&self) -> RawTarget {
        self.target
    }
    /// One entry per requested byte, ascending by offset.
    pub fn bytes(&self) -> &[RawByte] {
        &self.bytes
    }
}

/// Coordinate/confidentiality description, deliberately not an execution plan.
/// Private fields prevent constructing a forged admitted description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDescription {
    revision: Rev,
    purpose: Purpose,
    targets: Vec<TargetDescription>,
}
impl RawDescription {
    /// The reported revision the description was built for.
    pub fn revision(&self) -> Rev {
        self.revision
    }
    /// The request's read/write purpose.
    pub fn purpose(&self) -> Purpose {
        self.purpose
    }
    /// The per-target descriptions, in request order.
    pub fn targets(&self) -> &[TargetDescription] {
        &self.targets
    }
    /// The payload-free audit projection.
    pub fn audit(&self) -> AuditDescription {
        AuditDescription {
            revision: self.revision,
            purpose: self.purpose,
            range_count: self.targets.len(),
            targets: Some(self.targets.iter().map(TargetDescription::target).collect()),
        }
    }
    /// Exact passive read intent, not selector acquisition or a read transfer
    /// plan. R2 must bind it to supplied facts/generation/limits. Writes have
    /// no implicit pre-read, readback, checksum or write actions in R1.
    pub fn read_intent(&self) -> Option<ReadIntent> {
        (self.purpose == Purpose::Read).then(|| ReadIntent {
            requester: ReadRequester::ExplicitClient,
            exact_ranges: self.targets.iter().map(|t| t.target.range).collect(),
            allowed_effects: vec![ReadEffect::None],
        })
    }
}

/// Secret-free projection for a future durable audit owner. No operation ID
/// is derived from the payload. Refused requests redact all coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
/// Payload-free audit projection of a raw request or refusal.
pub struct AuditDescription {
    revision: Rev,
    purpose: Purpose,
    range_count: usize,
    targets: Option<Vec<RawTarget>>,
}
impl AuditDescription {
    /// The reported revision.
    pub fn revision(&self) -> Rev {
        self.revision
    }
    /// The request's purpose.
    pub fn purpose(&self) -> Purpose {
        self.purpose
    }
    /// How many ranges the request named.
    pub fn range_count(&self) -> usize {
        self.range_count
    }
    /// The named targets, when the description carries them.
    pub fn targets(&self) -> Option<&[RawTarget]> {
        self.targets.as_deref()
    }
}

/// Reasons carry no payload, target coordinates or caller-provided text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RefusalReason {
    /// The request names no range.
    #[error("no raw ranges requested")]
    NoRanges,
    /// More ranges than the R1 capacity.
    #[error("raw range count exceeds R1 capacity")]
    TooManyRanges,
    /// The address is not a legal seven-bit module address.
    #[error("invalid raw address")]
    InvalidAddress,
    /// The address does not match the explicit model.
    #[error("raw address does not match explicit model")]
    AddressModelMismatch,
    /// The range is malformed (empty, or outside the window).
    #[error("invalid raw range")]
    InvalidRange,
    /// The range crosses the lower/upper window boundary.
    #[error("raw range crosses lower/upper window")]
    WindowCrossing,
    /// A lower-memory range named a nonzero bank/page.
    #[error("lower memory requires canonical bank/page zero")]
    NonCanonicalLower,
    /// The family/revision/model/bank/page cell is unsupported, for the
    /// named reason.
    #[error("unsupported raw cell: {0:?}")]
    Unsupported(UnsupportedCell),
    /// A read request carried write material.
    #[error("read request carries write material")]
    UnexpectedPayload,
    /// A write request lacked its counted material.
    #[error("write request lacks counted material")]
    MissingPayload,
    /// The material length differs from the requested range.
    #[error("write material length differs from requested range")]
    PayloadLength,
    /// The request touches secret or write-only memory.
    #[error("raw request overlaps secret or write-only memory")]
    SecretOrWriteOnly,
    /// A raw write touches selector/control/command memory.
    #[error("raw write overlaps selector/control/command memory")]
    ControlOrCommand,
    /// The confidentiality coverage is unknown; fail closed.
    #[error("raw confidentiality coverage is unknown")]
    UnknownCoverage,
    /// The read/readback effect is not admitted.
    #[error("raw read/readback effect is not admitted")]
    ReadEffectNotAdmitted,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
/// A raw-request refusal: typed reason plus payload-free audit projection.
pub struct RawRefusal {
    reason: RefusalReason,
    audit: AuditDescription,
}
impl RawRefusal {
    /// Why the request was refused.
    pub fn reason(&self) -> RefusalReason {
        self.reason
    }
    /// The payload-free audit projection of the refused request.
    pub fn audit(&self) -> &AuditDescription {
        &self.audit
    }
}

fn coordinates(target: RawTarget) -> Result<RawTarget, RefusalReason> {
    let target = target.canonical()?;
    let range = target.range;
    if target.model == AddressModel::Cmis && range.offset < 128 {
        if range.bank != 0 || range.page != 0 {
            return Err(RefusalReason::NonCanonicalLower);
        }
        if range.end() > 128 {
            return Err(RefusalReason::WindowCrossing);
        }
    }
    Ok(target)
}

fn row_at(row: &CatalogRow, revision: Rev) -> bool {
    row.since.is_none_or(|since| revision >= since)
        && row.until.is_none_or(|until| revision < until)
}

fn secret(row: &CatalogRow) -> bool {
    row.tags.contains(&"BlindControl::Password")
        || matches!(row.access, AccessKind::Wo | AccessKind::WoSc)
        || row.read_effects.contains(&ReadEffect::WriteOnly)
}
fn control(row: &CatalogRow) -> bool {
    row.tags
        .iter()
        .any(|t| *t == "Control" || t.starts_with("Control::") || *t == "BlindControl")
        || matches!(row.access, AccessKind::Rw | AccessKind::Rww)
}
fn ordinary(row: &CatalogRow) -> bool {
    // An allowlist, not "anything that is not a password". Explicit custom,
    // reserved and vendor-private semantics override any informational tag.
    matches!(row.access, AccessKind::Ro | AccessKind::RoCor)
        && row.presence == FieldPresence::Unconditional
        && !row
            .tags
            .iter()
            .any(|t| *t == "Reserved" || *t == "VendorSpecific" || t.starts_with("Custom"))
        && row.tags.iter().any(|t| {
            matches!(
                t.split("::").next(),
                Some(
                    "Information"
                        | "CapabilityFlag"
                        | "CapabilityAdvertisement"
                        | "Status"
                        | "Flag"
                        | "AlarmFlag"
                        | "Monitor"
                        | "OperationalParameter"
                )
            )
        })
}

fn unknown() -> RawByte {
    RawByte {
        class: ByteClass::Unknown,
        read: None,
        nonsecret_coverage: false,
    }
}

/// Classify exactly one bounded target, even if its matrix cell is unsupported.
/// Unknown models/revisions get no borrowed semantics from CMIS4. This is a
/// pure inspection surface, NOT a substitute for whole-request `preflight`.
pub fn classify_target(
    revision: Rev,
    target: RawTarget,
) -> Result<TargetDescription, RefusalReason> {
    let target = coordinates(target)?;
    let range = target.range;
    let mut bytes = Vec::with_capacity(usize::from(range.len));
    if revision != Rev::CMIS_4_0 || target.model != AddressModel::Cmis {
        bytes.resize_with(usize::from(range.len), unknown);
    } else if range.offset >= 128
        && range.bank <= 1
        && (range.page == 0x9f || (0xa0..=0xaf).contains(&range.page))
    {
        // CMIS4 §§8.13–8.14 only. Negative authority for staging/trigger/EPL,
        // not positive acquisition or a confidentiality assertion about LPL.
        bytes.resize_with(usize::from(range.len), || RawByte {
            class: ByteClass::ControlOrCommand,
            read: None,
            nonsecret_coverage: false,
        });
    } else if support_cell(revision, target.model, range.bank, range.page).is_err() {
        bytes.resize_with(usize::from(range.len), unknown);
    } else {
        let lower = CmisLowerMemory::read_classifications();
        let upper = CmisPage00hAdministrative::read_classifications();
        let rows = catalog::family_catalog(Family::Cmis)
            .expect("CMIS catalog is delivered")
            .rows;
        for offset in u32::from(range.offset)..range.end() {
            let read = if offset < 128 {
                &lower[offset as usize]
            } else {
                &upper[offset as usize - 128]
            };
            let mut is_secret = read.write_only_mask != 0;
            let mut is_control = false;
            let mut known_mask = 0u8;
            let mut unqualified = false;
            for row in rows.iter().filter(|r| {
                r.address == range.address
                    && r.page == range.page
                    && !r.banked
                    && row_at(r, revision)
            }) {
                let base = [row.offset];
                let offsets = row
                    .instances
                    .as_ref()
                    .map_or(base.as_slice(), |i| i.offsets);
                for (instance, &start) in offsets.iter().enumerate() {
                    if offset < u32::from(start) || offset >= u32::from(start) + u32::from(row.len)
                    {
                        continue;
                    }
                    // Safety taints cover the WHOLE PHYSICAL span, regardless
                    // of field width, read effect, overlapping aliases or
                    // presence. A secret bit makes its entire byte secret.
                    is_secret |= secret(row);
                    is_control |= control(row);
                    let known = ordinary(row)
                        || (control(row) && row.presence == FieldPresence::Unconditional);
                    unqualified |= !known && !secret(row);
                    if known {
                        let bit_offset = row
                            .instances
                            .as_ref()
                            .map_or(row.bit_offset, |i| i.bit_offsets[instance]);
                        let byte_bit = (offset - u32::from(start)) * 8;
                        for bit in 0..8 {
                            if byte_bit + bit >= u32::from(bit_offset)
                                && byte_bit + bit < u32::from(bit_offset) + u32::from(row.bit_width)
                            {
                                known_mask |= 1 << bit;
                            }
                        }
                    }
                }
            }
            let nonsecret_coverage = !is_secret && !unqualified && known_mask == 0xff;
            let class = if is_secret {
                ByteClass::SecretOrWriteOnly
            } else if is_control {
                ByteClass::ControlOrCommand
            } else if nonsecret_coverage {
                ByteClass::OrdinaryReadable
            } else {
                ByteClass::Unknown
            };
            bytes.push(RawByte {
                class,
                read: Some(read.clone()),
                nonsecret_coverage,
            });
        }
    }
    Ok(TargetDescription { target, bytes })
}

/// All-or-nothing R1 preflight. Inspect only payload presence/count, never its
/// contents. No I/O, callback, clock, checksum policy or executable actions.
pub fn preflight(request: &RawRequest<'_>) -> Result<RawDescription, RawRefusal> {
    let refuse = |reason| RawRefusal {
        reason,
        audit: AuditDescription {
            revision: request.revision,
            purpose: request.purpose,
            range_count: request.regions.len(),
            targets: None,
        },
    };
    if request.regions.is_empty() {
        return Err(refuse(RefusalReason::NoRanges));
    }
    if request.regions.len() > MAX_RANGES {
        return Err(refuse(RefusalReason::TooManyRanges));
    }
    // Validate the ENTIRE coordinate set before allocating classifications.
    let targets: Vec<_> = request
        .regions
        .iter()
        .map(|r| coordinates(r.target))
        .collect::<Result<_, _>>()
        .map_err(refuse)?;
    for (region, target) in request.regions.iter().zip(&targets) {
        support_cell(
            request.revision,
            target.model,
            target.range.bank,
            target.range.page,
        )
        .map_err(|cell| refuse(RefusalReason::Unsupported(cell)))?;
        match (request.purpose, region.write_bytes) {
            (Purpose::Read, Some(_)) => return Err(refuse(RefusalReason::UnexpectedPayload)),
            (Purpose::Write, None) => return Err(refuse(RefusalReason::MissingPayload)),
            (Purpose::Write, Some(bytes)) if bytes.len() != usize::from(target.range.len) => {
                return Err(refuse(RefusalReason::PayloadLength))
            }
            _ => {}
        }
    }
    let targets: Vec<_> = targets
        .into_iter()
        .map(|t| classify_target(request.revision, t))
        .collect::<Result<_, _>>()
        .map_err(refuse)?;
    // Classify every byte before deciding. Safety taints outrank a merely
    // unknown prefix, so a partial password overlap is always a secret refusal.
    let bytes = || targets.iter().flat_map(|t| &t.bytes);
    if bytes().any(|b| b.class == ByteClass::SecretOrWriteOnly) {
        return Err(refuse(RefusalReason::SecretOrWriteOnly));
    }
    if request.purpose == Purpose::Write && bytes().any(|b| b.class == ByteClass::ControlOrCommand)
    {
        return Err(refuse(RefusalReason::ControlOrCommand));
    }
    if bytes().any(|b| b.class == ByteClass::Unknown) {
        return Err(refuse(RefusalReason::UnknownCoverage));
    }
    if bytes().any(|b| !b.passive()) {
        return Err(refuse(RefusalReason::ReadEffectNotAdmitted));
    }
    Ok(RawDescription {
        revision: request.revision,
        purpose: request.purpose,
        targets,
    })
}
