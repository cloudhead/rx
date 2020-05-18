use std::ops::Deref;
use std::ops::Range;

use rgx::math::Point2;
use rgx::rect::Rect;

use crate::util;

#[derive(Debug, Clone)]
pub enum FrameRange {
    Full,
    Partial(Range<usize>),
}

impl Default for FrameRange {
    fn default() -> Self {
        Self::Full
    }
}

/// Layer identifier.
pub type LayerId = usize;

#[derive(Debug, Default, Clone)]
pub struct Layer {
    /// Frame range.
    pub frames: FrameRange,
    /// Visbility.
    pub is_visible: bool,
    /// Sort order.
    pub index: usize,
}

impl Layer {
    pub fn new(frames: FrameRange, index: usize) -> Self {
        Self {
            frames,
            is_visible: true,
            index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerCoords<T>(Point2<T>);

impl<T> LayerCoords<T> {
    pub fn new(x: T, y: T) -> Self {
        Self(Point2::new(x, y))
    }
}

impl LayerCoords<i32> {
    pub fn clamp(&mut self, rect: Rect<i32>) {
        util::clamp(&mut self.0, rect);
    }
}

impl<T> Deref for LayerCoords<T> {
    type Target = Point2<T>;

    fn deref(&self) -> &Point2<T> {
        &self.0
    }
}

impl Into<LayerCoords<i32>> for LayerCoords<f32> {
    fn into(self) -> LayerCoords<i32> {
        LayerCoords::new(self.x.round() as i32, self.y.round() as i32)
    }
}

impl Into<LayerCoords<f32>> for LayerCoords<i32> {
    fn into(self) -> LayerCoords<f32> {
        LayerCoords::new(self.x as f32, self.y as f32)
    }
}

impl Into<LayerCoords<u32>> for LayerCoords<f32> {
    fn into(self) -> LayerCoords<u32> {
        LayerCoords::new(self.x.round() as u32, self.y.round() as u32)
    }
}

impl From<Point2<f32>> for LayerCoords<f32> {
    fn from(p: Point2<f32>) -> LayerCoords<f32> {
        LayerCoords::new(p.x, p.y)
    }
}
