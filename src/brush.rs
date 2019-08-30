use crate::kit::shape2d;
use crate::kit::shape2d::{Fill, Shape, Stroke};
use crate::kit::Origin;

use rgx::core::{Rect, Rgba8};

use cgmath::{Point2, Vector2};

use std::collections::BTreeSet;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum BrushState {
    NotDrawing = 0,
    DrawStarted = 1,
    Drawing = 2,
    DrawEnded = 3,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub enum BrushMode {
    Erase,
    Multi,
}

#[derive(Debug, Clone)]
pub struct Brush {
    pub size: usize,
    pub state: BrushState,
    pub modes: BTreeSet<BrushMode>,

    curr: Point2<i32>,
    prev: Point2<i32>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 1,
            state: BrushState::NotDrawing,
            modes: BTreeSet::new(),
            curr: Point2::new(0, 0),
            prev: Point2::new(0, 0),
        }
    }
}

impl Brush {
    pub fn tick(
        &mut self,
        p: Point2<i32>,
        color: Rgba8,
        offsets: &[Vector2<i32>],
        canvas: &mut shape2d::Batch,
    ) {
        if self.state == BrushState::DrawStarted {
            self.prev = p;
        } else {
            self.prev = self.curr;
        }
        self.curr = p;

        if offsets.is_empty() {
            self.draw(self.prev, self.curr, color, canvas);
        } else {
            for off in offsets {
                self.draw(self.prev + off, self.curr + off, color, canvas);
            }
        }
    }

    pub fn is_set(&self, m: BrushMode) -> bool {
        self.modes.contains(&m)
    }

    pub fn set(&mut self, m: BrushMode) -> bool {
        self.modes.insert(m)
    }

    pub fn unset(&mut self, m: BrushMode) -> bool {
        self.modes.remove(&m)
    }

    pub fn reset(&mut self) {
        self.modes.clear();
    }

    pub fn start_drawing(
        &mut self,
        p: Point2<i32>,
        color: Rgba8,
        offsets: &[Vector2<i32>],
        canvas: &mut shape2d::Batch,
    ) {
        self.state = BrushState::DrawStarted;
        self.tick(p, color, offsets, canvas);
    }

    pub fn stop_drawing(&mut self) {
        self.state = BrushState::DrawEnded;
    }

    pub fn draw(
        &self,
        mut p0: Point2<i32>,
        p1: Point2<i32>,
        color: Rgba8,
        canvas: &mut shape2d::Batch,
    ) {
        let fill = Fill::Solid(color.into());

        if self.state > BrushState::DrawStarted {
            let dx = i32::abs(p1.x - p0.x);
            let dy = i32::abs(p1.y - p0.y);
            let sx = if p0.x < p1.x { 1 } else { -1 };
            let sy = if p0.y < p1.y { 1 } else { -1 };

            let mut err1 = (if dx > dy { dx } else { -dy }) / 2;
            let mut err2;

            loop {
                canvas.add(self.stroke(
                    Point2::new(p0.x as f32, p0.y as f32),
                    Stroke::NONE,
                    fill,
                    1.0,
                    Origin::BottomLeft,
                ));

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
        } else {
            canvas.add(self.stroke(
                Point2::new(p0.x as f32, p0.y as f32),
                Stroke::NONE,
                fill,
                1.0,
                Origin::BottomLeft,
            ));
        }
    }

    pub fn stroke(
        &self,
        p: Point2<f32>,
        stroke: Stroke,
        fill: Fill,
        scale: f32,
        origin: Origin,
    ) -> Shape {
        let x = p.x;
        let y = p.y;

        let size = self.size as f32;

        let offset = match origin {
            Origin::Center => size * scale / 2.,
            Origin::BottomLeft => (self.size / 2) as f32 * scale,
            Origin::TopLeft => unreachable!(),
        };

        Shape::Rectangle(
            Rect::new(x, y, x + size * scale, y + size * scale)
                - Vector2::new(offset, offset),
            stroke,
            fill,
        )
    }
}
