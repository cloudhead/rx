use std::collections::HashMap;
use std::{io, time};

use crate::gfx::prelude::*;
use crate::platform;
use crate::platform::{WindowEvent, WindowHint};
use crate::renderer;
use crate::renderer::Renderer;
use crate::timer::FrameTimer;
use crate::ui::text::{FontError, FontFormat, FontId};
use crate::ui::*;

/// Default UI scale.
pub const DEFAULT_SCALE: f32 = 2.;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Font error: {0}")]
    Font(#[from] FontError),
}

/// Application launcher.
pub struct Application {
    title: String,
    graphics: Graphics,
    env: Env,
}

impl Application {
    pub fn new(title: &str) -> Self {
        let graphics = Graphics::default();
        let env = Env::default();

        Self {
            title: title.to_owned(),
            graphics,
            env,
        }
    }

    pub fn fonts(
        mut self,
        fonts: impl IntoIterator<Item = (impl Into<FontId>, impl AsRef<[u8]>, FontFormat)>,
    ) -> Result<Self, Error> {
        for (id, data, format) in fonts {
            let id = id.into();
            log::debug!("Loading font {:?}..", id);

            self.graphics.font(id, data.as_ref(), format)?;
        }
        Ok(self)
    }

    pub fn cursors(mut self, image: Image) -> Self {
        self.graphics.texture(TextureId::default_cursors(), image);
        self
    }

    pub fn image(mut self, name: &'static str, image: Image) -> Self {
        let id = TextureId::next();

        self.graphics.texture(id, image);
        self.env.set(env::Key::<TextureId>::new(name), id);
        self
    }

