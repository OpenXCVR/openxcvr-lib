//! Pure revision and field-availability report over a schema-derived catalog.
//!
//! [`field_availability`] walks one [`FamilyCatalog`](crate::field_catalog::FamilyCatalog)
//! and classifies every schema field name at one revision. Windows are the
//! catalog's `since`/`until` spans (`since` inclusive, `until` exclusive), the
//! same rule as [`crate::field_catalog::row_in_window`]. A name whose spans
//! miss the revision is [`FieldAvailability::Gated`] and stays in the report —
//! it is not dropped, and it is not the same as a name the schema never
//! declared ([`RevisionReport::field`] returns `None` only for that).
//!
//! The routine is pure: it reads the supplied catalog and revision and returns
//! a decision. It does not open a module, read a bus, or assume a revision the
//! caller did not pass. A revision outside the catalog's supported set is
//! [`RevisionReportError::UnsupportedRevision`].

use std::collections::BTreeMap;
use std::fmt;

use crate::field_catalog::{row_in_window, CatalogRow, FamilyCatalog};
use crate::rev::{Family, Rev};

/// Why a revision/field-availability report was refused.
///
/// Refusal is fail-closed: the report is not filled with guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RevisionReportError {
    /// No revision was supplied by detection. The caller detects one — the
    /// unified module's `detected_rev`, after `detect` or a detect-once open
    /// — and builds the report from that. A latest revision is never assumed
    /// in place of this error.
    #[error(
        "revision was not detected; field availability is not guessed — detect the module revision first"
    )]
    RevisionNotDetected,
    /// The family has no generated schema catalog (C-CMIS and ELSFP today).
    #[error("no schema catalog for family {family:?}; field availability is not guessed")]
    NoCatalog {
        /// The family that was asked for.
        family: Family,
    },
    /// `rev` is not in the catalog's supported-revision set, or it belongs
    /// to a different family than the catalog.
    #[error("revision {rev:?} is outside the schema catalog; field availability is not guessed")]
    UnsupportedRevision {
        /// The revision that was refused.
        rev: Rev,
    },
}

/// One schema `since`/`until` span of a field, with the page address the
/// span occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionWindow {
    /// Schema page id (for example `CmisLowerMemory`).
    pub page_id: &'static str,
    /// Schema page label (for example `lower`, `11h`, `A0h`).
    pub page_label: &'static str,
    /// Seven-bit I2C address of the page window.
    pub address: u8,
    /// Page selector (0 for lower/base pages).
    pub page: u8,
    /// Flat byte offset of the base instance inside the 256-byte window.
    pub offset: u16,
    /// First revision the span applies to, inclusive. `None` means the
    /// earliest family revision the schemas describe.
    pub since: Option<Rev>,
    /// First revision the span does not apply to, exclusive. `None` means
    /// the span has no upper bound.
    pub until: Option<Rev>,
}

impl RevisionWindow {
    /// Whether `rev` falls inside this span (`since` inclusive, `until`
    /// exclusive). Meaningful for a revision of the same family as the span;
    /// catalog resolution refuses a cross-family revision before this is used.
    #[must_use]
    pub fn contains(self, rev: Rev) -> bool {
        self.since.is_none_or(|since| rev >= since) && self.until.is_none_or(|until| rev < until)
    }
}

/// Whether a schema field name applies at the revision the report was built
/// for. The two states are exhaustive: a name the schema never declared is
/// not a third state — [`RevisionReport::field`] returns `None` for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldAvailability {
    /// At least one catalog span of this name covers the revision.
    /// `windows` are only those covering spans.
    Available {
        /// Spans that apply at the report's revision.
        windows: Vec<RevisionWindow>,
    },
    /// The name exists in the catalog, and no span covers the revision.
    /// `windows` are every span, so the caller can see when the field exists.
    /// An empty `windows` list would mean the name was invented; the report
    /// never builds a gated entry that way.
    Gated {
        /// Every schema span of this name. None of them contain the revision.
        windows: Vec<RevisionWindow>,
    },
}

/// One schema field name classified at the report's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldStatus {
    /// Schema-effective field name for the spans grouped here. A rename is a
    /// different name: `mci_max_speed` gated at a revision where
    /// `twi_max_speed_cmis4` is the active name is reported as gated, and the
    /// alias is its own entry.
    pub name: &'static str,
    /// Available or revision-gated at the report's revision.
    pub availability: FieldAvailability,
}

impl FieldStatus {
    /// True when no span of this name covers the revision.
    #[must_use]
    pub fn is_gated(&self) -> bool {
        matches!(self.availability, FieldAvailability::Gated { .. })
    }

    /// True when a span of this name covers the revision.
    #[must_use]
    pub fn is_available(&self) -> bool {
        matches!(self.availability, FieldAvailability::Available { .. })
    }

    /// The spans carried by [`Self::availability`].
    #[must_use]
    pub fn windows(&self) -> &[RevisionWindow] {
        match &self.availability {
            FieldAvailability::Available { windows } | FieldAvailability::Gated { windows } => {
                windows
            }
        }
    }
}

