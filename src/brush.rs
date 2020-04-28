use crate::pixels::PixelsMut;
use crate::view::layer::LayerCoords;
use crate::view::{ViewCoords, ViewExtent};

use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::{Rgba8, ZDepth};
use rgx::math::{Point2, Vector2};
use rgx::rect::Rect;

use std::collections::BTreeSet;
use std::fmt;

/// Input state of the brush.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum BrushState {
    /// Not currently drawing.
    NotDrawing,
    /// Drawing has just started.
    DrawStarted(ViewExtent),
    /// Drawing.
    Drawing(ViewExtent),
    /// Drawing has just ended.
    DrawEnded(ViewExtent),
}

/// Brush mode. Any number of these modes can be active at once.
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub enum BrushMode {
    /// Erase pixels.
    Erase,
    /// Draw on all frames at once.
    Multi,
    /// Pixel-perfect mode.
    Perfect,
    /// X-Symmetry mode.
    XSym,
    /// Y-Symmetry mode.
    YSym,
    /// X-Ray mode.
    XRay,
}

impl fmt::Display for BrushMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Erase => "erase".fmt(f),
            Self::Multi => "multi".fmt(f),
            Self::Perfect => "perfect".fmt(f),
            Self::XSym => "xsym".fmt(f),
            Self::YSym => "ysym".fmt(f),
            Self::XRay => "xray".fmt(f),
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Align {
    Center,
    BottomLeft,
}

/// Brush context.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Brush {
    /// Brush size in pixels.
    pub size: usize,
    /// Current brush state.
    pub state: BrushState,
    /// Current brush stroke.
    pub stroke: Vec<Point2<i32>>,
    /// Current stroke color.
    pub color: Rgba8,

    /// Currently active brush modes.
    modes: BTreeSet<BrushMode>,
    /// Current brush position.
    curr: Point2<i32>,
    /// Previous brush position.
    prev: Point2<i32>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 1,
            state: BrushState::NotDrawing,
            stroke: Vec::with_capacity(32),
            color: Rgba8::TRANSPARENT,
            modes: BTreeSet::new(),
            curr: Point2::new(0, 0),
            prev: Point2::new(0, 0),
        }
    }
}

impl Brush {
    /// Check whether the given mode is active.
    pub fn is_set(&self, m: BrushMode) -> bool {
        self.modes.contains(&m)
    }

    /// Activate the given brush mode.
    pub fn set(&mut self, m: BrushMode) -> bool {
        self.modes.insert(m)
    }

    /// De-activate the given brush mode.
    pub fn unset(&mut self, m: BrushMode) -> bool {
        self.modes.remove(&m)
    }

    /// Toggle the given brush mode.
    pub fn toggle(&mut self, m: BrushMode) {
        if self.is_set(m) {
            self.unset(m);
        } else {
            self.set(m);
        }
    }

