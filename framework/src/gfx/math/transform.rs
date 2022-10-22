// Copyright 2013-2014 The CGMath Developers
// Copyright 2012-2013 Mozilla Foundation
// Copyright 2021-2022 Alexis Sellier

use std::marker::PhantomData;
use std::ops::{Add, Div, Mul, Sub};

use super::algebra::*;
use super::float::*;
use super::traits::*;
use super::Size;

/// A 2d affine transform.
///
/// Transforms can be parametrized over the source and destination units, to describe a
/// transformation from a space to another.
///
/// For example, `Transform2D<f32, WorldSpace, ScreenSpace>::transform_point4d`
/// takes a `Point2D<f32, WorldSpace>` and returns a `Point2D<f32, ScreenSpace>`.
///
/// The matrix representation is conceptually equivalent to a 3 by 3 matrix transformation
/// compressed to 3 by 2 with the components that aren't needed to describe the set of 2d
/// transformations we are interested in implicitly defined:
///
/// ```text
///  | m11 m12 0 |   |x|   |x'|
///  | m21 m22 0 | x |y| = |y'|
///  | m31 m32 1 |   |1|   |w |
/// ```
///
/// When translating Transform2D into general matrix representations, consider that the
/// representation follows the column-major notation with column vectors.
///
/// The translation terms are m31 and m32.
#[repr(C)]
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Transform2D<Src = (), Dst = ()> {
    pub m11: f32, pub m12: f32,
    pub m21: f32, pub m22: f32,
    pub m31: f32, pub m32: f32,
    #[doc(hidden)]
    pub _unit: PhantomData<(Src, Dst)>,
}

impl<Src, Dst> Transform2D<Src, Dst> {
    /// Identity matrix.
    pub const IDENTITY: Self = Self::identity();

    /// Create a transform specifying its components in using the column-major-column-vector
    /// matrix notation.
    ///
    /// For example, the translation terms m31 and m32 are the last two parameters parameters.
    ///
    /// ```
    /// use rx_framework::gfx::math::Transform2D;
    ///
    /// let tx = 1.0;
    /// let ty = 2.0;
    ///
    /// let translation: Transform2D<(), ()> = Transform2D::new(
    ///   1.0, 0.0,
    ///   0.0, 1.0,
    ///   tx,  ty,
    /// );
    /// ```
    #[rustfmt::skip]
    pub const fn new(m11: f32, m12: f32, m21: f32, m22: f32, m31: f32, m32: f32) -> Self {
        Transform2D {
            m11, m12,
            m21, m22,
            m31, m32,

            _unit: PhantomData,
        }
    }

    /// Create an identity matrix:
    ///
    /// ```text
    /// 1 0
    /// 0 1
    /// 0 0
    /// ```
    #[inline]
    pub const fn identity() -> Self {
        Self::translate(Vector2D::ZERO)
    }

    /// Create a 2d translation transform:
    ///
    /// ```text
    /// 1 0
    /// 0 1
    /// x y
    /// ```
    #[inline]
    pub const fn translate(v: Vector2D<f32>) -> Self {
        Self::new(1., 0., 0., 1., v.x, v.y)
    }

    /// Get the translation component.
    #[inline]
    pub const fn translation(&self) -> Vector2D<f32> {
        Vector2D::new(self.m31, self.m32)
    }

    /// Create a 2d scale transform:
    ///
    /// ```text
    /// s 0
    /// 0 s
    /// 0 0
    /// ```
    #[inline]
    pub const fn scale(s: f32) -> Self {
        Self::new(s, 0., 0., s, 0., 0.)
    }

    /// Computes and returns the determinant of this transform.
    pub fn determinant(&self) -> f32 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    /// Returns whether it is possible to compute the inverse transform.
    #[inline]
    pub fn is_invertible(&self) -> bool {
        self.determinant() != 0.
    }

    /// Returns the inverse transform if possible.
    pub fn inverse(self) -> Transform2D<Dst, Src> {
        let idet = self.determinant().recip();

        Transform2D::new(
            idet * self.m22,
            idet * (0. - self.m12),
            idet * (0. - self.m21),
            idet * self.m11,
            idet * (self.m21 * self.m32 - self.m22 * self.m31),
            idet * (self.m31 * self.m12 - self.m11 * self.m32),
        )
    }
}

