//! Rust half of the CTest cross-language equivalence test. Synthetic fixtures
//! from CMIS 4.0 Tables 8-12/13/39 and 5.3 Tables 8-20/22/23/59.
use openxcvr::{
    core::Rev,
    rich::{
        application_records, application_records_footprint, application_records_from_ranges,
        ApplicationRange, InterfaceMeaning, LaneCount,
    },
};

fn meaning_state(m: &InterfaceMeaning) -> u32 {
    match m {
        InterfaceMeaning::Known(_) => 0,
        InterfaceMeaning::Undefined => 1,
        InterfaceMeaning::UnknownCode(_) => 2,
        InterfaceMeaning::UnsupportedMediaType(_) => 3,
        InterfaceMeaning::Unavailable => 4,
    }
}

fn lane_state(m: Option<LaneCount>) -> u32 {
    match m {
        Some(LaneCount::DefinedByInterface) => 0,
        Some(LaneCount::Explicit(_)) => 1,
        Some(LaneCount::Reserved(_)) => 2,
        None => 3,
    }
}

fn main() {
    let sparse = std::env::args().nth(4).as_deref() == Some("sparse");
    let args: Vec<u8> = std::env::args()
        .skip(1)
        .take(3)
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(args.len(), 3);
    let (code, media, scenario) = (args[0], args[1], args[2]);
    let revision = Rev {
        major: code >> 4,
        minor: code & 15,
        ..Rev::CMIS_4_0
    };
    let mut lower = [0; 128];
    let mut upper = [0; 128];
    lower[1] = code;
    lower[85] = media;
    for i in 0..8 {
        lower[86 + i * 4..90 + i * 4].copy_from_slice(&[0x1a, 1, 0x44, 0x11]);
    }
    for i in 0..7 {
        upper[95 + i * 4..99 + i * 4].copy_from_slice(&[0x1a, 1, 0x22, 0x55]);
    }
    match scenario {
        1 => lower[114] = 255,
        2 => upper[95] = 255,
        5 => {
            lower[86] = 254;
            lower[87] = 254;
        }
        6 => {
            lower[86] = 0;
            lower[87] = 0;
        }
        7 => {
            lower[88] = 9;
            lower[89] = 255;
        }
        8 => lower[1] = 0,
        11 => upper[119] = 255,
        _ => (),
    }
    let lo = match scenario {
        9 => None,
        10 => Some(&lower[..117]),
        _ => Some(&lower[..]),
    };
    let hi = match scenario {
        3 => None,
        4 => Some(&upper[..122]),
        _ => Some(&upper[..]),
    };
    let records = if sparse {
        let mut supplied = Vec::new();
        if let Ok(fp) = application_records_footprint(revision) {
            for mut range in fp.ranges {
                let (bytes, base) = if range.page == 0 { (lo, 0) } else { (hi, 128) };
                if let Some(bytes) = bytes {
                    let start = usize::from(range.offset) - base;
                    let end = (range.end() as usize - base).min(bytes.len());
                    if start < end {
                        range.len = (end - start) as u16;
                        supplied.push(ApplicationRange {
                            range,
                            bytes: &bytes[start..end],
                        });
                    }
                }
            }
        }
        let records = application_records_from_ranges(revision, &supplied).unwrap();
        // Same existing fixtures: complete pages (or missing pages) must retain
        // full-page behavior; only deliberately short pages have per-slot gates.
        if lo.is_none_or(|p| p.len() >= 128) && hi.is_none_or(|p| p.len() >= 128) {
            assert_eq!(records, application_records(revision, lo, hi));
        }
        records
    } else {
        application_records(revision, lo, hi)
    };
    for r in records.slots {
        let a = r.application.as_ref();
        println!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            r.slot,
            r.state as u32,
            u8::from(a.is_some()),
            a.map_or(0, |a| a.host_interface_code),
            a.map_or(0, |a| a.media_interface_code),
            a.map_or(0, |a| a.host_lane_count),
            a.map_or(0, |a| a.media_lane_count),
            a.map_or(0, |a| a.host_lane_assignment_options),
            meaning_state(&r.host),
            meaning_state(&r.media),
            lane_state(r.host_lanes),
            lane_state(r.media_lanes),
            match r.media {
                InterfaceMeaning::UnsupportedMediaType(code) => code,
                _ => 0,
            },
            r.host.meaning().as_deref().unwrap_or("-"),
            r.media.meaning().as_deref().unwrap_or("-")
        );
    }
}
