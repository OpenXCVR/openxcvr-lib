//! Consumer test kit: an in-memory [`Module`](crate::module::Module), a
//! synthetic page builder, and assertion helpers.
//!
//! Nothing in this module performs I/O, reads a clock, or keeps a resident
//! cache. [`MockModule`] returns only values the caller stored; an accessor
//! the caller did not set returns [`ModuleError::Unsupported`] instead of a
//! zero that could be mistaken for a real reading. [`PageBuilder`] only
//! writes the buffer the caller allocated.

use crate::error::ModuleError;
use crate::module::Module;
use crate::types::{Application, ChecksumStatus, LaneStatus, ModuleFlags, ModuleState, VendorInfo};

/// In-memory [`Module`](crate::module::Module) for tests that must not touch
/// hardware.
///
/// Each accessor is unset until a builder method stores a value. Unset
/// accessors fail closed with [`ModuleError::Unsupported`]. A stored
/// `Option::None` (temperature or supply with no reading) is different: the
/// monitor exists and currently has no sample.
#[derive(Debug, Clone, Default)]
pub struct MockModule {
    vendor_info: Option<VendorInfo>,
    identifier: Option<u8>,
    temperature_celsius: Option<Option<f32>>,
    supply_voltage_v: Option<Option<f32>>,
    lane_count: Option<u8>,
    lanes: Option<Vec<LaneStatus>>,
    flags: Option<ModuleFlags>,
    checksums: Option<ChecksumStatus>,
    applications: Option<Vec<Application>>,
    module_state: Option<ModuleState>,
}

impl MockModule {
    /// A module with every accessor unset.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store the vendor identity block `vendor_info()` will return.
    pub fn with_vendor_info(mut self, vendor_info: VendorInfo) -> Self {
        self.vendor_info = Some(vendor_info);
        self
    }

    /// Store the SFF-8024 identifier byte `identifier()` will return.
    pub fn with_identifier(mut self, identifier: u8) -> Self {
        self.identifier = Some(identifier);
        self
    }

    /// Store the temperature monitor. `None` means the monitor is implemented
    /// but has no sample; leaving the builder unset means unsupported.
    pub fn with_temperature_celsius(mut self, temperature_celsius: Option<f32>) -> Self {
        self.temperature_celsius = Some(temperature_celsius);
        self
    }

    /// Store the supply-voltage monitor. `None` means implemented with no
    /// sample; leaving the builder unset means unsupported.
    pub fn with_supply_voltage_v(mut self, supply_voltage_v: Option<f32>) -> Self {
        self.supply_voltage_v = Some(supply_voltage_v);
        self
    }

    /// Store the lane count `lane_count()` will return.
    pub fn with_lane_count(mut self, lane_count: u8) -> Self {
        self.lane_count = Some(lane_count);
        self
    }

    /// Store the per-lane status `lanes()` will return.
    ///
    /// [`LaneStatus::index`] is one-based (`1` is the first lane),
    /// matching the adapters and the binding JSON lane records.
    pub fn with_lanes(mut self, lanes: Vec<LaneStatus>) -> Self {
        self.lanes = Some(lanes);
        self
    }

    /// Store the module flags `flags()` will return.
    pub fn with_flags(mut self, flags: ModuleFlags) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Store the checksum status `checksums()` will return.
    pub fn with_checksums(mut self, checksums: ChecksumStatus) -> Self {
        self.checksums = Some(checksums);
        self
    }

    /// Store application descriptors. Unset means [`ModuleError::Unsupported`].
    pub fn with_applications(mut self, applications: Vec<Application>) -> Self {
        self.applications = Some(applications);
        self
    }

    /// Store synthesized module state. Unset means [`ModuleError::Unsupported`].
    pub fn with_module_state(mut self, module_state: ModuleState) -> Self {
        self.module_state = Some(module_state);
        self
    }
}

impl Module for MockModule {
    fn vendor_info(&self) -> Result<VendorInfo, ModuleError> {
        self.vendor_info.clone().ok_or(ModuleError::Unsupported)
    }

    fn identifier(&self) -> Result<u8, ModuleError> {
        self.identifier.ok_or(ModuleError::Unsupported)
    }

    fn temperature_celsius(&self) -> Result<Option<f32>, ModuleError> {
        self.temperature_celsius.ok_or(ModuleError::Unsupported)
    }

    fn supply_voltage_v(&self) -> Result<Option<f32>, ModuleError> {
        self.supply_voltage_v.ok_or(ModuleError::Unsupported)
    }