impl<Src, Dst, NewDst> Mul<Transform2D<Dst, NewDst>> for Transform2D<Src, Dst> {
    type Output = Transform2D<Src, NewDst>;

    /// Returns the multiplication of the two matrices such that mat's transformation
    /// applies after self's transformation.
    #[must_use]
    fn mul(self, mat: Transform2D<Dst, NewDst>) -> Transform2D<Src, NewDst> {
        Transform2D::new(
            self.m11 * mat.m11 + self.m12 * mat.m21,
            self.m11 * mat.m12 + self.m12 * mat.m22,
            self.m21 * mat.m11 + self.m22 * mat.m21,
            self.m21 * mat.m12 + self.m22 * mat.m22,
            self.m31 * mat.m11 + self.m32 * mat.m21 + mat.m31,
            self.m31 * mat.m12 + self.m32 * mat.m22 + mat.m32,
        )
    }
}

impl Mul<Point2D<f32>> for Transform2D {
    type Output = Point2D<f32>;

    #[inline]
    fn mul(self, point: Point2D<f32>) -> Point2D<f32> {
        Point2D::new(
            point.x * self.m11 + point.y * self.m21 + self.m31,
            point.x * self.m12 + point.y * self.m22 + self.m32,
        )
    }
}

impl Mul<Vector2D<f32>> for Transform2D {
    type Output = Vector2D<f32>;

    #[inline]
    fn mul(self, v: Vector2D<f32>) -> Vector2D<f32> {
        Vector2D::new(
            v.x * self.m11 + v.y * self.m21 + self.m31,
            v.x * self.m12 + v.y * self.m22 + self.m32,
        )
    }
}

/// A 4 x 4, column major matrix
///
/// This type is marked as `#[repr(C)]`.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Transform3D<S = f32, Src = (), Dst = ()> {
    /// The first column of the matrix.
    pub x: Vector4D<S>,
    /// The second column of the matrix.
    pub y: Vector4D<S>,
    /// The third column of the matrix.
    pub z: Vector4D<S>,
    /// The fourth column of the matrix.
    pub w: Vector4D<S>,

    #[doc(hidden)]
    pub _unit: PhantomData<(Src, Dst)>,
}

impl From<Transform3D<f32>> for [[f32; 4]; 4] {
    fn from(mat: Transform3D<f32>) -> Self {
        unsafe { std::mem::transmute(mat) }
    }
}

impl<S: Zero + One, Src, Dst> Transform3D<S, Src, Dst> {
    /// Create a new matrix, providing values for each index.
    #[inline]
    #[rustfmt::skip]
    pub fn new(
        m11: S, m12: S, m13: S, m14: S,
        m21: S, m22: S, m23: S, m24: S,
        m31: S, m32: S, m33: S, m34: S,
        m41: S, m42: S, m43: S, m44: S,
    ) -> Self {
        Self {
            x: Vector4D::new(m11, m12, m13, m14),
            y: Vector4D::new(m21, m22, m23, m24),
            z: Vector4D::new(m31, m32, m33, m34),
            w: Vector4D::new(m41, m42, m43, m44),

            _unit: PhantomData,
        }
    }

    /// Create a transform representing a 2d transformation from the components
    /// of a 2 by 3 matrix transformation.
    ///
    /// Components follow the column-major-column-vector notation (m41 and m42
    /// representating the translation terms).
    ///
    /// ```text
    /// m11  m12   0   0
    /// m21  m22   0   0
    ///   0    0   1   0
    /// m41  m42   0   1
    /// ```
    #[inline]
    #[rustfmt::skip]
    pub fn new_2d(m11: S, m12: S, m21: S, m22: S, m41: S, m42: S) -> Self
    where
        S: Zero + One,
    {
        Self::new(
            m11,     m12,     S::ZERO, S::ZERO,
            m21,     m22,     S::ZERO, S::ZERO,
            S::ZERO, S::ZERO, S::ONE,  S::ZERO,
            m41,     m42,     S::ZERO, S::ONE
       )
    }


    #[inline]
    #[rustfmt::skip]
    pub fn identity() -> Self {
        Transform3D::new(
            S::ONE,  S::ZERO, S::ZERO, S::ZERO,
            S::ZERO, S::ONE,  S::ZERO, S::ZERO,
            S::ZERO, S::ZERO, S::ONE,  S::ZERO,
            S::ZERO, S::ZERO, S::ZERO, S::ONE,
        )
    }

