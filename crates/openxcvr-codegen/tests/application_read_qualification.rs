//! Every constituent carries its own revision's read-effect authority.
use openxcvr_codegen::{
    ir::{
        pages::ReadEffectMetadata,
        version::{resolve_field_for_rev, Rev},
    },
    parser::pages::load_page_schema,
};

#[test]
fn lower_application_constituents_have_revision_local_authority_only() {
    let schema = load_page_schema(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/cmis/lower-memory.yaml"),
    )
    .unwrap();
    for (rev, source) in [
        (Rev::V4_0, "CMIS 4.0 (CMIS4p0_Third_Party_Spec)"),
        (Rev::V5_0, "CMIS 5.0 (CMIS5p0_Third_Party_Spec)"),
        (Rev::V5_1, "CMIS 5.1 (CMIS5p1_Third_Party_Spec)"),
        (Rev::V5_2, "OIF-CMIS-05.2"),
        (Rev::V5_3, "OIF-CMIS-05.3"),
    ] {
        let mut coverage = [0u8; 33];
        let mut constituents = 0;
        for field in &schema.fields {
            let resolved = resolve_field_for_rev(field, rev);
            let bits = resolved.bit_coverage().unwrap();
            if !bits.iter().any(|(b, _)| (85..=117).contains(b)) {
                continue;
            }
            assert!(bits.iter().all(|(b, _)| (85..=117).contains(b)));
            assert_eq!(
                resolved.metadata.read_effect,
                Some(ReadEffectMetadata::None),
                "{} {rev:?}",
                field.name
            );
            let provenance = resolved.metadata.read_effect_provenance.as_ref().unwrap();
            assert_eq!(
                provenance["source"].as_str(),
                Some(source),
                "{} {rev:?}",
                field.name
            );
            for key in ["section", "table", "row", "basis"] {
                assert!(!provenance[key].as_str().unwrap().is_empty());
            }
            assert!(!provenance["pdf_page"].as_sequence().unwrap().is_empty());
            constituents += bits.len();
            for (byte, mask) in bits {
                coverage[(byte - 85) as usize] |= mask;
            }
            let v3 = resolve_field_for_rev(field, Rev::V3_0);
            assert_ne!(v3.metadata.read_effect, Some(ReadEffectMetadata::None));
        }
        assert_eq!(constituents, 41); // MediaType plus 8 * 5 descriptor constituents.
        assert_eq!(coverage, [0xff; 33]);
    }
}
