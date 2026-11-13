//! Bounded ownership bridge to the Rust supplied-data APIs. No protocol tables.
use std::{ffi::CString, mem::size_of, os::raw::c_char, panic::catch_unwind, ptr, slice};

use openxcvr::{
    core::Rev,
    rich::{
        application_records, application_records_footprint, application_records_from_ranges,
        ApplicationRange, ApplicationRecords, InterfaceMeaning, LaneCount,
    },
};

#[repr(C)]
pub struct ApplicationRequest {
    pub size: usize,
    pub version: u32,
    pub revision_major: u8,
    pub revision_minor: u8,
    pub lower: *const u8,
    pub lower_len: usize,
    pub page01: *const u8,
    pub page01_len: usize,
}

#[repr(C)]
pub struct ApplicationRecord {
    pub slot: u8,
    pub has_application: u8,
    pub host_interface_code: u8,
    pub media_interface_code: u8,
    pub host_lane_count: u8,
    pub media_lane_count: u8,
    pub host_lane_assignment_options: u8,
    pub unsupported_media_type: u8,
    pub state: u32,
    pub host_meaning_state: u32,
    pub media_meaning_state: u32,
    pub host_lane_count_state: u32,
    pub media_lane_count_state: u32,
    pub host_meaning: *const c_char,
    pub media_meaning: *const c_char,
}

#[repr(C)]
pub struct ApplicationResult {
    pub size: usize,
    pub version: u32,
    pub revision_major: u8,
    pub revision_minor: u8,
    pub count: usize,
    pub records: *const ApplicationRecord,
}

// The C-visible prefix is first and immutable until free. Boxes keep records
// and strings stable while the owning result itself moves into its Box.
#[repr(C)]
struct OwnedResult {
    public: ApplicationResult,
    records: Box<[ApplicationRecord]>,
    meanings: Vec<CString>,
}

fn meaning_state(value: &InterfaceMeaning) -> u32 {
    match value {
        InterfaceMeaning::Known(_) => 0,
        InterfaceMeaning::Undefined => 1,
        InterfaceMeaning::UnknownCode(_) => 2,
        InterfaceMeaning::UnsupportedMediaType(_) => 3,
        InterfaceMeaning::Unavailable => 4,
    }
}

fn lane_state(value: Option<LaneCount>) -> u32 {
    match value {
        Some(LaneCount::DefinedByInterface) => 0,
        Some(LaneCount::Explicit(_)) => 1,
        Some(LaneCount::Reserved(_)) => 2,
        None => 3,
    }
}

fn meaning(value: &InterfaceMeaning, strings: &mut Vec<CString>) -> *const c_char {
    match value.meaning() {
        None => ptr::null(),
        Some(s) => {
            // Generated enum names contain no NUL; an unexpected name fails
            // the call under catch_unwind before any result is published.
            let name = CString::new(s).expect("generated enum spelling");
            let p = name.as_ptr();
            strings.push(name);
            p
        }
    }
}

unsafe fn supplied<'a>(bytes: *const u8, len: usize) -> Option<&'a [u8]> {
    if bytes.is_null() {
        None
    } else {
        // SAFETY: caller guarantees readable counted input, len <= 128 is
        // checked before entry, and the slice cannot escape the decode call.
        Some(unsafe { slice::from_raw_parts(bytes, len) })
    }
}

/// Internal symbol called by application_records.c.
///
/// # Safety
/// `out` is NULL or writable for one pointer. `request` is NULL or aligned and
/// readable for its size/version prefix, and (if size is sufficient) the full
/// v1 request. Each non-NULL input is readable for its counted length. See the
/// public C header for ownership and lifecycle requirements.
#[no_mangle]
pub unsafe extern "C" fn openxcvr_rich_application_records_impl(
    request: *const ApplicationRequest,
    out: *mut *const ApplicationResult,
) -> i32 {
    if out.is_null() {
        return 1;
    }
    // SAFETY: guaranteed writable by the function contract.
    unsafe { out.write(ptr::null()) };
    if request.is_null() {
        return 1;
    }
    // Read only the size prefix before forming a reference to the full struct.
    // SAFETY: the caller supplies at least the prefix, even for a short request.
    if unsafe { ptr::addr_of!((*request).size).read() } < size_of::<ApplicationRequest>() {
        return 1;
    }
    // SAFETY: full struct readability follows from the contract and size check.
    let request = unsafe { &*request };
    if request.version != 1
        || request.lower_len > 128
        || request.page01_len > 128
        || (request.lower.is_null() && request.lower_len != 0)
        || (request.page01.is_null() && request.page01_len != 0)
    {
        return 1;
    }
    // Only scalar fields and pointers are captured; no mutable library state.
    let decoded = catch_unwind(|| {
        let revision = Rev {
            major: request.revision_major,
            minor: request.revision_minor,
            ..Rev::CMIS_4_0
        };
        // SAFETY: counted inputs validated above; caller guarantees readability.
        let result = unsafe {
            application_records(
                revision,
                supplied(request.lower, request.lower_len),
                supplied(request.page01, request.page01_len),
            )
        };
        owned_result(result)
    });
    match decoded {
        Ok(result) => {
            // SAFETY: C prefix is at offset zero (repr(C)); the result owns all
            // transitively published memory until the matching free function.
            unsafe { out.write(Box::into_raw(result).cast()) };
            0
        }
        Err(_) => 3, // OPENXCVR_ERR_IO: no partial result or unwinding across C
    }
}

