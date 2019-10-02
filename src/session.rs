///! Session
use crate::brush::*;
use crate::cmd;
use crate::cmd::{Command, CommandLine, Key, Op, Value};
use crate::color;
use crate::data;
use crate::hashmap;
use crate::palette::*;
use crate::platform;
use crate::platform::{InputState, KeyboardInput, ModifiersState, WindowEvent};
use crate::resources::ResourceManager;
use crate::view::{FileStatus, View, ViewCoords, ViewId, ViewManager};

use rgx::core::{PresentMode, Rect};
use rgx::kit::{Bgra8, Rgba8};
use rgx::math::*;

use directories as dirs;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::ops::{Add, Deref, Sub};
use std::path::Path;
use std::str::FromStr;
use std::time;
use std::ffi::OsString;

/// Help string.
pub const HELP: &'static str = r#"
:help                    Toggle this help
:e <path..>              Edit path(s)
:w [<path>]              Write view / Write view as <path>
:q                       Quit view
:q!                      Force quit view
:echo <val>              Echo a value
:echo "pixel!"           Echo the string "pixel!"
:set <setting> = <val>   Set <setting> to <val>
:set <setting>           Set <setting> to `on`
:unset <setting>         Set <setting> to `off`
:toggle <setting>        Toggle <setting> `on` / `off`
:slice <n>               Slice view into <n> frames
:source <path>           Source an rx script (eg. a palette or config)
:map <key> <command>     Map a key combination to a command
:f/resize <w> <h>        Resize frames
:f/add                   Add a blank frame to the view
:f/remove                Remove the last frame of the view
:f/clone <index>         Clone frame <index> and add it to the view
:f/clone                 Clone the last frame and add it to the view
:p/clear                 Clear the palette
:p/add <color>           Add <color> to the palette, eg. #ff0011
:brush/set <mode>        Set brush mode, eg. `xsym` and `ysym` for symmetry
:brush/unset <mode>      Unset brush mode

SETTINGS

debug             on/off        Debug mode
checker           on/off        Alpha checker toggle
vsync             on/off        Vertical sync toggle
input/delay       0.0..32.0     Delay between render frames (ms)
scale             1.0..4.0      UI scale
animation         on/off        View animation toggle
animation/delay   1..1000       View animation delay (ms)
"#;

/// An RGB 8-bit color. Used when the alpha value isn't used.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Rgb8 {
    r: u8,
    g: u8,
    b: u8,
}

impl From<Rgba8> for Rgb8 {
    fn from(rgba: Rgba8) -> Self {
        Self {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
        }
    }
}

impl ToString for Rgb8 {
    fn to_string(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// Session coordinates.
/// Encompasses anything within the window, such as the cursor position.
#[derive(Copy, Clone, PartialEq)]
pub struct SessionCoords(Point2<f32>);

impl SessionCoords {
    pub fn new(x: f32, y: f32) -> Self {
        Self(Point2::new(x, y))
    }

    pub fn floor(&mut self) -> Self {
        Self(self.0.map(f32::floor))
    }
}

impl Deref for SessionCoords {
    type Target = Point2<f32>;

    fn deref(&self) -> &Point2<f32> {
        &self.0
    }
}

impl Add<Vector2<f32>> for SessionCoords {
    type Output = Self;

    fn add(self, vec: Vector2<f32>) -> Self {
        SessionCoords(self.0 + vec)
    }
}

impl Sub<Vector2<f32>> for SessionCoords {
    type Output = Self;

    fn sub(self, vec: Vector2<f32>) -> Self {
        SessionCoords(self.0 - vec)
    }
}

///////////////////////////////////////////////////////////////////////////////

/// An editing mode the `Session` can be in.
/// Some of these modes are inspired by vi.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Mode {
    /// Allows the user to paint pixels.
    Normal,
    /// Allows pixels to be selected, copied and manipulated visually.
    #[allow(dead_code)]
    Visual,
    /// Allows commands to be run.
    Command,
    /// Used to present work.
    #[allow(dead_code)]
    Present,
    /// Activated with the `:help` command.
    Help,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Normal
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => "normal".fmt(f),
            Self::Visual => "visual".fmt(f),
            Self::Command => "command".fmt(f),
            Self::Present => "present".fmt(f),
            Self::Help => "help".fmt(f),
        }
    }
}

/// Session state.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum State {
    /// The session is initializing.
    Initializing,
    /// The session is running normally.
    Running,
    /// The session is paused. Inputs are not processed.
    Paused,
    /// The session is being shut down.
    Closing,
}

/// An editing tool.
#[derive(Debug, Clone)]
pub enum Tool {
    /// The standard drawing tool.
    Brush(Brush),
    /// Used to sample colors.
    Sampler,
    /// Used to pan the workspace.
    #[allow(dead_code)]
    Pan,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Brush(Brush::default())
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A message to the user, displayed in the session.
pub struct Message {
    /// The message string.
    string: String,
    /// The message type.
    message_type: MessageType,
}

impl Message {
    /// Create a new message.
    pub fn new<D: fmt::Display>(s: D, t: MessageType) -> Self {
        Message {
            string: format!("{}", s),
            message_type: t,
        }
    }

    /// Return the color of a message.
    pub fn color(&self) -> Rgba8 {
        self.message_type.color()
    }

