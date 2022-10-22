use super::*;

#[derive(Debug, Copy, Clone)]
pub struct LayoutCtx<'a> {
    pub fonts: &'a HashMap<text::FontId, text::Font>,
}

impl<'a> LayoutCtx<'a> {
    pub fn new(fonts: &'a HashMap<text::FontId, text::Font>) -> Self {
        Self { fonts }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Context<'a> {
    pub transform: Transform,
    pub cursor: Point,
    pub surfaces: &'a HashMap<TextureId, Image>,
    pub hot: bool,
    pub active: bool,
}

impl<'a> Context<'a> {
    pub fn new(cursor: Point, surfaces: &'a HashMap<TextureId, Image>) -> Self {
        Self {
            transform: Transform::identity(),
            cursor,
            surfaces,
            hot: false,
            active: false,
        }
    }

    pub fn offset(self, offset: Offset) -> Self {
        self.transform(Transform::translate(offset))
    }

    pub fn hot(self, hot: bool) -> Self {
        Self { hot, ..self }
    }

    pub fn active(self, active: bool) -> Self {
        Self { active, ..self }
    }

    pub fn transform(self, t: impl Into<Transform>) -> Self {
        let t = t.into();
        let transform = self.transform * t;

        Self {
            transform,
            cursor: self.cursor.untransform(t),
            ..self
        }
    }

    pub fn is_hot(&self) -> bool {
        self.hot
    }
}
