#![deny(clippy::all)]
mod brush;
mod cmd;
mod color;
mod data;
mod font;
mod framebuffer2d;
mod gpu;
mod image;
mod palette;
mod platform;
mod renderer;
mod resources;
mod screen2d;
mod session;
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

use renderer::Renderer;
use resources::ResourceManager;
use session::*;
use timer::FrameTimer;
use view::FileStatus;

use rgx;
use rgx::core;
use rgx::kit;
use rgx::kit::shape2d;

#[macro_use]
extern crate log;

use env_logger;

use directories as dirs;

use std::path::Path;
use std::time;

pub struct Options<'a> {
    pub log: &'a str,
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
            .init()?;

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

    renderer.init(&session, &mut r);

    let physical = win_size.to_physical(hidpi_factor);
    let mut logical = win_size;
    let mut swap_chain = r.swap_chain(
        physical.width as u32,
        physical.height as u32,
        present_mode,
    );

    let mut render_timer = FrameTimer::new();
    let mut update_timer = FrameTimer::new();
    let mut canvas = shape2d::Batch::new();
    let mut session_events = Vec::with_capacity(16);
    let mut last = time::Instant::now();

    platform::run(win, events, move |w, event| {
        if event != platform::WindowEvent::Ready
            && event != platform::WindowEvent::RedrawRequested
        {
            debug!("event: {:?}", event);
        }

        match event {
            platform::WindowEvent::Resized(size) => {
                logical = size;

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
            platform::WindowEvent::CursorEntered { .. } => {
                w.set_cursor_visible(false);
            }
            platform::WindowEvent::CursorLeft { .. } => {
                w.set_cursor_visible(true);
            }
            platform::WindowEvent::Ready => {
                let frame_delay: f64 =
                    session.settings["frame_delay"].float64();
                std::thread::sleep(time::Duration::from_micros(
                    (frame_delay * 1000.) as u64,
                ));

                let delta = last.elapsed();
                last = time::Instant::now();

                update_timer.run(|avg| {
                    session.update(
                        &mut session_events,
                        &mut canvas,
                        delta,
                        avg,
                    );
                });
                w.request_redraw();

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
            platform::WindowEvent::RedrawRequested => {
                render_timer.run(|avg| {
                    renderer.frame(
                        &session,
                        &avg,
                        &mut r,
                        &mut swap_chain,
                        &canvas,
                    );
                });
                canvas.clear();
            }
            event => {
                session_events.push(event);
            }
        }

        if session.is_running {
            platform::ControlFlow::Continue
        } else {
            platform::ControlFlow::Exit
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
    let virtual_size =
        platform::LogicalSize::new(size.width / scale, size.height / scale);
    session.handle_resized(virtual_size);

    let physical = size.to_physical(hidpi_factor);
    *swap_chain = r.swap_chain(
        physical.width as u32,
        physical.height as u32,
        present_mode,
    );
    renderer.handle_resized(virtual_size, &r);
}
