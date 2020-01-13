use crate::brush::{Brush, BrushMode};
use crate::parser::{Error, Parse, Parser, Result};
use crate::platform;
use crate::session::{Direction, Mode, PanState, Tool, VisualState};

use rgx::kit::Rgba8;
use rgx::rect::Rect;

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::path::Path;
use std::result;
use std::str::FromStr;

pub const COMMENT: char = '-';

#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    Incr,
    Decr,
    Set(f32),
}

/// User command. Most of the interactions available to
/// the user are modeled as commands that are processed
/// by the session.
#[derive(PartialEq, Debug, Clone)]
pub enum Command {
    Brush,
    BrushSet(BrushMode),
    BrushToggle(BrushMode),
    BrushSize(Op),
    BrushUnset(BrushMode),
    #[allow(dead_code)]
    Crop(Rect<u32>),
    ChangeDir(String),
    Echo(Value),
    Edit(Vec<String>),
    Fill(Rgba8),
    ForceQuit,
    ForceQuitAll,
    Map(Box<KeyMapping>),
    MapClear,
    Mode(Mode),
    AddFrame,
    CloneFrame(i32),
    RemoveFrame,
    Noop,
    PaletteAdd(Rgba8),
    PaletteClear,
    PaletteSample,
    Pan(i32, i32),
    Quit,
    QuitAll,
    Reset,
    Redo,
    ResizeFrame(u32, u32),
    SelectionMove(i32, i32),
    SelectionResize(i32, i32),
    SelectionOffset(i32, i32),
    SelectionExpand,
    SelectionPaste,
    SelectionYank,
    SelectionCut,
    SelectionFill(Option<Rgba8>),
    SelectionErase,
    SelectionJump(Direction),
    Set(String, Value),
    Slice(Option<usize>),
    Source(String),
    SwapColors,
    Toggle(String),
    Tool(Tool),
    ToolPrev,
    Undo,
    ViewCenter,
    ViewNext,
    ViewPrev,
    Write(Option<String>),
    WriteQuit,
    Zoom(Op),
}

