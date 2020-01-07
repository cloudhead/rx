use rgx::kit::ZDepth;
use rgx::math::{Vector2, Vector3};
use rgx::rect::Rect;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex(Vector3<f32>, Vector2<f32>);

pub struct Sprite {
    w: u32,
    h: u32,
    buf: Vec<Vertex>,
}

impl Sprite {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            buf: Vec::with_capacity(6),
        }
    }

    pub fn set(&mut self, src: Rect<f32>, dst: Rect<f32>, z: ZDepth) {
        let ZDepth(z) = z;

        // Relative texture coordinates
        let rx1: f32 = src.x1 / self.w as f32;
        let ry1: f32 = src.y1 / self.h as f32;
        let rx2: f32 = src.x2 / self.w as f32;
        let ry2: f32 = src.y2 / self.h as f32;

        self.buf.extend_from_slice(&[
            Vertex(Vector3::new(dst.x1, dst.y1, z), Vector2::new(rx1, ry2)),
            Vertex(Vector3::new(dst.x2, dst.y1, z), Vector2::new(rx2, ry2)),
            Vertex(Vector3::new(dst.x2, dst.y2, z), Vector2::new(rx2, ry1)),
            Vertex(Vector3::new(dst.x1, dst.y1, z), Vector2::new(rx1, ry2)),
            Vertex(Vector3::new(dst.x1, dst.y2, z), Vector2::new(rx1, ry1)),
            Vertex(Vector3::new(dst.x2, dst.y2, z), Vector2::new(rx2, ry1)),
        ]);
    }

    #[cfg(not(feature = "wgpu"))]
    pub fn vertices(&self) -> Vec<Vertex> {
        self.buf.clone()
    }

    #[cfg(feature = "wgpu")]
    pub fn finish(self, r: &rgx::core::Renderer) -> rgx::core::VertexBuffer {
        r.device.create_buffer(self.buf.as_slice())
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}
