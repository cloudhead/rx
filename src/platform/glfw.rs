use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

use crate::platform::{
    ControlFlow, InputState, Key, KeyboardInput, LogicalPosition, LogicalSize,
    ModifiersState, MouseButton, WindowEvent,
};

use glfw::{self, Context};

use std::{io, sync};
#[cfg(feature = "vulkan")]
use std::{mem, os::raw::c_void, ptr};

///////////////////////////////////////////////////////////////////////////////

pub fn init(title: &str) -> io::Result<(Window, Events)> {
    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    glfw.window_hint(glfw::WindowHint::Resizable(true));
    glfw.window_hint(glfw::WindowHint::Visible(true));
    glfw.window_hint(glfw::WindowHint::Focused(true));
    glfw.window_hint(glfw::WindowHint::RefreshRate(None));

    let (mut window, events) = glfw
        .create_window(800, 600, title, glfw::WindowMode::Windowed)
        .ok_or(io::Error::new(
            io::ErrorKind::Other,
            "glfw: error creating window",
        ))?;

    window.set_all_polling(true);
    window.make_current();

    #[cfg(feature = "vulkan")]
    let (instance, instance_ptrs) = {
        assert!(glfw.vulkan_supported());

        let required_extensions = glfw
            .get_required_instance_extensions()
            .unwrap_or(Vec::new());

        assert!(required_extensions.contains(&"VK_KHR_surface".to_string()));

        // Load up all the entry points using 0 as the VkInstance,
        // since we can't have an instance before we get vkCreateInstance.
        let mut entry_points: EntryPoints = EntryPoints::load(|func| {
            window.get_instance_proc_address(0, func.to_str().unwrap())
                as *const c_void
        });

        let instance: VkInstance = unsafe {
            self::create_vulkan_instance(required_extensions, &mut entry_points)
        };

        (
            instance,
            InstancePointers::load(|func| {
                window
                    .get_instance_proc_address(instance, func.to_str().unwrap())
                    as *const c_void
            }),
        )
    };

    Ok((
        Window {
            handle: window,
            redraw_requested: false,
            #[cfg(feature = "vulkan")]
            instance,
            #[cfg(feature = "vulkan")]
            instance_ptrs,
        },
        Events {
            handle: events,
            glfw,
        },
    ))
}

pub fn run<F>(mut win: Window, events: Events, mut callback: F)
where
    F: 'static + FnMut(&mut Window, WindowEvent) -> ControlFlow,
{
    let mut glfw = events.glfw;

    while !win.handle.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events.handle) {
            if callback(&mut win, event.into()) == ControlFlow::Exit {
                win.handle.set_should_close(true);
            }
        }
        if callback(&mut win, WindowEvent::Ready) == ControlFlow::Exit {
            win.handle.set_should_close(true);
        }

        if win.redraw_requested {
            win.redraw_requested = false;

            if callback(&mut win, WindowEvent::RedrawRequested)
                == ControlFlow::Exit
            {
                win.handle.set_should_close(true);
            }
        }
    }
    #[cfg(feature = "vulkan")]
    unsafe {
        self::destroy_vulkan_instance(win.instance, &mut win.instance_ptrs);
    }
    callback(&mut win, WindowEvent::Destroyed);
}

pub struct Events {
    handle: sync::mpsc::Receiver<(f64, glfw::WindowEvent)>,
    glfw: glfw::Glfw,
}

pub struct Window {
    pub handle: glfw::Window,
    redraw_requested: bool,
    #[cfg(feature = "vulkan")]
    instance: VkInstance,
    #[cfg(feature = "vulkan")]
    instance_ptrs: InstancePointers,
}

impl Window {
    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn raw_handle(&self) -> RawWindowHandle {
        self.handle.raw_window_handle()
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
        assert_eq!(x, y, "glfw: content scale must be uniform");

        x as f64
    }