    fn lane_count(&self) -> Result<u8, ModuleError> {
        self.lane_count.ok_or(ModuleError::Unsupported)
    }

    fn lanes(&self) -> Result<Vec<LaneStatus>, ModuleError> {
        self.lanes.clone().ok_or(ModuleError::Unsupported)
    }

    fn flags(&self) -> Result<ModuleFlags, ModuleError> {
        self.flags.clone().ok_or(ModuleError::Unsupported)
    }

    fn checksums(&self) -> Result<ChecksumStatus, ModuleError> {
        self.checksums.clone().ok_or(ModuleError::Unsupported)
    }

    fn applications(&self) -> Result<Vec<Application>, ModuleError> {
        self.applications.clone().ok_or(ModuleError::Unsupported)
    }

    fn module_state(&self) -> Result<ModuleState, ModuleError> {
        self.module_state.clone().ok_or(ModuleError::Unsupported)
    }
}

/// Fixed-length synthetic page. The buffer length is chosen up front and
/// never grows; a write that would pass the end is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageBuilder {
    bytes: Vec<u8>,
}

impl PageBuilder {
    /// A zero-filled page of `len` bytes.
    pub fn new(len: usize) -> Self {
        Self {
            bytes: vec![0; len],
        }
    }

    /// Byte length of the page.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when the page was allocated with length zero.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Write one byte at `offset`.
    pub fn set_u8(&mut self, offset: usize, value: u8) -> Result<(), TestKitError> {
        self.ensure(offset, 1)?;
        self.bytes[offset] = value;
        Ok(())
    }

    /// Write a big-endian `u16` at `offset`.
    pub fn set_be_u16(&mut self, offset: usize, value: u16) -> Result<(), TestKitError> {
        self.fill(offset, &value.to_be_bytes())
    }

    /// Copy `bytes` starting at `offset`.
    pub fn fill(&mut self, offset: usize, bytes: &[u8]) -> Result<(), TestKitError> {
        self.ensure(offset, bytes.len())?;
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// Borrow the page bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Copy the page bytes out for a binding that takes an owned buffer.
    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    fn ensure(&self, offset: usize, needed: usize) -> Result<(), TestKitError> {
        let end = offset.saturating_add(needed);
        if needed == 0 || end > self.bytes.len() || end < offset {
            return Err(TestKitError::PageWrite {
                offset,
                needed,
                len: self.bytes.len(),
            });
        }
        Ok(())
    }
}

/// Failure from the page builder or an assertion helper.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TestKitError {
    /// A write did not fit in the allocated page.
    #[error("page write at offset {offset} needs {needed} bytes but page length is {len}")]
    PageWrite {
        /// First byte the write addressed.
        offset: usize,
        /// Number of bytes the write needed.
        needed: usize,
        /// Allocated page length.
        len: usize,
    },
    /// A lane record did not match the expectation.
    #[error("lane {index} field {field}: {detail}")]
    LaneMismatch {
        /// [`LaneStatus::index`] that failed (one-based).
        index: u8,
        /// Field name inside [`LaneStatus`].
        field: &'static str,
        /// What differed.
        detail: String,
    },
    /// The set of lane indexes did not match.
    #[error("lane indexes: {detail}")]
    LaneSet {
        /// What differed.
        detail: String,
    },
    /// A scalar monitor comparison failed.
    #[error("{field}: {detail}")]
    MonitorMismatch {
        /// Caller-supplied field name.
        field: String,
        /// What differed.
        detail: String,
    },
}

/// Fields of one [`LaneStatus`] a test wants to check.
///
/// `None` means "do not compare this field". Index is always compared and
/// is one-based, matching [`LaneStatus::index`].
#[derive(Debug, Clone, Default)]
pub struct LaneExpectation {
    /// One-based lane index: `1` is the first lane.
    pub index: u8,
    /// Expected receive loss-of-signal, when set.
    pub rx_los: Option<bool>,
    /// Expected transmit loss-of-signal, when set.
    pub tx_los: Option<bool>,
    /// Expected transmit fault, when set.
    pub tx_fault: Option<bool>,
    /// Expected transmit disable, when set.
    pub tx_disable: Option<bool>,
    /// Expected transmit CDR loss-of-lock, when set.
    pub tx_cdr_lol: Option<bool>,
    /// Expected receive CDR loss-of-lock, when set.
    pub rx_cdr_lol: Option<bool>,
    /// Expected adaptive-equalizer fault, when set.
    pub tx_adaptive_eq_fault: Option<bool>,
    /// Expected receive power in milliwatts, when set.
    pub rx_power_mw: Option<f32>,
    /// Expected transmit power in milliwatts, when set.
    pub tx_power_mw: Option<f32>,
    /// Expected transmit bias in milliamperes, when set.
    pub tx_bias_ma: Option<f32>,
}

