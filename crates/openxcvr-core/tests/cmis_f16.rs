//! Exhaustive and golden tests for the CMIS / SFF-8636 F16 numeric type.
//!
//! The exhaustive digest is bit-for-bit cross-language evidence: FNV-1a-64
//! over the little-endian binary64 bit patterns of every code's `to_f64`
//! result must equal the digest of the independently computed exact
//! reference (Python `fractions.Fraction`, nearest-even rounding), which
//! the C binding must reproduce as well.

use openxcvr_core::CmisF16;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
const REFERENCE_DIGEST: u64 = 0x98194d0508bcca80;

/// FNV-1a-64 over the little-endian bytes of every code's binary64 result,
/// codes 0..=65535 in order — the same byte stream as the exact reference.
#[test]
fn exhaustive_digest_matches_reference() {
    let mut hash = FNV_OFFSET;
    for code in 0..=u16::MAX {
        let bytes = CmisF16::from_raw(code).to_f64().to_bits().to_le_bytes();
        for &b in &bytes {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    assert_eq!(hash, REFERENCE_DIGEST, "exhaustive F16 conversion digest");
}

/// Per-code oracle: for every code, `to_f64` must be bit-identical to
/// parsing "<m>e<e>" as binary64 on this platform.
#[test]
fn per_code_parse_oracle() {
    for code in 0..=u16::MAX {
        let m = code & 0x7ff;
        let e = i32::from(code >> 11) - 24;
        let text = format!("{m}e{e}");
        let expected: f64 = text.parse().expect("valid f64 literal");
        let actual = CmisF16::from_raw(code).to_f64();
        if actual.to_bits() != expected.to_bits() {
            panic!(
                "first mismatching code {code:#06x}: m={m}, e={e}, \
                 to_f64 bits {:#018x}, oracle {text} bits {:#018x}",
                actual.to_bits(),
                expected.to_bits(),
            );
        }
    }
}

/// Golden binary64 bit patterns, shared with the C binding's tests.
#[test]
fn golden_bit_patterns() {
    let goldens: &[(u16, u64)] = &[
        (0x0000, 0x0000000000000000),
        (0xF800, 0x0000000000000000),
        (0x07FF, 0x3ba35557a1555996),
        (0x0801, 0x3b282db34012b251),
        (0x1001, 0x3b5e392010175ee6),
        (0x6001, 0x3d719799812dea11),
        (0xC001, 0x3ff0000000000000),
        (0xC00A, 0x4024000000000000),
        (0xFFFF, 0x4213106da6000000),
    ];
    for &(code, bits) in goldens {
        assert_eq!(
            CmisF16::from_raw(code).to_f64().to_bits(),
            bits,
            "golden bits for code {code:#06x}"
        );
    }
}

/// Accessors for every code, plus the zero-encoding distinctness contract.
#[test]
fn accessors_for_every_code() {
    for code in 0..=u16::MAX {
        let v = CmisF16::from_raw(code);
        assert_eq!(v.raw(), code, "raw round-trip for {code:#06x}");
        assert_eq!(v.mantissa(), code & 0x7ff, "mantissa for {code:#06x}");
        assert_eq!(v.scaled_exponent(), (code >> 11) as u8);
        assert_eq!(v.exponent(), (code >> 11) as i8 - 24);
        assert_eq!(
            CmisF16::from_be_bytes(code.to_be_bytes()),
            CmisF16::from_raw(code),
            "from_be_bytes for {code:#06x}"
        );
    }
    // 0x0000 and 0xF800: two distinct zero encodings — equal values,
    // distinct raw codes.
    let zero = CmisF16::from_raw(0x0000);
    let other_zero = CmisF16::from_raw(0xF800);
    assert_eq!(zero.to_f64().to_bits(), other_zero.to_f64().to_bits());
    assert_ne!(zero.raw(), other_zero.raw());
    assert_ne!(zero, other_zero);
}
