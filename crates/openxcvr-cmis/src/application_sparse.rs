//! Schema-catalog dependency binding for the supplied-data application decoder.
use super::*;
use openxcvr_core::{
    field_catalog::{resolve, CatalogError, FieldEntry, FieldQuery, LaneSelection},
    types::management::Range,
};

/// Decoder dependencies, not read/selector authority. Offsets are physical
/// (Page 01h is upper memory). Sorted, disjoint, adjacent spans are coalesced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationFootprint {
    /// The revision the footprint was resolved at.
    pub revision: Rev,
    /// The coalesced physical ranges the decoder consumes.
    pub ranges: Vec<Range>,
}

/// One actual supplied fragment. `bytes.len()` must equal `range.len`.
/// Lower memory uses page/bank 0; Page 01h uses bank 0 and offsets 128..255.
#[derive(Debug, Clone, Copy)]
pub struct ApplicationRange<'a> {
    /// The physical range the fragment covers.
    pub range: Range,
    /// The fragment's bytes.
    pub bytes: &'a [u8],
}

/// Why a supplied application fragment was rejected.
#[derive(Debug)]
pub enum ApplicationRangeError {
    /// The range is malformed.
    InvalidRange,
    /// Two fragments cover overlapping bytes.
    Overlap,
    /// Catalog resolution failed; the original error is the source.
    Metadata(CatalogError),
}

impl std::fmt::Display for ApplicationRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => f.write_str("invalid application supplied range"),
            Self::Overlap => f.write_str("overlapping application supplied ranges"),
            Self::Metadata(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ApplicationRangeError {}

struct Dependencies {
    revision: FieldEntry<'static>,
    media: FieldEntry<'static>,
    // host ID, media ID, host lanes, media lanes, host assignment options.
    slots: Vec<Vec<FieldEntry<'static>>>,
}

fn supported(rev: Rev) -> bool {
    matches!(
        rev,
        Rev::CMIS_4_0 | Rev::CMIS_5_0 | Rev::CMIS_5_1 | Rev::CMIS_5_2 | Rev::CMIS_5_3
    )
}

fn dependency(
    rev: Rev,
    page: &str,
    name: &str,
    lane: LaneSelection,
) -> Result<FieldEntry<'static>, CatalogError> {
    let entries = resolve(
        &crate::generated::catalog::CATALOG,
        rev,
        &FieldQuery {
            pattern: name,
            page: Some(page),
            lane,
            bank: 0,
        },
    )?;
    // All application constituents are scalar byte/nibble fields. If a future
    // schema changes that contract, refuse rather than truncate a field.
    match entries.as_slice() {
        [entry]
            if entry.row.len == 1
                && entry.width_bits() > 0
                && entry.width_bits() + u16::from(entry.bit_offset) <= 8 =>
        {
            Ok(*entry)
        }
        _ => Err(CatalogError::NoMatch {
            pattern: format!("application scalar {page}::{name}"),
            rev,
        }),
    }
}

fn dependencies(rev: Rev) -> Result<Dependencies, CatalogError> {
    if !supported(rev) {
        return Err(CatalogError::UnsupportedRevision { rev });
    }
    let revision = dependency(
        rev,
        "CmisLowerMemory",
        "cmis_revision",
        LaneSelection::Unspecified,
    )?;
    let media = dependency(
        rev,
        "CmisLowerMemory",
        "media_type",
        LaneSelection::Unspecified,
    )?;
    let mut slots = Vec::with_capacity(15);
    for (page, count) in [("CmisLowerMemory", 8), ("CmisPage01hAdvertising", 7)] {
        for i in 1..=count {
            slots.push(
                [
                    "host_interface_id_app",
                    "media_interface_id_app",
                    "host_lane_count_app",
                    "media_lane_count_app",
                    "host_lane_assignment_options_app",
                ]
                .into_iter()
                .map(|name| dependency(rev, page, name, LaneSelection::One(i)))
                .collect::<Result<_, _>>()?,
            );
        }
    }
    Ok(Dependencies {
        revision,
        media,
        slots,
    })
}

/// Return the exact maximum byte dependency set at the selected revision.
/// Unsupported revisions (including CMIS 3.0) and missing generated metadata
/// are explicit errors. No newest-revision fallback or acquisition is performed.
pub fn application_records_footprint(revision: Rev) -> Result<ApplicationFootprint, CatalogError> {
    let d = dependencies(revision)?;
    let mut fields: Vec<_> = std::iter::once(d.revision)
        .chain(std::iter::once(d.media))
        .chain(d.slots.into_iter().flatten())
        .map(|e| e.range())
        .collect();
    fields.sort_by_key(|r| (r.address, r.bank, r.page, r.offset));
    let mut ranges: Vec<Range> = Vec::new();
    for r in fields {
        if let Some(last) = ranges.last_mut() {
            if last.address == r.address
                && last.bank == r.bank
                && last.page == r.page
                && last.end() >= u32::from(r.offset)
            {
                last.len = (last.end().max(r.end()) - u32::from(last.offset)) as u16;
                continue;
            }
        }
        ranges.push(r);
    }
    Ok(ApplicationFootprint { revision, ranges })
}

