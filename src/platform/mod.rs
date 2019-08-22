#![allow(dead_code)]
use std::io;

#[cfg(feature = "winit")]
mod winit;
#[cfg(feature = "winit")]
use crate::platform::winit as backend;

/// Initialize the platform.
pub fn init(title: &str) -> io::Result<(backend::Window, backend::Events)> {
    backend::init(title)
}

/// Describes an event from a `Window`.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The size of the window has changed. Contains the client area's new dimensions.
    Resized(LogicalSize),

    /// The position of the window has changed. Contains the window's new position.
    Moved(LogicalPosition),

    /// The window has been requested to close.
    CloseRequested,

    /// The window has been destroyed.
    Destroyed,

    /// The window received a unicode character.
    ReceivedCharacter(char),

    /// The window gained or lost focus.
    Focused(bool),

    /// An event from the keyboard has been received.
    KeyboardInput(KeyboardInput),

    /// The cursor has moved on the window.
    CursorMoved {
        /// Coords in pixels relative to the top-left corner of the window.
        position: LogicalPosition,
        modifiers: ModifiersState,
    },

    /// The cursor has entered the window.
    CursorEntered,

    /// The cursor has left the window.
    CursorLeft,

    /// An mouse button press has been received.
    MouseInput {
        state: InputState,
        button: MouseButton,
        modifiers: ModifiersState,
    },

    /// The OS or application has requested that the window be redrawn.
    RedrawRequested,

    /// The DPI factor of the window has changed.
    HiDpiFactorChanged(f64),

    /// No-op event, for events we don't handle.
    Noop,
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
    LAlt, RAlt,
    LControl, RControl,
    LShift, RShift,

    // Math keys.
    Divide, Equals, Add, Minus, Subtract, Multiply,

    // Key is unknown/unsupported.
    Unknown,
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

    pub fn from_physical<T: Into<PhysicalPosition>>(
        physical: T,
        dpi_factor: f64,
    ) -> Self {
        physical.into().to_logical(dpi_factor)
    }

    pub fn to_physical(&self, dpi_factor: f64) -> PhysicalPosition {
        let x = self.x * dpi_factor;
        let y = self.y * dpi_factor;
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

    pub fn from_logical<T: Into<LogicalPosition>>(
        logical: T,
        dpi_factor: f64,
    ) -> Self {
        logical.into().to_physical(dpi_factor)
    }

    pub fn to_logical(&self, dpi_factor: f64) -> LogicalPosition {
        let x = self.x / dpi_factor;
        let y = self.y / dpi_factor;
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
    pub fn new(width: f64, height: f64) -> Self {
        LogicalSize { width, height }
    }

    pub fn from_physical<T: Into<PhysicalSize>>(
        physical: T,
        dpi_factor: f64,
    ) -> Self {
        physical.into().to_logical(dpi_factor)
    }

    pub fn to_physical(&self, dpi_factor: f64) -> PhysicalSize {
        let width = self.width * dpi_factor;
        let height = self.height * dpi_factor;
        PhysicalSize::new(width, height)
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

    pub fn from_logical<T: Into<LogicalSize>>(
        logical: T,
        dpi_factor: f64,
    ) -> Self {
        logical.into().to_physical(dpi_factor)
    }

    pub fn to_logical(&self, dpi_factor: f64) -> LogicalSize {
        let width = self.width / dpi_factor;
        let height = self.height / dpi_factor;
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
