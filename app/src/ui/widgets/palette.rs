use std::ops::ControlFlow;
use std::time;

use crate::app::{brush, Session, Tool};
use crate::framework::ui::event::WidgetEvent;
use crate::framework::ui::widgets::HStack;
use crate::framework::ui::CursorStyle;
use crate::framework::ui::*;
use crate::gfx::prelude::*;

#[derive(Default)]
pub struct Swatch {
    pub color: Rgba8,
    pub size: Size,
    pub hot: bool,
}

impl Widget<Session> for Swatch {
    fn layout(&mut self, _parent: Size, _ctx: &LayoutCtx<'_>, _data: &Session, _env: &Env) -> Size {
        self.size
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, _data: &Session) {
        let stroke = if self.hot { 1. } else { 0. };

        canvas.paint(
            rectangle([0., 0.], self.size)
                .stroke(stroke, Rgba8::WHITE)
                .fill(self.color),
        );
    }

    fn update(&mut self, _delta: time::Duration, _ctx: &Context<'_>, _data: &Session) {}

    fn event(
        &mut self,
        event: &WidgetEvent,
        _ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::MouseEnter { .. } => {
                self.hot = true;
                session.colors.hover = Some(self.color);
            }
            WidgetEvent::MouseExit => {
                self.hot = false;
            }
            WidgetEvent::MouseUp { .. } => {
                return ControlFlow::Break(());
            }
            WidgetEvent::MouseDown { .. } => {
                session.colors.fg = self.color;
                session.tool(Tool::Brush);

                if session.brush.mode != brush::Mode::Normal
                    && session.brush.mode != brush::Mode::Pencil
                {
                    session.brush.mode(brush::Mode::default());
                }

                return ControlFlow::Break(());
            }
            _ => {}
        }

        ControlFlow::Continue(())
    }

    fn contains(&self, point: Point) -> bool {
        Rect::<f32>::origin(self.size).contains(point)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        Some(CursorStyle::Sampler)
    }

    fn hw_cursor(&self) -> Option<&'static str> {
        Some("picker")
    }
}

pub struct Palette {
    pub swatches: HStack<Session>,
    pub hover: Option<Rgba8>,
    pub cellsize: Size,
    pub size: Size,
}

impl Palette {
    pub fn new(cellsize: Size<f32>) -> Self {
        Self {
            swatches: HStack::default(),
            hover: None,
            cellsize,
            size: Size::ZERO,
        }
    }
}

impl Widget<Session> for Palette {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, session: &Session, env: &Env) -> Size {
        self.size.h = f32::min(
            session.palette.colors.len() as f32 * self.cellsize.h,
            parent.h,
        );
        self.size.w = self.cellsize.w;
        self.swatches.layout(self.size, ctx, session, env);
        self.size
    }

    fn paint(&mut self, canvas: Canvas<'_>, session: &Session) {
        if !session.settings["ui/palette"].is_set() {
            return;
        }
        self.swatches.paint(canvas, session);
    }

    fn update(&mut self, _delta: time::Duration, _ctx: &Context<'_>, session: &Session) {
        // FIXME
        if session.palette.colors.len() == self.swatches.children.len() {
            return;
        }

        for color in session.palette.colors.iter().copied() {
            self.swatches.push(Swatch {
                color,
                hot: false,
                size: self.cellsize,
            });
        }
    }

    fn event(
        &mut self,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::MouseEnter { .. } => {}
            WidgetEvent::MouseExit => {
                session.colors.hover = None;
            }
            _ => {}
        }
        self.swatches.event(event, ctx, session)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        session: &Session,
        env: &Env,
    ) {
        self.swatches.lifecycle(lifecycle, ctx, session, env);
    }

    fn contains(&self, point: Point) -> bool {
        self.swatches.contains(point)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.swatches.cursor()
    }

    fn hw_cursor(&self) -> Option<&'static str> {
        dbg!(self.swatches.hw_cursor())
    }
}
