///! Session
use crate::brush::*;
use crate::cmd::{self, Command, CommandLine, Key, KeyMapping, Op, Value};
use crate::color;
use crate::data;
use crate::event::{Event, TimedEvent};
use crate::execution::{DigestMode, DigestState, Execution};
use crate::hashmap;
use crate::palette::*;
use crate::platform::{self, InputState, KeyboardInput, LogicalSize, ModifiersState};
use crate::resources::ResourceManager;
use crate::view::{FileStatus, View, ViewCoords, ViewId, ViewManager};

use rgx::core::{Blending, PresentMode, Rect};
use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::{Origin, Rgba8, ZDepth};
use rgx::math::*;

use directories as dirs;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::ops::{Add, Deref, Sub};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::time;

/// Help string.
pub const HELP: &str = r#"
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

debug             on/off             Debug mode
checker           on/off             Alpha checker toggle
vsync             on/off             Vertical sync toggle
input/delay       0.0..32.0          Delay between render frames (ms)
scale             1.0..4.0           UI scale
animation         on/off             View animation toggle
animation/delay   1..1000            View animation delay (ms)
background        #000000..#ffffff   Set background appearance to <color>, eg. #ff0011
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

#[derive(Copy, Clone, Debug)]
enum InternalCommand {
    StopRecording,
}

/// Session coordinates.
/// Encompasses anything within the window, such as the cursor position.
#[derive(Copy, Clone, PartialEq, Debug)]
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
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Mode {
    /// Allows the user to paint pixels.
    Normal,
    /// Allows pixels to be selected, copied and manipulated visually.
    Visual(VisualState),
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
            Self::Visual(VisualState::Selecting { dragging: true }) => "visual (dragging)".fmt(f),
            Self::Visual(VisualState::Selecting { .. }) => "visual".fmt(f),
            Self::Visual(VisualState::Pasting) => "visual (pasting)".fmt(f),
            Self::Command => "command".fmt(f),
            Self::Present => "present".fmt(f),
            Self::Help => "help".fmt(f),
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum VisualState {
    Selecting { dragging: bool },
    Pasting,
}

impl VisualState {
    pub fn selecting() -> Self {
        Self::Selecting { dragging: false }
    }
}

impl Default for VisualState {
    fn default() -> Self {
        Self::selecting()
    }
}

/// A pixel selection within a view.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct Selection(Rect<i32>);

impl Selection {
    /// Create a new selection from a rectangle.
    pub fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self(Rect::new(x1, y1, x2 - 1, y2 - 1))
    }

    /// Create a new selection from a rectangle.
    pub fn from(r: Rect<i32>) -> Self {
        Self::new(r.x1, r.y1, r.x2, r.y2)
    }

    /// Return the selection bounds as a non-empty rectangle. This function
    /// will never return an empty rectangle.
    pub fn bounds(&self) -> Rect<i32> {
        Rect::new(self.x1, self.y1, self.x2 + 1, self.y2 + 1)
    }

    /// Return the absolute selection.
    pub fn abs(&self) -> Selection {
        Self(self.0.abs())
    }

    /// Translate the selection rectangle.
    pub fn translate(&mut self, x: i32, y: i32) {
        self.0 += Vector2::new(x, y)
    }

    /// Resize the selection by a certain amount.
    pub fn resize(&mut self, x: i32, y: i32) {
        self.0.x2 += x;
        self.0.y2 += y;
    }
}

impl Deref for Selection {
    type Target = Rect<i32>;

    fn deref(&self) -> &Rect<i32> {
        &self.0
    }
}

/// Session effects. Eg. view creation/destruction.
/// Anything the renderer might want to know.
#[derive(Clone, Debug)]
pub enum Effect {
    /// When the session has been resized.
    SessionResized(LogicalSize),
    /// When a view has been activated.
    ViewActivated(ViewId),
    /// When a view has been added.
    ViewAdded(ViewId),
    /// When a view has been removed.
    ViewRemoved(ViewId),
    /// When a view has been touched (edited).
    ViewTouched(ViewId),
    /// When a view requires re-drawing.
    ViewDamaged(ViewId),
    /// When the active view is non-permanently painted on.
    ViewPaintDraft(Vec<Shape>),
    /// When the active view is painted on.
    ViewPaintFinal(Vec<Shape>),
    /// The blend mode used for painting has changed.
    ViewBlendingChanged(Blending),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ExitReason {
    Normal,
    Error(String),
}

impl Default for ExitReason {
    fn default() -> Self {
        Self::Normal
    }
}

/// Session state.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    /// The session is initializing.
    Initializing,
    /// The session is running normally.
    Running,
    /// The session is paused. Inputs are not processed.
    Paused,
    /// The session is being shut down.
    Closing(ExitReason),
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
    Pan(PanState),
    /// Used to move things,
    Move,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Brush(Brush::default())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PanState {
    Panning,
    NotPanning,
}

impl Default for PanState {
    fn default() -> Self {
        Self::NotPanning
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A generic direction that can be used for things that go backward
/// and forward.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Direction {
    Backward,
    Forward,
}

impl From<Direction> for i32 {
    fn from(dir: Direction) -> i32 {
        match dir {
            Direction::Backward => -1,
            Direction::Forward => 1,
        }
    }
}

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

    pub fn is_replay(&self) -> bool {
        self.message_type == MessageType::Replay
    }