/// Compare `actual` lanes to `expected`.
///
/// The sets of indexes must match. A field left `None` on an expectation is
/// not compared. Power and bias use absolute `power_tolerance`; a negative
/// tolerance is an error. NaN never matches.
pub fn assert_lanes(
    actual: &[LaneStatus],
    expected: &[LaneExpectation],
    power_tolerance: f32,
) -> Result<(), TestKitError> {
    if power_tolerance < 0.0 || power_tolerance.is_nan() {
        return Err(TestKitError::MonitorMismatch {
            field: "power_tolerance".to_string(),
            detail: format!("tolerance must be a non-negative finite f32, got {power_tolerance}"),
        });
    }
    let mut actual_indexes: Vec<u8> = actual.iter().map(|lane| lane.index).collect();
    let mut expected_indexes: Vec<u8> = expected.iter().map(|lane| lane.index).collect();
    actual_indexes.sort_unstable();
    expected_indexes.sort_unstable();
    if actual_indexes != expected_indexes {
        return Err(TestKitError::LaneSet {
            detail: format!("actual {actual_indexes:?} != expected {expected_indexes:?}"),
        });
    }
    for expect in expected {
        let lane = actual
            .iter()
            .find(|lane| lane.index == expect.index)
            .expect("index set was checked");
        cmp_flag(lane.index, "rx_los", lane.rx_los, expect.rx_los)?;
        cmp_flag(lane.index, "tx_los", lane.tx_los, expect.tx_los)?;
        cmp_flag(lane.index, "tx_fault", lane.tx_fault, expect.tx_fault)?;
        cmp_flag(lane.index, "tx_disable", lane.tx_disable, expect.tx_disable)?;
        cmp_flag(lane.index, "tx_cdr_lol", lane.tx_cdr_lol, expect.tx_cdr_lol)?;
        cmp_flag(lane.index, "rx_cdr_lol", lane.rx_cdr_lol, expect.rx_cdr_lol)?;
        cmp_flag(
            lane.index,
            "tx_adaptive_eq_fault",
            lane.tx_adaptive_eq_fault,
            expect.tx_adaptive_eq_fault,
        )?;
        cmp_power(
            lane.index,
            "rx_power_mw",
            lane.rx_power_mw,
            expect.rx_power_mw,
            power_tolerance,
        )?;
        cmp_power(
            lane.index,
            "tx_power_mw",
            lane.tx_power_mw,
            expect.tx_power_mw,
            power_tolerance,
        )?;
        cmp_power(
            lane.index,
            "tx_bias_ma",
            lane.tx_bias_ma,
            expect.tx_bias_ma,
            power_tolerance,
        )?;
    }
    Ok(())
}

/// Compare two finite monitors with an absolute tolerance.
///
/// `tolerance` must be finite and `>= 0`. NaN or infinity on either side
/// fails: `inf - inf` is NaN, so equal infinities never compare equal, and
/// an infinite tolerance never vacuously accepts a difference.
pub fn assert_close(
    field: &str,
    actual: f32,
    expected: f32,
    tolerance: f32,
) -> Result<(), TestKitError> {
    if !tolerance.is_finite() || tolerance < 0.0 || !actual.is_finite() || !expected.is_finite() {
        return Err(TestKitError::MonitorMismatch {
            field: field.to_string(),
            detail: format!("actual {actual}, expected {expected}, tolerance {tolerance}"),
        });
    }
    if (actual - expected).abs() > tolerance {
        return Err(TestKitError::MonitorMismatch {
            field: field.to_string(),
            detail: format!("actual {actual} is not within {tolerance} of {expected}"),
        });
    }
    Ok(())
}

fn cmp_flag(
    index: u8,
    field: &'static str,
    actual: Option<bool>,
    expected: Option<bool>,
) -> Result<(), TestKitError> {
    if let Some(expected) = expected {
        if actual != Some(expected) {
            return Err(TestKitError::LaneMismatch {
                index,
                field,
                detail: format!("actual {actual:?}, expected Some({expected})"),
            });
        }
    }
    Ok(())
}

