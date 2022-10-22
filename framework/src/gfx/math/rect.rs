use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

use crate::gfx::math;
use crate::gfx::math::{Point2D, Size, Vector2D, Zero};

/// Alias for a visual region.
pub type Region = Rect<f32>;

/// A generic rectangle with bottom left and top right coordinates.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Rect<T> {
    pub origin: Point2D<T>,
    pub size: Size<T>,
}

impl<T> fmt::Display for Rect<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[({}, {}), ({}, {})]",
            self.origin.x, self.origin.y, self.size.w, self.size.h
        )
    }
}

impl<T: Copy> Rect<T> {
    pub fn new(origin: impl Into<Point2D<T>>, size: impl Into<Size<T>>) -> Self {
        Self {
            origin: origin.into(),
            size: size.into(),
        }
    }

    pub fn points(min: impl Into<Point2D<T>>, max: impl Into<Point2D<T>>) -> Self
    where
        T: Sub<Output = T>,
    {
        let min = min.into();
        let max = max.into();

        Self {
            origin: min,
            size: Size::from(max - min),
        }
    }

    pub fn map<S, F>(self, f: F) -> Rect<S>
    where
        F: Fn(T) -> S,
    {
        Rect {
            origin: Point2D::new(f(self.origin.x), f(self.origin.y)),
            size: Size::new(f(self.size.w), f(self.size.h)),
        }
    }

    pub fn expand(&self, x: T, y: T) -> Self
    where
        T: Add<Output = T> + Sub<Output = T>,
    {
        let offset = Vector2D::new(x, y);

        Self {
            origin: self.origin - offset,
            size: Size::new(self.size.w + x + x, self.size.h + y + y),
        }
    }
}

