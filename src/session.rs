///! Session
use crate::autocomplete::FileCompleter;
use crate::brush::*;
use crate::cmd::{self, Command, CommandLine, KeyMapping, Op, Value};
use crate::color;
use crate::data;
use crate::event::{Event, TimedEvent};
use crate::execution::{DigestMode, DigestState, Execution};
use crate::hashmap;
use crate::palette::*;
use crate::platform::{self, InputState, Key, KeyboardInput, LogicalSize, ModifiersState};
use crate::resources::{Edit, EditId, Pixels, ResourceManager};
use crate::util;
use crate::view::layer::{LayerCoords, LayerId};
use crate::view::path;
use crate::view::{
    self, FileStatus, FileStorage, View, ViewCoords, ViewExtent, ViewId, ViewManager, ViewOp,
    ViewState,
};

use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::{Rgba8, ZDepth};
use rgx::math::*;
use rgx::rect::Rect;

use directories as dirs;
use nonempty::NonEmpty;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Write;
use std::ops::{Add, Deref, Sub};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time;

/// Settings help string.
pub const SETTINGS: &str = r#"
SETTINGS

debug             on/off             Debug mode
checker           on/off             Alpha checker toggle
vsync             on/off             Vertical sync toggle
scale             1.0..4.0           UI scale
animation         on/off             View animation toggle
animation/delay   1..1000            View animation delay (ms)
background        #000000..#ffffff   Set background appearance to <color>
grid              on/off             Grid display
grid/color        #000000..#ffffff   Grid color
grid/spacing      <x> <y>            Grid spacing
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