    /// Create a homogeneous transformation matrix from a set of scale values.
    #[inline]
    #[rustfmt::skip]
    pub fn from_nonuniform_scale(x: S, y: S, z: S) -> Transform3D<S, Src, Dst> {
        Transform3D::new(
            x,       S::ZERO, S::ZERO, S::ZERO,
            S::ZERO, y,       S::ZERO, S::ZERO,
            S::ZERO, S::ZERO, z,       S::ZERO,
            S::ZERO, S::ZERO, S::ZERO, S::ONE,
        )
    }
}

impl<S: Copy + Zero + One, Src, Dst> Transform3D<S, Src, Dst> {
    #[inline]
    pub fn row(&self, n: usize) -> Vector4D<S> {
        match n {
            0 => Vector4D::new(self.x.x, self.y.x, self.z.x, self.w.x),
            1 => Vector4D::new(self.x.y, self.y.y, self.z.y, self.w.y),
            2 => Vector4D::new(self.x.z, self.y.z, self.z.z, self.w.z),
            3 => Vector4D::new(self.x.w, self.y.w, self.z.w, self.w.w),
            _ => panic!("Transform3D::row: invalid row number: {}", n),
        }
    }

    #[inline]
    pub fn translation(&self) -> Vector3D<S> {
        Vector3D::new(self.w.x, self.w.y, self.w.z)
    }

    /// Create a homogeneous transformation matrix from a translation vector.
    #[inline]
    #[rustfmt::skip]
    pub fn from_translation(v: impl Into<Vector3D<S>>) -> Transform3D<S, Src, Dst> {
        let v = v.into();

        Transform3D::new(
            S::ONE,  S::ZERO, S::ZERO, S::ZERO,
            S::ZERO, S::ONE,  S::ZERO, S::ZERO,
            S::ZERO, S::ZERO, S::ONE,  S::ZERO,
            v.x,     v.y,     v.z,     S::ONE,
        )
    }

    #[inline]
    pub fn scale(&self) -> Vector3D<S> {
        Vector3D::new(self.x.x, self.y.y, self.z.z)
    }

    /// Create a homogeneous transformation matrix from a scale value.
    #[inline]
    pub fn from_scale(value: S) -> Transform3D<S, Src, Dst> {
        Transform3D::from_nonuniform_scale(value, value, value)
    }
}

impl<Src, Dst> Transform3D<f32, Src, Dst> {
    /// Create orthographic matrix.
    pub fn ortho(size: impl Into<Size<u32>>, origin: Origin) -> Self {
        let size = size.into();
        let (top, bottom) = match origin {
            Origin::BottomLeft => (size.h as f32, 0.),
            Origin::TopLeft => (0., size.h as f32),
        };
        Ortho::<f32> {
            left: 0.0,
            right: size.w as f32,
            bottom,
            top,
            near: -1.0,
            far: 1.0,
        }
        .into()
    }

    #[inline]
    pub fn offset(&self) -> Offset {
        Offset::new(self.w.x, self.w.y)
    }
}

