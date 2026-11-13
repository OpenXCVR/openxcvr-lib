//! Transport-level VDM operations over Pages 20h-2Fh (CMIS 4.0+).
//!
//! Two correctness rules drive this module (schema is the source of truth):
//!
//! 1. **Group gating** — the number of VDM groups a module implements is
//!    advertised by Page 2Fh byte 128 bits 1-0 (`vdm_support`, CmisVdmSupport:
//!    0 = group 1 only, 3 = groups 1-4). Enumeration and reads only touch
//!    descriptor/sample pages of advertised groups; a request outside the
//!    advertised set fails closed instead of reading a page the module never
//!    promised (pages 20h-23h / 24h-27h are distinct page addresses per
//!    group, not banks).
//!
//! 2. **Polymorphic sample decode** — Pages 24h-27h carry each instance's
//!    value as an X16 register whose storage type is selected by the
//! observable type of the matching descriptor instance (variant
//!    table in `page-24h-vdm-samples-1-64.yaml`): U16 for laser age, SNR,
//!    PAM4 LTP, SEWmax and the voltage monitors, S16 for TEC current, laser
//!    frequency error, laser temperature and ELS input power, F16 for the
//!    Pre-FEC BER and FERC observables. The decoded value is returned
//!    alongside the raw register.

use std::{error::Error, fmt};

use openxcvr_core::{error::ModuleError, transport::I2C_ADDR_MODULE};

use crate::{decode::cmis_f16be, module::CmisTransport};

/// Page addresses of the VDM feature (CMIS § 8.27 / pre-5.4 § 8.22).
const PAGE_2FH: u8 = 0x2F;
const DESCRIPTOR_BASE_PAGE: u8 = 0x20;
const SAMPLE_BASE_PAGE: u8 = 0x24;
/// Page 2Fh byte 128 bits 1-0: VDMSupport advertisement.
const VDM_SUPPORT_BYTE: u8 = 128;

/// Why a VDM operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum VdmError {
    /// The transport read or write failed; the original `ModuleError` is
    /// preserved as the error source instead of being flattened to text.
    Transport(ModuleError),
    /// The module does not advertise VDM telemetry support.
    Unsupported,
    /// The requested observable ID lives in a VDM group the module does not
    /// advertise (2Fh:128.1-0 `vdm_support`).
    GroupNotAdvertised {
        /// The advertised group count the module reported.
        advertised: u8,
        /// The group the request addressed.
        requested: u8,
    },
    /// The observable ID is not an active instance in the descriptor page.
    InvalidObservableId(u8),
}

impl fmt::Display for VdmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "VDM transport error: {err}"),
            Self::Unsupported => write!(f, "VDM telemetry is unsupported"),
            Self::GroupNotAdvertised {
                advertised,
                requested,
            } => write!(
                f,
                "VDM group {requested} is not advertised (vdm_support = {advertised})"
            ),
            Self::InvalidObservableId(id) => write!(f, "invalid VDM observable ID {id}"),
        }
    }
}

impl Error for VdmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ModuleError> for VdmError {
    fn from(err: ModuleError) -> Self {
        Self::Transport(err)
    }
}

/// One VDM instance sample: the raw X16 register contents plus the value
/// decoded per the instance's observable type.
#[derive(Debug, Clone, PartialEq)]
pub struct Observable {
    /// The observable's ID (Page 2Fh byte 128 group descriptor).
    pub id: u8,
    /// The lane the instance belongs to (0 = module-wide).
    pub lane: u8,
    /// The observable type of the matching descriptor instance (Pages 20h-23h
    /// odd byte); 0 means the instance is not active and the sample is
    /// ignored per the spec.
    pub observable_type: u8,
    /// The raw 16-bit big-endian register value exactly as read.
    pub raw_value: u16,
    /// The value decoded by the descriptor's observable type.
    pub value: VdmSampleValue,
}

/// The decoded per-observable-type sample value:
/// U16, S16 or F16, all big endian.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VdmSampleValue {
    /// An unsigned 16-bit value.
    U16(u16),
    /// A signed 16-bit value.
    I16(i16),
    /// A CMIS F16 half-precision value.
    F16(f32),
}

