use rgx::core as gfx;
use rgx::core::Rect;
use rgx::kit::sprite2d;
use rgx::kit::{Repeat, Rgba8, ZDepth};

pub struct Font {
    gw: f32,
    gh: f32,

    width: f32,
    height: f32,

    pub binding: gfx::BindingGroup,
    pub texture: gfx::Texture,
}

impl Font {
    pub fn new(
        texture: gfx::Texture,
        binding: gfx::BindingGroup,
        gw: f32,
        gh: f32,
    ) -> Font {
        let width = texture.w as f32;
        let height = texture.h as f32;

        Font {
            gw,
            gh,
            width,
            height,
            texture,
            binding,
        }
    }
}

pub struct TextBatch {
    raw: sprite2d::Batch,
    gw: f32,
    gh: f32,
}

impl TextBatch {
    pub fn new(f: &Font) -> Self {
        let raw = sprite2d::Batch::new(f.width as u32, f.height as u32);

        Self {
            raw,
            gw: f.gw,
            gh: f.gh,
        }
    }

    pub fn add(&mut self, text: &str, mut sx: f32, sy: f32, color: Rgba8) {
        let offset: f32 = 32.;

        let gw = self.gw;
        let gh = self.gh;
        let rgba = color.into();

        for c in text.bytes().into_iter() {
            let x: f32 = (c as f32 - offset) * gw;

            self.raw.add(
                Rect::new(x, 0., x + gw, gh),
                Rect::new(sx, sy, sx + gw, sy + gh),
                ZDepth::default(),
                rgba,
                1.0,
                Repeat::default(),
            );
            sx += gw;
        }
    }

    pub fn finish(self, r: &gfx::Renderer) -> gfx::VertexBuffer {
        self.raw.finish(r)
    }
}
