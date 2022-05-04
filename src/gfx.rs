#![allow(clippy::many_single_char_names)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::too_many_arguments)]

pub mod color;
pub mod math;
pub mod rect;
pub mod shape2d;
pub mod sprite2d;

pub use color::{Rgb8, Rgba, Rgba8};
pub use math::{Matrix4, Origin, Ortho, Point2, Vector2, Vector3, Vector4};
pub use rect::Rect;

use std::marker::PhantomData;
use std::ops::{Add, Deref, Sub};

use crate::util;

/// 2D point with type witness.
#[derive(Eq, Debug)]
pub struct Point<S, T> {
    pub point: Point2<T>,
    /// Type witness.
    witness: PhantomData<S>,
}

impl<S, T: Clone> Clone for Point<S, T> {
    fn clone(&self) -> Point<S, T> {
        Self {
            point: self.point.clone(),
            witness: PhantomData,
        }
    }
}

impl<S, T: Copy> Copy for Point<S, T> {}

impl<S, T: PartialEq> PartialEq for Point<S, T> {
    fn eq(&self, other: &Self) -> bool {
        self.point == other.point
    }
}

impl<S, T> Point<S, T> {
    pub fn new(x: T, y: T) -> Self {
        Self {
            point: Point2::new(x, y),
            witness: PhantomData,
        }
    }
}

impl<S> Point<S, f32> {
    pub fn floor(&mut self) -> Self {
        Self {
            point: self.point.map(f32::floor),
            witness: PhantomData,
        }
    }
}

impl<S, T> Deref for Point<S, T> {
    type Target = Point2<T>;

    fn deref(&self) -> &Point2<T> {
        &self.point
    }
}

impl<S> Add<Vector2<f32>> for Point<S, f32> {
    type Output = Self;

    fn add(self, vec: Vector2<f32>) -> Self {
        Self {
            point: self.point + vec,
            witness: self.witness,
        }
    }
}

impl<S> Sub<Vector2<f32>> for Point<S, f32> {
    type Output = Self;

    fn sub(self, vec: Vector2<f32>) -> Self {
        Self {
            point: self.point - vec,
            witness: self.witness,
        }
    }
}

impl<S> Point<S, i32> {
    pub fn clamp(&mut self, rect: Rect<i32>) {
        util::clamp(&mut self.point, rect);
    }
}

impl<S> From<Point<S, f32>> for Point<S, i32> {
    fn from(other: Point<S, f32>) -> Point<S, i32> {
        Point::new(other.x.round() as i32, other.y.round() as i32)
    }
}

impl<S> From<Point<S, i32>> for Point<S, f32> {
    fn from(other: Point<S, i32>) -> Point<S, f32> {
        Point::new(other.x as f32, other.y as f32)
    }
}

impl<S> From<Point<S, f32>> for Point<S, u32> {
    fn from(other: Point<S, f32>) -> Point<S, u32> {
        Point::new(other.x.round() as u32, other.y.round() as u32)
    }
}

impl<S> From<Point2<f32>> for Point<S, f32> {
    fn from(p: Point2<f32>) -> Point<S, f32> {
        Point::new(p.x, p.y)
    }
}

////////////////////////////////////////////////////////////////////////////////

pub trait Geometry {
    fn transform(self, m: Matrix4<f32>) -> Self;
}

impl Geometry for Rect<f32> {
    fn transform(self, m: Matrix4<f32>) -> Self {
        let p1 = m * Vector4::new(self.x1, self.y1, 0., 1.);
        let p2 = m * Vector4::new(self.x2, self.y2, 0., 1.);

        Self::new(p1.x, p1.y, p2.x, p2.y)
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct Repeat {
    pub x: f32,
    pub y: f32,
}

impl Repeat {
    pub fn new(x: f32, y: f32) -> Self {
        Repeat { x, y }
    }
}

impl Default for Repeat {
    fn default() -> Self {
        Repeat { x: 1.0, y: 1.0 }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct ZDepth(pub f32);

impl ZDepth {
    pub const ZERO: Self = ZDepth(0.0);
}

impl From<f32> for ZDepth {
    fn from(other: f32) -> Self {
        ZDepth(other)
    }
}

impl Default for ZDepth {
    fn default() -> Self {
        Self::ZERO
    }
}

impl std::ops::Deref for ZDepth {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
