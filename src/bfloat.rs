use core::{
    cmp::Ordering,
    iter::{Product, Sum},
    num::FpCategory,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign},
};
#[cfg(not(target_arch = "spirv"))]
use core::{
    fmt::{
        Binary,
        Debug,
        Display,
        Error,
        Formatter,
        LowerExp,
        LowerHex,
        Octal,
        UpperExp,
        UpperHex,
    },
    num::ParseFloatError,
    str::FromStr,
};

use crate::error::TryFromFloatError;
use crate::try_from::try_from_lossless;

pub(crate) mod convert;

/// A 16-bit floating point type implementing the [`bfloat16`] format.
///
/// The [`bfloat16`] floating point format is a truncated 16-bit version of the
/// IEEE 754 standard `binary32`, a.k.a [`f32`]. [`struct@bf16`] has
/// approximately the same dynamic range as [`f32`] by having a lower precision
/// than [`f16`][crate::f16]. While [`f16`][crate::f16] has a precision of
/// 11 bits, [`struct@bf16`] has a precision of only 8 bits.
///
/// [`bfloat16`]: https://en.wikipedia.org/wiki/Bfloat16_floating-point_format
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Default)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct bf16(u16);

impl bf16 {
    /// Constructs a [`struct@bf16`] value from the raw bits.
    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u16) -> bf16 {
        bf16(bits)
    }

    /// Constructs a [`struct@bf16`] value from a 32-bit floating point value.
    ///
    /// This operation is lossy. If the 32-bit value is too large to fit, ±∞
    /// will result. NaN values are preserved. Subnormal values that are too
    /// tiny to be represented will result in ±0. All other values are
    /// truncated and rounded to the nearest representable value.
    #[inline]
    #[must_use]
    pub fn from_f32(value: f32) -> bf16 {
        Self::from_f32_const(value)
    }

    /// Constructs a [`struct@bf16`] value from a 32-bit floating point value.
    ///
    /// This function is identical to [`from_f32`][Self::from_f32] except it
    /// never uses hardware intrinsics, which allows it to be `const`.
    /// [`from_f32`][Self::from_f32] should be preferred in any non-`const`
    /// context.
    ///
    /// This operation is lossy. If the 32-bit value is too large to fit, ±∞
    /// will result. NaN values are preserved. Subnormal values that are too
    /// tiny to be represented will result in ±0. All other values are
    /// truncated and rounded to the nearest representable value.
    #[inline]
    #[must_use]
    pub const fn from_f32_const(value: f32) -> bf16 {
        bf16(convert::f32_to_bf16(value))
    }

    /// Create a [`struct@bf16`] loslessly from an [`f32`].
    ///
    /// This is only true if the [`f32`] is non-finite
    /// (infinite or NaN), or no non-zero bits would
    /// be truncated.
    ///
    /// "Lossless" does not mean the data is represented the
    /// same as a decimal number. For example, an [`f32`]
    /// and [`f64`] have the significant digits (excluding the
    /// hidden bit) for a value closest to `1e35` of:
    /// - `f32`: `110100001001100001100`
    /// - `f64`: `11010000100110000110000000000000000000000000000000`
    ///
    /// However, the [`f64`] is displayed as `1.0000000409184788e+35`,
    /// while the value closest to `1e35` in [`f64`] is
    /// `11010000100110000101110010110001110100110110000010`. This
    /// makes it look like precision has been lost but this is
    /// due to the approximations used to represent binary values as
    /// a decimal.
    ///
    /// This does not respect signalling NaNs: if the value
    /// is NaN or inf, then it will return that value.
    ///
    /// Since [`struct@bf16`] has the same number of exponent
    /// bits  as [`f32`], this is effectively just checking if the
    /// value is non-finite (infinite or NaN) or the value
    /// is normal and the lower 16 bits are 0.
    #[inline]
    pub const fn from_f32_lossless(value: f32) -> Option<bf16> {
        // NOTE: This logic is effectively just getting the top 16 bits
        // and the bottom 16 bits, but it's done explicitly with mantissa
        // digits for this reason. For explicit clarity, we remove the
        // hidden bit in our exponent logic
        const BF16_MANT_BITS: u32 = bf16::MANTISSA_DIGITS - 1;
        const F32_MANT_BITS: u32 = f32::MANTISSA_DIGITS - 1;
        const EXP_MASK: u32 = (f32::MAX_EXP as u32 * 2 - 1) << F32_MANT_BITS;
        const TRUNCATED: u32 = F32_MANT_BITS - BF16_MANT_BITS;
        const TRUNC_MASK: u32 = (1 << TRUNCATED) - 1;

        // SAFETY: safe since it's plain old data
        let bits: u32 = unsafe { core::mem::transmute(value) };

        // `bits & exp_mask == exp_mask` -> infinite or NaN
        // `truncated == 0` -> no bits truncated
        // since the exp ranges are the same, any denormal handling
        // is already implicit.
        let exp = bits & EXP_MASK;
        let is_special = exp == EXP_MASK;
        if is_special || bits & TRUNC_MASK == 0 {
            Some(Self::from_f32_const(value))
        } else {
            None
        }
    }

    /// Constructs a [`struct@bf16`] value from a 64-bit floating point value.
    ///
    /// This operation is lossy. If the 64-bit value is to large to fit, ±∞ will
    /// result. NaN values are preserved. 64-bit subnormal values are too
    /// tiny to be represented and result in ±0. Exponents that underflow
    /// the minimum exponent will result in subnormals or ±0. All other
    /// values are truncated and rounded to the nearest representable value.
    #[inline]
    #[must_use]
    pub fn from_f64(value: f64) -> bf16 {
        Self::from_f64_const(value)
    }

    /// Constructs a [`struct@bf16`] value from a 64-bit floating point value.
    ///
    /// This function is identical to [`from_f64`][Self::from_f64] except it
    /// never uses hardware intrinsics, which allows it to be `const`.
    /// [`from_f64`][Self::from_f64] should be preferred in any non-`const`
    /// context.
    ///
    /// This operation is lossy. If the 64-bit value is to large to fit, ±∞ will
    /// result. NaN values are preserved. 64-bit subnormal values are too
    /// tiny to be represented and result in ±0. Exponents that underflow
    /// the minimum exponent will result in subnormals or ±0. All other
    /// values are truncated and rounded to the nearest representable value.
    #[inline]
    #[must_use]
    pub const fn from_f64_const(value: f64) -> bf16 {
        bf16(convert::f64_to_bf16(value))
    }

    /// Create a [`struct@bf16`] loslessly from an [`f64`].
    ///
    /// This is only true if the [`f64`] is non-finite
    /// (infinite or NaN), zero, or the exponent can be
    /// represented by a normal [`struct@bf16`] and no
    /// non-zero bits would be truncated.
    ///
    /// "Lossless" does not mean the data is represented the
    /// same as a decimal number. For example, an [`f32`]
    /// and [`f64`] have the significant digits (excluding the
    /// hidden bit) for a value closest to `1e35` of:
    /// - `f32`: `110100001001100001100`
    /// - `f64`: `11010000100110000110000000000000000000000000000000`
    ///
    /// However, the [`f64`] is displayed as `1.0000000409184788e+35`,
    /// while the value closest to `1e35` in [`f64`] is
    /// `11010000100110000101110010110001110100110110000010`. This
    /// makes it look like precision has been lost but this is
    /// due to the approximations used to represent binary values as
    /// a decimal.
    ///
    /// This does not respect signalling NaNs: if the value
    /// is NaN or inf, then it will return that value.
    #[inline]
    pub const fn from_f64_lossless(value: f64) -> Option<bf16> {
        try_from_lossless!(
            value => value,
            half => bf16,
            full => f64,
            half_bits => u16,
            full_bits => u64,
            to_half => from_f64
        )
    }

    /// Converts a [`struct@bf16`] into the underlying bit representation.
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u16 {
        self.0
    }

    /// Returns the memory representation of the underlying bit representation
    /// as a byte array in little-endian byte order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let bytes = bf16::from_f32(12.5).to_le_bytes();
    /// assert_eq!(bytes, [0x48, 0x41]);
    /// ```
    #[inline]
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// Returns the memory representation of the underlying bit representation
    /// as a byte array in big-endian (network) byte order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let bytes = bf16::from_f32(12.5).to_be_bytes();
    /// assert_eq!(bytes, [0x41, 0x48]);
    /// ```
    #[inline]
    #[must_use]
    pub const fn to_be_bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }

    /// Returns the memory representation of the underlying bit representation
    /// as a byte array in native byte order.
    ///
    /// As the target platform's native endianness is used, portable code should
    /// use [`to_be_bytes`][bf16::to_be_bytes] or
    /// [`to_le_bytes`][bf16::to_le_bytes], as appropriate, instead.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let bytes = bf16::from_f32(12.5).to_ne_bytes();
    /// assert_eq!(bytes, if cfg!(target_endian = "big") {
    ///     [0x41, 0x48]
    /// } else {
    ///     [0x48, 0x41]
    /// });
    /// ```
    #[inline]
    #[must_use]
    pub const fn to_ne_bytes(self) -> [u8; 2] {
        self.0.to_ne_bytes()
    }

    /// Creates a floating point value from its representation as a byte array
    /// in little endian.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let value = bf16::from_le_bytes([0x48, 0x41]);
    /// assert_eq!(value, bf16::from_f32(12.5));
    /// ```
    #[inline]
    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; 2]) -> bf16 {
        bf16::from_bits(u16::from_le_bytes(bytes))
    }

    /// Creates a floating point value from its representation as a byte array
    /// in big endian.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let value = bf16::from_be_bytes([0x41, 0x48]);
    /// assert_eq!(value, bf16::from_f32(12.5));
    /// ```
    #[inline]
    #[must_use]
    pub const fn from_be_bytes(bytes: [u8; 2]) -> bf16 {
        bf16::from_bits(u16::from_be_bytes(bytes))
    }

    /// Creates a floating point value from its representation as a byte array
    /// in native endian.
    ///
    /// As the target platform's native endianness is used, portable code likely
    /// wants to use [`from_be_bytes`][bf16::from_be_bytes] or
    /// [`from_le_bytes`][bf16::from_le_bytes], as appropriate instead.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    /// let value = bf16::from_ne_bytes(if cfg!(target_endian = "big") {
    ///     [0x41, 0x48]
    /// } else {
    ///     [0x48, 0x41]
    /// });
    /// assert_eq!(value, bf16::from_f32(12.5));
    /// ```
    #[inline]
    #[must_use]
    pub const fn from_ne_bytes(bytes: [u8; 2]) -> bf16 {
        bf16::from_bits(u16::from_ne_bytes(bytes))
    }

    /// Converts a [`struct@bf16`] value into an [`f32`] value.
    ///
    /// This conversion is lossless as all values can be represented exactly in
    /// [`f32`].
    #[inline]
    #[must_use]
    pub fn to_f32(self) -> f32 {
        self.to_f32_const()
    }

    /// Converts a [`struct@bf16`] value into an [`f32`] value.
    ///
    /// This function is identical to [`to_f32`][Self::to_f32] except it never
    /// uses hardware intrinsics, which allows it to be `const`.
    /// [`to_f32`][Self::to_f32] should be preferred in any non-`const`
    /// context.
    ///
    /// This conversion is lossless as all values can be represented exactly in
    /// [`f32`].
    #[inline]
    #[must_use]
    pub const fn to_f32_const(self) -> f32 {
        convert::bf16_to_f32(self.0)
    }

    /// Convert the data to an `f32` type, used for numerical operations.
    #[inline(always)]
    pub fn as_f32(self) -> f32 {
        self.to_f32_const()
    }

    /// Convert the data to an `f32` type, used for numerical operations.
    #[inline(always)]
    pub const fn as_f32_const(self) -> f32 {
        self.to_f32_const()
    }

    /// Converts a [`struct@bf16`] value into an [`f64`] value.
    ///
    /// This conversion is lossless as all values can be represented exactly in
    /// [`f64`].
    #[inline]
    #[must_use]
    pub fn to_f64(self) -> f64 {
        self.to_f64_const()
    }

    /// Converts a [`struct@bf16`] value into an [`f64`] value.
    ///
    /// This function is identical to [`to_f64`][Self::to_f64] except it never
    /// uses hardware intrinsics, which allows it to be `const`.
    /// [`to_f64`][Self::to_f64] should be preferred in any non-`const`
    /// context.
    ///
    /// This conversion is lossless as all values can be represented exactly in
    /// [`f64`].
    #[inline]
    #[must_use]
    pub const fn to_f64_const(self) -> f64 {
        convert::bf16_to_f64(self.0)
    }

    /// Convert the data to an `f64` type, used for numerical operations.
    #[inline(always)]
    pub fn as_f64(self) -> f64 {
        self.to_f64_const()
    }

    /// Convert the data to an `f64` type, used for numerical operations.
    #[inline(always)]
    pub const fn as_f64_const(self) -> f64 {
        self.to_f64_const()
    }

    /// Returns `true` if this value is NaN and `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let nan = bf16::NAN;
    /// let f = bf16::from_f32(7.0_f32);
    ///
    /// assert!(nan.is_nan());
    /// assert!(!f.is_nan());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_nan(self) -> bool {
        self.0 & Self::NOT_SIGN > Self::EXP_MASK
    }

    /// Computes the absolute value of `self`.
    #[must_use]
    #[inline(always)]
    pub const fn abs(self) -> Self {
        Self(self.0 & !Self::SIGN_MASK)
    }

    /// Returns `true` if this value is ±∞ and `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let f = bf16::from_f32(7.0f32);
    /// let inf = bf16::INFINITY;
    /// let neg_inf = bf16::NEG_INFINITY;
    /// let nan = bf16::NAN;
    ///
    /// assert!(!f.is_infinite());
    /// assert!(!nan.is_infinite());
    ///
    /// assert!(inf.is_infinite());
    /// assert!(neg_inf.is_infinite());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_infinite(self) -> bool {
        self.0 & Self::NOT_SIGN == Self::EXP_MASK
    }

    /// Returns `true` if this number is neither infinite nor NaN.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let f = bf16::from_f32(7.0f32);
    /// let inf = bf16::INFINITY;
    /// let neg_inf = bf16::NEG_INFINITY;
    /// let nan = bf16::NAN;
    ///
    /// assert!(f.is_finite());
    ///
    /// assert!(!nan.is_finite());
    /// assert!(!inf.is_finite());
    /// assert!(!neg_inf.is_finite());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_finite(self) -> bool {
        self.0 & Self::EXP_MASK != Self::EXP_MASK
    }

    /// Returns `true` if the number is [subnormal].
    ///
    /// [subnormal]: https://en.wikipedia.org/wiki/Denormal_number
    #[must_use]
    #[inline(always)]
    pub const fn is_subnormal(self) -> bool {
        matches!(self.classify(), FpCategory::Subnormal)
    }

    /// Returns `true` if the number is neither zero, infinite, subnormal, or
    /// NaN.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let min = bf16::MIN_POSITIVE;
    /// let max = bf16::MAX;
    /// let lower_than_min = bf16::from_f32(1.0e-39_f32);
    /// let zero = bf16::from_f32(0.0_f32);
    ///
    /// assert!(min.is_normal());
    /// assert!(max.is_normal());
    ///
    /// assert!(!zero.is_normal());
    /// assert!(!bf16::NAN.is_normal());
    /// assert!(!bf16::INFINITY.is_normal());
    /// // Values between 0 and `min` are subnormal.
    /// assert!(!lower_than_min.is_normal());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_normal(self) -> bool {
        let exp = self.0 & Self::EXP_MASK;
        exp != Self::EXP_MASK && exp != 0
    }

    /// Returns the floating point category of the number.
    ///
    /// If only one property is going to be tested, it is generally faster to
    /// use the specific predicate instead.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::num::FpCategory;
    /// # use float16::*;
    ///
    /// let num = bf16::from_f32(12.4_f32);
    /// let inf = bf16::INFINITY;
    ///
    /// assert_eq!(num.classify(), FpCategory::Normal);
    /// assert_eq!(inf.classify(), FpCategory::Infinite);
    /// ```
    #[inline]
    #[must_use]
    pub const fn classify(self) -> FpCategory {
        let exp = self.0 & Self::EXP_MASK;
        let man = self.0 & Self::MAN_MASK;
        match (exp, man) {
            (0, 0) => FpCategory::Zero,
            (0, _) => FpCategory::Subnormal,
            (Self::EXP_MASK, 0) => FpCategory::Infinite,
            (Self::EXP_MASK, _) => FpCategory::Nan,
            _ => FpCategory::Normal,
        }
    }

    /// Returns a number that represents the sign of `self`.
    ///
    /// * 1.0 if the number is positive, +0.0 or [`INFINITY`][bf16::INFINITY]
    /// * −1.0 if the number is negative, −0.0` or
    ///   [`NEG_INFINITY`][bf16::NEG_INFINITY]
    /// * [`NAN`][bf16::NAN] if the number is NaN
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let f = bf16::from_f32(3.5_f32);
    ///
    /// assert_eq!(f.signum(), bf16::from_f32(1.0));
    /// assert_eq!(bf16::NEG_INFINITY.signum(), bf16::from_f32(-1.0));
    ///
    /// assert!(bf16::NAN.signum().is_nan());
    /// ```
    #[inline]
    #[must_use]
    pub const fn signum(self) -> bf16 {
        if self.is_nan() {
            self
        } else if self.0 & Self::SIGN_MASK != 0 {
            Self::NEG_ONE
        } else {
            Self::ONE
        }
    }

    /// Returns `true` if and only if `self` has a positive sign, including
    /// +0.0, NaNs with a positive sign bit and +∞.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let nan = bf16::NAN;
    /// let f = bf16::from_f32(7.0_f32);
    /// let g = bf16::from_f32(-7.0_f32);
    ///
    /// assert!(f.is_sign_positive());
    /// assert!(!g.is_sign_positive());
    /// // NaN can be either positive or negative
    /// assert!(nan.is_sign_positive() != nan.is_sign_negative());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_sign_positive(self) -> bool {
        self.0 & Self::SIGN_MASK == 0
    }

    /// Returns `true` if and only if `self` has a negative sign, including
    /// −0.0, NaNs with a negative sign bit and −∞.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use float16::*;
    ///
    /// let nan = bf16::NAN;
    /// let f = bf16::from_f32(7.0f32);
    /// let g = bf16::from_f32(-7.0f32);
    ///
    /// assert!(!f.is_sign_negative());
    /// assert!(g.is_sign_negative());
    /// // NaN can be either positive or negative
    /// assert!(nan.is_sign_positive() != nan.is_sign_negative());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_sign_negative(self) -> bool {
        self.0 & Self::SIGN_MASK != 0
    }

    /// Returns a number composed of the magnitude of `self` and the sign of
    /// `sign`.
    ///
    /// Equal to `self` if the sign of `self` and `sign` are the same, otherwise
    /// equal to `-self`. If `self` is NaN, then NaN with the sign of `sign`
    /// is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # use float16::*;
    /// let f = bf16::from_f32(3.5);
    ///
    /// assert_eq!(f.copysign(bf16::from_f32(0.42)), bf16::from_f32(3.5));
    /// assert_eq!(f.copysign(bf16::from_f32(-0.42)), bf16::from_f32(-3.5));
    /// assert_eq!((-f).copysign(bf16::from_f32(0.42)), bf16::from_f32(3.5));
    /// assert_eq!((-f).copysign(bf16::from_f32(-0.42)), bf16::from_f32(-3.5));
    ///
    /// assert!(bf16::NAN.copysign(bf16::from_f32(1.0)).is_nan());
    /// ```
    #[inline]
    #[must_use]
    pub const fn copysign(self, sign: bf16) -> bf16 {
        bf16((sign.0 & Self::SIGN_MASK) | (self.0 & Self::NOT_SIGN))
    }

    /// Takes the reciprocal (inverse) of a number, `1/x`.
    #[must_use]
    #[inline(always)]
    pub fn recip(self) -> Self {
        Self::ONE / self
    }

    /// Converts radians to degrees.
    #[must_use]
    #[inline(always)]
    pub fn to_degrees(self) -> Self {
        self * Self::from(180u8) / Self::PI
    }

    /// Converts degrees to radians.
    #[must_use]
    #[inline(always)]
    pub fn to_radians(self) -> Self {
        self * Self::PI / Self::from(180u8)
    }

    /// Returns the maximum of the two numbers.
    ///
    /// If one of the arguments is NaN, then the other argument is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # use float16::*;
    /// let x = bf16::from_f32(1.0);
    /// let y = bf16::from_f32(2.0);
    ///
    /// assert_eq!(x.max(y), y);
    /// ```
    #[inline]
    #[must_use]
    pub const fn max(self, other: bf16) -> bf16 {
        if self.is_nan() || gt(other, self) {
            other
        } else {
            self
        }
    }

    /// Returns the minimum of the two numbers.
    ///
    /// If one of the arguments is NaN, then the other argument is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # use float16::*;
    /// let x = bf16::from_f32(1.0);
    /// let y = bf16::from_f32(2.0);
    ///
    /// assert_eq!(x.min(y), x);
    /// ```
    #[inline]
    #[must_use]
    pub const fn min(self, other: bf16) -> bf16 {
        if self.is_nan() || lt(other, self) {
            other
        } else {
            self
        }
    }

    /// Restrict a value to a certain interval unless it is NaN.
    ///
    /// Returns `max` if `self` is greater than `max`, and `min` if `self` is
    /// less than `min`. Otherwise this returns `self`.
    ///
    /// Note that this function returns NaN if the initial value was NaN as
    /// well.
    ///
    /// # Panics
    /// Panics if `min > max`, `min` is NaN, or `max` is NaN.
    ///
    /// # Examples
    ///
    /// ```
    /// # use float16::*;
    /// assert!(bf16::from_f32(-3.0).clamp(bf16::from_f32(-2.0), bf16::from_f32(1.0)) == bf16::from_f32(-2.0));
    /// assert!(bf16::from_f32(0.0).clamp(bf16::from_f32(-2.0), bf16::from_f32(1.0)) == bf16::from_f32(0.0));
    /// assert!(bf16::from_f32(2.0).clamp(bf16::from_f32(-2.0), bf16::from_f32(1.0)) == bf16::from_f32(1.0));
    /// assert!(bf16::NAN.clamp(bf16::from_f32(-2.0), bf16::from_f32(1.0)).is_nan());
    /// ```
    #[inline]
    #[must_use]
    pub const fn clamp(self, min: bf16, max: bf16) -> bf16 {
        assert!(le(min, max));
        let mut x = self;
        if lt(x, min) {
            x = min;
        }
        if gt(x, max) {
            x = max;
        }
        x
    }

    /// Returns the ordering between `self` and `other`.
    ///
    /// Unlike the standard partial comparison between floating point numbers,
    /// this comparison always produces an ordering in accordance to
    /// the `totalOrder` predicate as defined in the IEEE 754 (2008 revision)
    /// floating point standard. The values are ordered in the following
    /// sequence:
    ///
    /// - negative quiet NaN
    /// - negative signaling NaN
    /// - negative infinity
    /// - negative numbers
    /// - negative subnormal numbers
    /// - negative zero
    /// - positive zero
    /// - positive subnormal numbers
    /// - positive numbers
    /// - positive infinity
    /// - positive signaling NaN
    /// - positive quiet NaN.
    ///
    /// The ordering established by this function does not always agree with the
    /// [`PartialOrd`] and [`PartialEq`] implementations of `bf16`. For example,
    /// they consider negative and positive zero equal, while `total_cmp`
    /// doesn't.
    ///
    /// The interpretation of the signaling NaN bit follows the definition in
    /// the IEEE 754 standard, which may not match the interpretation by some of
    /// the older, non-conformant (e.g. MIPS) hardware implementations.
    ///
    /// # Examples
    /// ```
    /// # use float16::bf16;
    /// let mut v: Vec<bf16> = vec![];
    /// v.push(bf16::ONE);
    /// v.push(bf16::INFINITY);
    /// v.push(bf16::NEG_INFINITY);
    /// v.push(bf16::NAN);
    /// v.push(bf16::MAX_SUBNORMAL);
    /// v.push(-bf16::MAX_SUBNORMAL);
    /// v.push(bf16::ZERO);
    /// v.push(bf16::NEG_ZERO);
    /// v.push(bf16::NEG_ONE);
    /// v.push(bf16::MIN_POSITIVE);
    ///
    /// v.sort_by(|a, b| a.total_cmp(&b));
    ///
    /// assert!(v
    ///     .into_iter()
    ///     .zip(
    ///         [
    ///             bf16::NEG_INFINITY,
    ///             bf16::NEG_ONE,
    ///             -bf16::MAX_SUBNORMAL,
    ///             bf16::NEG_ZERO,
    ///             bf16::ZERO,
    ///             bf16::MAX_SUBNORMAL,
    ///             bf16::MIN_POSITIVE,
    ///             bf16::ONE,
    ///             bf16::INFINITY,
    ///             bf16::NAN
    ///         ]
    ///         .iter()
    ///     )
    ///     .all(|(a, b)| a.to_bits() == b.to_bits()));
    /// ```
    // Implementation based on: https://doc.rust-lang.org/std/primitive.f32.html#method.total_cmp
    #[inline]
    #[must_use]
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        let mut left = self.to_bits() as i16;
        let mut right = other.to_bits() as i16;
        left ^= (((left >> 15) as u16) >> 1) as i16;
        right ^= (((right >> 15) as u16) >> 1) as i16;
        left.cmp(&right)
    }

    /// Approximate number of [`struct@bf16`] significant digits in base 10
    pub const DIGITS: u32 = 2;
    /// [`struct@bf16`]
    /// [machine epsilon](https://en.wikipedia.org/wiki/Machine_epsilon) value
    ///
    /// This is the difference between 1.0 and the next largest representable
    /// number.
    pub const EPSILON: bf16 = bf16(0x3C00u16);
    /// [`struct@bf16`] positive Infinity (+∞)
    pub const INFINITY: bf16 = bf16(0x7F80u16);
    /// Number of [`struct@bf16`] significant digits in base 2
    pub const MANTISSA_DIGITS: u32 = 8;
    /// Largest finite [`struct@bf16`] value
    pub const MAX: bf16 = bf16(0x7F7F);
    /// Maximum possible [`struct@bf16`] power of 10 exponent
    pub const MAX_10_EXP: i32 = 38;
    /// Maximum possible [`struct@bf16`] power of 2 exponent
    pub const MAX_EXP: i32 = 128;
    /// Smallest finite [`struct@bf16`] value
    pub const MIN: bf16 = bf16(0xFF7F);
    /// Minimum possible normal [`struct@bf16`] power of 10 exponent
    pub const MIN_10_EXP: i32 = -37;
    /// One greater than the minimum possible normal [`struct@bf16`] power of 2
    /// exponent
    pub const MIN_EXP: i32 = -125;
    /// Smallest positive normal [`struct@bf16`] value
    pub const MIN_POSITIVE: bf16 = bf16(0x0080u16);
    /// [`struct@bf16`] Not a Number (NaN)
    pub const NAN: bf16 = bf16(0x7FC0u16);
    /// [`struct@bf16`] negative infinity (-∞).
    pub const NEG_INFINITY: bf16 = bf16(0xFF80u16);
    /// The radix or base of the internal representation of [`struct@bf16`]
    pub const RADIX: u32 = 2;

    /// Minimum positive subnormal [`struct@bf16`] value
    pub const MIN_POSITIVE_SUBNORMAL: bf16 = bf16(0x0001u16);
    /// Maximum subnormal [`struct@bf16`] value
    pub const MAX_SUBNORMAL: bf16 = bf16(0x007Fu16);

    /// [`struct@bf16`] 1
    pub const ONE: bf16 = bf16(0x3F80u16);
    /// [`struct@bf16`] 0
    pub const ZERO: bf16 = bf16(0x0000u16);
    /// [`struct@bf16`] -0
    pub const NEG_ZERO: bf16 = bf16(0x8000u16);
    /// [`struct@bf16`] -1
    pub const NEG_ONE: bf16 = bf16(0xBF80u16);

    /// [`struct@bf16`] Euler's number (ℯ)
    pub const E: bf16 = bf16(0x402Eu16);
    /// [`struct@bf16`] Archimedes' constant (π)
    pub const PI: bf16 = bf16(0x4049u16);
    /// [`struct@bf16`] 1/π
    pub const FRAC_1_PI: bf16 = bf16(0x3EA3u16);
    /// [`struct@bf16`] 1/√2
    pub const FRAC_1_SQRT_2: bf16 = bf16(0x3F35u16);
    /// [`struct@bf16`] 2/π
    pub const FRAC_2_PI: bf16 = bf16(0x3F23u16);
    /// [`struct@bf16`] 2/√π
    pub const FRAC_2_SQRT_PI: bf16 = bf16(0x3F90u16);
    /// [`struct@bf16`] π/2
    pub const FRAC_PI_2: bf16 = bf16(0x3FC9u16);
    /// [`struct@bf16`] π/3
    pub const FRAC_PI_3: bf16 = bf16(0x3F86u16);
    /// [`struct@bf16`] π/4
    pub const FRAC_PI_4: bf16 = bf16(0x3F49u16);
    /// [`struct@bf16`] π/6
    pub const FRAC_PI_6: bf16 = bf16(0x3F06u16);
    /// [`struct@bf16`] π/8
    pub const FRAC_PI_8: bf16 = bf16(0x3EC9u16);
    /// [`struct@bf16`] 𝗅𝗇 10
    pub const LN_10: bf16 = bf16(0x4013u16);
    /// [`struct@bf16`] 𝗅𝗇 2
    pub const LN_2: bf16 = bf16(0x3F31u16);
    /// [`struct@bf16`] 𝗅𝗈𝗀₁₀ℯ
    pub const LOG10_E: bf16 = bf16(0x3EDEu16);
    /// [`struct@bf16`] 𝗅𝗈𝗀₁₀2
    pub const LOG10_2: bf16 = bf16(0x3E9Au16);
    /// [`struct@bf16`] 𝗅𝗈𝗀₂ℯ
    pub const LOG2_E: bf16 = bf16(0x3FB9u16);
    /// [`struct@bf16`] 𝗅𝗈𝗀₂10
    pub const LOG2_10: bf16 = bf16(0x4055u16);
    /// [`struct@bf16`] √2
    pub const SQRT_2: bf16 = bf16(0x3FB5u16);

    /// Sign bit
    pub const SIGN_MASK: u16 = 0x8000;
    // Private helper for comparisons.
    const NOT_SIGN: u16 = !Self::SIGN_MASK;

    /// Exponent mask
    pub const EXP_MASK: u16 = 0x7F80;

    /// Mask for the hidden bit.
    pub const HIDDEN_BIT_MASK: u16 = 0x0080;

    /// Mantissa mask
    pub const MAN_MASK: u16 = 0x007F;

    /// Minimum representable positive value (min subnormal)
    pub const TINY_BITS: u16 = 0x1;

    /// Minimum representable negative value (min negative subnormal)
    pub const NEG_TINY_BITS: u16 = Self::TINY_BITS | Self::SIGN_MASK;
}

