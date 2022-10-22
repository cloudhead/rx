// Copyright 2013-2014 The CGMath Developers
// Copyright 2012-2013 Mozilla Foundation
// Copyright 2021-2022 Alexis Sellier

//! Linear algebra types and functions.
//! Most of the code in this module was borrowed from the `cgmath` package.

use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

use super::float::Float;
use super::rect::Rect;
use super::size::Size;
use super::traits::*;
use super::transform::Transform2D;

/// View origin.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Default)]
pub enum Origin {
    #[default]
    TopLeft,
    BottomLeft,
}

/// Alias for a transform matrix.
pub type Transform = Transform2D;
/// Alias for a 2D vector.
pub type Vector = Vector2D<f32>;
/// Alias for a 2D offset.
pub type Offset = Vector2D<f32>;
/// Alias for a 2D position.
pub type Point = Point2D<f32>;

/// 2D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector2D<S = f32> {
    pub x: S,
    pub y: S,
    // #[doc(hidden)]
    // pub _unit: PhantomData<Dst>,
}

impl<S: Copy + PartialEq + Zero> Vector2D<S> {
    pub fn zero() -> Self {
        Self::ZERO
    }
}

impl Vector2D<f32> {
    /// Returns the angle between two vectors, in radians.
    pub fn angle(&self, other: &Vector2D<f32>) -> f32 {
        (self.x - other.x).atan2(other.y - self.y)
    }
}

impl<S: Sized> Vector2D<S> {
    pub const fn new(x: S, y: S) -> Self {
        Vector2D { x, y }
    }

    /// Returns a vector with the same direction and a given magnitude.
    #[inline]
    pub fn normalize(self) -> Self
    where
        S: One + Float + Div + Mul,
    {
        self * (S::ONE / self.magnitude())
    }

    /// The distance from the tail to the tip of the vector.
    #[inline]
    pub fn magnitude(self) -> S
    where
        S: Float,
    {
        Float::sqrt(Self::dot(self, self))
    }

    /// Dot product of two vectors.
    ///
    /// ```
    /// use rx_framework::gfx::math::*;
    ///
    /// let v1 = Vector4D::new(1, 3, -5, 4);
    /// let v2 = Vector4D::new(4, -2, -1, 3);
    ///
    /// assert_eq!(v1 * v2, 15);
    /// ```
    #[inline]
    pub fn dot(a: Self, b: Self) -> <S as Add>::Output
    where
        S: Mul<Output = S> + Add,
    {
        a.x * b.x + a.y * b.y
    }

    /// Distance between two vectors.
    #[inline]
    pub fn distance(self, other: Self) -> S
    where
        S: Float,
    {
        (other - self).magnitude()
    }

    /// Extend vector to three dimensions.
    pub fn extend(self, z: S) -> Vector3D<S> {
        Vector3D::new(self.x, self.y, z)
    }

    pub fn map<F, T>(self, mut f: F) -> Vector2D<T>
    where
        F: FnMut(S) -> T,
    {
        Vector2D::new(f(self.x), f(self.y))
    }
}

impl<S: Zero + Copy + PartialEq> Zero for Vector2D<S> {
    const ZERO: Self = Vector2D::new(S::ZERO, S::ZERO);

    #[inline]
    fn is_zero(&self) -> bool {
        self == &Vector2D::ZERO
    }
}

impl<T: Copy> From<[T; 2]> for Vector2D<T> {
    #[inline]
    fn from(array: [T; 2]) -> Self {
        Vector2D::new(array[0], array[1])
    }
}

impl<T: Copy> From<T> for Vector2D<T> {
    #[inline]
    fn from(value: T) -> Self {
        Vector2D::new(value, value)
    }
}

impl<T> From<Vector2D<T>> for [T; 2] {
    fn from(vec: Vector2D<T>) -> Self {
        [vec.x, vec.y]
    }
}

impl<S> From<Vector3D<S>> for Vector2D<S> {
    #[inline]
    fn from(other: Vector3D<S>) -> Self {
        Vector2D::new(other.x, other.y)
    }
}

impl<S> Add<Vector2D<S>> for Vector2D<S>
where
    S: Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector2D<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl<S> Sub<Vector2D<S>> for Vector2D<S>
where
    S: Sub<Output = S> + Copy,
{
    type Output = Self;

    fn sub(self, other: Vector2D<S>) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<S> Mul<S> for Vector2D<S>
where
    S: Mul<Output = S> + Copy,
{
    type Output = Self;

    fn mul(self, s: S) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}

impl<S> Div<S> for Vector2D<S>
where
    S: Div<Output = S> + Copy,
{
    type Output = Self;

    fn div(self, s: S) -> Self {
        Self {
            x: self.x / s,
            y: self.y / s,
        }
    }
}

impl<S> From<Point2D<S>> for Vector2D<S> {
    fn from(p: Point2D<S>) -> Self {
        Self::new(p.x, p.y)
    }
}

impl<S> fmt::Display for Vector2D<S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

/// 3D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector3D<S = f32> {
    pub x: S,
    pub y: S,
    pub z: S,
}

impl<S> Vector3D<S> {
    #[inline]
    pub const fn new(x: S, y: S, z: S) -> Self {
        Vector3D { x, y, z }
    }

    /// Extend vector to four dimensions.
    pub fn extend(self, w: S) -> Vector4D<S> {
        Vector4D::new(self.x, self.y, self.z, w)
    }
}

impl<S: Zero> From<Vector2D<S>> for Vector3D<S> {
    fn from(other: Vector2D<S>) -> Self {
        other.extend(S::ZERO)
    }
}

impl<T: Copy> From<[T; 3]> for Vector3D<T> {
    #[inline]
    fn from(array: [T; 3]) -> Self {
        Vector3D::new(array[0], array[1], array[2])
    }
}

