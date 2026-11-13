//! Thin unifying facade over the per-family generated catalog tables:
//! dispatch by family, resolve by name/alias/glob/lane, revision-aware.
pub use openxcvr_core::field_catalog::{
    resolve as resolve_in, AccessKind, CatalogError, CatalogRow, ExternalLinear, ExternalPoly,
    FamilyCatalog, FieldEntry, FieldKind, FieldLoc, FieldQuery, InstanceDimension, LaneSelection,
    RenameRecord, ScaledLsb, Scaling, Unit, VariantDiscriminator,
};

use openxcvr_core::rev::{Family, Rev};

/// The generated catalog table for a family, or `None` for families whose
/// schemas are not part of the three cataloged specs.
pub fn family_catalog(family: Family) -> Option<&'static FamilyCatalog> {
    match family {
        Family::Cmis => Some(&openxcvr_cmis::generated::catalog::CATALOG),
        Family::Sff8636 => Some(&openxcvr_sff8636::generated::catalog::CATALOG),
        Family::Sff8472 => Some(&openxcvr_sff8472::generated::catalog::CATALOG),
        Family::CCmis | Family::Elsfp => None,
    }
}

/// Resolve fields by exact name, schema alias or glob at one revision.
/// The revision's family selects the generated table.
pub fn resolve(
    revision: Rev,
    query: &FieldQuery<'_>,
) -> Result<Vec<FieldEntry<'static>>, CatalogError> {
    let catalog = family_catalog(revision.family)
        .ok_or(CatalogError::UnsupportedRevision { rev: revision })?;
    resolve_in(catalog, revision, query)
}
