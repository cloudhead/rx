use crate::gfx::math;
use crate::gfx::math::{Point2, Vector2};

/// A generic rectangle with bottom left and top right coordinates.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Rect<T> {
    pub x1: T,
    pub y1: T,
    pub x2: T,
    pub y2: T,
}

impl<T> Rect<T> {
    pub const fn new(x1: T, y1: T, x2: T, y2: T) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub fn sized(x1: T, y1: T, w: T, h: T) -> Self
    where
        T: std::ops::Add<Output = T> + Copy,
    {
        Self::new(x1, y1, x1 + w, y1 + h)
    }

    pub fn zero() -> Self
    where
        T: math::Zero,
    {
        Self {
            x1: T::zero(),
            x2: T::zero(),
            y1: T::zero(),
            y2: T::zero(),
        }
    }

    pub fn origin(w: T, h: T) -> Self
    where
        T: math::Zero,
    {
        Self::new(T::zero(), T::zero(), w, h)
    }

    pub fn map<F, S>(self, f: F) -> Rect<S>
    where
        F: Fn(T) -> S,
    {
        Rect {
            x1: f(self.x1),
            x2: f(self.x2),
            y1: f(self.y1),
            y2: f(self.y2),
        }
    }

    pub fn scale(&self, x: T, y: T) -> Self
    where
        T: std::ops::Mul<Output = T> + Copy,
    {
        Self {
            x1: self.x1,
            y1: self.y1,
            x2: self.x2 * x,
            y2: self.y2 * y,
        }
    }

    /// Return the rectangle with a different origin.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::new(1, 1, 4, 4);
    /// assert_eq!(r.with_origin(0, 0), Rect::new(0, 0, 3, 3));
    /// ```
    pub fn with_origin(&self, x: T, y: T) -> Self
    where
        T: std::ops::Add<Output = T> + std::ops::Sub<Output = T> + Copy,
    {
        Self {
            x1: x,
            y1: y,
            x2: x + (self.x2 - self.x1),
            y2: y + (self.y2 - self.y1),
        }
    }

    /// Return the rectangle with a different size.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::new(1, 1, 4, 4);
    /// assert_eq!(r.with_size(9, 9), Rect::new(1, 1, 10, 10));
    /// ```
    pub fn with_size(&self, w: T, h: T) -> Self
    where
        T: std::ops::Add<Output = T> + std::ops::Sub<Output = T> + Copy,
    {
        Self {
            x1: self.x1,
            y1: self.y1,
            x2: self.x1 + w,
            y2: self.y1 + h,
        }
    }

    /// Return an expanded rectangle by a constant amount.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::new(0, 0, 3, 3);
    /// assert_eq!(r.expand(1, 1, 1, 1), Rect::new(-1, -1, 4, 4));
    ///
    /// let r = Rect::new(3, 3, 0, 0);
    /// assert_eq!(r.expand(1, 1, 1, 1), Rect::new(4, 4, -1, -1));
    ///
    /// let r = Rect::new(-1, 1, 1, -1);
    /// assert_eq!(r.expand(4, 4, 4, 4), Rect::new(-5, 5, 5, -5));
    /// ```
    pub fn expand(&self, x1: T, y1: T, x2: T, y2: T) -> Self
    where
        T: std::ops::Add<Output = T> + std::ops::Sub<Output = T> + PartialOrd + Copy,
    {
        let (x1, x2) = if self.x2 > self.x1 {
            (self.x1 - x1, self.x2 + x2)
        } else {
            (self.x1 + x1, self.x2 - x2)
        };
        let (y1, y2) = if self.y2 > self.y1 {
            (self.y1 - y1, self.y2 + y2)
        } else {
            (self.y1 + y1, self.y2 - y2)
        };
        Self { x1, x2, y1, y2 }
    }

    /// Return the rectangle flipped in the Y axis.
    pub fn flip_y(&self) -> Self
    where
        T: Copy,
    {
        Rect::new(self.x1, self.y2, self.x2, self.y1)
    }

    /// Return the rectangle flipped in the X axis.
    pub fn flip_x(&self) -> Self
    where
        T: Copy,
    {
        Rect::new(self.x2, self.y1, self.x1, self.y2)
    }

