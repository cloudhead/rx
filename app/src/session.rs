use std::fmt;
use std::fs::File;
use std::io;
use std::iter;
use std::mem;
use std::path::{Path, PathBuf};
use std::time;

use directories as dirs;

use crate::app::brush::Brush;
use crate::app::command::{Command, KeyMapping, Op};
use crate::app::command_line::MessageType;
use crate::app::keyboard;
use crate::app::keyboard::{Input, KeyBinding, KeyBindings};
use crate::app::script;
use crate::app::view;
use crate::app::view::ViewId;
use crate::app::DEFAULT_CONFIG;
use crate::app::{CommandLine, Palette, Settings};
use crate::framework::platform::{InputState, Key, ModifiersState, MouseButton};
use crate::gfx::color;
use crate::gfx::prelude::*;

/// Maximum zoom amount as a multiplier.
const MAX_ZOOM: f32 = 128.0;
/// Maximum frame width or height.
const MAX_FRAME_SIZE: u32 = 4096;
/// Zoom levels used when zooming in/out.
const ZOOM_LEVELS: &[f32] = &[
    1., 2., 3., 4., 6., 8., 10., 12., 16., 20., 24., 32., 64., MAX_ZOOM,
];

pub struct Session {
    /// The current session mode.
    pub mode: Mode,
    /// The previous mode.
    pub prev_mode: Option<Mode>,
    /// The last cursor coordinates.
    pub cursor: Point,
    /// Session workspace zoom level.
    pub zoom: f32,
    /// The command-line state.
    pub cmdline: CommandLine,
    /// The color palette.
    pub palette: Palette,
    /// The session's current settings.
    pub settings: Settings,
    /// Views loaded in the session.
    pub views: view::Manager,
    /// Pixel selection on the active view.
    pub selection: Option<Selection>,
    /// The current tool. Only used in `Normal` mode.
    pub tool: Tool,
    /// The previous tool, if any.
    pub prev_tool: Option<Tool>,
    /// The brush tool settings.
    pub brush: Brush,
    /// Session colors.
    pub colors: Colors,
    /// Key bindings.
    pub key_bindings: KeyBindings,
    /// Current directory.
    pub current_dir: PathBuf,
    /// Project directories.
    pub proj_dirs: dirs::ProjectDirs,
    /// Base directories.
    pub base_dirs: dirs::BaseDirs,
}

impl Session {
    pub fn new<P: AsRef<Path>>(
        current_dir: P,
        proj_dirs: dirs::ProjectDirs,
        base_dirs: dirs::BaseDirs,
    ) -> Self {
        let current_dir = current_dir.as_ref().to_path_buf();
        let history_path = proj_dirs.data_dir().join("history");

        Self {
            mode: Mode::default(),
            prev_mode: None,
            cursor: Point::ZERO,
            zoom: 1.,
            cmdline: CommandLine::new(current_dir.clone(), history_path, &color::FILE_EXTENSION),
            palette: Palette::default(),
            settings: Settings::default(),
            views: view::Manager::default(),
            selection: None,
            tool: Tool::default(),
            prev_tool: None,
            brush: Brush::default(),
            colors: Colors::default(),
            key_bindings: KeyBindings::default(),
            current_dir,
            proj_dirs,
            base_dirs,
        }
    }

    /// Initialize a session.
    pub fn init(&mut self) -> Result<(), Error> {
        // self.transition(State::Running);
        // self.reset()?;
        self.source_reader(io::BufReader::new(DEFAULT_CONFIG))
    }

    // if let Some(init) = source {
    //     let init = init.as_ref();

    //     // The special source '-' is used to skip initialization.
    //     if init.as_os_str() != "-" {
    //         self.source_path(&init)?;
    //     }
    // } else {
    //     let dir = self.proj_dirs.config_dir().to_owned();
    //     let cfg = dir.join(INIT);

    //     if cfg.exists() {
    //         self.source_path(cfg)?;
    //     }
    // }

    // self.source_path(&self.current_dir.join(".rxrc")).ok();
    // self.cmd_line.history.load()?;
    // // self.message(format!("rx v{}", crate::VERSION), MessageType::Debug);

    // Ok(self)
    // }

