use std::fmt;

use crate::gfx::color::Rgba;
use crate::gfx::math::rect::{Box2D, Rect};
use crate::gfx::math::*;
use crate::gfx::{Geometry, Rgba8, ZDepth};

use std::f32;

///////////////////////////////////////////////////////////////////////////
// Vertex
///////////////////////////////////////////////////////////////////////////

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Vertex {
    pub position: Vector3D<f32>,
    pub angle: f32,
    pub center: Vector2D<f32>,
    pub color: Rgba8,
}

impl Vertex {
    const fn new(x: f32, y: f32, z: f32, angle: f32, center: Point2D<f32>, color: Rgba8) -> Self {
        Self {
            position: Vector3D::new(x, y, z),
            angle,
            center: Vector2D::new(center.x, center.y),
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
    center: Point2D<f32>,
    color: Rgba8,
) -> Vertex {
    Vertex::new(x, y, z, angle, center, color)
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Shapes
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Copy, Clone, Debug)]
pub struct Stroke {
    pub width: f32,
    pub offset: f32,
    pub color: Rgba,
}

impl Stroke {
    pub const NONE: Self = Self {
        width: 0.,
        offset: 0.,
        color: Rgba::TRANSPARENT,
    };

    pub fn new(width: f32, color: impl Into<Rgba>) -> Self {
        Self {
            width,
            offset: 0.,
            color: color.into(),
        }
    }

