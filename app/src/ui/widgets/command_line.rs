use std::ops::ControlFlow;
use std::time;

use crate::app::{Mode, Session};
use crate::framework::platform::Key;
use crate::framework::ui::text;
use crate::framework::ui::text::Text;
use crate::framework::ui::*;
use crate::gfx::prelude::*;

#[derive(Default)]
pub struct CommandLine {}

impl Widget<Session> for CommandLine {
    fn layout(
        &mut self,
        parent: Size,
        _ctx: &LayoutCtx<'_>,
        _session: &Session,
        _env: &Env,
    ) -> Size {
        Size::new(parent.w, 16.)
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, session: &Session) {
        if session.mode == Mode::Command {
            let input = format!(":{}_", session.cmdline.input());
            canvas.paint(Paint::text(input, text::FontId::default(), &canvas));
        } else if let Some(msg) = &session.cmdline.message {
            canvas.paint(Text::new(msg).color(msg.color()));
        }
    }

    fn update(&mut self, _delta: time::Duration, _ctx: &Context<'_>, _session: &Session) {}

    fn event(
        &mut self,
        event: &WidgetEvent,
        _ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        if session.mode == Mode::Command {
            match event {
                WidgetEvent::KeyDown { key, .. } => {
                    match key {
                        Key::Up => {
                            session.cmdline.history_prev();
                        }
                        Key::Down => {
                            session.cmdline.history_next();
                        }
                        Key::Left => {
                            session.cmdline.cursor_backward();
                        }
                        Key::Right => {
                            session.cmdline.cursor_forward();
                        }
                        Key::Tab => {
                            session.cmdline.completion_next();
                        }
                        Key::Backspace => {
                            if session.cmdline.is_empty() {
                                session.prev_mode();
                            } else {
                                session.cmdline.delc();
                            }
                        }
                        Key::Return => {
                            let input = session.cmdline.input();
                            // Always hide the command line before executing the command,
                            // because commands will often require being in a specific mode, eg.
                            // visual mode for commands that run on selections.
                            session.prev_mode();

                            if input.is_empty() {
                                return ControlFlow::Continue(());
                            }

                            match session.cmdline.parse(&input) {
                                Err(e) => {
                                    session.cmdline.error(e);
                                }
                                Ok(cmd) => {
                                    if let Err(e) = session.command(cmd) {
                                        session.cmdline.error(e);
                                    }
                                    session.cmdline.history.add(input);
                                }
                            }
                        }
                        Key::Escape => {
                            session.prev_mode();
                        }
                        Key::Home => {
                            session.cmdline.cursor_back();
                        }
                        Key::End => {
                            session.cmdline.cursor_front();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        ControlFlow::Continue(())
    }

    fn contains(&self, _point: Point) -> bool {
        false
    }
}