    /// Log a message to stdout/stderr.
    fn log(&self) {
        match self.message_type {
            MessageType::Info => info!("{}", self),
            MessageType::Hint => {}
            MessageType::Echo => info!("{}", self),
            MessageType::Error => error!("{}", self),
            MessageType::Warning => warn!("{}", self),
            MessageType::Replay => {}
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
    Warning,
    /// Replay-related message.
    Replay,
    /// Success message.
    Okay,
}

impl MessageType {
    /// Returns the color associated with a `MessageType`.
    fn color(self) -> Rgba8 {
        match self {
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
#[derive(Clone, Debug)]
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
                    modes: vec![Mode::Normal, Mode::Help],
                    modifiers: ModifiersState {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        meta: false,
                    },
                    key: Key::Virtual(platform::Key::Slash),
                    state: InputState::Pressed,
                    command: Command::Mode(Mode::Help),
                    is_toggle: false,
                    display: Some("?".to_string()),
                },
                KeyBinding {
                    modes: vec![
                        Mode::Normal,
                        Mode::Visual(VisualState::Pasting),
                        Mode::Visual(VisualState::selecting()),
                    ],
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
                #[cfg(winit)]
                KeyBinding {
                    modes: vec![Mode::Normal],
                    modifiers: ModifiersState {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        meta: false,
                    },
                    key: Key::Virtual(platform::Key::Colon),
                    state: InputState::Pressed,
                    command: Command::Mode(Mode::Command),
                    is_toggle: false,
                    display: None,
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
        mode: Mode,
    ) -> Option<KeyBinding> {
        self.elems.iter().rev().cloned().find(|kb| {
            kb.key == key
                && kb.modifiers == modifiers
                && kb.state == state
                && kb.modes.contains(&mode)
        })
    }

    /// Iterate over all key bindings.
    pub fn iter(&self) -> std::slice::Iter<'_, KeyBinding> {
        self.elems.iter()
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A dictionary used to store session settings.
#[derive(Debug)]
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
                "background" => Value::Rgba8(color::BLACK),
                "vsync" => Value::Bool(false),
                "input/delay" => Value::Float(8.0),
                "input/mouse" => Value::Bool(true),
                "scale" => Value::Float(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::U32(160),
                "ui/palette" => Value::Bool(true),
                "ui/status" => Value::Bool(true),
                "ui/cursor" => Value::Bool(true),
                "ui/message" => Value::Bool(true),
                "ui/switcher" => Value::Bool(true),
                "ui/view-info" => Value::Bool(true),

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
    /// The previous `Mode`.
    pub prev_mode: Option<Mode>,
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

    /// The current frame number.
    frame_number: u64,

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
    pub selection: Option<Selection>,

    /// The session's current settings.
    pub settings: Settings,
    /// Settings recently changed.
    pub settings_changed: HashSet<String>,

    /// Views loaded in the session.
    pub views: ViewManager,
    /// Effects produced by the session. Cleared at the beginning of every
    /// update.
    pub effects: Vec<Effect>,

    /// The current state of the command line.
    pub cmdline: CommandLine,
    /// The color palette.
    pub palette: Palette,

    /// Average time it takes for a session update.
    pub avg_time: time::Duration,

    /// The current tool. Only used in `Normal` mode.
    pub tool: Tool,
    /// The previous tool, if any.
    pub prev_tool: Option<Tool>,

    /// Input state of the mouse.
    mouse_state: InputState,

    /// Internal command bus. Used to send internal messages asynchronously.
    /// We do this when we want the renderer to have a chance to run before
    /// the command is processed. For example, when displaying a message before
    /// an expensive process is kicked off.
    queue: Vec<InternalCommand>,

    #[allow(dead_code)]
    _onion: bool,
    #[allow(dead_code)]
    _grid_w: u32,
    #[allow(dead_code)]
    _grid_h: u32,
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
    /// Maximum frame width or height.
    const MAX_FRAME_SIZE: u32 = 4096;
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
            tool: Tool::default(),
            prev_tool: Option::default(),
            mouse_state: InputState::Released,
            hover_color: Option::default(),
            hover_view: Option::default(),
            fg: color::WHITE,
            bg: color::BLACK,
            settings: Settings::default(),
            settings_changed: HashSet::new(),
            views: ViewManager::new(),
            effects: Vec::new(),
            palette: Palette::new(Self::PALETTE_CELL_SIZE),
            key_bindings: KeyBindings::default(),
            keys_pressed: HashSet::new(),
            ignore_received_characters: false,
            cmdline: CommandLine::new(),
            mode: Mode::Normal,
            prev_mode: Option::default(),
            selection: Option::default(),
            message: Message::default(),
            resources,
            avg_time: time::Duration::from_secs(0),
            frame_number: 0,
            queue: Vec::new(),

            // Unused
            _onion: false,
            _grid_w: 0,
            _grid_h: 0,
        }
    }

    /// Initialize a session.
    pub fn init(mut self, source: Option<PathBuf>) -> std::io::Result<Self> {
        self.transition(State::Running);

        let cwd = std::env::current_dir()?;

        if let Some(init) = source {
            // The special source '-' is used to skip initialization.
            if init.as_os_str() != "-" {
                self.source_path(&init).map_err(|e| {
                    io::Error::new(e.kind(), format!("error sourcing {:?}: {}", init, e))
                })?
            }
        } else {
            let dir = self.base_dirs.config_dir().to_owned();
            let cfg = dir.join(Self::INIT);

            if cfg.exists() {
                self.source_path(cfg)?;
            } else {
                let cfg_data = self.reset()?;

                if let Err(e) = fs::create_dir_all(dir).and_then(|_| fs::write(&cfg, cfg_data)) {
                    warn!(
                        "Warning: couldn't create configuration file {:?}: {}",
                        cfg, e
                    );
                }
            }
        }
        self.source_dir(cwd).ok();
        self.message(format!("rx v{}", crate::VERSION), MessageType::Echo);

        Ok(self)
    }

    pub fn reset(&mut self) -> io::Result<&'static [u8]> {
        self.key_bindings = KeyBindings::default();
        self.settings = Settings::default();
        self.tool = Tool::default();

        self.source_reader(io::BufReader::new(data::CONFIG), "<init>")?;
        self.message("Settings reset to default values", MessageType::Okay);

        Ok(data::CONFIG)
    }

    /// Create a blank view.
    pub fn blank(&mut self, fs: FileStatus, w: u32, h: u32) {
        let id = self.views.add(fs, w, h);

        self.effects.push(Effect::ViewAdded(id));
        self.resources.add_blank_view(id, w, h);
        self.organize_views();
        self.edit_view(id);
    }

    /// Transition to a new state. Only allows valid state transitions.
    pub fn transition(&mut self, to: State) {
        match (&self.state, &to) {
            (State::Initializing, State::Running)
            | (State::Running, State::Paused)
            | (State::Paused, State::Running)
            | (State::Paused, State::Closing(_))
            | (State::Running, State::Closing(_)) => {
                debug!("state: {:?} -> {:?}", self.state, to);
                self.state = to;
            }
            _ => {}
        }
    }

    /// Update the session by processing new user events and advancing
    /// the internal state.
    pub fn update(
        &mut self,
        events: &mut Vec<Event>,
        exec: Rc<RefCell<Execution>>,
        delta: time::Duration,
        avg_time: time::Duration,
    ) -> Vec<Effect> {
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

        let exec = &mut *exec.borrow_mut();

        // TODO: This whole block needs refactoring..
        if let Execution::Replaying {
            events: recording,
            digest: DigestState { mode, .. },
            result,
            ..
        } = exec
        {
            let mode = *mode;
            let result = result.clone();

            {
                let frame = self.frame_number;
                let end = recording.iter().position(|t| t.frame != frame);

                recording
                    .drain(..end.unwrap_or_else(|| recording.len()))
                    .collect::<Vec<TimedEvent>>()
                    .into_iter()
                    .for_each(|t| self.handle_event(t.event, exec));

                let verify_ended = mode == DigestMode::Verify && result.is_done() && end.is_none();
                let replay_ended = mode != DigestMode::Verify && end.is_none();
                let verify_failed = result.is_err();

                // Replay is over.
                if verify_ended || replay_ended || verify_failed {
                    self.release_inputs();
                    self.message("Replay ended", MessageType::Replay);

                    match mode {
                        DigestMode::Verify => {
                            if result.is_ok() {
                                info!("replaying: {}", result.summary());
                                self.quit(ExitReason::Normal);
                            } else {
                                self.quit(ExitReason::Error(format!(
                                    "replay failed: {}",
                                    result.summary()
                                )));
                            }
                        }
                        DigestMode::Record => match exec.finalize_replaying() {
                            Ok(path) => {
                                info!("replaying: digest saved to `{}`", path.display());
                            }
                            Err(e) => {
                                error!("replaying: error saving recording: {}", e);
                            }
                        },
                        DigestMode::Ignore => {}
                    }
                    *exec = Execution::Normal;
                }
            }

            for event in events.drain(..) {
                match event {
                    Event::KeyboardInput(platform::KeyboardInput {
                        key: Some(platform::Key::Escape),
                        ..
                    }) => {
                        self.release_inputs();
                        self.message("Replay ended", MessageType::Replay);

                        *exec = Execution::Normal;
                    }
                    _ => debug!("event (ignored): {:?}", event),
                }
            }
        } else {
            // A common case is that we have multiple `CursorMoved` events
            // in one update. In that case we keep only the last one,
            // since the in-betweens will never be seen.
            if events.len() > 1
                && events.iter().all(|e| match e {
                    Event::CursorMoved(_) => true,
                    _ => false,
                })
            {
                events.drain(..events.len() - 1);
            }

            let cmds: Vec<_> = self.queue.drain(..).collect();
            for cmd in cmds.into_iter() {
                self.handle_internal_cmd(cmd, exec);
            }

            for event in events.drain(..) {
                self.handle_event(event, exec);
            }
        }

        if let Tool::Brush(ref brush) = self.tool {
            let output = brush.output(
                Stroke::NONE,
                Fill::Solid(brush.color.into()),
                1.0,
                Origin::BottomLeft,
            );
            if !output.is_empty() {
                match brush.state {
                    // If we're erasing, we can't use the staging framebuffer, since we
                    // need to be replacing pixels on the real buffer.
                    _ if brush.is_set(BrushMode::Erase) => {
                        self.effects.extend_from_slice(&[
                            Effect::ViewBlendingChanged(Blending::constant()),
                            Effect::ViewPaintFinal(output),
                        ]);
                    }
                    // As long as we haven't finished drawing, render into the staging buffer.
                    BrushState::DrawStarted(_) | BrushState::Drawing(_) => {
                        self.effects.push(Effect::ViewPaintDraft(output));
                    }
                    // Once we're done drawing, we can render into the real buffer.
                    BrushState::DrawEnded(_) => {
                        self.effects.extend_from_slice(&[
                            Effect::ViewBlendingChanged(Blending::default()),
                            Effect::ViewPaintFinal(output),
                        ]);
                    }
                    // If the brush output isn't empty, we can't possibly not
                    // be drawing!
                    BrushState::NotDrawing => unreachable!(),
                }
            }
        }

        if self.views.is_empty() {
            self.quit(ExitReason::Normal);
        } else {
            for (id, v) in self.views.iter() {
                if v.is_dirty() {
                    self.effects.push(Effect::ViewTouched(*id));
                } else if v.is_damaged() {
                    self.effects.push(Effect::ViewDamaged(*id));
                }
            }
        }

        match exec {
            Execution::Replaying {
                events: recording,
                digest: DigestState { mode, .. },
                ..
            } if *mode == DigestMode::Verify || *mode == DigestMode::Record => {
                // Skip to the next event frame to speed up replay.
                self.frame_number = recording
                    .front()
                    .map(|e| e.frame)
                    .unwrap_or(self.frame_number + 1);
            }
            _ => {
                self.frame_number += 1;
            }
        }

        // Make sure we don't have rounding errors
        debug_assert_eq!(self.offset, self.offset.map(|a| a.floor()));

        // Return and drain accumulated effects
        self.effects()
    }

    /// Quit the session.
    pub fn quit(&mut self, r: ExitReason) {
        self.transition(State::Closing(r));
    }

    /// Drain and return effects.
    pub fn effects(&mut self) -> Vec<Effect> {
        self.effects.drain(..).collect()
    }

    /// Return the session offset as a transformation matrix.
    pub fn transform(&self) -> Matrix4<f32> {
        Matrix4::from_translation(self.offset.extend(0.))
    }

    /// Snap the given session coordinates to the pixel grid.
    /// This only has an effect at zoom levels greater than `1.0`.
    #[allow(dead_code)]
    pub fn snap(&self, p: SessionCoords, offx: f32, offy: f32, zoom: f32) -> SessionCoords {
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

    /// Re-compute state related to the cursor position. This is useful
    /// when the cursor hasn't moved relative to the session, but things
    /// within the session have moved relative to the cursor.
    fn cursor_dirty(&mut self) {
        let cursor = self.cursor;
        let palette_hover = self.palette.hover.is_some();

        self.palette.handle_cursor_moved(cursor);
        self.hover_view = None;

        match &self.tool {
            Tool::Brush(b) if !b.is_drawing() => {
                if !palette_hover && self.palette.hover.is_some() {
                    // Gained palette focus with brush.
                    self.tool(Tool::Sampler);
                }
            }
            Tool::Sampler if palette_hover && self.palette.hover.is_none() => {
                // Lost palette focus with color sampler.
                self.prev_tool();
            }
            _ => {}
        }

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
            self.color_at(v, p)
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
            "scale" => {
                // TODO: We need to recompute the cursor position here
                // from the window coordinates. Currently, cursor position
                // is stored only in `SessionCoords`, which would have
                // to change.
            }
            _ => {}
        }
    }

    /// Toggle the session mode.
    fn toggle_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            self.switch_mode(Mode::Normal);
        } else {
            self.switch_mode(mode);
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
                self.cmdline.clear();
            }
            _ => {}
        }