macro_rules! from_int_impl {
    ($t:ty, $func:ident) => {
        /// Create from the integral type, as if by an `as` cast.
        #[inline(always)]
        pub const fn $func(value: $t) -> Self {
            Self::from_f32_const(value as f32)
        }
    };
}

impl bf16 {
    from_int_impl!(u8, from_u8);
    from_int_impl!(u16, from_u16);
    from_int_impl!(u32, from_u32);
    from_int_impl!(u64, from_u64);
    from_int_impl!(u128, from_u128);
    from_int_impl!(i8, from_i8);
    from_int_impl!(i16, from_i16);
    from_int_impl!(i32, from_i32);
    from_int_impl!(i64, from_i64);
    from_int_impl!(i128, from_i128);
}

impl From<bf16> for f32 {
    #[inline]
    fn from(x: bf16) -> f32 {
        x.to_f32()
    }
}

impl From<bf16> for f64 {
    #[inline]
    fn from(x: bf16) -> f64 {
        x.to_f64()
    }
}

impl From<i8> for bf16 {
    #[inline]
    fn from(x: i8) -> bf16 {
        // Convert to f32, then to bf16
        bf16::from_f32(f32::from(x))
    }
}

impl From<u8> for bf16 {
    #[inline]
    fn from(x: u8) -> bf16 {
        // Convert to f32, then to f16
        bf16::from_f32(f32::from(x))
    }
}

