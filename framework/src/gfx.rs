#![allow(clippy::many_single_char_names)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::too_many_arguments)]

pub mod color;
pub mod cursor2d;
pub mod math;
pub mod pixels;
pub mod shape2d;
pub mod sprite2d;

pub mod prelude {
    use super::*;

    pub use super::cursor2d;
    pub use super::{Axis, Geometry, Repeat, ZDepth};
    pub use color::{Color, Image, Rgb8, Rgba, Rgba8};
    pub use math::rect::{Rect, Region};
    pub use math::{
        Offset, Origin, Ortho, Point, Point2D, Size, Transform, Transform3D, Vector, Vector2D,
        Vector3D, Vector4D, Zero,
    };
    pub use shape2d::{
        circle, line, rectangle, Fill, IntoShape, Rectangle, Rotation, Shape, Stroke,
    };
}

pub use prelude::*;

pub trait Geometry: Sized {
    fn transform(self, t: impl Into<Transform>) -> Self;

    fn untransform(self, t: impl Into<Transform>) -> Self {
        self.transform(t.into().inverse())
    }
}

impl Geometry for Rect<f32> {
    fn transform(self, t: impl Into<Transform>) -> Self {
        let t = t.into();
        let min = t * self.min();
        let max = t * self.max();

        Self::points(min, max)
    }
}

impl Geometry for Point2D<f32> {
    fn transform(self, t: impl Into<Transform>) -> Self {
        t.into() * self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
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
