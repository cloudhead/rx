use std::collections::{HashMap, VecDeque};
use std::ops::Deref;
use std::sync::Arc;

use crate::gfx::prelude::*;
use crate::gfx::{self, shape2d, sprite2d};
use crate::platform::Cursor as HwCursor;
use crate::renderer::{Blending, Effect, Paint, TextureId};
use crate::ui::text::{Font, FontError, FontFormat, FontId, Text};
use crate::ui::Context;
use crate::ui::Cursor;

#[derive(Debug)]
pub struct Canvas<'a> {
    pub transform: Transform,
    pub size: Size<f32>,
    pub target: Option<TextureId>,
    pub blending: Blending,

    context: &'a Context<'a>,
    graphics: &'a mut Graphics,
}

impl<'a> Deref for Canvas<'a> {
    type Target = Graphics;

    fn deref(&self) -> &Self::Target {
        self.graphics
    }
}

impl<'a> Canvas<'a> {
    pub fn new(
        context: &'a Context<'a>,
        graphics: &'a mut Graphics,
        transform: Transform,
        size: Size<f32>,
    ) -> Self {
        Self {
            transform,
            size,
            target: None,
            blending: Blending::default(),
            context,
            graphics,
        }
    }

    pub fn is_hot(&self) -> bool {
        self.context.hot
    }

    pub fn is_active(&self) -> bool {
        self.context.active
    }

