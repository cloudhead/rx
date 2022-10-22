use crate::app::view::ViewExtent;
use crate::gfx::pixels::PixelsMut;
use crate::gfx::prelude::*;

use std::collections::HashSet;
use std::f32::consts::PI;
use std::fmt;

/// Input state of the brush.
#[derive(Clone, Debug)]
pub enum State {
    /// Not currently drawing.
    NotDrawing,
    /// Drawing.
    Drawing {
        prev: Point2D<i32>,
        extent: ViewExtent,
    },
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum Modifier {
    /// Draw on all frames at once.
    Multi,
    /// Symmetry mode.
    Mirror { axis: Axis },
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Multi => "multi".fmt(f),
            Self::Mirror {
                axis: Axis::Horizontal,
            } => "mirror/x".fmt(f),
            Self::Mirror {
                axis: Axis::Vertical,
            } => "mirror/y".fmt(f),
        }
    }
}

/// Brush mode. Any number of these modes can be active at once.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash, Default)]
pub enum Mode {
    /// Normal brush mode.
    #[default]
    Normal,
    /// Erase pixels.
    Erase,
    /// Pixel-perfect mode.
    Pencil,
    /// Confine stroke to a straight line from the starting point
    Line {
        /// Snap angle (degrees).
        snap: Option<u32>,
    },
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Erase => "erase".fmt(f),
            Self::Pencil => "pencil".fmt(f),
            Self::Normal => "brush".fmt(f),
            Self::Line { snap: Some(snap) } => write!(f, "{} degree snap line", snap),
            Self::Line { snap: None } => write!(f, "line"),
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Align {
    Center,
    BottomLeft,
}

/// Brush context.
#[derive(Debug, Clone)]
pub struct Brush {
    /// Brush size in pixels.
    pub size: usize,
    /// Current brush state.
    pub state: State,
    /// Current brush stroke.
    pub stroke: Vec<Point2D<i32>>,
    /// Current stroke color.
    pub color: Rgba8,
    /// Currently active brush mode.
    pub mode: Mode,
    /// Previous mode.
    pub prev_mode: Option<Mode>,
    /// Currently active modifiers.
    pub modifiers: HashSet<Modifier>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 1,
            state: State::NotDrawing,
            stroke: Vec::new(),
            color: Rgba8::TRANSPARENT,
            mode: Mode::Normal,
            prev_mode: None,
            modifiers: HashSet::new(),
        }
    }
}

impl Brush {
    /// Check the current mode.
    pub fn is_mode(&self, m: Mode) -> bool {
        self.mode == m
    }

    /// Check whether the given modifier is active.
    pub fn is_set(&self, m: Modifier) -> bool {
        self.modifiers.contains(&m)
    }

    /// Activate the given brush modifier.
    pub fn set(&mut self, m: Modifier) -> bool {
        self.modifiers.insert(m)
    }

    /// De-activate the given brush modifier.
    pub fn unset(&mut self, m: Modifier) -> bool {
        self.modifiers.remove(&m)
    }

    /// Toggle the given brush modifier.
    pub fn toggle(&mut self, m: Modifier) {
        if self.is_set(m) {
            self.unset(m);
        } else {
            self.set(m);
        }
    }

    /// Set the brush mode.
    pub fn mode(&mut self, mode: Mode) {
        if self.mode == mode {
            self.mode = self.prev_mode.unwrap_or(mode)
        } else {
            self.prev_mode = Some(self.mode);
            self.mode = mode;
        }
    }

    /// Check whether the brush is currently drawing.
    pub fn is_drawing(&self) -> bool {
        matches!(self.state, State::Drawing { .. })
    }

    /// Reset brush modifiers.
    pub fn reset(&mut self) {
        self.modifiers.clear();
    }

    /// Start drawing. Called when input is first received.
    pub fn begin_stroke(
        &mut self,
        origin: Point2D<i32>,
        color: Rgba8,
        extent: ViewExtent,
    ) -> Vec<Rectangle> {
        self.state = State::Drawing {
            prev: origin,
            extent,
        };
        self.color = color;
        self.stroke = vec![origin];
        self.stroke()
    }

