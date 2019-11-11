#![deny(clippy::all)]
#![allow(
    clippy::collapsible_if,
    clippy::many_single_char_names,
    clippy::expect_fun_call,
    clippy::useless_format,
    clippy::new_without_default,
    clippy::cognitive_complexity,
    clippy::type_complexity,
    clippy::or_fun_call,
    clippy::nonminimal_bool,
    clippy::single_match
)]

pub mod execution;
pub mod session;

mod alloc;
mod brush;
mod cmd;
mod color;
mod data;
mod event;
mod font;
mod framebuffer2d;
mod gpu;
mod image;
mod palette;
mod parser;
mod platform;
mod renderer;
mod resources;
mod screen2d;
mod timer;
mod view;

#[macro_use]
mod util;

use cmd::Value;
use event::Event;
use execution::Execution;
use platform::{WindowEvent, WindowHint};
use renderer::Renderer;
use resources::ResourceManager;
use session::*;
use timer::FrameTimer;
use view::FileStatus;

use rgx;
use rgx::core;
use rgx::kit;

#[macro_use]
extern crate log;

use directories as dirs;

use std::alloc::System;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time;

/// Program version.
pub const VERSION: &str = "0.2.0";

#[global_allocator]
pub static ALLOCATOR: alloc::Allocator = alloc::Allocator::new(System);

#[derive(Debug)]
pub struct Options {
    pub exec: Execution,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub source: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            exec: Execution::default(),
            width: 1280,
            height: 720,
            resizable: true,
            source: None,
        }
    }
}