impl<T: Zero> Zero for Rect<T> {
    const ZERO: Self = Self {
        origin: Point2D::ZERO,
        size: Size::ZERO,
    };

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl<T: Copy> Rect<T> {
    pub fn origin(size: impl Into<Size<T>>) -> Self
    where
        T: math::Zero,
    {
        Self::new(Point2D::ORIGIN, size.into())
    }

    pub fn scale(&self, s: T) -> Self
    where
        T: Mul<Output = T> + Copy,
    {
        Self {
            origin: self.origin,
            size: self.size * s,
        }
    }

    /// Return the rectangle with a different origin.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    ///
    /// let r = Rect::new([1, 1], [4, 4]);
    /// assert_eq!(r.with_origin([0, 0]), Rect::new([0, 0], [4, 4]));
    /// ```
    pub fn with_origin(&self, origin: impl Into<Point2D<T>>) -> Self
    where
        T: Add<Output = T> + Sub<Output = T> + Copy,
    {
        Self {
            origin: origin.into(),
            size: self.size,
        }
    }

    /// Return the rectangle with a different size.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    ///
    /// let r = Rect::new([1, 1], [4, 4]);
    /// assert_eq!(r.with_size([9, 9]), Rect::new([1, 1], [9, 9]));
    /// ```
    pub fn with_size(&self, size: impl Into<Size<T>>) -> Self
    where
        T: Add<Output = T> + Sub<Output = T> + Copy,
    {
        Self {
            origin: self.origin,
            size: size.into(),
        }
    }

    /// Return the area of a rectangle.
    pub fn area(&self) -> T
    where
        T: Copy + Sub<Output = T> + std::cmp::PartialOrd + Mul<Output = T>,
    {
        self.width() * self.height()
    }

    pub fn is_empty(&self) -> bool
    where
        T: Zero,
    {
        self.size.is_zero()
    }

    pub fn is_zero(&self) -> bool
    where
        T: math::Zero,
    {
        self.origin.is_zero() && self.size.is_zero()
    }

    /// Return the width of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    ///
    /// let r = Rect::new([0, 0], [3, 3]);
    /// assert_eq!(r.width(), 3);
    /// ```
    pub fn width(&self) -> T {
        self.size.w
    }

    /// Return the height of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    ///
    /// let r: Rect<u64> = Rect::origin([6, 6]);
    /// assert_eq!(r.height(), 6);
    /// ```
    pub fn height(&self) -> T {
        self.size.h
    }

    /// Return the center of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    /// use rx_framework::gfx::Point2D;
    ///
    /// let r = Rect::origin([8, 8]);
    /// assert_eq!(r.center(), Point2D::new(4, 4));
    ///
    /// let r = Rect::new([0, 0], [-8, -8]);
    /// assert_eq!(r.center(), Point2D::new(-4, -4));
    /// ```
    pub fn center(&self) -> Point2D<T>
    where
        T: math::Two + Div<Output = T> + Add<Output = T>,
    {
        Point2D::new(
            self.origin.x + self.width() / T::TWO,
            self.origin.y + self.height() / T::TWO,
        )
    }

    pub fn radius(&self) -> T
    where
        T: math::Two + Div<Output = T> + PartialOrd + Sub<Output = T>,
    {
        let w = self.width();
        let h = self.height();

        if w > h {
            w / T::TWO
        } else {
            h / T::TWO
        }
    }

    /// Check whether the given point is contained in the rectangle.
    ///
    /// ```
    /// use rx_framework::gfx::{Point2D, Rect};
    ///
    /// let r = Rect::origin([6, 6]);
    /// assert!(r.contains([0, 0]));
    /// assert!(r.contains([3, 3]));
    /// assert!(!r.contains([6, 6]));
    ///
    /// let r = Rect::new([-6, -6], [6, 6]);
    /// assert!(r.contains([-3, -3]));
    /// ```
    pub fn contains(&self, point: impl Into<Point2D<T>>) -> bool
    where
        T: PartialOrd + Add<Output = T>,
    {
        let min = self.min();
        let max = self.max();
        let p = point.into();

        p.x >= min.x && p.x < max.x && p.y >= min.y && p.y < max.y
    }

    #[inline]
    pub fn min(&self) -> Point2D<T> {
        self.origin
    }

    #[inline]
    pub fn max(&self) -> Point2D<T>
    where
        T: Add<Output = T>,
    {
        self.origin + self.size
    }

    pub fn intersects(&self, other: Rect<T>) -> bool
    where
        T: PartialOrd + Add<Output = T>,
    {
        self.max().y > other.origin.y
            && self.origin.y < other.max().y
            && self.origin.x < other.max().x
            && self.max().x > other.origin.x
    }

    /// Return the intersection between two rectangles.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx_framework::gfx::Rect;
    ///
    /// let other = Rect::points([0, 0], [3, 3]);
    ///
    /// let r = Rect::points([1, 1], [6, 6]);
    /// assert_eq!(r.intersection(other), Some(Rect::points([1, 1], [3, 3])));
    ///
    /// let r = Rect::points([1, 1], [2, 2]);
    /// assert_eq!(r.intersection(other), Some(Rect::points([1, 1], [2, 2])));
    ///
    /// let r = Rect::points([-1, -1], [3, 3]);
    /// assert_eq!(r.intersection(other), Some(Rect::points([0, 0], [3, 3])));
    ///
    /// let r = Rect::points([-1, -1], [4, 4]);
    /// assert_eq!(r.intersection(other), Some(other));
    ///
    /// let r = Rect::points([4, 4], [5, 5]);
    /// assert_eq!(r.intersection(other), None);
    /// assert_eq!(other.intersection(r), None);
    /// ```
    pub fn intersection(&self, other: Rect<T>) -> Option<Self>
    where
        T: Ord + Add<Output = T> + Sub<Output = T>,
    {
        let x1 = T::max(self.origin.x, other.origin.x);
        let y1 = T::max(self.origin.y, other.origin.y);
        let x2 = T::min(self.max().x, other.max().x);
        let y2 = T::min(self.max().y, other.max().y);

        if x2 < x1 || y2 < y1 {
            None
        } else {
            Some(Rect::points(Point2D::new(x1, y1), Point2D::new(x2, y2)))
        }
    }
}

impl<T> Mul<T> for Rect<T>
where
    T: Mul<Output = T> + Copy,
{
    type Output = Self;

    fn mul(self, s: T) -> Self {
        self.scale(s)
    }
}

impl<T> Add<Vector2D<T>> for Rect<T>
where
    T: Add<Output = T> + Copy,
{
    type Output = Self;

    fn add(self, vec: Vector2D<T>) -> Self {
        Self {
            origin: self.origin + vec,
            size: self.size,
        }
    }
}

impl<T> AddAssign<Vector2D<T>> for Rect<T>
where
    T: Add<Output = T> + Copy,
{
    fn add_assign(&mut self, vec: Vector2D<T>) {
        self.origin = self.origin + vec;
    }
}

impl<T> Sub<Vector2D<T>> for Rect<T>
where
    T: Sub<Output = T> + Copy,
{
    type Output = Self;

    fn sub(self, vec: Vector2D<T>) -> Self {
        Self {
            origin: self.origin - vec,
            size: self.size,
        }
    }
}

impl<T> SubAssign<Vector2D<T>> for Rect<T>
where
    T: Sub<Output = T> + Copy,
{
    fn sub_assign(&mut self, vec: Vector2D<T>) {
        self.origin = self.origin - vec;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Box2D<T> {
    pub min: Point2D<T>,
    pub max: Point2D<T>,
}

impl<T> Box2D<T> {
    pub fn new(min: Point2D<T>, max: Point2D<T>) -> Self {
        Self { min, max }
    }
}

impl<T: Copy + Add<Output = T>> From<Rect<T>> for Box2D<T> {
    fn from(rect: Rect<T>) -> Self {
        Self {
            min: rect.origin,
            max: rect.max(),
        }
    }
}

impl<T: Copy + Add<Output = T>> From<&Rect<T>> for Box2D<T> {
    fn from(rect: &Rect<T>) -> Self {
        Self {
            min: rect.origin,
            max: rect.max(),
        }
    }
}
