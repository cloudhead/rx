use crate::gfx::color::Rgba;
use crate::gfx::math::*;
use crate::gfx::rect::Rect;
use crate::gfx::{Geometry, Rgba8, ZDepth};

use std::f32;

///////////////////////////////////////////////////////////////////////////
// Vertex
///////////////////////////////////////////////////////////////////////////

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Vertex {
    pub position: Vector3<f32>,
    pub angle: f32,
    pub center: Vector2<f32>,
    pub color: Rgba8,
}

impl Vertex {
    const fn new(x: f32, y: f32, z: f32, angle: f32, center: Point2<f32>, color: Rgba8) -> Self {
        Self {
            position: Vector3::new(x, y, z),
            angle,
            center: Vector2::new(center.x, center.y),
            color,
        }
    }
}

#[inline]
pub const fn vertex(
    x: f32,
    y: f32,
    z: f32,
    angle: f32,
    center: Point2<f32>,
    color: Rgba8,
) -> Vertex {
    Vertex::new(x, y, z, angle, center, color)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Shapes
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Copy, Clone, Debug)]
pub struct Stroke {
    width: f32,
    color: Rgba,
}

impl Stroke {
    pub const NONE: Self = Self {
        width: 0.,
        color: Rgba::TRANSPARENT,
    };

