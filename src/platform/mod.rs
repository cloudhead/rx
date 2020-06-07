#![allow(dead_code)]
use std::io;

use std::fmt;

#[cfg(not(feature = "glfw"))]
#[path = "dummy.rs"]
pub mod backend;

#[cfg(feature = "glfw")]
#[path = "glfw.rs"]
pub mod backend;

/// Initialize the platform.
pub fn init(
    title: &str,
    w: u32,
    h: u32,
    hints: &[WindowHint],
    context: GraphicsContext,
) -> io::Result<(backend::Window, backend::Events)> {
    backend::init(title, w, h, hints, context)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GraphicsContext {
    None,
    Gl,
}

#[derive(Debug, Copy, Clone)]
pub enum WindowHint {
    Resizable(bool),
    Visible(bool),
}

/// Describes an event from a `Window`.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The size of the window has changed. Contains the client area's new dimensions.
    Resized(LogicalSize),

    /// The position of the window has changed. Contains the window's new position.
    Moved(LogicalPosition),

    /// The window was minimized.
    Minimized,

    /// The window was restored after having been minimized.
    Restored,

    /// The window has been requested to close.
    CloseRequested,

    /// The window has been destroyed.
    Destroyed,

    /// The window received a unicode character.
    ReceivedCharacter(char, ModifiersState),

    /// The window gained or lost focus.
    Focused(bool),

    /// An event from the keyboard has been received.
    KeyboardInput(KeyboardInput),

    /// The cursor has moved on the window.
    CursorMoved {
        /// Coords in pixels relative to the top-left corner of the window.
        position: LogicalPosition,
    },

    /// The cursor has entered the window.
    CursorEntered,

    /// The cursor has left the window.
    CursorLeft,

    /// A mouse button press has been received.
    MouseInput {
        state: InputState,
        button: MouseButton,
        modifiers: ModifiersState,
    },

    /// The mouse wheel has been used.
    MouseWheel { delta: LogicalDelta },

    /// The OS or application has requested that the window be redrawn.
    RedrawRequested,

    /// There are no more inputs to process, the application can do work.
    Ready,

    /// The content scale factor of the window has changed.  For example,
    /// the window was moved to a higher DPI screen.
    ScaleFactorChanged(f64),

    /// No-op event, for events we don't handle.
    Noop,
}

impl WindowEvent {
    /// Events that are triggered by user input.
    pub fn is_input(&self) -> bool {
        match self {
            Self::Resized(_)
            | Self::Moved(_)
            | Self::Minimized
            | Self::Restored
            | Self::CloseRequested
            | Self::Destroyed
            | Self::ReceivedCharacter(_, _)
            | Self::Focused(_)
            | Self::KeyboardInput(_)
            | Self::CursorMoved { .. }
            | Self::CursorEntered
            | Self::CursorLeft
            | Self::MouseInput { .. }
            | Self::ScaleFactorChanged(_) => true,
            _ => false,
        }
    }
}

/// Describes a keyboard input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyboardInput {
    pub state: InputState,
    pub key: Option<Key>,
    pub modifiers: ModifiersState,
}

/// Describes the input state of a key.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum InputState {
    Pressed,
    Released,
    Repeated,
}

/// Describes a mouse button.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Symbolic name for a keyboard key.
#[derive(Debug, Hash, Ord, PartialOrd, PartialEq, Eq, Clone, Copy)]
#[repr(u32)]
#[rustfmt::skip]
pub enum Key {
    // Number keys.
    Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0,

    // Alpha keys.
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,

    // Arrow keys.
    Left, Up, Right, Down,

    // Control characters.
    Backspace, Return, Space, Tab,
    Escape, Insert, Home, Delete, End, PageDown, PageUp,

    // Punctuation.
    Apostrophe, Grave, Caret, Comma, Period, Colon, Semicolon,
    LBracket, RBracket,
    Slash, Backslash,

    // Modifiers.
    Alt, Control, Shift,

    // Math keys.
    Equal, Minus,

    // Key is unknown/unsupported.
    Unknown,
}