impl<S> Add<Vector3D<S>> for Vector3D<S>
where
    S: Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector3D<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

/// 4D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector4D<S = f32> {
    pub x: S,
    pub y: S,
    pub z: S,
    pub w: S,
}

impl From<Vector4D<f32>> for [f32; 4] {
    fn from(mat: Vector4D<f32>) -> Self {
        unsafe { std::mem::transmute(mat) }
    }
}

impl<T: Copy> From<[T; 4]> for Vector4D<T> {
    #[inline]
    fn from(array: [T; 4]) -> Self {
        Vector4D::new(array[0], array[1], array[2], array[3])
    }
}

impl<S> Vector4D<S> {
    #[inline]
    pub const fn new(x: S, y: S, z: S, w: S) -> Self {
        Vector4D { x, y, z, w }
    }
}

impl<S> Mul<S> for Vector4D<S>
where
    S: Mul<Output = S> + Copy,
{
    type Output = Self;

    fn mul(self, s: S) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
            w: self.w * s,
        }
    }
}

impl<S> Mul<Vector4D<S>> for Vector4D<S>
where
    S: Mul<Output = S> + Add<Output = S> + Copy,
{
    type Output = S;

    fn mul(self, other: Vector4D<S>) -> S {
        other.x * self.x + other.y * self.y + other.z * self.z + other.w * self.w
    }
}

impl<S> Add<Vector4D<S>> for Vector4D<S>
where
    S: Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector4D<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
            w: self.w + other.w,
        }
    }
}

impl<S> Sub<Vector4D<S>> for Vector4D<S>
where
    S: Sub<Output = S> + Copy,
{
    type Output = Self;

    fn sub(self, other: Vector4D<S>) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
            w: self.w - other.w,
        }
    }
}

impl From<Vector3D<f32>> for Vector4D<f32> {
    fn from(other: Vector3D<f32>) -> Self {
        other.extend(1.)
    }
}

impl From<Vector2D<f32>> for Vector4D<f32> {
    fn from(other: Vector2D<f32>) -> Self {
        other.extend(0.).extend(1.)
    }
}

impl From<Point2D<f32>> for Vector4D<f32> {
    fn from(other: Point2D<f32>) -> Self {
        Self::new(other.x, other.y, 0., 1.)
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Default)]
pub struct Point2D<S = f32> {
    pub x: S,
    pub y: S,
}

impl<S> fmt::Display for Point2D<S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl<T: Zero + PartialEq> Point2D<T> {
    pub const ORIGIN: Self = Point2D::ZERO;
}

impl<S> Point2D<S> {
    pub const fn new(x: S, y: S) -> Self {
        Point2D { x, y }
    }

    pub fn map<F, T>(self, mut f: F) -> Point2D<T>
    where
        F: FnMut(S) -> T,
    {
        Point2D::new(f(self.x), f(self.y))
    }
}

impl Point2D<i32> {
    pub fn clamp(&mut self, rect: Rect<i32>) {
        if self.x < rect.min().x {
            self.x = rect.min().x;
        }
        if self.y < rect.min().y {
            self.y = rect.min().y;
        }
        if self.x > rect.max().x {
            self.x = rect.max().x;
        }
        if self.y > rect.max().y {
            self.y = rect.max().y;
        }
    }
}

impl<T: Zero + PartialEq> Zero for Point2D<T> {
    const ZERO: Self = Point2D::new(T::ZERO, T::ZERO);

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl<T: Copy> From<[T; 2]> for Point2D<T> {
    #[inline]
    fn from(array: [T; 2]) -> Self {
        Point2D::new(array[0], array[1])
    }
}

impl<T: Copy> From<(T, T)> for Point2D<T> {
    #[inline]
    fn from((x, y): (T, T)) -> Self {
        Point2D::new(x, y)
    }
}

impl From<Point2D<f64>> for Point2D<f32> {
    #[inline]
    fn from(other: Point2D<f64>) -> Self {
        Point2D::new(other.x as f32, other.y as f32)
    }
}

impl From<Point2D<f32>> for Point2D<i32> {
    #[inline]
    fn from(other: Point2D<f32>) -> Self {
        Point2D::new(other.x as i32, other.y as i32)
    }
}

impl<S> From<Vector2D<S>> for Point2D<S> {
    fn from(v: Vector2D<S>) -> Self {
        Self::new(v.x, v.y)
    }
}

impl<S> Div<S> for Point2D<S>
where
    S: Div<Output = S> + Copy,
{
    type Output = Self;

    fn div(self, s: S) -> Self {
        Self {
            x: self.x / s,
            y: self.y / s,
        }
    }
}

impl<S> Mul<S> for Point2D<S>
where
    S: Mul<Output = S> + Copy,
{
    type Output = Self;

    fn mul(self, s: S) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}

impl<S, T> Add<T> for Point2D<S>
where
    S: Add<Output = S> + Copy,
    T: Into<Vector2D<S>>,
{
    type Output = Self;

    fn add(self, other: T) -> Self {
        let other: Vector2D<S> = other.into();

        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl<S> Sub<Vector2D<S>> for Point2D<S>
where
    S: Sub<Output = S> + Copy,
{
    type Output = Self;

    fn sub(self, other: Vector2D<S>) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<S> Sub<Point2D<S>> for Point2D<S>
where
    S: Sub<Output = S> + Copy,
{
    type Output = Vector2D<S>;

    fn sub(self, other: Point2D<S>) -> Vector2D<S> {
        Vector2D {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<T: Add<Output = T>> Add<Size<T>> for Point2D<T> {
    type Output = Self;

    fn add(self, other: Size<T>) -> Self {
        Self {
            x: self.x + other.w,
            y: self.y + other.h,
        }
    }
}
