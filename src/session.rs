///! Session
use crate::brush::*;
use crate::cmd;
use crate::cmd::{Command, CommandLine, Key, Op, Value};
use crate::hashmap;
use crate::palette::*;
use crate::platform;
use crate::platform::{InputState, KeyboardInput, ModifiersState, WindowEvent};
use crate::resources::ResourceManager;
use crate::view::{FileStatus, View, ViewCoords, ViewId};

use rgx::core::{PresentMode, Rect};
use rgx::kit::shape2d;
use rgx::kit::Rgba8;

use cgmath::prelude::*;
use cgmath::{Matrix4, Point2, Vector2};

use directories as dirs;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::ops::{Add, Deref, Sub};
use std::path::Path;
use std::str::FromStr;
use std::time;

pub const HELP: &'static str = r#"
rx: default key mappings and commands (<escape> to exit)

KEY MAPPINGS                                             COMMANDS

.                Zoom in view                            :help                    Toggle this help
,                Zoom out view                           :e <path..>              Edit path(s)
/                Reset view zoom                         :w [<path>]              Write view / Write view as <path>
j                Go to previous view                     :q                       Quit view
k                Go to next view                         :q!                      Force quit view
z                Center active view                      :echo <val>              Echo a value
u                Undo active view edit                   :echo "pixel!"           Echo the string "pixel!"
r                Redo active view edit                   :set <setting> = <val>   Set <setting> to <val>
x                Swap foreground/background colors       :set <setting>           Set <setting> to `on`
b                Reset brush                             :unset <setting>         Set <setting> to `off`
e                Erase (hold)                            :toggle <setting>        Toggle <setting> `on` / `off`
<shift>          Multi-brush (hold)                      :slice <n>               Slice view into <n> frames
]                Increase brush size                     :source <path>           Source an rx script (eg. a palette or config)
[                Decrease brush size                     :map <key> <command>     Map a key combination to a command
<ctrl>           Sample color (hold)                     :f/resize <w> <h>        Resize frames
<up>             Pan view up                             :f/add                   Add a blank frame to the view
<down>           Pan view down                           :f/remove                Remove the last frame of the view
<left>           Pan view left                           :f/clone <index>         Clone frame <index> and add it to the view
<right>          Pan view right                          :f/clone                 Clone the last frame and add it to the view
<return>         Add a frame to the view                 :p/clear                 Clear the palette
<backspace>      Remove a frame from the view            :p/add <color>           Add <color> to the palette, eg. #ff0011


SETTINGS

debug             on/off          Debug mode
checker           on/off          Alpha checker toggle
vsync             on/off          Vertical sync toggle
frame_delay       0.0..32.0       Delay between render frames (ms)
scale             1.0..4.0        UI scale
animation         on/off          View animation toggle
animation/delay   1..1000         View animation delay (ms)
"#;

/// A BGRA color, used when dealing with framebuffers.
#[repr(C)]
#[derive(Copy, Clone)]
struct Bgra8 {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
}

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
            MessageType::Info => Rgba8::new(200, 200, 200, 255),
            MessageType::Hint => Rgba8::new(100, 100, 100, 255),
            MessageType::Echo => Rgba8::new(190, 255, 230, 255),
            MessageType::Error => Rgba8::new(255, 50, 100, 255),
            MessageType::Warning => Rgba8::new(255, 255, 100, 255),
            MessageType::Replay => Rgba8::new(255, 255, 255, 160),
            MessageType::Okay => Rgba8::new(90, 255, 90, 255),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A session error.
type Error = String;

/// A key binding.
#[derive(PartialEq, Clone, Debug)]
struct KeyBinding {
    /// The `Mode`s this binding applies to.
    modes: Vec<Mode>,
    /// Modifiers which must be held.
    modifiers: ModifiersState,
    /// Key which must be pressed or released.
    key: Key,
    /// Whether the key should be pressed or released.
    state: InputState,
    /// The `Command` to run when this binding is triggered.
    command: Command,
    /// Whether this key binding controls a toggle.
    is_toggle: bool,
}

/// Manages a list of key bindings.
#[derive(Debug)]
struct KeyBindings {
    elems: Vec<KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        // The only default is switching to command mode. On some platforms,
        // Pressing `<shift> + ;` sends us a `:` directly, while on others
        // we get `<shift>` and `;`.
        KeyBindings {
            elems: vec![
                KeyBinding {
                    modes: vec![Mode::Normal],
                    modifiers: ModifiersState::default(),
                    key: Key::Virtual(platform::Key::Colon),
                    state: InputState::Pressed,
                    command: Command::Mode(Mode::Command),
                    is_toggle: false,
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
}

///////////////////////////////////////////////////////////////////////////////

/// A dictionary used to store session settings.
pub struct Settings {
    map: HashMap<String, Value>,
}

impl Settings {
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
                "frame_delay" => Value::Float(8.0),
                "scale" => Value::Float(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::U32(160)
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
    /// Whether the session is running or not.
    pub is_running: bool,
    /// The current session `Mode`.
    pub mode: Mode,

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

    /// The set of keys currently pressed.
    keys_pressed: HashSet<platform::Key>,
    /// The list of all active key bindings.
    key_bindings: KeyBindings,

    /// Set to `true` if animations are paused.
    pub paused: bool,
    /// Current pixel selection.
    pub selection: Rect<f32>,

    /// The session's current settings.
    pub settings: Settings,

    /// Views loaded in the session.
    pub views: BTreeMap<ViewId, View>,
    /// Currently active view.
    pub active_view_id: ViewId,

    /// The next `ViewId`.
    next_view_id: ViewId,
    /// A last-recently-used list of views.
    views_lru: VecDeque<ViewId>,

    /// The current state of the command line.
    pub cmdline: CommandLine,
    /// The color palette.
    pub palette: Palette,

    /// Set to `true` if a view was added or removed from the session.
    pub dirty: bool,

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
    pub const MAX_VIEWS: usize = 64;
    pub const DEFAULT_VIEW_W: u32 = 128;
    pub const DEFAULT_VIEW_H: u32 = 128;

    const SUPPORTED_FORMATS: &'static [&'static str] = &["png", "gif"];
    const VIEW_MARGIN: f32 = 24.;
    const PALETTE_CELL_SIZE: f32 = 24.;
    const PAN_PIXELS: i32 = 32;
    const MIN_BRUSH_SIZE: usize = 1;
    const MAX_LRU: usize = 16;
    const MAX_ZOOM: f32 = 128.0;
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
    const THROTTLE_TIME: time::Duration = time::Duration::from_millis(96);

    /// Initial (default) configuration for rx.
    const CONFIG: &'static [u8] = include_bytes!("../config/init.rx");

    /// Create a new un-initialized session.
    pub fn new(
        w: u32,
        h: u32,
        hidpi_factor: f64,
        resources: ResourceManager,
        base_dirs: dirs::ProjectDirs,
    ) -> Self {
        Self {
            is_running: false,
            width: w as f32,
            height: h as f32,
            hidpi_factor,
            cursor: SessionCoords::new(0., 0.),
            base_dirs,
            offset: Vector2::zero(),
            tool: Tool::Brush(Brush::default()),
            prev_tool: None,
            mouse_down: false,
            _mouse_selection: Rect::new(0., 0., 0., 0.),
            _frame_count: 0,
            paused: false,
            _onion: false,
            hover_color: None,
            hover_view: None,
            fg: Rgba8::WHITE,
            bg: Rgba8::BLACK,
            settings: Settings::default(),
            palette: Palette::new(Self::PALETTE_CELL_SIZE),
            key_bindings: KeyBindings::default(),
            keys_pressed: HashSet::new(),
            views: BTreeMap::new(),
            views_lru: VecDeque::new(),
            cmdline: CommandLine::new(),
            _grid_w: 0,
            _grid_h: 0,
            throttled: None,
            mode: Mode::Normal,
            selection: Rect::new(0., 0., 0., 0.),
            message: Message::default(),
            _paste: (),
            _recording: false,
            _recording_opts: 0,
            active_view_id: ViewId::default(),
            next_view_id: ViewId(1),
            resources,
            dirty: true,
        }
    }

    /// Initialize a session.
    pub fn init(mut self) -> std::io::Result<Self> {
        self.is_running = true;

        let cwd = std::env::current_dir()?;
        let cfg = self.base_dirs.config_dir().join("init.rx");

        if cfg.exists() {
            self.source_path(cfg)?;
        } else {
            self.source_reader(io::BufReader::new(Self::CONFIG), "<init>")?;
        }
        self.source_dir(cwd).ok();

        Ok(self)
    }

    /// Create a blank view.
    pub fn blank(&mut self, fs: FileStatus, w: u32, h: u32) {
        let id = self.gen_view_id();

        self.add_view(View::new(id, fs, w, h));
        self.edit_view(id);

        self.resources.add_blank_view(&id, w, h);
    }

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

    pub fn active_view_coords(&self, p: SessionCoords) -> ViewCoords<f32> {
        self.view_coords(self.active_view_id, p)
    }

    pub fn frame(
        &mut self,
        events: &mut Vec<platform::WindowEvent>,
        out: &mut shape2d::Batch,
        delta: time::Duration,
    ) {
        self.dirty = false;

        for (_, v) in &mut self.views {
            v.okay();

            if self.settings["animation"].is_set() {
                v.frame(delta);
            }
        }

        for event in events.drain(..) {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let scale: f64 = self.settings["scale"].float64();
                    self.handle_cursor_moved(
                        SessionCoords::new(
                            (position.x / scale).floor() as f32,
                            self.height
                                - (position.y / scale).floor() as f32
                                - 1.,
                        ),
                        out,
                    );
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    self.handle_mouse_input(button, state, out);
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
        assert_eq!(self.offset, self.offset.map(|a| a.floor()));
    }

    pub fn transform(&self) -> Matrix4<f32> {
        Matrix4::from_translation(self.offset.extend(0.))
    }

    pub fn view(&self, id: ViewId) -> &View {
        self.views
            .get(&id)
            .expect(&format!("view #{} must exist", id))
    }

    pub fn view_mut(&mut self, id: ViewId) -> &mut View {
        self.views
            .get_mut(&id)
            .expect(&format!("view #{} must exist", id))
    }

    pub fn active_view(&self) -> &View {
        assert!(self.active_view_id != ViewId(0), "fatal: no active view");
        self.view(self.active_view_id)
    }

    pub fn active_view_mut(&mut self) -> &mut View {
        assert!(self.active_view_id != ViewId(0), "fatal: no active view");
        self.view_mut(self.active_view_id)
    }

    pub fn edit<P: AsRef<Path>>(&mut self, paths: &[P]) -> io::Result<()> {
        // TODO: Keep loading paths even if some fail?
        for path in paths {
            let path = path.as_ref();

            if let Some(View { id, .. }) = self
                .views
                .values()
                .find(|v| v.file_name().map_or(false, |f| f == path))
            {
                // TODO: Reload from disk.
                let id = *id;
                self.activate_view(id);
            } else if path.is_dir() {
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
                self.blank(FileStatus::New(path.into()), w, h);
            }
        }
        Ok(())
    }

    fn source_path<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        debug!("source: {}", path.display());

        let f = File::open(&path)
            .or_else(|_| File::open(self.base_dirs.config_dir().join(path)))?;

        self.source_reader(io::BufReader::new(f), path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

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

        let id = self.gen_view_id();
        let (width, height) = self.resources.load_view(&id, &path)?;
        let view = View::new(
            id,
            FileStatus::Saved(path.into()),
            width as u32,
            height as u32,
        );

        self.add_view(view);
        self.edit_view(id);

        self.message(
            format!("\"{}\" {} pixels read", path.display(), width * height),
            MessageType::Info,
        );

        Ok(())
    }

    fn destroy_view(&mut self, id: ViewId) {
        assert!(!self.views.is_empty());
        assert!(!self.views_lru.is_empty());

        self.views.remove(&id);
        self.views_lru.retain(|&v| v != id);
        self.resources.remove_view(&id);

        self.dirty = true;
    }

    fn quit_view(&mut self, id: ViewId) {
        self.destroy_view(id);

        if !self.views.is_empty() {
            let lru =
                *self.views_lru.front().expect("fatal: view cache is empty!");

            self.organize_views();
            self.activate_view(lru);
            self.center_active_view();
        }
    }

    pub fn quit(&mut self) {
        self.is_running = false;
    }

    pub fn save_view(&mut self, id: ViewId) -> io::Result<()> {
        // FIXME: We shouldn't need to clone here.
        if let Some(ref f) = self.view(id).file_name().map(|f| f.clone()) {
            self.save_view_as(id, f)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "no file name given"))
        }
    }

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

    fn setting_changed(&mut self, name: &str, old: &Value, new: &Value) {
        debug!("set `{}`: {} -> {}", name, old, new);

        match name {
            "animation/delay" => {
                self.active_view_mut().set_animation_delay(new.uint64());
            }
            _ => {}
        }
    }

    fn center_palette(&mut self) {
        let n = usize::min(self.palette.size(), 16) as f32;
        let p = &mut self.palette;

        p.x = 0.;
        p.y = self.height / 2. - n * p.cellsize / 2.;
    }

    fn center_active_view_v(&mut self) {
        let v = self.active_view();
        self.offset.y =
            (self.height / 2. - v.height() as f32 / 2. * v.zoom - v.offset.y)
                .floor();
    }

    fn center_active_view_h(&mut self) {
        let v = self.active_view();
        self.offset.x =
            (self.width / 2. - v.width() as f32 * v.zoom / 2. - v.offset.x)
                .floor();
    }

    fn center_active_view(&mut self) {
        self.center_active_view_v();
        self.center_active_view_h();
    }

    pub fn activate_view(&mut self, id: ViewId) {
        if self.active_view_id == id {
            return;
        }
        assert!(
            self.views.contains_key(&id),
            "the view being activated exists"
        );
        self.active_view_id = id;
        self.views_lru.push_front(id);
        self.views_lru.truncate(Self::MAX_LRU);
    }

    fn edit_view(&mut self, id: ViewId) {
        self.activate_view(id);
        self.center_active_view();
    }

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
    }

    fn gen_view_id(&mut self) -> ViewId {
        let ViewId(id) = self.next_view_id;
        self.next_view_id = ViewId(id + 1);

        ViewId(id)
    }

    fn add_view(&mut self, v: View) {
        let id = v.id;

        if self.views.is_empty() {
            self.views.insert(id, v);
            self.activate_view(id);
            self.center_active_view();
        } else {
            // FIXME: Handle case where there is no active view.
            if self.active_view().file_status == FileStatus::NoFile {
                self.destroy_view(self.active_view_id);
            }
            self.views.insert(id, v);
            self.organize_views();
        }
        self.dirty = true;
    }

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

    pub fn message<D: fmt::Display>(&mut self, msg: D, t: MessageType) {
        self.message = Message::new(msg, t);
        self.message.log();
    }

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

        let pressed: Vec<platform::Key> = self.keys_pressed.drain().collect();
        for k in pressed {
            self.handle_keyboard_input(platform::KeyboardInput {
                key: Some(k),
                modifiers: ModifiersState::default(),
                state: InputState::Released,
                repeat: false,
            });
        }

        self.mode = new;
    }

    fn command(&mut self, cmd: Command) {
        // Certain commands cause problems when run many times within
        // a short time frame. This might be because the GPU hasn't had
        // time to fully process the commands sent to it by the time we
        // execute the next command. We throttle them here to have a
        // minimum time between them.
        if self.throttle(&cmd) {
            return;
        }
        self.message_clear();

        debug!("command: {:?}", cmd);

        return match cmd {
            Command::Mode(m) => {
                self.switch_mode(m);
            }
            Command::Quit => match &self.active_view().file_status {
                FileStatus::Modified(_) | FileStatus::New(_) => {
                    self.message(
                        "Error: no write since last change",
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
                self.tool = Tool::Brush(Brush::default());
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
                v.resize_frame(fw, fh);
                v.touch();
            }
            Command::ForceQuit => self.quit_view(self.active_view_id),
            Command::Echo(ref v) => {
                let result = match v {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Ident(s) => match s.as_str() {
                        "s:config:dir" => Ok(Value::Str(format!(
                            "{}",
                            self.base_dirs.config_dir().display()
                        ))),
                        "s:hidpi" => {
                            Ok(Value::Str(format!("{:.1}", self.hidpi_factor)))
                        }
                        "s:offset" => Ok(Value::Vector2(self.offset)),
                        "v:offset" => {
                            Ok(Value::Vector2(self.active_view().offset))
                        }
                        "v:zoom" => {
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
            Command::Pan(x, y) => {
                self.offset.x -= (x * Self::PAN_PIXELS) as f32;
                self.offset.y -= (y * Self::PAN_PIXELS) as f32;
            }
            Command::ViewNext => {
                let id = self.active_view_id;
                if let Some(id) =
                    self.views.range(id..).nth(1).map(|(id, _)| *id)
                {
                    self.activate_view(id);
                    self.center_active_view_v();
                }
            }
            Command::ViewPrev => {
                let id = self.active_view_id;
                if let Some(id) =
                    self.views.range(..id).next_back().map(|(id, _)| *id)
                {
                    self.activate_view(id);
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
                self.active_view_mut().extend_clone(n);
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
                if let Err(e) = self.save_view(self.active_view_id) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::Write(Some(ref path)) => {
                if let Err(e) = self.save_view_as(self.active_view_id, path) {
                    self.message(format!("Error: {}", e), MessageType::Error);
                }
            }
            Command::WriteQuit => {
                if self.save_view(self.active_view_id).is_ok() {
                    self.quit_view(self.active_view_id);
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
                });
                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        key,
                        modes: vec![Mode::Normal],
                        command: cmd,
                        state: InputState::Released,
                        modifiers: platform::ModifiersState::default(),
                        is_toggle: true,
                    });
                }
            }
            Command::Undo => {
                self.undo(self.active_view_id);
            }
            Command::Redo => {
                self.redo(self.active_view_id);
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

    fn message_clear(&mut self) {
        self.message = Message::default();
    }

    fn undo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, true);
    }

    fn redo(&mut self, id: ViewId) {
        self.restore_view_snapshot(id, false);
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

    fn unimplemented(&mut self) {
        self.message("Error: not yet implemented", MessageType::Error);
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

            v.resize(fw, fh, nframes);
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

    fn color_at(&self, v: ViewId, p: ViewCoords<u32>) -> Rgba8 {
        let resources = self.resources.lock();
        let snapshot = resources.get_snapshot(&v);

        assert!(snapshot.pixels.len() % std::mem::size_of::<Bgra8>() == 0);

        let (head, pixels, tail) =
            unsafe { snapshot.pixels.align_to::<Bgra8>() };

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

    pub fn record_macro(&mut self, _cmd: String) {}

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
        out: &mut shape2d::Batch,
    ) {
        if button != platform::MouseButton::Left {
            return;
        }
        let is_cursor_active = self.active_view().hover;

        if state == platform::InputState::Pressed {
            self.mouse_down = true;
            self.record_macro(format!("cursor/down"));

            // Click on palette
            if let Some(color) = self.palette.hover {
                if self.mode == Mode::Command {
                    self.cmdline.puts(&color.to_string());
                } else {
                    self.pick_color(color);
                }
                return;
            }

            // Click on active view
            if is_cursor_active {
                if self.mode == Mode::Command {
                    self.cmdline_hide();
                    return;
                }

                let p = self.active_view_coords(self.cursor);
                let (nframes, fw, frame_index) = {
                    let v = self.active_view();
                    (v.animation.len(), v.fw, (p.x as u32 / v.fw) as i32)
                };

                match self.mode {
                    Mode::Normal => match self.tool {
                        // TODO: This whole block of code is duplicated in
                        // `handle_cursor_moved`.
                        Tool::Brush(ref mut brush) => {
                            let color = if brush.is_set(BrushMode::Erase) {
                                Rgba8::TRANSPARENT
                            } else {
                                self.fg
                            };

                            let offsets: Vec<_> = if brush
                                .is_set(BrushMode::Multi)
                            {
                                (0..nframes as i32 - frame_index)
                                    .map(|i| {
                                        Vector2::new((i as u32 * fw) as i32, 0)
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            brush.start_drawing(p.into(), color, &offsets, out);
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
                for (id, v) in &self.views {
                    if v.hover {
                        let id = id.clone();
                        self.activate_view(id);
                        self.center_active_view_v();
                        return;
                    }
                }
            }
        } else if state == platform::InputState::Released {
            self.mouse_down = false;
            self.record_macro(format!("cursor/up"));

            if let Tool::Brush(ref mut brush) = self.tool {
                match brush.state {
                    BrushState::Drawing | BrushState::DrawStarted => {
                        brush.stop_drawing();
                        self.active_view_mut().touch();
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_cursor_moved(
        &mut self,
        cursor: SessionCoords,
        out: &mut shape2d::Batch,
    ) {
        self.record_macro(format!("cursor/move {} {}", cursor.x, cursor.y));
        self.palette.handle_cursor_moved(cursor);

        self.hover_view = None;
        self.hover_color = None;
        for (_, v) in &mut self.views {
            v.handle_cursor_moved(cursor - self.offset);
            if v.hover {
                self.hover_view = Some(v.id);
            }
        }

        let p = self.active_view_coords(cursor);
        let (nframes, fw, frame_index, vw, vh) = {
            let v = self.active_view();
            (
                v.animation.len(),
                v.fw,
                i32::max(0, (p.x / v.fw as f32) as i32),
                v.width(),
                v.height(),
            )
        };
        if let Some(color) = self.palette.hover {
            self.hover_color = Some(color);
        } else if let Some(v) = self.hover_view {
            let p: ViewCoords<u32> = self.view_coords(v, cursor).into();
            self.hover_color = Some(self.color_at(v, p));
        }

        match self.mode {
            Mode::Normal => match self.tool {
                Tool::Brush(ref mut brush) => {
                    if brush.state == BrushState::DrawStarted
                        || brush.state == BrushState::Drawing
                    {
                        brush.state = BrushState::Drawing;

                        let color = if brush.is_set(BrushMode::Erase) {
                            Rgba8::TRANSPARENT
                        } else {
                            self.fg
                        };

                        let mut p: ViewCoords<i32> = p.into();

                        if brush.is_set(BrushMode::Multi) {
                            p.clamp(Rect::new(
                                (brush.size / 2) as i32,
                                (brush.size / 2) as i32,
                                vw as i32 - (brush.size / 2) as i32 - 1,
                                vh as i32 - (brush.size / 2) as i32 - 1,
                            ));
                            let offsets: Vec<_> = (0..nframes as i32
                                - frame_index)
                                .map(|i| {
                                    Vector2::new((i as u32 * fw) as i32, 0)
                                })
                                .collect();

                            brush.tick(p, color, &offsets, out);
                        } else {
                            brush.tick(p, color, &[], out);
                        }
                    }
                }
                Tool::Pan => {
                    self.offset.x += cursor.x - self.cursor.x;
                    self.offset.y += cursor.y - self.cursor.y;
                }
                Tool::Sampler => {}
            },
            Mode::Visual => {}
            _ => {}
        }
        self.cursor = cursor;
    }

    pub fn handle_received_character(&mut self, c: char) {
        if self.mode == Mode::Command {
            if c.is_control() {
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
            repeat,
        } = input;

        if let Some(key) = key {
            // While the mouse is down, don't accept keyboard input.
            if self.mouse_down {
                return;
            }

            if state == InputState::Pressed {
                self.keys_pressed.insert(key);
            } else if state == InputState::Released {
                self.keys_pressed.remove(&key);
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
                        }
                    }
                    return;
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
}
