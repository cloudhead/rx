use crate::ui::*;

pub trait Controller<T, W: Widget<T>> {
    /// Analogous to [`Widget::event`].
    fn event(
        &mut self,
        child: &mut W,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        data: &mut T,
    ) -> ControlFlow<()> {
        child.event(event, ctx, data)
    }

    /// Analogous to [`Widget::lifecycle`].
    fn lifecycle(
        &mut self,
        child: &mut W,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        child.lifecycle(lifecycle, ctx, data, env)
    }

    /// Analogous to [`Widget::update`].
    fn update(&mut self, child: &mut W, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        child.update(delta, ctx, data)
    }

    /// Analogous to [`Widget::frame`].
    fn frame(&mut self, child: &mut W, surfaces: &Surfaces, data: &mut T) {
        child.frame(surfaces, data)
    }
}

/// A [`Widget`] that manages a child and a [`Controller`].
pub struct Control<W, C> {
    widget: W,
    controller: C,
}

impl<W, C> Control<W, C> {
    pub fn new(widget: W, controller: C) -> Control<W, C> {
        Control { widget, controller }
    }
}

impl<T, W: Widget<T>, C: Controller<T, W>> Widget<T> for Control<W, C> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.widget.layout(parent, ctx, data, env)
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        self.widget.paint(canvas, data)
    }

    fn contains(&self, point: Point) -> bool {
        self.widget.contains(point)
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.controller.update(&mut self.widget, delta, ctx, data)
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        self.controller.event(&mut self.widget, event, ctx, data)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        self.controller
            .lifecycle(&mut self.widget, lifecycle, ctx, data, env)
    }

    fn frame(&mut self, surfaces: &Surfaces, data: &mut T) {
        self.controller.frame(&mut self.widget, surfaces, data)
    }

    fn display(&self) -> String {
        format!("Control({})", self.widget.display())
    }
}
