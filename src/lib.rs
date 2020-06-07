#![forbid(clippy::all)]
#![allow(
    clippy::collapsible_if,
    clippy::many_single_char_names,
    clippy::expect_fun_call,
    clippy::useless_format,
    clippy::new_without_default,
    clippy::cognitive_complexity,
    clippy::comparison_chain,
    clippy::type_complexity,
    clippy::or_fun_call,
    clippy::nonminimal_bool,
    clippy::single_match,
    clippy::large_enum_variant
)]

pub mod data;
pub mod execution;
pub mod session;

mod alloc;
mod autocomplete;
mod brush;
mod cmd;
mod color;
mod draw;
mod event;
mod font;
mod history;
mod image;
mod palette;
mod parser;
mod pixels;
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
pub mod util;

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
use std::time::{Duration, Instant};

/// Program version.
pub const VERSION: &str = "0.4.0";

#[global_allocator]
pub static ALLOCATOR: alloc::Allocator = alloc::Allocator::new(System);

#[derive(Debug)]
pub struct Options<'a> {
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub headless: bool,
    pub source: Option<PathBuf>,
    pub exec: ExecutionMode,
    pub glyphs: &'a [u8],
    pub debug: bool,
}

impl<'a> Default for Options<'a> {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            headless: false,
            resizable: true,
            source: None,
            exec: ExecutionMode::Normal,
            glyphs: data::GLYPHS,
            debug: false,
        }
    }
}

