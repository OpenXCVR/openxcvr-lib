use openxcvr_core::{
    error::ModuleError,
    transport::Transport,
    types::{ChecksumEntry, ChecksumStatus},
};

pub(super) fn checksums(t: &mut dyn Transport) -> Result<ChecksumStatus, ModuleError> {
    // Read all 128 bytes of Upper Page 00h (flat offsets 128-255).
    let mut page = [0u8; 128];
    for (i, byte) in page.iter_mut().enumerate() {
        let mut b = [0u8; 1];
        t.read(0x50, 0x00, 0, 128 + i as u8, &mut b)?;
        *byte = b[0];
    }

    let cc_base_computed = page[0..63].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    let cc_base_stored = page[63];
    let cc_ext_computed = page[64..95].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    let cc_ext_stored = page[95];

    Ok(ChecksumStatus {
        cc_base: Some(ChecksumEntry {
            stored: cc_base_stored,
            computed: cc_base_computed,
            valid: cc_base_stored == cc_base_computed,
        }),
        cc_ext: Some(ChecksumEntry {
            stored: cc_ext_stored,
            computed: cc_ext_computed,
            valid: cc_ext_stored == cc_ext_computed,
        }),
        cc_dmi: None,
    })
}
