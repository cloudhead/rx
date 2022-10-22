use crate::ui::widgets::Click;
use crate::ui::*;

use super::Controller;

pub struct Button<T> {
    child: Pod<T, Box<dyn Widget<T>>>,
    controller: Click<T>,
}

impl<T> Button<T> {
    pub fn new(
        child: impl Widget<T> + 'static,
        on_click: impl Fn(&Context<'_>, &mut T) + 'static,
    ) -> Self {
        Self {
            child: Pod::new(Box::new(child)),
            controller: Click::new(on_click),
        }
    }
}

impl<T> Widget<T> for Button<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.child.layout(parent, ctx, data, env)
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        self.child.paint(canvas, data);
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.child.update(delta, ctx, data);
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        self.controller.event(&mut self.child, event, ctx, data)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        self.child.lifecycle(lifecycle, ctx, data, env)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.child.cursor()
    }

    fn contains(&self, point: Point) -> bool {
        self.child.contains(point)
    }

    fn display(&self) -> String {
        format!("Button({})", self.child.display())
    }
}