    pub fn framebuffer_size(&self) -> io::Result<LogicalSize> {
        let (w, h) = self.handle.get_framebuffer_size();
        Ok(LogicalSize::new(w as f64, h as f64))
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
            Glfw::FramebufferSize(w, h) => {
                WindowEvent::Resized(LogicalSize::new(w as f64, h as f64))
            }
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
                assert_eq!(x, y, "glfw: content scale must be uniform");
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
            Glfw::Num1 => Key::Num1,
            Glfw::Num2 => Key::Num2,
            Glfw::Num3 => Key::Num3,
            Glfw::Num4 => Key::Num4,
            Glfw::Num5 => Key::Num5,
            Glfw::Num6 => Key::Num6,
            Glfw::Num7 => Key::Num7,
            Glfw::Num8 => Key::Num8,
            Glfw::Num9 => Key::Num9,
            Glfw::Num0 => Key::Num0,
            Glfw::A => Key::A,
            Glfw::B => Key::B,
            Glfw::C => Key::C,
            Glfw::D => Key::D,
            Glfw::E => Key::E,
            Glfw::F => Key::F,
            Glfw::G => Key::G,
            Glfw::H => Key::H,
            Glfw::I => Key::I,
            Glfw::J => Key::J,
            Glfw::K => Key::K,
            Glfw::L => Key::L,
            Glfw::M => Key::M,
            Glfw::N => Key::N,
            Glfw::O => Key::O,
            Glfw::P => Key::P,
            Glfw::Q => Key::Q,
            Glfw::R => Key::R,
            Glfw::S => Key::S,
            Glfw::T => Key::T,
            Glfw::U => Key::U,
            Glfw::V => Key::V,
            Glfw::W => Key::W,
            Glfw::X => Key::X,
            Glfw::Y => Key::Y,
            Glfw::Z => Key::Z,
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
            Glfw::Apostrophe => Key::Apostrophe,
            Glfw::Backslash => Key::Backslash,
            Glfw::Comma => Key::Comma,
            Glfw::Equal => Key::Equals,
            Glfw::GraveAccent => Key::Grave,
            Glfw::LeftAlt => Key::LAlt,
            Glfw::LeftBracket => Key::LBracket,
            Glfw::LeftControl => Key::LControl,
            Glfw::LeftShift => Key::LShift,
            Glfw::Minus => Key::Minus,
            Glfw::Period => Key::Period,
            Glfw::RightAlt => Key::RAlt,
            Glfw::RightBracket => Key::RBracket,
            Glfw::RightControl => Key::RControl,
            Glfw::RightShift => Key::RShift,
            Glfw::Semicolon => Key::Semicolon,
            Glfw::Slash => Key::Slash,
            Glfw::Tab => Key::Tab,
            _ => Key::Unknown,
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

#[cfg(feature = "vulkan")]
use vk_sys::{
    self as vk, EntryPoints, Instance as VkInstance, InstanceCreateInfo,
    InstancePointers, Result as VkResult,
};

#[cfg(feature = "vulkan")]
unsafe fn create_vulkan_instance(
    required_extensions: Vec<String>,
    entry_points: &mut EntryPoints,
) -> VkInstance {
    use std::ffi::CString;

    let mut instance: VkInstance = mem::uninitialized();

    let cstr_argv: Vec<_> = required_extensions
        .iter()
        .map(|arg| CString::new(arg.as_str()).unwrap())
        .collect();

    let mut p_argv: Vec<_> = cstr_argv.iter().map(|arg| arg.as_ptr()).collect();

    p_argv.push(std::ptr::null());

    let p: *const *const i8 = p_argv.as_ptr();

    let info: InstanceCreateInfo = InstanceCreateInfo {
        sType: vk::STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        pNext: ptr::null(),
        flags: 0,
        pApplicationInfo: ptr::null(),
        enabledLayerCount: 0,
        ppEnabledLayerNames: ptr::null(),
        enabledExtensionCount: required_extensions.len() as u32,
        ppEnabledExtensionNames: p,
    };

    let res: VkResult = entry_points.CreateInstance(
        &info as *const InstanceCreateInfo,
        ptr::null(),
        &mut instance as *mut VkInstance,
    );

    assert_eq!(res, vk::SUCCESS);

    instance
}

#[cfg(feature = "vulkan")]
unsafe fn destroy_vulkan_instance(
    instance: VkInstance,
    instance_ptrs: &mut InstancePointers,
) {
    instance_ptrs.DestroyInstance(instance, ptr::null());
}
