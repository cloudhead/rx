#![allow(unused)]
use std::ops::ControlFlow;
use std::time;

use rx_framework::ui::widgets::ZStack;

use crate::app::brush;
use crate::app::{Session, Tool};
use crate::framework::ui::canvas::Canvas;
use crate::framework::ui::cursor::CursorStyle;
use crate::framework::ui::widgets::Pod;
use crate::framework::ui::{
    Context, Env, LayoutCtx, Surfaces, Widget, WidgetEvent, WidgetLifecycle,
};
use crate::gfx::prelude::*;

/// Root of the widget tree.
pub struct Root {
    widgets: ZStack<Session>,
    cursor: CursorStyle,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            widgets: ZStack::new(),
            cursor: CursorStyle::default(),
        }
    }
}

impl Root {
    pub fn child(mut self, widget: impl Widget<Session> + 'static) -> Self {
        Self {
            widgets: self.widgets.push(widget),
            cursor: self.cursor,
        }
    }
}

impl Widget<Session> for Root {
    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, session: &Session) {
        self.widgets.update(delta, ctx, session);
    }

    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, session: &Session, env: &Env) -> Size {
        self.widgets.layout(parent, ctx, session, env)
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, session: &Session) {
        self.widgets.paint(canvas.clone(), session)
    }

    fn event(
        &mut self,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::Resized(size) => {
                session.handle_resize(*size);
            }
            WidgetEvent::CharacterReceived(c, mods) => {
                session.handle_received_character(*c, *mods);
            }
            WidgetEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                session.handle_key_down(*key, *modifiers, *repeat);
            }
            WidgetEvent::KeyUp { key, modifiers } => {
                session.handle_key_up(*key, *modifiers);
            }
            WidgetEvent::MouseDown(input) => {
                session.handle_mouse_down(*input);
            }
            WidgetEvent::MouseUp(input) => {
                session.handle_mouse_up(*input);
            }
            WidgetEvent::MouseMove(point) => {
                session.handle_cursor_moved(*point);
            }
            WidgetEvent::Tick(delta) => {
                session.update(*delta);
            }
            _ => {}
        }

        if let flow @ ControlFlow::Break(_) = self.widgets.event(event, ctx, session) {
            return flow;
        }

        match session.tool {
            Tool::Pan { panning: true } => {
                self.cursor = CursorStyle::Grab;
            }
            Tool::Pan { panning: false } => {
                self.cursor = CursorStyle::Hand;
            }
            Tool::Brush if session.brush.is_mode(brush::Mode::Erase) => {
                self.cursor = CursorStyle::Pointer;
            }
            Tool::Brush if session.brush.is_mode(brush::Mode::Normal) => {
                self.cursor = CursorStyle::Pointer;
            }
            _ => {
                self.cursor = CursorStyle::Pointer;
            }
        }

        ControlFlow::Continue(())
    }

    fn contains(&self, point: Point) -> bool {
        self.widgets.contains(point)
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        session: &Session,
        env: &Env,
    ) {
        self.widgets.lifecycle(lifecycle, ctx, session, env);
    }

    fn frame(&mut self, surfaces: &Surfaces, session: &mut Session) {
        self.widgets.frame(surfaces, session);
    }

    fn cursor(&self) -> Option<CursorStyle> {
        self.widgets.cursor().or(Some(self.cursor))
    }
}