    /// Return the area of a rectangle.
    pub fn area(&self) -> T
    where
        T: Copy + std::ops::Sub<Output = T> + std::cmp::PartialOrd + std::ops::Mul<Output = T>,
    {
        self.width() * self.height()
    }

    pub fn is_empty(&self) -> bool
    where
        T: PartialEq,
    {
        self.x1 == self.x2 && self.y1 == self.y2
    }

    pub fn is_zero(&self) -> bool
    where
        T: math::Zero,
    {
        self.x1.is_zero() && self.x2.is_zero() && self.y1.is_zero() && self.y2.is_zero()
    }

    /// Return the width of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::new(0, 0, 3, 3);
    /// assert_eq!(r.width(), 3);
    /// ```
    pub fn width(&self) -> T
    where
        T: Copy + PartialOrd + std::ops::Sub<Output = T>,
    {
        if self.x1 < self.x2 {
            self.x2 - self.x1
        } else {
            self.x1 - self.x2
        }
    }

    /// Return the height of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::origin(-6, -6);
    /// assert_eq!(r.height(), 6);
    /// ```
    pub fn height(&self) -> T
    where
        T: Copy + PartialOrd + std::ops::Sub<Output = T>,
    {
        if self.y1 < self.y2 {
            self.y2 - self.y1
        } else {
            self.y1 - self.y2
        }
    }

    /// Return the minimum point of a rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    /// use rx::gfx::math::Point2;
    ///
    /// let r = Rect::new(0, 0, 1, -1);
    /// assert_eq!(r.min(), Point2::new(0, -1));
    /// ```
    pub fn min(&self) -> Point2<T>
    where
        T: PartialOrd + Copy,
    {
        let x = if self.x1 < self.x2 { self.x1 } else { self.x2 };
        let y = if self.y1 < self.y2 { self.y1 } else { self.y2 };

        Point2::new(x, y)
    }

    /// Return the maximum point of a rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    /// use rx::gfx::math::Point2;
    ///
    /// let r = Rect::origin(-1, 1);
    /// assert_eq!(r.max(), Point2::new(0, 1));
    /// ```
    pub fn max(&self) -> Point2<T>
    where
        T: PartialOrd + Copy,
    {
        let x = if self.x1 > self.x2 { self.x1 } else { self.x2 };
        let y = if self.y1 > self.y2 { self.y1 } else { self.y2 };

        Point2::new(x, y)
    }

    /// Return the center of the rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    /// use rx::gfx::math::Point2;
    ///
    /// let r = Rect::origin(8, 8);
    /// assert_eq!(r.center(), Point2::new(4, 4));
    ///
    /// let r = Rect::new(0, 0, -8, -8);
    /// assert_eq!(r.center(), Point2::new(-4, -4));
    /// ```
    pub fn center(&self) -> Point2<T>
    where
        T: std::ops::Div<Output = T>
            + Copy
            + Ord
            + From<i16>
            + PartialOrd
            + math::Zero
            + std::ops::Neg<Output = T>
            + std::ops::Sub<Output = T>,
    {
        let r = self.abs();
        Point2::new(r.x1 + r.width() / 2.into(), r.y1 + r.height() / 2.into())
    }

    pub fn radius(&self) -> T
    where
        T: std::ops::Div<Output = T>
            + Copy
            + From<i16>
            + PartialOrd
            + math::Zero
            + std::ops::Neg<Output = T>
            + std::ops::Sub<Output = T>,
    {
        let w = self.width();
        let h = self.height();

        if w > h {
            w / 2.into()
        } else {
            h / 2.into()
        }
    }

    /// Check whether the given point is contained in the rectangle.
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    /// use rx::gfx::math::Point2;
    ///
    /// let r = Rect::origin(6, 6);
    /// assert!(r.contains(Point2::new(0, 0)));
    /// assert!(r.contains(Point2::new(3, 3)));
    /// assert!(!r.contains(Point2::new(6, 6)));
    ///
    /// let r = Rect::new(0, 0, -6, -6);
    /// assert!(r.contains(Point2::new(-3, -3)));
    /// ```
    pub fn contains(&self, p: Point2<T>) -> bool
    where
        T: Copy + PartialOrd,
    {
        let min = self.min();
        let max = self.max();
        p.x >= min.x && p.x < max.x && p.y >= min.y && p.y < max.y
    }

