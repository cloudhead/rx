use crate::gfx::{sprite2d};
use crate::gfx::{Rect, Repeat, Rgba8, ZDepth};

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
    ) -> f32 {
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

        for c in text.bytes() {
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

        return sx;
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

    pub fn vertices(&self) -> Vec<sprite2d::Vertex> {
        self.raw.vertices()
    }

    pub fn clear(&mut self) {
        self.raw.clear()
    }
}
