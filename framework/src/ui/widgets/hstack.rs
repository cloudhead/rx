use std::ops::ControlFlow;
use std::time;

use crate::gfx::prelude::*;
use crate::ui::*;

pub struct HStack<T> {
    pub size: Size,
    pub spacing: f32,
    pub children: Vec<Pod<T, Box<dyn Widget<T>>>>,
}

impl<T> Default for HStack<T> {
    fn default() -> Self {
        Self {
            size: Size::default(),
            spacing: 0.,
            children: Vec::default(),
        }
    }
}

impl<T> HStack<T> {
    pub fn new(children: Vec<Box<dyn Widget<T>>>) -> Self {
        Self {
            size: Size::default(),
            spacing: 0.,
            children: children.into_iter().map(|c| Pod::new(c)).collect(),
        }
    }

    pub fn push(&mut self, child: impl Widget<T> + 'static) {
        self.children.push(Pod::new(Box::new(child)));
    }

    pub fn child(mut self, child: impl Widget<T> + 'static) -> Self {
        self.push(child);
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn bounds(&self) -> Rect<f32> {
        Rect::origin(self.size)
    }
}

impl<T> Widget<T> for HStack<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        let mut offset = Vector::ZERO;
        let mut height: f32 = 0.;

        for widget in &mut self.children {
            widget.layout(parent, ctx, data, env);
            height = height.max(widget.size.h);

            if offset.x + widget.size.w > parent.w {
                offset.y += widget.size.h;
                offset.x = 0.;

                height += widget.size.h;
            }
            widget.offset = offset;
            offset.x += widget.size.w + self.spacing;
        }
        self.size = Size::new(offset.x - self.spacing, height);
        self.size
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, data: &T) {
        for widget in &mut self.children {
            widget.paint(canvas.clone(), data);
        }
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        for widget in &mut self.children {
            widget.update(delta, ctx, data);
        }
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        for widget in &mut self.children {
            if let flow @ ControlFlow::Break(_) = widget.event(event, ctx, data) {
                return flow;
            }
        }
        ControlFlow::Continue(())
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        data: &T,
        env: &Env,
    ) {
        for widget in &mut self.children {
            widget.lifecycle(lifecycle, ctx, data, env);
        }
    }

    fn contains(&self, point: Point) -> bool {
        self.bounds().contains(point)
        // self.children.iter().any(|w| w.contains(point))
    }

    fn cursor(&self) -> Option<CursorStyle> {
        for widget in &self.children {
            if widget.hot {
                return widget.cursor();
            }
        }
        None
    }

    fn display(&self) -> String {
        format!("HStack({})", self.children.len())
    }
}

pub fn hstack<T>(children: Vec<Box<dyn Widget<T>>>) -> HStack<T> {
    HStack::new(children)
}
