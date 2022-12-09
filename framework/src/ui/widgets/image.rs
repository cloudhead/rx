use crate::ui::*;

pub enum Image {
    ById(TextureId, TextureInfo),
    ByName(&'static str),
}

impl Image {
    pub fn texture(id: TextureId, info: TextureInfo) -> Self {
        Self::ById(id, info)
    }

    pub fn named(name: &'static str) -> Self {
        Self::ByName(name)
    }
}

impl<T> Widget<T> for Image {
    fn layout(&mut self, _parent: Size, _ctx: &LayoutCtx<'_>, _data: &T, _env: &Env) -> Size {
        if let Self::ById(_, info) = self {
            return info.size.into();
        }
        Size::ZERO
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, _data: &T) {
        if let Self::ById(id, _) = self {
            canvas.paint(Paint::texture(id, &canvas));
        }
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        _ctx: &Context<'_>,
        _data: &T,
        env: &Env,
    ) {
        match lifecycle {
            WidgetLifecycle::Initialized(textures) => {
                if let Self::ByName(name) = self {
                    if let Some(id) = env.get(env::Key::<TextureId>::new(name)) {
                        if let Some(info) = textures.get(&id) {
                            *self = Self::ById(id, *info);
                        }
                    }
                }
            }
        }
    }
}