impl Command {
    pub fn repeats(&self) -> bool {
        match self {
            Self::Zoom(_)
            | Self::BrushSize(_)
            | Self::Pan(_, _)
            | Self::Undo
            | Self::Redo
            | Self::ViewNext
            | Self::ViewPrev
            | Self::SelectionMove(_, _)
            | Self::SelectionJump(_)
            | Self::SelectionResize(_, _)
            | Self::SelectionOffset(_, _) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brush => write!(f, "Reset brush"),
            Self::BrushSet(m) => write!(f, "Set brush mode to `{}`", m),
            Self::BrushToggle(m) => write!(f, "Toggle `{}` brush mode", m),
            Self::BrushSize(Op::Incr) => write!(f, "Increase brush size"),
            Self::BrushSize(Op::Decr) => write!(f, "Decrease brush size"),
            Self::BrushSize(Op::Set(s)) => write!(f, "Set brush size to {}", s),
            Self::BrushUnset(m) => write!(f, "Unset brush `{}` mode", m),
            Self::Crop(_) => write!(f, "Crop view"),
            Self::ChangeDir(_) => write!(f, "Change the current working directory"),
            Self::Echo(_) => write!(f, "Echo a value"),
            Self::Edit(_) => write!(f, "Edit path(s)"),
            Self::Fill(c) => write!(f, "Fill view with {color}", color = c),
            Self::ForceQuit => write!(f, "Quit view without saving"),
            Self::ForceQuitAll => write!(f, "Quit all views without saving"),
            Self::Map(_) => write!(f, "Map a key combination to a command"),
            Self::MapClear => write!(f, "Clear all key mappings"),
            Self::Mode(m) => write!(f, "Switch session mode to {}", m),
            Self::AddFrame => write!(f, "Add a blank frame to the view"),
            Self::CloneFrame(i) => write!(f, "Clone frame {} and add it to the view", i),
            Self::RemoveFrame => write!(f, "Remove the last frame of the view"),
            Self::Noop => write!(f, "No-op"),
            Self::PaletteAdd(c) => write!(f, "Add {color} to palette", color = c),
            Self::PaletteClear => write!(f, "Clear palette"),
            Self::PaletteSample => write!(f, "Sample palette from view"),
            Self::Pan(x, 0) if *x > 0 => write!(f, "Pan workspace right"),
            Self::Pan(x, 0) if *x < 0 => write!(f, "Pan workspace left"),
            Self::Pan(0, y) if *y > 0 => write!(f, "Pan workspace up"),
            Self::Pan(0, y) if *y < 0 => write!(f, "Pan workspace down"),
            Self::Pan(x, y) => write!(f, "Pan workspace by {},{}", x, y),
            Self::Quit => write!(f, "Quit active view"),
            Self::QuitAll => write!(f, "Quit all views"),
            Self::Redo => write!(f, "Redo view edit"),
            Self::ResizeFrame(_, _) => write!(f, "Resize active view frame"),
            Self::Tool(Tool::Pan(_)) => write!(f, "Pan tool"),
            Self::Tool(Tool::Brush(_)) => write!(f, "Brush tool"),
            Self::Tool(Tool::Sampler) => write!(f, "Color sampler tool"),
            Self::ToolPrev => write!(f, "Switch to previous tool"),
            Self::Set(s, v) => write!(f, "Set {setting} to {val}", setting = s, val = v),
            Self::Slice(Some(n)) => write!(f, "Slice view into {} frame(s)", n),
            Self::Slice(None) => write!(f, "Reset view slices"),
            Self::Source(_) => write!(f, "Source an rx script (eg. a palette)"),
            Self::SwapColors => write!(f, "Swap foreground & background colors"),
            Self::Toggle(s) => write!(f, "Toggle {setting} on/off", setting = s),
            Self::Undo => write!(f, "Undo view edit"),
            Self::ViewCenter => write!(f, "Center active view"),
            Self::ViewNext => write!(f, "Go to next view"),
            Self::ViewPrev => write!(f, "Go to previous view"),
            Self::Write(None) => write!(f, "Write view to disk"),
            Self::Write(Some(_)) => write!(f, "Write view to disk as..."),
            Self::WriteQuit => write!(f, "Write file to disk and quit"),
            Self::Zoom(Op::Incr) => write!(f, "Zoom in view"),
            Self::Zoom(Op::Decr) => write!(f, "Zoom out view"),
            Self::Zoom(Op::Set(z)) => write!(f, "Set view zoom to {:.1}", z),
            Self::Reset => write!(f, "Reset all settings to default"),
            Self::SelectionFill(None) => write!(f, "Fill selection with foreground color"),
            Self::SelectionYank => write!(f, "Yank (copy) selection"),
            Self::SelectionCut => write!(f, "Cut selection"),
            Self::SelectionPaste => write!(f, "Paste selection"),
            Self::SelectionExpand => write!(f, "Expand selection to frame"),
            Self::SelectionOffset(1, 1) => write!(f, "Outset selection"),
            Self::SelectionOffset(-1, -1) => write!(f, "Inset selection"),
            Self::SelectionOffset(x, y) => write!(f, "Offset selection by {:2},{:2}", x, y),
            Self::SelectionMove(x, 0) if *x > 0 => write!(f, "Move selection right"),
            Self::SelectionMove(x, 0) if *x < 0 => write!(f, "Move selection left"),
            Self::SelectionMove(0, y) if *y > 0 => write!(f, "Move selection up"),
            Self::SelectionMove(0, y) if *y < 0 => write!(f, "Move selection down"),
            Self::SelectionJump(Direction::Forward) => {
                write!(f, "Move selection forward by one frame")
            }
            Self::SelectionJump(Direction::Backward) => {
                write!(f, "Move selection backward by one frame")
            }
            Self::SelectionErase => write!(f, "Erase selection contents"),
            _ => write!(f, "..."),
        }
    }
}