/// Read the module's advertised VDM group count (Page 2Fh:128 bits 1-0,
/// `CmisVdmSupport`: code 0 = group 1 only .. code 3 = groups 1-4, so the
/// advertised group count is the code plus one).
///
/// Fails closed: an unreadable Page 2Fh is a transport error, never an
/// implicit "all four groups".
fn advertised_group_count<T: CmisTransport>(transport: &mut T) -> Result<u8, VdmError> {
    let mut buf = [0u8; 1];
    transport.read(I2C_ADDR_MODULE, PAGE_2FH, 0, VDM_SUPPORT_BYTE, &mut buf)?;
    Ok((buf[0] & 0x03) + 1)
}

/// Enumerate the active VDM observable IDs across the advertised groups.
///
/// A descriptor instance (2 bytes on Pages 20h-23h) is active when its
/// observable-type byte is non-zero (CMIS: an unused slot is 00h in both
/// descriptor bytes). Groups beyond the advertised set are never read.
pub fn list_observables<T: CmisTransport>(transport: &mut T) -> Result<Vec<u8>, VdmError> {
    let advertised = advertised_group_count(transport)?;
    let mut observables = Vec::new();
    for group in 0..advertised {
        let mut descriptors = [0u8; 128];
        transport.read(
            I2C_ADDR_MODULE,
            DESCRIPTOR_BASE_PAGE + group,
            0,
            128,
            &mut descriptors,
        )?;
        for local in 0..64usize {
            if descriptors[local * 2 + 1] != 0 {
                observables.push((group as usize * 64 + local) as u8);
            }
        }
    }
    Ok(observables)
}

/// Read one VDM observable's current sample and decode it by the instance's
/// observable type.
///
/// The observable ID addresses one of up to 256 VDM instances: group = id / 64
/// selects the descriptor/sample page pair (20h+group / 24h+group), local =
/// id % 64 the instance within the group. Reading a group the module does not
/// advertise fails closed with [`VdmError::GroupNotAdvertised`]; the
/// descriptor's observable type is always re-read from the module rather
/// than assumed.
pub fn read_observable<T: CmisTransport>(
    transport: &mut T,
    observable_id: u8,
    lane: u8,
) -> Result<Observable, VdmError> {
    let advertised = advertised_group_count(transport)?;
    let group = observable_id / 64;
    let local = observable_id % 64;
    if group >= advertised {
        // `advertised` is the one-based group count; report the raw
        // CmisVdmSupport code (count - 1) in the error, matching the
        // advertisement register a host would read.
        return Err(VdmError::GroupNotAdvertised {
            advertised: advertised - 1,
            requested: group,
        });
    }

    // The descriptor's observable type selects the sample storage type.
    let mut type_buf = [0u8; 1];
    transport.read(
        I2C_ADDR_MODULE,
        DESCRIPTOR_BASE_PAGE + group,
        0,
        128 + 2 * local + 1,
        &mut type_buf,
    )?;
    let observable_type = type_buf[0];

    let mut buf = [0u8; 2];
    transport.read(
        I2C_ADDR_MODULE,
        SAMPLE_BASE_PAGE + group,
        0,
        128 + 2 * local,
        &mut buf,
    )?;
    let raw_value = u16::from_be_bytes(buf);

    Ok(Observable {
        id: observable_id,
        lane,
        observable_type,
        raw_value,
        value: decode_sample(observable_type, buf),
    })
}

