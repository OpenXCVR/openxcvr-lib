//! cargo-fuzz target: hottest generated decode entry points.
//! The in-tree proptest suites (crates/openxcvr-sff8636/tests/property_generated_surface.rs)
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
    let sel = (data[0] as usize) % 7;
    let bytes = &data[1..];
    match sel {
        0 => drop(
            <openxcvr_sff8636::generated::union::Sff8636LowerMemory>::decode(&fill::<128>(bytes)),
        ),
        1 => drop(
            <openxcvr_sff8636::generated::union::Sff8636Page00hUpperAdministrative>::decode(
                &fill::<128>(bytes),
            ),
        ),
        2 => drop(
            <openxcvr_sff8636::generated::Sff8636Page02hUserEeprom>::decode(&fill::<128>(bytes)),
        ),
        3 => drop(
            <openxcvr_sff8636::generated::union::Sff8636Page03hThresholdsAndControls>::decode(
                &fill::<128>(bytes),
            ),
        ),
        4 => drop(
            <openxcvr_sff8636::generated::union::Sff8636Page20hPam4Monitoring1>::decode(
                &fill::<128>(bytes),
            ),
        ),
        5 => drop(
            <openxcvr_sff8636::generated::union::Sff8636Page21hPam4Monitoring2>::decode(
                &fill::<128>(bytes),
            ),
        ),
        6 => drop(
            <openxcvr_sff8636::generated::union::Sff8636Page22hHighAccuracyTiming>::decode(
                &fill::<128>(bytes),
            ),
        ),
        _ => unreachable!("selector is bounded"),
    }
});
