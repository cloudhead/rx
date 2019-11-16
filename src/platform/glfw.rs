use crate::platform::{
    ControlFlow, InputState, Key, KeyboardInput, LogicalDelta, LogicalPosition,
    LogicalSize, ModifiersState, MouseButton, WindowEvent, WindowHint,
};

use glfw;

use std::{io, sync};

///////////////////////////////////////////////////////////////////////////////

pub fn init(
    title: &str,
    w: u32,
    h: u32,
    hints: &[WindowHint],
) -> io::Result<(Window, Events)> {
    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    glfw.window_hint(glfw::WindowHint::Resizable(true));
    glfw.window_hint(glfw::WindowHint::Visible(true));
    glfw.window_hint(glfw::WindowHint::Focused(true));
    glfw.window_hint(glfw::WindowHint::RefreshRate(None));
    glfw.window_hint(glfw::WindowHint::ClientApi(glfw::ClientApiHint::NoApi));

    for hint in hints {
        glfw.window_hint((*hint).into());
    }

    let (mut window, events) = glfw
        .create_window(w, h, title, glfw::WindowMode::Windowed)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "glfw: error creating window")
        })?;

    window.set_all_polling(true);

    Ok((
        Window {
            handle: window,
            redraw_requested: false,
        },
        Events {
            handle: events,
            glfw,
        },
    ))
}

pub fn run<F, T>(mut win: Window, events: Events, mut callback: F) -> T
where
    F: 'static + FnMut(&mut Window, WindowEvent) -> ControlFlow<T>,
    T: Default,
{
    let mut glfw = events.glfw;
    let mut exit = T::default();

    while !win.handle.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events.handle) {
            if let ControlFlow::Exit(r) = callback(&mut win, event.into()) {
                win.handle.set_should_close(true);
                exit = r;
            }
        }
        if let ControlFlow::Exit(r) = callback(&mut win, WindowEvent::Ready) {
            win.handle.set_should_close(true);
            exit = r;
        }

        if win.redraw_requested {
            win.redraw_requested = false;

            if let ControlFlow::Exit(r) =
                callback(&mut win, WindowEvent::RedrawRequested)
            {
                win.handle.set_should_close(true);
                exit = r;
            }
        }
    }
    callback(&mut win, WindowEvent::Destroyed);

    exit
}

pub struct Events {
    handle: sync::mpsc::Receiver<(f64, glfw::WindowEvent)>,
    glfw: glfw::Glfw,
}

pub struct Window {
    pub handle: glfw::Window,
    redraw_requested: bool,
}

impl Window {
    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn handle(&self) -> &glfw::Window {
        &self.handle
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.handle.set_cursor_mode(if visible {
            glfw::CursorMode::Normal
        } else {
            glfw::CursorMode::Hidden
        });
    }

    pub fn hidpi_factor(&self) -> f64 {
        let (x, y) = self.handle.get_content_scale();
        if (x - y).abs() > 0.1 {
            warn!("glfw: content scale isn't uniform: {} x {}", x, y);
        }

        x as f64
    }

    pub fn size(&self) -> io::Result<LogicalSize> {
        let (w, h) = self.handle.get_size();
        Ok(LogicalSize::new(w as f64, h as f64))
    }
}

impl Into<glfw::WindowHint> for WindowHint {
    fn into(self) -> glfw::WindowHint {
        match self {
            Self::Resizable(b) => glfw::WindowHint::Resizable(b),
            Self::Visible(b) => glfw::WindowHint::Visible(b),
        }
    }
}

impl From<glfw::MouseButton> for MouseButton {
    fn from(button: glfw::MouseButton) -> Self {
        match button {
            glfw::MouseButton::Button1 => MouseButton::Left,
            glfw::MouseButton::Button2 => MouseButton::Right,
            glfw::MouseButton::Button3 => MouseButton::Middle,
            glfw::MouseButton::Button4 => MouseButton::Other(4),
            glfw::MouseButton::Button5 => MouseButton::Other(5),
            glfw::MouseButton::Button6 => MouseButton::Other(6),
            glfw::MouseButton::Button7 => MouseButton::Other(7),
            glfw::MouseButton::Button8 => MouseButton::Other(8),
        }
    }
}