    /// Edit paths.
    ///
    /// Loads the given files into the session. Returns an error if one of
    /// the paths couldn't be loaded. If a path points to a directory,
    /// loads all files within that directory.
    pub fn edit<'a, P: AsRef<Path> + 'a>(
        &mut self,
        paths: impl Iterator<Item = &'a P>,
    ) -> io::Result<Vec<Result<usize, view::Error>>> {
        let mut results = Vec::new();

        for path in paths {
            let path = path.as_ref();

            if path.is_dir() {
                for entry in path.read_dir()? {
                    let entry = entry?;
                    let path = entry.path();

                    if path.is_dir() {
                        results.extend(self.edit(iter::once(&path))?);
                        continue;
                    }
                    if path.extension() != Some(color::FILE_EXTENSION.as_ref()) {
                        continue;
                    }
                    results.push(self.views.open(path));
                }
                self.source_dir(path).ok();
            } else if !path.exists() && path.with_extension(color::FILE_EXTENSION).exists() {
                results.push(self.views.open(path.with_extension(color::FILE_EXTENSION)));
            } else {
                results.push(self.views.open(path));
            }
        }

        Ok(results)
    }

    /// Quit view if it has been saved. Otherwise, display an error.
    fn quit_view(&mut self, id: ViewId) -> Result<(), Error> {
        let v = self.views.get(&id).unwrap();
        if v.modified {
            return Err(Error::QuitWithoutSave);
        } else {
            self.views.remove(&id);
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Sourcing
    ///////////////////////////////////////////////////////////////////////////

    /// Source an rx script at the given path. Returns an error if the path
    /// does not exist or the script couldn't be sourced.
    fn source_path<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        let path = path.as_ref();

        debug!("source: {}", path.display());

        File::open(&path)
            .or_else(|_| File::open(self.proj_dirs.config_dir().join(path)))
            .map_err(|e| Error::Source(path.to_path_buf(), Box::new(e)))
            .and_then(|f| self.source_reader(io::BufReader::new(f)))
            .map_err(|e| Error::Source(path.to_path_buf(), Box::new(e)))
    }

    /// Source a directory which contains a `.rxrc` script. Returns an
    /// error if the script wasn't found or couldn't be sourced.
    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<(), Error> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

    /// Source a script from an [`io::BufRead`].
    fn source_reader(&mut self, r: impl io::BufRead) -> Result<(), Error> {
        for (i, line) in r.lines().enumerate() {
            let line = line?;

            if line.starts_with(script::COMMENT) {
                continue;
            }
            match self.cmdline.parse(&line) {
                Err(e) => {
                    return Err(Error::Parse(e, i + 1));
                }
                Ok(cmd) => self.command(cmd)?,
            }
        }
        Ok(())
    }

    ////////////////////////////////////////////////////////////////////////////
    // Modes
    ////////////////////////////////////////////////////////////////////////////

    /// Toggle the session mode.
    pub fn mode(&mut self, mode: Mode) {
        if self.mode == mode {
            self.set_mode(Mode::Normal);
        } else {
            self.set_mode(mode);
        }
    }

    /// Set the session to the previous mode.
    pub fn prev_mode(&mut self) {
        self.set_mode(self.prev_mode.unwrap_or(Mode::Normal));
    }

    /// Set the session mode.
    pub fn set_mode(&mut self, mode: Mode) {
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
                // self.selection = None;
            }
            Mode::Command => {
                // When switching to command mode via the keyboard, we simultaneously
                // also receive the character input equivalent of the key pressed.
                // This input, since we are now in command mode, is processed as
                // text input to the command line. To avoid this, we have to ignore
                // all such input until the end of the current upate.
                // self.ignore_received_characters = true;
                // self.cmdline_handle_input(':');
            }
            _ => {}
        }