impl TryFrom<f32> for bf16 {
    type Error = TryFromFloatError;

    #[inline]
    fn try_from(x: f32) -> Result<Self, Self::Error> {
        Self::from_f32_lossless(x).ok_or(TryFromFloatError(()))
    }
}

impl TryFrom<f64> for bf16 {
    type Error = TryFromFloatError;

    #[inline]
    fn try_from(x: f64) -> Result<Self, Self::Error> {
        Self::from_f64_lossless(x).ok_or(TryFromFloatError(()))
    }
}

impl PartialEq for bf16 {
    fn eq(&self, other: &bf16) -> bool {
        eq(*self, *other)
    }
}

impl PartialOrd for bf16 {
    fn partial_cmp(&self, other: &bf16) -> Option<Ordering> {
        if self.is_nan() || other.is_nan() {
            None
        } else {
            let neg = self.0 & Self::SIGN_MASK != 0;
            let other_neg = other.0 & Self::SIGN_MASK != 0;
            match (neg, other_neg) {
                (false, false) => Some(self.0.cmp(&other.0)),
                (false, true) => {
                    if (self.0 | other.0) & Self::NOT_SIGN == 0 {
                        Some(Ordering::Equal)
                    } else {
                        Some(Ordering::Greater)
                    }
                },
                (true, false) => {
                    if (self.0 | other.0) & Self::NOT_SIGN == 0 {
                        Some(Ordering::Equal)
                    } else {
                        Some(Ordering::Less)
                    }
                },
                (true, true) => Some(other.0.cmp(&self.0)),
            }
        }
    }