    /// Launch the UI by passing in the root widget and initial data.
    pub fn launch<T>(mut self, widget: impl Widget<T> + 'static, mut data: T) -> io::Result<()> {
        let hints = &[WindowHint::Resizable(true), WindowHint::Visible(true)];
        let (mut win, mut win_events) =
            platform::init(&self.title, 640, 480, hints, platform::GraphicsContext::Gl)?;

        if win.scale_factor() != 1. {
            warn!(
                "non-standard pixel scaling factor detected: {}",
                win.scale_factor()
            );
        }

        let win_scale = 1.;
        let win_size = win.size();
        let ui_scale = DEFAULT_SCALE;

        info!("window size: {}x{}", win_size.width, win_size.height);
        info!("window scale: {}", win_scale);
        info!("ui scale: {}", ui_scale);
        info!(
            "ui size: {}x{}",
            win_size.width as f32 / ui_scale,
            win_size.height as f32 / ui_scale
        );

        let mut renderer: renderer::backends::gl::Renderer =
            Renderer::new(&mut win, win_size, win_scale, ui_scale)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let mut root: Pod<T, Box<dyn Widget<T>>> = Pod::new(Box::new(widget));
        let mut store = HashMap::new();
        let mut render_timer = FrameTimer::new();
        let mut update_timer = FrameTimer::new();
        let mut paint_timer = FrameTimer::new();
        let mut events = Vec::with_capacity(16);
        let mut last = time::Instant::now();
        let mut delta;

        // Window state.
        let mut resized = false;
        let mut hovered = false;
        let mut minimized = false;

        root.lifecycle(
            &WidgetLifecycle::Initialized(&self.graphics.textures),
            &Context::new(Point::ORIGIN, &store),
            &data,
            &self.env,
        );
        // Initial update and layout so that the first events, eg. `CursorMove` work.
        // If we don't do this, widget sizes will be zero when the first events land.
        // It's important however that in the general case, update and layout are run
        // *after* events are processed.
        root.update(
            time::Duration::ZERO,
            &Context::new(Point::ORIGIN, &store),
            &data,
        );
        root.layout(
            Size::from(win.size()) / ui_scale,
            &LayoutCtx::new(&self.graphics.fonts),
            &data,
            &self.env,
        );

        while win.is_open() {
            win_events.wait();

            println!("-----------------");

            for event in win_events.flush() {
                if event.is_input() {
                    trace!("event: {:?}", event);
                }

                match event {
                    WindowEvent::Resized(size) => {
                        if size.is_zero() {
                            // On certain operating systems, the window size will be set to
                            // zero when the window is minimized. Since a zero-sized framebuffer
                            // is not valid, we don't render anything in this case.
                            minimized = true;
                        } else {
                            minimized = false;
                            resized = true;
                        }
                    }
                    WindowEvent::CursorEntered { .. } => {
                        // events.push(WidgetEvent::CursorEntered);

                        if win.is_focused() {
                            win.set_cursor_visible(false);
                        }
                        hovered = true;
                    }
                    WindowEvent::CursorLeft { .. } => {
                        // events.push(WidgetEvent::CursorLeft);
                        win.set_cursor_visible(true);

                        hovered = false;
                    }
                    WindowEvent::Minimized => {
                        minimized = true;
                    }
                    WindowEvent::Restored => {
                        minimized = false;
                    }
                    WindowEvent::Focused(true) => {
                        if hovered {
                            win.set_cursor_visible(false);
                        }
                    }
                    WindowEvent::Focused(false) => {
                        win.set_cursor_visible(true);
                    }
                    WindowEvent::RedrawRequested => {
                        // All events currently trigger a redraw, we don't need to
                        // do anything special here.
                    }
                    WindowEvent::ScaleFactorChanged(factor) => {
                        renderer.handle_scale_factor_changed(factor);
                    }
                    WindowEvent::CloseRequested => {
                        // Ignore.
                    }
                    WindowEvent::CursorMoved { position } => {
                        events.push(WidgetEvent::MouseMove(Point::new(
                            (position.x as f32 / ui_scale).floor(),
                            (position.y as f32 / ui_scale).floor(),
                        )));
                    }
                    WindowEvent::MouseInput { state, button, .. } => match state {
                        platform::InputState::Pressed => {
                            events.push(WidgetEvent::MouseDown(button));
                        }
                        platform::InputState::Released => {
                            events.push(WidgetEvent::MouseUp(button));
                        }
                        _ => {}
                    },
                    WindowEvent::Scroll { delta, .. } => {
                        events.push(WidgetEvent::MouseScroll(delta));
                    }
                    WindowEvent::KeyboardInput(input) => {
                        // Intercept `<insert>` key for pasting.
                        //
                        // Reading from the clipboard causes the loop to wake up for some strange
                        // reason I cannot comprehend. So we only read from clipboard when we
                        // need to paste.
                        match input {
                            platform::KeyboardInput {
                                key: Some(platform::Key::Insert),
                                state: platform::InputState::Pressed,
                                modifiers: platform::ModifiersState { shift: true, .. },
                            } => events.push(WidgetEvent::Paste(win.clipboard())),

                            platform::KeyboardInput {
                                state,
                                key: Some(key),
                                modifiers,
                            } => match state {
                                platform::InputState::Pressed => {
                                    events.push(WidgetEvent::KeyDown {
                                        key,
                                        modifiers,
                                        repeat: false,
                                    });
                                }
                                platform::InputState::Repeated => {
                                    events.push(WidgetEvent::KeyDown {
                                        key,
                                        modifiers,
                                        repeat: true,
                                    });
                                }
                                platform::InputState::Released => {
                                    events.push(WidgetEvent::KeyUp { key, modifiers });
                                }
                            },
                            _ => {
                                debug!("Ignored keyboard input with unknown key: {:?}", input);
                            }
                        }
                    }
                    WindowEvent::ReceivedCharacter(c, mods) => {
                        events.push(WidgetEvent::CharacterReceived(c, mods));
                    }
                    _ => {}
                };
            }
            // If minimized, don't update or render.
            if minimized {
                continue;
            }

            let cursor = Point2D::<f64>::from(win.get_cursor_pos()) / ui_scale as f64;
            let cursor = cursor.map(|n| n.floor());
            let win_size_logical = win.size();
            let win_size_ui = Size::from(win_size_logical) / ui_scale;
            let ctx = Context::new(Point::from(cursor), &store);

            self.graphics.cursor.origin = cursor.into();

            // Since we may receive multiple resize events at once, instead of responded to each
            // resize event, we handle the resize only once.
            if resized {
                resized = false;
                renderer.handle_resized(win_size_logical);
                events.push(WidgetEvent::Resized(win_size_ui));
            }
            delta = last.elapsed();
            last += delta;

            events.push(WidgetEvent::Tick(delta));
            // A common case is that we have multiple `CursorMoved` events
            // in one update. In that case we keep only the last one,
            // since the in-betweens will never be seen.
            if events.len() > 1
                && events
                    .iter()
                    .all(|e| matches!(e, WidgetEvent::MouseMove(_)))
            {
                events.drain(..events.len() - 1);
            }

            let mut event_ctx = ctx.into();
            for ev in events.drain(..) {
                root.event(&ev, &mut event_ctx, &mut data);
            }
            self.graphics.cursor.style = root.cursor().unwrap_or_default();

            update_timer.run(|_avg| {
                root.update(delta, &ctx, &data);
                root.layout(
                    win_size_ui,
                    &LayoutCtx::new(&self.graphics.fonts),
                    &data,
                    &self.env,
                );
            });

            let graphics = &mut self.graphics;

            paint_timer.run(|_avg| {
                root.paint(
                    Canvas::new(graphics, Transform::identity(), win_size_ui),
                    &data,
                );
            });

            render_timer.run(|_avg| {
                let cursor = graphics.cursor();

                renderer
                    .frame(graphics.effects(), cursor, &mut store)
                    .unwrap_or_else(|err| {
                        error!("{}", err);
                    });

                root.frame(&store, &mut data);
            });

            win.present();
        }

        Ok(())
    }
}
