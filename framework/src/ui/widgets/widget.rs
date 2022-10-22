use crate::gfx::prelude::*;
use crate::ui::*;

/// A UI widget that can be painted on screen.
#[allow(unused_variables)]
pub trait Widget<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        parent
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {}

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {}

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
    }

    fn frame(&mut self, surfaces: &Surfaces, data: &mut T) {}

    fn cursor(&self) -> Option<CursorStyle> {
        None
    }

    fn contains(&self, point: Point) -> bool {
        true
    }

    fn display(&self) -> String {
        self.type_name().to_owned()
    }

    #[doc(hidden)]
    /// Get the identity of the widget; this is basically only implemented by
    /// `IdentityWrapper`. Widgets should not implement this on their own.
    fn id(&self) -> Option<WidgetId> {
        None
    }

    #[doc(hidden)]
    /// Get the (verbose) type name of the widget for debugging purposes.
    /// You should not override this method.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl<T> Widget<T> for Box<dyn Widget<T>> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.deref_mut().layout(parent, ctx, data, env)
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.deref_mut().update(delta, ctx, data)
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        self.deref_mut().paint(canvas, data)
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        self.deref_mut().event(event, ctx, data)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        self.deref_mut().lifecycle(lifecycle, ctx, data, env)
    }

    fn frame(&mut self, surfaces: &Surfaces, data: &mut T) {
        self.deref_mut().frame(surfaces, data)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.deref().cursor()
    }

    fn contains(&self, point: Point) -> bool {
        self.deref().contains(point)
    }

    fn display(&self) -> String {
        self.deref().display()
    }
}

impl<T> Widget<T> for Rgba8 {
    fn paint(&mut self, mut canvas: Canvas<'_>, _data: &T) {
        canvas.fill(canvas.bounds(), *self);
    }

    fn display(&self) -> String {
        self.to_string()
    }
}
