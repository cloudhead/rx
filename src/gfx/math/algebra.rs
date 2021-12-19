// Copyright 2013-2014 The CGMath Developers.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Linear algebra types and functions.
//! Most of the code in this module was borrowed from the `cgmath` package.

pub use num_traits::{cast, Float, One, Zero};

/// View origin.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Origin {
    BottomLeft,
    TopLeft,
}

impl Default for Origin {
    fn default() -> Self {
        Self::TopLeft
    }
}

/// 2D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector2<S> {
    pub x: S,
    pub y: S,
}

impl<S: Sized> Vector2<S> {
    #[inline]
    pub const fn new(x: S, y: S) -> Self {
        Vector2 { x, y }
    }

    /// Returns a vector with the same direction and a given magnitude.
    #[inline]
    pub fn normalize(self) -> Self
    where
        S: One + Float + std::ops::Div + std::ops::Mul,
    {
        self * (S::one() / self.magnitude())
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
    /// use rx::gfx::math::*;
    ///
    /// let v1 = Vector4::new(1, 3, -5, 4);
    /// let v2 = Vector4::new(4, -2, -1, 3);
    ///
    /// assert_eq!(v1 * v2, 15);
    /// ```
    #[inline]
    pub fn dot(a: Self, b: Self) -> <S as std::ops::Add>::Output
    where
        S: std::ops::Mul<Output = S> + std::ops::Add,
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
    pub fn extend(self, z: S) -> Vector3<S> {
        Vector3::new(self.x, self.y, z)
    }

    pub fn map<F, T>(self, mut f: F) -> Vector2<T>
    where
        F: FnMut(S) -> T,
    {
        Vector2::new(f(self.x), f(self.y))
    }
}

impl<S: Zero + Copy + PartialEq> Zero for Vector2<S> {
    #[inline]
    fn zero() -> Self {
        Vector2::new(S::zero(), S::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        *self == Vector2::zero()
    }
}

impl<T: Copy> From<[T; 2]> for Vector2<T> {
    #[inline]
    fn from(array: [T; 2]) -> Self {
        Vector2::new(array[0], array[1])
    }
}

impl<S> std::ops::Add<Vector2<S>> for Vector2<S>
where
    S: std::ops::Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector2<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl<S> std::ops::Sub<Vector2<S>> for Vector2<S>
where
    S: std::ops::Sub<Output = S> + Copy,
{
    type Output = Self;

    fn sub(self, other: Vector2<S>) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<S> std::ops::Mul<S> for Vector2<S>
where
    S: std::ops::Mul<Output = S> + Copy,
{
    type Output = Self;

    fn mul(self, s: S) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}

impl<S> From<Point2<S>> for Vector2<S> {
    fn from(p: Point2<S>) -> Self {
        Self::new(p.x, p.y)
    }
}

/// 3D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector3<S> {
    pub x: S,
    pub y: S,
    pub z: S,
}

impl<S> Vector3<S> {
    #[inline]
    pub const fn new(x: S, y: S, z: S) -> Self {
        Vector3 { x, y, z }
    }

    /// Extend vector to four dimensions.
    pub fn extend(self, w: S) -> Vector4<S> {
        Vector4::new(self.x, self.y, self.z, w)
    }
}

impl<T: Copy> From<[T; 3]> for Vector3<T> {
    #[inline]
    fn from(array: [T; 3]) -> Self {
        Vector3::new(array[0], array[1], array[2])
    }
}

/// 4D vector.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Vector4<S> {
    pub x: S,
    pub y: S,
    pub z: S,
    pub w: S,
}

impl From<Vector4<f32>> for [f32; 4] {
    fn from(mat: Vector4<f32>) -> Self {
        unsafe { std::mem::transmute(mat) }
    }
}

impl<T: Copy> From<[T; 4]> for Vector4<T> {
    #[inline]
    fn from(array: [T; 4]) -> Self {
        Vector4::new(array[0], array[1], array[2], array[3])
    }
}

impl<S> Vector4<S> {
    #[inline]
    pub const fn new(x: S, y: S, z: S, w: S) -> Self {
        Vector4 { x, y, z, w }
    }
}

impl<S> std::ops::Mul<S> for Vector4<S>
where
    S: std::ops::Mul<Output = S> + Copy,
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

impl<S> std::ops::Mul<Vector4<S>> for Vector4<S>
where
    S: std::ops::Mul<Output = S> + std::ops::Add<Output = S> + Copy,
{
    type Output = S;

    fn mul(self, other: Vector4<S>) -> S {
        other.x * self.x + other.y * self.y + other.z * self.z + other.w * self.w
    }
}

impl<S> std::ops::Add<Vector4<S>> for Vector4<S>
where
    S: std::ops::Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector4<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
            w: self.w + other.w,
        }
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Point2<S> {
    pub x: S,
    pub y: S,
}

impl<S> Point2<S> {
    pub const fn new(x: S, y: S) -> Self {
        Point2 { x, y }
    }