impl From<Command> for String {
    fn from(cmd: Command) -> Self {
        match cmd {
            Command::Brush => format!("brush"),
            Command::BrushSet(m) => format!("brush/set {}", m),
            Command::BrushSize(Op::Incr) => format!("brush/size +"),
            Command::BrushSize(Op::Decr) => format!("brush/size -"),
            Command::BrushSize(Op::Set(s)) => format!("brush/size {}", s),
            Command::BrushUnset(m) => format!("brush/unset {}", m),
            Command::Echo(_) => unimplemented!(),
            Command::Edit(_) => unimplemented!(),
            Command::Fill(c) => format!("v/fill {}", c),
            Command::ForceQuit => format!("q!"),
            Command::ForceQuitAll => format!("qa!"),
            Command::Map(_) => format!("map <key> <command> {{<command>}}"),
            Command::Mode(m) => format!("mode {}", m),
            Command::AddFrame => format!("f/add"),
            Command::CloneFrame(i) => format!("f/clone {}", i),
            Command::RemoveFrame => format!("f/remove"),
            Command::Noop => format!(""),
            Command::PaletteAdd(c) => format!("p/add {}", c),
            Command::PaletteClear => format!("p/clear"),
            Command::PaletteSample => unimplemented!(),
            Command::Pan(x, y) => format!("pan {} {}", x, y),
            Command::Quit => format!("q"),
            Command::Redo => format!("redo"),
            Command::ResizeFrame(w, h) => format!("f/resize {} {}", w, h),
            Command::Set(s, v) => format!("set {} = {}", s, v),
            Command::Slice(Some(n)) => format!("slice {}", n),
            Command::Slice(None) => format!("slice"),
            Command::Source(path) => format!("source {}", path),
            Command::SwapColors => format!("swap"),
            Command::Toggle(s) => format!("toggle {}", s),
            Command::Undo => format!("undo"),
            Command::ViewCenter => format!("v/center"),
            Command::ViewNext => format!("v/next"),
            Command::ViewPrev => format!("v/prev"),
            Command::Write(None) => format!("w"),
            Command::Write(Some(path)) => format!("w {}", path),
            Command::WriteQuit => format!("wq"),
            Command::Zoom(Op::Incr) => format!("v/zoom +"),
            Command::Zoom(Op::Decr) => format!("v/zoom -"),
            Command::Zoom(Op::Set(z)) => format!("v/zoom {}", z),
            _ => unimplemented!(),
        }
    }
}

impl<'a> Parse<'a> for platform::Key {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        if let Ok((_, p)) = p.clone().sigil('<') {
            let (key, p) = p.alpha()?;
            let (_, p) = p.sigil('>')?;
            let key = match key {
                "up" => platform::Key::Up,
                "down" => platform::Key::Down,
                "left" => platform::Key::Left,
                "right" => platform::Key::Right,
                "ctrl" => platform::Key::Control,
                "alt" => platform::Key::Alt,
                "shift" => platform::Key::Shift,
                "space" => platform::Key::Space,
                "return" => platform::Key::Return,
                "backspace" => platform::Key::Backspace,
                "tab" => platform::Key::Tab,
                "end" => platform::Key::End,
                "esc" => platform::Key::Escape,
                other => return Err(Error::new(format!("unknown key <{}>", other))),
            };
            Ok((key, p))
        } else {
            let (c, p) = p.parse::<char>()?;
            let key: platform::Key = c.into();

            if key == platform::Key::Unknown {
                return Err(Error::new(format!("unknown key {:?}", c)));
            }
            Ok((key, p))
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone)]
pub struct KeyMapping {
    pub key: platform::Key,
    pub press: Command,
    pub release: Option<Command>,
    pub modes: Vec<Mode>,
}

impl KeyMapping {
    fn parse<'a>(p: Parser<'a>, modes: &[Mode]) -> Result<'a, Self> {
        let modes = modes.to_vec();

        let (key, p) = p.parse::<platform::Key>()?;
        let (_, p) = p.whitespace()?;
        let (press, p) = p.parse::<Command>()?;
        let (_, p) = p.whitespace()?;

        let (release, p) = if let Ok((_, p)) = p.clone().sigil('{') {
            let (cmd, p) = p.parse::<Command>()?;
            let (_, p) = p.sigil('}')?;
            (Some(cmd), p)
        } else {
            (None, p)
        };
        Ok((
            KeyMapping {
                key,
                press,
                release,
                modes,
            },
            p,
        ))
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    U32(u32),
    U32Tuple(u32, u32),
    F64(f64),
    F64Tuple(f32, f32),
    Str(String),
    Ident(String),
    Rgba8(Rgba8),
}

