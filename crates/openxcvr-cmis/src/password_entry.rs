//! Standalone CMIS **4.0** entry: one write, transport evidence only.
//!
//! No I/O, permit, retry, selector, probe, readback, checksum or unlock state
//! is owned here. Supplied facts must come from the exclusive operation owner.
use crate::vendor_programming::{
    encode_password_entry, password_entry_invalidations, password_entry_range,
};
use openxcvr_core::{
    rev::Family,
    types::management::{operations::RetryDecision, ActuationOutcome, Invalidation, Range},
    Rev,
};
use std::fmt;

/// Closed, secret-free preflight refusal. Width is intentionally not retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordEntryRefusal {
    /// The family has no password-entry function.
    UnsupportedFamily(Family),
    /// The raw revision byte does not carry the password function.
    UnsupportedRevision(u8),
    /// The revision-typed entry point's refusal of a non-4.0 CMIS revision.
    UnsupportedRevisionOf(Rev),
    /// No secret was supplied.
    MissingSecret,
    /// The supplied secret is not exactly 4 bytes.
    InvalidSecretWidth,
}

/// An original operation-scoped input, not a cache or reusable unlock token.
/// No Clone, Copy or serialization. The buffer exists only to call the exact
/// generated 4.0 setter, whose mutator contract is the lower-memory page's
/// fixed-size `&mut [u8; 128]`; only its entry bytes can be borrowed for
/// submission. The caller must separately wipe its original input and any
/// transport copies.
pub struct PasswordEntrySecret {
    buffer: [u8; 128],
}

impl PasswordEntrySecret {
    /// Big-endian bytes for the one changing transfer. Do not log or retain.
    pub fn as_bytes(&self) -> &[u8] {
        let range = password_entry_range();
        &self.buffer[usize::from(range.offset)..range.end() as usize]
    }
}