    pub fn new(width: f32, color: Rgba) -> Self {
        Self { width, color }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.,
            color: Rgba::default(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Fill {
    Empty,
    Solid(Rgba),
}

impl Fill {
    pub fn solid<T: Into<Rgba>>(color: T) -> Self {
        Self::Solid(color.into())
    }
}

impl Default for Fill {
    fn default() -> Self {
        Self::Empty
    }
}

#[derive(Clone, Debug)]
pub struct Rotation {
    angle: f32,
    center: Point2<f32>,
}

impl Rotation {
    pub const ZERO: Rotation = Rotation {
        angle: 0.0,
        center: Point2 { x: 0.0, y: 0.0 },
    };

    pub fn new(angle: f32, center: Point2<f32>) -> Self {
        Self { angle, center }
    }
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation::ZERO
    }
}

#[derive(Clone, Debug)]
pub enum Shape {
    Line(Line, ZDepth, Rotation, Stroke),
    Rectangle(Rect<f32>, ZDepth, Rotation, Stroke, Fill),
    Circle(Circle, ZDepth, Stroke, Fill),
}

impl Shape {
    pub fn circle<P: Into<Point2<f32>>>(position: P, radius: f32, sides: u32) -> Self {
        let position = position.into();

        Self::Circle(
            Circle {
                position,
                radius,
                sides,
            },
            ZDepth::default(),
            Stroke::default(),
            Fill::default(),
        )
    }

    pub fn line<P: Into<Point2<f32>>>(p1: P, p2: P) -> Self {
        Self::Line(
            Line::new(p1, p2),
            ZDepth::default(),
            Rotation::default(),
            Stroke::default(),
        )
    }

    pub fn rect<P: Into<Point2<f32>>>(p1: P, p2: P) -> Self {
        let (p1, p2) = (p1.into(), p2.into());

        Self::Rectangle(
            Rect::new(p1.x, p1.y, p2.x, p2.y),
            ZDepth::default(),
            Rotation::default(),
            Stroke::default(),
            Fill::default(),
        )
    }

    pub fn zdepth<T: Into<ZDepth>>(mut self, z: T) -> Self {
        let z: ZDepth = z.into();

        match self {
            Self::Line(_, ref mut zdepth, _, _) => *zdepth = z,
            Self::Rectangle(_, ref mut zdepth, _, _, _) => *zdepth = z,
            Self::Circle(_, ref mut zdepth, _, _) => *zdepth = z,
        }
        self
    }

    pub fn rotation<P: Into<Point2<f32>>>(mut self, angle: f32, center: P) -> Self {
        let center = center.into();
        let r = Rotation::new(angle, center);

        match self {
            Self::Line(_, _, ref mut rotation, _) => *rotation = r,
            Self::Rectangle(_, _, ref mut rotation, _, _) => *rotation = r,
            _ => {}
        }
        self
    }

    pub fn fill(mut self, f: Fill) -> Self {
        match self {
            Self::Rectangle(_, _, _, _, ref mut fill) => *fill = f,
            Self::Circle(_, _, _, ref mut fill) => *fill = f,
            _ => {}
        }
        self
    }

    pub fn stroke<T: Into<Rgba>>(mut self, width: f32, color: T) -> Self {
        let s = Stroke::new(width, color.into());

        match self {
            Self::Line(_, _, _, ref mut stroke) => *stroke = s,
            Self::Rectangle(_, _, _, ref mut stroke, _) => *stroke = s,
            Self::Circle(_, _, ref mut stroke, _) => *stroke = s,
        }
        self
    }

    pub fn triangulate(&self) -> Vec<Vertex> {
        match *self {
            Shape::Line(l, ZDepth(z), Rotation { angle, center }, Stroke { width, color }) => {
                let v = (l.p2 - l.p1).normalize();

                let wx = width / 2.0 * v.y;
                let wy = width / 2.0 * v.x;
                let rgba8 = color.into();

                vec![
                    vertex(l.p1.x - wx, l.p1.y + wy, z, angle, center, rgba8),
                    vertex(l.p1.x + wx, l.p1.y - wy, z, angle, center, rgba8),
                    vertex(l.p2.x - wx, l.p2.y + wy, z, angle, center, rgba8),
                    vertex(l.p2.x - wx, l.p2.y + wy, z, angle, center, rgba8),
                    vertex(l.p1.x + wx, l.p1.y - wy, z, angle, center, rgba8),
                    vertex(l.p2.x + wx, l.p2.y - wy, z, angle, center, rgba8),
                ]
            }
            Shape::Rectangle(r, ZDepth(z), Rotation { angle, center }, stroke, fill) => {
                let width = stroke.width;
                let inner = Rect::new(r.x1 + width, r.y1 + width, r.x2 - width, r.y2 - width);

                let mut verts = if stroke != Stroke::NONE {
                    let rgba8 = stroke.color.into();

                    let outer = r;

                    vec![
                        // Bottom
                        vertex(outer.x1, outer.y1, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y1, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y1, z, angle, center, rgba8),
                        vertex(inner.x2, inner.y1, z, angle, center, rgba8),
                        // Left
                        vertex(outer.x1, outer.y1, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                        vertex(outer.x1, outer.y2, z, angle, center, rgba8),
                        vertex(outer.x1, outer.y2, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y2, z, angle, center, rgba8),
                        // Right
                        vertex(inner.x2, inner.y1, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y1, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y2, z, angle, center, rgba8),
                        vertex(inner.x2, inner.y1, z, angle, center, rgba8),
                        vertex(inner.x2, inner.y2, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y2, z, angle, center, rgba8),
                        // Top
                        vertex(outer.x1, outer.y2, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y2, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y2, z, angle, center, rgba8),
                        vertex(inner.x1, inner.y2, z, angle, center, rgba8),
                        vertex(outer.x2, outer.y2, z, angle, center, rgba8),
                        vertex(inner.x2, inner.y2, z, angle, center, rgba8),
                    ]
                } else {
                    Vec::with_capacity(6)
                };

                match fill {
                    Fill::Solid(color) => {
                        let rgba8 = color.into();

                        verts.extend_from_slice(&[
                            vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                            vertex(inner.x2, inner.y1, z, angle, center, rgba8),
                            vertex(inner.x2, inner.y2, z, angle, center, rgba8),
                            vertex(inner.x1, inner.y1, z, angle, center, rgba8),
                            vertex(inner.x1, inner.y2, z, angle, center, rgba8),
                            vertex(inner.x2, inner.y2, z, angle, center, rgba8),
                        ]);
                    }
                    Fill::Empty => {}
                }
                verts
            }
            Shape::Circle(circle, ZDepth(z), stroke, fill) => {
                let Circle {
                    position,
                    radius,
                    sides,
                } = circle;
                let inner = Self::circle_points(position, radius - stroke.width, sides);

                let mut verts = if stroke != Stroke::NONE {
                    // If there is a stroke, the outer circle is larger.
                    let outer = Self::circle_points(position, radius, sides);
                    let rgba8 = stroke.color.into();

                    let n = inner.len() - 1;
                    let mut vs = Vec::with_capacity(n * 6);
                    for i in 0..n {
                        let (i0, i1) = (inner[i], inner[i + 1]);
                        let (o0, o1) = (outer[i], outer[i + 1]);

                        vs.extend_from_slice(&[
                            vertex(i0.x, i0.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                            vertex(o0.x, o0.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                            vertex(o1.x, o1.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                            vertex(i0.x, i0.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                            vertex(o1.x, o1.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                            vertex(i1.x, i1.y, z, 0.0, Point2::new(0.0, 0.0), rgba8),
                        ]);
                    }
                    vs
                } else {
                    Vec::new()
                };

                match fill {
                    Fill::Solid(color) => {
                        let rgba8 = color.into();
                        let center = Vertex::new(
                            position.x,
                            position.y,
                            z,
                            0.0,
                            Point2::new(0.0, 0.0),
                            rgba8,
                        );
                        let inner_verts: Vec<Vertex> = inner
                            .iter()
                            .map(|p| Vertex::new(p.x, p.y, z, 0., Point2::new(0.0, 0.0), rgba8))
                            .collect();
                        for i in 0..sides as usize {
                            verts.extend_from_slice(&[center, inner_verts[i], inner_verts[i + 1]]);
                        }
                        verts.extend_from_slice(&[
                            center,
                            *inner_verts.last().unwrap(),
                            *inner_verts.first().unwrap(),
                        ]);
                    }
                    Fill::Empty => {}
                }
                verts
            }
        }
    }

    fn circle_points(position: Point2<f32>, radius: f32, sides: u32) -> Vec<Point2<f32>> {
        let mut verts = Vec::with_capacity(sides as usize + 1);

        for i in 0..=sides as usize {
            let angle: f32 = i as f32 * ((2. * f32::consts::PI) / sides as f32);
            verts.push(Point2::new(
                position.x + radius * angle.cos(),
                position.y + radius * angle.sin(),
            ));
        }
        verts
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Line {
    pub p1: Point2<f32>,
    pub p2: Point2<f32>,
}

impl Line {
    pub fn new<P: Into<Point2<f32>>>(p1: P, p2: P) -> Self {
        Self {
            p1: p1.into(),
            p2: p2.into(),
        }
    }
}

impl Geometry for Line {
    fn transform(self, m: Matrix4<f32>) -> Self {
        let v1 = m * Vector4::new(self.p1.x, self.p1.y, 0., 1.);
        let v2 = m * Vector4::new(self.p2.x, self.p2.y, 0., 1.);

        Self {
            p1: Point2::new(v1.x, v1.y),
            p2: Point2::new(v2.x, v2.y),
        }
    }
}

impl std::ops::Mul<f32> for Line {
    type Output = Self;

    fn mul(self, n: f32) -> Self {
        Self {
            p1: self.p1 * n,
            p2: self.p2 * n,
        }
    }
}

impl std::ops::Add<Vector2<f32>> for Line {
    type Output = Self;

    fn add(self, vec: Vector2<f32>) -> Self {
        Self {
            p1: self.p1 + vec,
            p2: self.p2 + vec,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Circle {
    pub position: Point2<f32>,
    pub radius: f32,
    pub sides: u32,
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Batch
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Default)]
pub struct Batch {
    items: Vec<Shape>,
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, shape: Shape) {
        self.items.push(shape);
    }

    pub fn vertices(&self) -> Vec<Vertex> {
        // TODO: This is a lower-bound estimate of how much space we need.
        // We should get the actual numbers from the shapes.
        let mut buf = Vec::with_capacity(6 * self.items.len());

        for shape in self.items.iter() {
            let mut verts: Vec<Vertex> = shape.triangulate();
            buf.append(&mut verts);
        }
        buf
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}