impl From<char> for Key {
    #[rustfmt::skip]
    fn from(c: char) -> Self {
        match c {
            '0' => Key::Num0, '1' => Key::Num1, '2' => Key::Num2,
            '3' => Key::Num3, '4' => Key::Num4, '5' => Key::Num5,
            '6' => Key::Num6, '7' => Key::Num7, '8' => Key::Num8,
            '9' => Key::Num9,

            'a' => Key::A, 'b' => Key::B, 'c' => Key::C, 'd' => Key::D,
            'e' => Key::E, 'f' => Key::F, 'g' => Key::G, 'h' => Key::H,
            'i' => Key::I, 'j' => Key::J, 'k' => Key::K, 'l' => Key::L,
            'm' => Key::M, 'n' => Key::N, 'o' => Key::O, 'p' => Key::P,
            'q' => Key::Q, 'r' => Key::R, 's' => Key::S, 't' => Key::T,
            'u' => Key::U, 'v' => Key::V, 'w' => Key::W, 'x' => Key::X,
            'y' => Key::Y, 'z' => Key::Z,

            '/' => Key::Slash, '[' => Key::LBracket, ']' => Key::RBracket,
            '`' => Key::Grave, ',' => Key::Comma, '.' => Key::Period,
            '=' => Key::Equal, '-' => Key::Minus, '\'' => Key::Apostrophe,
            ';' => Key::Semicolon, ':' => Key::Colon, ' ' => Key::Space,
            '\\' => Key::Backslash,
            _ => Key::Unknown,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::A => "a".fmt(f),
            Key::B => "b".fmt(f),
            Key::C => "c".fmt(f),
            Key::D => "d".fmt(f),
            Key::E => "e".fmt(f),
            Key::F => "f".fmt(f),
            Key::G => "g".fmt(f),
            Key::H => "h".fmt(f),
            Key::I => "i".fmt(f),
            Key::J => "j".fmt(f),
            Key::K => "k".fmt(f),
            Key::L => "l".fmt(f),
            Key::M => "m".fmt(f),
            Key::N => "n".fmt(f),
            Key::O => "o".fmt(f),
            Key::P => "p".fmt(f),
            Key::Q => "q".fmt(f),
            Key::R => "r".fmt(f),
            Key::S => "s".fmt(f),
            Key::T => "t".fmt(f),
            Key::U => "u".fmt(f),
            Key::V => "v".fmt(f),
            Key::W => "w".fmt(f),
            Key::X => "x".fmt(f),
            Key::Y => "y".fmt(f),
            Key::Z => "z".fmt(f),
            Key::Num0 => "0".fmt(f),
            Key::Num1 => "1".fmt(f),
            Key::Num2 => "2".fmt(f),
            Key::Num3 => "3".fmt(f),
            Key::Num4 => "4".fmt(f),
            Key::Num5 => "5".fmt(f),
            Key::Num6 => "6".fmt(f),
            Key::Num7 => "7".fmt(f),
            Key::Num8 => "8".fmt(f),
            Key::Num9 => "9".fmt(f),
            Key::LBracket => "[".fmt(f),
            Key::RBracket => "]".fmt(f),
            Key::Comma => ",".fmt(f),
            Key::Period => ".".fmt(f),
            Key::Slash => "/".fmt(f),
            Key::Backslash => "\\".fmt(f),
            Key::Apostrophe => "'".fmt(f),
            Key::Control => "<ctrl>".fmt(f),
            Key::Shift => "<shift>".fmt(f),
            Key::Alt => "<alt>".fmt(f),
            Key::Up => "<up>".fmt(f),
            Key::Down => "<down>".fmt(f),
            Key::Left => "<left>".fmt(f),
            Key::Right => "<right>".fmt(f),
            Key::Return => "<return>".fmt(f),
            Key::Backspace => "<backspace>".fmt(f),
            Key::Space => "<space>".fmt(f),
            Key::Tab => "<tab>".fmt(f),
            Key::Escape => "<esc>".fmt(f),
            Key::Insert => "<insert>".fmt(f),
            Key::Delete => "<delete>".fmt(f),
            Key::Home => "<home>".fmt(f),
            Key::PageUp => "<pgup>".fmt(f),
            Key::PageDown => "<pgdown>".fmt(f),
            Key::Grave => "`".fmt(f),
            Key::Caret => "^".fmt(f),
            Key::End => "<end>".fmt(f),
            Key::Colon => ":".fmt(f),
            Key::Semicolon => ";".fmt(f),
            Key::Equal => "=".fmt(f),
            Key::Minus => "-".fmt(f),
            _ => "???".fmt(f),
        }
    }
}

impl Key {
    pub fn is_modifier(self) -> bool {
        match self {
            Key::Alt | Key::Control | Key::Shift => true,
            _ => false,
        }
    }
}

/// Represents the current state of the keyboard modifiers
#[derive(Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct ModifiersState {
    /// The "shift" key
    pub shift: bool,
    /// The "control" key
    pub ctrl: bool,
    /// The "alt" key
    pub alt: bool,
    /// The "meta" key. This is the "windows" key on PC and "command" key on Mac.
    pub meta: bool,
}

