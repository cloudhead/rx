#![forbid(clippy::all)]
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
    clippy::single_match,
    clippy::large_enum_variant
)]

pub mod execution;
pub mod session;

mod alloc;
mod brush;
mod cmd;
mod color;
mod data;
mod draw;
mod event;
mod font;
mod image;
mod palette;
mod parser;
mod platform;
mod renderer;
mod resources;
mod sprite;
mod timer;
mod view;

#[cfg(feature = "wgpu")]
#[path = "wgpu/mod.rs"]
mod gfx;

#[cfg(not(feature = "wgpu"))]
#[path = "gl/mod.rs"]
mod gfx;

#[macro_use]
mod util;

use cmd::Value;
use event::Event;
use execution::{DigestMode, Execution, ExecutionMode, GifMode};
use platform::{WindowEvent, WindowHint};
use renderer::Renderer;
use resources::ResourceManager;
use session::*;
use timer::FrameTimer;
use view::FileStatus;

#[macro_use]
extern crate log;

use directories as dirs;

use std::alloc::System;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time;

/// Program version.
pub const VERSION: &str = "0.3.0";

#[global_allocator]
pub static ALLOCATOR: alloc::Allocator = alloc::Allocator::new(System);

#[derive(Debug)]
pub struct Options {
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub headless: bool,
    pub source: Option<PathBuf>,
    pub exec: ExecutionMode,
    pub debug: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            headless: false,
            resizable: true,
            source: None,
            exec: ExecutionMode::Normal,
            debug: false,
        }
    }
}

pub fn init<P: AsRef<Path>>(paths: &[P], options: Options) -> std::io::Result<()> {
    use std::io;

    debug!("options: {:?}", options);

    let context = if cfg!(feature = "gl") {
        platform::GraphicsContext::Gl
    } else {
        platform::GraphicsContext::None
    };

    let hints = &[
        WindowHint::Resizable(options.resizable),
        WindowHint::Visible(!options.headless),
    ];
    let (mut win, events) = platform::init("rx", options.width, options.height, hints, context)?;

    let scale_factor = win.scale_factor();
    let win_size = win.size();
    let (win_w, win_h) = (win_size.width as u32, win_size.height as u32);

    info!("framebuffer size: {}x{}", win_size.width, win_size.height);
    info!("scale factor: {}", scale_factor);

    let resources = ResourceManager::new();
    let base_dirs = dirs::ProjectDirs::from("io", "cloudhead", "rx")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    let mut session = Session::new(win_w, win_h, resources.clone(), base_dirs)
        .with_blank(
            FileStatus::NoFile,
            Session::DEFAULT_VIEW_W,
            Session::DEFAULT_VIEW_H,
        )
        .init(options.source.clone())?;

    if options.debug {
        session
            .settings
            .set("debug", Value::Bool(true))
            .expect("'debug' is a bool'");
    }

    if let ExecutionMode::Record(_, _, GifMode::Record) = options.exec {
        session
            .settings
            .set("input/delay", Value::F64(0.0))
            .expect("'input/delay' is a float");
        session
            .settings
            .set("vsync", Value::Bool(true))
            .expect("'vsync' is a bool");
    }

    let exec = match options.exec {
        ExecutionMode::Normal => Execution::normal(),
        ExecutionMode::Replay(path, digest) => Execution::replaying(path, digest),
        ExecutionMode::Record(path, digest, gif) => {
            Execution::recording(path, digest, win_w as u16, win_h as u16, gif)
        }
    }?;

    // When working with digests, certain settings need to be overwritten
    // to ensure things work correctly.
    match &exec {
        Execution::Replaying { digest, .. } | Execution::Recording { digest, .. }
            if digest.mode != DigestMode::Ignore =>
        {
            session
                .settings
                .set("input/delay", Value::F64(0.0))
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

    let execution = Rc::new(RefCell::new(exec));
    let present_mode = session.settings.present_mode();

    let mut renderer: gfx::Renderer =
        Renderer::new(&mut win, win_size, scale_factor, present_mode, resources)?;

    if let Err(e) = session.edit(paths) {
        session.message(format!("Error loading path(s): {}", e), MessageType::Error);
    }

    renderer.init(session.effects(), &session.views);

    let mut render_timer = FrameTimer::new();
    let mut update_timer = FrameTimer::new();
    let mut session_events = Vec::with_capacity(16);
    let mut last = time::Instant::now();
    let mut resized = false;

    let exit = platform::run(win, events, move |w, event| {
        // Don't process events while the window is minimized.
        if w.size().is_zero() {
            return platform::ControlFlow::Wait;
        }
        if event.is_input() {
            debug!("event: {:?}", event);
        }

        match event {
            WindowEvent::Resized(size) => {
                // It's possible that the above check for zero size is delayed
                // by a frame, in which case we need to catch things here.
                if size.is_zero() {
                    session.transition(State::Paused);
                    return platform::ControlFlow::Wait;
                } else {
                    session.transition(State::Running);
                    resized = true;
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
                if resized {
                    session.handle_resized(w.size());
                    resized = false;
                } else {
                    let input_delay: f64 = session.settings["input/delay"].clone().into();
                    std::thread::sleep(time::Duration::from_micros((input_delay * 1000.) as u64));
                }

                let delta = last.elapsed();
                last = time::Instant::now();

                // If we're paused, we want to keep the timer running to not get a
                // "jump" when we unpause, but skip session updates and rendering.
                if session.state == State::Paused {
                    return platform::ControlFlow::Wait;
                }

                let effects = update_timer
                    .run(|avg| session.update(&mut session_events, execution.clone(), delta, avg));
                render_timer.run(|avg| {
                    renderer.frame(&session, execution.clone(), effects, &avg);
                });

                if session.settings_changed.contains("vsync") {
                    renderer.handle_present_mode_changed(session.settings.present_mode());
                }
            }
            WindowEvent::Minimized => {
                session.transition(State::Paused);
                return platform::ControlFlow::Wait;
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
                // We currently don't draw in here, as it negatively
                // affects resize smoothness.  (╯°□°）╯︵ ┻━┻
            }
            WindowEvent::ScaleFactorChanged(factor) => {
                renderer.handle_scale_factor_changed(factor);
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