impl fmt::Display for Rgb8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
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
    /// When the session UI scale has changed.
    SessionScaled(f64),
    /// When a view has been activated.
    ViewActivated(ViewId),
    /// When a view has been added.
    ViewAdded(ViewId),
    /// When a view has been removed.
    ViewRemoved(ViewId),
    /// When a view has been touched (edited).
    ViewTouched(ViewId),
    /// When a view operation has taken place.
    ViewOps(ViewId, Vec<ViewOp>),
    /// When a view requires re-drawing.
    ViewDamaged(ViewId, Option<ViewExtent>),
    /// When a view layer requires re-drawing.
    ViewLayerDamaged(ViewId, LayerId),
    /// When the active view is non-permanently painted on.
    ViewPaintDraft(Vec<Shape>),
    /// When the active view is painted on.
    ViewPaintFinal(Vec<Shape>),
    /// The blend mode used for painting has changed.
    ViewBlendingChanged(Blending),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Blending {
    Constant,
    Alpha,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PresentMode {
    Vsync,
    NoVsync,
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
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Tool {
    /// The standard drawing tool.
    Brush(Brush),
    /// Used to sample colors.
    Sampler,
    /// Used to pan the workspace.
    Pan(PanState),
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Brush(Brush::default())
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
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

    pub fn is_execution(&self) -> bool {
        self.message_type == MessageType::Execution
    }

    pub fn is_debug(&self) -> bool {
        self.message_type == MessageType::Debug
    }

    /// Log a message to stdout/stderr.
    fn log(&self) {
        match self.message_type {
            MessageType::Info => info!("{}", self),
            MessageType::Hint => {}
            MessageType::Echo => info!("{}", self),
            MessageType::Error => error!("{}", self),
            MessageType::Warning => warn!("{}", self),
            MessageType::Execution => {}
            MessageType::Okay => info!("{}", self),
            MessageType::Debug => debug!("{}", self),
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
    /// Execution-related message.
    Execution,
    /// Debug message.
    Debug,
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
            MessageType::Execution => color::GREY,
            MessageType::Debug => color::LIGHT_GREEN,
            MessageType::Okay => color::GREEN,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A session error.
type Error = String;

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Input {
    Key(Key),
    Character(char),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(k) => write!(f, "{}", k),
            Self::Character(c) => write!(f, "{}", c),
        }
    }
}

/// A key binding.
#[derive(PartialEq, Clone, Debug)]
pub struct KeyBinding {
    /// The `Mode`s this binding applies to.
    pub modes: Vec<Mode>,
    /// Modifiers which must be held.
    pub modifiers: ModifiersState,
    /// Input expected to trigger the binding.
    pub input: Input,
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

impl KeyBinding {
    fn is_match(
        &self,
        input: Input,
        state: InputState,
        modifiers: ModifiersState,
        mode: Mode,
    ) -> bool {
        match (input, self.input) {
            (Input::Key(key), Input::Key(k)) => {
                key == k
                    && self.state == state
                    && self.modes.contains(&mode)
                    && (self.modifiers == modifiers
                        || state == InputState::Released
                        || key.is_modifier())
            }
            (Input::Character(a), Input::Character(b)) => {
                // Nb. We only check the <ctrl> modifier with characters,
                // because the others (especially <shift>) will most likely
                // input a different character.
                a == b
                    && self.modes.contains(&mode)
                    && self.state == state
                    && self.modifiers.ctrl == modifiers.ctrl
            }
            _ => false,
        }
    }
}

/// Manages a list of key bindings.
#[derive(Debug)]
pub struct KeyBindings {
    elems: Vec<KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        KeyBindings { elems: vec![] }
    }
}

impl KeyBindings {
    /// Create an empty set of key bindings.
    pub fn new() -> Self {
        Self { elems: Vec::new() }
    }

    /// Add a key binding.
    pub fn add(&mut self, binding: KeyBinding) {
        for mode in binding.modes.iter() {
            self.elems
                .retain(|kb| !kb.is_match(binding.input, binding.state, binding.modifiers, *mode));
        }
        self.elems.push(binding);
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// Find a key binding based on some input state.
    pub fn find(
        &self,
        input: Input,
        modifiers: ModifiersState,
        state: InputState,
        mode: Mode,
    ) -> Option<KeyBinding> {
        self.elems
            .iter()
            .rev()
            .cloned()
            .find(|kb| kb.is_match(input, state, modifiers, mode))
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
    const DEPRECATED: &'static [&'static str] = &["frame_delay", "input/delay"];

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
                "invalid value `{}` for `{}`, expected {}",
                v,
                k,
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
                "background" => Value::Rgba8(color::TRANSPARENT),
                "vsync" => Value::Bool(false),
                "input/mouse" => Value::Bool(true),
                "scale" => Value::F64(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::U32(160),
                "ui/palette" => Value::Bool(true),
                "ui/status" => Value::Bool(true),
                "ui/cursor" => Value::Bool(true),
                "ui/message" => Value::Bool(true),
                "ui/switcher" => Value::Bool(true),
                "ui/view-info" => Value::Bool(true),

                "grid" => Value::Bool(false),
                "grid/color" => Value::Rgba8(color::BLUE),
                "grid/spacing" => Value::U32Tuple(8, 8),

                "p/height" => Value::U32(Session::PALETTE_HEIGHT),

                "debug/crosshair" => Value::Bool(false),

                // Deprecated.
                "frame_delay" => Value::F64(0.0),
                "input/delay" => Value::F64(8.0)
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
    /// The current working directory.
    pub cwd: PathBuf,

    /// The cursor coordinates.
    pub cursor: SessionCoords,

    /// The color under the cursor, if any.
    pub hover_color: Option<Rgba8>,
    /// The view under the cursor, if any.
    pub hover_view: Option<(ViewId, LayerId)>,

    /// The workspace offset. Views are offset by this vector.
    pub offset: Vector2<f32>,
    /// The help view offset.
    pub help_offset: Vector2<f32>,
    /// The current message displayed to the user.
    pub message: Message,

    /// The session foreground color.
    pub fg: Rgba8,
    /// The session background color.
    pub bg: Rgba8,

    /// The current frame number.
    frame_number: u64,

    /// Directories in which application-specific user configuration is stored.
    proj_dirs: dirs::ProjectDirs,
    /// User directories.
    base_dirs: dirs::BaseDirs,

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
}

impl Session {
    /// Maximum number of views in a session.
    pub const MAX_VIEWS: usize = 64;
    /// Default view width.
    pub const DEFAULT_VIEW_W: u32 = 128;
    /// Default view height.
    pub const DEFAULT_VIEW_H: u32 = 128;

    /// Minimum margin between views, in pixels.
    const VIEW_MARGIN: f32 = 24.;
    /// Size of palette cells, in pixels.
    const PALETTE_CELL_SIZE: f32 = 24.;
    /// Default palette height in cells.
    const PALETTE_HEIGHT: u32 = 16;
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
    pub fn new<P: AsRef<Path>>(
        w: u32,
        h: u32,
        cwd: P,
        resources: ResourceManager,
        proj_dirs: dirs::ProjectDirs,
        base_dirs: dirs::BaseDirs,
    ) -> Self {
        let history_path = proj_dirs.data_dir().join("history");
        let cwd = cwd.as_ref().to_path_buf();

        Self {
            state: State::Initializing,
            width: w as f32,
            height: h as f32,
            cwd: cwd.clone(),
            cursor: SessionCoords::new(0., 0.),
            base_dirs,
            proj_dirs,
            offset: Vector2::zero(),
            help_offset: Vector2::zero(),
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
            palette: Palette::new(Self::PALETTE_CELL_SIZE, Self::PALETTE_HEIGHT as usize),
            key_bindings: KeyBindings::default(),
            keys_pressed: HashSet::new(),
            ignore_received_characters: false,
            cmdline: CommandLine::new(cwd, history_path, path::SUPPORTED_READ_FORMATS),
            mode: Mode::Normal,
            prev_mode: Option::default(),
            selection: Option::default(),
            message: Message::default(),
            resources,
            avg_time: time::Duration::from_secs(0),
            frame_number: 0,
            queue: Vec::new(),
        }
    }

    /// Initialize a session.
    pub fn init(mut self, source: Option<PathBuf>) -> std::io::Result<Self> {
        self.transition(State::Running);
        self.reset()?;

        if let Some(init) = source {
            // The special source '-' is used to skip initialization.
            if init.as_os_str() != "-" {
                self.source_path(&init)?;
            }
        } else {
            let dir = self.proj_dirs.config_dir().to_owned();
            let cfg = dir.join(Self::INIT);

            if cfg.exists() {
                self.source_path(cfg)?;
            }
        }

        self.source_dir(self.cwd.clone()).ok();
        self.cmdline.history.load()?;
        self.message(format!("rx v{}", crate::VERSION), MessageType::Debug);

        Ok(self)
    }

    // Reset to factory defaults.
    pub fn reset(&mut self) -> io::Result<()> {
        self.key_bindings = KeyBindings::default();
        self.settings = Settings::default();
        self.tool = Tool::default();

        self.source_reader(io::BufReader::new(data::CONFIG), "<init>")
    }

    /// Create a blank view.
    pub fn blank(&mut self, fs: FileStatus, w: u32, h: u32) {
        let frames = vec![vec![Rgba8::TRANSPARENT; w as usize * h as usize]];
        let id = self.add_view(fs, w, h, frames);
        self.organize_views();
        self.edit_view(id);
    }

    pub fn with_blank(mut self, fs: FileStatus, w: u32, h: u32) -> Self {
        self.blank(fs, w, h);

        self
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

        for v in self.views.iter_mut() {
            if self.settings["animation"].is_set() {
                v.update(delta);
            }
        }
        if self.ignore_received_characters {
            self.ignore_received_characters = false;
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
                    self.message("Replay ended", MessageType::Execution);

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
                        self.message("Replay ended", MessageType::Execution);

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
                Align::BottomLeft,
            );
            if !output.is_empty() {
                match brush.state {
                    // If we're erasing, we can't use the staging framebuffer, since we
                    // need to be replacing pixels on the real buffer.
                    _ if brush.is_set(BrushMode::Erase) => {
                        self.effects.extend_from_slice(&[
                            Effect::ViewBlendingChanged(Blending::Constant),
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
                            Effect::ViewBlendingChanged(Blending::Alpha),
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
            for v in self.views.iter_mut() {
                if !v.ops.is_empty() {
                    self.effects
                        .push(Effect::ViewOps(v.id, v.ops.drain(..).collect()));
                }
                match v.state {
                    ViewState::Dirty(_) | ViewState::LayerDirty(_) => {}
                    ViewState::Damaged(extent) => {
                        self.effects.push(Effect::ViewDamaged(v.id, extent));
                    }
                    ViewState::LayerDamaged(layer) => {
                        self.effects.push(Effect::ViewLayerDamaged(v.id, layer));
                    }
                    ViewState::Okay => {}
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
        self.effects.drain(..).collect()
    }

    /// Cleanup to be run at the end of the frame.
    pub fn cleanup(&mut self) {
        for v in self.views.iter_mut() {
            v.okay();
        }
    }

    /// Quit the session.
    pub fn quit(&mut self, r: ExitReason) {
        if self.cmdline.history.save().is_err() {
            error!(
                "Error: couldn't save command history to {}",
                self.cmdline.history.path.display()
            );
        }
        self.transition(State::Closing(r));
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

    /// Get the current animation delay. Returns `None` if animations aren't playing,
    /// or if none of the views have more than one frame.
    pub fn animation_delay(&self) -> Option<time::Duration> {
        let animations = self.views.iter().any(|v| v.animation.len() > 1);

        if self.settings["animation"].is_set() && animations {
            let delay = self.settings["animation/delay"].to_u64();
            Some(time::Duration::from_millis(delay))
        } else {
            None
        }
    }

    /// Check whether the session is running.
    pub fn is_running(&self) -> bool {
        self.state == State::Running
    }

    /// Return help string.
    pub fn help(&self) -> Vec<String> {
        self.cmdline
            .commands
            .iter()
            .map(|(_, help, parser)| format!(":{:<36} {}", parser.to_string(), help))
            .collect()
    }

    ////////////////////////////////////////////////////////////////////////////

    /// Pan the view by a relative amount.
    fn pan(&mut self, x: f32, y: f32) {
        match self.mode {
            Mode::Help => {
                self.help_offset.x += x;
                self.help_offset.y += y;

                if self.help_offset.x > 0. {
                    self.help_offset.x = 0.;
                }
                if self.help_offset.y < 0. {
                    self.help_offset.y = 0.;
                }
            }
            _ => {
                self.offset.x += x;
                self.offset.y += y;
            }
        }
        self.cursor_dirty();
    }

    /// Re-compute state related to the cursor position. This is useful
    /// when the cursor hasn't moved relative to the session, but things
    /// within the session have moved relative to the cursor.
    fn cursor_dirty(&mut self) {
        if !self.settings["input/mouse"].is_set() {
            return;
        }
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

        for v in self.views.iter_mut() {
            let p = cursor - self.offset;
            if let Some(l) = v.contains(p) {
                self.hover_view = Some((v.id, l));
                break;
            }
        }

        self.hover_color = if self.palette.hover.is_some() {
            self.palette.hover
        } else if let Some((v, l)) = self.hover_view {
            let p: LayerCoords<u32> = self.layer_coords(v, l, cursor).into();
            self.color_at(v, l, p)
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
                self.views
                    .iter_mut()
                    .for_each(|v| v.set_animation_delay(new.to_u64()));
            }
            "p/height" => {
                self.palette.height = new.to_u64() as usize;
                self.center_palette();
            }
            "scale" => {
                // TODO: We need to recompute the cursor position here
                // from the window coordinates. Currently, cursor position
                // is stored only in `SessionCoords`, which would have
                // to change.
                self.rescale(old.to_f64(), new.to_f64());
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
            .get(id)
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
    }

    /// Check whether a view is active.
    pub fn is_active(&self, id: ViewId) -> bool {
        self.views.active_id == id
    }

    /// Activate the currently hovered layer, if in the active view.
    pub fn activate_hover_layer(&mut self) {
        let active_id = self.views.active_id;

        match self.hover_view {
            Some((view_id, layer_id)) if view_id == active_id => {
                self.view_mut(active_id).activate_layer(layer_id);
            }
            _ => {}
        }
    }

    /// Convert "logical" window coordinates to session coordinates.
    pub fn window_to_session_coords(&self, position: platform::LogicalPosition) -> SessionCoords {
        let (x, y) = (position.x, position.y);
        let scale: f64 = self.settings["scale"].to_f64();
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

    pub fn layer_coords(&self, v: ViewId, l: LayerId, p: SessionCoords) -> LayerCoords<f32> {
        let v = self.view(v);
        let SessionCoords(p) = p;

        let p = p - self.offset - v.offset - v.layer_offset(l, v.zoom);
        let mut p = p / v.zoom;

        if v.flip_x {
            p.x = v.width() as f32 - p.x;
        }
        if v.flip_y {
            p.y = v.height() as f32 - p.y;
        }

        LayerCoords::new(p.x.floor(), p.y.floor())
    }

    /// Convert session coordinates to layer coordinates of the active layer.
    pub fn active_layer_coords(&self, p: SessionCoords) -> LayerCoords<f32> {
        let v = self.active_view();
        self.layer_coords(v.id, v.active_layer_id, p)
    }

    /// Check whether a point is inside the selection, if any.
    pub fn is_selected(&self, p: LayerCoords<i32>) -> bool {
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
        use std::ffi::OsStr;

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
                    if path.file_name() == Some(OsStr::new(".rxrc")) {
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
                self.blank(
                    FileStatus::New(FileStorage::Single(path.with_extension("png"))),
                    w,
                    h,
                );
            }
        }

        if let Some(id) = self.views.last().map(|v| v.id) {
            self.organize_views();
            self.edit_view(id);
        }

        Ok(())
    }

    /// Load the given paths into the session as frames in a new view.
    pub fn edit_frames<P: AsRef<Path>>(&mut self, paths: &[P]) -> io::Result<()> {
        let completer = FileCompleter::new(&self.cwd, path::SUPPORTED_READ_FORMATS);
        let mut dirs = Vec::new();

        let mut paths = paths
            .iter()
            .map(|path| {
                let path = path.as_ref();

                if path.is_dir() {
                    dirs.push(path);
                    completer
                        .paths(path)
                        .map(|paths| paths.map(|p| path.join(p)).collect())
                } else if path.exists() {
                    Ok(vec![path.to_path_buf()])
                } else if !path.exists() && path.with_extension("png").exists() {
                    Ok(vec![path.with_extension("png")])
                } else {
                    Ok(vec![])
                }
            })
            // Collect paths and errors.
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|p| p.file_name().is_some() && p.file_stem().is_some())
            .collect::<Vec<_>>();

        // Sort by filenames. This allows us to combine frames from multiple
        // locations without worrying about the full path name.
        paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        // If our paths list is empty, return early.
        let paths = if let Some(paths) = NonEmpty::from_slice(paths.as_slice()) {
            paths
        } else {
            return Ok(());
        };

        // Load images and collect errors.
        let mut frames = paths
            .iter()
            .map(ResourceManager::load_image)
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .peekable();

        // Use the first frame as a reference for what size the rest of
        // the frames should be.
        if let Some((fw, fh, _)) = frames.peek() {
            let (fw, fh) = (*fw, *fh);

            if frames.clone().all(|(w, h, _)| w == fw && h == fh) {
                let frames: Vec<_> = frames.map(|(_, _, pixels)| pixels).collect();
                self.add_view(FileStatus::Saved(FileStorage::Range(paths)), fw, fh, frames);
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("frame dimensions must all match {}x{}", fw, fh),
                ));
            }
        }

        for dir in dirs.iter() {
            self.source_dir(dir).ok();
        }

        if let Some(id) = self.views.last().map(|v| v.id) {
            self.organize_views();
            self.edit_view(id);
        }

        Ok(())
    }

    /// Save the given view to disk with the current file name. Returns
    /// an error if the view has no file name.
    pub fn save_view(&mut self, id: ViewId) -> io::Result<()> {
        if let Some(f) = self.view(id).file_storage().cloned() {
            self.save_view_as(id, f)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "no file name given"))
        }
    }

    /// Save a view with the given file name. Returns an error if
    /// the format is not supported.
    pub fn save_view_as(&mut self, id: ViewId, storage: FileStorage) -> io::Result<()> {
        let view = self.view(id);
        let active_layer_id = view.active_layer_id;
        let ext = view.extent();
        let nlayers = view.layers.len();

        match &storage {
            FileStorage::Single(path) if nlayers > 1 => {
                let written = self.resources.save_view_archive(id, path)?;
                let edit_id = self.resources.lock().current_edit(id);

                self.view_mut(id).save_as(edit_id, storage.clone());
                self.message(
                    format!("\"{}\" {} pixels written", storage, written),
                    MessageType::Info,
                );
            }
            FileStorage::Single(path) => {
                if let Some(s_id) =
                    self.save_layer_rect_as(id, active_layer_id, ext.rect(), &path)?
                {
                    self.view_mut(id).save_as(s_id, storage.clone());
                }
                self.message(
                    format!(
                        "\"{}\" {} pixels written",
                        storage,
                        ext.width() * ext.height()
                    ),
                    MessageType::Info,
                );
            }
            FileStorage::Range(paths) if nlayers == 1 => {
                for (i, path) in paths.iter().enumerate() {
                    self.save_layer_rect_as(id, active_layer_id, ext.frame(i), path)?;
                }

                let edit_id = self.resources.lock().current_edit(id);
                self.view_mut(id).save_as(edit_id, storage.clone());
                self.message(
                    format!(
                        "{} {} pixels written",
                        storage,
                        paths.len() * (ext.fw * ext.fh) as usize
                    ),
                    MessageType::Info,
                )
            }
            FileStorage::Range(_) => {
                self.message(
                    "Error: range storage is not supported for more than one layer",
                    MessageType::Error,
                );
            }
        };

        Ok(())
    }

    /// Private ///////////////////////////////////////////////////////////////////

    fn save_layer_rect_as(
        &mut self,
        id: ViewId,
        layer_id: LayerId,
        rect: Rect<u32>,
        path: &Path,
    ) -> io::Result<Option<EditId>> {
        let ext = path.extension().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "file path requires an extension (.gif or .png)",
            )
        })?;
        let ext = ext.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "file extension is not valid unicode")
        })?;

        if !path::SUPPORTED_WRITE_FORMATS.contains(&ext) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{}` is not a supported output format", ext),
            ));
        }

        if ext == "gif" {
            self.save_view_gif(id, layer_id, path)?;
            return Ok(None);
        } else if ext == "svg" {
            self.save_view_svg(id, layer_id, path)?;
            return Ok(None);
        }

        // Only allow overwriting of files if it's the file of the view being saved.
        if path.exists()
            && self
                .view(id)
                .file_storage()
                .map_or(true, |f| !f.contains(path))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("\"{}\" already exists", path.display()),
            ));
        }

        let (e_id, _) = self.resources.save_layer(id, layer_id, rect, &path)?;

        Ok(Some(e_id))
    }

    /// Load a view into the session.
    fn load_view<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        let path = view::Path::try_from(path)?;

        debug!("load: {:?}", path);

        // View is already loaded.
        if let Some(View { id, .. }) = self
            .views
            .find(|v| v.file_storage().map_or(false, |f| f.contains(&*path)))
        {
            // TODO: Reload from disk.
            let id = *id;
            self.activate(id);
            return Ok(());
        }

        match path.format {
            view::Format::Png => {
                let (width, height, pixels) = ResourceManager::load_image(&*path)?;

                self.add_view(
                    FileStatus::Saved(FileStorage::Single((*path).into())),
                    width,
                    height,
                    vec![pixels],
                );
                self.message(
                    format!("\"{}\" {} pixels read", path.display(), width * height),
                    MessageType::Info,
                );
            }
            view::Format::Archive => {
                let archive = ResourceManager::load_archive(&*path)?;
                let extent = archive.manifest.extent;
                let mut layers = archive.layers.into_iter();

                let frames = layers.next().expect("there is at least one layer");
                let view_id = self.add_view(
                    FileStatus::Saved(FileStorage::Single((*path).into())),
                    extent.fw,
                    extent.fh,
                    frames,
                );

                for layer in layers {
                    let pixels = util::stitch_frames(
                        layer,
                        extent.fw as usize,
                        extent.fh as usize,
                        Rgba8::TRANSPARENT,
                    );
                    self.add_layer(view_id, Some(Pixels::from_rgba8(pixels.into())));
                }
            }
            view::Format::Gif => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "gif files are not supported",
                ));
            }
        }

        Ok(())
    }

    fn add_layer(&mut self, view_id: ViewId, pixels: Option<Pixels>) {
        let l = self.view_mut(view_id).add_layer();
        let v = self.view(view_id);

        self.resources
            .lock_mut()
            .get_view_mut(v.id)
            .expect(&format!("view #{} must exist", v.id))
            .add_layer(
                l,
                v.extent(),
                pixels.unwrap_or(Pixels::blank(v.width() as usize, v.height() as usize)),
            );
    }

    fn add_view(
        &mut self,
        file_status: FileStatus,
        fw: u32,
        fh: u32,
        frames: Vec<Vec<Rgba8>>,
    ) -> ViewId {
        let nframes = frames.len();
        assert!(nframes >= 1);

        // Replace the active view if it's a scratch pad.
        if let Some(v) = self.views.active() {
            let id = v.id;

            if v.file_status == FileStatus::NoFile {
                self.destroy_view(id);
            }
        }

        let pixels = util::stitch_frames(frames, fw as usize, fh as usize, Rgba8::TRANSPARENT);
        let delay = self.settings["animation/delay"].to_u64();
        let id = self.views.add(file_status, fw, fh, nframes, delay);

        self.effects.push(Effect::ViewAdded(id));
        self.resources.add_view(
            id,
            ViewExtent::new(fw, fh, nframes),
            Pixels::from_rgba8(pixels.into()),
        );
        id
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
    fn save_view_gif<P: AsRef<Path>>(
        &mut self,
        id: ViewId,
        layer_id: LayerId,
        path: P,
    ) -> io::Result<()> {
        let delay = self.view(id).animation.delay;
        let palette = self.colors();
        let npixels = self
            .resources
            .save_view_gif(id, layer_id, &path, delay, &palette)?;

        self.message(
            format!("\"{}\" {} pixels written", path.as_ref().display(), npixels),
            MessageType::Info,
        );
        Ok(())
    }

    /// Save a view as an svg.
    fn save_view_svg<P: AsRef<Path>>(
        &mut self,
        id: ViewId,
        layer_id: LayerId,
        path: P,
    ) -> io::Result<()> {
        let npixels = self.resources.save_view_svg(id, layer_id, &path)?;

        self.message(
            format!("\"{}\" {} pixels written", path.as_ref().display(), npixels),
            MessageType::Info,
        );
        Ok(())
    }

    fn colors(&self) -> Vec<Rgba8> {
        let mut palette = self.palette.colors.clone();

        palette.push(self.fg);
        palette.push(self.bg);
        palette
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
        let first = self
            .views
            .first_mut()
            .expect("view list should never be empty");

        first.offset.y = 0.;

        // TODO: We need a way to distinguish view content size with real (rendered) size. Also
        // create a distinction between layer and view height. Right now `View::height` is layer
        // height.
        let mut offset =
            (first.height() * first.layers.len() as u32) as f32 * first.zoom + Self::VIEW_MARGIN;

        for v in self.views.iter_mut().skip(1) {
            if v.layers.len() > 1 {
                // Account for layer composite.
                offset += v.height() as f32 * v.zoom;
            }
            v.offset.y = offset;

            offset += (v.height() * v.layers.len() as u32) as f32 * v.zoom + Self::VIEW_MARGIN;
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
    fn yank_selection(&mut self) -> Option<Rect<i32>> {
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
        let result = self.resources.lock_mut().get_view_mut(id).and_then(|s| {
            if dir == Direction::Backward {
                s.history_prev()
            } else {
                s.history_next()
            }
        });

        match result {
            Some((eid, Edit::LayerPainted(layer))) => {
                self.view_mut(id).restore_layer(eid, layer);
            }
            Some((eid, Edit::LayerAdded(layer))) => {
                match dir {
                    Direction::Backward => {
                        self.view_mut(id).remove_layer(layer);
                    }
                    Direction::Forward => {
                        // TODO: This relies on the fact that `remove_layer` can
                        // only remove the last layer.
                        let layer_id = self.view_mut(id).add_layer();
                        debug_assert!(layer_id == layer);
                    }
                };
                self.view_mut(id).refresh_file_status(eid);
            }
            Some((eid, Edit::ViewResized(_, from, to))) => {
                let extent = match dir {
                    Direction::Backward => from,
                    Direction::Forward => to,
                };
                self.view_mut(id).restore_extent(eid, extent);
            }
            Some((eid, Edit::ViewPainted(_))) => {
                self.view_mut(id).restore(eid);
            }
            Some((_, Edit::Initial)) => {}
            None => {}
        }
        self.organize_views();
        self.cursor_dirty();
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
                        MessageType::Execution,
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
            Event::ReceivedCharacter(c, mods) => self.handle_received_character(c, mods),
            Event::Paste(p) => self.handle_paste(p),
        }
    }

    pub fn resize(&mut self, size: platform::LogicalSize, scale: f64) {
        let (w, h) = (size.width / scale, size.height / scale);

        self.width = w as f32;
        self.height = h as f32;

        // TODO: Reset session cursor coordinates
        self.center_palette();
        self.center_active_view();
    }

    pub fn rescale(&mut self, old: f64, new: f64) {
        let (w, h) = (self.width as f64 * old, self.height as f64 * old);

        self.resize(platform::LogicalSize::new(w, h), new);
        self.effects.push(Effect::SessionScaled(new));
    }

    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        self.resize(size, self.settings["scale"].to_f64());
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
                if let Some((id, layer_id)) = self.hover_view {
                    // Clicking on a view is one way to get out of command mode.
                    if self.mode == Mode::Command {
                        self.cmdline_hide();
                        return;
                    }
                    if self.is_active(id) {
                        {
                            let v = self.view_mut(id);
                            v.activate_layer(layer_id);
                        }
                        let v = self.view(id);
                        let p = self.active_layer_coords(self.cursor);

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
                                // Re-center the selection in-case we've switched layer.
                                self.center_selection(self.cursor);
                                self.command(Command::SelectionPaste);
                            }
                            Mode::Present | Mode::Help => {}
                        }
                    } else {
                        self.activate(id);
                        self.center_selection(self.cursor);
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
                                self.active_view_mut().touch_layer();
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
            if let Some((v, _)) = self.hover_view {
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

        let prev_cursor = self.cursor;
        let p = self.active_layer_coords(cursor);
        let prev_p = self.active_layer_coords(prev_cursor);
        let (vw, vh) = self.active_view().size();

        self.cursor = cursor;
        self.cursor_dirty();

        match self.tool {
            Tool::Pan(PanState::Panning) => {
                self.pan(cursor.x - prev_cursor.x, cursor.y - prev_cursor.y);
            }
            Tool::Sampler if self.mouse_state == InputState::Pressed => {
                self.sample_color();
            }
            _ => {
                match self.mode {
                    Mode::Normal => match self.tool {
                        Tool::Brush(ref mut brush) if p != prev_p => match brush.state {
                            BrushState::DrawStarted { .. } | BrushState::Drawing { .. } => {
                                let mut p: LayerCoords<i32> = p.into();
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
                            _ => self.activate_hover_layer(),
                        },
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
                        self.activate_hover_layer();
                        self.center_selection(cursor);
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_paste(&mut self, paste: Option<String>) {
        if let Some(s) = paste {
            self.cmdline.puts(s.as_str())
        }
    }

    fn handle_received_character(&mut self, c: char, mods: ModifiersState) {
        if self.mode == Mode::Command {
            if c.is_control() || self.ignore_received_characters {
                return;
            }
            self.cmdline_handle_input(c);
        } else if let Some(kb) =
            self.key_bindings
                .find(Input::Character(c), mods, InputState::Pressed, self.mode)
        {
            self.command(kb.command);
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
                            platform::Key::Up => {
                                self.cmdline.history_prev();
                            }
                            platform::Key::Down => {
                                self.cmdline.history_next();
                            }
                            platform::Key::Left => {
                                self.cmdline.cursor_backward();
                            }
                            platform::Key::Right => {
                                self.cmdline.cursor_forward();
                            }
                            platform::Key::Tab => {
                                self.cmdline.completion_next();
                            }
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
                .find(Input::Key(key), modifiers, state, self.mode)
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
                    self.message("Saving recording...", MessageType::Execution);
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

        File::open(&path)
            .or_else(|_| File::open(self.proj_dirs.config_dir().join(path)))
            .and_then(|f| self.source_reader(io::BufReader::new(f), path))
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("error sourcing {}: {}", path.display(), e),
                )
            })
    }

    /// Source a directory which contains a `.rxrc` script. Returns an
    /// error if the script wasn't found or couldn't be sourced.
    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

    /// Source a script from an [`io::BufRead`].
    fn source_reader<P: AsRef<Path>, R: io::BufRead>(&mut self, r: R, _path: P) -> io::Result<()> {
        for (i, line) in r.lines().enumerate() {
            let line = line?;

            if line.starts_with(cmd::COMMENT) {
                continue;
            }
            match self.cmdline.parse(&format!(":{}", line)) {
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("{} on line {}", e, i + 1),
                    ))
                }
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
        let h = self.settings["p/height"].to_u64() as usize;
        let n = usize::min(self.palette.size(), h) as f32;
        let p = &mut self.palette;

        p.x = 0.;
        p.y = self.height / 2. - n * p.cellsize / 2.;
    }

    /// Vertically the active view in the workspace.
    fn center_active_view_v(&mut self) {
        if let Some(v) = self.views.active() {
            self.offset.y =
                (self.height / 2. - v.height() as f32 / 2. * v.zoom - v.offset.y).floor();
            self.cursor_dirty();
        }
    }

    /// Horizontally center the active view in the workspace.
    fn center_active_view_h(&mut self) {
        if let Some(v) = self.views.active() {
            self.offset.x = (self.width / 2. - v.width() as f32 * v.zoom / 2. - v.offset.x).floor();
            self.cursor_dirty();
        }
    }

    /// Center the active view in the workspace.
    fn center_active_view(&mut self) {
        self.center_active_view_v();
        self.center_active_view_h();
    }

    /// Center the given frame of the active view in the workspace.
    fn center_active_view_frame(&mut self, frame: usize) {
        self.center_active_view_v();

        if let Some(v) = self.views.active() {
            let offset = (frame as u32 * v.fw) as f32 * v.zoom;

            self.offset.x = self.width / 2. - offset - v.offset.x - v.fw as f32 / 2. * v.zoom;
            self.offset.x = self.offset.x.floor();

            self.cursor_dirty();
        }
    }

    /// The session center.
    fn center(&self) -> SessionCoords {
        SessionCoords::new(self.width / 2., self.height / 2.)
    }

    /// Center the selection to the given session coordinates.
    fn center_selection(&mut self, p: SessionCoords) {
        let c = self.active_layer_coords(p);
        if let Some(ref mut s) = self.selection {
            let r = s.abs().bounds();
            let (w, h) = (r.width(), r.height());
            let (x, y) = (c.x as i32 - w / 2, c.y as i32 - h / 2);
            *s = Selection::new(x, y, x + w, y + h);
        }
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
                let ids: Vec<ViewId> = self.views.ids().collect();
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
            Command::FrameResize(fw, fh) => {
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

                self.check_selection();
                self.organize_views();
            }
            Command::FramePrev => {
                let v = self.active_view().extent();
                let center = self.active_view_coords(self.center());

                if center.x >= 0. {
                    let frame = v.to_frame(center.into()).min(v.nframes);
                    self.center_active_view_frame(frame.saturating_sub(1));
                }
            }
            Command::FrameNext => {
                let v = self.active_view().extent();
                let center = self.active_view_coords(self.center());
                let frame = v.to_frame(center.into());

                if center.x < 0. {
                    self.center_active_view_frame(0);
                } else if frame < v.nframes {
                    self.center_active_view_frame((frame + 1).min(v.nframes - 1));
                }
            }
            Command::ForceQuit => self.quit_view(self.views.active_id),
            Command::ForceQuitAll => self.quit(ExitReason::Normal),
            Command::Echo(ref v) => {
                let result = match v {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Ident(s) => match s.as_str() {
                        "config/dir" => Ok(Value::Str(format!(
                            "{}",
                            self.proj_dirs.config_dir().display()
                        ))),
                        "s/cwd" | "cwd" => Ok(Value::Str(self.cwd.display().to_string())),
                        "s/offset" => Ok(Value::F32Tuple(self.offset.x, self.offset.y)),
                        "v/offset" => {
                            let v = self.active_view();
                            Ok(Value::F32Tuple(v.offset.x, v.offset.y))
                        }
                        "v/zoom" => Ok(Value::F64(self.active_view().zoom as f64)),
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
            Command::PaletteSort => {
                // Sort by total luminosity. This is pretty lame, but it's
                // something to work with.
                self.palette.colors.sort_by(|a, b| {
                    (a.r as u32 + a.g as u32 + a.b as u32)
                        .cmp(&(b.r as u32 + b.g as u32 + b.b as u32))
                });
            }
            Command::PaletteSample => {
                {
                    let v = self.active_view();
                    let resources = self.resources.lock();
                    let (_, pixels) = resources.get_snapshot(v.id, v.active_layer_id);

                    for pixel in pixels.iter() {
                        if pixel != Rgba8::TRANSPARENT {
                            self.palette.add(pixel);
                        }
                    }
                }
                self.command(Command::PaletteSort);
                self.center_palette();
            }
            Command::PaletteWrite(path) => match File::create(&path) {
                Ok(mut f) => {
                    for color in self.palette.colors.iter() {
                        writeln!(&mut f, "{}", color.to_string()).ok();
                    }
                    self.message(
                        format!(
                            "Palette written to {} ({} colors)",
                            path,
                            self.palette.size()
                        ),
                        MessageType::Info,
                    );
                }
                Err(err) => {
                    self.message(format!("Error: `{}`: {}", path, err), MessageType::Error);
                }
            },
            Command::Zoom(op) => {
                let center = if let Some(s) = self.selection {
                    self.session_coords(
                        self.views.active_id,
                        s.bounds().center().map(|n| n as f32).into(),
                    )
                } else if self.hover_view.is_some() {
                    self.cursor
                } else {
                    self.session_coords(self.views.active_id, self.active_view().center())
                };

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
                } else {
                    self.message("Settings reset to default values", MessageType::Okay);
                }
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

                if let Some(id) = self
                    .views
                    .after(id)
                    .or_else(|| self.views.first().map(|v| v.id))
                {
                    self.activate(id);
                    self.center_active_view();
                }
            }
            Command::ViewPrev => {
                let id = self.views.active_id;

                if let Some(id) = self
                    .views
                    .before(id)
                    .or_else(|| self.views.last().map(|v| v.id))
                {
                    self.activate(id);
                    self.center_active_view();
                }
            }
            Command::ViewCenter => {
                self.center_active_view();
            }
            Command::FrameAdd => {
                self.active_view_mut().extend();
            }
            Command::FrameClone(n) => {
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
            Command::FrameRemove => {
                self.active_view_mut().shrink();
                self.check_selection();
            }
            Command::LayerAdd => {
                let view_id = self.views.active_id;
                self.add_layer(view_id, None);
                self.organize_views();
            }
            Command::LayerRemove(id) => {
                if let Some(id) = id {
                    self.active_view_mut().remove_layer(id);
                    self.organize_views();
                } else {
                    unimplemented!()
                }
            }
            Command::LayerExtend(_id) => unimplemented!(),
            Command::Slice(None) => {
                let v = self.active_view_mut();
                v.slice(1);
            }
            Command::Slice(Some(nframes)) => {
                let v = self.active_view_mut();
                if !v.slice(nframes) {
                    self.message(
                        format!("Error: slice: view width is not divisible by {}", nframes),
                        MessageType::Error,
                    );
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
            Command::ChangeDir(dir) => {
                let home = self.base_dirs.home_dir().to_path_buf();
                let path = dir.map(|s| s.into()).unwrap_or(home);

                match std::env::set_current_dir(&path) {
                    Ok(()) => {
                        self.cwd = path.clone();
                        self.cmdline.set_cwd(path.as_path());
                    }
                    Err(e) => self.message(format!("Error: {}: {:?}", e, path), MessageType::Error),
                }
            }
            Command::Source(Some(ref path)) => {
                if let Err(ref e) = self.source_path(path) {
                    self.message(
                        format!("Error sourcing `{}`: {}", path, e),
                        MessageType::Error,
                    );
                }
            }
            Command::Source(None) => {
                self.message(
                    format!("Error: source command requires a path"),
                    MessageType::Error,
                );
            }
            Command::Edit(ref paths) => {
                if paths.is_empty() {
                    self.unimplemented();
                } else if let Err(e) = self.edit(paths) {
                    self.message(format!("Error loading path(s): {}", e), MessageType::Error);
                }
            }
            Command::EditFrames(ref paths) => {
                if !paths.is_empty() {
                    if let Err(e) = self.edit_frames(paths) {
                        self.message(
                            format!("Error loading frames(s): {}", e),
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
                if let Err(e) = self.save_view_as(self.views.active_id, Path::new(path).into()) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::WriteFrames(None) => {
                self.command(Command::WriteFrames(Some(".".to_owned())));
            }
            Command::WriteFrames(Some(ref dir)) => {
                let path = Path::new(dir);

                std::fs::create_dir_all(path).ok();

                let paths: Vec<_> = (0..self.active_view().animation.len())
                    .map(|i| path.join(format!("{:03}.png", i)))
                    .collect();
                let paths = NonEmpty::from_slice(paths.as_slice())
                    .expect("views always have at least one frame");

                if let Err(e) = self.save_view_as(self.views.active_id, FileStorage::Range(paths)) {
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
                    input,
                    press,
                    release,
                    modes,
                } = *map;

                self.key_bindings.add(KeyBinding {
                    input,
                    modes: modes.clone(),
                    command: press,
                    state: InputState::Pressed,
                    modifiers: platform::ModifiersState::default(),
                    is_toggle: release.is_some(),
                    display: Some(format!("{}", input)),
                });
                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        input,
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
                let extent = self.active_view().extent();

                if let Some(ref mut s) = self.selection {
                    s.translate(x, y);

                    let rect = s.bounds();
                    let v = self
                        .views
                        .active_mut()
                        .expect("there is always an active view");

                    if y > 0 && rect.max().y > extent.height() as i32 {
                        if v.activate_next_layer() {
                            *s = Selection::new(rect.x1, 0, rect.x2, rect.height());
                        }
                    } else if y < 0 && rect.min().y < 0 {
                        if v.activate_prev_layer() {
                            *s = Selection::new(
                                rect.x1,
                                extent.height() as i32 - rect.height(),
                                rect.x2,
                                extent.height() as i32,
                            );
                        }
                    }
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
                } else if let Some((id, _)) = self.hover_view {
                    if id == self.views.active_id {
                        let p = self.active_view_coords(self.cursor).map(|n| n as i32);
                        self.selection = Some(Selection::new(p.x, p.y, p.x + 1, p.y + 1));
                    }
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
                self.yank_selection();
            }
            Command::SelectionCut => {
                // To mimick the behavior of `vi`, we yank the selection
                // before deleting it.
                if self.yank_selection().is_some() {
                    self.command(Command::SelectionErase);
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
                    self.active_view_mut().touch_layer();
                }
            }
            Command::SelectionErase => {
                if let Some(s) = self.selection {
                    self.effects.extend_from_slice(&[
                        Effect::ViewBlendingChanged(Blending::Constant),
                        Effect::ViewPaintFinal(vec![Shape::Rectangle(
                            s.abs().bounds().map(|n| n as f32),
                            ZDepth::default(),
                            Rotation::ZERO,
                            Stroke::NONE,
                            Fill::Solid(Rgba8::TRANSPARENT.into()),
                        )]),
                    ]);
                    self.active_view_mut().touch_layer();
                }
            }
            Command::PaintColor(rgba, x, y) => {
                self.active_view_mut().paint_color(rgba, x, y);
            }
            Command::PaintForeground(x, y) => {
                let fg = self.fg;
                self.active_view_mut().paint_color(fg, x, y);
            }
            Command::PaintBackground(x, y) => {
                let bg = self.bg;
                self.active_view_mut().paint_color(bg, x, y);
            }
            Command::PaintPalette(i, x, y) => {
                let c = self.palette.colors.to_vec();
                let v = self.active_view_mut();

                if let Some(color) = c.get(i) {
                    v.paint_color(*color, x, y);
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

        match self.cmdline.parse(&input) {
            Err(e) => self.message(format!("Error: {}", e), MessageType::Error),
            Ok(cmd) => {
                self.command(cmd);
                self.cmdline.history.add(input);
            }
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
    pub fn color_at(&self, v: ViewId, l: LayerId, p: LayerCoords<u32>) -> Option<Rgba8> {
        let view = self.view(v);
        let resources = self.resources.lock();
        let (snapshot, pixels) = resources.get_snapshot(view.id, l);

        let y_offset = snapshot
            .height()
            .checked_sub(p.y)
            .and_then(|x| x.checked_sub(1));
        let index = y_offset.map(|y| (y * snapshot.width() + p.x) as usize);

        index.and_then(|idx| pixels.get(idx))
    }

    fn sample_color(&mut self) {
        if let Some(color) = self.hover_color {
            self.pick_color(color);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_key_bindings() {
        let mut kbs = KeyBindings::new();
        let state = InputState::Pressed;
        let modifiers = Default::default();

        let kb1 = KeyBinding {
            modes: vec![Mode::Normal],
            input: Input::Key(platform::Key::A),
            command: Command::Noop,
            is_toggle: false,
            display: None,
            modifiers,
            state,
        };
        let kb2 = KeyBinding {
            modes: vec![Mode::Command],
            ..kb1.clone()
        };

        kbs.add(kb1.clone());
        kbs.add(kb2.clone());

        assert_eq!(
            kbs.len(),
            2,
            "identical bindings for different modes can co-exist"
        );

        let kb3 = KeyBinding {
            command: Command::Quit,
            ..kb2.clone()
        };
        kbs.add(kb3.clone());

        assert_eq!(kbs.len(), 2, "bindings can be overwritten");
        assert_eq!(
            kbs.find(kb2.input, kb2.modifiers, kb2.state, kb2.modes[0]),
            Some(kb3),
            "bindings can be overwritten"
        );
    }

    #[test]
    fn test_key_bindings_modifier() {
        let kb = KeyBinding {
            modes: vec![Mode::Normal],
            input: Input::Key(platform::Key::Control),
            command: Command::Noop,
            is_toggle: false,
            display: None,
            modifiers: Default::default(),
            state: InputState::Pressed,
        };

        let mut kbs = KeyBindings::new();
        kbs.add(kb.clone());

        assert_eq!(
            kbs.find(
                Input::Key(platform::Key::Control),
                ModifiersState {
                    ctrl: true,
                    alt: false,
                    shift: false,
                    meta: false
                },
                InputState::Pressed,
                Mode::Normal
            ),
            Some(kb.clone())
        );

        assert_eq!(
            kbs.find(
                Input::Key(platform::Key::Control),
                ModifiersState::default(),
                InputState::Pressed,
                Mode::Normal
            ),
            Some(kb)
        );
    }
}