impl From<glfw::Action> for InputState {
    fn from(state: glfw::Action) -> Self {
        match state {
            glfw::Action::Press => InputState::Pressed,
            glfw::Action::Release => InputState::Released,
            glfw::Action::Repeat => InputState::Pressed,
        }
    }
}

impl From<glfw::WindowEvent> for WindowEvent {
    fn from(event: glfw::WindowEvent) -> Self {
        use glfw::WindowEvent as Glfw;

        match event {
            Glfw::Size(w, h) => {
                WindowEvent::Resized(LogicalSize::new(w as f64, h as f64))
            }
            Glfw::Iconify(true) => WindowEvent::Minimized,
            Glfw::Iconify(false) => WindowEvent::Restored,
            Glfw::Close => WindowEvent::CloseRequested,
            Glfw::Refresh => WindowEvent::RedrawRequested,
            Glfw::Pos(x, y) => {
                WindowEvent::Moved(LogicalPosition::new(x as f64, y as f64))
            }
            Glfw::MouseButton(button, action, modifiers) => {
                WindowEvent::MouseInput {
                    state: action.into(),
                    button: button.into(),
                    modifiers: modifiers.into(),
                }
            }
            Glfw::Scroll(x, y) => WindowEvent::MouseWheel {
                delta: LogicalDelta { x, y },
            },
            Glfw::CursorEnter(true) => WindowEvent::CursorEntered,
            Glfw::CursorEnter(false) => WindowEvent::CursorLeft,
            Glfw::CursorPos(x, y) => WindowEvent::CursorMoved {
                position: LogicalPosition::new(x, y),
            },
            Glfw::Char(c) => WindowEvent::ReceivedCharacter(c),
            Glfw::Key(key, _, action, modifiers) => {
                WindowEvent::KeyboardInput(KeyboardInput {
                    key: Some(key.into()),
                    state: action.into(),
                    modifiers: modifiers.into(),
                })
            }
            Glfw::Focus(b) => WindowEvent::Focused(b),
            Glfw::ContentScale(x, y) => {
                if (x - y).abs() > 0.1 {
                    warn!("glfw: content scale isn't uniform: {} x {}", x, y);
                }
                WindowEvent::HiDpiFactorChanged(x as f64)
            }
            _ => WindowEvent::Noop,
        }
    }
}

impl From<glfw::Key> for Key {
    fn from(k: glfw::Key) -> Self {
        use glfw::Key as Glfw;

        match k {
            Glfw::Escape => Key::Escape,
            Glfw::Insert => Key::Insert,
            Glfw::Home => Key::Home,
            Glfw::Delete => Key::Delete,
            Glfw::End => Key::End,
            Glfw::PageDown => Key::PageDown,
            Glfw::PageUp => Key::PageUp,
            Glfw::Left => Key::Left,
            Glfw::Up => Key::Up,
            Glfw::Right => Key::Right,
            Glfw::Down => Key::Down,
            Glfw::Backspace => Key::Backspace,
            Glfw::Enter => Key::Return,
            Glfw::Space => Key::Space,
            Glfw::LeftAlt => Key::Alt,
            Glfw::LeftBracket => Key::LBracket,
            Glfw::LeftControl => Key::Control,
            Glfw::LeftShift => Key::Shift,
            Glfw::RightAlt => Key::Alt,
            Glfw::RightBracket => Key::RBracket,
            Glfw::RightControl => Key::Control,
            Glfw::RightShift => Key::Shift,
            Glfw::Tab => Key::Tab,

            _ => {
                if let Some(sym) = k.get_name() {
                    if let Some(c) = sym.chars().next() {
                        return Key::from(c);
                    }
                }
                Key::Unknown
            }
        }
    }
}

impl From<glfw::Modifiers> for ModifiersState {
    fn from(mods: glfw::Modifiers) -> Self {
        Self {
            shift: mods.contains(glfw::Modifiers::Shift),
            ctrl: mods.contains(glfw::Modifiers::Control),
            alt: mods.contains(glfw::Modifiers::Alt),
            meta: mods.contains(glfw::Modifiers::Super),
        }
    }
}
