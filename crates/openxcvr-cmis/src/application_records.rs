//! Owned, supplied-data application advertising; no I/O or acquisition authority.
//!
//! CMIS 4.0 §8.2.9 Tables 8-12/13 and §8.4.14 Table 8-39;
//! CMIS 5.0 §8.2.11/8.4.14; CMIS 5.1 Tables 8-16/18 and §8.4.14;
//! CMIS 5.2 Tables 8-17/19/20/53; CMIS 5.3 Tables 8-20/22/23/59.
//! Each revision independently specifies slots 1–8 in lower memory and 9–15
//! at Page 01h bytes 223–250, FF as the terminator, and media-table selection
//! by byte 85. Interface names come from the generated shared SFF-8024 registry,
//! not from an assertion that a particular registry code existed at a CMIS date.
//! Zero lane counts are implicit, not zero physical lanes. Assignment bits
//! identify permitted *first* host lanes; the slot index is never a lane.

use crate::{generated::*, ops::application::Application, Rev};

/// One application slot's advertised state (CMIS Table 8-59 codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SlotState {
    /// A valid application descriptor.
    Present = 0,
    /// The FFh list terminator.
    EndOfList = 1,
    /// Explicitly no application.
    Absent = 2,
    /// The slot cannot be read or used.
    Unavailable = 3,
    /// The record is shorter than a full descriptor.
    ShortData = 4,
    /// The record is not defined at the reported revision.
    UnsupportedRevision = 5,
    /// The record's declared revision does not match the module's.
    RevisionMismatch = 6,
}

/// The library-owned interpretation from the generated SFF-8024 registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceMeaning {
    /// A code the shared registry names.
    Known(String),
    /// The registry's explicit Undefined code.
    Undefined,
    /// A code the registry reserves without a name.
    UnknownCode(u8),
    /// A media type outside the decodable tables.
    UnsupportedMediaType(u8),
    /// The host did not supply the media-table selection byte.
    Unavailable,
}

impl InterfaceMeaning {
    /// Generated enum spelling; unknown/reserved media types have no invented name.
    pub fn meaning(&self) -> Option<String> {
        match self {
            Self::Known(name) => Some(name.clone()),
            Self::Undefined => Some("Undefined".into()),
            _ => None,
        }
    }
}

/// One side's advertised lane count (code 0 is defined by the interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneCount {
    /// Lane count comes from the interface code itself.
    DefinedByInterface,
    /// An explicit lane count (1-8).
    Explicit(u8),
    /// A reserved code, carried without interpretation.
    Reserved(u8),
}

impl From<u8> for LaneCount {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::DefinedByInterface,
            1..=8 => Self::Explicit(value),
            _ => Self::Reserved(value),
        }
    }
}

/// One slot's decoded application record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRecord {
    /// The slot index (1-15).
    pub slot: u8,
    /// The slot's advertised state.
    pub state: SlotState,
    /// The decoded application selector, when present.
    pub application: Option<Application>,
    /// The host electrical interface meaning.
    pub host: InterfaceMeaning,
    /// The media interface meaning.
    pub media: InterfaceMeaning,
    /// The host lane count, when the record supplies one.
    pub host_lanes: Option<LaneCount>,
    /// The media lane count, when the record supplies one.
    pub media_lanes: Option<LaneCount>,
}

/// The module's full advertised application list: fifteen slots at the
/// reported revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRecords {
    /// The revision the decode ran at.
    pub revision: Rev,
    /// Always fifteen slots, including unavailable, sentinel and absent slots.
    pub slots: Vec<ApplicationRecord>,
}

macro_rules! meaning {
    ($ty:ident, $code:expr) => {
        match $ty::from($code) {
            $ty::Undefined => InterfaceMeaning::Undefined,
            $ty::Unknown(v) => InterfaceMeaning::UnknownCode(v),
            value => InterfaceMeaning::Known(value.name()),
        }
    };
}

fn media_meaning(media_type: CmisMediaType, code: u8) -> InterfaceMeaning {
    match media_type {
        CmisMediaType::OpticalInterfacesMmf => meaning!(Sff8024MmfMediaInterface, code),
        CmisMediaType::OpticalInterfacesSmf => meaning!(Sff8024SmfMediaInterface, code),
        CmisMediaType::PassiveAndLinearActiveCopperCables => {
            meaning!(Sff8024PassiveCableCodes, code)
        }
        CmisMediaType::ActiveCables => meaning!(Sff8024ActiveCableCodes, code),
        CmisMediaType::BaseT => meaning!(Sff8024BaseTCodes, code),
        other => InterfaceMeaning::UnsupportedMediaType(other.repr()),
    }
}

type Fields<const N: usize> = ([Sff8024HostElectricalInterface; N], [u8; N], [u8; N]);

// No newest-revision fallback. Both page decoders are selected at the reported
// revision, even though the consumed fields presently have identical layouts.
macro_rules! fields {
    ($module:ident, $page:ident, $buf:expr) => {{
        let p = $module::$page::decode($buf);
        (
            p.host_interface_id_app,
            p.host_lane_count_app,
            p.media_lane_count_app,
        )
    }};
}