    pub fn inside(mut self) -> Self {
        self.offset -= self.width;
        self
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum Fill {
    #[default]
    Empty,
    Solid(Rgba),
}

impl Fill {
    pub fn solid<T: Into<Rgba>>(color: T) -> Self {
        Self::Solid(color.into())
    }
}

impl From<Rgba8> for Fill {
    fn from(color: Rgba8) -> Self {
        Self::Solid(color.into())
    }
}

#[derive(Clone, Debug)]
pub struct Rotation {
    angle: f32,
    center: Point2D<f32>,
}

impl Rotation {
    pub const ZERO: Rotation = Rotation {
        angle: 0.0,
        center: Point2D { x: 0.0, y: 0.0 },
    };

    pub fn new(angle: f32, center: Point2D<f32>) -> Self {
        Self { angle, center }
    }
}

impl Default for Rotation {
    fn default() -> Self {
        Rotation::ZERO
    }
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    rect: Rect<f32>,
    zdepth: ZDepth,
    rotation: Rotation,
    stroke: Stroke,
    fill: Fill,
}

impl From<Rect<f32>> for Rectangle {
    fn from(rect: Rect<f32>) -> Self {
        Self::new(rect.origin, rect.size)
    }
}

impl Rectangle {
    pub fn new(origin: impl Into<Point>, size: impl Into<Size>) -> Self {
        Self {
            rect: Rect::new(origin, size),
            zdepth: ZDepth::default(),
            rotation: Rotation::default(),
            stroke: Stroke::default(),
            fill: Fill::default(),
        }
    }

    pub fn zdepth(self, zdepth: impl Into<ZDepth>) -> Self {
        Self {
            zdepth: zdepth.into(),
            ..self
        }
    }

    pub fn rotation(self, angle: f32, center: impl Into<Point>) -> Self {
        let center = center.into();
        let rotation = Rotation::new(angle, center);

        Self { rotation, ..self }
    }

    pub fn stroke(self, width: impl Into<f32>, color: impl Into<Rgba>) -> Self {
        let stroke = Stroke::new(width.into(), color.into());

        Self { stroke, ..self }
    }

    pub fn fill(self, fill: impl Into<Fill>) -> Self {
        Self {
            fill: fill.into(),
            ..self
        }
    }
}

pub trait Shape: fmt::Debug {
    fn vertices(&self) -> Vec<Vertex>;
}

impl Shape for Rectangle {
    fn vertices(&self) -> Vec<Vertex> {
        let ZDepth(z) = self.zdepth;
        let Rotation { angle, center } = self.rotation;
        let stroke = &self.stroke;
        let fill = &self.fill;
        let width = stroke.width;
        let outer = Box2D::from(self.rect.expand(stroke.offset, stroke.offset));
        let inner = Box2D::new(
            Point::new(outer.min.x + width, outer.min.y + width),
            Point::new(outer.max.x - width, outer.max.y - width),
        );

        let mut verts = if stroke != &Stroke::NONE {
            let rgba8 = stroke.color.into();

            vec![
                // Bottom
                vertex(outer.min.x, outer.min.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.min.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.min.y, z, angle, center, rgba8),
                vertex(inner.max.x, inner.min.y, z, angle, center, rgba8),
                // Left
                vertex(outer.min.x, outer.min.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                vertex(outer.min.x, outer.max.y, z, angle, center, rgba8),
                vertex(outer.min.x, outer.max.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.max.y, z, angle, center, rgba8),
                // Right
                vertex(inner.max.x, inner.min.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.min.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.max.y, z, angle, center, rgba8),
                vertex(inner.max.x, inner.min.y, z, angle, center, rgba8),
                vertex(inner.max.x, inner.max.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.max.y, z, angle, center, rgba8),
                // Top
                vertex(outer.min.x, outer.max.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.max.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.max.y, z, angle, center, rgba8),
                vertex(inner.min.x, inner.max.y, z, angle, center, rgba8),
                vertex(outer.max.x, outer.max.y, z, angle, center, rgba8),
                vertex(inner.max.x, inner.max.y, z, angle, center, rgba8),
            ]
        } else {
            Vec::with_capacity(6)
        };

        match fill {
            Fill::Solid(color) => {
                let rgba8 = (*color).into();

                verts.extend([
                    vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                    vertex(inner.max.x, inner.min.y, z, angle, center, rgba8),
                    vertex(inner.max.x, inner.max.y, z, angle, center, rgba8),
                    vertex(inner.min.x, inner.min.y, z, angle, center, rgba8),
                    vertex(inner.min.x, inner.max.y, z, angle, center, rgba8),
                    vertex(inner.max.x, inner.max.y, z, angle, center, rgba8),
                ]);
            }
            Fill::Empty => {}
        }
        verts
    }
}

#[derive(Clone, Debug)]
pub struct Circle {
    pub origin: Point,
    pub radius: f32,
    pub sides: u32,
    pub zdepth: ZDepth,
    pub stroke: Stroke,
    pub fill: Fill,
}

impl Circle {
    pub fn new(origin: impl Into<Point>, radius: f32, sides: u32) -> Self {
        let origin = origin.into();

        Self {
            origin,
            radius,
            sides,
            zdepth: ZDepth::default(),
            stroke: Stroke::default(),
            fill: Fill::default(),
        }
    }

    fn points(position: Point, radius: f32, sides: u32) -> Vec<Point> {
        let mut verts = Vec::with_capacity(sides as usize + 1);

        for i in 0..=sides as usize {
            let angle: f32 = i as f32 * ((2. * f32::consts::PI) / sides as f32);
            verts.push(Point::new(
                position.x + radius * angle.cos(),
                position.y + radius * angle.sin(),
            ));
        }
        verts
    }
}

impl Shape for Circle {
    fn vertices(&self) -> Vec<Vertex> {
        let Self {
            origin,
            radius,
            sides,
            stroke,
            fill,
            zdepth,
        } = self;

        let ZDepth(z) = zdepth;
        let inner = Self::points(*origin, radius - stroke.width, *sides);

        let mut verts = if stroke != &Stroke::NONE {
            // If there is a stroke, the outer circle is larger.
            let outer = Self::points(*origin, *radius, *sides);
            let rgba8 = stroke.color.into();

            let n = inner.len() - 1;
            let mut vs = Vec::with_capacity(n * 6);
            for i in 0..n {
                let (i0, i1) = (inner[i], inner[i + 1]);
                let (o0, o1) = (outer[i], outer[i + 1]);

                vs.extend_from_slice(&[
                    vertex(i0.x, i0.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                    vertex(o0.x, o0.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                    vertex(o1.x, o1.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                    vertex(i0.x, i0.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                    vertex(o1.x, o1.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                    vertex(i1.x, i1.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8),
                ]);
            }
            vs
        } else {
            Vec::new()
        };

        match fill {
            Fill::Solid(color) => {
                let rgba8 = (*color).into();
                let center =
                    Vertex::new(origin.x, origin.y, *z, 0.0, Point2D::new(0.0, 0.0), rgba8);
                let inner_verts: Vec<Vertex> = inner
                    .iter()
                    .map(|p| Vertex::new(p.x, p.y, *z, 0., Point2D::new(0.0, 0.0), rgba8))
                    .collect();
                for i in 0..*sides as usize {
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

#[derive(Clone, Debug)]
pub struct Line {
    pub p1: Point2D<f32>,
    pub p2: Point2D<f32>,
    pub zdepth: ZDepth,
    pub rotation: Rotation,
    pub stroke: Stroke,
}

impl Line {
    pub fn new<P: Into<Point2D<f32>>>(p1: P, p2: P) -> Self {
        Self {
            p1: p1.into(),
            p2: p2.into(),
            zdepth: ZDepth::default(),
            rotation: Rotation::default(),
            stroke: Stroke::default(),
        }
    }
}

impl Shape for Line {
    fn vertices(&self) -> Vec<Vertex> {
        let (p1, p2) = (self.p1, self.p2);
        let ZDepth(z) = self.zdepth;
        let Rotation { angle, center } = self.rotation;
        let Stroke { width, color, .. } = self.stroke;
        let v = (p2 - p1).normalize();
        let wx = width / 2. * v.y;
        let wy = width / 2. * v.x;
        let rgba8 = color.into();

        vec![
            vertex(p1.x - wx, p1.y + wy, z, angle, center, rgba8),
            vertex(p1.x + wx, p1.y - wy, z, angle, center, rgba8),
            vertex(p2.x - wx, p2.y + wy, z, angle, center, rgba8),
            vertex(p2.x - wx, p2.y + wy, z, angle, center, rgba8),
            vertex(p1.x + wx, p1.y - wy, z, angle, center, rgba8),
            vertex(p2.x + wx, p2.y - wy, z, angle, center, rgba8),
        ]
    }
}

impl Geometry for Line {
    fn transform(self, m: impl Into<Transform>) -> Self {
        let t = m.into();

        Self {
            p1: t * self.p1,
            p2: t * self.p2,
            ..self
        }
    }
}

impl std::ops::Mul<f32> for Line {
    type Output = Self;

    fn mul(self, n: f32) -> Self {
        Self {
            p1: self.p1 * n,
            p2: self.p2 * n,
            ..self
        }
    }
}

impl std::ops::Add<Vector2D<f32>> for Line {
    type Output = Self;

    fn add(self, vec: Vector2D<f32>) -> Self {
        Self {
            p1: self.p1 + vec,
            p2: self.p2 + vec,
            ..self
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Batch
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Default)]
pub struct Batch {
    vertices: Vec<Vertex>,
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shape(mut self, shape: impl Shape) -> Self {
        self.vertices.extend(shape.vertices());
        self
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
    }
}

impl From<Batch> for Vec<Vertex> {
    fn from(batch: Batch) -> Self {
        batch.vertices
    }
}

pub trait IntoShape<T: Shape> {
    fn into_shape(self) -> T;
}

impl IntoShape<Rectangle> for Rect<f32> {
    fn into_shape(self) -> Rectangle {
        self.into()
    }
}

pub fn line<P: Into<Point2D<f32>>>(p1: P, p2: P) -> Line {
    Line::new(p1, p2)
}

pub fn rectangle(rect: impl Into<Rect<f32>>) -> Rectangle {
    Rectangle::from(rect.into())
}

pub fn circle(origin: impl Into<Point>, radius: f32, sides: u32) -> Circle {
    Circle::new(origin, radius, sides)
}