impl fmt::Display for FieldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.availability {
            FieldAvailability::Available { .. } => write!(f, "{} is available", self.name)?,
            FieldAvailability::Gated { .. } => write!(f, "{} is revision-gated", self.name)?,
        }
        write_windows(f, self.windows())
    }
}

/// Field availability at one revision: the revision asked for, and every
/// schema field name classified against it.
///
/// Names are sorted. Each name appears once. Gated names are included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct RevisionReport {
    /// The revision the classification used. Not assumed, and not read from
    /// a module by this type — the caller supplies it.
    pub rev: Rev,
    /// Every schema field name in the catalog, available or revision-gated.
    pub fields: Vec<FieldStatus>,
}

impl RevisionReport {
    /// The classification for `name`, or `None` when the schema catalog has
    /// no span under that name at any revision.
    ///
    /// `None` means "not a schema field". A field that exists only at other
    /// revisions is `Some` and [`FieldStatus::is_gated`], not `None`.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&FieldStatus> {
        self.fields.iter().find(|field| field.name == name)
    }
}

impl fmt::Display for RevisionReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let available = self
            .fields
            .iter()
            .filter(|field| field.is_available())
            .count();
        let gated = self.fields.len() - available;
        write!(
            f,
            "{}: {available} fields available, {gated} revision-gated",
            rev_label(self.rev)
        )
    }
}

/// Classify every field in `catalog` at `rev`.
///
/// Pure and schema-driven: the only inputs are the generated (or otherwise
/// supplied) catalog rows and the revision. A revision the catalog does not
/// list, including a revision of another family, is refused — fields are not
/// reported as gated in its place, and a later revision is not assumed.
pub fn field_availability(
    catalog: &FamilyCatalog,
    rev: Rev,
) -> Result<RevisionReport, RevisionReportError> {
    if rev.family != catalog.family || !catalog.supported_revs.contains(&rev) {
        return Err(RevisionReportError::UnsupportedRevision { rev });
    }
    let mut by_name: BTreeMap<&str, Vec<&CatalogRow>> = BTreeMap::new();
    for row in catalog.rows {
        by_name.entry(row.name).or_default().push(row);
    }
    let mut fields = Vec::with_capacity(by_name.len());
    for (name, rows) in by_name {
        let applicable: Vec<&CatalogRow> = rows
            .iter()
            .copied()
            .filter(|row| row_in_window(row, rev))
            .collect();
        let availability = if applicable.is_empty() {
            FieldAvailability::Gated {
                windows: rows.iter().map(|row| window(row)).collect(),
            }
        } else {
            FieldAvailability::Available {
                windows: applicable.iter().map(|row| window(row)).collect(),
            }
        };
        fields.push(FieldStatus { name, availability });
    }
    Ok(RevisionReport { rev, fields })
}

fn window(row: &CatalogRow) -> RevisionWindow {
    RevisionWindow {
        page_id: row.page_id,
        page_label: row.page_label,
        address: row.address,
        page: row.page,
        offset: row.offset,
        since: row.since,
        until: row.until,
    }
}

fn family_name(family: Family) -> &'static str {
    match family {
        Family::Cmis => "CMIS",
        Family::Sff8472 => "SFF-8472",
        Family::Sff8636 => "SFF-8636",
        Family::CCmis => "C-CMIS",
        Family::Elsfp => "ELSFP",
    }
}

fn rev_label(rev: Rev) -> String {
    format!("{} {}.{}", family_name(rev.family), rev.major, rev.minor)
}

