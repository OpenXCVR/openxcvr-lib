//! Round-trip tests (encode-round-trip Wave 1): `set_<field>` is the exact
//! inverse of `decode`. Writing a decoded value back into the original bytes
//! must be a no-op (the PLAN.md layer-3 "RW fields only" contract).

use openxcvr_cmis::generated::union::CmisPage00hAdministrative as Admin;
use std::path::PathBuf;

fn fixture(name: &str) -> [u8; 128] {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/cmis")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let mut buf = [0u8; 128];
    buf.copy_from_slice(&bytes[..128]);
    buf
}

#[test]
fn u8_setter_is_decode_inverse() {
    let mut buf = [0u8; 128];
    Admin::set_max_power(&mut buf, 42);
    assert_eq!(Admin::decode(&buf).max_power, 42);
}

#[test]
fn ascii_setter_is_decode_inverse() {
    let mut buf = [0u8; 128];
    let name = *b"ACME OPTICS     ";
    Admin::set_vendor_name(&mut buf, name);
    assert_eq!(Admin::decode(&buf).vendor_name, name);
}

#[test]
fn enum_setter_is_decode_inverse() {
    // set then decode reproduces the same variant.
    let mut buf = [0u8; 128];
    let orig = fixture("fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin");
    let decoded = Admin::decode(&orig);
    Admin::set_vendor_name(&mut buf, decoded.vendor_name);
    assert_eq!(Admin::decode(&buf).vendor_name, decoded.vendor_name);
}

/// The core round-trip property: writing every writable field's *decoded* value
/// back into a copy of the original bytes leaves the buffer byte-identical.
#[test]
fn writing_decoded_values_back_is_identity() {
    let orig = fixture("fboss-cmis-4-0-qsfpdd-400g-dr4.page-00h.bin");
    let d = Admin::decode(&orig);

    let mut copy = orig;
    Admin::set_max_power(&mut copy, d.max_power);
    Admin::set_vendor_name(&mut copy, d.vendor_name);
    Admin::set_vendor_oui(&mut copy, d.vendor_oui);
    Admin::set_vendor_pn(&mut copy, d.vendor_pn);

    assert_eq!(
        copy, orig,
        "writing decoded values back must be a no-op (encode ∘ decode = identity)"
    );
}

/// Read-modify-write: a bit-field setter must not clobber neighbouring bits in
/// the same byte (media_lane_unsupported lanes 1-4 share byte 82, bits 0-3).
#[test]
fn bit_field_setter_preserves_neighbors() {
    let mut buf = [0u8; 128];
    buf[82] = 0xFF;
    Admin::set_media_lane_unsupported_lane_1(&mut buf, false); // clear bit 0
    assert_eq!(buf[82], 0xFE, "only bit 0 cleared");
    Admin::set_media_lane_unsupported_lane_2(&mut buf, false); // clear bit 1
    assert_eq!(buf[82], 0xFC, "only bits 0,1 cleared");

    let d = Admin::decode(&buf);
    assert!(!d.media_lane_unsupported_lane_1);
    assert!(!d.media_lane_unsupported_lane_2);
    assert!(d.media_lane_unsupported_lane_3, "bit 2 untouched");
}
