pub mod fonts;

use std::array::TryFromSliceError;
use std::fmt;
use thiserror::Error;

use crate::gfx::pixels::PixelsMut;
use crate::gfx::prelude::*;
use crate::gfx::sprite2d;

use super::{Canvas, Env, IntoPaint, LayoutCtx, Paint, TextureId, Widget};

#[derive(Debug, Error)]
pub enum FontError {
    #[error("Invalid font")]
    TryFromSlice(#[from] TryFromSliceError),
    #[error("Invalid tile count '{0}'")]
    TileCount(usize),
    #[error("Invalid font byte length '{0}'")]
    ByteLength(usize),
}

#[derive(Debug, Clone, Copy)]
pub enum FontFormat {
    UF1,
    UF2,
}

impl FontFormat {
    pub fn size(&self) -> Size<f32> {
        match self {
            Self::UF1 => Size::from(8.),
            Self::UF2 => Size::from(16.),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Font {
    pub widths: [u8; 256],
    pub texture_id: TextureId,
    pub tile: Size<f32>,
}

impl Font {
    pub fn decode(bytes: &[u8], format: FontFormat) -> Result<(Image, [u8; 256]), FontError> {
        // Tile width and height. For UF2 fonts, each glyph is represented by four tiles.
        const T: usize = 8;
        // Glyph count. Represents the ASCII range.
        const N: usize = 256;
        // Number of tiles per glyph.
        const G: usize = 2 * 2;

        assert!(matches!(format, FontFormat::UF2));

        let (widths, glyphs) = bytes.split_at(N);
        let (head, tiles, tail) = unsafe { glyphs.align_to::<[u8; T]>() };

        if !head.is_empty() || !tail.is_empty() {
            return Err(FontError::ByteLength(glyphs.len()));
        }
        if tiles.len() != N * G {
            return Err(FontError::TileCount(tiles.len()));
        }

        // Rasterize the font into a 256x256 texture.
        let size = Size::new(N, N);
        let widths: [u8; N] = widths.try_into()?;
        let mut texels = vec![Rgba8::ZERO; size.area()];
        let mut pixels = PixelsMut::new(&mut texels, size.w, size.h);

        // Each glyph is a 2x2 grid of tiles encoded in the following order:
        //
        //   0 2
        //   1 3
        //
        // We loop through the tiles in chunks of 2, where each iteration renders half a glyph
        // into the texture.
        //
        let v = Rgba8::WHITE;
        let (mut x, mut y) = (0, 0);

        for window in tiles.chunks(G / 2) {
            if let &[a, b] = window {
                pixels.icn(a, x, y, v);
                pixels.icn(b, x, y + T, v);

                x += T;

                if x == size.w {
                    x = 0;
                    y += T + T;
                }
            }
        }

        Ok((Image::new(texels, size), widths))
    }

    pub fn glyph_width(&self, c: u8) -> f32 {
        self.widths[c as usize] as f32
    }

    pub fn text_width(&self, text: &str) -> f32 {
        text.bytes().map(|c| self.glyph_width(c)).sum()
    }

    pub fn text_height(&self) -> f32 {
        FontFormat::UF2.size().h
    }
}

/// Identifies a font.
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct FontId(pub String);

impl fmt::Display for FontId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for FontId {
    fn default() -> Self {
        Self(String::from("default"))
    }
}

impl From<&str> for FontId {
    fn from(other: &str) -> Self {
        Self(other.to_owned())
    }
}

impl From<String> for FontId {
    fn from(other: String) -> Self {
        Self(other)
    }
}

pub struct Text {
    pub body: String,
    pub font: FontId,
    pub color: Rgba8,
    pub transform: Transform,
    pub align: TextAlign,
    pub size: Size,
}

impl Text {
    pub fn new(body: impl ToString) -> Self {
        Self {
            body: body.to_string(),
            font: FontId::default(),
            color: Rgba8::WHITE,
            transform: Transform::identity(),
            align: TextAlign::Left,
            size: Size::ZERO,
        }
    }

    pub fn color(self, color: Rgba8) -> Self {
        Self { color, ..self }
    }

    pub fn font(self, font: FontId) -> Self {
        Self { font, ..self }
    }

    pub fn transform(self, transform: Transform) -> Self {
        Self { transform, ..self }
    }

    pub fn offset(self, offset: impl Into<Offset>) -> Self {
        Self {
            transform: self.transform * Transform::translate(offset.into()),
            ..self
        }
    }

    pub fn align(self, align: TextAlign) -> Self {
        Self { align, ..self }
    }
}

impl IntoPaint for &Text {
    fn into_paint(self, canvas: &Canvas<'_>) -> Paint {
        let font = canvas.fonts.get(&self.font).unwrap();
        let texture = canvas.textures().get(&font.texture_id).unwrap();
        // XXX Don't clone the font?
        let vertices = TextBatch::new(*font, texture.size)
            .add(
                &self.body.to_string(),
                0.,
                0.,
                ZDepth::default(),
                self.color,
                self.align,
            )
            .vertices();

        Paint::Sprite {
            transform: self.transform,
            texture: font.texture_id,
            vertices,
            target: canvas.target,
        }
    }
}

impl IntoPaint for Text {
    fn into_paint(self, canvas: &Canvas<'_>) -> Paint {
        (&self).into_paint(canvas)
    }
}

impl<T> Widget<T> for Text {
    fn layout(&mut self, _parent: Size, ctx: &LayoutCtx<'_>, _data: &T, _env: &Env) -> Size {
        if let Some(font) = ctx.fonts.get(&self.font) {
            self.size = Size::new(font.text_width(&self.body), font.text_height());
        }
        self.size
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, _data: &T) {
        canvas.paint(&*self)
    }

    // TODO: Maybe return `Option<bool>`, and if `None`, it's determined
    // by the Pod?!
    fn contains(&self, point: Point) -> bool {
        // Rect::<f32>::origin(self.size).contains(point)
        true
    }
    // fn contains(&self, parent:Rect, point: Point) -> bool {
    //     parent.contains(point)
    // }

    fn display(&self) -> String {
        format!("Text({:?})", self.body)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct TextBatch {
    raw: sprite2d::Batch,
    font: Font,
}

impl TextBatch {
    pub fn new(font: Font, size: Size<u32>) -> Self {
        let raw = sprite2d::Batch::new(size);

        Self { raw, font }
    }

    pub fn add(
        mut self,
        text: &str,
        mut sx: f32,
        sy: f32,
        z: ZDepth,
        color: Rgba8,
        align: TextAlign, // TODO: Shouldn't be a property of text, should be the container!
    ) -> Self {
        let size = Size::new(16., 16.);
        let rgba = color.into();

        match align {
            TextAlign::Left => {}
            TextAlign::Right => {
                sx -= self.font.text_width(text);
            }
            TextAlign::Center => {
                sx -= self.font.text_width(text) / 2.;
            }
        }

        for c in text.bytes() {
            let w = self.font.glyph_width(c);
            let i = c as usize;
            let x = (i % 16) as f32 * self.font.tile.w;
            let y = (i / 16) as f32 * self.font.tile.h;

            self.raw.add(
                Rect::new(Point2D::new(x, y), size),
                Rect::new(Point2D::new(sx, sy), size),
                z,
                rgba,
                1.0,
                Repeat::default(),
            );
            sx += w;
        }
        self
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        self.raw.offset(x, y);
    }

    pub fn glyph(&mut self, glyph: usize, sx: f32, sy: f32, z: ZDepth, color: Rgba8) {
        let rgba = color.into();

        let gw = 16.;
        let gh = 16.;
        let size = Size::new(16., 16.);

        let i: usize = glyph;
        let x: f32 = (i % 16) as f32 * gw;
        let y: f32 = (i / 16) as f32 * gh;

        self.raw.add(
            Rect::new(Point2D::new(x, y), size),
            Rect::new(Point2D::new(sx, sy), size),
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
