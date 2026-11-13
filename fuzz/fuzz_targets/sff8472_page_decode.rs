//! cargo-fuzz target: hottest generated decode entry points.
//! The in-tree proptest suites (crates/openxcvr-sff8472/tests/property_generated_surface.rs)
//! prove totality and determinism on bounded random input; this target hands
//! the same decoders arbitrary byte streams for unbounded coverage-fuzzing.
//! Table mechanically derived from src/generated/ (see fuzz/README.md).
#![no_main]

use libfuzzer_sys::fuzz_target;

fn fill<const N: usize>(data: &[u8]) -> [u8; N] {
    std::array::from_fn(|i| data.get(i).copied().unwrap_or(0))
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let sel = (data[0] as usize) % 6;
    let bytes = &data[1..];
    match sel {
        0 => drop(
            <openxcvr_sff8472::generated::union::PageA0hBaseMemory>::decode(&fill::<256>(bytes)),
        ),
        1 => drop(<openxcvr_sff8472::generated::PageA2h00hUserEeprom>::decode(
            &fill::<256>(bytes),
        )),
        2 => drop(<openxcvr_sff8472::generated::PageA2h01hUserEeprom>::decode(
            &fill::<256>(bytes),
        )),
        3 => drop(
            <openxcvr_sff8472::generated::union::PageA2h02hTunabilityRdtRpm>::decode(&fill::<256>(
                bytes,
            )),
        ),
        4 => drop(
            <openxcvr_sff8472::generated::union::PageA2h03hHighAccuracyTiming>::decode(
                &fill::<256>(bytes),
            ),
        ),
        5 => drop(
            <openxcvr_sff8472::generated::union::PageA2hDiagnosticsBase>::decode(&fill::<256>(
                bytes,
            )),
        ),
        _ => unreachable!("selector is bounded"),
    }
});