impl Value {
    pub fn is_set(&self) -> bool {
        if let Value::Bool(b) = self {
            return *b;
        }
        panic!("expected {:?} to be a `bool`", self);
    }

    pub fn to_f64(&self) -> f64 {
        if let Value::F64(n) = self {
            return *n;
        }
        panic!("expected {:?} to be a `float`", self);
    }

    pub fn to_u64(&self) -> u64 {
        if let Value::U32(n) = self {
            return *n as u64;
        }
        panic!("expected {:?} to be a `uint`", self);
    }

    pub fn to_rgba8(&self) -> Rgba8 {
        if let Value::Rgba8(rgba8) = self {
            return *rgba8;
        }
        panic!("expected {:?} to be a `Rgba8`", self);
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Bool(_) => "on / off",
            Self::U32(_) => "positive integer, eg. 32",
            Self::F64(_) => "float, eg. 1.33",
            Self::U32Tuple(_, _) => "two positive integers, eg. 32, 48",
            Self::F64Tuple(_, _) => "two floats , eg. 32.17, 48.29",
            Self::Str(_) => "string, eg. \"fnord\"",
            Self::Rgba8(_) => "color, eg. #ffff00",
            Self::Ident(_) => "identifier, eg. fnord",
        }
    }
}

impl Into<(u32, u32)> for Value {
    fn into(self) -> (u32, u32) {
        if let Value::U32Tuple(x, y) = self {
            return (x, y);
        }
        panic!("expected {:?} to be a `(u32, u32)`", self);
    }
}

impl Into<f32> for Value {
    fn into(self) -> f32 {
        if let Value::F64(x) = self {
            return x as f32;
        }
        panic!("expected {:?} to be a `f64`", self);
    }
}

impl Into<f64> for Value {
    fn into(self) -> f64 {
        if let Value::F64(x) = self {
            return x as f64;
        }
        panic!("expected {:?} to be a `f64`", self);
    }
}

impl<'a> Parse<'a> for Value {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let c = p.peek();

        if c == Some('"') {
            let (v, p) = p.string()?;
            Ok((Value::Str(v.to_string()), p))
        } else if c == Some('#') {
            let (v, p) = p.parse::<Rgba8>()?;
            Ok((Value::Rgba8(v), p))
        } else if c.map_or(false, |c| c.is_digit(10)) {
            if let Ok(((x, y), p)) = p.clone().parse::<(u32, u32)>() {
                Ok((Value::U32Tuple(x, y), p))
            } else if let Ok((v, p)) = p.clone().parse::<u32>() {
                Ok((Value::U32(v), p))
            } else if let Ok((v, p)) = p.clone().parse::<f64>() {
                Ok((Value::F64(v), p))
            } else {
                let (input, _) = p.until(|c| c.is_whitespace())?;
                Err(Error::new(format!("malformed number: `{}`", input)))
            }
        } else {
            let (i, p) = p.identifier()?;
            match i {
                "on" => Ok((Value::Bool(true), p)),
                "off" => Ok((Value::Bool(false), p)),
                _ => Ok((Value::Ident(i.to_string()), p)),
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => "on".fmt(f),
            Value::Bool(false) => "off".fmt(f),
            Value::U32(u) => u.fmt(f),
            Value::F64(x) => x.fmt(f),
            Value::U32Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::F64Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::Str(s) => s.fmt(f),
            Value::Rgba8(c) => c.fmt(f),
            Value::Ident(i) => i.fmt(f),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, PartialEq, Eq)]
pub struct History {
    /// History path.
    pub path: std::path::PathBuf,
    /// The history of commands entered.
    entries: VecDeque<String>,
    /// The current cursor into the history.
    cursor: Option<usize>,
    /// Maximum number of entries.
    capacity: usize,
}

impl History {
    pub fn new<P: AsRef<Path>>(path: P, capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: None,
            capacity,
            path: path.as_ref().into(),
        }
    }

    pub fn load(&mut self) -> io::Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let f = File::open(&self.path)?;
        let r = BufReader::new(f);