fn cmp_power(
    index: u8,
    field: &'static str,
    actual: Option<f32>,
    expected: Option<f32>,
    tolerance: f32,
) -> Result<(), TestKitError> {
    if let Some(expected) = expected {
        match actual {
            Some(actual)
                if !actual.is_nan()
                    && !expected.is_nan()
                    && tolerance.is_finite()
                    && (actual - expected).abs() <= tolerance =>
            {
                Ok(())
            }
            other => Err(TestKitError::LaneMismatch {
                index,
                field,
                detail: format!("actual {other:?}, expected {expected} ± {tolerance}"),
            }),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_accessors_are_unsupported() {
        let module = MockModule::new();
        assert!(matches!(module.identifier(), Err(ModuleError::Unsupported)));
        assert!(matches!(
            module.temperature_celsius(),
            Err(ModuleError::Unsupported)
        ));
        assert!(matches!(module.lanes(), Err(ModuleError::Unsupported)));
        assert!(matches!(
            module.applications(),
            Err(ModuleError::Unsupported)
        ));
        assert!(matches!(
            module.module_state(),
            Err(ModuleError::Unsupported)
        ));
    }

    #[test]
    fn stored_values_round_trip_without_inventing_readings() {
        let module = MockModule::new()
            .with_identifier(0x11)
            .with_temperature_celsius(None)
            .with_lane_count(1)
            .with_lanes(vec![LaneStatus {
                index: 1,
                rx_los: Some(true),
                ..LaneStatus::default()
            }]);
        assert_eq!(module.identifier().unwrap(), 0x11);
        assert_eq!(module.temperature_celsius().unwrap(), None);
        assert_eq!(module.lane_count().unwrap(), 1);
        let lanes = module.lanes().unwrap();
        assert_lanes(
            &lanes,
            &[LaneExpectation {
                index: 1,
                rx_los: Some(true),
                tx_los: None,
                ..LaneExpectation::default()
            }],
            0.0,
        )
        .unwrap();
    }

    #[test]
    fn lane_assertion_reports_a_mismatch_and_an_extra_lane() {
        let actual = [LaneStatus {
            index: 1,
            rx_los: Some(false),
            ..LaneStatus::default()
        }];
        let err = assert_lanes(
            &actual,
            &[LaneExpectation {
                index: 1,
                rx_los: Some(true),
                ..LaneExpectation::default()
            }],
            0.0,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TestKitError::LaneMismatch {
                field: "rx_los",
                ..
            }
        ));

        let err = assert_lanes(
            &actual,
            &[
                LaneExpectation {
                    index: 1,
                    ..LaneExpectation::default()
                },
                LaneExpectation {
                    index: 2,
                    ..LaneExpectation::default()
                },
            ],
            0.0,
        )
        .unwrap_err();
        assert!(matches!(err, TestKitError::LaneSet { .. }));
    }

    #[test]
    fn page_builder_writes_and_rejects_a_past_the_end_store() {
        let mut page = PageBuilder::new(4);
        page.set_u8(1, 0x0a).unwrap();
        page.set_be_u16(2, 0x0102).unwrap();
        assert_eq!(page.as_bytes(), &[0, 0x0a, 0x01, 0x02]);
        assert!(page.set_u8(4, 1).is_err());
        assert!(page.fill(3, &[1, 2]).is_err());
        assert!(!page.is_empty());
    }

    #[test]
    fn assert_close_rejects_nan_and_accepts_tolerance() {
        assert!(assert_close("temp", 1.0, 1.05, 0.1).is_ok());
        assert!(assert_close("temp", 1.0, 1.2, 0.1).is_err());
        assert!(assert_close("temp", f32::NAN, 1.0, 0.1).is_err());
        assert!(assert_close("temp", 1.0, 1.0, -0.1).is_err());
    }

    #[test]
    fn assert_close_rejects_non_finite() {
        // inf - inf = NaN, so equal infinities must not compare equal.
        assert!(assert_close("temp", f32::INFINITY, f32::INFINITY, 0.0).is_err());
        assert!(assert_close("temp", f32::NEG_INFINITY, 1.0, 0.5).is_err());
        assert!(assert_close("temp", 1.0, f32::INFINITY, 0.5).is_err());
        // An infinite tolerance must not vacuously accept any difference.
        assert!(assert_close("temp", 1.0, 2.0, f32::INFINITY).is_err());
        assert!(assert_close("temp", 1.0, 1.0, f32::NAN).is_err());
        // Control: the finite close case still passes.
        assert!(assert_close("temp", 1.0, 1.0 + 1e-6, 0.01).is_ok());
    }
}
