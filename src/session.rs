///! Session
use crate::brush::*;
use crate::cmd;
use crate::cmd::{Command, CommandLine, Key, Op, Value};
use crate::palette::*;
use crate::resources::{ResourceManager, SnapshotId};
use crate::view::{FileStatus, View, ViewId};

use rgx::core::{PresentMode, Rect};
use rgx::kit::shape2d;
use rgx::kit::Rgba8;
use rgx::winit;
use rgx::winit::{ElementState, KeyboardInput, VirtualKeyCode, WindowEvent};

use cgmath::prelude::*;
use cgmath::{Matrix4, Point2, Vector2};

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::io::BufRead;
use std::path::Path;
use std::str::FromStr;
use std::time;

#[repr(C)]
#[derive(Copy, Clone)]
struct Bgra8 {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
}

///////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Mode {
    Normal,
    Visual,
    Command,
    #[allow(dead_code)]
    Present,
}

#[derive(Debug, Clone)]
pub enum Tool {
    Brush(Brush),
    Sampler,
    #[allow(dead_code)]
    Pan,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Brush(Brush::default())
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum MessageType {
    Info,
    Hint,
    Echo,
    Error,
    Warning,
    #[allow(dead_code)]
    Replay,
    #[allow(dead_code)]
    Okay,
}

impl MessageType {
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

pub struct Message {
    string: String,
    message_type: MessageType,
}

impl Message {
    pub fn new<D: fmt::Display>(s: D, t: MessageType) -> Self {
        Message {
            string: format!("{}", s),
            message_type: t,
        }
    }

    pub fn color(&self) -> Rgba8 {
        self.message_type.color()
    }

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

#[derive(PartialEq, Clone, Debug)]
struct KeyBinding {
    modes: Vec<Mode>,
    modifiers: winit::ModifiersState,
    key: Key,
    state: ElementState,
    command: Command,
}

#[derive(Debug)]
struct KeyBindings {
    elems: Vec<KeyBinding>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        KeyBindings {
            elems: vec![KeyBinding {
                modes: vec![Mode::Normal],
                modifiers: winit::ModifiersState::default(),
                key: Key::Virtual(winit::VirtualKeyCode::Colon),
                state: ElementState::Pressed,
                command: Command::Mode(Mode::Command),
            }],
        }
    }
}

impl KeyBindings {
    pub fn add(&mut self, binding: KeyBinding) {
        self.elems.push(binding);
    }

    pub fn find(
        &self,
        key: Key,
        modifiers: winit::ModifiersState,
        state: winit::ElementState,
        mode: &Mode,
    ) -> Option<KeyBinding> {
        self.elems.iter().cloned().find(|kb| {
            kb.key == key
                && (kb.modifiers == winit::ModifiersState::default()
                    || kb.modifiers == modifiers)
                && kb.state == state
                && kb.modes.contains(mode)
        })
    }
}

pub struct Settings {
    pub checker: bool,
    pub vsync: bool,
    pub frame_delay: time::Duration,
}

impl Settings {
    pub fn present_mode(&self) -> PresentMode {
        if self.vsync {
            PresentMode::Vsync
        } else {
            PresentMode::NoVsync
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            checker: false,
            vsync: true,
            frame_delay: time::Duration::from_micros(16666),
        }
    }
}

pub struct Session {
    pub is_running: bool,
    pub mode: Mode,

    pub width: f32,
    pub height: f32,

    pub hidpi_factor: f64,

    pub cx: f32,
    pub cy: f32,

    pub offset: Vector2<f32>,
    pub message: Message,

    pub fg: Rgba8,
    pub bg: Rgba8,

    resources: ResourceManager,

    key_bindings: KeyBindings,

    pub paused: bool,
    pub onion: bool,
    pub help: bool,
    pub recording: bool,
    pub selection: Rect<f32>,

    pub settings: Settings,

    pub views: BTreeMap<ViewId, View>,
    pub active_view_id: ViewId,

    next_view_id: ViewId,
    views_lru: VecDeque<ViewId>,

    pub cmdline: CommandLine,
    pub palette: Palette,

    /// Set to `true` if a view was added or removed from the session.
    pub dirty: bool,

    pub tool: Tool,
    pub prev_tool: Option<Tool>,

    #[allow(dead_code)]
    paste: (),
    #[allow(dead_code)]
    recording_opts: u32,
    #[allow(dead_code)]
    grid_w: u32,
    #[allow(dead_code)]
    grid_h: u32,
    #[allow(dead_code)]
    fps: u32,
    #[allow(dead_code)]
    mouse_down: bool,
    #[allow(dead_code)]
    mouse_selection: Rect<f32>,
    #[allow(dead_code)]
    frame_count: u64,
}

impl Session {
    pub const MAX_VIEWS: usize = 64;
    pub const DEFAULT_VIEW_W: u32 = 128;
    pub const DEFAULT_VIEW_H: u32 = 128;

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