        // self.release_inputs();
        self.prev_mode = Some(self.mode);
        self.mode = new;
    }

    /// Process a command.
    pub fn command(&mut self, cmd: Command) -> Result<(), Error> {
        debug!("command: {:?}", cmd);

        match cmd {
            Command::Edit(ref paths) => match self.edit(paths.iter()) {
                Ok(results) => {
                    let (ok, mut err): (Vec<_>, Vec<_>) =
                        results.into_iter().partition(|r| r.is_ok());
                    if !ok.is_empty() {
                        if err.is_empty() {
                            self.cmdline
                                .message(format!("{} path(s) loaded", ok.len()), MessageType::Info)
                        } else {
                            self.cmdline.message(
                                format!(
                                    "{} path(s) loaded, {} path(s) skipped",
                                    ok.len(),
                                    paths.len() - ok.len()
                                ),
                                MessageType::Info,
                            )
                        }
                    } else if let Some(Err(err)) = err.pop() {
                        return Err(Error::View(err));
                    }
                }
                Err(e) => self
                    .cmdline
                    .message(format!("Error loading path(s): {}", e), MessageType::Error),
            },
            Command::Mode(m) => {
                self.mode(m);
            }
            Command::Quit => {
                self.quit_view(self.views.active)?;
            }
            Command::Tool(t) => {
                self.tool(t);
            }
            Command::ToolPrev => {
                self.prev_tool();
            }
            Command::Undo => {
                self.views.active_mut().map(view::View::undo);
            }
            Command::Redo => {
                self.views.active_mut().map(view::View::redo);
            }
            Command::Write(None) => {
                if let Some(v) = self.views.active_mut() {
                    if let Some(path) = v.path.clone() {
                        self.command(Command::Write(Some(path)))?;
                    } else {
                        return Err(Error::NoFileName);
                    }
                } else {
                    return Err(Error::NoActiveView);
                }
            }
            Command::Write(Some(ref path)) => {
                if let Some(v) = self.views.active_mut() {
                    match v.save_as(path.as_path()) {
                        Ok(n) => {
                            self.cmdline
                                .info(format!("{:?} {} pixels written", path.display(), n));
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                } else {
                    return Err(Error::NoActiveView);
                }
            }
            Command::WriteQuit => {}
            Command::Zoom(op) => match op {
                Op::Incr => {
                    self.zoom_in();
                }
                Op::Decr => {
                    self.zoom_out();
                }
                Op::Set(z) => {
                    if !(1. ..=MAX_ZOOM).contains(&z) && z.fract() == 0. {
                        self.cmdline
                            .message("Error: invalid zoom level", MessageType::Error);
                    } else {
                        self.zoom = z;
                    }
                }
            },
            Command::FrameResize { size } => {
                if size.w < 1 || size.h < 1 || size.w > MAX_FRAME_SIZE || size.h > MAX_FRAME_SIZE {
                    return Err(Error::InvalidFrameSize {
                        min: 1,
                        max: MAX_FRAME_SIZE,
                    });
                }
                if let Some(v) = self.views.active_mut() {
                    v.resize(size)?;
                }

                // TODO: Invalidate selection
            }
            Command::PaletteAdd(color) => {
                self.palette.add(color);
            }
            Command::BrushMode(m) => {
                self.brush.mode(m);
            }
            Command::BrushSet(m) => {
                self.brush.set(m);
            }
            Command::BrushUnset(m) => {
                self.brush.unset(m);
            }
            Command::BrushToggle(m) => {
                self.brush.toggle(m);
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
                    modifiers: ModifiersState::default(),
                    is_toggle: release.is_some(),
                    display: Some(format!("{}", input)),
                });

                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        input,
                        modes,
                        command: cmd,
                        state: InputState::Released,
                        modifiers: ModifiersState::default(),
                        is_toggle: true,
                        display: None,
                    });
                }
            }
            _ => {}
        };

        Ok(())
    }

    /// Zoom the workspace in.
    fn zoom_in(&mut self) {
        let lvls = ZOOM_LEVELS;

        // Find the next zoom value greater than the current one.
        for (i, zoom) in lvls.iter().enumerate() {
            if self.zoom <= *zoom {
                if let Some(z) = lvls.get(i + 1) {
                    self.zoom = *z;
                } else {
                    self.cmdline
                        .message("Maximum zoom level reached", MessageType::Hint);
                }
                return;
            }
        }
    }

    /// Zoom the workspace out.
    fn zoom_out(&mut self) {
        let lvls = ZOOM_LEVELS;

        // Find the next zoom value smaller than the current one.
        for (i, zoom) in lvls.iter().enumerate() {
            if self.zoom <= *zoom {
                if i == 0 {
                    self.cmdline
                        .message("Minimum zoom level reached", MessageType::Hint);
                } else if let Some(z) = lvls.get(i - 1) {
                    self.zoom = *z;
                }
                return;
            }
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Tool functions
    ///////////////////////////////////////////////////////////////////////////

    pub fn tool(&mut self, t: Tool) {
        if mem::discriminant(&t) != mem::discriminant(&self.tool) {
            self.prev_tool = Some(self.tool);
        }
        self.tool = t;
    }

    pub fn prev_tool(&mut self) {
        self.tool = self.prev_tool.unwrap_or_default();
    }

    pub fn handle_cursor_moved(&mut self, cursor: Point) {
        self.cursor = cursor;
    }

    pub fn handle_mouse_up(&mut self, _button: MouseButton) {}
    pub fn handle_mouse_down(&mut self, _button: MouseButton) {}

    pub fn handle_resize(&mut self, _size: Size) {}

    pub fn handle_received_character(&mut self, c: char, mods: ModifiersState) {
        if self.mode == Mode::Command {
            if c.is_control() {
                return;
            }
            self.cmdline.putc(c);
        } else if let Some(kb) = self.key_bindings.find(
            keyboard::Input::Character(c),
            mods,
            InputState::Pressed,
            self.mode,
        ) {
            if let Err(e) = self.command(kb.command) {
                self.cmdline.error(e);
            }
        }
    }

    pub fn handle_key_down(&mut self, key: Key, modifiers: ModifiersState, repeat: bool) {
        if let Some(kb) =
            self.key_bindings
                .find(Input::Key(key), modifiers, InputState::Pressed, self.mode)
        {
            // For toggle-like key bindings, we don't want to run the command
            // on key repeats. For regular key bindings, we run the command
            // depending on if it's supposed to repeat.
            if !repeat || kb.command.repeats() && !kb.is_toggle {
                if let Err(e) = self.command(kb.command) {
                    self.cmdline.error(e);
                }
            }
        }
    }

    pub fn handle_key_up(&mut self, key: Key, modifiers: ModifiersState) {
        if let Some(kb) =
            self.key_bindings
                .find(Input::Key(key), modifiers, InputState::Released, self.mode)
        {
            if let Err(e) = self.command(kb.command) {
                self.cmdline.error(e);
            }
        }
    }

    pub fn update(&mut self, _delta: time::Duration) {
        if let Tool::Brush = self.tool {
            // self.brush.update();
        }
    }
}

/// An editing tool.
#[derive(PartialEq, Eq, Debug, Copy, Clone, Default)]
pub enum Tool {
    /// The standard drawing tool.
    #[default]
    Brush,
    /// Used for filling enclosed regions with color.
    Bucket,
    /// Used to sample colors.
    Sampler,
    /// Used to pan the workspace.
    Pan {
        /// Whether or not we are currently panning.
        panning: bool,
    },
}

/// An editing mode the `Session` can be in.
/// Some of these modes are inspired by vi.
#[derive(Eq, PartialEq, Copy, Clone, Debug, Default)]
pub enum Mode {
    /// Allows the user to paint pixels.
    #[default]
    Normal,
    /// Allows pixels to be selected, copied and manipulated visually.
    Visual(VisualState),
    /// Allows commands to be run.
    Command,
    /// Activated with the `:help` command.
    Help,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => "normal".fmt(f),
            Self::Visual(VisualState::Selecting { dragging: true }) => "visual (dragging)".fmt(f),
            Self::Visual(VisualState::Selecting { .. }) => "visual".fmt(f),
            Self::Visual(VisualState::Pasting) => "visual (pasting)".fmt(f),
            Self::Command => "command".fmt(f),
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

/// A pixel selection within a view.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct Selection {
    pub origin: Point2D<i32>,
    pub cursor: Point2D<i32>,
}

impl Selection {
    pub fn new(origin: impl Into<Point2D<i32>>, cursor: impl Into<Point2D<i32>>) -> Self {
        Self {
            origin: origin.into(),
            cursor: cursor.into(),
        }
    }

    /// Create a new selection from a rectangle.
    pub fn from(r: Rect<i32>) -> Self {
        Self::new(r.origin, r.origin + r.size)
    }

    /// Return the selection bounds as a non-empty rectangle. This function
    /// will never return an empty rectangle.
    pub fn bounds(&self) -> Rect<i32> {
        let min = Point2D::<i32>::new(
            self.origin.x.min(self.cursor.x),
            self.origin.y.min(self.cursor.y),
        );
        let max = Point2D::<i32>::new(
            self.origin.x.max(self.cursor.x),
            self.origin.y.max(self.cursor.y),
        );

        Rect::new(min, Size::new(max.x - min.x, max.y - min.y))
    }

    /// Translate the selection rectangle.
    pub fn translate(&mut self, x: i32, y: i32) {
        self.origin = self.origin + Vector2D::new(x, y);
        self.cursor = self.cursor + Vector2D::new(x, y);
    }

    /// Resize the selection by setting a new cursor.
    pub fn resize(&mut self, cursor: impl Into<Point2D<i32>>) {
        let cursor = cursor.into();
        if cursor.x == self.origin.x || cursor.y == self.origin.y {
            return;
        }
        self.cursor = cursor;
    }

    /// Expand the selection by a certain amount.
    pub fn expand(&mut self, x: i32, y: i32) -> Self {
        let bounds = self.bounds();
        let rect = bounds.expand(x, y);

        Self::from(rect)
    }
}

pub struct Colors {
    /// The session foreground color.
    pub fg: Rgba8,
    /// The session background color.
    pub bg: Rgba8,
    /// Color under the cursor.
    pub hover: Option<Rgba8>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            fg: Rgba8::WHITE,
            bg: Rgba8::BLACK,
            hover: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No write since last change (enter `:q!` to quit without saving)")]
    QuitWithoutSave,
    #[error("No file name")]
    NoFileName,
    #[error("No active view")]
    NoActiveView,
    #[error(transparent)]
    View(#[from] view::Error),
    #[error("Frame dimensions must be between {min} and {max}")]
    InvalidFrameSize { min: u32, max: u32 },
    #[error("{0} on line {1}")]
    Parse(memoir::result::Error, usize),
    #[error("error sourcing {0}: {1}")]
    Source(PathBuf, Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Io(#[from] io::Error),
}