impl fmt::Debug for PasswordEntrySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PasswordEntrySecret([REDACTED])")
    }
}
impl fmt::Display for PasswordEntrySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl PasswordEntrySecret {
    fn wipe(&mut self) {
        for byte in &mut self.buffer {
            // SAFETY: byte is a live, uniquely borrowed, aligned u8. Volatile
            // stores cannot be eliminated as dead stores by the optimizer.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}
impl Drop for PasswordEntrySecret {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// Immutable, secret-free description. Its only transfer is `write()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordEntryPlan {
    invalidations: [Invalidation; 2],
}
impl PasswordEntryPlan {
    /// The revision this plan is valid at (always CMIS 4.0).
    pub fn revision(&self) -> Rev {
        Rev::CMIS_4_0
    }
    /// The password-entry byte range (page 9Fh 128..=131).
    pub fn write(&self) -> Range {
        password_entry_range()
    }
    /// The retry budget: always 0 (uncertainty is never replayed).
    pub fn max_retries(&self) -> u8 {
        0
    }
    /// Reuse the embedded entry's terminal uncertainty decision; no retry.
    pub fn unconfirmed_retry(&self) -> RetryDecision {
        RetryDecision::Terminal(ActuationOutcome::Uncertain {
            reason: "step-unconfirmed".into(),
        })
    }
    /// Whether the password is read back: never (transport-confirmed only).
    pub fn readback(&self) -> bool {
        false
    }
    /// Whether the plan probes for unlock state: never.
    pub fn probe(&self) -> bool {
        false
    }
    /// Whether the plan writes a selector: never.
    pub fn selector_change(&self) -> bool {
        false
    }
    /// Whether the plan maintains a checksum: never.
    pub fn checksum(&self) -> bool {
        false
    }
    /// Apply after any submitted attempt, including completed or uncertain.
    pub fn invalidations(&self) -> &[Invalidation] {
        &self.invalidations
    }
}

/// Non-cloneable, transient operation ownership; planning is not authorization.
#[derive(Debug)]
pub struct PasswordEntryOperation {
    plan: PasswordEntryPlan,
    secret: PasswordEntrySecret,
}
impl PasswordEntryOperation {
    /// The secret-free plan.
    pub fn plan(&self) -> &PasswordEntryPlan {
        &self.plan
    }
    /// The operation-scoped secret (submission borrow only).
    pub fn secret(&self) -> &PasswordEntrySecret {
        &self.secret
    }
}

/// Admit only the reported family and raw revision byte CMIS/0x40. Do not
/// choose a revision for the device. No partial plan escapes a refusal.
/// Input bytes are the four-byte big-endian representation, not native endian.
pub fn plan_password_entry(
    family: Family,
    reported_revision: u8,
    secret: Option<&[u8]>,
) -> Result<PasswordEntryOperation, PasswordEntryRefusal> {
    if family != Family::Cmis {
        return Err(PasswordEntryRefusal::UnsupportedFamily(family));
    }
    if reported_revision != 0x40 {
        return Err(PasswordEntryRefusal::UnsupportedRevision(reported_revision));
    }
    plan_cmis40(secret)
}

/// The same admission for an owner that holds the detected revision as a
/// library [`Rev`], as the sibling programming and read planners take it.
/// Only `Rev::CMIS_4_0` is admitted; the plan is identical to the byte form's.
pub fn plan_password_entry_for_revision(
    revision: Rev,
    secret: Option<&[u8]>,
) -> Result<PasswordEntryOperation, PasswordEntryRefusal> {
    if revision.family != Family::Cmis {
        return Err(PasswordEntryRefusal::UnsupportedFamily(revision.family));
    }
    if revision != Rev::CMIS_4_0 {
        return Err(PasswordEntryRefusal::UnsupportedRevisionOf(revision));
    }
    plan_cmis40(secret)
}

fn plan_cmis40(secret: Option<&[u8]>) -> Result<PasswordEntryOperation, PasswordEntryRefusal> {
    let bytes: &[u8; 4] = secret
        .ok_or(PasswordEntryRefusal::MissingSecret)?
        .try_into()
        .map_err(|_| PasswordEntryRefusal::InvalidSecretWidth)?;
    let mut secret = PasswordEntrySecret { buffer: [0; 128] };
    encode_password_entry(&mut secret.buffer, u32::from_be_bytes(*bytes));
    Ok(PasswordEntryOperation {
        plan: PasswordEntryPlan {
            invalidations: password_entry_invalidations(),
        },
        secret,
    })
}

/// Supplied owner facts, NOT facts discovered by this library. A transport
/// terminal fact is valid only for the exact planned transfer and correlation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordEntryFact {
    /// The exact planned transfer was admitted by the owner.
    Admitted,
    /// The exact planned transfer was submitted.
    Submitted,
    /// The exact transfer reached its backend terminal condition.
    TransportCompleted,
    /// The owner positively knows application was possible.
    ApplicationPossible,
    /// The owner refused the exact planned transfer.
    Refused,
    /// The transfer failed before it was submitted.
    FailedBeforeSubmission,
}

/// This closed vocabulary has no stored-byte or resulting-state verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordEntryEvidence {
    /// Admitted/submitted, not applied.
    Accepted,
    /// Exact transfer reached backend transport terminal condition only.
    Completed,
    /// Possible application cannot be excluded; never retry.
    Uncertain,
    /// The owner refused the exact planned transfer.
    Refused,
    /// The transfer failed before it was submitted.
    FailedBeforeSubmission,
}

/// Secret-free projection, deliberately not convertible to ActuationOutcome
/// (whose broader verified variants are not evidence this operation can yield).
///
/// ```compile_fail
/// use openxcvr_cmis::password_entry::PasswordEntryEvidence;
/// let _ = PasswordEntryEvidence::StoredBytesVerified;
/// ```
/// ```compile_fail
/// use openxcvr_cmis::password_entry::PasswordEntryEvidence;
/// let _ = PasswordEntryEvidence::ResultingStateVerified;
/// ```
/// ```compile_fail
/// use openxcvr_cmis::password_entry::PasswordEntryEvidence;
/// let _ = PasswordEntryEvidence::Unlocked;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordEntryOutcome {
    evidence: PasswordEntryEvidence,
    possible_application: bool,
    verification_not_performed: bool,
    invalidations: Vec<Invalidation>,
}
impl From<PasswordEntryFact> for PasswordEntryOutcome {
    fn from(fact: PasswordEntryFact) -> Self {
        use PasswordEntryFact as Fact;
        let (evidence, possible_application) = match fact {
            Fact::Admitted => (PasswordEntryEvidence::Accepted, false),
            Fact::Submitted => (PasswordEntryEvidence::Accepted, true),
            Fact::TransportCompleted => (PasswordEntryEvidence::Completed, true),
            Fact::ApplicationPossible => (PasswordEntryEvidence::Uncertain, true),
            Fact::Refused => (PasswordEntryEvidence::Refused, false),
            Fact::FailedBeforeSubmission => (PasswordEntryEvidence::FailedBeforeSubmission, false),
        };
        Self {
            evidence,
            possible_application,
            verification_not_performed: true,
            invalidations: if possible_application {
                password_entry_invalidations().to_vec()
            } else {
                vec![]
            },
        }
    }
}
impl PasswordEntryOutcome {
    /// The outcome's evidence class.
    pub fn evidence(&self) -> PasswordEntryEvidence {
        self.evidence
    }
    /// Always true: this vocabulary never verifies stored bytes or state.
    pub fn verification_not_performed(&self) -> bool {
        self.verification_not_performed
    }
    /// Whether application cannot be excluded (uncertainty).
    pub fn possible_application(&self) -> bool {
        self.possible_application
    }
    /// The invalidations to apply (password pin plus covered bytes).
    pub fn invalidations(&self) -> &[Invalidation] {
        &self.invalidations
    }
}

macro_rules! display_debug {
    ($($ty:ty),+ $(,)?) => {$(impl fmt::Display for $ty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Debug::fmt(self, f)
        }
    })+};
}
display_debug!(
    PasswordEntryPlan,
    PasswordEntryOperation,
    PasswordEntryRefusal
);
impl fmt::Display for PasswordEntryOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.evidence {
            PasswordEntryEvidence::Accepted => "accepted",
            PasswordEntryEvidence::Completed => "completed",
            PasswordEntryEvidence::Uncertain => "uncertain",
            PasswordEntryEvidence::Refused => "refused",
            PasswordEntryEvidence::FailedBeforeSubmission => "failed-before-submission",
        };
        write!(f, "{name}; verification_not_performed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wipe_clears_the_entire_owned_buffer_in_place() {
        // Drop delegates to wipe(); test wipe() on a live value so nothing is
        // read after its destructor has run.
        let mut secret = PasswordEntrySecret {
            buffer: [0x71; 128],
        };
        secret.wipe();
        assert_eq!(secret.buffer, [0; 128]);
    }
}