impl<T, Src, Dst> Transform3D<T, Src, Dst>
where
    T: Copy
        + Add<T, Output = T>
        + Sub<T, Output = T>
        + Mul<T, Output = T>
        + Div<T, Output = T>
        + One
        + Zero,
{
    /// Returns whether it is possible to compute the inverse transform.
    #[inline]
    pub fn is_invertible(&self) -> bool {
        self.determinant() != Zero::ZERO
    }

    /// Return the inverse transform.
    ///
    /// Panics if the transform is not invertible.
    #[inline]
    pub fn inverse(&self) -> Transform3D<T, Dst, Src> {
        self.try_inverse()
            .expect("Transform3d::inverse: matrix is not invertible")
    }

    /// Returns the inverse transform if possible.
    #[rustfmt::skip]
    pub fn try_inverse(&self) -> Option<Transform3D<T, Dst, Src>> {
        let det = self.determinant();

        if det == Zero::ZERO {
            return None;
        }

        let m = Transform3D::new(
            self.y.z * self.z.w * self.w.y - self.y.w * self.z.z * self.w.y +
            self.y.w * self.z.y * self.w.z - self.y.y * self.z.w * self.w.z -
            self.y.z * self.z.y * self.w.w + self.y.y * self.z.z * self.w.w,

            self.x.w * self.z.z * self.w.y - self.x.z * self.z.w * self.w.y -
            self.x.w * self.z.y * self.w.z + self.x.y * self.z.w * self.w.z +
            self.x.z * self.z.y * self.w.w - self.x.y * self.z.z * self.w.w,

            self.x.z * self.y.w * self.w.y - self.x.w * self.y.z * self.w.y +
            self.x.w * self.y.y * self.w.z - self.x.y * self.y.w * self.w.z -
            self.x.z * self.y.y * self.w.w + self.x.y * self.y.z * self.w.w,

            self.x.w * self.y.z * self.z.y - self.x.z * self.y.w * self.z.y -
            self.x.w * self.y.y * self.z.z + self.x.y * self.y.w * self.z.z +
            self.x.z * self.y.y * self.z.w - self.x.y * self.y.z * self.z.w,

            self.y.w * self.z.z * self.w.x - self.y.z * self.z.w * self.w.x -
            self.y.w * self.z.x * self.w.z + self.y.x * self.z.w * self.w.z +
            self.y.z * self.z.x * self.w.w - self.y.x * self.z.z * self.w.w,

            self.x.z * self.z.w * self.w.x - self.x.w * self.z.z * self.w.x +
            self.x.w * self.z.x * self.w.z - self.x.x * self.z.w * self.w.z -
            self.x.z * self.z.x * self.w.w + self.x.x * self.z.z * self.w.w,

            self.x.w * self.y.z * self.w.x - self.x.z * self.y.w * self.w.x -
            self.x.w * self.y.x * self.w.z + self.x.x * self.y.w * self.w.z +
            self.x.z * self.y.x * self.w.w - self.x.x * self.y.z * self.w.w,

            self.x.z * self.y.w * self.z.x - self.x.w * self.y.z * self.z.x +
            self.x.w * self.y.x * self.z.z - self.x.x * self.y.w * self.z.z -
            self.x.z * self.y.x * self.z.w + self.x.x * self.y.z * self.z.w,

            self.y.y * self.z.w * self.w.x - self.y.w * self.z.y * self.w.x +
            self.y.w * self.z.x * self.w.y - self.y.x * self.z.w * self.w.y -
            self.y.y * self.z.x * self.w.w + self.y.x * self.z.y * self.w.w,

            self.x.w * self.z.y * self.w.x - self.x.y * self.z.w * self.w.x -
            self.x.w * self.z.x * self.w.y + self.x.x * self.z.w * self.w.y +
            self.x.y * self.z.x * self.w.w - self.x.x * self.z.y * self.w.w,

            self.x.y * self.y.w * self.w.x - self.x.w * self.y.y * self.w.x +
            self.x.w * self.y.x * self.w.y - self.x.x * self.y.w * self.w.y -
            self.x.y * self.y.x * self.w.w + self.x.x * self.y.y * self.w.w,

            self.x.w * self.y.y * self.z.x - self.x.y * self.y.w * self.z.x -
            self.x.w * self.y.x * self.z.y + self.x.x * self.y.w * self.z.y +
            self.x.y * self.y.x * self.z.w - self.x.x * self.y.y * self.z.w,

            self.y.z * self.z.y * self.w.x - self.y.y * self.z.z * self.w.x -
            self.y.z * self.z.x * self.w.y + self.y.x * self.z.z * self.w.y +
            self.y.y * self.z.x * self.w.z - self.y.x * self.z.y * self.w.z,

            self.x.y * self.z.z * self.w.x - self.x.z * self.z.y * self.w.x +
            self.x.z * self.z.x * self.w.y - self.x.x * self.z.z * self.w.y -
            self.x.y * self.z.x * self.w.z + self.x.x * self.z.y * self.w.z,

            self.x.z * self.y.y * self.w.x - self.x.y * self.y.z * self.w.x -
            self.x.z * self.y.x * self.w.y + self.x.x * self.y.z * self.w.y +
            self.x.y * self.y.x * self.w.z - self.x.x * self.y.y * self.w.z,

            self.x.y * self.y.z * self.z.x - self.x.z * self.y.y * self.z.x +
            self.x.z * self.y.x * self.z.y - self.x.x * self.y.z * self.z.y -
            self.x.y * self.y.x * self.z.z + self.x.x * self.y.y * self.z.z
        );

        Some(m * (T::ONE / det))
    }

    /// Compute the determinant of the transform.
    #[rustfmt::skip]
    pub fn determinant(&self) -> T {
        self.x.w * self.y.z * self.z.y * self.w.x -
        self.x.z * self.y.w * self.z.y * self.w.x -
        self.x.w * self.y.y * self.z.z * self.w.x +
        self.x.y * self.y.w * self.z.z * self.w.x +
        self.x.z * self.y.y * self.z.w * self.w.x -
        self.x.y * self.y.z * self.z.w * self.w.x -
        self.x.w * self.y.z * self.z.x * self.w.y +
        self.x.z * self.y.w * self.z.x * self.w.y +
        self.x.w * self.y.x * self.z.z * self.w.y -
        self.x.x * self.y.w * self.z.z * self.w.y -
        self.x.z * self.y.x * self.z.w * self.w.y +
        self.x.x * self.y.z * self.z.w * self.w.y +
        self.x.w * self.y.y * self.z.x * self.w.z -
        self.x.y * self.y.w * self.z.x * self.w.z -
        self.x.w * self.y.x * self.z.y * self.w.z +
        self.x.x * self.y.w * self.z.y * self.w.z +
        self.x.y * self.y.x * self.z.w * self.w.z -
        self.x.x * self.y.y * self.z.w * self.w.z -
        self.x.z * self.y.y * self.z.x * self.w.w +
        self.x.y * self.y.z * self.z.x * self.w.w +
        self.x.z * self.y.x * self.z.y * self.w.w -
        self.x.x * self.y.z * self.z.y * self.w.w -
        self.x.y * self.y.x * self.z.z * self.w.w +
        self.x.x * self.y.y * self.z.z * self.w.w
    }
}

