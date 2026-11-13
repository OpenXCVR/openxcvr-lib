use openxcvr_core::{
    error::ModuleError,
    transport::Transport,
    types::{ChecksumEntry, ChecksumStatus},
};

pub(super) fn checksums(t: &mut dyn Transport) -> Result<ChecksumStatus, ModuleError> {
    let mut a0 = [0u8; 96];
    t.read(0xA0, 0, 0, 0, &mut a0)?;

    let cc_base_computed = a0[0..63].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    let cc_ext_computed = a0[64..95].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));

    let mut a2 = [0u8; 96];
    t.read(0xA2, 0, 0, 0, &mut a2)?;
    let cc_dmi_computed = a2[0..95].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));

    Ok(ChecksumStatus {
        cc_base: Some(ChecksumEntry {
            stored: a0[63],
            computed: cc_base_computed,
            valid: a0[63] == cc_base_computed,
        }),
        cc_ext: Some(ChecksumEntry {
            stored: a0[95],
            computed: cc_ext_computed,
            valid: a0[95] == cc_ext_computed,
        }),
        cc_dmi: Some(ChecksumEntry {
            stored: a2[95],
            computed: cc_dmi_computed,
            valid: a2[95] == cc_dmi_computed,
        }),
    })
}
