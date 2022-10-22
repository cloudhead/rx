use crate::gfx::math::rect::{Box2D, Rect};
use crate::gfx::math::{Vector2D, Vector3D};
use crate::gfx::Size;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex(Vector3D, Vector2D);

pub struct Sprite {
    vertices: Vec<Vertex>,
}

impl Sprite {
    pub fn new(size: impl Into<Size<u32>>, src: Rect<f32>, dst: Rect<f32>) -> Self {
        let size = size.into();
        let [w, h]: [u32; 2] = size.into();
        let src: Box2D<f32> = src.into();
        let dst: Box2D<f32> = dst.into();
        let z = 1.;

        // Relative texture coordinates.
        let rx1: f32 = src.min.x / w as f32;
        let ry1: f32 = src.min.y / h as f32;
        let rx2: f32 = src.max.x / w as f32;
        let ry2: f32 = src.max.y / h as f32;

        let vertices = vec![
            Vertex(
                Vector3D::new(dst.min.x, dst.min.y, z),
                Vector2D::new(rx1, ry1),
            ),
            Vertex(
                Vector3D::new(dst.max.x, dst.max.y, z),
                Vector2D::new(rx2, ry2),
            ),
            Vertex(
                Vector3D::new(dst.max.x, dst.min.y, z),
                Vector2D::new(rx2, ry1),
            ),
            Vertex(
                Vector3D::new(dst.min.x, dst.min.y, z),
                Vector2D::new(rx1, ry1),
            ),
            Vertex(
                Vector3D::new(dst.max.x, dst.max.y, z),
                Vector2D::new(rx2, ry2),
            ),
            Vertex(
                Vector3D::new(dst.min.x, dst.max.y, z),
                Vector2D::new(rx1, ry2),
            ),
        ];
        Self { vertices }
    }

    pub fn vertices(&self) -> Vec<Vertex> {
        self.vertices.clone()
    }
}
