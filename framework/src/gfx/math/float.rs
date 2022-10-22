//! Generic trait for floating point numbers

use std::ops::{Add, Div, Mul, Neg, Sub};

use super::{One, Two, Zero};

pub trait Float:
    Copy
    + PartialOrd
    + Zero
    + One
    + Two
    + Neg<Output = Self>
    + Add<Output = Self>
    + Sub<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
{
    fn sqrt(self) -> Self;
}

impl Float for f32 {
    fn sqrt(self) -> Self {
        f32::sqrt(self)
    }
}

impl Float for f64 {
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
}

/// Adds convenience methods to `f32` and `f64`.
pub trait FloatExt<T> {
    /// Rounds to the nearest integer away from zero,
    /// unless the provided value is already an integer.
    ///
    /// It is to `ceil` what `trunc` is to `floor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::math::float::FloatExt;
    ///
    /// let f = 3.7_f64;
    /// let g = 3.0_f64;
    /// let h = -3.7_f64;
    /// let i = -5.1_f32;
    ///
    /// assert_eq!(f.expand(), 4.0);
    /// assert_eq!(g.expand(), 3.0);
    /// assert_eq!(h.expand(), -4.0);
    /// assert_eq!(i.expand(), -6.0);
    /// ```
    fn expand(&self) -> T;
}

impl FloatExt<f64> for f64 {
    #[inline]
    fn expand(&self) -> f64 {
        self.abs().ceil().copysign(*self)
    }
}

impl FloatExt<f32> for f32 {
    #[inline]
    fn expand(&self) -> f32 {
        self.abs().ceil().copysign(*self)
    }
}
