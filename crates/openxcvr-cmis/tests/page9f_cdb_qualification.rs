//! CMIS 5.3 Page 9Fh metadata is evidence, not command admission.
use openxcvr_cmis::generated::v5_3::CmisPage9FhCdbMessage;
use openxcvr_core::types::management::{ChangeSource, ReadEffect};

#[test]
fn existing_flat_cdb_codec_paths_remain_available() {
    let bytes = [0; 128];
    let root = openxcvr_cmis::generated::CmisPage9FhCdbMessage::decode(&bytes);
    let module =
        openxcvr_cmis::generated::cmis_page9_fh_cdb_message::CmisPage9FhCdbMessage::decode(&bytes);
    let qualified = CmisPage9FhCdbMessage::decode(&bytes);
    assert_eq!(root.cmdid, module.cmdid);
    assert_eq!(root.cmdid, qualified.cmdid);
    assert_eq!(root.lpl, qualified.lpl);
}

#[test]
fn cmis53_cdb_page_reads_cover_all_physical_bits_without_cache_static_claims() {
    let bytes = CmisPage9FhCdbMessage::read_classifications();
    assert_eq!(bytes.len(), 128);
    // Tables 8-178/179/180 cover every physical bit of 128..255. In
    // particular 134..245 is the exact reply footprint of managed 0100h.
    // Qualification of the page does not authorize reading the unused tail.
    for (index, byte) in bytes.iter().enumerate() {
        assert!(byte.is_qualified_none(), "physical byte {}", index + 128);
        assert_eq!(byte.readable_mask, 0xff);
        assert_eq!(byte.unknown_mask, 0);
        assert_eq!(byte.uncovered_mask, 0);
        assert_eq!(byte.write_only_mask, 0);
        assert_eq!(byte.known_effects, vec![ReadEffect::None]);
        assert_eq!(byte.change_source, ChangeSource::Dynamic);
    }
}
