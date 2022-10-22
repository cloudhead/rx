use crate::gfx::color::Rgba;
use crate::gfx::math::rect::{Box2D, Rect};
use crate::gfx::math::*;
use crate::gfx::ZDepth;
use crate::gfx::{Repeat, Rgba8};

///////////////////////////////////////////////////////////////////////////
// Vertex
///////////////////////////////////////////////////////////////////////////

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Vertex {
    pub position: Vector3D<f32>,
    pub uv: Vector2D<f32>,
    pub color: Rgba8,
    pub opacity: f32,
}

impl Vertex {
    fn new(x: f32, y: f32, z: f32, u: f32, v: f32, color: Rgba8, opacity: f32) -> Self {
        Self {
            position: Vector3D::new(x, y, z),
            uv: Vector2D::new(u, v),
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
    pub fn new(size: impl Into<Size<u32>>) -> Self {
        let size = size.into();

        Self {
            w: size.w,
            h: size.h,
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
        let mut view = Self::new([w, h]);
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

    pub fn item(
        mut self,
        src: Rect<f32>,
        dst: Rect<f32>,
        depth: ZDepth,
        rgba: Rgba,
        alpha: f32,
        repeat: Repeat,
    ) -> Self {
        self.add(src, dst, depth, rgba, alpha, repeat);
        self
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
                src == Rect::origin(Size::new(self.w as f32, self.h as f32)),
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
            let src = Box2D::from(src);
            let dst = Box2D::from(dst);
            let ZDepth(z) = zdepth;
            let re = repeat;

            // Relative texture coordinates
            let rx1: f32 = src.min.x / self.w as f32;
            let ry1: f32 = src.min.y / self.h as f32;
            let rx2: f32 = src.max.x / self.w as f32;
            let ry2: f32 = src.max.y / self.h as f32;

            let c: Rgba8 = (*color).into();

            // TODO: Use an index buffer
            buf.extend_from_slice(&[
                Vertex::new(dst.min.x, dst.min.y, *z, rx1 * re.x, ry1 * re.y, c, *alpha),
                Vertex::new(dst.max.x, dst.max.y, *z, rx2 * re.x, ry2 * re.y, c, *alpha),
                Vertex::new(dst.max.x, dst.min.y, *z, rx2 * re.x, ry1 * re.y, c, *alpha),
                Vertex::new(dst.min.x, dst.min.y, *z, rx1 * re.x, ry1 * re.y, c, *alpha),
                Vertex::new(dst.max.x, dst.max.y, *z, rx2 * re.x, ry2 * re.y, c, *alpha),
                Vertex::new(dst.min.x, dst.max.y, *z, rx1 * re.x, ry2 * re.y, c, *alpha),
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
            sprite.dst += Vector2D::new(x, y);
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
        let mut batch = Batch::new([32, 32]);
        batch.push(
            Sprite::new(
                Rect::origin(Size::new(32., 32.)),
                Rect::new(Point::new(32., 32.), Size::new(64., 64.)),
            )
            .color(Rgba::BLUE)
            .alpha(0.5)
            .zdepth(0.1)
            .repeat(8., 8.),
        );
    }
}