/// # Safety
/// NULL or an original, live result from the matching decode, exactly once.
#[no_mangle]
pub unsafe extern "C" fn openxcvr_rich_application_records_free_impl(
    result: *const ApplicationResult,
) {
    if !result.is_null() {
        // SAFETY: pointer originated as Box<OwnedResult>, with C prefix at zero.
        drop(unsafe { Box::from_raw(result.cast_mut().cast::<OwnedResult>()) });
    }
}

fn owned_result(result: ApplicationRecords) -> Box<OwnedResult> {
    let mut meanings = Vec::with_capacity(30);
    let records: Box<[_]> = result
        .slots
        .iter()
        .map(|record| {
            let app = record.application.as_ref();
            ApplicationRecord {
                slot: record.slot,
                state: record.state as u32,
                has_application: u8::from(app.is_some()),
                host_interface_code: app.map_or(0, |a| a.host_interface_code),
                media_interface_code: app.map_or(0, |a| a.media_interface_code),
                host_lane_count: app.map_or(0, |a| a.host_lane_count),
                media_lane_count: app.map_or(0, |a| a.media_lane_count),
                host_lane_assignment_options: app.map_or(0, |a| a.host_lane_assignment_options),
                unsupported_media_type: match record.media {
                    InterfaceMeaning::UnsupportedMediaType(code) => code,
                    _ => 0,
                },
                host_meaning_state: meaning_state(&record.host),
                media_meaning_state: meaning_state(&record.media),
                host_lane_count_state: lane_state(record.host_lanes),
                media_lane_count_state: lane_state(record.media_lanes),
                host_meaning: meaning(&record.host, &mut meanings),
                media_meaning: meaning(&record.media, &mut meanings),
            }
        })
        .collect();
    Box::new(OwnedResult {
        public: ApplicationResult {
            size: size_of::<ApplicationResult>(),
            version: 1,
            revision_major: result.revision.major,
            revision_minor: result.revision.minor,
            count: records.len(),
            records: records.as_ptr(),
        },
        records,
        meanings,
    })
}

