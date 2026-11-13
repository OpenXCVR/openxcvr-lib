//! Pure supplied-data rich records. These functions do not authorize acquisition.
//!
//! Applications currently support CMIS 4.0 and 5.0–5.3. The selected revision
//! must match the supplied lower-memory revision byte. See the family module
//! for exact page shape, sentinel and availability semantics.
pub use openxcvr_cmis::application_records::{
    application_records, application_records_footprint, application_records_from_ranges,
    ApplicationFootprint, ApplicationRange, ApplicationRangeError, ApplicationRecord,
    ApplicationRecords, InterfaceMeaning, LaneCount, SlotState,
};