fn write_windows(f: &mut fmt::Formatter<'_>, windows: &[RevisionWindow]) -> fmt::Result {
    for window in windows {
        write!(
            f,
            "; page {} address 0x{:02x} offset {}",
            window.page_label, window.address, window.offset
        )?;
        match (window.since, window.until) {
            (None, None) => {}
            (Some(since), None) => write!(f, " since {}", rev_label(since))?,
            (None, Some(until)) => write!(f, " until {}", rev_label(until))?,
            (Some(since), Some(until)) => {
                write!(f, " since {} until {}", rev_label(since), rev_label(until))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_catalog::{AccessKind, CatalogRow, FieldKind, FieldPresence, Scaling};
    use crate::types::management::ReadEffect;

    const fn row(
        name: &'static str,
        offset: u16,
        since: Option<Rev>,
        until: Option<Rev>,
    ) -> CatalogRow {
        CatalogRow {
            name,
            display_name: name,
            page_id: "SyntheticLower",
            page_label: "lower",
            address: 0x50,
            page: 0,
            banked: false,
            offset,
            len: 1,
            bit_offset: 0,
            bit_width: 8,
            kind: FieldKind::U8,
            enum_ref: None,
            scaling: Scaling::None,
            access: AccessKind::Ro,
            read_effects: &[ReadEffect::None],
            change_source: None,
            instances: None,
            since,
            until,
            renamed_from: None,
            aliased_to: &[],
            tags: &[],
            category: None,
            unit: None,
            discriminator: None,
            presence: FieldPresence::Unconditional,
        }
    }

    static ROWS: &[CatalogRow] = &[
        row("always", 1, None, None),
        row("later", 18, Some(Rev::CMIS_5_0), None),
        row("early_only", 3, None, Some(Rev::CMIS_4_0)),
        row("mci", 2, None, Some(Rev::CMIS_4_0)),
        row("twi", 2, Some(Rev::CMIS_4_0), Some(Rev::CMIS_5_0)),
        row("split", 4, None, Some(Rev::CMIS_4_0)),
        row("split", 4, Some(Rev::CMIS_5_0), None),
    ];

    static CATALOG: FamilyCatalog = FamilyCatalog {
        family: Family::Cmis,
        supported_revs: &[Rev::CMIS_3_0, Rev::CMIS_4_0, Rev::CMIS_5_0],
        revision_probe: None,
        rows: ROWS,
    };

    fn at(rev: Rev) -> RevisionReport {
        field_availability(&CATALOG, rev).expect("synthetic revision is supported")
    }

    #[test]
    fn gated_field_is_listed_not_absent() {
        let report = at(Rev::CMIS_3_0);
        let later = report
            .field("later")
            .expect("a field introduced later must stay in the report");
        assert!(later.is_gated(), "gated field looked available: {later}");
        assert!(!later.is_available());
        assert!(
            later
                .windows()
                .iter()
                .all(|window| !window.contains(report.rev)),
            "a gated window must not contain the revision"
        );
        assert!(
            report.field("not_a_schema_field").is_none(),
            "an unknown name is absent from the schema, which is not the gated state"
        );
        let text = later.to_string();
        assert!(
            text.contains("revision-gated"),
            "gated display must say so: {text}"
        );
        assert!(
            text.contains("page lower") && text.contains("0x50") && text.contains("offset 18"),
            "gated display must name the page address: {text}"
        );
        assert!(
            text.contains("since CMIS 5.0"),
            "gated display must name the window: {text}"
        );
        assert!(
            !text.contains("is available"),
            "gated display must not claim the field is available: {text}"
        );
    }

    #[test]
    fn field_becomes_available_when_its_window_opens() {
        let early = at(Rev::CMIS_3_0);
        let late = at(Rev::CMIS_5_0);
        assert!(early.field("later").unwrap().is_gated());
        let opened = late.field("later").unwrap();
        assert!(opened.is_available());
        assert!(opened
            .windows()
            .iter()
            .any(|window| window.contains(late.rev)));
        assert!(early.field("early_only").unwrap().is_available());
        assert!(
            at(Rev::CMIS_4_0).field("early_only").unwrap().is_gated(),
            "until is exclusive: 4.0 is outside a span that ends at 4.0"
        );
    }

    #[test]
    fn renamed_spans_stay_separate_names() {
        let at_4 = at(Rev::CMIS_4_0);
        let mci = at_4.field("mci").expect("old name stays listed");
        assert!(mci.is_gated(), "the pre-rename name is gated, not deleted");
        assert!(at_4.field("twi").unwrap().is_available());
        let at_3 = at(Rev::CMIS_3_0);
        assert!(at_3.field("mci").unwrap().is_available());
        assert!(at_3.field("twi").unwrap().is_gated());
    }

    #[test]
    fn split_name_with_no_covering_span_is_gated_with_every_window() {
        let report = at(Rev::CMIS_4_0);
        let split = report.field("split").unwrap();
        assert!(split.is_gated());
        assert_eq!(
            split.windows().len(),
            2,
            "both spans are evidence, not a drop"
        );
        assert!(split
            .windows()
            .iter()
            .all(|window| !window.contains(report.rev)));
        let earlier_report = at(Rev::CMIS_3_0);
        let earlier = earlier_report.field("split").unwrap();
        assert!(earlier.is_available());
        assert_eq!(earlier.windows().len(), 1);
    }

    #[test]
    fn unsupported_revision_is_refused_not_reported_as_gated() {
        let err = field_availability(&CATALOG, Rev::CMIS_5_1).unwrap_err();
        assert!(matches!(
            err,
            RevisionReportError::UnsupportedRevision { rev } if rev == Rev::CMIS_5_1
        ));
        let err = field_availability(&CATALOG, Rev::SFF8636_2_12).unwrap_err();
        assert!(matches!(
            err,
            RevisionReportError::UnsupportedRevision { rev } if rev == Rev::SFF8636_2_12
        ));
    }

    #[test]
    fn report_display_names_the_revision_and_counts() {
        let report = at(Rev::CMIS_3_0);
        let text = report.to_string();
        assert!(text.contains("CMIS 3.0"), "{text}");
        assert!(text.contains("revision-gated"), "{text}");
        assert_eq!(report.fields.len(), 6, "six distinct schema names");
    }
}