impl fmt::Display for ModifiersState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("<ctrl>");
        }
        if self.alt {
            s.push_str("<alt>");
        }
        if self.meta {
            s.push_str("<meta>");
        }
        if self.shift {
            s.push_str("<shift>");
        }
        s.fmt(f)
    }
}

/// A delta represented in logical pixels.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LogicalDelta {
    pub x: f64,
    pub y: f64,
}

/// A position represented in logical pixels.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LogicalPosition {
    pub x: f64,
    pub y: f64,
}

impl LogicalPosition {
    pub fn new(x: f64, y: f64) -> Self {
        LogicalPosition { x, y }
    }

    pub fn from_physical<T: Into<PhysicalPosition>>(physical: T, scale_factor: f64) -> Self {
        physical.into().to_logical(self::pixel_ratio(scale_factor))
    }

    pub fn to_physical(&self, scale_factor: f64) -> PhysicalPosition {
        let x = self.x * self::pixel_ratio(scale_factor);
        let y = self.y * self::pixel_ratio(scale_factor);
        PhysicalPosition::new(x, y)
    }
}

/// A position represented in physical pixels.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PhysicalPosition {
    pub x: f64,
    pub y: f64,
}

impl PhysicalPosition {
    pub fn new(x: f64, y: f64) -> Self {
        PhysicalPosition { x, y }
    }

    pub fn from_logical<T: Into<LogicalPosition>>(logical: T, scale_factor: f64) -> Self {
        logical.into().to_physical(self::pixel_ratio(scale_factor))
    }

    pub fn to_logical(&self, scale_factor: f64) -> LogicalPosition {
        let x = self.x / self::pixel_ratio(scale_factor);
        let y = self.y / self::pixel_ratio(scale_factor);
        LogicalPosition::new(x, y)
    }
}

/// A size represented in logical pixels.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LogicalSize {
    pub width: f64,
    pub height: f64,
}

impl LogicalSize {
    pub const fn new(width: f64, height: f64) -> Self {
        LogicalSize { width, height }
    }

    pub fn from_physical<T: Into<PhysicalSize>>(physical: T, scale_factor: f64) -> Self {
        physical.into().to_logical(self::pixel_ratio(scale_factor))
    }

    pub fn to_physical(&self, scale_factor: f64) -> PhysicalSize {
        let width = self.width * self::pixel_ratio(scale_factor);
        let height = self.height * self::pixel_ratio(scale_factor);
        PhysicalSize::new(width, height)
    }

    pub fn is_zero(&self) -> bool {
        self.width < 1. || self.height < 1.
    }
}

impl From<(u32, u32)> for LogicalSize {
    fn from((width, height): (u32, u32)) -> Self {
        Self::new(width as f64, height as f64)
    }
}

impl Into<(u32, u32)> for LogicalSize {
    /// Note that this rounds instead of truncating.
    fn into(self) -> (u32, u32) {
        (self.width.round() as _, self.height.round() as _)
    }
}

/// A size represented in physical pixels.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PhysicalSize {
    pub width: f64,
    pub height: f64,
}

impl PhysicalSize {
    pub fn new(width: f64, height: f64) -> Self {
        PhysicalSize { width, height }
    }

    pub fn from_logical<T: Into<LogicalSize>>(logical: T, scale_factor: f64) -> Self {
        logical.into().to_physical(self::pixel_ratio(scale_factor))
    }

    pub fn to_logical(&self, scale_factor: f64) -> LogicalSize {
        let width = self.width / self::pixel_ratio(scale_factor);
        let height = self.height / self::pixel_ratio(scale_factor);
        LogicalSize::new(width, height)
    }
}

impl From<(u32, u32)> for PhysicalSize {
    fn from((width, height): (u32, u32)) -> Self {
        Self::new(width as f64, height as f64)
    }
}

impl Into<(u32, u32)> for PhysicalSize {
    /// Note that this rounds instead of truncating.
    fn into(self) -> (u32, u32) {
        (self.width.round() as _, self.height.round() as _)
    }
}

/// The ratio between screen coordinates and pixels, given the
/// content scale.
/// On macOS, screen coordinates don't map 1:1 with pixels. Hence,
/// our ratio between screen coordinates and pixels is whatever
/// the scaling factor is, which is always `2.0` on modern hardware.
#[cfg(target_os = "macos")]
pub fn pixel_ratio(scale_factor: f64) -> f64 {
    scale_factor
}

/// The ratio between screen coordinates and pixels, given the
/// content scale.
/// On Linux and Windows, screen coordinates always map 1:1 with pixels.
/// No matter the DPI settings and display, we always want to map a screen
/// coordinate with a single pixel.
#[cfg(not(target_os = "macos"))]
pub fn pixel_ratio(_scale_factor: f64) -> f64 {
    1.0
}