    fn lt(&self, other: &bf16) -> bool {
        lt(*self, *other)
    }

    fn le(&self, other: &bf16) -> bool {
        le(*self, *other)
    }

    fn gt(&self, other: &bf16) -> bool {
        gt(*self, *other)
    }

    fn ge(&self, other: &bf16) -> bool {
        ge(*self, *other)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl FromStr for bf16 {
    type Err = ParseFloatError;

    #[inline]
    fn from_str(src: &str) -> Result<bf16, ParseFloatError> {
        f32::from_str(src).map(bf16::from_f32)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl Debug for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Debug::fmt(&self.to_f32(), f)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl Display for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Display::fmt(&self.to_f32(), f)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl LowerExp for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:e}", self.to_f32())
    }
}

#[cfg(not(target_arch = "spirv"))]
impl UpperExp for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:E}", self.to_f32())
    }
}

#[cfg(not(target_arch = "spirv"))]
impl Binary for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:b}", self.0)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl Octal for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:o}", self.0)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl LowerHex for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:x}", self.0)
    }
}

#[cfg(not(target_arch = "spirv"))]
impl UpperHex for bf16 {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:X}", self.0)
    }
}

impl Neg for bf16 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self(self.0 ^ Self::SIGN_MASK)
    }
}

impl Neg for &bf16 {
    type Output = <bf16 as Neg>::Output;

    #[inline]
    fn neg(self) -> Self::Output {
        Neg::neg(*self)
    }
}

impl Add for bf16 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self::from_f32(Self::to_f32(self) + Self::to_f32(rhs))
    }
}

impl Add<&bf16> for bf16 {
    type Output = <bf16 as Add<bf16>>::Output;

    #[inline]
    fn add(self, rhs: &bf16) -> Self::Output {
        self.add(*rhs)
    }
}

impl Add<&bf16> for &bf16 {
    type Output = <bf16 as Add<bf16>>::Output;

    #[inline]
    fn add(self, rhs: &bf16) -> Self::Output {
        (*self).add(*rhs)
    }
}

impl Add<bf16> for &bf16 {
    type Output = <bf16 as Add<bf16>>::Output;

