#![deny(clippy::all)]

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

#[cfg(not(any(
    feature = "vulkan",
    feature = "metal",
    feature = "dx11",
    feature = "dx12"
)))]
compile_error!(
    "a graphics backend must be enabled with `--features <backend>`: \
     available backends are: 'vulkan', 'metal', 'dx11' and 'dx12'"
);

use event::Event;
use platform::WindowEvent;
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

use env_logger;

use directories as dirs;

use std::alloc::System;
use std::path::Path;
use std::time;

/// Program version.
pub const VERSION: &'static str = "0.2.0";

#[global_allocator]
pub static ALLOCATOR: alloc::Allocator = alloc::Allocator::new(System);

pub struct Options<'a> {
    pub log: &'a str,
    pub exec: ExecutionMode,
}

pub fn init<'a, P: AsRef<Path>>(
    paths: &[P],
    options: Options<'a>,
) -> std::io::Result<()> {
    use std::io;

    let mut logger = env_logger::Builder::new();
    logger.parse_filters(options.log);
    logger.init();

    let (win, events) = platform::init("rx")?;

    let hidpi_factor = win.hidpi_factor();
    let win_size = win.size()?;
    let (win_w, win_h) = (win_size.width as u32, win_size.height as u32);

    let resources = ResourceManager::new();
    let base_dirs = dirs::ProjectDirs::from("org", "void", "rx").ok_or(
        io::Error::new(io::ErrorKind::NotFound, "home directory not found"),
    )?;
    let mut session =
        Session::new(win_w, win_h, hidpi_factor, resources.clone(), base_dirs)
            .init(options.exec)?;

    let mut present_mode = session.settings.present_mode();
    let mut r = core::Renderer::new(win.raw_handle());
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

    platform::run(win, events, move |w, event| {
        if event.is_input() {
            debug!("event: {:?}", event);
        }

        match event {
            WindowEvent::Resized(size) => {
                logical = size;

                // Pause the session if our window size is zero.
                // This happens on *windows* when the window is minimized.
                if size.is_zero() {
                    session.transition(State::Paused);
                } else {
                    session.transition(State::Running);

                    self::resize(
                        &mut session,
                        &mut renderer,
                        &mut r,
                        &mut swap_chain,
                        size,
                        hidpi_factor,
                        present_mode,
                    );
                }
            }
            WindowEvent::CursorEntered { .. } => {
                w.set_cursor_visible(false);
            }
            WindowEvent::CursorLeft { .. } => {
                w.set_cursor_visible(true);
            }
            WindowEvent::Ready => {
                let input_delay: f64 =
                    session.settings["input/delay"].float64();
                std::thread::sleep(time::Duration::from_micros(
                    (input_delay * 1000.) as u64,
                ));

                let delta = last.elapsed();
                last = time::Instant::now();

                // If we're paused, we want to keep the timer running to not get a
                // "jump" when we unpause, but skip session updates and rendering.
                if session.state == State::Paused {
                    return platform::ControlFlow::Continue;
                }

                let effects = update_timer
                    .run(|avg| session.update(&mut session_events, delta, avg));
                render_timer.run(|avg| {
                    renderer.frame(
                        &session,
                        effects,
                        &avg,
                        &mut r,
                        &mut swap_chain,
                    );
                });

                if session.settings_changed.contains("scale") {
                    self::resize(
                        &mut session,
                        &mut renderer,
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
                session.quit();
            }
            WindowEvent::CursorMoved { position } => {
                let relative = platform::LogicalPosition::new(
                    position.x / session.width as f64,
                    position.y / session.height as f64,
                );
                session_events.push(Event::CursorMoved(relative));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                session_events.push(Event::MouseInput(button, state));
            }
            WindowEvent::KeyboardInput(input) => {
                session_events.push(Event::KeyboardInput(input));
            }
            WindowEvent::ReceivedCharacter(c) => {
                session_events.push(Event::ReceivedCharacter(c));
            }
            _ => {}
        };

        if session.state == State::Closing {
            platform::ControlFlow::Exit
        } else {
            platform::ControlFlow::Continue
        }
    });
    Ok(())
}

fn resize(
    session: &mut Session,
    renderer: &mut Renderer,
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
    renderer.handle_resized(logical_size, &r);
}