    pub fn new(
        w: u32,
        h: u32,
        hidpi_factor: f64,
        resources: ResourceManager,
    ) -> Self {
        Self {
            is_running: false,
            width: w as f32,
            height: h as f32,
            hidpi_factor,
            cx: 0.,
            cy: 0.,
            offset: Vector2::zero(),
            tool: Tool::Brush(Brush::default()),
            prev_tool: None,
            mouse_down: false,
            mouse_selection: Rect::new(0., 0., 0., 0.),
            frame_count: 0,
            paused: false,
            help: false,
            onion: false,
            fg: Rgba8::WHITE,
            bg: Rgba8::BLACK,
            settings: Settings::default(),
            palette: Palette::new(Self::PALETTE_CELL_SIZE),
            key_bindings: KeyBindings::default(),
            fps: 6,
            views: BTreeMap::new(),
            views_lru: VecDeque::new(),
            cmdline: CommandLine::new(),
            grid_w: 0,
            grid_h: 0,
            mode: Mode::Normal,
            selection: Rect::new(0., 0., 0., 0.),
            message: Message::default(),
            paste: (),
            recording: false,
            recording_opts: 0,
            active_view_id: ViewId(0),
            next_view_id: ViewId(1),
            resources,
            dirty: true,
        }
    }

    pub fn init(mut self) -> Self {
        self.is_running = true;

        if let Some(dir) = std::env::var_os("HOME") {
            self.source_dir(dir).ok();
        }
        self.source_dir(".").ok();
        self
    }

    pub fn blank(&mut self, fs: FileStatus, w: u32, h: u32) {
        let id = self.gen_view_id();

        self.add_view(View::new(id, fs, w, h));
        self.edit_view(id);

        self.resources.add_blank_view(&id, w, h);
    }

    pub fn active_view_coords(&self, x: f32, y: f32) -> Point2<f32> {
        let v = self.active_view();
        let mut p = Point2::new(x, y);

        p = p - self.offset - v.offset;
        p = p / v.zoom;

        if v.flip_x {
            p.x = v.width() as f32 - p.x;
        }
        if v.flip_y {
            p.y = v.height() as f32 - p.y;
        }

        p.map(f32::floor)
    }

    pub fn frame(
        &mut self,
        events: &mut Vec<winit::WindowEvent>,
        out: &mut shape2d::Batch,
        delta: time::Duration,
    ) {
        self.dirty = false;

        for (_, v) in &mut self.views {
            v.frame(delta);
        }

        for event in events.drain(..) {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    self.handle_cursor_moved(
                        position.x.floor() as f32,
                        self.height - position.y.floor() as f32,
                        out,
                    );
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    self.handle_mouse_input(button, state, out);
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    self.handle_keyboard_input(input);
                }
                WindowEvent::ReceivedCharacter(c) => {
                    self.handle_received_character(c);
                }
                WindowEvent::CloseRequested => {
                    self.quit();
                }
                WindowEvent::Refresh => {}
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

    pub fn new_frame(&mut self) {
        let (_id, _w, _h) = {
            let v = self.active_view_mut();
            v.extend();

            (v.id, v.width(), v.height())
        };
        // TODO: Copy previous frame to next frame.
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
        debug!("source: {}", path.as_ref().display());

        let f = File::open(&path)?;
        let r = io::BufReader::new(f);

        for line in r.lines() {
            let line = line?;

            if line.starts_with(cmd::COMMENT) {
                continue;
            }
            match Command::from_str(&format!(":{}", line)) {
                Err(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                Ok(cmd) => self.command(cmd),
            }
        }
        Ok(())
    }

    fn source_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.source_path(dir.as_ref().join(".rxrc"))
    }

    fn load_view<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        debug!("load: {:?}", path);

        if let Some(ext) = path.extension() {
            if ext != "png" {
                self.message(
                    "Warning: trying to load file with unrecognized extension",
                    MessageType::Warning,
                );
            }
        } else {
            self.message(
                "Warning: trying to load file without an extension",
                MessageType::Warning,
            );
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
            let s_id = self.save_view_as(id, f)?;
            self.view_mut(id).saved(s_id);
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "no file name given"))
        }
    }

