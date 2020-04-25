use rgx::kit::sprite2d;
use rgx::kit::{Repeat, Rgba8, ZDepth};
use rgx::rect::Rect;

#[cfg(feature = "wgpu")]
use rgx::core::Renderable;

pub enum TextAlign {
    Left,
    Right,
}

pub struct TextBatch {
    raw: sprite2d::Batch,
    gw: f32,
    gh: f32,
}

impl TextBatch {
    pub fn new(w: u32, h: u32, gw: f32, gh: f32) -> Self {
        let raw = sprite2d::Batch::new(w, h);

        Self { raw, gw, gh }
    }

    pub fn add(
        &mut self,
        text: &str,
        mut sx: f32,
        sy: f32,
        z: ZDepth,
        color: Rgba8,
        align: TextAlign,
    ) {
        let offset: usize = 32;

        let gw = self.gw;
        let gh = self.gh;
        let rgba = color.into();

        match align {
            TextAlign::Left => {}
            TextAlign::Right => {
                sx -= gw * text.chars().count() as f32;
            }
        }

        for c in text.bytes().into_iter() {
            let i: usize = c as usize - offset;
            let x: f32 = (i % 16) as f32 * gw;
            let y: f32 = (i / 16) as f32 * gh;

            self.raw.add(
                Rect::new(x, y, x + gw, y + gh),
                Rect::new(sx, sy, sx + gw, sy + gh),
                z,
                rgba,
                1.0,
                Repeat::default(),
            );
            sx += gw;
        }
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        self.raw.offset(x, y);
    }

    pub fn glyph(&mut self, glyph: usize, sx: f32, sy: f32, z: ZDepth, color: Rgba8) {
        let gw = self.gw;
        let gh = self.gh;
        let rgba = color.into();

        let i: usize = glyph;
        let x: f32 = (i % 16) as f32 * gw;
        let y: f32 = (i / 16) as f32 * gh;

        self.raw.add(
            Rect::new(x, y, x + gw, y + gh),
            Rect::new(sx, sy, sx + gw, sy + gh),
            z,
            rgba,
            1.0,
            Repeat::default(),
        );
    }

    #[cfg(feature = "wgpu")]
    pub fn finish(self, r: &rgx::core::Renderer) -> rgx::core::VertexBuffer {
        self.raw.finish(r)
    }

    #[cfg(not(feature = "wgpu"))]
    pub fn vertices(&self) -> Vec<sprite2d::Vertex> {
        self.raw.vertices()
    }

    pub fn clear(&mut self) {
        self.raw.clear()
    }
}