    pub fn map<F, T>(self, mut f: F) -> Point2<T>
    where
        F: FnMut(S) -> T,
    {
        Point2::new(f(self.x), f(self.y))
    }
}

impl<T: Copy> From<[T; 2]> for Point2<T> {
    #[inline]
    fn from(array: [T; 2]) -> Self {
        Point2::new(array[0], array[1])
    }
}

impl<S> From<Vector2<S>> for Point2<S> {
    fn from(v: Vector2<S>) -> Self {
        Self::new(v.x, v.y)
    }
}

impl<S> std::ops::Div<S> for Point2<S>
where
    S: std::ops::Div<Output = S> + Copy,
{
    type Output = Self;

    fn div(self, s: S) -> Self {
        Self {
            x: self.x / s,
            y: self.y / s,
        }
    }
}

impl<S> std::ops::Mul<S> for Point2<S>
where
    S: std::ops::Mul<Output = S> + Copy,
{
    type Output = Self;

    fn mul(self, s: S) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}

impl<S> std::ops::Add<Vector2<S>> for Point2<S>
where
    S: std::ops::Add<Output = S> + Copy,
{
    type Output = Self;

    fn add(self, other: Vector2<S>) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl<S> std::ops::Sub<Vector2<S>> for Point2<S>
where
    S: std::ops::Sub<Output = S> + Copy,
{
    type Output = Self;

    fn sub(self, other: Vector2<S>) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<S> std::ops::Sub<Point2<S>> for Point2<S>
where
    S: std::ops::Sub<Output = S> + Copy,
{
    type Output = Vector2<S>;

    fn sub(self, other: Point2<S>) -> Vector2<S> {
        Vector2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

/// A 4 x 4, column major matrix
///
/// This type is marked as `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Matrix4<S> {
    /// The first column of the matrix.
    pub x: Vector4<S>,
    /// The second column of the matrix.
    pub y: Vector4<S>,
    /// The third column of the matrix.
    pub z: Vector4<S>,
    /// The fourth column of the matrix.
    pub w: Vector4<S>,
}

impl From<Matrix4<f32>> for [[f32; 4]; 4] {
    fn from(mat: Matrix4<f32>) -> Self {
        unsafe { std::mem::transmute(mat) }
    }
}

impl<S: Copy + Zero + One> Matrix4<S> {
    /// Create a new matrix, providing values for each index.
    #[inline]
    #[rustfmt::skip]
    pub fn new(
        c0r0: S, c0r1: S, c0r2: S, c0r3: S,
        c1r0: S, c1r1: S, c1r2: S, c1r3: S,
        c2r0: S, c2r1: S, c2r2: S, c2r3: S,
        c3r0: S, c3r1: S, c3r2: S, c3r3: S,
    ) -> Self {
        Self {
            x: Vector4::new(c0r0, c0r1, c0r2, c0r3),
            y: Vector4::new(c1r0, c1r1, c1r2, c1r3),
            z: Vector4::new(c2r0, c2r1, c2r2, c2r3),
            w: Vector4::new(c3r0, c3r1, c3r2, c3r3),
        }
    }

    #[inline]
    #[rustfmt::skip]
    pub fn identity() -> Self {
        Matrix4::new(
            S::one(), S::zero(), S::zero(), S::zero(),
            S::zero(), S::one(), S::zero(), S::zero(),
            S::zero(), S::zero(), S::one(), S::zero(),
            S::zero(), S::zero(), S::zero(), S::one(),
        )
    }

    /// Create a homogeneous transformation matrix from a translation vector.
    #[inline]
    #[rustfmt::skip]
    pub fn from_translation(v: Vector3<S>) -> Self {
        Matrix4::new(
            S::one(), S::zero(), S::zero(), S::zero(),
            S::zero(), S::one(), S::zero(), S::zero(),
            S::zero(), S::zero(), S::one(), S::zero(),
            v.x, v.y, v.z, S::one(),
        )
    }

    #[inline]
    pub fn row(&self, n: usize) -> Vector4<S> {
        match n {
            0 => Vector4::new(self.x.x, self.y.x, self.z.x, self.w.x),
            1 => Vector4::new(self.x.y, self.y.y, self.z.y, self.w.y),
            2 => Vector4::new(self.x.z, self.y.z, self.z.z, self.w.z),
            3 => Vector4::new(self.x.w, self.y.w, self.z.w, self.w.w),
            _ => panic!("Matrix4::row: invalid row number: {}", n),
        }
    }

    /// Create a homogeneous transformation matrix from a scale value.
    #[inline]
    pub fn from_scale(value: S) -> Matrix4<S> {
        Matrix4::from_nonuniform_scale(value, value, value)
    }

    /// Create a homogeneous transformation matrix from a set of scale values.
    #[inline]
    #[rustfmt::skip]
    pub fn from_nonuniform_scale(x: S, y: S, z: S) -> Matrix4<S> {
        Matrix4::new(
            x,         S::zero(), S::zero(), S::zero(),
            S::zero(), y,         S::zero(), S::zero(),
            S::zero(), S::zero(), z,         S::zero(),
            S::zero(), S::zero(), S::zero(), S::one(),
        )
    }
}

impl Matrix4<f32> {
    /// Create orthographic matrix.
    pub fn ortho(w: u32, h: u32, origin: Origin) -> Matrix4<f32> {
        let (top, bottom) = match origin {
            Origin::BottomLeft => (h as f32, 0.),
            Origin::TopLeft => (0., h as f32),
        };
        Ortho::<f32> {
            left: 0.0,
            right: w as f32,
            bottom,
            top,
            near: -1.0,
            far: 1.0,
        }
        .into()
    }
}

impl<S> std::ops::Mul<Matrix4<S>> for Matrix4<S>
where
    S: std::ops::Mul<Output = S> + std::ops::Add<Output = S> + Copy,
{
    type Output = Self;

    #[rustfmt::skip]
    fn mul(self, rhs: Matrix4<S>) -> Matrix4<S> {
        let a = self.x;
        let b = self.y;
        let c = self.z;
        let d = self.w;

        Matrix4 {
            x: a * rhs.x.x + b * rhs.x.y + c * rhs.x.z + d * rhs.x.w,
            y: a * rhs.y.x + b * rhs.y.y + c * rhs.y.z + d * rhs.y.w,
            z: a * rhs.z.x + b * rhs.z.y + c * rhs.z.z + d * rhs.z.w,
            w: a * rhs.w.x + b * rhs.w.y + c * rhs.w.z + d * rhs.w.w,
        }
    }
}

/// Transform a [`Vector3`] with a [`Matrix4`].
///
/// ```
/// use rx::gfx::math::*;
/// let m = Matrix4::from_translation(Vector3::new(8., 8., 0.));
/// let v = Vector3::new(1., 1., 0.);
///
/// assert_eq!(m * v, Vector3::new(9., 9., 0.));
/// ```
impl std::ops::Mul<Vector3<f32>> for Matrix4<f32> {
    type Output = Vector3<f32>;

    fn mul(self, vec: Vector3<f32>) -> Vector3<f32> {
        let vec = vec.extend(1.);
        Vector3::new(self.row(0) * vec, self.row(1) * vec, self.row(2) * vec)
    }
}

/// Transform a [`Vector4`] with a [`Matrix4`].
///
/// ```
/// use rx::gfx::math::*;
/// let m = Matrix4::from_translation(Vector3::new(8., 8., 0.));
/// let v = Vector4::new(1., 1., 0., 1.);
///
/// assert_eq!(m * v, Vector4::new(9., 9., 0., 1.));
/// ```
impl std::ops::Mul<Vector4<f32>> for Matrix4<f32> {
    type Output = Vector4<f32>;

    fn mul(self, vec: Vector4<f32>) -> Vector4<f32> {
        Vector4::new(
            self.row(0) * vec,
            self.row(1) * vec,
            self.row(2) * vec,
            self.row(3) * vec,
        )
    }
}

/// Transform a [`Point2`] with a [`Matrix4`].
///
/// ```
/// use rx::gfx::math::*;
/// let m = Matrix4::from_translation(Vector3::new(8., 8., 0.));
/// let p = Point2::new(1., 1.);
///
/// assert_eq!(m * p, Point2::new(9., 9.));
/// ```
impl std::ops::Mul<Point2<f32>> for Matrix4<f32> {
    type Output = Point2<f32>;

    fn mul(self, p: Point2<f32>) -> Point2<f32> {
        let vec = Vector4::new(p.x, p.y, 0., 1.);
        Point2::new(self.row(0) * vec, self.row(1) * vec)
    }
}

/// An orthographic projection with arbitrary left/right/bottom/top distances
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Ortho<S> {
    pub left: S,
    pub right: S,
    pub bottom: S,
    pub top: S,
    pub near: S,
    pub far: S,
}

impl<S: Float> From<Ortho<S>> for Matrix4<S> {
    #[rustfmt::skip]
    fn from(ortho: Ortho<S>) -> Matrix4<S> {
        let two: S = cast(2).unwrap();

        let c0r0 = two / (ortho.right - ortho.left);
        let c0r1 = S::zero();
        let c0r2 = S::zero();
        let c0r3 = S::zero();

        let c1r0 = S::zero();
        let c1r1 = two / (ortho.top - ortho.bottom);
        let c1r2 = S::zero();
        let c1r3 = S::zero();

        let c2r0 = S::zero();
        let c2r1 = S::zero();
        let c2r2 = -two / (ortho.far - ortho.near);
        let c2r3 = S::zero();

        let c3r0 = -(ortho.right + ortho.left) / (ortho.right - ortho.left);
        let c3r1 = -(ortho.top + ortho.bottom) / (ortho.top - ortho.bottom);
        let c3r2 = -(ortho.far + ortho.near) / (ortho.far - ortho.near);
        let c3r3 = S::one();

        Matrix4::new(
            c0r0, c0r1, c0r2, c0r3,
            c1r0, c1r1, c1r2, c1r3,
            c2r0, c2r1, c2r2, c2r3,
            c3r0, c3r1, c3r2, c3r3,
        )
    }
}