    pub fn save_view_as<P: AsRef<Path>>(
        &mut self,
        id: ViewId,
        path: P,
    ) -> io::Result<SnapshotId> {
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

        self.message(
            format!(
                "\"{}\" {} pixels written",
                path.as_ref().display(),
                npixels,
            ),
            MessageType::Info,
        );
        Ok(s_id)
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
        let (_, first) = self
            .views
            .iter()
            .next()
            .expect("view list should never be empty");

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
        p: Point2<f32>,
        offx: f32,
        offy: f32,
        zoom: f32,
    ) -> Point2<f32> {
        Point2::new(
            p.x - ((p.x - offx - self.offset.x) % zoom),
            p.y - ((p.y - offy - self.offset.y) % zoom),
        )
        .map(f32::floor)
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
        let px = self.cx - self.offset.x;
        let py = self.cy - self.offset.y;

        let cursor = Point2::new(self.cx, self.cy);

        let within = self.active_view().contains(cursor - self.offset);
        let zprev = self.active_view().zoom;

        debug!("zoom: {} -> {}", zprev, z);

        self.offset = if within {
            let zdiff = z / zprev;

            let nx = (px * zdiff).floor();
            let ny = (py * zdiff).floor();

            let mut offset = Vector2::new(self.cx - nx, self.cy - ny);

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

    fn command(&mut self, cmd: Command) {
        debug!("command: {:?}", cmd);

        self.message_clear();

        return match cmd {
            Command::Mode(ref m) => {
                self.mode = *m;
            }
            Command::Quit => {
                if let FileStatus::Modified(_) = &self.active_view().file_status
                {
                    self.message(
                        "Error: no write since last change",
                        MessageType::Error,
                    );
                } else {
                    self.command(Command::ForceQuit);
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
                let v = self.active_view_mut();
                v.resize_frame(fw, fh);
                v.touch();
            }
            Command::ForceQuit => self.quit_view(self.active_view_id),
            Command::Echo(ref v) => {
                let result = match v {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Ident(s) => match s.as_str() {
                        "s:vsync" => Ok(Value::Bool(self.settings.vsync)),
                        "s:offset" => Ok(Value::Vector2(self.offset)),
                        "v:offset" => {
                            Ok(Value::Vector2(self.active_view().offset))
                        }
                        "v:zoom" => Ok(Value::F32(self.active_view().zoom)),
                        _ => Err(format!("Error: {} is undefined", s)),
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
                self.unimplemented();
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
            Command::NewFrame => {
                self.new_frame();
            }
            Command::Slice(None) => {
                self.active_view_mut().slice(1);
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
                }
            }
            Command::Set(ref k, ref v) => match k.as_str() {
                "checker" => {
                    if let &Value::Bool(b) = v {
                        self.settings.checker = b;
                    }
                }
                "vsync" => {
                    if let &Value::Bool(b) = v {
                        self.settings.vsync = b;
                    }
                }
                "frame_delay" => {
                    if let &Value::F32(f) = v {
                        self.settings.frame_delay =
                            time::Duration::from_micros((f * 1000.) as u64);
                    } else if let &Value::U32(u) = v {
                        self.settings.frame_delay =
                            time::Duration::from_micros(u as u64 * 1000);
                    }
                }
                setting => {
                    self.message(
                        format!("Error: setting {} not recognized", setting),
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
                    state: ElementState::Pressed,
                    modifiers: winit::ModifiersState::default(),
                });
                if let Some(cmd) = release {
                    self.key_bindings.add(KeyBinding {
                        key,
                        modes: vec![Mode::Normal],
                        command: cmd,
                        state: ElementState::Released,
                        modifiers: winit::ModifiersState::default(),
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
        self.mode = Mode::Normal;
        self.selection = Rect::empty();
        self.cmdline.clear();
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
            Err(e) => self.message(e, MessageType::Error),
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

    fn sample_color(&mut self, x: u32, y: u32) {
        let color = {
            // TODO: Sample from any view.
            let resources = self.resources.lock();
            let snapshot = resources.get_snapshot(&self.active_view_id);

            assert!(snapshot.pixels.len() % std::mem::size_of::<Bgra8>() == 0);

            let (head, pixels, tail) =
                unsafe { snapshot.pixels.align_to::<Bgra8>() };

            assert!(head.is_empty());
            assert!(tail.is_empty());

            let index =
                ((snapshot.height() - y) * snapshot.width() + x) as usize;
            let bgra = pixels[index];

            Rgba8::new(bgra.r, bgra.g, bgra.b, bgra.a)
        };
        self.pick_color(color);
    }

    pub fn record_macro(&mut self, _cmd: String) {}

    ///////////////////////////////////////////////////////////////////////////
    // Event handlers
    ///////////////////////////////////////////////////////////////////////////

    pub fn handle_resized(&mut self, size: winit::dpi::LogicalSize) {
        let win = size.to_physical(self.hidpi_factor);

        self.width = win.width as f32;
        self.height = win.height as f32;

        // TODO: Reset session cursor coordinates
        self.center_palette();
        self.center_active_view();
    }

    pub fn handle_mouse_input(
        &mut self,
        button: winit::MouseButton,
        state: winit::ElementState,
        out: &mut shape2d::Batch,
    ) {
        if button != winit::MouseButton::Left {
            return;
        }

        if state == winit::ElementState::Pressed {
            self.mouse_down = true;
            self.record_macro(format!("cursor/down"));

            // Click on palette
            if let Some(color) = self.palette.hover_color {
                if self.mode == Mode::Command {
                    self.cmdline.puts(&color.to_string());
                } else {
                    self.pick_color(color);
                }
                return;
            }

            // Click on active view
            let p = Point2::new(self.cx, self.cy) - self.offset;
            if self.active_view().contains(p) {
                let p = self.active_view_coords(self.cx, self.cy);

                match self.mode {
                    Mode::Normal => match self.tool {
                        Tool::Brush(ref mut brush) => {
                            if brush.is_set(BrushMode::Erase) {
                                brush.start_drawing(p, Rgba8::TRANSPARENT, out);
                            } else {
                                brush.start_drawing(p, self.fg, out);
                            }
                        }
                        Tool::Sampler => {
                            self.sample_color(
                                p.x.round() as u32,
                                p.y.round() as u32,
                            );
                        }
                        Tool::Pan => {}
                    },
                    Mode::Command => {
                        // TODO
                    }
                    Mode::Visual => {
                        // TODO
                    }
                    Mode::Present => {}
                }
            }
        } else if state == winit::ElementState::Released {
            self.mouse_down = false;
            self.record_macro(format!("cursor/up"));

            if let Tool::Brush(ref mut brush) = self.tool {
                brush.stop_drawing();
                self.active_view_mut().touch();
            }
        }
    }

    pub fn handle_cursor_moved(
        &mut self,
        cx: f32,
        cy: f32,
        out: &mut shape2d::Batch,
    ) {
        self.record_macro(format!("cursor/move {} {}", cx, cy));
        self.palette.handle_cursor_moved(cx, cy);
        for (_, v) in &mut self.views {
            v.handle_cursor_moved(cx, cy);
        }
        let p = self.active_view_coords(cx, cy);

        match self.mode {
            Mode::Normal => match self.tool {
                Tool::Brush(ref mut brush) => {
                    if brush.state == BrushState::DrawStarted
                        || brush.state == BrushState::Drawing
                    {
                        brush.state = BrushState::Drawing;
                        if brush.is_set(BrushMode::Erase) {
                            brush.tick(p, Rgba8::TRANSPARENT, out);
                        } else {
                            brush.tick(p, self.fg, out);
                        }
                    }
                }
                Tool::Pan => {
                    self.offset.x += cx - self.cx;
                    self.offset.y += cy - self.cy;
                }
                Tool::Sampler => {}
            },
            Mode::Visual => {}
            _ => {}
        }
        self.cx = cx;
        self.cy = cy;
    }

    pub fn handle_received_character(&mut self, c: char) {
        if self.mode == Mode::Command {
            if c.is_control() {
                return;
            }
            self.cmdline_handle_input(c);
        } else {
            if let Some(kb) = self.key_bindings.find(
                Key::Char(c),
                winit::ModifiersState::default(),
                winit::ElementState::Pressed,
                &self.mode,
            ) {
                self.command(kb.command);
            }
        }
    }

    pub fn handle_keyboard_input(&mut self, input: winit::KeyboardInput) {
        let KeyboardInput {
            state,
            modifiers,
            virtual_keycode,
            ..
        } = input;

        if let Some(key) = virtual_keycode {
            // While the mouse is down, don't accept keyboard input.
            if self.mouse_down {
                return;
            }

            if self.mode == Mode::Visual {
                if key == VirtualKeyCode::Escape
                    && state == ElementState::Pressed
                {
                    self.selection = Rect::empty();
                    self.mode = Mode::Normal;
                    return;
                }
            } else if self.mode == Mode::Command {
                if state == ElementState::Pressed {
                    match key {
                        VirtualKeyCode::Back => {
                            self.cmdline_handle_backspace();
                        }
                        VirtualKeyCode::Return => {
                            self.cmdline_handle_enter();
                        }
                        VirtualKeyCode::Escape => {
                            self.cmdline_hide();
                        }
                        _ => {}
                    }
                }
                return;
            }

            if let Some(kb) = self.key_bindings.find(
                Key::Virtual(key),
                modifiers,
                state,
                &self.mode,
            ) {
                self.command(kb.command);
            }
        }
    }
}