#[repr(C)]
pub struct FootprintRequest {
    pub size: usize,
    pub version: u32,
    pub revision_major: u8,
    pub revision_minor: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApplicationLocation {
    pub address: u8,
    pub bank: u8,
    pub page: u8,
    pub offset: u8,
    pub len: u16,
}

impl ApplicationLocation {
    fn range(self) -> openxcvr::core::types::management::Range {
        openxcvr::core::types::management::Range {
            address: self.address,
            bank: self.bank,
            page: self.page,
            offset: self.offset,
            len: self.len,
        }
    }
}

#[repr(C)]
pub struct FootprintResult {
    pub size: usize,
    pub version: u32,
    pub revision_major: u8,
    pub revision_minor: u8,
    pub count: usize,
    pub ranges: *const ApplicationLocation,
}

#[repr(C)]
struct OwnedFootprint {
    public: FootprintResult,
    ranges: Box<[ApplicationLocation]>,
}

#[repr(C)]
pub struct SuppliedApplicationRange {
    pub range: ApplicationLocation,
    pub bytes: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct SparseRequest {
    pub size: usize,
    pub version: u32,
    pub revision_major: u8,
    pub revision_minor: u8,
    pub ranges: *const SuppliedApplicationRange,
    pub count: usize,
}

/// # Safety
/// Same prefix/readable request and writable out contract as the full-page
/// function. Published storage must be freed only with the matching free.
#[no_mangle]
pub unsafe extern "C" fn openxcvr_rich_application_records_footprint_impl(
    request: *const FootprintRequest,
    out: *mut *const FootprintResult,
) -> i32 {
    if out.is_null() {
        return 1;
    }
    // SAFETY: caller guarantees writable out and readable request prefix.
    unsafe {
        out.write(ptr::null());
    }
    if request.is_null()
        || unsafe { ptr::addr_of!((*request).size).read() } < size_of::<FootprintRequest>()
    {
        return 1;
    }
    // SAFETY: caller's size covers the complete request.
    let request = unsafe { &*request };
    if request.version != 1 {
        return 1;
    }
    let decoded = catch_unwind(|| {
        let revision = Rev {
            major: request.revision_major,
            minor: request.revision_minor,
            ..Rev::CMIS_4_0
        };
        let fp = application_records_footprint(revision).map_err(|e| match e {
            openxcvr::core::field_catalog::CatalogError::UnsupportedRevision { .. } => 5,
            _ => 6,
        })?;
        let ranges: Box<[_]> = fp
            .ranges
            .into_iter()
            .map(|r| ApplicationLocation {
                address: r.address,
                bank: r.bank,
                page: r.page,
                offset: r.offset,
                len: r.len,
            })
            .collect();
        Ok::<_, i32>(Box::new(OwnedFootprint {
            public: FootprintResult {
                size: size_of::<FootprintResult>(),
                version: 1,
                revision_major: revision.major,
                revision_minor: revision.minor,
                count: ranges.len(),
                ranges: ranges.as_ptr(),
            },
            ranges,
        }))
    });
    match decoded {
        Ok(Ok(result)) => {
            // SAFETY: repr(C) prefix at offset zero, owned until free.
            unsafe {
                out.write(Box::into_raw(result).cast());
            }
            0
        }
        Ok(Err(status)) => status,
        Err(_) => 3,
    }
}

/// # Safety
/// NULL or the original live footprint result, exactly once.
#[no_mangle]
pub unsafe extern "C" fn openxcvr_rich_application_records_footprint_free_impl(
    result: *const FootprintResult,
) {
    if !result.is_null() {
        // SAFETY: original allocation and repr(C) prefix contract.
        drop(unsafe { Box::from_raw(result.cast_mut().cast::<OwnedFootprint>()) });
    }
}

/// # Safety
/// Request is NULL or readable for its prefix/full declared size, out is NULL
/// or writable; ranges/count and bytes/len must be readable until return. No
/// input storage is retained. See application_records.h for bounds/ownership.
#[no_mangle]
pub unsafe extern "C" fn openxcvr_rich_application_records_from_ranges_impl(
    request: *const SparseRequest,
    out: *mut *const ApplicationResult,
) -> i32 {
    if out.is_null() {
        return 1;
    }
    // SAFETY: writable out guaranteed by caller.
    unsafe {
        out.write(ptr::null());
    }
    if request.is_null()
        || unsafe { ptr::addr_of!((*request).size).read() } < size_of::<SparseRequest>()
    {
        return 1;
    }
    // SAFETY: caller's size covers the full request.
    let request = unsafe { &*request };
    if request.version != 1
        || request.count > 256
        || (request.ranges.is_null() && request.count != 0)
    {
        return 1;
    }
    let decoded = catch_unwind(|| {
        let revision = Rev {
            major: request.revision_major,
            minor: request.revision_minor,
            ..Rev::CMIS_4_0
        };
        let ranges = if request.count == 0 {
            &[]
        } else {
            // SAFETY: bounded count, non-NULL, caller guarantees readable array.
            unsafe { slice::from_raw_parts(request.ranges, request.count) }
        };
        let mut supplied_ranges = Vec::with_capacity(ranges.len());
        for s in ranges {
            if s.bytes.is_null() || s.len == 0 || s.len > 128 || s.len != usize::from(s.range.len) {
                return Err(1);
            }
            // SAFETY: bounded non-NULL bytes, caller guarantees readability.
            let bytes = unsafe { slice::from_raw_parts(s.bytes, s.len) };
            supplied_ranges.push(ApplicationRange {
                range: s.range.range(),
                bytes,
            });
        }
        let records = application_records_from_ranges(revision, &supplied_ranges).map_err(|_| 1)?;
        Ok(owned_result(records))
    });
    match decoded {
        Ok(Ok(result)) => {
            // SAFETY: repr(C) prefix; transitively owned until free.
            unsafe {
                out.write(Box::into_raw(result).cast());
            }
            0
        }
        Ok(Err(status)) => status,
        Err(_) => 3,
    }
}