    pub fn transform(&mut self, transform: Transform) -> Canvas<'_> {
        Canvas {
            transform: transform * self.transform,
            size: self.size,
            target: self.target,
            blending: self.blending,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn resize(&mut self, size: Size<f32>) -> Canvas<'_> {
        Canvas {
            transform: self.transform,
            size,
            target: self.target,
            blending: self.blending,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn with(&mut self, transform: Transform) -> Canvas<'_> {
        Canvas {
            transform,
            size: self.size,
            target: self.target,
            blending: self.blending,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn fill(&mut self, rect: impl Into<Rect<f32>>, color: Rgba8) {
        self.paint(Rectangle::from(rect.into()).fill(color))
    }

    pub fn stroke(&mut self, rect: impl Into<Rect<f32>>, width: f32, color: Rgba8) {
        self.paint(Rectangle::from(rect.into()).stroke(width, color))
    }

    pub fn paint(&mut self, paint: impl IntoPaint) {
        let paint = paint.into_paint(self);

        self.graphics.paint(
            if let Some(target) = self.target {
                paint.on(target)
            } else {
                paint.transform(self.transform)
            },
            self.blending,
        );
    }

    pub fn on(&mut self, texture: TextureId) -> Canvas<'_> {
        Canvas {
            target: Some(texture),
            size: self.size,
            transform: self.transform,
            blending: self.blending,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn blending(&mut self, blending: Blending) -> Canvas<'_> {
        Canvas {
            target: self.target,
            size: self.size,
            blending,
            transform: self.transform,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn offscreen(
        &mut self,
        id: TextureId,
        size: Size<u32>,
        image: impl FnOnce() -> Image,
    ) -> TextureId {
        self.graphics.offscreen(id, size, image);
        id
    }

    pub fn clear(&mut self, color: Rgba8) {
        if let Some(id) = self.target {
            self.graphics.clear(id, color);
        } else {
            todo!();
        }
    }

    pub fn upload(&mut self, texels: Arc<[Rgba8]>) {
        if let Some(id) = self.target {
            self.graphics
                .effects
                .push_back(Effect::Upload { id, texels });
        } else {
            todo!();
        }
    }

    pub fn textures(&self) -> &HashMap<TextureId, TextureInfo> {
        &self.graphics.textures
    }

    pub fn clone(&mut self) -> Canvas<'_> {
        Canvas {
            size: self.size,
            transform: self.transform,
            target: self.target,
            blending: self.blending,
            context: self.context,
            graphics: self.graphics,
        }
    }

    pub fn bounds(&self) -> Rect<f32> {
        Rect::origin(self.size)
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct TextureInfo {
    /// Size in pixels of the texture.
    pub size: Size<u32>,
}

/// Graphics context in host memory. Holds pending paint operations.
/// Shared between UI widgets and renderer.
#[derive(Debug, Default)]
pub struct Graphics {
    /// Textures in use.
    pub textures: HashMap<TextureId, TextureInfo>,
    /// Hardware cursors registered.
    pub cursors: HashMap<&'static str, HwCursor>,
    /// Cursor state.
    pub cursor: Cursor,
    /// Fonts in use.
    pub fonts: HashMap<FontId, Font>,
    /// Pending effects to be handled by backend.
    effects: VecDeque<Effect>,

    // XXX
    pub hw_cursor: &'static str,
}

impl Graphics {
    pub fn texture(&mut self, id: TextureId, image: Image) {
        self.textures.entry(id).or_insert_with(|| {
            let size = image.size;

            self.effects.push_back(Effect::Texture {
                id,
                image,
                offscreen: false,
            });
            TextureInfo { size }
        });
    }

    pub fn font(
        &mut self,
        id: impl Into<FontId>,
        bytes: &[u8],
        format: FontFormat,
    ) -> Result<FontId, FontError> {
        let texture_id = TextureId::next();
        let font_id = id.into();
        let (image, widths) = Font::decode(bytes, format)?;

        self.texture(texture_id, image);
        self.fonts.insert(
            font_id.clone(),
            Font {
                widths,
                texture_id,
                tile: format.size(),
            },
        );

        Ok(font_id)
    }

    pub fn offscreen(&mut self, id: TextureId, size: Size<u32>, image: impl FnOnce() -> Image) {
        let texture = self.textures.entry(id).or_insert_with(|| {
            let image = image();
            let size = image.size;

            self.effects.push_back(Effect::Texture {
                id,
                image,
                offscreen: true,
            });
            TextureInfo { size }
        });

        if texture.size != size {
            texture.size = size;

            self.effects.push_back(Effect::Resize { id, size });
        }
    }

    pub fn paint(&mut self, paint: Paint, blending: Blending) {
        self.effects.push_back(Effect::Paint { paint, blending });
    }

    pub fn clear(&mut self, id: TextureId, color: Rgba8) {
        self.effects.push_back(Effect::Clear { id, color });
    }

    pub fn effects(&mut self) -> impl Iterator<Item = Effect> + '_ {
        self.effects.drain(..)
    }

    pub fn cursor(&self) -> gfx::cursor2d::Sprite {
        let texture = self.textures.get(&TextureId::default_cursors()).unwrap();
        self.cursor.sprite(texture.size)
    }
}

impl Paint {
    pub fn text(body: impl ToString, font: FontId, canvas: &Canvas<'_>) -> Self {
        Text::new(body).font(font).into_paint(canvas)
    }

    pub fn sprite(texture_id: &TextureId, sprite: sprite2d::Sprite, canvas: &Canvas<'_>) -> Self {
        let texture = canvas.textures().get(texture_id).unwrap();
        let batch = sprite2d::Batch::new(texture.size).sprite(sprite);
        let vertices = batch.vertices();

        Paint::Sprite {
            transform: Transform::identity(),
            texture: *texture_id,
            vertices,
            target: None,
        }
    }

    pub fn texture(texture_id: &TextureId, canvas: &Canvas<'_>) -> Self {
        let texture = canvas.textures().get(texture_id).unwrap();
        let vertices = sprite2d::Batch::new(texture.size)
            .item(
                Rect::origin(texture.size),
                Rect::origin(texture.size),
                ZDepth::default(),
                Rgba::TRANSPARENT,
                1.,
                Repeat::default(),
            )
            .vertices();

        Paint::Sprite {
            transform: Transform::identity(),
            texture: *texture_id,
            vertices,
            target: None,
        }
    }

    pub fn on(self, target: TextureId) -> Self {
        match self {
            Self::Shape {
                transform,
                vertices,
                ..
            } => Self::Shape {
                transform,
                vertices,
                target: Some(target),
            },
            Self::Sprite {
                transform,
                vertices,
                texture,
                ..
            } => Self::Sprite {
                transform,
                vertices,
                texture,
                target: Some(target),
            },
        }
    }

    pub fn offset(self, offset: Offset) -> Self {
        let translation = Transform::translate(offset);

        match self {
            Self::Shape {
                transform,
                vertices,
                target,
            } => Self::Shape {
                transform: transform * translation,
                vertices,
                target,
            },
            Self::Sprite {
                transform,
                vertices,
                texture,
                target,
            } => Self::Sprite {
                transform: transform * translation,
                vertices,
                texture,
                target,
            },
        }
    }

    pub fn scale(self, scale: f32) -> Self {
        self.transform(Transform::scale(scale))
    }

    pub fn transform(self, t: Transform) -> Self {
        match self {
            Self::Shape {
                transform,
                vertices,
                target,
            } => Self::Shape {
                transform: transform * t,
                vertices,
                target,
            },
            Self::Sprite {
                transform,
                vertices,
                target,
                texture,
            } => Self::Sprite {
                transform: transform * t,
                vertices,
                texture,
                target,
            },
        }
    }
}

/// Types that can be turned into paint, given a canvas.
pub trait IntoPaint {
    /// Turn into paint.
    fn into_paint(self, canvas: &Canvas<'_>) -> Paint;
}

impl<T: Into<Paint>> IntoPaint for T {
    fn into_paint(self, _canvas: &Canvas<'_>) -> Paint {
        self.into()
    }
}

impl IntoPaint for TextureId {
    fn into_paint(self, canvas: &Canvas<'_>) -> Paint {
        let texture = canvas.textures().get(&self).unwrap();
        let vertices = sprite2d::Batch::new(texture.size)
            .item(
                Rect::origin(texture.size),
                Rect::origin(texture.size),
                ZDepth::default(),
                Rgba::TRANSPARENT,
                1.,
                Repeat::default(),
            )
            .vertices();

        Paint::Sprite {
            transform: Transform::identity(),
            texture: self,
            vertices,
            target: None,
        }
    }
}

impl From<Rect<f32>> for Paint {
    fn from(rect: Rect<f32>) -> Self {
        let batch = shape2d::Batch::new().shape(Rectangle::from(rect));

        Self::Shape {
            transform: Transform::identity(),
            vertices: batch.into(),
            target: None,
        }
    }
}

impl<T: Shape> From<T> for Paint {
    fn from(shape: T) -> Self {
        let vertices = shape.vertices();

        Self::Shape {
            transform: Transform::identity(),
            vertices,
            target: None,
        }
    }
}

impl From<Vec<shape2d::Vertex>> for Paint {
    fn from(vertices: Vec<shape2d::Vertex>) -> Self {
        Self::Shape {
            transform: Transform::identity(),
            vertices,
            target: None,
        }
    }
}

impl From<std::vec::Drain<'_, shape2d::Vertex>> for Paint {
    fn from(vertices: std::vec::Drain<'_, shape2d::Vertex>) -> Self {
        Self::Shape {
            transform: Transform::identity(),
            vertices: vertices.collect(),
            target: None,
        }
    }
}