    #[inline]
    fn add(self, rhs: bf16) -> Self::Output {
        (*self).add(rhs)
    }
}

impl AddAssign for bf16 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = (*self).add(rhs);
    }
}

impl AddAssign<&bf16> for bf16 {
    #[inline]
    fn add_assign(&mut self, rhs: &bf16) {
        *self = (*self).add(rhs);
    }
}

impl Sub for bf16 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::from_f32(Self::to_f32(self) - Self::to_f32(rhs))
    }
}

impl Sub<&bf16> for bf16 {
    type Output = <bf16 as Sub<bf16>>::Output;

    #[inline]
    fn sub(self, rhs: &bf16) -> Self::Output {
        self.sub(*rhs)
    }
}

impl Sub<&bf16> for &bf16 {
    type Output = <bf16 as Sub<bf16>>::Output;

    #[inline]
    fn sub(self, rhs: &bf16) -> Self::Output {
        (*self).sub(*rhs)
    }
}

impl Sub<bf16> for &bf16 {
    type Output = <bf16 as Sub<bf16>>::Output;

    #[inline]
    fn sub(self, rhs: bf16) -> Self::Output {
        (*self).sub(rhs)
    }
}

impl SubAssign for bf16 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = (*self).sub(rhs);
    }
}

impl SubAssign<&bf16> for bf16 {
    #[inline]
    fn sub_assign(&mut self, rhs: &bf16) {
        *self = (*self).sub(rhs);
    }
}

impl Mul for bf16 {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self::from_f32(Self::to_f32(self) * Self::to_f32(rhs))
    }
}

impl Mul<&bf16> for bf16 {
    type Output = <bf16 as Mul<bf16>>::Output;

    #[inline]
    fn mul(self, rhs: &bf16) -> Self::Output {
        self.mul(*rhs)
    }
}

impl Mul<&bf16> for &bf16 {
    type Output = <bf16 as Mul<bf16>>::Output;

    #[inline]
    fn mul(self, rhs: &bf16) -> Self::Output {
        (*self).mul(*rhs)
    }
}

impl Mul<bf16> for &bf16 {
    type Output = <bf16 as Mul<bf16>>::Output;

    #[inline]
    fn mul(self, rhs: bf16) -> Self::Output {
        (*self).mul(rhs)
    }
}

impl MulAssign for bf16 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = (*self).mul(rhs);
    }
}

impl MulAssign<&bf16> for bf16 {
    #[inline]
    fn mul_assign(&mut self, rhs: &bf16) {
        *self = (*self).mul(rhs);
    }
}

impl Div for bf16 {
    type Output = Self;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        Self::from_f32(Self::to_f32(self) / Self::to_f32(rhs))
    }
}

impl Div<&bf16> for bf16 {
    type Output = <bf16 as Div<bf16>>::Output;

    #[inline]
    fn div(self, rhs: &bf16) -> Self::Output {
        self.div(*rhs)
    }
}

impl Div<&bf16> for &bf16 {
    type Output = <bf16 as Div<bf16>>::Output;

    #[inline]
    fn div(self, rhs: &bf16) -> Self::Output {
        (*self).div(*rhs)
    }
}

impl Div<bf16> for &bf16 {
    type Output = <bf16 as Div<bf16>>::Output;

    #[inline]
    fn div(self, rhs: bf16) -> Self::Output {
        (*self).div(rhs)
    }
}

impl DivAssign for bf16 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = (*self).div(rhs);
    }
}

impl DivAssign<&bf16> for bf16 {
    #[inline]
    fn div_assign(&mut self, rhs: &bf16) {
        *self = (*self).div(rhs);
    }
}

impl Rem for bf16 {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self::from_f32(Self::to_f32(self) % Self::to_f32(rhs))
    }
}

impl Rem<&bf16> for bf16 {
    type Output = <bf16 as Rem<bf16>>::Output;

    #[inline]
    fn rem(self, rhs: &bf16) -> Self::Output {
        self.rem(*rhs)
    }
}

impl Rem<&bf16> for &bf16 {
    type Output = <bf16 as Rem<bf16>>::Output;

    #[inline]
    fn rem(self, rhs: &bf16) -> Self::Output {
        (*self).rem(*rhs)
    }
}

impl Rem<bf16> for &bf16 {
    type Output = <bf16 as Rem<bf16>>::Output;

    #[inline]
    fn rem(self, rhs: bf16) -> Self::Output {
        (*self).rem(rhs)
    }
}

impl RemAssign for bf16 {
    #[inline]
    fn rem_assign(&mut self, rhs: Self) {
        *self = (*self).rem(rhs);
    }
}

impl RemAssign<&bf16> for bf16 {
    #[inline]
    fn rem_assign(&mut self, rhs: &bf16) {
        *self = (*self).rem(rhs);
    }
}

impl Product for bf16 {
    #[inline]
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        bf16::from_f32(iter.map(|f| f.to_f32()).product())
    }
}

impl<'a> Product<&'a bf16> for bf16 {
    #[inline]
    fn product<I: Iterator<Item = &'a bf16>>(iter: I) -> Self {
        bf16::from_f32(iter.map(|f| f.to_f32()).product())
    }
}

impl Sum for bf16 {
    #[inline]
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        bf16::from_f32(iter.map(|f| f.to_f32()).sum())
    }
}

impl<'a> Sum<&'a bf16> for bf16 {
    #[inline]
    fn sum<I: Iterator<Item = &'a bf16>>(iter: I) -> Self {
        bf16::from_f32(iter.map(|f| f.to_f32()).sum())
    }
}

#[inline]
const fn eq(lhs: bf16, rhs: bf16) -> bool {
    if lhs.is_nan() || rhs.is_nan() {
        false
    } else {
        (lhs.0 == rhs.0) || ((lhs.0 | rhs.0) & bf16::NOT_SIGN == 0)
    }
}

#[inline]
const fn lt(lhs: bf16, rhs: bf16) -> bool {
    if lhs.is_nan() || rhs.is_nan() {
        false
    } else {
        let neg = lhs.0 & bf16::SIGN_MASK != 0;
        let rhs_neg = rhs.0 & bf16::SIGN_MASK != 0;
        match (neg, rhs_neg) {
            (false, false) => lhs.0 < rhs.0,
            (false, true) => false,
            (true, false) => (lhs.0 | rhs.0) & bf16::NOT_SIGN != 0,
            (true, true) => lhs.0 > rhs.0,
        }
    }
}

#[inline]
const fn le(lhs: bf16, rhs: bf16) -> bool {
    if lhs.is_nan() || rhs.is_nan() {
        false
    } else {
        let neg = lhs.0 & bf16::SIGN_MASK != 0;
        let rhs_neg = rhs.0 & bf16::SIGN_MASK != 0;
        match (neg, rhs_neg) {
            (false, false) => lhs.0 <= rhs.0,
            (false, true) => (lhs.0 | rhs.0) & bf16::NOT_SIGN == 0,
            (true, false) => true,
            (true, true) => lhs.0 >= rhs.0,
        }
    }
}

#[inline]
const fn gt(lhs: bf16, rhs: bf16) -> bool {
    if lhs.is_nan() || rhs.is_nan() {
        false
    } else {
        let neg = lhs.0 & bf16::SIGN_MASK != 0;
        let rhs_neg = rhs.0 & bf16::SIGN_MASK != 0;
        match (neg, rhs_neg) {
            (false, false) => lhs.0 > rhs.0,
            (false, true) => (lhs.0 | rhs.0) & bf16::NOT_SIGN != 0,
            (true, false) => false,
            (true, true) => lhs.0 < rhs.0,
        }
    }
}

#[inline]
const fn ge(lhs: bf16, rhs: bf16) -> bool {
    if lhs.is_nan() || rhs.is_nan() {
        false
    } else {
        let neg = lhs.0 & bf16::SIGN_MASK != 0;
        let rhs_neg = rhs.0 & bf16::SIGN_MASK != 0;
        match (neg, rhs_neg) {
            (false, false) => lhs.0 >= rhs.0,
            (false, true) => true,
            (true, false) => (lhs.0 | rhs.0) & bf16::NOT_SIGN == 0,
            (true, true) => lhs.0 <= rhs.0,
        }
    }
}

#[allow(clippy::cognitive_complexity, clippy::float_cmp, clippy::neg_cmp_op_on_partial_ord)]
#[cfg(test)]
mod test {
    use core::cmp::Ordering;

    use super::*;