        match new {
            Mode::Normal => {
                self.selection = None;
            }
            Mode::Command => {
                // When switching to command mode via the keyboard, we simultaneously
                // also receive the character input equivalent of the key pressed.
                // This input, since we are now in command mode, is processed as
                // text input to the command line. To avoid this, we have to ignore
                // all such input until the end of the current upate.
                self.ignore_received_characters = true;
                self.cmdline_handle_input(':');
            }
            Mode::Visual(_) => {
                if self.selection.is_none() && self.hover_view == Some(self.views.active_id) {
                    let p = self.active_view_coords(self.cursor).map(|n| n as i32);
                    self.selection = Some(Selection::new(p.x, p.y, p.x + 1, p.y + 1));
                }
            }
            _ => {}
        }

        self.release_inputs();
        self.prev_mode = Some(self.mode);
        self.mode = new;
    }

    /// Release all keys and mouse buttons.
    fn release_inputs(&mut self) {
        let pressed: Vec<platform::Key> = self.keys_pressed.iter().cloned().collect();
        for k in pressed {
            self.handle_keyboard_input(
                platform::KeyboardInput {
                    key: Some(k),
                    modifiers: ModifiersState::default(),
                    state: InputState::Released,
                },
                &mut Execution::Normal,
            );
        }
        if self.mouse_state == InputState::Pressed {
            self.handle_mouse_input(platform::MouseButton::Left, InputState::Released);
        }
    }

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
            .get_mut(id)
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

