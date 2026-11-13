//! The CMIS / SFF-8636 F16 numeric type.
//!
//! F16 is the diagnostic-value decimal floating-point encoding of the
//! CMIS and SFF-8636 diagnostic tables (the Page 14h BER rows carry
//! provenance "CMIS 5.3 § 8.13.3"): an 11-bit mantissa and a 5-bit
//! scaled exponent, `value = m * 10^(s - 24)` with `m` in `0..=2047`
//! and `s` in `0..=31`.
//!
//! This is **not IEEE 754 binary16**: the type is unsigned, there is no
//! NaN, no infinity and no sentinel encoding. Every one of the 65,536 raw
//! codes is a valid value; the 32 encodings with mantissa 0 are distinct
//! encodings of zero and remain distinct through [`CmisF16::raw`].

/// A CMIS / SFF-8636 F16 diagnostic value: an unsigned decimal
/// floating-point code with `value = m * 10^(s - 24)` where `m` (bits
/// 10..0, `0..=2047`) is the mantissa and `s` (bits 15..11, `0..=31`) is
/// the scaled exponent.
///
/// Not IEEE binary16: unsigned, no NaN, no infinity, no sentinels. All
/// 65,536 codes are valid; the 32 mantissa-zero encodings of zero are
/// preserved as distinct raw codes by [`raw`](CmisF16::raw).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CmisF16(u16);

/// Exact decimal powers of ten as binary64: `10^k` for `k` in `0..=22`
/// is exactly representable, so `m * P10[e]` and `m / P10[-e]` are a
/// single correctly rounded operation each.
const P10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

impl CmisF16 {
    /// Wrap a raw F16 code. Every 16-bit pattern is a valid code.
    pub const fn from_raw(raw: u16) -> Self {
        CmisF16(raw)
    }

    /// Wrap the two big-endian bytes of an F16 code.
    pub const fn from_be_bytes(bytes: [u8; 2]) -> Self {
        CmisF16(u16::from_be_bytes(bytes))
    }

    /// The raw 16-bit code, exactly as supplied to [`from_raw`](CmisF16::from_raw):
    /// the 32 distinct zero encodings stay distinct.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Mantissa `m`: bits 10..0, `0..=2047`.
    pub const fn mantissa(self) -> u16 {
        self.0 & 0x7ff
    }

    /// Scaled exponent `s`: bits 15..11, `0..=31`.
    pub const fn scaled_exponent(self) -> u8 {
        (self.0 >> 11) as u8
    }

    /// Exponent `s - 24`, `-24..=7`.
    pub const fn exponent(self) -> i8 {
        (self.0 >> 11) as i8 - 24
    }

    /// The value as binary64: the nearest `f64` to `m * 10^(s - 24)`
    /// (round-to-nearest, ties-to-even), assuming ordinary IEEE binary64
    /// arithmetic. Verified exhaustively over all 65,536 codes by this
    /// crate's tests.
    ///
    /// `e >= 0`: `m * 10^e` with the exact power table `P10` — the product
    /// of an integer `m <= 2047` and an exact `10^e` (`e <= 7`) is exact.
    /// `-22 <= e < 0`: `m / 10^-e` — the divisor is exact, so one correctly
    /// rounded division. `e` in `-23..=-24`: `10^e` is not exactly
    /// representable; a two-part (Dekker-style) split `10^e = hi + lo` with
    /// `hi` the rounded value and `lo` the exact error (2^23 resp. 2^24)
    /// yields the correctly rounded quotient via one fused multiply-add.
    pub fn to_f64(self) -> f64 {
        let m = f64::from(self.mantissa());
        let e = i32::from(self.scaled_exponent()) - 24;
        if e >= 0 {
            return m * P10[e as usize];
        }
        if e >= -22 {
            return m / P10[(-e) as usize];
        }
        let (hi, lo) = if e == -23 {
            (1e23, 8388608.0)
        } else {
            (1e24, 16777216.0)
        };
        let q = m / hi;
        let r = (-q).mul_add(hi, m) - q * lo;
        q + r / hi
    }
}
