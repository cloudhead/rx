use std::ops::{Div, Mul};

use super::{float::FloatExt as _, Vector2D, Zero};

/// Size.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Size<T = f32> {
    pub w: T,
    pub h: T,
}

impl<T> Size<T> {
    pub fn map<S>(self, f: impl Fn(T) -> S) -> Size<S> {
        Size::new(f(self.w), f(self.h))
    }
}

impl Size<f32> {
    /// Returns a new `Size`,
    /// with `width` and `height` rounded away from zero to the nearest integer,
    /// unless they are already an integer.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Size;
    ///
    /// let size_pos = Size::new(3.3, 3.6).expand();
    /// assert_eq!(size_pos.w, 4.0);
    /// assert_eq!(size_pos.h, 4.0);
    ///
    /// let size_neg = Size::new(-3.3, -3.6).expand();
    /// assert_eq!(size_neg.w, -4.0);
    /// assert_eq!(size_neg.h, -4.0);
    /// ```
    #[inline]
    pub fn expand(self) -> Size {
        Size::new(self.w.expand(), self.h.expand())
    }

    /// Whether this size has zero area.
    ///
    /// Note: a size with negative area is not considered empty.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.area() == 0.0
    }

    /// Returns a new size bounded by `min` and `max.`
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Size;
    ///
    /// let this = Size::new(0., 100.);
    /// let min = Size::new(10., 10.,);
    /// let max = Size::new(50., 50.);
    ///
    /// assert_eq!(this.clamp(min, max), Size::new(10., 50.))
    /// ```
    pub fn clamp(self, min: Size, max: Size) -> Self {
        let width = self.w.max(min.w).min(max.w);
        let height = self.h.max(min.h).min(max.h);

        Size {
            w: width,
            h: height,
        }
    }
}

impl<T: Mul<Output = T>> Size<T> {
    /// The area covered by this size.
    #[inline]
    pub fn area(self) -> T {
        self.w * self.h
    }
}

impl Div<f32> for Size<f32> {
    type Output = Size<f32>;

    fn div(self, rhs: f32) -> Self::Output {
        Size {
            w: self.w / rhs,
            h: self.h / rhs,
        }
    }
}

impl<T: Mul<Output = T> + Copy> Mul<T> for Size<T> {
    type Output = Size<T>;

    fn mul(self, rhs: T) -> Self::Output {
        Size {
            w: self.w * rhs,
            h: self.h * rhs,
        }
    }
}

impl<T> From<(T, T)> for Size<T> {
    fn from((w, h): (T, T)) -> Self {
        Self::new(w, h)
    }
}

impl<T: Copy> From<T> for Size<T> {
    fn from(n: T) -> Self {
        Self::new(n, n)
    }
}

impl<T> From<[T; 2]> for Size<T> {
    fn from([w, h]: [T; 2]) -> Self {
        Self::new(w, h)
    }
}

impl From<Size<u32>> for Size<f32> {
    fn from(other: Size<u32>) -> Self {
        Self::new(other.w as f32, other.h as f32)
    }
}

impl From<Size<usize>> for Size<u32> {
    fn from(other: Size<usize>) -> Self {
        Self::new(other.w as u32, other.h as u32)
    }
}

impl<T> From<Vector2D<T>> for Size<T> {
    fn from(other: Vector2D<T>) -> Self {
        Self::new(other.x, other.y)
    }
}

impl<T: Zero + PartialEq> Zero for Size<T> {
    const ZERO: Self = Size::new(T::ZERO, T::ZERO);

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl<T: Sized> Size<T> {
    pub const fn new(width: T, height: T) -> Self {
        Self {
            w: width,
            h: height,
        }
    }
}

impl From<crate::platform::LogicalSize> for Size {
    fn from(other: crate::platform::LogicalSize) -> Self {
        Size {
            w: other.width as f32,
            h: other.height as f32,
        }
    }
}

impl<T> From<Size<T>> for [T; 2] {
    fn from(size: Size<T>) -> Self {
        [size.w, size.h]
    }
}