    /// Activate a view. This makes the given view the "active" view.
    pub fn activate(&mut self, id: ViewId) {
        if self.views.active_id == id {
            return;
        }
        self.views.activate(id);
        self.effects.push(Effect::ViewActivated(id));

        match self.mode {
            Mode::Visual(VisualState::Selecting { .. }) => {
                self.selection = None;
            }
            Mode::Visual(VisualState::Pasting) => {
                // When pasting, if the selection fits in the activated
                // view, we allow it to transfer. Otherwise we switch
                // back to selection mode.
                if let Some(s) = self.selection {
                    let r = self.active_view().bounds();
                    if !r.contains(s.min()) || !r.contains(s.max()) {
                        self.selection = None;
                        self.mode = Mode::Visual(VisualState::default());
                    }
                }
            }
            _ => {}
        }
    }

    /// Check whether a view is active.
    pub fn is_active(&self, id: ViewId) -> bool {
        self.views.active_id == id
    }

    /// Convert "logical" window coordinates to session coordinates.
    pub fn window_to_session_coords(&self, position: platform::LogicalPosition) -> SessionCoords {
        let (x, y) = (position.x, position.y);
        let scale: f64 = self.settings["scale"].float64();
        SessionCoords::new(
            (x / scale).floor() as f32,
            self.height - (y / scale).floor() as f32 - 1.,
        )
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
    pub fn session_coords(&self, v: ViewId, p: ViewCoords<f32>) -> SessionCoords {
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

    /// Check whether a point is inside the selection, if any.
    pub fn is_selected(&self, p: ViewCoords<i32>) -> bool {
        if let Some(s) = self.selection {
            s.abs().bounds().contains(*p)
        } else {
            false
        }
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
            let path = path.as_ref();

            if path.is_dir() {
                for entry in path.read_dir()? {
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
            } else if !path.exists() && path.with_extension("png").exists() {
                self.load_view(path.with_extension("png"))?;
            } else {
                let (w, h) = if !self.views.is_empty() {
                    let v = self.active_view();
                    (v.width(), v.height())
                } else {
                    (Self::DEFAULT_VIEW_W, Self::DEFAULT_VIEW_H)
                };
                self.blank(FileStatus::New(path.with_extension("png")), w, h);
            }
        }

        if let Some(id) = self.views.keys().cloned().next_back() {
            self.organize_views();
            self.edit_view(id);
        }

        Ok(())
    }

    /// Save the given view to disk with the current file name. Returns
    /// an error if the view has no file name.
    pub fn save_view(&mut self, id: ViewId) -> io::Result<()> {
        if let Some(ref f) = self.view(id).file_name().cloned() {
            self.save_view_as(id, f)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "no file name given"))
        }
    }

