pub mod backends;

use std::collections::HashMap;
use std::fmt;
use std::sync::{atomic, Arc};

use crate::gfx;
use crate::gfx::{Image, Rgba8, Size, Transform};
use crate::platform::{self, LogicalSize};

/// Identifies a texture in memory.
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct TextureId(u64);

impl TextureId {
    /// Get the texture id for the default font.
    pub const fn default_font() -> Self {
        Self(0)
    }
    /// Get the texture id for the default cursors.
    pub const fn default_cursors() -> Self {
        Self(1)
    }
    /// Get the next texture id.
    pub fn next() -> Self {
        static NEXT: atomic::AtomicU64 = atomic::AtomicU64::new(2);

        Self(NEXT.fetch_add(1, atomic::Ordering::SeqCst))
    }
}

impl fmt::Display for TextureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TextureId#{}", self.0)
    }
}

impl From<u64> for TextureId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
pub enum Blending {
    #[default]
    Alpha,
    Constant,
}

#[derive(Debug)]
pub enum Paint {
    /// Paint a 2D shape.
    Shape {
        transform: Transform,
        /// Vertices.
        vertices: Vec<gfx::shape2d::Vertex>,
        /// Target framebuffer to paint on. If not set, paints to the screen.
        target: Option<TextureId>,
    },
    /// Paint a sprite.
    Sprite {
        transform: Transform,
        /// Vertices.
        vertices: Vec<gfx::sprite2d::Vertex>,
        /// Texture to draw texels from.
        texture: TextureId,
        /// Target framebuffer to paint on. If not set, paints to the screen.
        target: Option<TextureId>,
    },
}

/// Render effect.
#[derive(Debug)]
pub enum Effect {
    /// Paint.
    Paint { paint: Paint, blending: Blending },
    /// Clear a texture.
    Clear { id: TextureId, color: Rgba8 },
    /// Load a texture.
    Texture {
        id: TextureId,
        image: Image,
        offscreen: bool,
    },
    /// Resize a texture.
    Resize { id: TextureId, size: Size<u32> },
    /// Upload data to a texture.
    Upload { id: TextureId, texels: Arc<[Rgba8]> },
}

impl From<Paint> for Effect {
    fn from(paint: Paint) -> Self {
        Self::Paint {
            paint,
            blending: Blending::default(),
        }
    }
}

pub trait TextureStore {
    fn put(&mut self, texture_id: TextureId, image: Image);
}

impl TextureStore for HashMap<TextureId, Image> {
    fn put(&mut self, texture_id: TextureId, image: Image) {
        self.insert(texture_id, image);
    }
}

/// Renderer trait for all render surfaces.
pub trait Renderer: Sized {
    type Error;

    fn new(
        win: &mut platform::backend::Window,
        win_size: LogicalSize,
        win_scale: f64,
        ui_scale: f32,
    ) -> Result<Self, Self::Error>;

    fn frame<E, T>(
        &mut self,
        effects: E,
        cursor: gfx::cursor2d::Sprite,
        store: &mut T,
    ) -> Result<(), Self::Error>
    where
        E: Iterator<Item = Effect>,
        T: TextureStore;

    fn scale(&mut self, factor: f32) -> f32;
    fn handle_scale_factor_changed(&mut self, scale_factor: f64);
}