    pub fn intersects(&self, other: Rect<T>) -> bool
    where
        T: PartialOrd,
    {
        self.y2 > other.y1 && self.y1 < other.y2 && self.x1 < other.x2 && self.x2 > other.x1
    }

    /// Return the absolute rectangle.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let r = Rect::new(3, 3, 1, 1).abs();
    /// assert_eq!(r, Rect::new(1, 1, 3, 3));
    ///
    /// let r = Rect::new(-1, -1, 1, 1).abs();
    /// assert_eq!(r, Rect::new(-1, -1, 1, 1));
    /// ```
    pub fn abs(&self) -> Rect<T>
    where
        T: Ord + Copy,
    {
        Rect::new(
            T::min(self.x1, self.x2),
            T::min(self.y1, self.y2),
            T::max(self.x1, self.x2),
            T::max(self.y1, self.y2),
        )
    }

    /// Return the intersection between two rectangles.
    ///
    /// # Examples
    ///
    /// ```
    /// use rx::gfx::rect::Rect;
    ///
    /// let other = Rect::new(0, 0, 3, 3);
    ///
    /// let r = Rect::new(1, 1, 6, 6);
    /// assert_eq!(r.intersection(other), Rect::new(1, 1, 3, 3));
    ///
    /// let r = Rect::new(1, 1, 2, 2);
    /// assert_eq!(r.intersection(other), Rect::new(1, 1, 2, 2));
    ///
    /// let r = Rect::new(-1, -1, 3, 3);
    /// assert_eq!(r.intersection(other), Rect::new(0, 0, 3, 3));
    ///
    /// let r = Rect::new(-1, -1, 4, 4);
    /// assert_eq!(r.intersection(other), other);
    ///
    /// let r = Rect::new(4, 4, 5, 5);
    /// assert!(r.intersection(other).is_empty());
    /// ```
    pub fn intersection(&self, other: Rect<T>) -> Self
    where
        T: Ord + Copy,
    {
        let x1 = T::max(self.x1, other.x1);
        let y1 = T::max(self.y1, other.y1);
        let x2 = T::min(self.x2, other.x2);
        let y2 = T::min(self.y2, other.y2);

        Rect::new(x1, y1, T::max(x1, x2), T::max(y1, y2))
    }
}

impl<T> std::ops::Add<Vector2<T>> for Rect<T>
where
    T: std::ops::Add<Output = T> + Copy,
{
    type Output = Self;

    fn add(self, vec: Vector2<T>) -> Self {
        Self {
            x1: self.x1 + vec.x,
            y1: self.y1 + vec.y,
            x2: self.x2 + vec.x,
            y2: self.y2 + vec.y,
        }
    }
}

impl<T> std::ops::AddAssign<Vector2<T>> for Rect<T>
where
    T: std::ops::AddAssign<T> + Copy,
{
    fn add_assign(&mut self, vec: Vector2<T>) {
        self.x1 += vec.x;
        self.y1 += vec.y;
        self.x2 += vec.x;
        self.y2 += vec.y;
    }
}

impl<T> std::ops::Sub<Vector2<T>> for Rect<T>
where
    T: std::ops::Sub<Output = T> + Copy,
{
    type Output = Self;

    fn sub(self, vec: Vector2<T>) -> Self {
        Self {
            x1: self.x1 - vec.x,
            y1: self.y1 - vec.y,
            x2: self.x2 - vec.x,
            y2: self.y2 - vec.y,
        }
    }
}

impl<T> std::ops::SubAssign<Vector2<T>> for Rect<T>
where
    T: std::ops::SubAssign<T> + Copy,
{
    fn sub_assign(&mut self, vec: Vector2<T>) {
        self.x1 -= vec.x;
        self.y1 -= vec.y;
        self.x2 -= vec.x;
        self.y2 -= vec.y;
    }
}

impl<T> std::ops::Mul<T> for Rect<T>
where
    T: std::ops::Mul<Output = T> + Copy,
{
    type Output = Self;

    fn mul(self, s: T) -> Self {
        Self {
            x1: self.x1 * s,
            y1: self.y1 * s,
            x2: self.x2 * s,
            y2: self.y2 * s,
        }
    }
}