fn validate(supplied: &[ApplicationRange<'_>]) -> Result<(), ApplicationRangeError> {
    if supplied.len() > 256 {
        return Err(ApplicationRangeError::InvalidRange);
    }
    for (i, s) in supplied.iter().enumerate() {
        let r = s.range;
        if r.validate().is_err()
            || s.bytes.len() != usize::from(r.len)
            || r.address != 0x50
            || r.bank != 0
            || !((r.page == 0 && r.end() <= 128) || (r.page == 1 && r.offset >= 128))
        {
            return Err(ApplicationRangeError::InvalidRange);
        }
        if supplied[..i].iter().any(|p| {
            p.range.page == r.page
                && u32::from(p.range.offset) < r.end()
                && u32::from(r.offset) < p.range.end()
        }) {
            return Err(ApplicationRangeError::Overlap);
        }
    }
    Ok(())
}

fn scalar(entry: &FieldEntry<'_>, supplied: &[ApplicationRange<'_>]) -> Option<u8> {
    let field = entry.range();
    // Check the entire generated span, even for bit fields, before reading a
    // value. Coverage can come from adjacent fragments, never backing storage.
    if !(u32::from(field.offset)..field.end()).all(|byte| {
        supplied.iter().any(|s| {
            s.range.address == field.address
                && s.range.bank == field.bank
                && s.range.page == field.page
                && u32::from(s.range.offset) <= byte
                && s.range.end() > byte
        })
    }) {
        return None;
    }
    let s = supplied.iter().find(|s| s.range.contains(field))?;
    let byte = s.bytes[usize::from(field.offset - s.range.offset)];
    Some((byte >> entry.bit_offset) & ((1u16 << entry.width_bits()) - 1) as u8)
}

/// Decode only actual supplied fragments, without I/O, invented pages or reads
/// of unsupplied bytes. Malformed/overlapping fragments are errors (even when
/// overlapping values agree). At most 256 fragments, in any order, are accepted.
///
/// Each complete slot decodes independently. A missing lower dependency yields
/// ShortData, or Unavailable if no lower fragment was supplied at all. Revision
/// and media type are common lower dependencies; missing either gates all slots.
/// A supplied revision mismatch gates all slots as RevisionMismatch. Page 01h
/// absent means upper slots Unavailable, even after a lower FFh; partially
/// supplied slots are ShortData. Coverage is checked before sentinel/Absent
/// handling, so an unsupplied slot is never inferred absent. Only a supplied,
/// complete sentinel slot ends the list. Unsupported revisions are per-slot
/// UnsupportedRevision, as in the full-page API.
pub fn application_records_from_ranges(
    revision: Rev,
    supplied: &[ApplicationRange<'_>],
) -> Result<ApplicationRecords, ApplicationRangeError> {
    validate(supplied)?;
    let mut result = ApplicationRecords {
        revision,
        slots: (1..=15).map(|s| empty(s, SlotState::Unavailable)).collect(),
    };
    let lower_present = supplied.iter().any(|s| s.range.page == 0);
    let missing = if lower_present {
        SlotState::ShortData
    } else {
        SlotState::Unavailable
    };
    let gate = |state| {
        let mut r = result.clone();
        for s in &mut r.slots {
            s.state = state;
        }
        r
    };
    if !supported(revision) {
        return Ok(gate(SlotState::UnsupportedRevision));
    }
    let d = dependencies(revision).map_err(ApplicationRangeError::Metadata)?;
    let Some(code) = scalar(&d.revision, supplied) else {
        return Ok(gate(missing));
    };
    if code != revision.major << 4 | revision.minor {
        return Ok(gate(SlotState::RevisionMismatch));
    }
    let Some(media) = scalar(&d.media, supplied) else {
        return Ok(gate(missing));
    };
    let media_type = CmisMediaType::from(media);
    let upper_present = supplied.iter().any(|s| s.range.page == 1);
    let mut ended = false;
    for (record, fields) in result.slots.iter_mut().zip(d.slots) {
        let values: Option<Vec<_>> = fields.iter().map(|e| scalar(e, supplied)).collect();
        let Some(v) = values else {
            record.state = if record.slot > 8 && !upper_present {
                SlotState::Unavailable
            } else {
                SlotState::ShortData
            };
            continue;
        };
        if ended {
            record.state = SlotState::Absent;
            continue;
        }
        let host = Sff8024HostElectricalInterface::from(v[0]);
        if host == Sff8024HostElectricalInterface::EndOfList {
            record.state = SlotState::EndOfList;
            ended = true;
            continue;
        }
        record.state = SlotState::Present;
        record.host = meaning!(Sff8024HostElectricalInterface, host.repr());
        record.media = media_meaning(media_type, v[1]);
        record.host_lanes = Some(v[2].into());
        record.media_lanes = Some(v[3].into());
        record.application = Some(Application {
            app_sel_code: record.slot,
            host_interface_code: v[0],
            media_interface_code: v[1],
            host_lane_count: v[2],
            media_lane_count: v[3],
            host_lane_assignment_options: v[4],
        });
    }
    Ok(result)
}