    /// Draw. Called while input is pressed.
    pub fn extend_stroke(&mut self, point: Point2D<i32>) -> Vec<Rectangle> {
        if let State::Drawing { prev, extent } = self.state {
            if let Mode::Line { snap } = self.mode {
                let start = *self.stroke.first().unwrap_or(&point);

                let end = match snap {
                    None => point,
                    Some(snap) => {
                        let snap_rad = snap as f32 * PI / 180.;
                        let curr: Vector2D<f32> = point.map(|x| x as f32).into();
                        let start: Vector2D<f32> = start.map(|x| x as f32).into();
                        let dist = curr.distance(start);
                        let angle = curr.angle(&start) - PI / 2.;
                        let round_angle = (angle / snap_rad).round() * snap_rad;
                        let end =
                            start + Vector2D::new(round_angle.cos(), round_angle.sin()) * dist;

                        Point2D::new(end.x.round() as i32, end.y.round() as i32)
                    }
                };
                self.stroke.clear();

                Brush::line(start, end, &mut self.stroke);
            } else {
                Brush::line(prev, point, &mut self.stroke);

                self.stroke.dedup();

                if self.mode == Mode::Pencil {
                    self.stroke = Brush::filter(&self.stroke);
                }
            }

            self.state = State::Drawing {
                prev: point,
                extent,
            };
            self.stroke()
        } else {
            vec![]
        }
    }

    /// Stop drawing. Called when input is released.
    pub fn end_stroke(&mut self) -> Vec<Rectangle> {
        assert!(self.is_drawing());

        let stroke = self.stroke();

        self.state = State::NotDrawing;
        self.stroke.clear();

        stroke
    }

    /// Expand a point into all brush heads.
    pub fn expand(&self, p: Point2D<i32>, extent: ViewExtent) -> Vec<Point2D<i32>> {
        let mut pixels = vec![p];
        let ViewExtent { fw, fh, nframes } = extent;

        if self.is_set(Modifier::Mirror {
            axis: Axis::Horizontal,
        }) {
            for p in pixels.clone() {
                let frame_index = p.x / fw as i32;

                pixels.push(Point2D::new(
                    (frame_index + 1) * fw as i32 - (p.x - frame_index * fw as i32) - 1,
                    p.y,
                ));
            }
        }

        if self.is_set(Modifier::Mirror {
            axis: Axis::Vertical,
        }) {
            for p in pixels.clone() {
                pixels.push(Point2D::new(p.x, fh as i32 - p.y - 1));
            }
        }

        if self.is_set(Modifier::Multi) {
            for p in pixels.clone() {
                let frame_index = p.x / fw as i32;
                for i in 0..nframes as i32 - frame_index {
                    let offset = Vector2D::new((i as f32 * fw as f32) as i32, 0);
                    pixels.push(p + offset);
                }
            }
        }
        pixels.to_vec()
    }

    /// Return the brush's stroke as shapes.
    pub fn stroke(&mut self) -> Vec<Rectangle> {
        match self.state {
            State::Drawing { extent, .. } => {
                let mut pixels = Vec::new();

                for p in &self.stroke {
                    pixels.extend(self.expand(*p, extent));
                }
                pixels
                    .iter()
                    .map(|p| self.paint(p.map(|v| v as f32)))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Return the shape that should be painted when the brush is at the given position.
    pub fn paint(&self, point: Point2D<f32>) -> Rectangle {
        let size = self.size as f32;
        let offset = (self.size / 2) as f32;

        Rectangle::from(Rect::new(point, size) - Vector2D::from(offset)).fill(self.color)
    }

    ///////////////////////////////////////////////////////////////////////////

    /// Draw a line between two points. Uses Bresenham's line algorithm.
    pub fn line(mut p0: Point2D<i32>, p1: Point2D<i32>, canvas: &mut Vec<Point2D<i32>>) {
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
    fn circle(
        pixels: &mut [Rgba8],
        w: usize,
        h: usize,
        position: Point2D<f32>,
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
    fn filter(stroke: &[Point2D<i32>]) -> Vec<Point2D<i32>> {
        let mut filtered = Vec::with_capacity(stroke.len());
        filtered.extend(stroke.first().cloned());

        let mut triples = stroke.windows(3);
        while let Some(&[prev, curr, next]) = triples.next() {
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
            Brush::circle(&mut canvas, 3, 3, Point2D::new(1., 1.), 1., Rgba8::WHITE);
            assert_eq!(canvas, brush1);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 4 * 4];
            Brush::circle(&mut canvas, 4, 4, Point2D::new(1.5, 1.5), 2., Rgba8::WHITE);
            assert_eq!(canvas, brush2);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 5 * 5];
            Brush::circle(&mut canvas, 5, 5, Point2D::new(2., 2.), 3., Rgba8::WHITE);
            assert_eq!(canvas, brush3);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 7 * 7];
            Brush::circle(&mut canvas, 7, 7, Point2D::new(3., 3.), 5., Rgba8::WHITE);
            assert_eq!(canvas, brush5);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 9 * 9];
            Brush::circle(&mut canvas, 9, 9, Point2D::new(4., 4.), 7., Rgba8::WHITE);
            assert_eq!(canvas, brush7);
        }

        {
            let mut canvas = vec![Rgba8::TRANSPARENT; 15 * 15];
            Brush::circle(&mut canvas, 15, 15, Point2D::new(7., 7.), 15., Rgba8::WHITE);
            assert_eq!(canvas, brush15);
        }
    }
}
