use crate::ui::*;

pub struct Align<T> {
    widget: Pod<T, Box<dyn Widget<T>>>,
    size: Size,
    position: Position,
}

impl<T> Align<T> {
    pub fn new(widget: impl Widget<T> + 'static) -> Self {
        Self {
            widget: Pod::new(Box::new(widget)),
            size: Size::ZERO,
            position: Position::default(),
        }
    }

    pub fn position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }

    pub fn left(mut self, offset: f32) -> Self {
        self.position = self.position.left(offset);
        self
    }

    pub fn right(mut self, offset: f32) -> Self {
        self.position = self.position.right(offset);
        self
    }

    pub fn top(mut self, offset: f32) -> Self {
        self.position = self.position.top(offset);
        self
    }

    pub fn bottom(mut self, offset: f32) -> Self {
        self.position = self.position.bottom(offset);
        self
    }
}

impl<T> Widget<T> for Align<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.size = self.widget.layout(parent, ctx, data, env);

        let mut x = (parent.w - self.widget.size.w) / 2.;
        let mut y = (parent.h - self.widget.size.h) / 2.;

        if let Some(top) = self.position.top {
            y = top;
        } else if let Some(bottom) = self.position.bottom {
            y = parent.h - bottom - self.widget.size.h;
        }

        if let Some(left) = self.position.left {
            x = left;
        } else if let Some(right) = self.position.right {
            x = parent.w - right - self.widget.size.w;
        }

        self.widget.offset = Offset::new(x, y);
        // self.size
        parent
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        self.widget.paint(canvas, data);
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

    fn frame(&mut self, surfaces: &Surfaces, data: &mut T) {
        self.widget.frame(surfaces, data);
    }

    fn contains(&self, point: Point) -> bool {
        self.widget.contains(point)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        if self.widget.hot {
            self.widget.cursor()
        } else {
            None
        }
    }

    fn hw_cursor(&self) -> Option<&'static str> {
        if self.widget.hot {
            self.widget.hw_cursor()
        } else {
            None
        }
    }

    fn display(&self) -> String {
        format!("Align({})", self.widget.display())
    }
}

pub fn align<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    Align::new(widget)
}

pub fn center<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    align(widget)
}

pub fn top<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    align(widget).top(0.)
}

pub fn left<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    align(widget).left(0.)
}

pub fn bottom<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    align(widget).bottom(0.)
}

pub fn right<T, W: Widget<T> + 'static>(widget: W) -> Align<T> {
    align(widget).right(0.)
}