/// Decode one X16 sample register per the observable type, mirroring the
/// variant table declared by `page-24h-vdm-samples-1-64.yaml`:
///
/// - 0x00 not used: raw U16, sample ignored per the spec;
/// - 0x01 laser age: U16;
/// - 0x02-0x04 TEC current, laser frequency error, laser temperature: S16;
/// - 0x05-0x08 SNR and PAM4 level transition parameters: U16;
/// - 0x09-0x1a Pre-FEC BER and FERC (min/max/average/current/accumulated):
///   F16 (`m * 10^(s-24)`);
/// - 0x1b-0x22 SEWmax: U16;
/// - 0x23-0x4c reserved: carried as raw U16;
/// - 0x4d-0x53 module supply voltage monitors: U16;
/// - 0x54 ELS input power: S16;
/// - 0x55-0xff reserved/custom/restricted: carried as raw U16.
fn decode_sample(observable_type: u8, bytes: [u8; 2]) -> VdmSampleValue {
    match observable_type {
        0x02..=0x04 | 0x54 => VdmSampleValue::I16(i16::from_be_bytes(bytes)),
        0x09..=0x1a => VdmSampleValue::F16(cmis_f16be(bytes)),
        _ => VdmSampleValue::U16(u16::from_be_bytes(bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openxcvr_core::{
        error::ModuleError,
        transport::{Transport, I2C_ADDR_MODULE},
    };

    struct MockTransport {
        pages: [[u8; 256]; 16],
    }

    impl Default for MockTransport {
        fn default() -> Self {
            let mut pages = [[0u8; 256]; 16];
            // Page 2Fh byte 128 bits 1-0: advertise groups 1-2 by default.
            pages[(PAGE_2FH - 0x20) as usize][VDM_SUPPORT_BYTE as usize] = 0x01;
            Self { pages }
        }
    }

    impl Transport for MockTransport {
        fn read(
            &mut self,
            slave_addr: u8,
            page: u8,
            bank: u8,
            offset: u8,
            buf: &mut [u8],
        ) -> Result<(), ModuleError> {
            assert_eq!(slave_addr, I2C_ADDR_MODULE);
            assert_eq!(bank, 0);
            assert!((0x20..=0x2f).contains(&page));
            let start = offset as usize;
            buf.copy_from_slice(&self.pages[(page - 0x20) as usize][start..start + buf.len()]);
            Ok(())
        }

        fn write(
            &mut self,
            _slave_addr: u8,
            _page: u8,
            _bank: u8,
            _offset: u8,
            _data: &[u8],
        ) -> Result<(), ModuleError> {
            Ok(())
        }

        fn probe(&mut self) -> Result<(), ModuleError> {
            Ok(())
        }
    }

    impl CmisTransport for MockTransport {}

    fn page_index(page: u8) -> usize {
        (page - 0x20) as usize
    }

    #[test]
    fn test_list_observables_respects_advertised_groups() {
        let mut transport = MockTransport::default();
        // Group 0 and group 1 each hold one active descriptor; group 3 holds
        // one too but is NOT advertised (vdm_support = 1, groups 1-2 only).
        transport.pages[page_index(0x20)][129] = 0x01;
        transport.pages[page_index(0x21)][129] = 0x02;
        transport.pages[page_index(0x23)][255] = 0x03;

        assert_eq!(list_observables(&mut transport).unwrap(), vec![0, 64]);

        // Widening the advertisement to all four groups surfaces group 4.
        transport.pages[page_index(0x2f)][VDM_SUPPORT_BYTE as usize] = 0x03;
        assert_eq!(list_observables(&mut transport).unwrap(), vec![0, 64, 255]);
    }

    #[test]
    fn test_list_observables_zero_advertisement_reads_nothing() {
        let mut transport = MockTransport::default();
        transport.pages[page_index(0x2f)][VDM_SUPPORT_BYTE as usize] = 0x00;
        // vdm_support = 0 still means group 1 is supported; only groups 2+
        // are excluded, so the group-1 descriptor is still enumerated.
        transport.pages[page_index(0x20)][129] = 0x01;
        assert_eq!(list_observables(&mut transport).unwrap(), vec![0]);
    }

    #[test]
    fn test_read_observable_gates_on_advertised_groups() {
        let mut transport = MockTransport::default();
        transport.pages[page_index(0x23)][255] = 0x03;
        transport.pages[page_index(0x27)][128] = 0x12;
        transport.pages[page_index(0x27)][129] = 0x34;

        // Observable 255 lives in group 3 (pages 23h/27h); the module
        // advertises groups 1-2 only.
        assert!(matches!(
            read_observable(&mut transport, 255, 0),
            Err(VdmError::GroupNotAdvertised {
                advertised: 1,
                requested: 3,
            })
        ));

        // Widening the advertisement admits the read. Observable 255 is
        // local instance 63 of group 3: descriptor type byte at 23h:255,
        // sample at 27h:254-255.
        transport.pages[page_index(0x2f)][VDM_SUPPORT_BYTE as usize] = 0x03;
        transport.pages[page_index(0x27)][254] = 0x12;
        transport.pages[page_index(0x27)][255] = 0x34;
        let obs = read_observable(&mut transport, 255, 0).unwrap();
        assert_eq!(obs.raw_value, 0x1234);
        assert_eq!(obs.observable_type, 0x03);
        // Type 3 (laser frequency error) is S16.
        assert_eq!(obs.value, VdmSampleValue::I16(0x1234));
    }

    #[test]
    fn test_read_observable_dispatches_f16be() {
        let mut transport = MockTransport::default();
        // Instance 1 of group 0: descriptor byte 129 declares observable type
        // 0x09 (pre-FEC BER minimum, F16), sample at 24h:128-129.
        transport.pages[page_index(0x20)][129] = 0x09;
        transport.pages[page_index(0x24)][128] = 0xc0;
        transport.pages[page_index(0x24)][129] = 0x01;

        let obs = read_observable(&mut transport, 0, 3).unwrap();
        assert_eq!(obs.observable_type, 0x09);
        assert_eq!(obs.raw_value, 0xc001);
        // F16 m * 10^(s-24) with m=1, s=24 decodes to 1.0.
        assert_eq!(obs.value, VdmSampleValue::F16(1.0));
        assert_eq!(obs.lane, 3);
    }

    #[test]
    fn test_read_observable_dispatches_u16_and_i16() {
        let mut transport = MockTransport::default();
        // Instance 2 of group 0: laser age (type 1, U16), sample 0xABCD.
        transport.pages[page_index(0x20)][133] = 0x01;
        transport.pages[page_index(0x24)][130] = 0xAB;
        transport.pages[page_index(0x24)][131] = 0xCD;
        // Instance 3 of group 0 (local index 2): TEC current (type 2, S16),
        // descriptor type byte at 20h:133, sample at 24h:132-133 = 0x8000.
        transport.pages[page_index(0x20)][133] = 0x02;
        transport.pages[page_index(0x24)][132] = 0x80;
        transport.pages[page_index(0x24)][133] = 0x00;

        let age = read_observable(&mut transport, 1, 0).unwrap();
        assert_eq!(age.value, VdmSampleValue::U16(0xABCD));

        let tec = read_observable(&mut transport, 2, 0).unwrap();
        assert_eq!(tec.value, VdmSampleValue::I16(-32768));
    }

    #[test]
    fn test_decode_sample_matches_variant_table() {
        // U16 carriers: type 0 (not used), 1 (laser age), 5-8 (SNR/LTP),
        // 0x1b-0x22 (SEWmax), 0x23-0x4c (reserved), 0x4d-0x53 (supplies),
        // 0x55-0xff (reserved/custom/restricted).
        for ty in [
            0x00u8, 0x01, 0x05, 0x08, 0x1b, 0x22, 0x23, 0x4c, 0x4d, 0x53, 0x55, 0xff,
        ] {
            assert_eq!(
                decode_sample(ty, [0xFF, 0x80]),
                VdmSampleValue::U16(0xFF80),
                "ty={ty:#04x}"
            );
        }
        // S16 carriers: types 2-4 (TEC/frequency error/temperature) and 0x54
        // (ELS input power).
        for ty in [0x02u8, 0x03, 0x04, 0x54] {
            assert_eq!(
                decode_sample(ty, [0xFF, 0x80]),
                VdmSampleValue::I16(-128),
                "ty={ty:#04x}"
            );
        }
        // F16 carriers: types 0x09-0x1a (pre-FEC BER + FERC families).
        for ty in 0x09u8..=0x1a {
            assert!(
                matches!(decode_sample(ty, [0xC0, 0x01]), VdmSampleValue::F16(v) if (v - 1.0).abs() < f32::EPSILON),
                "ty={ty:#04x}"
            );
        }
    }
}