    #[test]
    fn test_bf16_consts_from_f32() {
        let one = bf16::from_f32(1.0);
        let zero = bf16::from_f32(0.0);
        let neg_zero = bf16::from_f32(-0.0);
        let neg_one = bf16::from_f32(-1.0);
        let inf = bf16::from_f32(core::f32::INFINITY);
        let neg_inf = bf16::from_f32(core::f32::NEG_INFINITY);
        let nan = bf16::from_f32(core::f32::NAN);

        assert_eq!(bf16::ONE, one);
        assert_eq!(bf16::ZERO, zero);
        assert!(zero.is_sign_positive());
        assert_eq!(bf16::NEG_ZERO, neg_zero);
        assert!(neg_zero.is_sign_negative());
        assert_eq!(bf16::NEG_ONE, neg_one);
        assert!(neg_one.is_sign_negative());
        assert_eq!(bf16::INFINITY, inf);
        assert_eq!(bf16::NEG_INFINITY, neg_inf);
        assert!(nan.is_nan());
        assert!(bf16::NAN.is_nan());

        let e = bf16::from_f32(core::f32::consts::E);
        let pi = bf16::from_f32(core::f32::consts::PI);
        let frac_1_pi = bf16::from_f32(core::f32::consts::FRAC_1_PI);
        let frac_1_sqrt_2 = bf16::from_f32(core::f32::consts::FRAC_1_SQRT_2);
        let frac_2_pi = bf16::from_f32(core::f32::consts::FRAC_2_PI);
        let frac_2_sqrt_pi = bf16::from_f32(core::f32::consts::FRAC_2_SQRT_PI);
        let frac_pi_2 = bf16::from_f32(core::f32::consts::FRAC_PI_2);
        let frac_pi_3 = bf16::from_f32(core::f32::consts::FRAC_PI_3);
        let frac_pi_4 = bf16::from_f32(core::f32::consts::FRAC_PI_4);
        let frac_pi_6 = bf16::from_f32(core::f32::consts::FRAC_PI_6);
        let frac_pi_8 = bf16::from_f32(core::f32::consts::FRAC_PI_8);
        let ln_10 = bf16::from_f32(core::f32::consts::LN_10);
        let ln_2 = bf16::from_f32(core::f32::consts::LN_2);
        let log10_e = bf16::from_f32(core::f32::consts::LOG10_E);
        // core::f32::consts::LOG10_2 requires rustc 1.43.0
        let log10_2 = bf16::from_f32(2f32.log10());
        let log2_e = bf16::from_f32(core::f32::consts::LOG2_E);
        // core::f32::consts::LOG2_10 requires rustc 1.43.0
        let log2_10 = bf16::from_f32(10f32.log2());
        let sqrt_2 = bf16::from_f32(core::f32::consts::SQRT_2);

        assert_eq!(bf16::E, e);
        assert_eq!(bf16::PI, pi);
        assert_eq!(bf16::FRAC_1_PI, frac_1_pi);
        assert_eq!(bf16::FRAC_1_SQRT_2, frac_1_sqrt_2);
        assert_eq!(bf16::FRAC_2_PI, frac_2_pi);
        assert_eq!(bf16::FRAC_2_SQRT_PI, frac_2_sqrt_pi);
        assert_eq!(bf16::FRAC_PI_2, frac_pi_2);
        assert_eq!(bf16::FRAC_PI_3, frac_pi_3);
        assert_eq!(bf16::FRAC_PI_4, frac_pi_4);
        assert_eq!(bf16::FRAC_PI_6, frac_pi_6);
        assert_eq!(bf16::FRAC_PI_8, frac_pi_8);
        assert_eq!(bf16::LN_10, ln_10);
        assert_eq!(bf16::LN_2, ln_2);
        assert_eq!(bf16::LOG10_E, log10_e);
        assert_eq!(bf16::LOG10_2, log10_2);
        assert_eq!(bf16::LOG2_E, log2_e);
        assert_eq!(bf16::LOG2_10, log2_10);
        assert_eq!(bf16::SQRT_2, sqrt_2);
    }

    #[test]
    fn test_bf16_consts_from_f64() {
        let one = bf16::from_f64(1.0);
        let zero = bf16::from_f64(0.0);
        let neg_zero = bf16::from_f64(-0.0);
        let inf = bf16::from_f64(core::f64::INFINITY);
        let neg_inf = bf16::from_f64(core::f64::NEG_INFINITY);
        let nan = bf16::from_f64(core::f64::NAN);

        assert_eq!(bf16::ONE, one);
        assert_eq!(bf16::ZERO, zero);
        assert_eq!(bf16::NEG_ZERO, neg_zero);
        assert_eq!(bf16::INFINITY, inf);
        assert_eq!(bf16::NEG_INFINITY, neg_inf);
        assert!(nan.is_nan());
        assert!(bf16::NAN.is_nan());

        let e = bf16::from_f64(core::f64::consts::E);
        let pi = bf16::from_f64(core::f64::consts::PI);
        let frac_1_pi = bf16::from_f64(core::f64::consts::FRAC_1_PI);
        let frac_1_sqrt_2 = bf16::from_f64(core::f64::consts::FRAC_1_SQRT_2);
        let frac_2_pi = bf16::from_f64(core::f64::consts::FRAC_2_PI);
        let frac_2_sqrt_pi = bf16::from_f64(core::f64::consts::FRAC_2_SQRT_PI);
        let frac_pi_2 = bf16::from_f64(core::f64::consts::FRAC_PI_2);
        let frac_pi_3 = bf16::from_f64(core::f64::consts::FRAC_PI_3);
        let frac_pi_4 = bf16::from_f64(core::f64::consts::FRAC_PI_4);
        let frac_pi_6 = bf16::from_f64(core::f64::consts::FRAC_PI_6);
        let frac_pi_8 = bf16::from_f64(core::f64::consts::FRAC_PI_8);
        let ln_10 = bf16::from_f64(core::f64::consts::LN_10);
        let ln_2 = bf16::from_f64(core::f64::consts::LN_2);
        let log10_e = bf16::from_f64(core::f64::consts::LOG10_E);
        // core::f64::consts::LOG10_2 requires rustc 1.43.0
        let log10_2 = bf16::from_f64(2f64.log10());
        let log2_e = bf16::from_f64(core::f64::consts::LOG2_E);
        // core::f64::consts::LOG2_10 requires rustc 1.43.0
        let log2_10 = bf16::from_f64(10f64.log2());
        let sqrt_2 = bf16::from_f64(core::f64::consts::SQRT_2);

        assert_eq!(bf16::E, e);
        assert_eq!(bf16::PI, pi);
        assert_eq!(bf16::FRAC_1_PI, frac_1_pi);
        assert_eq!(bf16::FRAC_1_SQRT_2, frac_1_sqrt_2);
        assert_eq!(bf16::FRAC_2_PI, frac_2_pi);
        assert_eq!(bf16::FRAC_2_SQRT_PI, frac_2_sqrt_pi);
        assert_eq!(bf16::FRAC_PI_2, frac_pi_2);
        assert_eq!(bf16::FRAC_PI_3, frac_pi_3);
        assert_eq!(bf16::FRAC_PI_4, frac_pi_4);
        assert_eq!(bf16::FRAC_PI_6, frac_pi_6);
        assert_eq!(bf16::FRAC_PI_8, frac_pi_8);
        assert_eq!(bf16::LN_10, ln_10);
        assert_eq!(bf16::LN_2, ln_2);
        assert_eq!(bf16::LOG10_E, log10_e);
        assert_eq!(bf16::LOG10_2, log10_2);
        assert_eq!(bf16::LOG2_E, log2_e);
        assert_eq!(bf16::LOG2_10, log2_10);
        assert_eq!(bf16::SQRT_2, sqrt_2);
    }

    #[test]
    fn test_nan_conversion_to_smaller() {
        let nan64 = f64::from_bits(0x7FF0_0000_0000_0001u64);
        let neg_nan64 = f64::from_bits(0xFFF0_0000_0000_0001u64);
        let nan32 = f32::from_bits(0x7F80_0001u32);
        let neg_nan32 = f32::from_bits(0xFF80_0001u32);
        let nan32_from_64 = nan64 as f32;
        let neg_nan32_from_64 = neg_nan64 as f32;
        let nan16_from_64 = bf16::from_f64(nan64);
        let neg_nan16_from_64 = bf16::from_f64(neg_nan64);
        let nan16_from_32 = bf16::from_f32(nan32);
        let neg_nan16_from_32 = bf16::from_f32(neg_nan32);

        assert!(nan64.is_nan() && nan64.is_sign_positive());
        assert!(neg_nan64.is_nan() && neg_nan64.is_sign_negative());
        assert!(nan32.is_nan() && nan32.is_sign_positive());
        assert!(neg_nan32.is_nan() && neg_nan32.is_sign_negative());

        // f32/f64 NaN conversion sign is non-deterministic: https://github.com/starkat99/half-rs/issues/103
        assert!(neg_nan32_from_64.is_nan());
        assert!(nan32_from_64.is_nan());
        assert!(nan16_from_64.is_nan());
        assert!(neg_nan16_from_64.is_nan());
        assert!(nan16_from_32.is_nan());
        assert!(neg_nan16_from_32.is_nan());
    }