impl<S, Src, Dst, NewDst> Mul<Transform3D<S, Dst, NewDst>> for Transform3D<S, Src, Dst>
where
    S: Mul<Output = S> + Add<Output = S> + Copy,
{
    type Output = Transform3D<S, Src, NewDst>;

    #[rustfmt::skip]
    fn mul(self, rhs: Transform3D<S, Dst, NewDst>) -> Self::Output {
        let a = self.x;
        let b = self.y;
        let c = self.z;
        let d = self.w;

        Transform3D {
            x: a * rhs.x.x + b * rhs.x.y + c * rhs.x.z + d * rhs.x.w,
            y: a * rhs.y.x + b * rhs.y.y + c * rhs.y.z + d * rhs.y.w,
            z: a * rhs.z.x + b * rhs.z.y + c * rhs.z.z + d * rhs.z.w,
            w: a * rhs.w.x + b * rhs.w.y + c * rhs.w.z + d * rhs.w.w,

            _unit: PhantomData,
        }
    }
}

/// Transform a [`Vector2D`] with a [`Transform3D`].
///
/// ```
/// use rx_framework::gfx::math::*;
/// let m = Transform3D::from_translation(Vector3D::new(8., 8., 0.));
/// let v = Vector2D::new(1., 1.);
///
/// assert_eq!(m * v, Vector2D::new(9., 9.));
/// ```
impl Mul<Vector2D<f32>> for Transform3D<f32> {
    type Output = Vector2D<f32>;

    fn mul(self, vec: Vector2D<f32>) -> Vector2D<f32> {
        let vec = Vector4D::from(vec);
        Vector2D::new(self.row(0) * vec, self.row(1) * vec)
    }
}

/// Transform a [`Vector3D`] with a [`Transform3D`].
///
/// ```
/// use rx_framework::gfx::math::*;
/// let m = Transform3D::from_translation(Vector3D::new(8., 8., 0.));
/// let v = Vector3D::new(1., 1., 0.);
///
/// assert_eq!(m * v, Vector3D::new(9., 9., 0.));
/// ```
impl Mul<Vector3D<f32>> for Transform3D<f32> {
    type Output = Vector3D<f32>;

    fn mul(self, vec: Vector3D<f32>) -> Vector3D<f32> {
        let vec = Vector4D::from(vec);
        Vector3D::new(self.row(0) * vec, self.row(1) * vec, self.row(2) * vec)
    }
}

