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