        for line in r.lines() {
            self.add(line?);
        }
        Ok(())
    }

    pub fn save(&self) -> io::Result<()> {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        if self.is_empty() {
            return Ok(());
        }

        let parent = self
            .path
            .parent()
            .expect("saving to a path with a parent directory");
        std::fs::create_dir_all(parent)?;

        let f = File::create(&self.path)?;
        let mut w = BufWriter::new(f);

        for entry in self.entries.iter().rev() {
            w.write_all(entry.as_bytes())?;
            w.write_all(b"\n")?;
        }
        w.flush()
    }

    pub fn add<S: Into<String>>(&mut self, s: S) {
        let entry = s.into();
        if self.entries.front() != Some(&entry) {
            self.entries.push_front(entry);
            self.entries.truncate(self.capacity);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn next(&mut self) -> Option<&str> {
        if self.is_empty() {
            return None;
        }
        let cursor = match self.cursor {
            None | Some(0) => self.len() - 1,
            Some(i) => i - 1,
        };
        self.cursor = Some(cursor);
        self.get(cursor)
    }

    pub fn prev(&mut self) -> Option<&str> {
        if self.is_empty() {
            return None;
        }
        let cursor = match self.cursor {
            None => 0,
            Some(i) => (i + 1) % self.len(),
        };
        self.cursor = Some(cursor);
        self.get(cursor)
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|s| s.as_str())
    }
}

pub struct CommandLine {
    /// The history of commands entered.
    pub history: History,

    /// The current input string displayed to the user.
    input: String,
}

impl CommandLine {
    const MAX_INPUT: usize = 256;

    pub fn new<P: AsRef<Path>>(history_path: P) -> Self {
        Self {
            input: String::with_capacity(Self::MAX_INPUT),
            history: History::new(history_path, 1024),
        }
    }

    pub fn input(&self) -> String {
        self.input.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    pub fn putc(&mut self, c: char) {
        if self.input.len() + 1 >= self.input.capacity() {
            return;
        }
        self.input.push(c);
    }

    pub fn puts(&mut self, s: &str) {
        self.input.push_str(s);
    }

    pub fn replace(&mut self, s: &str) {
        self.input.clear();
        self.input.push_str(s);
    }

    pub fn delc(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.history.cursor = None;
    }
}

///////////////////////////////////////////////////////////////////////////////

impl FromStr for Command {
    type Err = Error;

    fn from_str(input: &str) -> result::Result<Self, Self::Err> {
        let p = Parser::new(input);
        match p.parse::<Command>() {
            Ok((cmd, p)) => {
                let (_, p) = p.clone().comment().unwrap_or(("", p));
                p.finish()?; // Make sure we've consumed all the input
                Ok(cmd)
            }
            // TODO: Use `enum` for error.
            Err(e) => Err(e),
        }
    }
}

impl<'a> Parse<'a> for Command {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (_, p) = p.sigil(':')?;
        let (_, p) = p.whitespace()?;

        if p.is_empty() {
            return Ok((Command::Noop, p));
        }

        if Some('#') == p.peek() {
            let (rgba, p) = p.parse::<Rgba8>()?;
            return Ok((Command::PaletteAdd(rgba), p));
        }

        let (cmd, p) = p.identifier()?;
        let (_, p) = p.whitespace()?;