/// Transform a [`Vector4D`] with a [`Transform3D`].
///
/// ```
/// use rx_framework::gfx::math::*;
/// let m = Transform3D::from_translation(Vector3D::new(8., 8., 0.));
/// let v = Vector4D::new(1., 1., 0., 1.);
///
/// assert_eq!(m * v, Vector4D::new(9., 9., 0., 1.));
/// ```
impl Mul<Vector4D<f32>> for Transform3D<f32> {
    type Output = Vector4D<f32>;

    fn mul(self, vec: Vector4D<f32>) -> Vector4D<f32> {
        Vector4D::new(
            self.row(0) * vec,
            self.row(1) * vec,
            self.row(2) * vec,
            self.row(3) * vec,
        )
    }
}

/// Transform a [`Point2D`] with a [`Transform3D`].
///
/// ```
/// use rx_framework::gfx::math::*;
/// let m = Transform3D::from_translation(Vector3D::new(8., 8., 0.));
/// let p = Point2D::new(1., 1.);
///
/// assert_eq!(m * p, Point2D::new(9., 9.));
/// ```
impl Mul<Point2D<f32>> for Transform3D<f32> {
    type Output = Point2D<f32>;

    fn mul(self, p: Point2D<f32>) -> Point2D<f32> {
        let vec = Vector4D::new(p.x, p.y, 0., 1.);
        Point2D::new(self.row(0) * vec, self.row(1) * vec)
    }
}

/// Multiplies all of the transform's component by a scalar and returns the result.
impl<S: Copy + Zero + One + Mul<Output = S>, Src, Dst> Mul<S> for Transform3D<S, Src, Dst> {
    type Output = Self;

    #[rustfmt::skip]
    fn mul(self, x: S) -> Self {
        Transform3D::new(
            self.x.x * x, self.x.y * x, self.x.z * x, self.x.w * x,
            self.y.x * x, self.y.y * x, self.y.z * x, self.y.w * x,
            self.z.x * x, self.z.y * x, self.z.z * x, self.z.w * x,
            self.w.x * x, self.w.y * x, self.w.z * x, self.w.w * x
        )
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

impl<S: Float, Src, Dst> From<Ortho<S>> for Transform3D<S, Src, Dst> {
    #[rustfmt::skip]
    fn from(ortho: Ortho<S>) -> Self {
        let m11 = S::TWO / (ortho.right - ortho.left);
        let m12 = S::ZERO;
        let m13 = S::ZERO;
        let m14 = S::ZERO;

        let m21 = S::ZERO;
        let m22 = S::TWO / (ortho.top - ortho.bottom);
        let m23 = S::ZERO;
        let m24 = S::ZERO;

        let m31 = S::ZERO;
        let m32 = S::ZERO;
        let m33 = -S::TWO / (ortho.far - ortho.near);
        let m34 = S::ZERO;

        let m41 = -(ortho.right + ortho.left) / (ortho.right - ortho.left);
        let m42 = -(ortho.top + ortho.bottom) / (ortho.top - ortho.bottom);
        let m43 = -(ortho.far + ortho.near) / (ortho.far - ortho.near);
        let m44 = S::ONE;

        Transform3D::new(
            m11, m12, m13, m14,
            m21, m22, m23, m24,
            m31, m32, m33, m34,
            m41, m42, m43, m44,
        )
    }
}

impl<S: One + Zero + Copy> From<Point2D<S>> for Transform3D<S> {
    #[inline]
    fn from(other: Point2D<S>) -> Self {
        Transform3D::from_translation(Vector3D::new(other.x, other.y, S::ZERO))
    }
}

impl<S: One + Zero + Copy> From<Vector2D<S>> for Transform3D<S> {
    #[inline]
    fn from(other: Vector2D<S>) -> Self {
        Transform3D::from_translation(Vector3D::new(other.x, other.y, S::ZERO))
    }
}

impl<Src, Dst> From<Transform2D<Src, Dst>> for Transform3D<f32, Src, Dst> {
    /// Create a 3D transform from the current transform
    fn from(m: Transform2D<Src, Dst>) -> Self {
        Transform3D::new_2d(m.m11, m.m12, m.m21, m.m22, m.m31, m.m32)
    }
}
