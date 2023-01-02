use std::fmt;
use std::ops::{ControlFlow, Deref};
use std::time;

use crate::ui::canvas::*;
use crate::ui::*;

pub struct Pod<T, W> {
    pub id: WidgetId,
    pub size: Size,
    pub offset: Offset,
    pub hot: bool,
    pub active: bool,

    widget: W,
    data: PhantomData<T>,
}

impl<T, W: Widget<T>> Pod<T, W> {
    pub fn new(widget: W) -> Self {
        Self {
            id: WidgetId::next(),
            size: Size::ZERO,
            offset: Offset::ZERO,
            hot: false,
            active: false,
            widget,
            data: PhantomData,
        }
    }

    fn context<'a>(&self, parent: &'a Context<'_>) -> Context<'a> {
        parent.offset(self.offset).hot(self.hot).active(self.active)
    }

    fn bounds(&self) -> Rect<f32> {
        Rect::origin(self.size)
    }

    fn transform(&self) -> Transform {
        Transform::translate(self.offset)
    }
}

impl<T, W: Widget<T>> fmt::Display for Pod<T, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.widget.type_name();
        // .split("::")
        // .last()
        // .unwrap_or("Widget");
        write!(f, "{}#{}", name, self.id)
    }
}

impl<T, W: Widget<T>> Widget<T> for Pod<T, W> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.size = self.widget.layout(parent, ctx, data, env);
        self.size
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, data: &T) {
        self.widget.paint(canvas.transform(self.transform()), data);

        if false {
            canvas.paint(
                Rectangle::new(self.offset, self.size)
                    .stroke(1., Rgba8::GREEN.alpha(0x44))
                    .fill(Rgba8::GREEN.alpha(if self.hot { 0x22 } else { 0x11 })),
            );
            // canvas.paint(
            //     Text::new(format!(
            //         "#{} {}x{}",
            //         // self.widget.type_name().split("::").last().unwrap(),
            //         self.id,
            //         self.size.w,
            //         self.size.h
            //     ))
            //     .color(Rgba8::GREEN.alpha(0xff))
            //     .transform(Transform::scale(0.5) * self.transform())
            //     .offset([2., 2.]),
            // );
        }
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.widget.update(delta, &self.context(ctx), data)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.widget.cursor()
    }

    fn hw_cursor(&self) -> Option<&'static str> {
        dbg!(self.widget.hw_cursor())
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        let ctx = self.context(ctx);

        match event {
            WidgetEvent::MouseEnter(point) => {
                let cursor = point.untransform(self.transform());
                let contains = self.bounds().contains(cursor) && self.widget.contains(cursor);

                if contains {
                    self.hot = true;
                    self.widget
                        .event(&WidgetEvent::MouseEnter(cursor), &ctx, data)
                } else {
                    ControlFlow::Continue(())
                }
            }
            WidgetEvent::MouseExit => {
                if self.hot {
                    self.hot = false;
                    self.widget.event(&WidgetEvent::MouseExit, &ctx, data)
                } else {
                    ControlFlow::Continue(())
                }
            }
            WidgetEvent::MouseMove(point) => {
                let cursor = point.untransform(self.transform());
                let contains = self.bounds().contains(cursor) && self.widget.contains(cursor);

                if contains {
                    // If the widget wasn't hot before, we send a `MouseOver`.
                    if self.hot {
                        self.widget
                            .event(&WidgetEvent::MouseMove(cursor), &ctx, data)
                    } else {
                        self.hot = true;
                        self.widget
                            .event(&WidgetEvent::MouseEnter(cursor), &ctx, data)
                    }
                } else if self.hot {
                    self.hot = false;
                    self.widget.event(&WidgetEvent::MouseExit, &ctx, data)
                } else {
                    ControlFlow::Continue(())
                }
            }
            WidgetEvent::MouseDown(_) => {
                log::debug!(target: "ui", "{} #{} MouseDown (hot = {})", self.display(), self.id, self.hot);

                // Only propagate event if hot.
                if self.hot {
                    self.active = true;
                    self.widget.event(event, &ctx, data)
                } else {
                    ControlFlow::Continue(())
                }
            }
            WidgetEvent::MouseUp(_) => {
                // Only propagate event if active.
                if self.active {
                    log::debug!(target: "ui", "{} MouseUp (active)", self.display());

                    // It may look wrong that we're setting active to `false`
                    // here while telling our widget that we're active, but
                    // it's not! We're active until
                    self.active = false;
                    self.widget.event(event, &ctx.active(true), data)
                } else {
                    log::debug!(target: "ui", "{} MouseUp (inactive)", self.display());

                    ControlFlow::Continue(())
                }
            }
            _ => self.widget.event(event, &ctx, data),
        }
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
        self.widget.contains(point - self.offset)
    }

    fn type_name(&self) -> &'static str {
        self.widget.type_name()
    }

    fn display(&self) -> String {
        self.widget.display()
    }
}

impl<T, W> Deref for Pod<T, W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.widget
    }
}