    #[test]
    fn test_nan_conversion_to_larger() {
        let nan16 = bf16::from_bits(0x7F81u16);
        let neg_nan16 = bf16::from_bits(0xFF81u16);
        let nan32 = f32::from_bits(0x7F80_0001u32);
        let neg_nan32 = f32::from_bits(0xFF80_0001u32);
        let nan32_from_16 = f32::from(nan16);
        let neg_nan32_from_16 = f32::from(neg_nan16);
        let nan64_from_16 = f64::from(nan16);
        let neg_nan64_from_16 = f64::from(neg_nan16);
        let nan64_from_32 = f64::from(nan32);
        let neg_nan64_from_32 = f64::from(neg_nan32);

        assert!(nan16.is_nan() && nan16.is_sign_positive());
        assert!(neg_nan16.is_nan() && neg_nan16.is_sign_negative());
        assert!(nan32.is_nan() && nan32.is_sign_positive());
        assert!(neg_nan32.is_nan() && neg_nan32.is_sign_negative());

        // // f32/f64 NaN conversion sign is non-deterministic: https://github.com/starkat99/half-rs/issues/103
        assert!(nan32_from_16.is_nan());
        assert!(neg_nan32_from_16.is_nan());
        assert!(nan64_from_16.is_nan());
        assert!(neg_nan64_from_16.is_nan());
        assert!(nan64_from_32.is_nan());
        assert!(neg_nan64_from_32.is_nan());
    }

    #[test]
    fn test_bf16_to_f32() {
        let f = bf16::from_f32(7.0);
        assert_eq!(f.to_f32(), 7.0f32);

        // 7.1 is NOT exactly representable in 16-bit, it's rounded
        let f = bf16::from_f32(7.1);
        let diff = (f.to_f32() - 7.1f32).abs();
        // diff must be <= 4 * EPSILON, as 7 has two more significant bits than 1
        assert!(diff <= 4.0 * bf16::EPSILON.to_f32());

        let tiny32 = f32::from_bits(0x0001_0000u32);
        assert_eq!(bf16::from_bits(0x0001).to_f32(), tiny32);
        assert_eq!(bf16::from_bits(0x0005).to_f32(), 5.0 * tiny32);

        assert_eq!(bf16::from_bits(0x0001), bf16::from_f32(tiny32));
        assert_eq!(bf16::from_bits(0x0005), bf16::from_f32(5.0 * tiny32));
    }

    #[test]
    fn test_bf16_to_f64() {
        let f = bf16::from_f64(7.0);
        assert_eq!(f.to_f64(), 7.0f64);

        // 7.1 is NOT exactly representable in 16-bit, it's rounded
        let f = bf16::from_f64(7.1);
        let diff = (f.to_f64() - 7.1f64).abs();
        // diff must be <= 4 * EPSILON, as 7 has two more significant bits than 1
        assert!(diff <= 4.0 * bf16::EPSILON.to_f64());

        let tiny64 = 2.0f64.powi(-133);
        assert_eq!(bf16::from_bits(0x0001).to_f64(), tiny64);
        assert_eq!(bf16::from_bits(0x0005).to_f64(), 5.0 * tiny64);

        assert_eq!(bf16::from_bits(0x0001), bf16::from_f64(tiny64));
        assert_eq!(bf16::from_bits(0x0005), bf16::from_f64(5.0 * tiny64));
    }

    #[test]
    fn test_comparisons() {
        let zero = bf16::from_f64(0.0);
        let one = bf16::from_f64(1.0);
        let neg_zero = bf16::from_f64(-0.0);
        let neg_one = bf16::from_f64(-1.0);

        assert_eq!(zero.partial_cmp(&neg_zero), Some(Ordering::Equal));
        assert_eq!(neg_zero.partial_cmp(&zero), Some(Ordering::Equal));
        assert!(zero == neg_zero);
        assert!(neg_zero == zero);
        assert!(!(zero != neg_zero));
        assert!(!(neg_zero != zero));
        assert!(!(zero < neg_zero));
        assert!(!(neg_zero < zero));
        assert!(zero <= neg_zero);
        assert!(neg_zero <= zero);
        assert!(!(zero > neg_zero));
        assert!(!(neg_zero > zero));
        assert!(zero >= neg_zero);
        assert!(neg_zero >= zero);

        assert_eq!(one.partial_cmp(&neg_zero), Some(Ordering::Greater));
        assert_eq!(neg_zero.partial_cmp(&one), Some(Ordering::Less));
        assert!(!(one == neg_zero));
        assert!(!(neg_zero == one));
        assert!(one != neg_zero);
        assert!(neg_zero != one);
        assert!(!(one < neg_zero));
        assert!(neg_zero < one);
        assert!(!(one <= neg_zero));
        assert!(neg_zero <= one);
        assert!(one > neg_zero);
        assert!(!(neg_zero > one));
        assert!(one >= neg_zero);
        assert!(!(neg_zero >= one));

        assert_eq!(one.partial_cmp(&neg_one), Some(Ordering::Greater));
        assert_eq!(neg_one.partial_cmp(&one), Some(Ordering::Less));
        assert!(!(one == neg_one));
        assert!(!(neg_one == one));
        assert!(one != neg_one);
        assert!(neg_one != one);
        assert!(!(one < neg_one));
        assert!(neg_one < one);
        assert!(!(one <= neg_one));
        assert!(neg_one <= one);
        assert!(one > neg_one);
        assert!(!(neg_one > one));
        assert!(one >= neg_one);
        assert!(!(neg_one >= one));
    }

    #[test]
    #[allow(clippy::erasing_op, clippy::identity_op)]
    fn round_to_even_f32() {
        // smallest positive subnormal = 0b0.0000_001 * 2^-126 = 2^-133
        let min_sub = bf16::from_bits(1);
        let min_sub_f = (-133f32).exp2();
        assert_eq!(bf16::from_f32(min_sub_f).to_bits(), min_sub.to_bits());
        assert_eq!(f32::from(min_sub).to_bits(), min_sub_f.to_bits());

        // 0.0000000_011111 rounded to 0.0000000 (< tie, no rounding)
        // 0.0000000_100000 rounded to 0.0000000 (tie and even, remains at even)
        // 0.0000000_100001 rounded to 0.0000001 (> tie, rounds up)
        assert_eq!(bf16::from_f32(min_sub_f * 0.49).to_bits(), min_sub.to_bits() * 0);
        assert_eq!(bf16::from_f32(min_sub_f * 0.50).to_bits(), min_sub.to_bits() * 0);
        assert_eq!(bf16::from_f32(min_sub_f * 0.51).to_bits(), min_sub.to_bits() * 1);

        // 0.0000001_011111 rounded to 0.0000001 (< tie, no rounding)
        // 0.0000001_100000 rounded to 0.0000010 (tie and odd, rounds up to even)
        // 0.0000001_100001 rounded to 0.0000010 (> tie, rounds up)
        assert_eq!(bf16::from_f32(min_sub_f * 1.49).to_bits(), min_sub.to_bits() * 1);
        assert_eq!(bf16::from_f32(min_sub_f * 1.50).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f32(min_sub_f * 1.51).to_bits(), min_sub.to_bits() * 2);

        // 0.0000010_011111 rounded to 0.0000010 (< tie, no rounding)
        // 0.0000010_100000 rounded to 0.0000010 (tie and even, remains at even)
        // 0.0000010_100001 rounded to 0.0000011 (> tie, rounds up)
        assert_eq!(bf16::from_f32(min_sub_f * 2.49).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f32(min_sub_f * 2.50).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f32(min_sub_f * 2.51).to_bits(), min_sub.to_bits() * 3);

        assert_eq!(bf16::from_f32(250.49f32).to_bits(), bf16::from_f32(250.0).to_bits());
        assert_eq!(bf16::from_f32(250.50f32).to_bits(), bf16::from_f32(250.0).to_bits());
        assert_eq!(bf16::from_f32(250.51f32).to_bits(), bf16::from_f32(251.0).to_bits());
        assert_eq!(bf16::from_f32(251.49f32).to_bits(), bf16::from_f32(251.0).to_bits());
        assert_eq!(bf16::from_f32(251.50f32).to_bits(), bf16::from_f32(252.0).to_bits());
        assert_eq!(bf16::from_f32(251.51f32).to_bits(), bf16::from_f32(252.0).to_bits());
        assert_eq!(bf16::from_f32(252.49f32).to_bits(), bf16::from_f32(252.0).to_bits());
        assert_eq!(bf16::from_f32(252.50f32).to_bits(), bf16::from_f32(252.0).to_bits());
        assert_eq!(bf16::from_f32(252.51f32).to_bits(), bf16::from_f32(253.0).to_bits());
    }