    /// Log a message to stdout/stderr.
    fn log(&self) {
        match self.message_type {
            MessageType::Info => info!("{}", self),
            MessageType::Hint => {}
            MessageType::Echo => info!("{}", self),
            MessageType::Error => error!("{}", self),
            MessageType::Warning => warn!("{}", self),
            MessageType::Replay => trace!("{}", self),
            MessageType::Okay => info!("{}", self),
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Message::new("", MessageType::Info)
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}

/// The type of a `Message`.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum MessageType {
    /// A hint that can be ignored.
    Hint,
    /// Informational message.
    Info,
    /// A message that is displayed by the `:echo` command.
    Echo,
    /// An error message.
    Error,
    /// Non-critical warning.
    #[allow(dead_code)]
    Warning,
    #[allow(dead_code)]
    Replay,
    #[allow(dead_code)]
    Okay,
}

impl MessageType {
    /// Returns the color associated with a `MessageType`.
    fn color(&self) -> Rgba8 {
        match *self {
            MessageType::Info => color::LIGHT_GREY,
            MessageType::Hint => color::DARK_GREY,
            MessageType::Echo => color::LIGHT_GREEN,
            MessageType::Error => color::RED,
            MessageType::Warning => color::YELLOW,
            MessageType::Replay => color::GREY,
            MessageType::Okay => color::GREEN,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A session error.
type Error = String;

/// A key binding.
#[derive(PartialEq, Clone, Debug)]
pub struct KeyBinding {
    /// The `Mode`s this binding applies to.
    pub modes: Vec<Mode>,
    /// Modifiers which must be held.
    pub modifiers: ModifiersState,
    /// Key which must be pressed or released.
    pub key: Key,
    /// Whether the key should be pressed or released.
    pub state: InputState,
    /// The `Command` to run when this binding is triggered.
    pub command: Command,
    /// Whether this key binding controls a toggle.
    pub is_toggle: bool,
    /// How this key binding should be displayed to the user.
    /// If `None`, then this binding shouldn't be shown to the user.
    pub display: Option<String>,
}

/// Manages a list of key bindings.
#[derive(Debug)]
pub struct KeyBindings {
    elems: Vec<KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        // The only defaults are switching to command mode and 'help'. On some platforms,
        // Pressing `<shift> + ;` sends us a `:` directly, while on others
        // we get `<shift>` and `;`.
        KeyBindings {
            elems: vec![
                KeyBinding {
                    modes: vec![Mode::Normal],
                    modifiers: ModifiersState {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        meta: false,
                    },
                    key: Key::Virtual(platform::Key::Slash),
                    state: InputState::Pressed,
                    command: Command::Help,
                    is_toggle: true,
                    display: Some("?".to_string()),
                },
                KeyBinding {
                    modes: vec![Mode::Help],
                    modifiers: ModifiersState {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        meta: false,
                    },
                    key: Key::Virtual(platform::Key::Slash),
                    state: InputState::Released,
                    command: Command::Help,
                    is_toggle: true,
                    display: None,
                },
                KeyBinding {
                    modes: vec![Mode::Normal],
                    modifiers: ModifiersState::default(),
                    key: Key::Virtual(platform::Key::Colon),
                    state: InputState::Pressed,
                    command: Command::Mode(Mode::Command),
                    is_toggle: false,
                    display: None,
                },
                KeyBinding {
                    modes: vec![Mode::Normal],
                    modifiers: ModifiersState {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        meta: false,
                    },
                    key: Key::Virtual(platform::Key::Semicolon),
                    state: InputState::Pressed,
                    command: Command::Mode(Mode::Command),
                    is_toggle: false,
                    display: Some(":".to_string()),
                },
            ],
        }
    }
}

impl KeyBindings {
    /// Add a key binding.
    pub fn add(&mut self, binding: KeyBinding) {
        self.elems.push(binding);
    }

    /// Find a key binding based on some input state.
    pub fn find(
        &self,
        key: Key,
        modifiers: ModifiersState,
        state: InputState,
        mode: &Mode,
    ) -> Option<KeyBinding> {
        self.elems.iter().cloned().find(|kb| {
            kb.key == key
                && (kb.modifiers == ModifiersState::default()
                    || kb.modifiers == modifiers)
                && kb.state == state
                && kb.modes.contains(mode)
        })
    }

    /// Iterate over all key bindings.
    pub fn iter(&self) -> std::slice::Iter<'_, KeyBinding> {
        self.elems.iter()
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A dictionary used to store session settings.
pub struct Settings {
    map: HashMap<String, Value>,
}

impl Settings {
    const DEPRECATED: &'static [&'static str] = &["frame_delay"];

    /// Presentation mode.
    pub fn present_mode(&self) -> PresentMode {
        if self["vsync"].is_set() {
            PresentMode::Vsync
        } else {
            PresentMode::NoVsync
        }
    }

    /// Lookup a setting.
    pub fn get(&self, setting: &str) -> Option<&Value> {
        self.map.get(setting)
    }

    /// Set an existing setting to a new value. Returns `Err` if there is a type
    /// mismatch or the setting isn't found. Otherwise, returns `Ok` with the
    /// old value.
    pub fn set(&mut self, k: &str, v: Value) -> Result<Value, Error> {
        if let Some(current) = self.get(k) {
            if std::mem::discriminant(&v) == std::mem::discriminant(current) {
                return Ok(self.map.insert(k.to_string(), v).unwrap());
            }
            Err(format!(
                "invalid value `{}`, expected {}",
                v,
                current.description()
            ))
        } else {
            Err(format!("no such setting `{}`", k))
        }
    }
}

impl Default for Settings {
    /// The default settings.
    fn default() -> Self {
        Self {
            map: hashmap! {
                "debug" => Value::Bool(false),
                "checker" => Value::Bool(false),
                "vsync" => Value::Bool(false),
                "input/delay" => Value::Float(8.0),
                "scale" => Value::Float(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::U32(160),

                // Deprecated.
                "frame_delay" => Value::Float(0.0)
            },
        }
    }
}

impl std::ops::Index<&str> for Settings {
    type Output = Value;

    fn index(&self, setting: &str) -> &Self::Output {
        &self
            .get(setting)
            .expect(&format!("setting {} should exist", setting))
    }
}

///////////////////////////////////////////////////////////////////////////////

/// The user session.
///
/// Stores all relevant session state.
pub struct Session {
    /// The current session `Mode`.
    pub mode: Mode,
    /// The current session `State`.
    pub state: State,

    /// The width of the session workspace.
    pub width: f32,
    /// The height of the session workspace.
    pub height: f32,

    /// The HiDPI factor of the host.
    pub hidpi_factor: f64,

    /// The cursor coordinates.
    pub cursor: SessionCoords,

    /// The color under the cursor, if any.
    pub hover_color: Option<Rgba8>,
    /// The view under the cursor, if any.
    pub hover_view: Option<ViewId>,

    /// The workspace offset. Views are offset by this vector.
    pub offset: Vector2<f32>,
    /// The current message displayed to the user.
    pub message: Message,

    /// The session foreground color.
    pub fg: Rgba8,
    /// The session background color.
    pub bg: Rgba8,

    /// Directories in which user configuration is stored.
    base_dirs: dirs::ProjectDirs,

    /// Resources shared with the `Renderer`.
    resources: ResourceManager,

    /// Whether we should ignore characters received.
    ignore_received_characters: bool,
    /// The set of keys currently pressed.
    keys_pressed: HashSet<platform::Key>,
    /// The list of all active key bindings.
    pub key_bindings: KeyBindings,

    /// Current pixel selection.
    pub selection: Rect<f32>,

    /// The session's current settings.
    pub settings: Settings,
    /// Settings recently changed.
    pub settings_changed: HashSet<String>,

    /// Views loaded in the session.
    pub views: ViewManager,

    /// The current state of the command line.
    pub cmdline: CommandLine,
    /// The color palette.
    pub palette: Palette,

    /// Set to `true` if a view was added or removed from the session.
    pub dirty: bool,

    /// Average time it takes for a session update.
    pub avg_time: time::Duration,

    /// The current tool.
    pub tool: Tool,
    /// The previous tool, if any.
    pub prev_tool: Option<Tool>,

    /// Whether session inputs are being throttled.
    throttled: Option<time::Instant>,
    /// Set to `true` if the mouse button is pressed.
    mouse_down: bool,

    #[allow(dead_code)]
    _paste: (),
    #[allow(dead_code)]
    _onion: bool,
    #[allow(dead_code)]
    _recording: bool,
    #[allow(dead_code)]
    _recording_opts: u32,
    #[allow(dead_code)]
    _grid_w: u32,
    #[allow(dead_code)]
    _grid_h: u32,
    #[allow(dead_code)]
    _mouse_selection: Rect<f32>,
    #[allow(dead_code)]
    _frame_count: u64,
}

impl Session {
    /// Maximum number of views in a session.
    pub const MAX_VIEWS: usize = 64;
    /// Default view width.
    pub const DEFAULT_VIEW_W: u32 = 128;
    /// Default view height.
    pub const DEFAULT_VIEW_H: u32 = 128;

    /// Supported image formats for writing.
    const SUPPORTED_FORMATS: &'static [&'static str] = &["png", "gif"];
    /// Minimum margin between views, in pixels.
    const VIEW_MARGIN: f32 = 24.;
    /// Size of palette cells, in pixels.
    const PALETTE_CELL_SIZE: f32 = 24.;
    /// Distance to pan when using keyboard.
    const PAN_PIXELS: i32 = 32;
    /// Minimum brush size.
    const MIN_BRUSH_SIZE: usize = 1;
    /// Maximum zoom amount as a multiplier.
    const MAX_ZOOM: f32 = 128.0;
    /// Zoom levels used when zooming in/out.
    const ZOOM_LEVELS: &'static [f32] = &[
        1.,
        2.,
        3.,
        4.,
        6.,
        8.,
        10.,
        12.,
        16.,
        20.,
        24.,
        32.,
        64.,
        Self::MAX_ZOOM,
    ];
    /// Minimum time to wait between invocations of a throttled command.
    const THROTTLE_TIME: time::Duration = time::Duration::from_millis(96);

    /// Name of rx initialization script.
    const INIT: &'static str = "init.rx";

    /// Create a new un-initialized session.
    pub fn new(
        w: u32,
        h: u32,
        hidpi_factor: f64,
        resources: ResourceManager,
        base_dirs: dirs::ProjectDirs,
    ) -> Self {
        Self {
            state: State::Initializing,
            width: w as f32,
            height: h as f32,
            hidpi_factor,
            cursor: SessionCoords::new(0., 0.),
            base_dirs,
            offset: Vector2::zero(),
            tool: Tool::Brush(Brush::default()),
            prev_tool: None,
            mouse_down: false,
            hover_color: None,
            hover_view: None,
            fg: color::WHITE,
            bg: color::BLACK,
            settings: Settings::default(),
            settings_changed: HashSet::new(),
            views: ViewManager::new(),
            palette: Palette::new(Self::PALETTE_CELL_SIZE),
            key_bindings: KeyBindings::default(),
            keys_pressed: HashSet::new(),
            ignore_received_characters: false,
            cmdline: CommandLine::new(),
            throttled: None,
            mode: Mode::Normal,
            selection: Rect::new(0., 0., 0., 0.),
            message: Message::default(),
            resources,
            dirty: true,
            avg_time: time::Duration::from_secs(0),

            // Unused
            _onion: false,
            _mouse_selection: Rect::new(0., 0., 0., 0.),
            _frame_count: 0,
            _grid_w: 0,
            _grid_h: 0,
            _paste: (),
            _recording: false,
            _recording_opts: 0,
        }
    }

    /// Initialize a session.
    pub fn init(mut self) -> std::io::Result<Self> {
        self.transition(State::Running);

        let cwd = std::env::current_dir()?;
        let dir = self.base_dirs.config_dir();
        let cfg = dir.join(Self::INIT);

        if cfg.exists() {
            self.source_path(cfg)?;
        } else {
            if let Err(e) = fs::create_dir_all(dir)
                .and_then(|_| fs::write(&cfg, data::CONFIG))
            {
                warn!(
                    "Warning: couldn't create configuration file {:?}: {}",
                    cfg, e
                );
            }
            self.source_reader(io::BufReader::new(data::CONFIG), "<init>")?;
        }
        self.source_dir(cwd).ok();
        self.message(format!("rx v{}", crate::VERSION), MessageType::Echo);

        Ok(self)
    }

    /// Create a blank view.
    pub fn blank(&mut self, fs: FileStatus, w: u32, h: u32) {
        let id = self.views.add(fs, w, h);

        self.resources.add_blank_view(id, w, h);
        self.organize_views();
        self.edit_view(id);

        self.dirty = true;
    }

    /// Transition to a new state. Only allows valid state transitions.
    pub fn transition(&mut self, to: State) {
        let state = match (self.state, to) {
            (State::Initializing, State::Running)
            | (State::Running, State::Paused)
            | (State::Paused, State::Running)
            | (State::Paused, State::Closing)
            | (State::Running, State::Closing) => to,
            _ => self.state,
        };
        debug!("state: {:?} -> {:?}", self.state, state);

        self.state = state;
    }

    /// Update the session by processing new user events and advancing
    /// the internal state.
    pub fn update(
        &mut self,
        events: &mut Vec<platform::WindowEvent>,
        delta: time::Duration,
        avg_time: time::Duration,
    ) {
        self.dirty = false;
        self.settings_changed.clear();
        self.avg_time = avg_time;

        if let Tool::Brush(ref mut b) = self.tool {
            b.update();
        }

        for (_, v) in self.views.iter_mut() {
            v.okay();

            if self.settings["animation"].is_set() {
                v.update(delta);
            }
        }

        for event in events.drain(..) {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let scale: f64 = self.settings["scale"].float64();
                    self.handle_cursor_moved(SessionCoords::new(
                        (position.x / scale).floor() as f32,
                        self.height - (position.y / scale).floor() as f32 - 1.,
                    ));
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    self.handle_mouse_input(button, state);
                }
                WindowEvent::KeyboardInput(input) => {
                    self.handle_keyboard_input(input);
                }
                WindowEvent::ReceivedCharacter(c) => {
                    self.handle_received_character(c);
                }
                WindowEvent::HiDpiFactorChanged(factor) => {
                    self.hidpi_factor = factor;
                }
                WindowEvent::CloseRequested => {
                    self.quit();
                }
                _ => {}
            }
        }
        if self.views.is_empty() {
            self.quit();
        }

