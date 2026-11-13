//! Bounded, passive CMIS 4.0 / 5.3 read planning over generated schema metadata.
//!
//! `CmisReadPlanner` preserves address 0x50 lower memory and bank-0 page 00h.
//! `CmisPage01ReadFootprint` separately qualifies CMIS 5.3 page-01h data and
//! names its prerequisite selection, without performing a selector change.
//! Unknown/missing effects stay refused: RO alone never proves no effect.
use crate::generated::union::{CmisLowerMemory, CmisPage00hAdministrative};
use openxcvr_core::{
    rev::Rev,
    types::management::{
        assemble_snapshot, plan_observation, AssembleInput, AssembleRefusal, InputError,
        PlanObservationInput, PlanRefusal, Range, RangeClassified, ReadFootprint, ReadPlan,
        Snapshot,
    },
};

/// Family binding for the daemon's later ReadPlanningLibrary adapter.
/// No transport, device handle, clock, cache, or actuation is owned here.
#[derive(Debug, Clone)]
pub struct CmisReadPlanner {
    footprint: ReadFootprint,
}

impl CmisReadPlanner {
    /// Preserve the original CMIS 5.3 binding.
    pub fn cmis_5_3() -> Self {
        Self::for_revision(Rev::CMIS_5_3).expect("delivered revision")
    }

    /// Bind the delivered lower-memory + bank-0 page-00h footprint to one
    /// revision. Both tables use that revision's source-qualified metadata;
    /// no 5.3 read-effect authority is substituted for 4.0.
    pub fn for_revision(revision: Rev) -> Result<Self, InputError> {
        let (lower, upper_page00) = match revision {
            Rev::CMIS_4_0 => (
                crate::generated::v4_0::CmisLowerMemory::read_classifications(),
                crate::generated::v4_0::CmisPage00hAdministrative::read_classifications(),
            ),
            Rev::CMIS_5_3 => (
                CmisLowerMemory::read_classifications(),
                CmisPage00hAdministrative::read_classifications(),
            ),
            _ => return Err(InputError::UnsupportedRevision(revision)),
        };
        Ok(Self {
            footprint: ReadFootprint {
                revision,
                address: 0x50,
                lower,
                upper_page00,
            },
        })
    }

    /// Classify without revising the caller's advertised revision or range.
    pub fn classify_range(
        &self,
        revision: Rev,
        range: Range,
    ) -> Result<RangeClassified, InputError> {
        if revision != self.footprint.revision {
            return Err(InputError::UnsupportedRevision(revision));
        }
        self.footprint.classify_range(range)
    }

    /// Plan bounded, passive read-only transfers using the supplied selector
    /// statement and generation/epoch. Nothing effectful is admitted.
    pub fn plan_observation(&self, input: &PlanObservationInput) -> Result<ReadPlan, PlanRefusal> {
        plan_observation(&self.footprint, input)
    }

    /// Assemble only the supplied results of the plan's own transfers.
    pub fn assemble_snapshot(
        &self,
        input: &AssembleInput<'_>,
    ) -> Result<Snapshot, AssembleRefusal> {
        assemble_snapshot(input)
    }
}

/// CMIS 5.3 page-01h data qualification, separate from the existing lower/page-00h
/// `ReadFootprint` contract. Only generated, positively qualified bytes pass.
/// This is not evidence that a module implements paged memory or this page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmisPage01ReadFootprint {
    bytes: [openxcvr_core::types::management::ByteClassification; 128],
}

/// An exact passive data read and its prerequisite selection, not an executable
/// selector transaction. The caller must use the selector core under continuous
/// exclusion and the current generation/epoch, and obtain data permission before
/// issuing `read`. No selector write or consumer admission is delivered here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmisPage01ReadPlan {
    /// Must be confirmed by the selector core before issuing the data read.
    pub required_selection: openxcvr_core::types::management::Selection,
    /// The exact qualified data range, never a selector operation or wider read.
    pub read: openxcvr_core::types::management::ReadStep,
}

impl CmisPage01ReadFootprint {
    /// Bind only the independently source-qualified revision, never a decoder's
    /// broad field applicability or another revision's read-effect metadata.
    pub fn for_revision(revision: Rev) -> Result<Self, InputError> {
        if revision != Rev::CMIS_5_3 {
            return Err(InputError::UnsupportedRevision(revision));
        }
        Ok(Self {
            bytes: crate::generated::v5_3::CmisPage01hAdvertising::read_classifications(),
        })
    }

    /// Classify exact upper-page addressing. Wrong banks/pages/addresses and
    /// ranges including lower memory are uncovered, not normalized or widened.
    pub fn classify_range(&self, range: Range) -> Result<RangeClassified, InputError> {
        use openxcvr_core::types::management::ChangeSource;
        range.validate()?;
        if range.address != 0x50 || range.bank != 0 || range.page != 1 || range.offset < 128 {
            return Ok(RangeClassified::Uncovered);
        }
        let mut change_source = ChangeSource::Static;
        for offset in u32::from(range.offset)..range.end() {
            let classification = &self.bytes[(offset - 128) as usize];
            if let Some(reason) = classification.passive_refusal() {
                return Ok(RangeClassified::NonPassive {
                    byte: offset as u8,
                    reason,
                });
            }
            if classification.change_source == ChangeSource::Dynamic {
                change_source = ChangeSource::Dynamic;
            }
        }
        Ok(RangeClassified::Passive { change_source })
    }

    /// Supply one exact passive read and name the selection the caller must
    /// establish. This does not claim selection is already confirmed. Unlike
    /// `plan_observation`, it does not assemble transfers or carry a stamp;
    /// the acquisition owner must bind execution to its current boundary.
    pub fn plan_range(&self, range: Range) -> Result<CmisPage01ReadPlan, PlanRefusal> {
        use openxcvr_core::types::management::{ReadEffect, ReadRequester, ReadStep, Selection};
        match self.classify_range(range) {
            Ok(RangeClassified::Passive { .. }) => {}
            Ok(RangeClassified::NonPassive { byte, reason }) => {
                return Err(PlanRefusal::NonPassive { byte, reason })
            }
            Ok(RangeClassified::Uncovered) => return Err(PlanRefusal::Uncovered { range }),
            Err(_) => return Err(PlanRefusal::MalformedRange(range)),
        }
        Ok(CmisPage01ReadPlan {
            required_selection: Selection {
                address: range.address,
                bank: range.bank,
                page: range.page,
            },
            read: ReadStep {
                physical_range: range,
                requester: ReadRequester::PassiveInspection,
                classifications: self.bytes
                    [usize::from(range.offset - 128)..(range.end() - 128) as usize]
                    .to_vec(),
                declared_effects: vec![ReadEffect::None],
            },
        })
    }
}