pub fn init<'a, P: AsRef<Path>>(paths: &[P], options: Options<'a>) -> std::io::Result<()> {
    use std::io;

    debug!("options: {:?}", options);

    let context = if cfg!(feature = "wgpu") {
        platform::GraphicsContext::None
    } else {
        platform::GraphicsContext::Gl
    };

    let hints = &[
        WindowHint::Resizable(options.resizable),
        WindowHint::Visible(!options.headless),
    ];
    let (mut win, mut events) =
        platform::init("rx", options.width, options.height, hints, context)?;

    let scale_factor = win.scale_factor();
    let win_size = win.size();
    let (win_w, win_h) = (win_size.width as u32, win_size.height as u32);

    info!("framebuffer size: {}x{}", win_size.width, win_size.height);
    info!("scale factor: {}", scale_factor);

    let resources = ResourceManager::new();
    let assets = data::Assets::new(options.glyphs);
    let proj_dirs = dirs::ProjectDirs::from("io", "cloudhead", "rx")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "config directory not found"))?;
    let base_dirs = dirs::BaseDirs::new()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    let cwd = std::env::current_dir()?;
    let mut session = Session::new(win_w, win_h, cwd, resources.clone(), proj_dirs, base_dirs)
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
                .set("vsync", Value::Bool(false))
                .expect("'vsync' is a bool");
            session
                .settings
                .set("animation", Value::Bool(false))
                .expect("'animation' is a bool");
        }
        _ => {}
    }

    let wait_events = exec.is_normal() || exec.is_recording();
    let execution = Rc::new(RefCell::new(exec));
    let present_mode = session.settings.present_mode();

    let mut renderer: gfx::Renderer = Renderer::new(
        &mut win,
        win_size,
        scale_factor,
        present_mode,
        resources,
        assets,
    )?;

    if let Err(e) = session.edit(paths) {
        session.message(format!("Error loading path(s): {}", e), MessageType::Error);
    }
    // Make sure our session ticks once before anything is rendered.
    let effects = session.update(
        &mut vec![],
        execution.clone(),
        Duration::default(),
        Duration::default(),
    );
    renderer.init(effects);

    let mut render_timer = FrameTimer::new();
    let mut update_timer = FrameTimer::new();
    let mut session_events = Vec::with_capacity(16);
    let mut last = Instant::now();
    let mut resized = false;
    let mut hovering = false;

    // Accumulated error from animation timeout.
    let mut anim_accum = Duration::from_secs(0);

    while !win.is_closing() {
        let start = Instant::now();

        match session.animation_delay() {
            Some(delay) if session.is_running() => {
                if delay > anim_accum {
                    events.wait_timeout(delay - anim_accum);
                } else {
                    events.poll();
                }
                // How much time has actually passed waiting for events.
                let d = start.elapsed();

                if d > delay {
                    // If more time has passed than the desired animation delay, then
                    // add the difference to our accumulated error.
                    anim_accum += d - delay;
                } else if delay > d {
                    // If less time has passed than our desired delay, then
                    // reset the accumulator to zero, because we've overshot.
                    anim_accum = Duration::from_secs(0);
                };
            }
            _ if wait_events => events.wait(),
            _ => events.poll(),
        }

        for event in events.flush() {
            if event.is_input() {
                debug!("event: {:?}", event);
            }

            match event {
                WindowEvent::Resized(size) => {
                    if size.is_zero() {
                        // On certain operating systems, the window size will be set to
                        // zero when the window is minimized. Since a zero-sized framebuffer
                        // is not valid, we pause the session until the window is restored.
                        session.transition(State::Paused);
                    } else {
                        resized = true;
                        session.transition(State::Running);
                    }
                }
                WindowEvent::CursorEntered { .. } => {
                    if win.is_focused() {
                        win.set_cursor_visible(false);
                    }
                    hovering = true;
                }
                WindowEvent::CursorLeft { .. } => {
                    win.set_cursor_visible(true);

                    hovering = false;
                }
                WindowEvent::Minimized => {
                    session.transition(State::Paused);
                }
                WindowEvent::Restored => {
                    if win.is_focused() {
                        session.transition(State::Running);
                    }
                }
                WindowEvent::Focused(true) => {
                    session.transition(State::Running);

                    if hovering {
                        win.set_cursor_visible(false);
                    }
                }
                WindowEvent::Focused(false) => {
                    win.set_cursor_visible(true);
                    session.transition(State::Paused);
                }
                WindowEvent::RedrawRequested => {
                    render_timer.run(|avg| {
                        renderer.frame(&session, execution.clone(), vec![], &avg);
                    });
                    win.present();
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
                WindowEvent::KeyboardInput(input) => match input {
                    // Intercept `<insert>` key for pasting.
                    //
                    // Reading from the clipboard causes the loop to wake up for some strange
                    // reason I cannot comprehend. So we only read from clipboard when we
                    // need to paste.
                    platform::KeyboardInput {
                        key: Some(platform::Key::Insert),
                        state: platform::InputState::Pressed,
                        modifiers: platform::ModifiersState { shift: true, .. },
                    } => {
                        session_events.push(Event::Paste(win.clipboard()));
                    }
                    _ => session_events.push(Event::KeyboardInput(input)),
                },
                WindowEvent::ReceivedCharacter(c, mods) => {
                    session_events.push(Event::ReceivedCharacter(c, mods));
                }
                _ => {}
            };
        }

        if resized {
            // Instead of responded to each resize event by creating a new framebuffer,
            // we respond to the event *once*, here.
            resized = false;
            session.handle_resized(win.size());
        }

        let delta = last.elapsed();
        last = Instant::now();

        // If we're paused, we want to keep the timer running to not get a
        // "jump" when we unpause, but skip session updates and rendering.
        if session.state == State::Paused {
            continue;
        }

        let effects = update_timer
            .run(|avg| session.update(&mut session_events, execution.clone(), delta, avg));

        render_timer.run(|avg| {
            renderer.frame(&session, execution.clone(), effects, &avg);
        });

        session.cleanup();
        win.present();

        if session.settings_changed.contains("vsync") {
            renderer.handle_present_mode_changed(session.settings.present_mode());
        }

        match session.state {
            State::Closing(ExitReason::Normal) => {
                return Ok(());
            }
            State::Closing(ExitReason::Error(e)) => {
                return Err(io::Error::new(io::ErrorKind::Other, e));
            }
            _ => {}
        }
    }

    Ok(())
}