fn lower_fields(rev: Rev, buf: &[u8; 128]) -> (CmisMediaType, Fields<8>) {
    use crate::generated::{v4_0, v5_0, v5_1, v5_2, v5_3};
    macro_rules! decode {
        ($m:ident) => {{
            let p = $m::CmisLowerMemory::decode(buf);
            (p.media_type, fields!($m, CmisLowerMemory, buf))
        }};
    }
    match rev {
        Rev::CMIS_4_0 => decode!(v4_0),
        Rev::CMIS_5_0 => decode!(v5_0),
        Rev::CMIS_5_1 => decode!(v5_1),
        Rev::CMIS_5_2 => decode!(v5_2),
        Rev::CMIS_5_3 => decode!(v5_3),
        _ => unreachable!("checked revision"),
    }
}

fn upper_fields(rev: Rev, buf: &[u8; 128]) -> Fields<7> {
    use crate::generated::{v4_0, v5_0, v5_1, v5_2, v5_3};
    match rev {
        Rev::CMIS_4_0 => fields!(v4_0, CmisPage01hAdvertising, buf),
        Rev::CMIS_5_0 => fields!(v5_0, CmisPage01hAdvertising, buf),
        Rev::CMIS_5_1 => fields!(v5_1, CmisPage01hAdvertising, buf),
        Rev::CMIS_5_2 => fields!(v5_2, CmisPage01hAdvertising, buf),
        Rev::CMIS_5_3 => fields!(v5_3, CmisPage01hAdvertising, buf),
        _ => unreachable!("checked revision"),
    }
}

fn page(bytes: Option<&[u8]>) -> Result<&[u8; 128], SlotState> {
    let bytes = bytes.ok_or(SlotState::Unavailable)?;
    bytes
        .get(..128)
        .and_then(|p| p.try_into().ok())
        .ok_or(SlotState::ShortData)
}

fn empty(slot: u8, state: SlotState) -> ApplicationRecord {
    ApplicationRecord {
        slot,
        state,
        application: None,
        host: InterfaceMeaning::Unavailable,
        media: InterfaceMeaning::Unavailable,
        host_lanes: None,
        media_lanes: None,
    }
}

/// Decode the first 128 bytes of lower memory and the 128 *upper* bytes of
/// Page 01h. Short pages are not padded. Missing lower memory also leaves
/// upper records unavailable (revision and media type cannot be established).
/// Missing/short Page 01h remains explicit even if a lower sentinel was seen.
/// Unsupported revisions and a supplied revision-byte mismatch fail closed in
/// every slot; this function never falls back to a newer decoder.
pub fn application_records(
    revision: Rev,
    lower: Option<&[u8]>,
    page01: Option<&[u8]>,
) -> ApplicationRecords {
    let mut result = ApplicationRecords {
        revision,
        slots: (1..=15).map(|s| empty(s, SlotState::Unavailable)).collect(),
    };
    let gate = if !matches!(
        revision,
        Rev::CMIS_4_0 | Rev::CMIS_5_0 | Rev::CMIS_5_1 | Rev::CMIS_5_2 | Rev::CMIS_5_3
    ) {
        Err(SlotState::UnsupportedRevision)
    } else {
        page(lower).and_then(|p| {
            if p[1] == revision.major << 4 | revision.minor {
                Ok(p)
            } else {
                Err(SlotState::RevisionMismatch)
            }
        })
    };
    let lower = match gate {
        Ok(p) => p,
        Err(state) => {
            for record in &mut result.slots {
                record.state = state;
            }
            return result;
        }
    };
    let (media_type, lo) = lower_fields(revision, lower);
    let upper = page(page01).map(|p| (p, upper_fields(revision, p)));
    let mut ended = false;
    for record in &mut result.slots {
        let slot = record.slot;
        let (host, host_lanes, media_lanes, assignments, media_code) = if slot <= 8 {
            let i = usize::from(slot - 1);
            (
                lo.0[i],
                lo.1[i],
                lo.2[i],
                lower[89 + 4 * i],
                lower[87 + 4 * i],
            )
        } else {
            match &upper {
                Err(state) => {
                    record.state = *state;
                    continue;
                }
                Ok((p, hi)) => {
                    let i = usize::from(slot - 9);
                    (hi.0[i], hi.1[i], hi.2[i], p[98 + 4 * i], p[96 + 4 * i])
                }
            }
        };
        if ended {
            record.state = SlotState::Absent;
            continue;
        }
        if host == Sff8024HostElectricalInterface::EndOfList {
            record.state = SlotState::EndOfList;
            ended = true;
            continue;
        }
        record.state = SlotState::Present;
        record.host = meaning!(Sff8024HostElectricalInterface, host.repr());
        record.media = media_meaning(media_type, media_code);
        record.host_lanes = Some(host_lanes.into());
        record.media_lanes = Some(media_lanes.into());
        // Like ops/application.rs, media variants are skipped by page codegen;
        // the byte itself is specified by the revision's tables cited above.
        record.application = Some(Application {
            app_sel_code: slot,
            host_interface_code: host.repr(),
            media_interface_code: media_code,
            host_lane_count: host_lanes,
            media_lane_count: media_lanes,
            host_lane_assignment_options: assignments,
        });
    }
    result
}

#[path = "application_sparse.rs"]
mod sparse;
pub use sparse::{
    application_records_footprint, application_records_from_ranges, ApplicationFootprint,
    ApplicationRange, ApplicationRangeError,
};
