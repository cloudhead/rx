use std::{ops::ControlFlow, time};

use crate::ui::*;

pub struct SizedBox<T> {
    widget: Pod<T, Box<dyn Widget<T>>>,
    size: Size<f32>,
}

impl<T> SizedBox<T> {
    pub fn new(widget: impl Widget<T> + 'static) -> Self {
        Self {
            widget: Pod::new(Box::new(widget)),
            size: Size::default(),
        }
    }

    /// Set container's width.
    pub fn width(mut self, width: f32) -> Self {
        self.size.w = width;
        self
    }

    /// Set container's height.
    pub fn height(mut self, height: f32) -> Self {
        self.size.h = height;
        self
    }
}

impl<T> Widget<T> for SizedBox<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.widget.layout(
            Size::new(self.size.w.min(parent.w), self.size.h.min(parent.h)),
            ctx,
            data,
            env,
        )
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, data: &T) {
        self.widget.paint(canvas.resize(self.widget.size), data);
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.widget.update(delta, ctx, data);
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        self.widget.event(event, ctx, data)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        self.widget.lifecycle(lifecycle, ctx, data, env)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.widget.cursor()
    }

    fn contains(&self, point: Point) -> bool {
        Rect::<f32>::origin(self.size).contains(point)
    }

    fn display(&self) -> String {
        format!(
            "SizedBox[{}, {}]({})",
            self.size.w,
            self.size.h,
            self.widget.display()
        )
    }
}
