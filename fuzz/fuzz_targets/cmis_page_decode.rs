//! cargo-fuzz target: hottest generated decode entry points.
//! The in-tree proptest suites (crates/openxcvr-cmis/tests/property_generated_surface.rs)
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
    let sel = (data[0] as usize) % 52;
    let bytes = &data[1..];
    match sel {
        0 => drop(<openxcvr_cmis::generated::union::CmisLowerMemory>::decode(
            &fill::<128>(bytes),
        )),
        1 => drop(
            <openxcvr_cmis::generated::union::CmisPage00hAdministrative>::decode(&fill::<128>(
                bytes,
            )),
        ),
        2 => drop(
            <openxcvr_cmis::generated::union::CmisPage01hAdvertising>::decode(&fill::<128>(bytes)),
        ),
        3 => drop(<openxcvr_cmis::generated::CmisPage02hThresholds>::decode(
            &fill::<128>(bytes),
        )),
        4 => drop(<openxcvr_cmis::generated::CmisPage03hUserEeprom>::decode(
            &fill::<128>(bytes),
        )),
        5 => drop(
            <openxcvr_cmis::generated::union::CmisPage04hLaserCapabilities>::decode(&fill::<128>(
                bytes,
            )),
        ),
        6 => drop(
            <openxcvr_cmis::generated::CmisPage05hFormFactorSignals>::decode(&fill::<128>(bytes)),
        ),
        7 => drop(
            <openxcvr_cmis::generated::union::CmisPage0ChModuleManagement>::decode(&fill::<128>(
                bytes,
            )),
        ),
        8 => drop(
            <openxcvr_cmis::generated::union::CmisPage0DhFirmwareManagement>::decode(&fill::<128>(
                bytes,
            )),
        ),
        9 => drop(<openxcvr_cmis::generated::CmisPage10hLaneControl>::decode(
            &fill::<128>(bytes),
        )),
        10 => drop(
            <openxcvr_cmis::generated::union::CmisPage11hLaneStatus>::decode(&fill::<128>(bytes)),
        ),
        11 => drop(
            <openxcvr_cmis::generated::union::CmisPage12hTunableLaser>::decode(&fill::<128>(bytes)),
        ),
        12 => drop(
            <openxcvr_cmis::generated::CmisPage13hDiagnosticsControl>::decode(&fill::<128>(bytes)),
        ),
        13 => drop(
            <openxcvr_cmis::generated::union::CmisPage14hDiagnosticsResults>::decode(&fill::<128>(
                bytes,
            )),
        ),
        14 => drop(<openxcvr_cmis::generated::CmisPage15hTiming>::decode(
            &fill::<128>(bytes),
        )),
        15 => drop(
            <openxcvr_cmis::generated::union::CmisPage16hNetworkPath>::decode(&fill::<128>(bytes)),
        ),
        16 => drop(
            <openxcvr_cmis::generated::union::CmisPage18hControlSetExtension>::decode(
                &fill::<128>(bytes),
            ),
        ),
        17 => drop(
            <openxcvr_cmis::generated::union::CmisPage19hStatusExtension>::decode(&fill::<128>(
                bytes,
            )),
        ),
        18 => drop(
            <openxcvr_cmis::generated::union::CmisPage1ChNormalizedApplicationDescriptors>::decode(
                &fill::<128>(bytes),
            ),
        ),
        19 => drop(
            <openxcvr_cmis::generated::union::CmisPage1DhHostLaneSwitching>::decode(&fill::<128>(
                bytes,
            )),
        ),
        20 => drop(
            <openxcvr_cmis::generated::union::CmisPage20hVdmDescriptors1To64>::decode(
                &fill::<128>(bytes),
            ),
        ),
        21 => drop(
            <openxcvr_cmis::generated::union::CmisPage21hVdmDescriptors65To128>::decode(
                &fill::<128>(bytes),
            ),
        ),
        22 => {
            drop(
                <openxcvr_cmis::generated::union::CmisPage22hVdmDescriptors129To192>::decode(
                    &fill::<128>(bytes),
                ),
            )
        }
        23 => {
            drop(
                <openxcvr_cmis::generated::union::CmisPage23hVdmDescriptors193To256>::decode(
                    &fill::<128>(bytes),
                ),
            )
        }
        24 => drop(
            <openxcvr_cmis::generated::union::CmisPage28hVdmThresholds1To16>::decode(&fill::<128>(
                bytes,
            )),
        ),
        25 => drop(
            <openxcvr_cmis::generated::union::CmisPage29hVdmThresholds17To32>::decode(
                &fill::<128>(bytes),
            ),
        ),
        26 => drop(
            <openxcvr_cmis::generated::union::CmisPage2AhVdmThresholds33To48>::decode(
                &fill::<128>(bytes),
            ),
        ),
        27 => drop(
            <openxcvr_cmis::generated::union::CmisPage2BhVdmThresholds49To64>::decode(
                &fill::<128>(bytes),
            ),
        ),
        28 => drop(
            <openxcvr_cmis::generated::union::CmisPage2ChVdmFlags>::decode(&fill::<128>(bytes)),
        ),
        29 => drop(
            <openxcvr_cmis::generated::union::CmisPage2DhVdmMasks>::decode(&fill::<128>(bytes)),
        ),
        30 => drop(
            <openxcvr_cmis::generated::union::CmisPage2FhVdmAdvertisement>::decode(&fill::<128>(
                bytes,
            )),
        ),
        31 => drop(
            <openxcvr_cmis::generated::union::CmisPage60hLaneDatapathManagementExtensions>::decode(
                &fill::<128>(bytes),
            ),
        ),
        32 => {
            drop(
                <openxcvr_cmis::generated::union::CmisPage61hLaneDatapathMonitoring>::decode(
                    &fill::<128>(bytes),
                ),
            )
        }
        33 => drop(
            <openxcvr_cmis::generated::union::CmisPage62hLaneSupervisionThresholds>::decode(
                &fill::<128>(bytes),
            ),
        ),
        34 => drop(
            <openxcvr_cmis::generated::union::CmisPage6DhMediaLaneSwitching>::decode(&fill::<128>(
                bytes,
            )),
        ),
        35 => drop(
            <openxcvr_cmis::generated::union::CmisPage9FhCdbMessage>::decode(&fill::<128>(bytes)),
        ),
        36 => {
            drop(<openxcvr_cmis::generated::CmisPageA0hCdbEplSegment1>::decode(&fill::<128>(bytes)))
        }
        37 => {
            drop(<openxcvr_cmis::generated::CmisPageA1hCdbEplSegment2>::decode(&fill::<128>(bytes)))
        }
        38 => {
            drop(<openxcvr_cmis::generated::CmisPageA2hCdbEplSegment3>::decode(&fill::<128>(bytes)))
        }
        39 => {
            drop(<openxcvr_cmis::generated::CmisPageA3hCdbEplSegment4>::decode(&fill::<128>(bytes)))
        }
        40 => {
            drop(<openxcvr_cmis::generated::CmisPageA4hCdbEplSegment5>::decode(&fill::<128>(bytes)))
        }
        41 => {
            drop(<openxcvr_cmis::generated::CmisPageA5hCdbEplSegment6>::decode(&fill::<128>(bytes)))
        }
        42 => {
            drop(<openxcvr_cmis::generated::CmisPageA6hCdbEplSegment7>::decode(&fill::<128>(bytes)))
        }
        43 => {
            drop(<openxcvr_cmis::generated::CmisPageA7hCdbEplSegment8>::decode(&fill::<128>(bytes)))
        }
        44 => {
            drop(<openxcvr_cmis::generated::CmisPageA8hCdbEplSegment9>::decode(&fill::<128>(bytes)))
        }
        45 => drop(
            <openxcvr_cmis::generated::CmisPageA9hCdbEplSegment10>::decode(&fill::<128>(bytes)),
        ),
        46 => drop(
            <openxcvr_cmis::generated::CmisPageAAhCdbEplSegment11>::decode(&fill::<128>(bytes)),
        ),
        47 => drop(
            <openxcvr_cmis::generated::CmisPageABhCdbEplSegment12>::decode(&fill::<128>(bytes)),
        ),
        48 => drop(
            <openxcvr_cmis::generated::CmisPageAChCdbEplSegment13>::decode(&fill::<128>(bytes)),
        ),
        49 => drop(
            <openxcvr_cmis::generated::CmisPageADhCdbEplSegment14>::decode(&fill::<128>(bytes)),
        ),
        50 => drop(
            <openxcvr_cmis::generated::CmisPageAEhCdbEplSegment15>::decode(&fill::<128>(bytes)),
        ),
        51 => drop(
            <openxcvr_cmis::generated::CmisPageAFhCdbEplSegment16>::decode(&fill::<128>(bytes)),
        ),
        _ => unreachable!("selector is bounded"),
    }
});
