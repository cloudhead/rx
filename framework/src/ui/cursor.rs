use std::ops::Deref;

use crate::gfx::cursor2d;
use crate::gfx::prelude::*;

const SIZE: Size = Size::new(16., 16.);

#[derive(Default, Debug, Copy, Clone)]
pub struct Cursor {
    pub style: CursorStyle,
    pub origin: Point,
}

impl From<Point> for Cursor {
    fn from(origin: Point) -> Self {
        Self {
            style: CursorStyle::default(),
            origin,
        }
    }
}

impl Deref for Cursor {
    type Target = Point;

    fn deref(&self) -> &Self::Target {
        &self.origin
    }
}

impl Cursor {
    pub fn sprite(&self, size: impl Into<Size<u32>>) -> cursor2d::Sprite {
        let size = size.into();
        let info = self.style.info();

        cursor2d::Sprite::new(
            size,
            info.rect,
            info.rect.with_origin(self.origin) + info.offset,
        )
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum CursorStyle {
    #[default]
    Pointer,
    Hand,
    Grab,
    Sampler,
    Crosshair,
    Omni,
    Erase,
    Flood,
}

impl CursorStyle {
    pub fn info(&self) -> Info {
        match self {
            Self::Pointer => Info::new([96., 0.], SIZE, -5., -1., false),
            Self::Hand => Info::new([48., 0.], SIZE, -5., -1., false),
            Self::Grab => Info::new([112., 0.], SIZE, -5., -1., false),
            Self::Sampler => Info::new([0., 0.], SIZE, -2., -15., false),
            Self::Crosshair => Info::new([16., 0.], SIZE, -8., -8., true),
            Self::Omni => Info::new([32., 0.], SIZE, -8., -8., false),
            Self::Erase => Info::new([64., 0.], SIZE, -8., -8., true),
            Self::Flood => Info::new([80., 0.], SIZE, -8., -8., false),
        }
    }
}

pub struct Info {
    pub rect: Rect<f32>,
    pub offset: Vector2D<f32>,
    pub invert: bool,
}

impl Info {
    fn new(
        origin: impl Into<Point2D<f32>>,
        size: Size<f32>,
        off_x: f32,
        off_y: f32,
        invert: bool,
    ) -> Self {
        Self {
            rect: Rect {
                origin: origin.into(),
                size,
            },
            offset: Vector2D::new(off_x, off_y),
            invert,
        }
    }
}
