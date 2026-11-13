use crate::emitters::rust::pages::emit_page_for_revision;
use crate::ir::pages::{PageField, PageSchema};
use crate::ir::version::{applicable_revs, resolve_field_for_rev, Rev};

/// Filter `page.fields` to those present in `rev`, materialize per-rev
/// overrides (`changed_in`, `renamed_from`, `aliased_to`), then defer to
/// the existing scalar emitter. Per-rev emission has zero runtime cost
/// inside generated modules. `rev` is resolved within the family domain the
/// page's `spec.id` names (CMIS when the page carries no spec block, matching
/// the historical union-run behavior) — a rev string from another family
/// never matches, so cross-family windows are empty, not guessed.
pub fn emit_page_per_rev(page: &PageSchema, rev: Rev) -> String {
    let family =
        crate::ir::version::family_revs_for_spec(page.spec.as_ref().map(|s| s.id.as_str()));
    let filtered: Vec<PageField> = page
        .fields
        .iter()
        .filter(|f| applicable_revs(f, family).contains(&rev))
        .map(|f| resolve_field_for_rev(f, rev))
        .collect();
    let mut page_for_rev = page.clone();
    page_for_rev.fields = filtered;
    emit_page_for_revision(&page_for_rev, rev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::pages::{
        AliasedToEntry, ChangedInEntry, FieldType, PageField, PageSchema, RenamedFromEntry,
    };
    use crate::ir::version::Rev;

    #[test]
    fn emits_field_into_v4_0_only_when_since_4_0() {
        let mut field = page_field_default("f_since_4_0", 0);
        field.since = Some("4.0".into());

        let page = PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v2".into(),
            kind: "page".into(),
            id: "TestPage".into(),
            title: "T".into(),
            page_address: "0x00".into(),
            size_bytes: Some(16),
            field_includes: vec![],
            fields: vec![field],
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        };
        let out = emit_page_per_rev(&page, Rev::V3_0);
        assert!(
            !out.contains("f_since_4_0"),
            "field leaked into v3.0: {out}"
        );
        let out = emit_page_per_rev(&page, Rev::V4_0);
        assert!(out.contains("f_since_4_0"), "field missing in v4.0: {out}");
    }

    /// Per-revision pages decode enum fields through the revision-qualified
    /// `Enum::from_rev(<major-revision byte>, ..)`, while the flat union page
    /// (and the `decode_*_supplied` accessors) keep the unqualified
    /// `Enum::from(..)` — a raw reserved at the emitted revision must never
    /// borrow a later revision's named state (B1).
    #[test]
    fn per_revision_pages_decode_enums_through_from_rev() {
        // Offset 128 is page-relative 0 for an upper 128-byte page.
        let mut field = page_field_default("dp_state", 128);
        field.enum_ref = Some("CmisDataPathState".into());
        let page = make_page(vec![field]);

        for rev in Rev::CMIS_ALL {
            let out = emit_page_per_rev(&page, *rev);
            assert!(
                out.contains(&format!(
                    "CmisDataPathState::from_rev({:#04x}, buf[0] as u8)",
                    rev.major_byte_cmis()
                )),
                "per-revision page must decode through from_rev at {rev:?}:\n{out}"
            );
        }

        let flat = crate::emitters::rust::pages::emit_page(&page);
        assert!(
            flat.contains("CmisDataPathState::from(buf[0] as u8)"),
            "flat union page keeps the unqualified From decode:\n{flat}"
        );
        assert!(
            !flat.contains("from_rev"),
            "flat union page must not decode through from_rev:\n{flat}"
        );
    }

    /// Local test helper. Returns a `PageField` with every optional
    /// member set to its default; callers mutate the specific fields
    /// they need (`field.since = Some(...)`). Avoids both struct-update
    /// syntax (`..base`) and field duplication between literal and base.
    fn page_field_default(name: &str, off: u32) -> PageField {
        PageField {
            name: name.into(),
            display_name: None,
            offset: Some(off),
            offset_expr: None,
            bit_offset: None,
            bit_offset_expr: None,
            bit_width: None,
            size_bytes: None,
            ty: FieldType::U8,
            enum_ref: None,
            composite_ref: None,
            encoding: None,
            unit: None,
            access: None,
            metadata: Default::default(),
            setter_eligible: None,
            explicit_access: None,
            field_category: None,
            tags: vec![],
            flags: None,
            description: None,
            instances: None,
            variants: None,
            present_when: None,
            content_switch: None,
            enum_inline: None,
            provenance: None,
            scope: None,
            behavior: None,
            constraints: None,
            default_value: None,
            template: None,
            supplement: None,
            since: None,
            until: None,
            changed_in: None,
            renamed_from: None,
            aliased_to: None,
        }
    }

    fn make_page(fields: Vec<PageField>) -> PageSchema {
        PageSchema {
            spec: None,
            provenance: None,
            schema: "openxcvr.page.v2".into(),
            kind: "page".into(),
            id: "TestPage".into(),
            title: "T".into(),
            page_address: "0x00".into(),
            size_bytes: Some(16),
            field_includes: vec![],
            fields,
            present_when: None,
            banking: None,
            page_select: None,
            page_select_field: None,
        }
    }

    /// changed_in type override: field starts as u8; at v4.0+ becomes u16.
    #[test]
    fn changed_in_type_override() {
        let mut field = page_field_default("field_width", 0);
        field.changed_in = Some(vec![ChangedInEntry {
            rev: "4.0".into(),
            ty: Some(FieldType::U16Be),
            enum_ref: None,
            note: None,
            ..Default::default()
        }]);

        let page = make_page(vec![field]);

        let out_v3 = emit_page_per_rev(&page, Rev::V3_0);
        // v3.0: u8 (base type) — check the struct field declaration specifically.
        assert!(
            out_v3.contains("field_width: u8"),
            "v3.0 should emit u8 but got: {out_v3}"
        );
        assert!(
            !out_v3.contains("field_width: u16"),
            "v3.0 must not declare field_width as u16: {out_v3}"
        );

        let out_v4 = emit_page_per_rev(&page, Rev::V4_0);
        // v4.0: u16 (changed_in override) — check the struct field declaration specifically.
        assert!(
            out_v4.contains("field_width: u16"),
            "v4.0 should emit u16 but got: {out_v4}"
        );
        assert!(
            !out_v4.contains("field_width: u8"),
            "v4.0 must not declare field_width as u8: {out_v4}"
        );
    }

    /// changed_in enum_ref override: field starts with CmisOldEnum; at v5.0+ becomes CmisNewEnum.
    #[test]
    fn changed_in_enum_ref_override() {
        let mut field = page_field_default("field_enum", 0);
        field.enum_ref = Some("CmisOldEnum".into());
        field.changed_in = Some(vec![ChangedInEntry {
            rev: "5.0".into(),
            ty: None,
            enum_ref: Some("CmisNewEnum".into()),
            note: None,
            ..Default::default()
        }]);

        let page = make_page(vec![field]);

        let out_v4 = emit_page_per_rev(&page, Rev::V4_0);
        assert!(
            out_v4.contains("CmisOldEnum"),
            "v4.0 should reference CmisOldEnum but got: {out_v4}"
        );
        assert!(
            !out_v4.contains("CmisNewEnum"),
            "v4.0 must not reference CmisNewEnum: {out_v4}"
        );

        let out_v5 = emit_page_per_rev(&page, Rev::V5_0);
        assert!(
            out_v5.contains("CmisNewEnum"),
            "v5.0 should reference CmisNewEnum but got: {out_v5}"
        );
        assert!(
            !out_v5.contains("CmisOldEnum"),
            "v5.0 must not reference CmisOldEnum: {out_v5}"
        );
    }

    /// renamed_from: field is "new_name" since v5.0; before that it was "old_name".
    #[test]
    fn renamed_from_applies_old_name_before_rename_rev() {
        let mut field = page_field_default("new_name", 0);
        field.renamed_from = Some(RenamedFromEntry {
            rev: "5.0".into(),
            old_name: "old_name".into(),
        });

        let page = make_page(vec![field]);

        let out_v4 = emit_page_per_rev(&page, Rev::V4_0);
        assert!(
            out_v4.contains("old_name"),
            "v4.0 should use old_name but got: {out_v4}"
        );
        assert!(
            !out_v4.contains("new_name"),
            "v4.0 must not use new_name: {out_v4}"
        );

        let out_v5 = emit_page_per_rev(&page, Rev::V5_0);
        assert!(
            out_v5.contains("new_name"),
            "v5.0 should use new_name but got: {out_v5}"
        );
        assert!(
            !out_v5.contains("old_name"),
            "v5.0 must not use old_name: {out_v5}"
        );
    }

    /// aliased_to: at v5.1+ the field gets alias_name in addition to current_name.
    #[test]
    fn aliased_to_applies_at_alias_rev() {
        let mut field = page_field_default("current_name", 0);
        field.aliased_to = Some(vec![AliasedToEntry {
            rev: "5.1".into(),
            new_name: "alias_name".into(),
        }]);

        let page = make_page(vec![field]);

        let out_v5_0 = emit_page_per_rev(&page, Rev::V5_0);
        assert!(
            out_v5_0.contains("current_name"),
            "v5.0 should use current_name but got: {out_v5_0}"
        );
        assert!(
            !out_v5_0.contains("alias_name"),
            "v5.0 must not use alias_name: {out_v5_0}"
        );

        let out_v5_1 = emit_page_per_rev(&page, Rev::V5_1);
        assert!(
            out_v5_1.contains("alias_name"),
            "v5.1 should use alias_name but got: {out_v5_1}"
        );
        assert!(
            !out_v5_1.contains("current_name"),
            "v5.1 must not use current_name: {out_v5_1}"
        );
    }
}