        match cmd {
            "q" => Ok((Command::Quit, p)),
            "qa" => Ok((Command::QuitAll, p)),
            "q!" => Ok((Command::ForceQuit, p)),
            "qa!" => Ok((Command::ForceQuitAll, p)),
            "wq" | "x" => Ok((Command::WriteQuit, p)),
            "w" => {
                if p.is_empty() {
                    Ok((Command::Write(None), p))
                } else {
                    let (path, p) = p.path()?;
                    Ok((Command::Write(Some(path)), p))
                }
            }
            "e" => {
                if p.is_empty() {
                    Ok((Command::Edit(Vec::with_capacity(0)), p))
                } else {
                    let mut q = p;
                    let mut edits = Vec::new();

                    while let Ok((path, p)) = q.clone().path() {
                        edits.push(path);
                        let (_, p) = p.whitespace()?;
                        q = p;
                    }
                    Ok((Command::Edit(edits), q))
                }
            }
            "help" => Ok((Command::Mode(Mode::Help), p)),
            "set" => {
                let (k, p) = p.identifier()?;
                let (_, p) = p.whitespace()?;

                if p.is_empty() {
                    Ok((Command::Set(k.to_string(), Value::Bool(true)), p))
                } else {
                    let (_, p) = p.sigil('=')?;
                    let (_, p) = p.whitespace()?;
                    let (v, p) = p.parse::<Value>()?;
                    Ok((Command::Set(k.to_string(), v), p))
                }
            }
            "unset" => {
                let (k, p) = p.identifier()?;
                Ok((Command::Set(k.to_string(), Value::Bool(false)), p))
            }
            "toggle" => {
                let (k, p) = p.identifier()?;
                Ok((Command::Toggle(k.to_string()), p))
            }
            "echo" => {
                let (v, p) = p.parse::<Value>()?;
                Ok((Command::Echo(v), p))
            }
            "slice" => {
                if p.is_empty() {
                    Ok((Command::Slice(None), p))
                } else {
                    let (n, p) = p.parse::<u32>()?;
                    Ok((Command::Slice(Some(n as usize)), p))
                }
            }
            "source" => {
                let (path, p) = p.path()?;
                Ok((Command::Source(path), p))
            }
            "cd" => {
                let (path, p) = p.path()?;
                Ok((Command::ChangeDir(path), p))
            }
            "zoom" => {
                if let Ok((_, p)) = p.clone().sigil('+') {
                    Ok((Command::Zoom(Op::Incr), p))
                } else if let Ok((_, p)) = p.clone().sigil('-') {
                    Ok((Command::Zoom(Op::Decr), p))
                } else if let Ok((z, p)) = p.parse::<f64>() {
                    Ok((Command::Zoom(Op::Set(z as f32)), p))
                } else {
                    Err(Error::new("couldn't parse zoom parameter"))
                }
            }
            "brush" => Ok((Command::Tool(Tool::Brush(Brush::default())), p)),
            "brush/size" => {
                let (c, p) = p.parse::<char>()?;
                match c {
                    '+' => Ok((Command::BrushSize(Op::Incr), p)),
                    '-' => Ok((Command::BrushSize(Op::Decr), p)),
                    _ => Err(Error::new("invalid parameter")),
                }
            }
            "brush/set" => {
                let (mode, p) = p.parse::<BrushMode>()?;
                Ok((Command::BrushSet(mode), p))
            }
            "brush/unset" => {
                let (mode, p) = p.parse::<BrushMode>()?;
                Ok((Command::BrushUnset(mode), p))
            }
            "brush/toggle" => {
                let (mode, p) = p.parse::<BrushMode>()?;
                Ok((Command::BrushToggle(mode), p))
            }
            "mode" => {
                let (mode, p) = p.parse::<Mode>()?;
                Ok((Command::Mode(mode), p))
            }
            "visual" => Ok((Command::Mode(Mode::Visual(VisualState::default())), p)),
            "sampler" => Ok((Command::Tool(Tool::Sampler), p)),
            "sampler/off" => Ok((Command::ToolPrev, p)),
            "v/next" => Ok((Command::ViewNext, p)),
            "v/prev" => Ok((Command::ViewPrev, p)),
            "v/center" => Ok((Command::ViewCenter, p)),
            "v/clear" => {
                if let Ok((rgba, p)) = p.clone().parse::<Rgba8>() {
                    Ok((Command::Fill(rgba), p))
                } else {
                    Ok((Command::Fill(Rgba8::TRANSPARENT), p))
                }
            }
            "pan" => {
                let ((x, y), p) = p.parse::<(i32, i32)>()?;
                Ok((Command::Pan(x, y), p))
            }
            "map/visual" => {
                let (km, p) = KeyMapping::parse(
                    p,
                    &[
                        Mode::Visual(VisualState::selecting()),
                        Mode::Visual(VisualState::Pasting),
                    ],
                )?;
                Ok((Command::Map(Box::new(km)), p))
            }
            "map/normal" => {
                let (km, p) = KeyMapping::parse(p, &[Mode::Normal])?;
                Ok((Command::Map(Box::new(km)), p))
            }
            "map" => {
                let (km, p) = KeyMapping::parse(
                    p,
                    &[
                        Mode::Normal,
                        Mode::Visual(VisualState::selecting()),
                        Mode::Visual(VisualState::Pasting),
                    ],
                )?;
                Ok((Command::Map(Box::new(km)), p))
            }
            "map/clear!" => Ok((Command::MapClear, p)),
            "p/add" => {
                let (rgba, p) = p.parse::<Rgba8>()?;
                Ok((Command::PaletteAdd(rgba), p))
            }
            "p/clear" => Ok((Command::PaletteClear, p)),
            "p/sample" => Ok((Command::PaletteSample, p)),
            "undo" => Ok((Command::Undo, p)),
            "redo" => Ok((Command::Redo, p)),
            "f/new" => Err(Error::new(
                "parsing failed: `f/new` has been renamed to `f/add`",
            )),
            "f/add" => Ok((Command::AddFrame, p)),
            "f/clone" => match p.clone().parse::<i32>().or_else(|_| Ok((-1, p))) {
                Ok((index, p)) => Ok((Command::CloneFrame(index), p)),
                Err(e) => Err(e),
            },
            "f/remove" => Ok((Command::RemoveFrame, p)),
            "f/resize" => {
                let ((w, h), p) = p.parse::<(u32, u32)>()?;
                Ok((Command::ResizeFrame(w, h), p))
            }
            "tool" => {
                let (t, p) = p.word()?;
                match t {
                    "pan" => Ok((Command::Tool(Tool::Pan(PanState::default())), p)),
                    "brush" => Ok((Command::Tool(Tool::Brush(Brush::default())), p)),
                    "sampler" => Ok((Command::Tool(Tool::Sampler), p)),
                    _ => Err(Error::new(format!("unknown tool {:?}", t))),
                }
            }
            "tool/prev" => Ok((Command::ToolPrev, p)),
            "swap" => Ok((Command::SwapColors, p)),
            "reset!" => Ok((Command::Reset, p)),
            "selection/move" => {
                let ((x, y), p) = p.parse::<(i32, i32)>()?;
                Ok((Command::SelectionMove(x, y), p))
            }
            "selection/resize" => {
                let ((x, y), p) = p.parse::<(i32, i32)>()?;
                Ok((Command::SelectionResize(x, y), p))
            }
            "selection/yank" => Ok((Command::SelectionYank, p)),
            "selection/cut" => Ok((Command::SelectionCut, p)),
            "selection/paste" => Ok((Command::SelectionPaste, p)),
            "selection/expand" => Ok((Command::SelectionExpand, p)),
            "selection/erase" => Ok((Command::SelectionErase, p)),
            "selection/offset" => {
                let ((x, y), p) = p.parse::<(i32, i32)>()?;
                Ok((Command::SelectionOffset(x, y), p))
            }
            "selection/jump" => {
                let (dir, p) = p.parse::<Direction>()?;
                Ok((Command::SelectionJump(dir), p))
            }
            "selection/fill" => {
                if let Ok((rgba, p)) = p.clone().parse::<Rgba8>() {
                    Ok((Command::SelectionFill(Some(rgba)), p))
                } else {
                    Ok((Command::SelectionFill(None), p))
                }
            }
            unrecognized => Err(Error::new(format!(
                "unrecognized command ':{}'",
                unrecognized
            ))),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use super::History;
    use tempfile;

    #[test]
    fn test_history() {
        let mut h = History::new("/dev/null", 16);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("third");

        assert_eq!(h.prev(), Some("third"));
        assert_eq!(h.prev(), Some("second"));
        assert_eq!(h.prev(), Some("first"));
        assert_eq!(h.prev(), Some("third"));
        assert_eq!(h.next(), Some("first"));
        assert_eq!(h.next(), Some("second"));
    }

    #[test]
    fn test_history_empty() {
        let mut h = History::new("/dev/null", 16);

        assert_eq!(h.prev(), None);
        assert_eq!(h.next(), None);
    }

    #[test]
    fn test_history_save_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".history");
        let mut h1 = History::new(&path, 16);

        h1.add("first");
        h1.add("second");
        h1.add("third");
        h1.save().unwrap();
        h1.save().unwrap();

        let mut h2 = History::new(&path, 16);
        h2.load().unwrap();

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_history_capacity() {
        let mut h = History::new("/dev/null", 3);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("fourth");
        h.add("fifth");

        assert_eq!(
            h.entries.iter().collect::<Vec<_>>().as_slice(),
            &["fifth", "fourth", "third"]
        );
    }
}
