use crate::gfx::color::Rgba;
use crate::gfx::math::*;
use crate::gfx::rect::Rect;
use crate::gfx::ZDepth;
use crate::gfx::{Repeat, Rgba8};

///////////////////////////////////////////////////////////////////////////
// Vertex
///////////////////////////////////////////////////////////////////////////

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Vertex {
    pub position: Vector3<f32>,
    pub uv: Vector2<f32>,
    pub color: Rgba8,
    pub opacity: f32,
}

impl Vertex {
    fn new(x: f32, y: f32, z: f32, u: f32, v: f32, color: Rgba8, opacity: f32) -> Self {
        Self {
            position: Vector3::new(x, y, z),
            uv: Vector2::new(u, v),
            color,
            opacity,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Sprite
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, Default)]
pub struct Sprite {
    pub src: Rect<f32>,
    pub dst: Rect<f32>,
    pub zdepth: ZDepth,
    pub color: Rgba,
    pub alpha: f32,
    pub repeat: Repeat,
}

impl Sprite {
    pub fn new(src: Rect<f32>, dst: Rect<f32>) -> Self {
        Self {
            src,
            dst,
            ..Default::default()
        }
    }

    pub fn color<T: Into<Rgba>>(mut self, color: T) -> Self {
        self.color = color.into();
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn zdepth<T: Into<ZDepth>>(mut self, zdepth: T) -> Self {
        self.zdepth = zdepth.into();
        self
    }

    pub fn repeat(mut self, x: f32, y: f32) -> Self {
        self.repeat = Repeat::new(x, y);
        self
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Batch
///////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct Batch {
    pub w: u32,
    pub h: u32,
    pub size: usize,

    items: Vec<Sprite>,
}

impl Batch {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            items: Vec::new(),
            size: 0,
        }
    }

    pub fn singleton(
        w: u32,
        h: u32,
        src: Rect<f32>,
        dst: Rect<f32>,
        zdepth: ZDepth,
        rgba: Rgba,
        alpha: f32,
        repeat: Repeat,
    ) -> Self {
        let mut view = Self::new(w, h);
        view.push(
            Sprite::new(src, dst)
                .zdepth(zdepth)
                .color(rgba)
                .alpha(alpha)
                .repeat(repeat.x, repeat.y),
        );
        view
    }

    pub fn push(&mut self, sprite: Sprite) {
        self.items.push(sprite);
    }

    pub fn add(
        &mut self,
        src: Rect<f32>,
        dst: Rect<f32>,
        depth: ZDepth,
        rgba: Rgba,
        alpha: f32,
        repeat: Repeat,
    ) {
        if repeat != Repeat::default() {
            assert!(
                src == Rect::origin(self.w as f32, self.h as f32),
                "using texture repeat is only valid when using the entire {}x{} texture",
                self.w,
                self.h
            );
        }
        self.items.push(
            Sprite::new(src, dst)
                .zdepth(depth)
                .color(rgba)
                .alpha(alpha)
                .repeat(repeat.x, repeat.y),
        );
        self.size += 1;
    }

    pub fn vertices(&self) -> Vec<Vertex> {
        let mut buf = Vec::with_capacity(6 * self.items.len());

        for Sprite {
            src,
            dst,
            zdepth,
            color,
            alpha,
            repeat,
        } in self.items.iter()
        {
            let ZDepth(z) = zdepth;
            let re = repeat;

            // Relative texture coordinates
            let rx1: f32 = src.x1 / self.w as f32;
            let ry1: f32 = src.y1 / self.h as f32;
            let rx2: f32 = src.x2 / self.w as f32;
            let ry2: f32 = src.y2 / self.h as f32;

            let c: Rgba8 = (*color).into();

            // TODO: Use an index buffer
            buf.extend_from_slice(&[
                Vertex::new(dst.x1, dst.y1, *z, rx1 * re.x, ry2 * re.y, c, *alpha),
                Vertex::new(dst.x2, dst.y1, *z, rx2 * re.x, ry2 * re.y, c, *alpha),
                Vertex::new(dst.x2, dst.y2, *z, rx2 * re.x, ry1 * re.y, c, *alpha),
                Vertex::new(dst.x1, dst.y1, *z, rx1 * re.x, ry2 * re.y, c, *alpha),
                Vertex::new(dst.x1, dst.y2, *z, rx1 * re.x, ry1 * re.y, c, *alpha),
                Vertex::new(dst.x2, dst.y2, *z, rx2 * re.x, ry1 * re.y, c, *alpha),
            ]);
        }
        buf
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.size = 0;
    }

    pub fn offset(&mut self, x: f32, y: f32) {
        for sprite in self.items.iter_mut() {
            sprite.dst += Vector2::new(x, y);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        let mut batch = Batch::new(32, 32);
        batch.push(
            Sprite::new(Rect::origin(32., 32.), Rect::new(32., 32., 64., 64.))
                .color(Rgba::BLUE)
                .alpha(0.5)
                .zdepth(0.1)
                .repeat(8., 8.),
        );
    }
}