    /// Save a view with the given file name. Returns an error if
    /// the format is not supported.
    pub fn save_view_as<P: AsRef<Path>>(&mut self, id: ViewId, path: P) -> io::Result<()> {
        let ext = path.as_ref().extension().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "file path requires an extension (.gif or .png)",
            )
        })?;
        let ext = ext.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "file extension is not valid unicode")
        })?;

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

        let (s_id, npixels) = self.resources.save_view(id, &path)?;
        self.view_mut(id).save_as(s_id, path.as_ref().into());

        self.message(
            format!("\"{}\" {} pixels written", path.as_ref().display(), npixels,),
            MessageType::Info,
        );
        Ok(())
    }

    /// Private ///////////////////////////////////////////////////////////////////

    /// Load a view into the session.
    fn load_view<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        debug!("load: {:?}", path);

        match path.extension() {
            Some(ext) if ext != "png" => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "trying to load file with unsupported extension",
                ));
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "trying to load file with no extension",
                ));
            }
            _ => {}
        }

        // View is already loaded.
        if let Some(View { id, .. }) = self
            .views
            .values()
            .find(|v| v.file_name().map_or(false, |f| f == path))
        {
            // TODO: Reload from disk.
            let id = *id;
            self.activate(id);
            return Ok(());
        }

        if let Some(v) = self.views.active() {
            let id = v.id;

            if v.file_status == FileStatus::NoFile {
                self.destroy_view(id);
            }
        }

        let (width, height, pixels) = ResourceManager::load_image(&path)?;
        let id = self
            .views
            .add(FileStatus::Saved(path.into()), width as u32, height as u32);

        self.effects.push(Effect::ViewAdded(id));
        self.resources.add_view(id, width, height, &pixels);
        self.message(
            format!("\"{}\" {} pixels read", path.display(), width * height),
            MessageType::Info,
        );

        Ok(())
    }

    /// Destroys the resources associated with a view.
    fn destroy_view(&mut self, id: ViewId) {
        assert!(!self.views.is_empty());

        self.views.remove(id);
        self.resources.remove_view(id);
        self.effects.push(Effect::ViewRemoved(id));
    }

    /// Quit the view.
    fn quit_view(&mut self, id: ViewId) {
        self.destroy_view(id);

        if !self.views.is_empty() {
            self.organize_views();
            self.center_active_view();
        }
    }

    /// Quit view if it has been saved. Otherwise, display an error.
    fn quit_view_safe(&mut self, id: ViewId) {
        let v = self.view(id);
        match &v.file_status {
            FileStatus::Modified(_) | FileStatus::New(_) => {
                self.message(
                    "Error: no write since last change (enter `:q!` to quit without saving)",
                    MessageType::Error,
                );
            }
            _ => self.quit_view(id),
        }
    }

    /// Save a view as a gif animation.
    fn save_view_gif<P: AsRef<Path>>(&mut self, id: ViewId, path: P) -> io::Result<()> {
        let delay = self.view(id).animation.delay;
        let npixels = self.resources.save_view_gif(id, &path, delay)?;

        self.message(
            format!("\"{}\" {} pixels written", path.as_ref().display(), npixels,),
            MessageType::Info,
        );
        Ok(())
    }

    /// Start editing the given view.
    fn edit_view(&mut self, id: ViewId) {
        self.activate(id);
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

    /// Check the current selection and invalidate it if necessary.
    fn check_selection(&mut self) {
        let v = self.active_view();
        let r = v.bounds();
        if let Some(s) = &self.selection {
            if !r.contains(s.min()) && !r.contains(s.max()) {
                self.selection = None;
            }
        }
    }

    /// Yank the selection.
    fn yank(&mut self) -> Option<Rect<i32>> {
        if let (Mode::Visual(VisualState::Selecting { .. }), Some(s)) = (self.mode, self.selection)
        {
            let v = self.active_view_mut();
            let s = s.abs().bounds();

            if s.intersects(v.bounds()) {
                let s = s.intersection(v.bounds());

                v.yank(s);

                self.selection = Some(Selection::from(s));
                self.switch_mode(Mode::Visual(VisualState::Pasting));

                return Some(s);
            }
        }
        None
    }

    fn undo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, Direction::Backward);
    }

    fn redo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, Direction::Forward);
    }

    fn restore_view_snapshot(&mut self, id: ViewId, dir: Direction) {
        let snapshot = self
            .resources
            .lock_mut()
            .get_view_mut(id)
            .and_then(|s| {
                if dir == Direction::Backward {
                    s.prev_snapshot()
                } else {
                    s.next_snapshot()
                }
            })
            .map(|s| (s.id, s.extent));

        if let Some((sid, extent)) = snapshot {
            let v = self.view_mut(id);

            v.reset(extent);
            v.damaged();

            // If the snapshot was saved to disk, we mark the view as saved too.
            // Otherwise, if the view was saved before restoring the snapshot,
            // we mark it as modified.
            match v.file_status {
                FileStatus::Modified(ref f) if v.is_snapshot_saved(sid) => {
                    v.file_status = FileStatus::Saved(f.clone());
                }
                FileStatus::Saved(ref f) => {
                    v.file_status = FileStatus::Modified(f.clone());
                }
                _ => {
                    // TODO
                }
            }
            self.cursor_dirty();
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    // Internal command handler
    ///////////////////////////////////////////////////////////////////////////

    fn handle_internal_cmd(&mut self, cmd: InternalCommand, exec: &mut Execution) {
        match cmd {
            InternalCommand::StopRecording => match exec.stop_recording() {
                Ok(path) => {
                    self.message(
                        format!("Recording saved to `{}`", path.display()),
                        MessageType::Replay,
                    );
                    info!("recording: events saved to `{}`", path.display());
                    self.quit(ExitReason::Normal);
                }
                Err(e) => {
                    error!("recording: error stopping: {}", e);
                }
            },
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    // Event handlers
    ///////////////////////////////////////////////////////////////////////////

    pub fn handle_event(&mut self, event: Event, exec: &mut Execution) {
        if let Execution::Recording {
            ref mut events,
            start,
            ..
        } = exec
        {
            events.push(TimedEvent::new(
                self.frame_number,
                start.elapsed(),
                event.clone(),
            ));
        }

        match event {
            Event::MouseInput(btn, st) => {
                if self.settings["input/mouse"].is_set() {
                    self.handle_mouse_input(btn, st);
                }
            }
            Event::MouseWheel(delta) => {
                if self.settings["input/mouse"].is_set() {
                    self.handle_mouse_wheel(delta);
                }
            }
            Event::CursorMoved(position) => {
                if self.settings["input/mouse"].is_set() {
                    let coords = self.window_to_session_coords(position);
                    self.handle_cursor_moved(coords);
                }
            }
            Event::KeyboardInput(input) => self.handle_keyboard_input(input, exec),
            Event::ReceivedCharacter(c) => self.handle_received_character(c),
        }
    }

    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        self.width = size.width as f32;
        self.height = size.height as f32;

        // TODO: Reset session cursor coordinates
        self.center_palette();
        self.center_active_view();

        self.effects.push(Effect::SessionResized(size));
    }

    fn handle_mouse_input(&mut self, button: platform::MouseButton, state: platform::InputState) {
        if button != platform::MouseButton::Left {
            return;
        }
        self.mouse_state = state;

        // Pan tool.
        match &mut self.tool {
            Tool::Pan(ref mut p) => match (&p, state) {
                (PanState::Panning, InputState::Released) => {
                    *p = PanState::NotPanning;
                    return;
                }
                (PanState::NotPanning, InputState::Pressed) => {
                    *p = PanState::Panning;
                    return;
                }
                _ => {}
            },
            _ => {}
        }

        match state {
            InputState::Pressed => {
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
                    if self.is_active(id) {
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
                                Tool::Pan(_) => {}
                                Tool::Move => {}
                            },
                            Mode::Command => {
                                // TODO
                            }
                            Mode::Visual(VisualState::Selecting { ref mut dragging }) => {
                                let p = p.map(|n| n as i32);
                                let unit = Selection::new(p.x, p.y, p.x + 1, p.y + 1);

                                if let Some(s) = &mut self.selection {
                                    if s.abs().bounds().contains(p) {
                                        *dragging = true;
                                    } else {
                                        self.selection = Some(unit);
                                    }
                                } else {
                                    self.selection = Some(unit);
                                }
                            }
                            Mode::Visual(VisualState::Pasting) => {
                                self.command(Command::SelectionPaste);
                            }
                            Mode::Present | Mode::Help => {}
                        }
                    } else {
                        self.activate(id);
                    }
                } else {
                    // Clicking outside a view...
                    match self.mode {
                        Mode::Visual(VisualState::Selecting { ref mut dragging }) => {
                            self.selection = None;
                            *dragging = false;
                        }
                        _ => {}
                    }
                }
            }
            InputState::Released => match self.mode {
                Mode::Visual(VisualState::Selecting { ref mut dragging }) => {
                    *dragging = false;
                }
                Mode::Normal => {
                    if let Tool::Brush(ref mut brush) = self.tool {
                        match brush.state {
                            BrushState::Drawing { .. } | BrushState::DrawStarted { .. } => {
                                brush.stop_drawing();
                                self.active_view_mut().touch();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            InputState::Repeated => {}
        }
    }

    fn handle_mouse_wheel(&mut self, delta: platform::LogicalDelta) {
        if delta.y > 0. {
            if let Some(v) = self.hover_view {
                self.activate(v);
            }
            self.zoom_in(self.cursor);
        } else if delta.y < 0. {
            self.zoom_out(self.cursor);
        }
    }

    fn handle_cursor_moved(&mut self, cursor: SessionCoords) {
        if self.cursor == cursor {
            return;
        }

        let p = self.active_view_coords(cursor);
        let prev_p = self.active_view_coords(self.cursor);
        let (vw, vh) = self.active_view().size();

        match self.mode {
            Mode::Normal => match self.tool {
                Tool::Brush(ref mut brush) if p != prev_p => match brush.state {
                    BrushState::DrawStarted { .. } | BrushState::Drawing { .. } => {
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
                },
                Tool::Pan(PanState::Panning) => {
                    self.pan(cursor.x - self.cursor.x, cursor.y - self.cursor.y);
                }
                Tool::Sampler if self.mouse_state == InputState::Pressed => {
                    self.sample_color();
                }
                _ => {}
            },
            Mode::Visual(VisualState::Selecting { dragging: false }) => {
                if self.mouse_state == InputState::Pressed {
                    if let Some(ref mut s) = self.selection {
                        *s = Selection::new(s.x1, s.y1, p.x as i32 + 1, p.y as i32 + 1);
                    }
                }
            }
            Mode::Visual(VisualState::Selecting { dragging: true }) => {
                let view = self.active_view().bounds();

                if self.mouse_state == InputState::Pressed && p != prev_p {
                    if let Some(ref mut s) = self.selection {
                        // TODO: (rgx) Better API.
                        let delta = *p - Vector2::new(prev_p.x, prev_p.y);
                        let delta = Vector2::new(delta.x as i32, delta.y as i32);
                        let t = Selection::from(s.bounds() + delta);

                        if view.intersects(t.abs().bounds()) {
                            *s = t;
                        }
                    }
                }
            }
            Mode::Visual(VisualState::Pasting) => {
                // Center paste selection on cursor.
                let c = self.active_view_coords(cursor);
                if let Some(ref mut s) = self.selection {
                    let r = s.abs().bounds();
                    let (w, h) = (r.width(), r.height());
                    let (x, y) = (c.x as i32 - w / 2, c.y as i32 - h / 2);
                    *s = Selection::new(x, y, x + w, y + h);
                }
            }
            _ => {}
        }
        self.cursor = cursor;
        self.cursor_dirty();
    }

    fn handle_received_character(&mut self, c: char) {
        if self.mode == Mode::Command {
            if c.is_control() {
                return;
            }
            if self.ignore_received_characters {
                self.ignore_received_characters = false;
                return;
            }
            self.cmdline_handle_input(c);
        }
    }

    fn handle_keyboard_input(&mut self, input: platform::KeyboardInput, exec: &mut Execution) {
        let KeyboardInput {
            state,
            modifiers,
            key,
            ..
        } = input;

        let mut repeat = state == InputState::Repeated;
        let state = if repeat { InputState::Pressed } else { state };

        if let Some(key) = key {
            // While the mouse is down, don't accept keyboard input.
            if self.mouse_state == InputState::Pressed {
                return;
            }

            if state == InputState::Pressed {
                repeat = repeat || !self.keys_pressed.insert(key);
            } else if state == InputState::Released {
                if !self.keys_pressed.remove(&key) {
                    return;
                }
            }

            match self.mode {
                Mode::Visual(VisualState::Selecting { .. }) => {
                    if key == platform::Key::Escape && state == InputState::Pressed {
                        self.switch_mode(Mode::Normal);
                        return;
                    }
                }
                Mode::Visual(VisualState::Pasting) => {
                    if key == platform::Key::Escape && state == InputState::Pressed {
                        self.switch_mode(Mode::Visual(VisualState::default()));
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
                    if state == InputState::Pressed && key == platform::Key::Escape {
                        self.switch_mode(Mode::Normal);
                        return;
                    }
                }
                _ => {}
            }

            if let Some(kb) = self
                .key_bindings
                .find(Key::Virtual(key), modifiers, state, self.mode)
            {
                // For toggle-like key bindings, we don't want to run the command
                // on key repeats. For regular key bindings, we run the command
                // depending on if it's supposed to repeat.
                if (repeat && kb.command.repeats() && !kb.is_toggle) || !repeat {
                    self.command(kb.command);
                }
                return;
            }

            if let Execution::Recording { events, .. } = exec {
                if key == platform::Key::End {
                    events.pop(); // Discard this key event.
                    self.message("Saving recording...", MessageType::Info);
                    self.queue.push(InternalCommand::StopRecording);
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
        let path = path.as_ref();
        debug!("source: {}", path.display());

        let f =
            File::open(&path).or_else(|_| File::open(self.base_dirs.config_dir().join(path)))?;

        self.source_reader(io::BufReader::new(f), path)
    }

    /// Source a directory which contains a `.rxrc` script. Returns an
    /// error if the script wasn't found or couldn't be sourced.
    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

    /// Source a script from an [`io::BufRead`].
    fn source_reader<P: AsRef<Path>, R: io::BufRead>(&mut self, r: R, _path: P) -> io::Result<()> {
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
        self.offset.y = (self.height / 2. - v.height() as f32 / 2. * v.zoom - v.offset.y).floor();
        self.cursor_dirty();
    }

    /// Horizontally center the active view in the workspace.
    fn center_active_view_h(&mut self) {
        let v = self.active_view();
        self.offset.x = (self.width / 2. - v.width() as f32 * v.zoom / 2. - v.offset.x).floor();
        self.cursor_dirty();
    }

    /// Center the active view in the workspace.
    fn center_active_view(&mut self) {
        self.center_active_view_v();
        self.center_active_view_h();
    }

    /// The session center.
    #[allow(dead_code)]
    fn center(&self) -> SessionCoords {
        SessionCoords::new(self.width / 2., self.height / 2.)
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Zoom functions
    ///////////////////////////////////////////////////////////////////////////

    /// Zoom the active view in.
    fn zoom_in(&mut self, center: SessionCoords) {
        let view = self.active_view_mut();
        let lvls = Self::ZOOM_LEVELS;

        for (i, zoom) in lvls.iter().enumerate() {
            if view.zoom <= *zoom {
                if let Some(z) = lvls.get(i + 1) {
                    self.zoom(*z, center);
                } else {
                    self.message("Maximum zoom level reached", MessageType::Hint);
                }
                return;
            }
        }
    }

    /// Zoom the active view out.
    fn zoom_out(&mut self, center: SessionCoords) {
        let view = self.active_view_mut();
        let lvls = Self::ZOOM_LEVELS;

        for (i, zoom) in lvls.iter().enumerate() {
            if view.zoom <= *zoom {
                if i == 0 {
                    self.message("Minimum zoom level reached", MessageType::Hint);
                } else if let Some(z) = lvls.get(i - 1) {
                    self.zoom(*z, center);
                } else {
                    unreachable!();
                }
                return;
            }
        }
    }

    /// Set the active view zoom. Takes a center to zoom to.
    fn zoom(&mut self, z: f32, center: SessionCoords) {
        let px = center.x - self.offset.x;
        let py = center.y - self.offset.y;

        let zprev = self.active_view().zoom;
        let zdiff = z / zprev;

        let nx = (px * zdiff).floor();
        let ny = (py * zdiff).floor();

        let mut offset = Vector2::new(center.x - nx, center.y - ny);

        let v = self.active_view_mut();

        let vx = v.offset.x;
        let vy = v.offset.y;

        v.zoom = z;

        let dx = v.offset.x - (vx * zdiff);
        let dy = v.offset.y - (vy * zdiff);

        offset.x -= dx;
        offset.y -= dy;

        self.offset = offset.map(f32::floor);
        self.organize_views();
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Commands
    ///////////////////////////////////////////////////////////////////////////

    /// Process a command.
    fn command(&mut self, cmd: Command) {
        debug!("command: {:?}", cmd);

        match cmd {
            Command::Mode(m) => {
                self.toggle_mode(m);
            }
            Command::Quit => {
                self.quit_view_safe(self.views.active_id);
            }
            Command::QuitAll => {
                // TODO (rust)
                let ids: Vec<ViewId> = self.views.keys().cloned().collect();
                for id in ids {
                    self.quit_view_safe(id);
                }
            }
            Command::SwapColors => {
                std::mem::swap(&mut self.fg, &mut self.bg);
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
            Command::BrushToggle(mode) => {
                if let Tool::Brush(ref mut b) = self.tool {
                    b.toggle(mode);
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
                if fw > Self::MAX_FRAME_SIZE || fh > Self::MAX_FRAME_SIZE {
                    self.message(
                        format!(
                            "Error: maximum frame size is {}x{}",
                            Self::MAX_FRAME_SIZE,
                            Self::MAX_FRAME_SIZE,
                        ),
                        MessageType::Error,
                    );
                    return;
                }

                let v = self.active_view_mut();
                v.resize_frames(fw, fh);
                v.touch();

                self.check_selection();
                self.organize_views();
            }
            Command::ForceQuit => self.quit_view(self.views.active_id),
            Command::ForceQuitAll => self.quit(ExitReason::Normal),
            Command::Echo(ref v) => {
                let result = match v {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Ident(s) => match s.as_str() {
                        "config/dir" => Ok(Value::Str(format!(
                            "{}",
                            self.base_dirs.config_dir().display()
                        ))),
                        "s/hidpi" => Ok(Value::Str(format!("{:.1}", self.hidpi_factor))),
                        "s/offset" => Ok(Value::Float2(self.offset.x, self.offset.y)),
                        "v/offset" => {
                            let v = self.active_view();
                            Ok(Value::Float2(v.offset.x, v.offset.y))
                        }
                        "v/zoom" => Ok(Value::Float(self.active_view().zoom as f64)),
                        _ => match self.settings.get(s) {
                            None => Err(format!("Error: {} is undefined", s)),
                            Some(result) => Ok(Value::Str(format!("{} = {}", v.clone(), result))),
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
            Command::Zoom(op) => {
                let center = self.session_coords(
                    self.views.active_id,
                    self.selection
                        .map(|s| s.bounds().center().map(|n| n as f32).into())
                        .unwrap_or(self.active_view().center()),
                );

                match op {
                    Op::Incr => {
                        self.zoom_in(center);
                    }
                    Op::Decr => {
                        self.zoom_out(center);
                    }
                    Op::Set(z) => {
                        if z < 1. || z > Self::MAX_ZOOM {
                            self.message("Error: invalid zoom level", MessageType::Error);
                        } else {
                            self.zoom(z, center);
                        }
                    }
                }
            }
            Command::Reset => {
                if let Err(e) = self.reset() {
                    self.message(format!("Error: {}", e), MessageType::Error);
                };
            }
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
                if let Some(id) = self.views.range(id..).nth(1).map(|(id, _)| *id) {
                    self.activate(id);
                    self.center_active_view_v();
                }
            }
            Command::ViewPrev => {
                let id = self.views.active_id;
                if let Some(id) = self.views.range(..id).next_back().map(|(id, _)| *id) {
                    self.activate(id);
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
                        format!("Error: clone index must be in the range {}..{}", 0, l - 1),
                        MessageType::Error,
                    );
                }
            }
            Command::RemoveFrame => {
                self.active_view_mut().shrink();
                self.check_selection();
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
                        format!("Error: slice: view width is not divisible by {}", nframes),
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
                        format!("Warning: the setting `{}` has been deprecated", k),
                        MessageType::Warning,
                    );
                    return;
                }
                match self.settings.set(k, v.clone()) {
                    Err(e) => {
                        self.message(format!("Error: {}", e), MessageType::Error);
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
                Some(Value::Bool(b)) => self.command(Command::Set(k.clone(), Value::Bool(!b))),
                Some(_) => {
                    self.message(format!("Error: can't toggle `{}`", k), MessageType::Error);
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
                } else if let Err(e) = self.edit(paths) {
                    self.message(format!("Error loading path(s): {}", e), MessageType::Error);
                }
            }
            Command::Write(None) => {
                if let Err(e) = self.save_view(self.views.active_id) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::Write(Some(ref path)) => {
                if let Err(e) = self.save_view_as(self.views.active_id, path) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::WriteQuit => {
                if self.save_view(self.views.active_id).is_ok() {
                    self.quit_view(self.views.active_id);
                }
            }
            Command::Map(map) => {
                let KeyMapping {
                    key,
                    press,
                    release,
                    modes,
                } = *map;

                self.key_bindings.add(KeyBinding {
                    key,
                    modes: modes.clone(),
                    command: press,
                    state: InputState::Pressed,
                    modifiers: platform::ModifiersState::default(),
                    is_toggle: release.is_some(),
                    display: Some(format!("{}", key)),
                });
                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        key,
                        modes,
                        command: cmd,
                        state: InputState::Released,
                        modifiers: platform::ModifiersState::default(),
                        is_toggle: true,
                        display: None,
                    });
                }
            }
            Command::MapClear => {
                self.key_bindings = KeyBindings::default();
            }
            Command::Undo => {
                self.undo(self.views.active_id);
            }
            Command::Redo => {
                self.redo(self.views.active_id);
            }
            Command::Tool(t) => {
                self.tool(t);
            }
            Command::ToolPrev => {
                self.prev_tool();
            }
            Command::Crop(_) => {
                self.unimplemented();
            }
            Command::SelectionMove(x, y) => {
                if let Some(ref mut s) = self.selection {
                    s.translate(x, y);
                }
            }
            Command::SelectionResize(x, y) => {
                if let Some(ref mut s) = self.selection {
                    s.resize(x, y);
                }
            }
            Command::SelectionExpand => {
                let v = self.active_view();
                let (fw, fh) = (v.fw as i32, v.fh as i32);
                let (vw, vh) = (v.width() as i32, v.height() as i32);

                if let Some(ref mut selection) = self.selection {
                    let r = Rect::origin(vw, vh);
                    let s = selection.bounds();
                    let min = s.min();
                    let max = s.max();

                    // If the selection is within the view rectangle, expand it,
                    // otherwise do nothing.
                    if r.contains(min) && r.contains(max.map(|n| n - 1)) {
                        let x1 = if min.x % fw == 0 {
                            min.x - fw
                        } else {
                            min.x - min.x % fw
                        };
                        let x2 = max.x + (fw - max.x % fw);
                        let y2 = fh;

                        *selection = Selection::from(Rect::new(x1, 0, x2, y2).intersection(r));
                    }
                } else {
                    self.selection = Some(Selection::new(0, 0, fw, fh));
                }
            }
            Command::SelectionOffset(mut x, mut y) => {
                if let Some(s) = &mut self.selection {
                    let r = s.abs().bounds();
                    if r.width() <= 2 && x < 0 {
                        x = 0;
                    }
                    if r.height() <= 2 && y < 0 {
                        y = 0;
                    }
                    *s = Selection::from(s.bounds().expand(x, y, x, y));
                }
            }
            Command::SelectionJump(dir) => {
                let v = self.active_view();
                let r = v.bounds();
                let fw = v.extent().fw as i32;
                if let Some(s) = &mut self.selection {
                    let mut t = *s;
                    t.translate(fw * i32::from(dir), 0);

                    if r.intersects(t.abs().bounds()) {
                        *s = t;
                    }
                }
            }
            Command::SelectionPaste => {
                if let (Mode::Visual(VisualState::Pasting), Some(s)) = (self.mode, self.selection) {
                    self.active_view_mut().paste(s.abs().bounds());
                } else {
                    // TODO: Enter paste mode?
                }
            }
            Command::SelectionYank => {
                self.yank();
            }
            Command::SelectionDelete => {
                // To mimick the behavior of `vi`, we yank the selection
                // before deleting it.
                if let Some(s) = self.yank() {
                    self.effects.extend_from_slice(&[
                        Effect::ViewBlendingChanged(Blending::constant()),
                        Effect::ViewPaintFinal(vec![Shape::Rectangle(
                            s.map(|n| n as f32),
                            ZDepth::default(),
                            Rotation::ZERO,
                            Stroke::NONE,
                            Fill::Solid(Rgba8::TRANSPARENT.into()),
                        )]),
                    ]);
                    self.active_view_mut().touch();
                }
            }
            Command::SelectionFill(color) => {
                if let Some(s) = self.selection {
                    self.effects
                        .push(Effect::ViewPaintFinal(vec![Shape::Rectangle(
                            s.abs().bounds().map(|n| n as f32),
                            ZDepth::default(),
                            Rotation::ZERO,
                            Stroke::NONE,
                            Fill::Solid(color.unwrap_or(self.fg).into()),
                        )]));
                    self.active_view_mut().touch();
                }
            }
        };
    }

    fn cmdline_hide(&mut self) {
        self.switch_mode(self.prev_mode.unwrap_or(Mode::Normal));
    }

    fn cmdline_handle_backspace(&mut self) {
        self.cmdline.delc();

        if self.cmdline.is_empty() {
            self.cmdline_hide();
        }
    }

    fn cmdline_handle_enter(&mut self) {
        let input = self.cmdline.input();
        // Always hide the command line before executing the command,
        // because commands will often require being in a specific mode, eg.
        // visual mode for commands that run on selections.
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

    fn tool(&mut self, t: Tool) {
        if std::mem::discriminant(&t) != std::mem::discriminant(&self.tool) {
            self.prev_tool = Some(self.tool.clone());
        }
        self.tool = t;
    }

    fn prev_tool(&mut self) {
        self.tool = self.prev_tool.clone().unwrap_or(Tool::default());
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
    fn color_at(&self, v: ViewId, p: ViewCoords<u32>) -> Option<Rgba8> {
        let resources = self.resources.lock();
        let (snapshot, pixels) = resources.get_snapshot(v);

        let y_offset = snapshot
            .height()
            .checked_sub(p.y)
            .and_then(|x| x.checked_sub(1));
        let index = y_offset.map(|y| (y * snapshot.width() + p.x) as usize);

        if let Some(bgra) = index.and_then(|idx| pixels.get(idx)) {
            Some(Rgba8::new(bgra.r, bgra.g, bgra.b, bgra.a))
        } else {
            None
        }
    }

    fn sample_color(&mut self) {
        if let Some(color) = self.hover_color {
            self.pick_color(color);
        }
    }
}