    #[test]
    #[allow(clippy::erasing_op, clippy::identity_op)]
    fn round_to_even_f64() {
        // smallest positive subnormal = 0b0.0000_001 * 2^-126 = 2^-133
        let min_sub = bf16::from_bits(1);
        let min_sub_f = (-133f64).exp2();
        assert_eq!(bf16::from_f64(min_sub_f).to_bits(), min_sub.to_bits());
        assert_eq!(f64::from(min_sub).to_bits(), min_sub_f.to_bits());

        // 0.0000000_011111 rounded to 0.0000000 (< tie, no rounding)
        // 0.0000000_100000 rounded to 0.0000000 (tie and even, remains at even)
        // 0.0000000_100001 rounded to 0.0000001 (> tie, rounds up)
        assert_eq!(bf16::from_f64(min_sub_f * 0.49).to_bits(), min_sub.to_bits() * 0);
        assert_eq!(bf16::from_f64(min_sub_f * 0.50).to_bits(), min_sub.to_bits() * 0);
        assert_eq!(bf16::from_f64(min_sub_f * 0.51).to_bits(), min_sub.to_bits() * 1);

        // 0.0000001_011111 rounded to 0.0000001 (< tie, no rounding)
        // 0.0000001_100000 rounded to 0.0000010 (tie and odd, rounds up to even)
        // 0.0000001_100001 rounded to 0.0000010 (> tie, rounds up)
        assert_eq!(bf16::from_f64(min_sub_f * 1.49).to_bits(), min_sub.to_bits() * 1);
        assert_eq!(bf16::from_f64(min_sub_f * 1.50).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f64(min_sub_f * 1.51).to_bits(), min_sub.to_bits() * 2);

        // 0.0000010_011111 rounded to 0.0000010 (< tie, no rounding)
        // 0.0000010_100000 rounded to 0.0000010 (tie and even, remains at even)
        // 0.0000010_100001 rounded to 0.0000011 (> tie, rounds up)
        assert_eq!(bf16::from_f64(min_sub_f * 2.49).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f64(min_sub_f * 2.50).to_bits(), min_sub.to_bits() * 2);
        assert_eq!(bf16::from_f64(min_sub_f * 2.51).to_bits(), min_sub.to_bits() * 3);

        assert_eq!(bf16::from_f64(250.49f64).to_bits(), bf16::from_f64(250.0).to_bits());
        assert_eq!(bf16::from_f64(250.50f64).to_bits(), bf16::from_f64(250.0).to_bits());
        assert_eq!(bf16::from_f64(250.51f64).to_bits(), bf16::from_f64(251.0).to_bits());
        assert_eq!(bf16::from_f64(251.49f64).to_bits(), bf16::from_f64(251.0).to_bits());
        assert_eq!(bf16::from_f64(251.50f64).to_bits(), bf16::from_f64(252.0).to_bits());
        assert_eq!(bf16::from_f64(251.51f64).to_bits(), bf16::from_f64(252.0).to_bits());
        assert_eq!(bf16::from_f64(252.49f64).to_bits(), bf16::from_f64(252.0).to_bits());
        assert_eq!(bf16::from_f64(252.50f64).to_bits(), bf16::from_f64(252.0).to_bits());
        assert_eq!(bf16::from_f64(252.51f64).to_bits(), bf16::from_f64(253.0).to_bits());
    }

    #[test]
    fn from_f32_lossless() {
        let from_f32 = |v: f32| bf16::from_f32_lossless(v);
        let roundtrip = |v: f32, expected: Option<bf16>| {
            let half = from_f32(v);
            assert_eq!(half, expected);
            if !expected.is_none() {
                let as_f32 = expected.unwrap().to_f32_const();
                assert_eq!(v, as_f32);
            }
        };

        assert_eq!(from_f32(f32::NAN).map(bf16::is_nan), Some(true));
        roundtrip(f32::INFINITY, Some(bf16::INFINITY));
        roundtrip(f32::NEG_INFINITY, Some(bf16::NEG_INFINITY));
        roundtrip(f32::from_bits(0b0_00000000_00000000000000000000000), Some(bf16(0)));
        roundtrip(
            f32::from_bits(0b1_00000000_00000000000000000000000),
            Some(bf16(bf16::SIGN_MASK)),
        );
        roundtrip(f32::from_bits(1), None);
        roundtrip(f32::from_bits(0b0_00001010_10101001010110100101110), None);
        roundtrip(f32::from_bits(0b0_00001010_10101001010110100101110), None);
        roundtrip(f32::from_bits(0b0_00001010_10101011000000000000000), None);
        roundtrip(
            f32::from_bits(0b0_00001010_10101010000000000000000),
            Some(bf16(0b0_00001010_1010101)),
        );
        roundtrip(f32::from_bits(0b0_00000000_10000000000000000000000), Some(bf16(0x40)));
        // special truncation with denormals, etc.
        roundtrip(f32::from_bits(0b0_00000000_00000001000000000000000), None);
        roundtrip(f32::from_bits(0b0_00000000_00000010000000000000000), Some(bf16(1)));
        roundtrip(f32::from_bits(0b0_00000000_00000100000000000000000), Some(bf16(2)));
        roundtrip(f32::from_bits(0b0_00000000_00000110000000000000000), Some(bf16(3)));
        roundtrip(f32::from_bits(0b0_00000000_00000111000000000000000), None);
        roundtrip(f32::from_bits(0b0_00001011_10100111101101101001001), None);
        // 1.99170198e-35 and has bits until 16 to the end, so truncated 2
        roundtrip(f32::from_bits(0b0_00001011_10100111100000000000000), None);
        // 1.99170198e-35 and has bits until 15 to the end, so truncated 1
        roundtrip(f32::from_bits(0b0_00001011_10100111000000000000000), None);
        // 1.99170198e-35 and has bits until 15 to the end, so truncated 1
        roundtrip(f32::from_bits(0b0_00001011_10100110000000000000000), Some(bf16(0x05d3)));
    }

    #[test]
    fn from_f64_lossless() {
        let from_f64 = |v: f64| bf16::from_f64_lossless(v);
        let roundtrip = |v: f64, expected: Option<bf16>| {
            let half = from_f64(v);
            assert_eq!(half, expected);
            if !expected.is_none() {
                let as_f64 = expected.unwrap().to_f64_const();
                assert_eq!(v, as_f64);
            }
        };

        assert_eq!(from_f64(f64::NAN).map(bf16::is_nan), Some(true));
        roundtrip(f64::INFINITY, Some(bf16::INFINITY));
        roundtrip(f64::NEG_INFINITY, Some(bf16::NEG_INFINITY));
        roundtrip(
            f64::from_bits(0b0_00000000000_0000000000000000000000000000000000000000000000000000),
            Some(bf16(0)),
        );
        roundtrip(
            f64::from_bits(0b1_00000000000_0000000000000000000000000000000000000000000000000000),
            Some(bf16(bf16::SIGN_MASK)),
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010100101011010010110110111111110000111101000001111),
            None,
        );
        // 1.99170198e-35 and has bits until 44 to the end, so truncated 1
        roundtrip(
            f64::from_bits(0b0_01110001010_1010100100000000000000000000000000000000000000000000),
            None,
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010100000000000000000000000000000000000000000000000),
            Some(bf16(0x0554)),
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010101000000000000000000000000000000000000000000000),
            Some(bf16(0x0555)),
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010110000000000000000000000000000000000000000000000),
            Some(bf16(0x0556)),
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010111000000000000000000000000000000000000000000000),
            Some(bf16(0x0557)),
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010101100000000000000000000000000000000000000000000),
            None,
        );
        roundtrip(
            f64::from_bits(0b0_01110001010_1010100110000000000000000000000000000000000000000000),
            None,
        );
        roundtrip(
            f64::from_bits(0b1_01110001010_1010100000000000000000000000000000000000000000000000),
            Some(bf16(0x8554)),
        );
        roundtrip(
            f64::from_bits(0b1_01110001010_1010101000000000000000000000000000000000000000000000),
            Some(bf16(0x8555)),
        );
        // exp out of range but finite
        roundtrip(
            f64::from_bits(0b1_11110001010_1010101000000000000000000000000000000000000000000000),
            None,
        );
        // explicitly check denormals
        roundtrip(
            f64::from_bits(0b0_01101111010_0000000000000000000000000000000000000000000000000000),
            Some(bf16(1)),
        );
        roundtrip(
            f64::from_bits(0b0_01101111011_1000000000000000000000000000000000000000000000000000),
            Some(bf16(3)),
        );
        roundtrip(
            f64::from_bits(0b0_01101111011_1100000000000000000000000000000000000000000000000000),
            None,
        );
        // Due to being denormal, this is truncated out
        roundtrip(
            f64::from_bits(0b0_01101111010_0001000000000000000000000000000000000000000000000000),
            None,
        );
        roundtrip(
            f64::from_bits(0b0_01101111010_1000000000000000000000000000000000000000000000000000),
            None,
        );
    }

    #[test]
    fn test_max() {
        let a = bf16::from_f32(0.0);
        let b = bf16::from_f32(42.0);
        assert_eq!(a.max(b), b);

        let a = bf16::from_f32(42.0);
        let b = bf16::from_f32(0.0);
        assert_eq!(a.max(b), a);

        let a = bf16::NAN;
        let b = bf16::from_f32(42.0);
        assert_eq!(a.max(b), b);

        let a = bf16::from_f32(42.0);
        let b = bf16::NAN;
        assert_eq!(a.max(b), a);

        let a = bf16::NAN;
        let b = bf16::NAN;
        assert!(a.max(b).is_nan());
    }

    #[test]
    fn test_min() {
        let a = bf16::from_f32(0.0);
        let b = bf16::from_f32(42.0);
        assert_eq!(a.min(b), a);

        let a = bf16::from_f32(42.0);
        let b = bf16::from_f32(0.0);
        assert_eq!(a.min(b), b);

        let a = bf16::NAN;
        let b = bf16::from_f32(42.0);
        assert_eq!(a.min(b), b);

        let a = bf16::from_f32(42.0);
        let b = bf16::NAN;
        assert_eq!(a.min(b), a);

        let a = bf16::NAN;
        let b = bf16::NAN;
        assert!(a.min(b).is_nan());
    }
}