        // Make sure we don't have rounding errors
        debug_assert_eq!(self.offset, self.offset.map(|a| a.floor()));
    }

    /// Quit the session.
    pub fn quit(&mut self) {
        self.transition(State::Closing);
    }

    /// Return the session offset as a transformation matrix.
    pub fn transform(&self) -> Matrix4<f32> {
        Matrix4::from_translation(self.offset.extend(0.))
    }

    /// Snap the given session coordinates to the pixel grid.
    /// This only has an effect at zoom levels greater than `1.0`.
    #[allow(dead_code)]
    pub fn snap(
        &self,
        p: SessionCoords,
        offx: f32,
        offy: f32,
        zoom: f32,
    ) -> SessionCoords {
        SessionCoords::new(
            p.x - ((p.x - offx - self.offset.x) % zoom),
            p.y - ((p.y - offy - self.offset.y) % zoom),
        )
        .floor()
    }

    ////////////////////////////////////////////////////////////////////////////

    /// Pan the view by a relative amount.
    fn pan(&mut self, x: f32, y: f32) {
        self.offset.x += x;
        self.offset.y += y;

        self.cursor_dirty();
    }

    /// Set the user cursor to the given position.
    fn set_cursor(&mut self, cursor: SessionCoords) {
        if self.cursor == cursor {
            return;
        }

        let p = self.active_view_coords(cursor);
        let prev_p = self.active_view_coords(self.cursor);
        let (vw, vh) = self.active_view().size();

        match self.mode {
            Mode::Normal => match self.tool {
                Tool::Brush(ref mut brush) => {
                    if p != prev_p {
                        match brush.state {
                            BrushState::DrawStarted { .. }
                            | BrushState::Drawing { .. } => {
                                let mut p: ViewCoords<i32> = p.into();

                                if brush.is_set(BrushMode::Multi) {
                                    p.clamp(Rect::new(
                                        (brush.size / 2) as i32,
                                        (brush.size / 2) as i32,
                                        vw as i32 - (brush.size / 2) as i32 - 1,
                                        vh as i32 - (brush.size / 2) as i32 - 1,
                                    ));
                                    brush.draw(p);
                                } else {
                                    brush.draw(p);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Tool::Pan => {
                    self.pan(
                        cursor.x - self.cursor.x,
                        cursor.y - self.cursor.y,
                    );
                }
                Tool::Sampler => {}
            },
            Mode::Visual => {}
            _ => {}
        }
        self.cursor = cursor;
        self.cursor_dirty();
    }

    /// Re-compute state related to the cursor position. This is useful
    /// when the cursor hasn't moved relative to the session, but things
    /// within the session have moved relative to the cursor.
    fn cursor_dirty(&mut self) {
        let cursor = self.cursor;

        self.palette.handle_cursor_moved(cursor);
        self.hover_view = None;

        for (_, v) in self.views.iter_mut() {
            if v.contains(cursor - self.offset) {
                self.hover_view = Some(v.id);
                break;
            }
        }

        self.hover_color = if self.palette.hover.is_some() {
            self.palette.hover
        } else if let Some(v) = self.hover_view {
            let p: ViewCoords<u32> = self.view_coords(v, cursor).into();
            Some(self.color_at(v, p))
        } else {
            None
        };
    }

    /// Called when settings have been changed.
    fn setting_changed(&mut self, name: &str, old: &Value, new: &Value) {
        debug!("set `{}`: {} -> {}", name, old, new);

        self.settings_changed.insert(name.to_owned());

        match name {
            "animation/delay" => {
                self.active_view_mut().set_animation_delay(new.uint64());
            }
            _ => {}
        }
    }

    /// Switch the session mode.
    fn switch_mode(&mut self, mode: Mode) {
        let (old, new) = (self.mode, mode);
        if old == new {
            return;
        }

        match old {
            Mode::Command => {
                self.selection = Rect::empty();
                self.cmdline.clear();
            }
            _ => {}
        }

        match new {
            Mode::Command => {
                // When switching to command mode via the keyboard, we simultaneously
                // also receive the character input equivalent of the key pressed.
                // This input, since we are now in command mode, is processed as
                // text input to the command line. To avoid this, we have to ignore
                // all such input until the end of the current upate.
                self.ignore_received_characters = true;
                self.cmdline.putc(':');
            }
            _ => {}
        }

        let pressed: Vec<platform::Key> =
            self.keys_pressed.iter().cloned().collect();
        for k in pressed {
            self.handle_keyboard_input(platform::KeyboardInput {
                key: Some(k),
                modifiers: ModifiersState::default(),
                state: InputState::Released,
            });
        }

        self.mode = new;
    }

    fn record_macro(&mut self, _cmd: String) {}

    ///////////////////////////////////////////////////////////////////////////////
    /// Messages
    ///////////////////////////////////////////////////////////////////////////////

    /// Display a message to the user. Also logs.
    pub fn message<D: fmt::Display>(&mut self, msg: D, t: MessageType) {
        self.message = Message::new(msg, t);
        self.message.log();
    }

    fn message_clear(&mut self) {
        self.message = Message::default();
    }

    fn unimplemented(&mut self) {
        self.message("Error: not yet implemented", MessageType::Error);
    }

    ///////////////////////////////////////////////////////////////////////////////
    /// View functions
    ///////////////////////////////////////////////////////////////////////////////

    /// Get the view with the given id.
    ///
    /// # Panics
    ///
    /// Panics if the view isn't found.
    pub fn view(&self, id: ViewId) -> &View {
        self.views
            .get(&id)
            .expect(&format!("view #{} must exist", id))
    }

    /// Get the view with the given id (mutable).
    ///
    /// # Panics
    ///
    /// Panics if the view isn't found.
    pub fn view_mut(&mut self, id: ViewId) -> &mut View {
        self.views
            .get_mut(&id)
            .expect(&format!("view #{} must exist", id))
    }

    /// Get the currently active view.
    ///
    /// # Panics
    ///
    /// Panics if there is no active view.
    pub fn active_view(&self) -> &View {
        assert!(
            self.views.active_id != ViewId::default(),
            "fatal: no active view"
        );
        self.view(self.views.active_id)
    }

    /// Get the currently active view (mutable).
    ///
    /// # Panics
    ///
    /// Panics if there is no active view.
    pub fn active_view_mut(&mut self) -> &mut View {
        assert!(
            self.views.active_id != ViewId::default(),
            "fatal: no active view"
        );
        self.view_mut(self.views.active_id)
    }

    /// Check whether a view is active.
    pub fn is_active(&self, id: &ViewId) -> bool {
        &self.views.active_id == id
    }

    /// Convert session coordinates to view coordinates of the given view.
    pub fn view_coords(&self, v: ViewId, p: SessionCoords) -> ViewCoords<f32> {
        let v = self.view(v);
        let SessionCoords(mut p) = p;

        p = p - self.offset - v.offset;
        p = p / v.zoom;

        if v.flip_x {
            p.x = v.width() as f32 - p.x;
        }
        if v.flip_y {
            p.y = v.height() as f32 - p.y;
        }

        ViewCoords::new(p.x.floor(), p.y.floor())
    }

    /// Convert view coordinates to session coordinates.
    pub fn session_coords(
        &self,
        v: ViewId,
        p: ViewCoords<f32>,
    ) -> SessionCoords {
        let v = self.view(v);

        let p = Point2::new(p.x * v.zoom, p.y * v.zoom);
        let p = p + self.offset + v.offset;

        if v.flip_x {
            unimplemented!();
        }
        if v.flip_y {
            unimplemented!();
        }

        SessionCoords::new(p.x, p.y).floor()
    }

    /// Convert session coordinates to view coordinates of the active view.
    pub fn active_view_coords(&self, p: SessionCoords) -> ViewCoords<f32> {
        self.view_coords(self.views.active_id, p)
    }

    /// Expand paths, e.g. by converting ~ into the user's home directory path
    pub fn expand_path<P: AsRef<Path>>(&mut self, path: P) -> OsString {
            let mut path = path.as_ref();

            // for Linux and BSD and MacOS
            if cfg!(unix) && path.starts_with("~") {

                // ensure that the base directory list can be loaded by
                // ensuring it has at least one element
                if let Some(base_dirs) = dirs::BaseDirs::new() {

                    // alloc space + store a copy of home directory as an
                    // OsString, which are easily used with Path::new()
                    let home = base_dirs.home_dir().as_os_str().to_os_string();

                    if path == Path::new("~") {
                        return home;
                    }

                    path = &path.strip_prefix("~").unwrap();
                    return Path::new(&home).join(path).into_os_string();
                }
            }

            // for Windows and other platforms
            path.as_os_str().to_os_string()
    }

    /// Edit paths.
    ///
    /// Loads the given files into the session. Returns an error if one of
    /// the paths couldn't be loaded. If a path points to a directory,
    /// loads all files within that directory.
    ///
    /// If a path doesn't exist, creates a blank view for that path.
    pub fn edit<P: AsRef<Path>>(&mut self, paths: &[P]) -> io::Result<()> {
        // TODO: Keep loading paths even if some fail?
        for path in paths {
            let path_osstring = &self.expand_path(path);
            let path = Path::new(path_osstring);

            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();

                    if path.is_dir() {
                        continue;
                    }
                    self.load_view(path)?;
                }
                self.source_dir(path).ok();
            } else if path.exists() {
                self.load_view(path)?;
            } else {
                let (w, h) = if !self.views.is_empty() {
                    let v = self.active_view();
                    (v.width(), v.height())
                } else {
                    (Self::DEFAULT_VIEW_W, Self::DEFAULT_VIEW_H)
                };
                self.blank(
                    FileStatus::New(path.with_extension("png").into()),
                    w,
                    h,
                );
            }
        }
        Ok(())
    }

    /// Save the given view to disk with the current file name. Returns
    /// an error if the view has no file name.
    pub fn save_view(&mut self, id: ViewId) -> io::Result<()> {
        if let Some(ref f) = self.view(id).file_name().map(|f| f.clone()) {
            let path_osstring = &self.expand_path(f);
            let path = Path::new(path_osstring);
            self.save_view_as(id, path)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "no file name given"))
        }
    }

    /// Save a view with the given file name. Returns an error if
    /// the format is not supported.
    pub fn save_view_as<P: AsRef<Path>>(
        &mut self,
        id: ViewId,
        path: P,
    ) -> io::Result<()> {

        let ext = path.as_ref().extension().ok_or(io::Error::new(
            io::ErrorKind::Other,
            "file path requires an extension (.gif or .png)",
        ))?;
        let ext = ext.to_str().ok_or(io::Error::new(
            io::ErrorKind::Other,
            "file extension is not valid unicode",
        ))?;

        if !Self::SUPPORTED_FORMATS.contains(&ext) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{}` is not a supported output format", ext),
            ));
        }

        if ext == "gif" {
            return self.save_view_gif(id, path);
        }

        // Make sure we don't overwrite other files!
        if self
            .view(id)
            .file_name()
            .map_or(true, |f| path.as_ref() != f)
            && path.as_ref().exists()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("\"{}\" already exists", path.as_ref().display()),
            ));
        }

        let (s_id, npixels) = self.resources.save_view(&id, &path)?;
        self.view_mut(id).save_as(s_id, path.as_ref().into());

        self.message(
            format!(
                "\"{}\" {} pixels written",
                path.as_ref().display(),
                npixels,
            ),
            MessageType::Info,
        );
        Ok(())
    }

    /// Private ///////////////////////////////////////////////////////////////////

    /// Load a view into the session.
    fn load_view<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        debug!("load: {:?}", path);

        if let Some(ext) = path.extension() {
            if ext != "png" {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "trying to load file with unsupported extension",
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "trying to load file with no extension",
            ));
        }

        // View is already loaded.
        if let Some(View { id, .. }) = self
            .views
            .values()
            .find(|v| v.file_name().map_or(false, |f| f == path))
        {
            // TODO: Reload from disk.
            let id = *id;
            self.views.activate(id);
            return Ok(());
        }

        if let Some(v) = self.views.active() {
            let id = v.id;

            if v.file_status == FileStatus::NoFile {
                self.destroy_view(id);
            }
        }

        let (width, height, pixels) = ResourceManager::load_image(&path)?;
        let id = self.views.add(
            FileStatus::Saved(path.into()),
            width as u32,
            height as u32,
        );

        self.resources.add_view(id, width, height, &pixels);
        self.organize_views();
        self.edit_view(id);
        self.message(
            format!("\"{}\" {} pixels read", path.display(), width * height),
            MessageType::Info,
        );
        self.dirty = true;

        Ok(())
    }

    /// Destroys the resources associated with a view.
    fn destroy_view(&mut self, id: ViewId) {
        assert!(!self.views.is_empty());

        self.views.remove(&id);
        self.resources.remove_view(&id);

        self.dirty = true;
    }

    /// Quit the view.
    fn quit_view(&mut self, id: ViewId) {
        self.destroy_view(id);

        if !self.views.is_empty() {
            self.organize_views();
            self.center_active_view();
        }
    }

    /// Save a view as a gif animation.
    fn save_view_gif<P: AsRef<Path>>(
        &mut self,
        id: ViewId,
        path: P,
    ) -> io::Result<()> {
        let delay = self.view(id).animation.delay;
        let npixels = self.resources.save_view_gif(&id, &path, delay)?;

        self.message(
            format!(
                "\"{}\" {} pixels written",
                path.as_ref().display(),
                npixels,
            ),
            MessageType::Info,
        );
        Ok(())
    }

    /// Start editing the given view.
    fn edit_view(&mut self, id: ViewId) {
        self.views.activate(id);
        self.center_active_view();
    }

    /// Re-position all views relative to each other so that they don't overlap.
    fn organize_views(&mut self) {
        if self.views.is_empty() {
            return;
        }
        let (_, first) = self
            .views
            .iter_mut()
            .next()
            .expect("view list should never be empty");

        first.offset.y = 0.;

        let mut offset = first.height() as f32 * first.zoom + Self::VIEW_MARGIN;

        for (_, v) in self.views.iter_mut().skip(1) {
            v.offset.y = offset;
            offset += v.height() as f32 * v.zoom + Self::VIEW_MARGIN;
        }
        self.cursor_dirty();
    }

    fn undo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, true);
    }

    fn redo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, false);
    }

    fn restore_view_snapshot(&mut self, id: ViewId, backwards: bool) {
        let snapshot = self
            .resources
            .lock_mut()
            .get_snapshots_mut(&id)
            .and_then(|s| if backwards { s.prev() } else { s.next() })
            .map(|s| (s.id, s.fw, s.fh, s.nframes));

        if let Some((sid, fw, fh, nframes)) = snapshot {
            let v = self.view_mut(id);

            v.reset(fw, fh, nframes);
            v.damaged();

            // If the snapshot was saved to disk, we mark the view as saved too.
            // Otherwise, if the view was saved before restoring the snapshot,
            // we mark it as modified.
            if let FileStatus::Modified(ref f) = v.file_status {
                if v.is_snapshot_saved(sid) {
                    v.file_status = FileStatus::Saved(f.clone());
                }
            } else if let FileStatus::Saved(ref f) = v.file_status {
                v.file_status = FileStatus::Modified(f.clone());
            } else {
                // TODO
            }
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    // Event handlers
    ///////////////////////////////////////////////////////////////////////////

    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        self.width = size.width as f32;
        self.height = size.height as f32;

        // TODO: Reset session cursor coordinates
        self.center_palette();
        self.center_active_view();
    }

    pub fn handle_mouse_input(
        &mut self,
        button: platform::MouseButton,
        state: platform::InputState,
    ) {
        if button != platform::MouseButton::Left {
            return;
        }
        if state == platform::InputState::Pressed {
            self.mouse_down = true;
            self.record_macro(format!("cursor/down"));

            // Click on palette.
            if let Some(color) = self.palette.hover {
                if self.mode == Mode::Command {
                    self.cmdline.puts(&Rgb8::from(color).to_string());
                } else {
                    self.pick_color(color);
                }
                return;
            }

            // Click on a view.
            if let Some(id) = self.hover_view {
                // Clicking on a view is one way to get out of command mode.
                if self.mode == Mode::Command {
                    self.cmdline_hide();
                    return;
                }
                if self.is_active(&id) {
                    let v = self.active_view();
                    let p = self.view_coords(v.id, self.cursor);
                    let extent = v.extent();

                    match self.mode {
                        Mode::Normal => match self.tool {
                            Tool::Brush(ref mut brush) => {
                                let color = if brush.is_set(BrushMode::Erase) {
                                    Rgba8::TRANSPARENT
                                } else {
                                    self.fg
                                };
                                brush.start_drawing(p.into(), color, extent);
                            }
                            Tool::Sampler => {
                                self.sample_color();
                            }
                            Tool::Pan => {}
                        },
                        Mode::Command => {
                            // TODO
                        }
                        Mode::Visual => {
                            // TODO
                        }
                        Mode::Present | Mode::Help => {}
                    }
                } else {
                    self.views.activate(id);
                }
            }
        } else if state == platform::InputState::Released {
            self.mouse_down = false;
            self.record_macro(format!("cursor/up"));

            if let Tool::Brush(ref mut brush) = self.tool {
                match brush.state {
                    BrushState::Drawing { .. }
                    | BrushState::DrawStarted { .. } => {
                        brush.stop_drawing();
                        self.active_view_mut().touch();
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_cursor_moved(&mut self, cursor: SessionCoords) {
        self.record_macro(format!("cursor/move {} {}", cursor.x, cursor.y));
        self.set_cursor(cursor);
    }

    pub fn handle_received_character(&mut self, c: char) {
        if self.mode == Mode::Command {
            if c.is_control() {
                return;
            }
            if self.ignore_received_characters {
                self.ignore_received_characters = false;
                return;
            }
            self.cmdline_handle_input(c);
        } else if let Some(kb) = self.key_bindings.find(
            Key::Char(c),
            platform::ModifiersState::default(),
            platform::InputState::Pressed,
            &self.mode,
        ) {
            self.command(kb.command);
        }
    }

    pub fn handle_keyboard_input(&mut self, input: platform::KeyboardInput) {
        let KeyboardInput {
            state,
            modifiers,
            key,
            ..
        } = input;

        debug!("{:?}", input);

        let mut repeat = false;

        if let Some(key) = key {
            // While the mouse is down, don't accept keyboard input.
            if self.mouse_down {
                return;
            }

            if state == InputState::Pressed {
                repeat = !self.keys_pressed.insert(key);
            } else if state == InputState::Released {
                if !self.keys_pressed.remove(&key) {
                    return;
                }
            }

            match self.mode {
                Mode::Visual => {
                    if key == platform::Key::Escape
                        && state == InputState::Pressed
                    {
                        self.selection = Rect::empty();
                        self.switch_mode(Mode::Normal);
                        return;
                    }
                }
                Mode::Command => {
                    if state == InputState::Pressed {
                        match key {
                            platform::Key::Backspace => {
                                self.cmdline_handle_backspace();
                            }
                            platform::Key::Return => {
                                self.cmdline_handle_enter();
                            }
                            platform::Key::Escape => {
                                self.cmdline_hide();
                            }
                            _ => {}
                        }
                    }
                    return;
                }
                Mode::Help => {
                    if state == InputState::Pressed {
                        if key == platform::Key::Escape {
                            self.switch_mode(Mode::Normal);
                            return;
                        }
                    }
                }
                _ => {}
            }

            if let Some(kb) = self.key_bindings.find(
                Key::Virtual(key),
                modifiers,
                state,
                &self.mode,
            ) {
                // For toggle-like key bindings, we don't want to run the command
                // on key repeats. For regular key bindings, we run the command
                // either way.
                if !kb.is_toggle || kb.is_toggle && !repeat {
                    self.command(kb.command);
                }
            }
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Sourcing
    ///////////////////////////////////////////////////////////////////////////

    /// Source an rx script at the given path. Returns an error if the path
    /// does not exist or the script couldn't be sourced.
    fn source_path<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path_osstring = &self.expand_path(path);
        let path = Path::new(path_osstring);
        debug!("source: {}", path.display());

        let f = File::open(&path)
            .or_else(|_| File::open(self.base_dirs.config_dir().join(path)))?;

        self.source_reader(io::BufReader::new(f), path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    /// Source a directory which contains a `.rxrc` script. Returns an
    /// error if the script wasn't found or couldn't be sourced.
    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

    /// Source a script from an [`io::BufRead`].
    fn source_reader<P: AsRef<Path>, R: io::BufRead>(
        &mut self,
        r: R,
        _path: P,
    ) -> io::Result<()> {
        for line in r.lines() {
            let line = line?;

            if line.starts_with(cmd::COMMENT) {
                continue;
            }
            match Command::from_str(&format!(":{}", line)) {
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
                Ok(cmd) => self.command(cmd),
            }
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Centering
    ///////////////////////////////////////////////////////////////////////////

    /// Center the palette in the workspace.
    fn center_palette(&mut self) {
        let n = usize::min(self.palette.size(), 16) as f32;
        let p = &mut self.palette;

        p.x = 0.;
        p.y = self.height / 2. - n * p.cellsize / 2.;
    }

    /// Vertically the active view in the workspace.
    fn center_active_view_v(&mut self) {
        let v = self.active_view();
        self.offset.y =
            (self.height / 2. - v.height() as f32 / 2. * v.zoom - v.offset.y)
                .floor();
        self.cursor_dirty();
    }

    /// Horizontally center the active view in the workspace.
    fn center_active_view_h(&mut self) {
        let v = self.active_view();
        self.offset.x =
            (self.width / 2. - v.width() as f32 * v.zoom / 2. - v.offset.x)
                .floor();
        self.cursor_dirty();
    }

    /// Center the active view in the workspace.
    fn center_active_view(&mut self) {
        self.center_active_view_v();
        self.center_active_view_h();
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Zoom functions
    ///////////////////////////////////////////////////////////////////////////

    /// Zoom the active view in.
    fn zoom_in(&mut self) {
        let view = self.active_view_mut();
        let lvls = Self::ZOOM_LEVELS;

        for (i, zoom) in lvls.iter().enumerate() {
            if view.zoom <= *zoom {
                if let Some(z) = lvls.get(i + 1) {
                    self.zoom(*z);
                } else {
                    self.message(
                        "Maximum zoom level reached",
                        MessageType::Hint,
                    );
                }
                return;
            }
        }
    }

    /// Zoom the active view out.
    fn zoom_out(&mut self) {
        let view = self.active_view_mut();
        let lvls = Self::ZOOM_LEVELS;

        for (i, zoom) in lvls.iter().enumerate() {
            if view.zoom <= *zoom {
                if i == 0 {
                    self.message(
                        "Minimum zoom level reached",
                        MessageType::Hint,
                    );
                } else if let Some(z) = lvls.get(i - 1) {
                    self.zoom(*z);
                } else {
                    unreachable!();
                }
                return;
            }
        }
    }

    /// Set the active view zoom.
    fn zoom(&mut self, z: f32) {
        let px = self.cursor.x - self.offset.x;
        let py = self.cursor.y - self.offset.y;

        let cursor = self.cursor;

        let within = self.active_view().contains(cursor - self.offset);
        let zprev = self.active_view().zoom;

        debug!("zoom: {} -> {}", zprev, z);

        self.offset = if within {
            let zdiff = z / zprev;

            let nx = (px * zdiff).floor();
            let ny = (py * zdiff).floor();

            let mut offset =
                Vector2::new(self.cursor.x - nx, self.cursor.y - ny);

            let v = self.active_view_mut();

            let vx = v.offset.x;
            let vy = v.offset.y;

            v.zoom = z;

            let dx = v.offset.x - (vx * zdiff);
            let dy = v.offset.y - (vy * zdiff);

            offset.x -= dx;
            offset.y -= dy;

            offset.map(f32::floor)
        } else {
            let v = self.active_view_mut();
            v.zoom = z;

            self.center_active_view();
            self.offset
        };
        self.organize_views();
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Commands
    ///////////////////////////////////////////////////////////////////////////

    /// Process a command.
    fn command(&mut self, cmd: Command) {
        // Certain commands cause problems when run many times within
        // a short time frame. This might be because the GPU hasn't had
        // time to fully process the commands sent to it by the time we
        // execute the next command. We throttle them here to have a
        // minimum time between them.
        if self.throttle(&cmd) {
            return;
        }
        if cmd != Command::Noop {
            self.message_clear();
        }

        debug!("command: {:?}", cmd);

        return match cmd {
            Command::Mode(m) => {
                self.switch_mode(m);
            }
            Command::Quit => match &self.active_view().file_status {
                FileStatus::Modified(_) | FileStatus::New(_) => {
                    self.message(
                        "Error: no write since last change (enter `:q!` to quit without saving)",
                        MessageType::Error,
                    );
                }
                _ => {
                    self.command(Command::ForceQuit);
                }
            },
            Command::Help => {
                if self.mode == Mode::Help {
                    self.switch_mode(Mode::Normal);
                } else {
                    self.switch_mode(Mode::Help);
                }
            }
            Command::SwapColors => {
                std::mem::swap(&mut self.fg, &mut self.bg);
            }
            Command::Sampler(true) => {
                self.prev_tool = Some(self.tool.clone());
                self.tool = Tool::Sampler;
            }
            Command::Sampler(false) => {
                self.tool = self.prev_tool.clone().unwrap_or(Tool::default());
            }
            Command::BrushSet(mode) => {
                if let Tool::Brush(ref mut b) = self.tool {
                    b.set(mode);
                }
            }
            Command::BrushUnset(mode) => {
                if let Tool::Brush(ref mut b) = self.tool {
                    b.unset(mode);
                }
            }
            Command::Brush => {
                self.unimplemented();
            }
            Command::BrushSize(op) => {
                if let Tool::Brush(ref mut b) = self.tool {
                    match op {
                        Op::Incr => {
                            b.size += 1;
                            b.size += b.size % 2;
                        }
                        Op::Decr => {
                            b.size -= 1;
                            b.size -= b.size % 2;
                        }
                        Op::Set(s) => {
                            b.size = s as usize;
                        }
                    }
                    if b.size < Self::MIN_BRUSH_SIZE {
                        b.size = Self::MIN_BRUSH_SIZE;
                    }
                }
            }
            Command::ResizeFrame(fw, fh) => {
                if fw == 0 || fh == 0 {
                    self.message(
                        "Error: cannot set frame dimension to `0`",
                        MessageType::Error,
                    );
                    return;
                }

                let v = self.active_view_mut();
                v.resize_frames(fw, fh);
                v.touch();

                self.organize_views();
            }
            Command::ForceQuit => self.quit_view(self.views.active_id),
            Command::Echo(ref v) => {
                let result = match v {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Ident(s) => match s.as_str() {
                        "config/dir" => Ok(Value::Str(format!(
                            "{}",
                            self.base_dirs.config_dir().display()
                        ))),
                        "s/hidpi" => {
                            Ok(Value::Str(format!("{:.1}", self.hidpi_factor)))
                        }
                        "s/offset" => {
                            Ok(Value::Float2(self.offset.x, self.offset.y))
                        }
                        "v/offset" => {
                            let v = self.active_view();
                            Ok(Value::Float2(v.offset.x, v.offset.y))
                        }
                        "v/zoom" => {
                            Ok(Value::Float(self.active_view().zoom as f64))
                        }
                        _ => match self.settings.get(s) {
                            None => Err(format!("Error: {} is undefined", s)),
                            Some(result) => Ok(Value::Str(format!(
                                "{} = {}",
                                v.clone(),
                                result
                            ))),
                        },
                    },
                    _ => Err(format!("Error: argument cannot be echoed")),
                };
                match result {
                    Ok(v) => self.message(v, MessageType::Echo),
                    Err(e) => self.message(e, MessageType::Error),
                }
            }
            Command::PaletteAdd(rgba) => {
                self.palette.add(rgba);
                self.center_palette();
            }
            Command::PaletteClear => {
                self.palette.clear();
            }
            Command::PaletteSample => {
                self.unimplemented();
            }
            Command::Zoom(op) => match op {
                Op::Incr => {
                    self.zoom_in();
                }
                Op::Decr => {
                    self.zoom_out();
                }
                Op::Set(z) => {
                    if z < 1. || z > Self::MAX_ZOOM {
                        self.message(
                            "Error: invalid zoom level",
                            MessageType::Error,
                        );
                    } else {
                        self.zoom(z);
                    }
                }
            },
            Command::Fill(color) => {
                self.active_view_mut().clear(color);
            }
            Command::Pan(x, y) => {
                self.pan(
                    -(x * Self::PAN_PIXELS) as f32,
                    -(y * Self::PAN_PIXELS) as f32,
                );
            }
            Command::ViewNext => {
                let id = self.views.active_id;
                if let Some(id) =
                    self.views.range(id..).nth(1).map(|(id, _)| *id)
                {
                    self.views.activate(id);
                    self.center_active_view_v();
                }
            }
            Command::ViewPrev => {
                let id = self.views.active_id;
                if let Some(id) =
                    self.views.range(..id).next_back().map(|(id, _)| *id)
                {
                    self.views.activate(id);
                    self.center_active_view_v();
                }
            }
            Command::ViewCenter => {
                self.center_active_view();
            }
            Command::AddFrame => {
                self.active_view_mut().extend();
            }
            Command::CloneFrame(n) => {
                let v = self.active_view_mut();
                let l = v.animation.len() as i32;
                if n >= -1 && n < l {
                    v.extend_clone(n);
                } else {
                    self.message(
                        format!(
                            "Error: clone index must be in the range {}..{}",
                            0,
                            l - 1
                        ),
                        MessageType::Error,
                    );
                }
            }
            Command::RemoveFrame => {
                let v = self.active_view_mut();
                v.shrink();
            }
            Command::Slice(None) => {
                let v = self.active_view_mut();
                v.slice(1);
                // FIXME: This is very inefficient. Since the actual frame contents
                // haven't changed, we don't need to create a full snapshot. We just
                // have to record how many frames are in this snapshot.
                v.touch();
            }
            Command::Slice(Some(nframes)) => {
                let v = self.active_view_mut();
                if !v.slice(nframes) {
                    self.message(
                        format!(
                            "Error: slice: view width is not divisible by {}",
                            nframes
                        ),
                        MessageType::Error,
                    );
                } else {
                    // FIXME: This is very inefficient. Since the actual frame contents
                    // haven't changed, we don't need to create a full snapshot. We just
                    // have to record how many frames are in this snapshot.
                    v.touch();
                }
            }
            Command::Set(ref k, ref v) => {
                if Settings::DEPRECATED.contains(&k.as_str()) {
                    self.message(
                        format!(
                            "Warning: the setting `{}` has been deprecated",
                            k
                        ),
                        MessageType::Warning,
                    );
                    return;
                }
                match self.settings.set(k, v.clone()) {
                    Err(e) => {
                        self.message(
                            format!("Error: {}", e),
                            MessageType::Error,
                        );
                    }
                    Ok(ref old) => {
                        if old != v {
                            self.setting_changed(k, old, v);
                        }
                    }
                }
            }
            #[allow(mutable_borrow_reservation_conflict)]
            Command::Toggle(ref k) => match self.settings.get(k) {
                Some(Value::Bool(b)) => {
                    self.command(Command::Set(k.clone(), Value::Bool(!b)))
                }
                Some(_) => {
                    self.message(
                        format!("Error: can't toggle `{}`", k),
                        MessageType::Error,
                    );
                }
                None => {
                    self.message(
                        format!("Error: no such setting `{}`", k),
                        MessageType::Error,
                    );
                }
            },
            Command::Noop => {
                // Nothing happening!
            }
            Command::Source(ref path) => {
                if let Err(ref e) = self.source_path(path) {
                    self.message(
                        format!("Error sourcing `{}`: {}", path, e),
                        MessageType::Error,
                    );
                }
            }
            Command::Edit(ref paths) => {
                if paths.is_empty() {
                    self.unimplemented();
                } else {
                    if let Err(e) = self.edit(paths) {
                        self.message(
                            format!("Error loading path(s): {}", e),
                            MessageType::Error,
                        );
                    }
                }
            }
            Command::Write(None) => {
                if let Err(e) = self.save_view(self.views.active_id) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::Write(Some(ref path)) => {
                let path_osstring = &self.expand_path(path);
                let path = Path::new(path_osstring);
                if let Err(e) = self.save_view_as(self.views.active_id, path) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::WriteQuit => {
                if self.save_view(self.views.active_id).is_ok() {
                    self.quit_view(self.views.active_id);
                }
            }
            Command::Map(key, cmds) => {
                let (press, release) = *cmds;
                self.key_bindings.add(KeyBinding {
                    key,
                    modes: vec![Mode::Normal],
                    command: press,
                    state: InputState::Pressed,
                    modifiers: platform::ModifiersState::default(),
                    is_toggle: release.is_some(),
                    display: Some(format!("{}", key)),
                });
                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        key,
                        modes: vec![Mode::Normal],
                        command: cmd,
                        state: InputState::Released,
                        modifiers: platform::ModifiersState::default(),
                        is_toggle: true,
                        display: None,
                    });
                }
            }
            Command::Undo => {
                self.undo(self.views.active_id);
            }
            Command::Redo => {
                self.redo(self.views.active_id);
            }
            _ => {
                self.message(
                    format!("Error: command not yet implemented: {:?}", cmd),
                    MessageType::Error,
                );
            }
        };
    }

    fn cmdline_hide(&mut self) {
        // XXX: When visual mode is implemented, we'll have to switch
        // back to whatever the *previous* mode is - either Normal or
        // Visual.
        self.switch_mode(Mode::Normal);
    }

    fn cmdline_handle_backspace(&mut self) {
        self.cmdline.delc();

        if self.cmdline.is_empty() {
            self.cmdline_hide();
        }
    }

    fn cmdline_handle_enter(&mut self) {
        let input = self.cmdline.input();
        self.cmdline_hide();

        if input.is_empty() {
            return;
        }

        match Command::from_str(&input) {
            Err(e) => self.message(format!("Error: {}", e), MessageType::Error),
            Ok(cmd) => self.command(cmd),
        }
    }

    fn cmdline_handle_input(&mut self, c: char) {
        self.cmdline.putc(c);
        self.message_clear();
    }

    fn throttle(&mut self, cmd: &Command) -> bool {
        match cmd {
            // FIXME: Throttling these commands arbitrarily is not ideal.
            // We should be using a fence somewhere to ensure that we are
            // in sync with the GPU. This could be done by using the frame
            // read-back as a synchronization point. For now though, this
            // does the job.
            &Command::AddFrame
            | &Command::RemoveFrame
            | &Command::ResizeFrame { .. } => {
                if let Some(t) = self.throttled {
                    let now = time::Instant::now();
                    if now - t <= Self::THROTTLE_TIME {
                        true
                    } else {
                        self.throttled = Some(now);
                        false
                    }
                } else {
                    self.throttled = Some(time::Instant::now());
                    false
                }
            }
            _ => false,
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Color functions
    ///////////////////////////////////////////////////////////////////////////

    /// Pick the given color as foreground color.
    fn pick_color(&mut self, color: Rgba8) {
        if color.a == 0x0 {
            return;
        }
        if color != self.fg {
            self.bg = self.fg;
            self.fg = color;
        }
        // TODO: Switch to brush.
    }

    /// Get the color at the given view coordinate.
    fn color_at(&self, v: ViewId, p: ViewCoords<u32>) -> Rgba8 {
        let resources = self.resources.lock();
        let snapshot = resources.get_snapshot(&v);

        assert!(snapshot.len() % std::mem::size_of::<Bgra8>() == 0);

        let pixels = snapshot.pixels();
        let (head, pixels, tail) = unsafe { pixels.align_to::<Bgra8>() };

        assert!(head.is_empty());
        assert!(tail.is_empty());

        let index =
            ((snapshot.height() - p.y - 1) * snapshot.width() + p.x) as usize;
        let bgra = pixels[index];

        Rgba8::new(bgra.r, bgra.g, bgra.b, bgra.a)
    }

    fn sample_color(&mut self) {
        if let Some(color) = self.hover_color {
            self.pick_color(color);
        }
    }
}