pub fn init<P: AsRef<Path>>(
    paths: &[P],
    options: Options,
) -> std::io::Result<()> {
    use std::io;

    debug!("options: {:?}", options);

    let hints = &[WindowHint::Resizable(options.resizable)];
    let (win, events) =
        platform::init("rx", options.width, options.height, hints)?;

    let hidpi_factor = win.hidpi_factor();
    let win_size = win.size()?;
    let (win_w, win_h) = (win_size.width as u32, win_size.height as u32);

    let resources = ResourceManager::new();
    let base_dirs =
        dirs::ProjectDirs::from("org", "void", "rx").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "home directory not found")
        })?;
    let mut session =
        Session::new(win_w, win_h, hidpi_factor, resources.clone(), base_dirs)
            .init(options.source.clone())?;

    match &options.exec {
        Execution::Replaying { digest: true, .. }
        | Execution::Recording { digest: true, .. } => {
            session
                .settings
                .set("input/delay", Value::Float(0.0))
                .expect("'input/delay' is a float");
            session
                .settings
                .set("vsync", Value::Bool(false))
                .expect("'vsync' is a bool");
            session
                .settings
                .set("animation", Value::Bool(false))
                .expect("'animation' is a bool");
        }
        _ => {}
    }

    let execution = Rc::new(RefCell::new(options.exec));
    let mut present_mode = session.settings.present_mode();
    let mut r = core::Renderer::new(win.handle());
    let mut renderer = Renderer::new(&mut r, win_size, resources);

    if let Err(e) = session.edit(paths) {
        session.message(
            format!("Error loading path(s): {}", e),
            MessageType::Error,
        );
    }
    if session.views.is_empty() {
        session.blank(
            FileStatus::NoFile,
            Session::DEFAULT_VIEW_W,
            Session::DEFAULT_VIEW_H,
        );
    }

    renderer.init(session.effects(), &session.views, &mut r);

    let physical = win_size.to_physical(hidpi_factor);
    let mut logical = win_size;
    let mut swap_chain = r.swap_chain(
        physical.width as u32,
        physical.height as u32,
        present_mode,
    );

    let mut render_timer = FrameTimer::new();
    let mut update_timer = FrameTimer::new();
    let mut session_events = Vec::with_capacity(16);
    let mut last = time::Instant::now();

    let exit = platform::run(win, events, move |w, event| {
        if event.is_input() {
            debug!("event: {:?}", event);
        }

        match event {
            WindowEvent::Resized(size) => {
                // Pause the session if our window size is zero.
                // This happens on *windows* when the window is minimized.
                if size.is_zero() {
                    session.transition(State::Paused);
                } else {
                    session.transition(State::Running);
                    // We don't re-create the swap chain in this handler,
                    // because it is often called many times before we
                    // actually need to render anything. Instead, we cache
                    // the new size, and resize when drawing is ready.
                    logical = size;
                }
            }
            WindowEvent::CursorEntered { .. } => {
                // TODO: [winit] This doesn't fire if the cursor is already
                // in the window.
                w.set_cursor_visible(false);
            }
            WindowEvent::CursorLeft { .. } => {
                w.set_cursor_visible(true);
            }
            WindowEvent::Ready => {
                if logical != renderer.window {
                    self::resize(
                        &mut session,
                        &mut r,
                        &mut swap_chain,
                        logical,
                        hidpi_factor,
                        present_mode,
                    );
                } else {
                    let input_delay: f64 =
                        session.settings["input/delay"].float64();
                    std::thread::sleep(time::Duration::from_micros(
                        (input_delay * 1000.) as u64,
                    ));
                }

                let delta = last.elapsed();
                last = time::Instant::now();

                // If we're paused, we want to keep the timer running to not get a
                // "jump" when we unpause, but skip session updates and rendering.
                if session.state == State::Paused {
                    return platform::ControlFlow::Continue;
                }

                let effects = update_timer.run(|avg| {
                    session.update(
                        &mut session_events,
                        execution.clone(),
                        delta,
                        avg,
                    )
                });
                render_timer.run(|avg| {
                    renderer.frame(
                        &session,
                        execution.clone(),
                        effects,
                        &avg,
                        &mut r,
                        &mut swap_chain,
                    );
                });

                if session.settings_changed.contains("scale") {
                    self::resize(
                        &mut session,
                        &mut r,
                        &mut swap_chain,
                        logical,
                        hidpi_factor,
                        present_mode,
                    );
                }

                if session.settings_changed.contains("vsync") {
                    present_mode = session.settings.present_mode();

                    swap_chain = r.swap_chain(
                        swap_chain.width as u32,
                        swap_chain.height as u32,
                        present_mode,
                    );
                }
            }
            WindowEvent::Minimized => {
                session.transition(State::Paused);
            }
            WindowEvent::Restored => {
                session.transition(State::Running);
            }
            WindowEvent::Focused(true) => {
                session.transition(State::Running);
            }
            WindowEvent::Focused(false) => {
                session.transition(State::Paused);
            }
            WindowEvent::RedrawRequested => {
                if session.state == State::Running {
                    render_timer.run(|avg| {
                        renderer.frame(
                            &session,
                            execution.clone(),
                            vec![],
                            &avg,
                            &mut r,
                            &mut swap_chain,
                        );
                    });
                }
            }
            WindowEvent::HiDpiFactorChanged(factor) => {
                session.hidpi_factor = factor;
            }
            WindowEvent::CloseRequested => {
                session.quit(ExitReason::Normal);
            }
            WindowEvent::CursorMoved { position } => {
                session_events.push(Event::CursorMoved(position));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                session_events.push(Event::MouseInput(button, state));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                session_events.push(Event::MouseWheel(delta));
            }
            WindowEvent::KeyboardInput(input) => {
                session_events.push(Event::KeyboardInput(input));
            }
            WindowEvent::ReceivedCharacter(c) => {
                session_events.push(Event::ReceivedCharacter(c));
            }
            _ => {}
        };

        if let State::Closing(reason) = &session.state {
            platform::ControlFlow::Exit(reason.clone())
        } else {
            platform::ControlFlow::Continue
        }
    });

    match exit {
        ExitReason::Normal => Ok(()),
        ExitReason::Error(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

fn resize(
    session: &mut Session,
    r: &mut core::Renderer,
    swap_chain: &mut core::SwapChain,
    size: platform::LogicalSize,
    hidpi_factor: f64,
    present_mode: core::PresentMode,
) {
    let scale: f64 = session.settings["scale"].float64();
    let logical_size =
        platform::LogicalSize::new(size.width / scale, size.height / scale);
    session.handle_resized(logical_size);

    let physical = size.to_physical(hidpi_factor);
    *swap_chain = r.swap_chain(
        physical.width as u32,
        physical.height as u32,
        present_mode,
    );
}