    /// Check whether the brush is currently drawing.
    pub fn is_drawing(&self) -> bool {
        match self.state {
            BrushState::NotDrawing => false,
            _ => true,
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.modes.clear();
    }

    /// Run every frame by the session.
    pub fn update(&mut self) {
        if let BrushState::DrawEnded(_) = self.state {
            self.state = BrushState::NotDrawing;
            self.stroke.clear();
        }
    }

    /// Start drawing. Called when input is first pressed.
    pub fn start_drawing(&mut self, p: LayerCoords<i32>, color: Rgba8, extent: ViewExtent) {
        self.state = BrushState::DrawStarted(extent);
        self.color = color;
        self.stroke = Vec::with_capacity(32);
        self.draw(p);
    }

    /// Draw. Called while input is pressed.
    pub fn draw(&mut self, p: LayerCoords<i32>) {
        self.prev = if let BrushState::DrawStarted(_) = self.state {
            *p
        } else {
            self.curr
        };
        self.curr = *p;

        Brush::line(self.prev, self.curr, &mut self.stroke);
        self.stroke.dedup();

        if self.is_set(BrushMode::Perfect) {
            self.stroke = Brush::filter(&self.stroke);
        }

        match self.state {
            BrushState::Drawing(_) => {}
            BrushState::DrawStarted(extent) => {
                self.state = BrushState::Drawing(extent);
            }
            _ => unreachable!(),
        }
    }

    /// Stop drawing. Called when input is released.
    pub fn stop_drawing(&mut self) {
        match self.state {
            BrushState::DrawStarted(ex) | BrushState::Drawing(ex) => {
                self.state = BrushState::DrawEnded(ex);
            }
            _ => unreachable!(),
        }
    }

    /// Expand a point into all brush heads.
    pub fn expand(&self, p: ViewCoords<i32>, extent: ViewExtent) -> Vec<ViewCoords<i32>> {
        let mut pixels = vec![*p];
        let ViewExtent { fw, fh, nframes } = extent;

        if self.is_set(BrushMode::XSym) {
            for p in pixels.clone() {
                let frame_index = p.x / fw as i32;

                pixels.push(Point2::new(
                    (frame_index + 1) * fw as i32 - (p.x - frame_index * fw as i32) - 1,
                    p.y,
                ));
            }
        }
        if self.is_set(BrushMode::YSym) {
            for p in pixels.clone() {
                pixels.push(Point2::new(p.x, fh as i32 - p.y - 1));
            }
        }
        if self.is_set(BrushMode::Multi) {
            for p in pixels.clone() {
                let frame_index = p.x / fw as i32;
                for i in 0..nframes as i32 - frame_index {
                    let offset = Vector2::new((i as u32 * fw) as i32, 0);
                    pixels.push(p + offset);
                }
            }
        }
        pixels.iter().map(|p| ViewCoords::new(p.x, p.y)).collect()
    }

    /// Return the brush's output strokes as shapes.
    pub fn output(&self, stroke: Stroke, fill: Fill, scale: f32, align: Align) -> Vec<Shape> {
        match self.state {
            BrushState::DrawStarted(extent)
            | BrushState::Drawing(extent)
            | BrushState::DrawEnded(extent) => {
                let mut pixels = Vec::new();

                for p in &self.stroke {
                    pixels.extend_from_slice(
                        self.expand(ViewCoords::new(p.x, p.y), extent).as_slice(),
                    );
                }
                pixels
                    .iter()
                    .map(|p| {
                        self.shape(
                            Point2::new(p.x as f32, p.y as f32),
                            ZDepth::ZERO,
                            stroke,
                            fill,
                            scale,
                            align,
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Return the shape that should be painted when the brush is at the given
    /// position with the given parameters. Takes an `Origin` which describes
    /// whether to align the position to the bottom-left of the shape, or the
    /// center.
    pub fn shape(
        &self,
        p: Point2<f32>,
        z: ZDepth,
        stroke: Stroke,
        fill: Fill,
        scale: f32,
        align: Align,
    ) -> Shape {
        let x = p.x;
        let y = p.y;

        let size = self.size as f32;

        let offset = match align {
            Align::Center => size * scale / 2.,
            Align::BottomLeft => (self.size / 2) as f32 * scale,
        };

        Shape::Rectangle(
            Rect::new(x, y, x + size * scale, y + size * scale) - Vector2::new(offset, offset),
            z,
            Rotation::ZERO,
            stroke,
            fill,
        )
    }

    ///////////////////////////////////////////////////////////////////////////

    /// Draw a line between two points. Uses Bresenham's line algorithm.
    fn line(mut p0: Point2<i32>, p1: Point2<i32>, canvas: &mut Vec<Point2<i32>>) {
        let dx = i32::abs(p1.x - p0.x);
        let dy = i32::abs(p1.y - p0.y);
        let sx = if p0.x < p1.x { 1 } else { -1 };
        let sy = if p0.y < p1.y { 1 } else { -1 };

        let mut err1 = (if dx > dy { dx } else { -dy }) / 2;
        let mut err2;

        loop {
            canvas.push(p0);

            if p0 == p1 {
                break;
            }

            err2 = err1;

            if err2 > -dx {
                err1 -= dy;
                p0.x += sx;
            }
            if err2 < dy {
                err1 += dx;
                p0.y += sy;
            }
        }
    }

    /// Paint a circle into a pixel buffer.
    #[allow(dead_code)]
    fn paint(
        pixels: &mut [Rgba8],
        w: usize,
        h: usize,
        position: Point2<f32>,
        diameter: f32,
        color: Rgba8,
    ) {
        let mut grid = PixelsMut::new(pixels, w, h);
        let bias = if diameter <= 2. {
            0.0
        } else if diameter <= 3. {
            0.5
        } else {
            0.0
        };
        let radius = diameter / 2. - bias;

        for (x, y, c) in grid.iter_mut() {
            let (x, y) = (x as f32, y as f32);

            let dx = (x - position.x).abs();
            let dy = (y - position.y).abs();
            let d = (dx.powi(2) + dy.powi(2)).sqrt();

            if d <= radius {
                *c = color;
            }
        }
    }

    /// Filter a brush stroke to remove 'L' shapes. This is often called
    /// *pixel perfect* mode.
    fn filter(stroke: &[Point2<i32>]) -> Vec<Point2<i32>> {
        let mut filtered = Vec::with_capacity(stroke.len());

        filtered.extend(stroke.first().cloned());

        let mut triples = stroke.windows(3);
        while let Some(triple) = triples.next() {
            let (prev, curr, next) = (triple[0], triple[1], triple[2]);
            if (prev.y == curr.y && next.x == curr.x) || (prev.x == curr.x && next.y == curr.y) {
                filtered.push(next);
                triples.next();
            } else {
                filtered.push(curr);
            }
        }

        filtered.extend(stroke.last().cloned());

        filtered
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_paint() {
        let z = Rgba8::TRANSPARENT;
        let w = Rgba8::WHITE;

        #[rustfmt::skip]
        let brush1 = vec![
            z, z, z,
            z, w, z,
            z, z, z,
        ];

        #[rustfmt::skip]
        let brush2 = vec![
            z, z, z, z,
            z, w, w, z,
            z, w, w, z,
            z, z, z, z,
        ];

        #[rustfmt::skip]
        let brush3 = vec![
            z, z, z, z, z,
            z, z, w, z, z,
            z, w, w, w, z,
            z, z, w, z, z,
            z, z, z, z, z,
        ];

        #[rustfmt::skip]
        let brush5 = vec![
            z, z, z, z, z, z, z,
            z, z, w, w, w, z, z,
            z, w, w, w, w, w, z,
            z, w, w, w, w, w, z,
            z, w, w, w, w, w, z,
            z, z, w, w, w, z, z,
            z, z, z, z, z, z, z,
        ];

        #[rustfmt::skip]
        let brush7 = vec![
            z, z, z, z, z, z, z, z, z,
            z, z, z, w, w, w, z, z, z,
            z, z, w, w, w, w, w, z, z,
            z, w, w, w, w, w, w, w, z,
            z, w, w, w, w, w, w, w, z,
            z, w, w, w, w, w, w, w, z,
            z, z, w, w, w, w, w, z, z,
            z, z, z, w, w, w, z, z, z,
            z, z, z, z, z, z, z, z, z
        ];

        #[rustfmt::skip]
        let brush15 = vec![
            z, z, z, z, z, w, w, w, w, w, z, z, z, z, z,
            z, z, z, w, w, w, w, w, w, w, w, w, z, z, z,
            z, z, w, w, w, w, w, w, w, w, w, w, w, z, z,
            z, w, w, w, w, w, w, w, w, w, w, w, w, w, z,
            z, w, w, w, w, w, w, w, w, w, w, w, w, w, z,
            w, w, w, w, w, w, w, w, w, w, w, w, w, w, w,
            w, w, w, w, w, w, w, w, w, w, w, w, w, w, w,
            w, w, w, w, w, w, w, w, w, w, w, w, w, w, w,
            w, w, w, w, w, w, w, w, w, w, w, w, w, w, w,
            w, w, w, w, w, w, w, w, w, w, w, w, w, w, w,
            z, w, w, w, w, w, w, w, w, w, w, w, w, w, z,
            z, w, w, w, w, w, w, w, w, w, w, w, w, w, z,
            z, z, w, w, w, w, w, w, w, w, w, w, w, z, z,
            z, z, z, w, w, w, w, w, w, w, w, w, z, z, z,
            z, z, z, z, z, w, w, w, w, w, z, z, z, z, z,
        ];

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 3 * 3];
            Brush::paint(&mut canvas, 3, 3, Point2::new(1., 1.), 1., Rgba8::WHITE);
            assert_eq!(canvas, brush1);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 4 * 4];
            Brush::paint(&mut canvas, 4, 4, Point2::new(1.5, 1.5), 2., Rgba8::WHITE);
            assert_eq!(canvas, brush2);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 5 * 5];
            Brush::paint(&mut canvas, 5, 5, Point2::new(2., 2.), 3., Rgba8::WHITE);
            assert_eq!(canvas, brush3);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 7 * 7];
            Brush::paint(&mut canvas, 7, 7, Point2::new(3., 3.), 5., Rgba8::WHITE);
            assert_eq!(canvas, brush5);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 9 * 9];
            Brush::paint(&mut canvas, 9, 9, Point2::new(4., 4.), 7., Rgba8::WHITE);
            assert_eq!(canvas, brush7);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 15 * 15];
            Brush::paint(&mut canvas, 15, 15, Point2::new(7., 7.), 15., Rgba8::WHITE);
            assert_eq!(canvas, brush15);
        }
    }
}
