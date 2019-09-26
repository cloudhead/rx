use crate::brush::BrushMode;
use crate::platform;
use crate::session::Mode;

use rgx::core::Rect;
use rgx::kit::Rgba8;

use std::fmt;
use std::result;
use std::str::FromStr;
use std::time;

pub const COMMENT: char = '-';

/// User command. Most of the interactions available to
/// the user are modeled as commands that are processed
/// by the session.
#[derive(PartialEq, Debug, Clone)]
pub enum Command {
    Brush,
    BrushSet(BrushMode),
    BrushSize(Op),
    BrushUnset(BrushMode),
    #[allow(dead_code)]
    Crop(Rect<u32>),
    #[allow(dead_code)]
    CursorMove(f32, f32),
    #[allow(dead_code)]
    CursorPress,
    #[allow(dead_code)]
    CursorRelease,
    Echo(Value),
    Edit(Vec<String>),
    #[allow(dead_code)]
    Fill(Rgba8),
    ForceQuit,
    ForceQuitAll,
    FullScreen,
    Help,
    Map(Key, Box<(Command, Option<Command>)>),
    Mode(Mode),
    AddFrame,
    CloneFrame(i32),
    RemoveFrame,
    Noop,
    PaletteAdd(Rgba8),
    PaletteClear,
    PaletteSample,
    Pan(i32, i32),
    #[allow(dead_code)]
    Pause,
    #[allow(dead_code)]
    Play,
    Quit,
    #[allow(dead_code)]
    Record,
    Redo,
    ResizeFrame(u32, u32),
    Sampler(bool),
    Set(String, Value),
    Sleep(time::Duration),
    Slice(Option<usize>),
    Source(String),
    SwapColors,
    #[allow(dead_code)]
    TestCheck,
    #[allow(dead_code)]
    TestDigest,
    #[allow(dead_code)]
    TestDiscard,
    #[allow(dead_code)]
    TestPlay,
    #[allow(dead_code)]
    TestRecord,
    #[allow(dead_code)]
    TestSave,
    Toggle(String),
    Undo,
    ViewCenter,
    ViewNext,
    ViewPrev,
    Write(Option<String>),
    #[allow(dead_code)]
    WriteQuit,
    Zoom(Op),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Brush => write!(f, "Reset brush"),
            Self::BrushSet(m) => write!(f, "Set brush mode to `{}`", m),
            Self::BrushSize(Op::Incr) => write!(f, "Increase brush size"),
            Self::BrushSize(Op::Decr) => write!(f, "Decrease brush size"),
            Self::BrushSize(Op::Set(s)) => write!(f, "Set brush size to {}", s),
            Self::BrushUnset(m) => write!(f, "Unset brush `{}` mode", m),
            Self::Echo(_) => write!(f, "Echo a value"),
            Self::Edit(_) => write!(f, "Edit path(s)"),
            Self::Fill(c) => write!(f, "Fill view with {color}", color = c),
            Self::ForceQuit => write!(f, "Quit view without saving"),
            Self::ForceQuitAll => write!(f, "Quit all views without saving"),
            Self::Help => write!(f, "Toggle help"),
            Self::Map(_, _) => write!(f, "Map a key combination to a command"),
            Self::Mode(m) => write!(f, "Switch session mode to {}", m),
            Self::AddFrame => write!(f, "Add a blank frame to the view"),
            Self::CloneFrame(i) => {
                write!(f, "Clone frame {} and add it to the view", i)
            }
            Self::RemoveFrame => write!(f, "Remove the last frame of the view"),
            Self::Noop => write!(f, "No-op"),
            Self::PaletteAdd(c) => {
                write!(f, "Add {color} to palette", color = c)
            }
            Self::PaletteClear => write!(f, "Clear palette"),
            Self::PaletteSample => write!(f, "Sample palette from view"),
            Self::Pan(x, 0) if *x > 0 => write!(f, "Pan workspace right"),
            Self::Pan(x, 0) if *x < 0 => write!(f, "Pan workspace left"),
            Self::Pan(0, y) if *y > 0 => write!(f, "Pan workspace up"),
            Self::Pan(0, y) if *y < 0 => write!(f, "Pan workspace down"),
            Self::Quit => write!(f, "Quit active view"),
            Self::Redo => write!(f, "Redo view edit"),
            Self::ResizeFrame(_, _) => write!(f, "Resize active view frame"),
            Self::Sampler(_) => write!(f, "Toggle color sampler"),
            Self::Set(s, v) => {
                write!(f, "Set {setting} to {val}", setting = s, val = v)
            }
            Self::Slice(Some(n)) => write!(f, "Slice view into {} frame(s)", n),
            Self::Slice(None) => write!(f, "Reset view slices"),
            Self::Source(_) => write!(f, "Source an rx script (eg. a palette)"),
            Self::SwapColors => {
                write!(f, "Swap foreground & background colors")
            }
            Self::Toggle(s) => {
                write!(f, "Toggle {setting} on/off", setting = s)
            }
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
            _ => write!(f, ""),
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Key {
    Char(char),
    Virtual(platform::Key),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => c.fmt(f),
            Key::Virtual(k) => k.fmt(f),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    U32(u32),
    Float(f64),
    Float2(f32, f32),
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

    pub fn float64(&self) -> f64 {
        if let Value::Float(n) = self {
            return *n;
        }
        panic!("expected {:?} to be a `float`", self);
    }

    pub fn uint64(&self) -> u64 {
        if let Value::U32(n) = self {
            return *n as u64;
        }
        panic!("expected {:?} to be a `uint`", self);
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Bool(_) => "on / off",
            Self::U32(_) => "positive integer, eg. 32",
            Self::Float(_) => "float, eg. 1.33",
            Self::Float2(_, _) => "two floats, eg. 32.0, 48.0",
            Self::Str(_) => "string, eg. \"fnord\"",
            Self::Rgba8(_) => "color, eg. #ffff00",
            Self::Ident(_) => "identifier, eg. fnord",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    Incr,
    Decr,
    Set(f32),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => "on".fmt(f),
            Value::Bool(false) => "off".fmt(f),
            Value::U32(u) => u.fmt(f),
            Value::Float(x) => x.fmt(f),
            Value::Float2(x, y) => write!(f, "{},{}", x, y),
            Value::Str(s) => s.fmt(f),
            Value::Rgba8(c) => c.fmt(f),
            Value::Ident(i) => i.fmt(f),
        }
    }
}

pub struct CommandLine {
    input: String,
}

impl CommandLine {
    const MAX_INPUT: usize = 256;

    pub fn new() -> Self {
        Self {
            input: String::with_capacity(Self::MAX_INPUT),
        }
    }

    pub fn input(&self) -> String {
        if self.input.is_empty() {
            String::new()
        } else {
            self.input.to_string()
        }
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

    pub fn delc(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct Error {
    msg: String,
}

impl Error {
    fn new<S: Into<String>>(msg: S) -> Self {
        Self { msg: msg.into() }
    }

    #[allow(dead_code)]
    fn from<S: Into<String>, E: std::error::Error>(msg: S, err: E) -> Self {
        Self {
            msg: format!("{}: {}", msg.into(), err),
        }
    }
}

impl std::error::Error for Error {}

impl From<&str> for Error {
    fn from(input: &str) -> Self {
        Error::new(input)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.msg.fmt(f)
    }
}

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
            Err(e) => Err(e),
        }
    }
}

impl<'a> Parse<'a> for platform::Key {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (c, p) = p.parse::<char>()?;
        let key: platform::Key = c.into();

        if key == platform::Key::Unknown {
            return Err(Error::new(format!("unknown key {:?}", c)));
        }
        Ok((key, p))
    }
}

impl<'a> Parse<'a> for Mode {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (id, p) = p.identifier()?;
        match id {
            "command" => Ok((Mode::Command, p)),
            "normal" => Ok((Mode::Normal, p)),
            "visual" => Ok((Mode::Visual, p)),
            "present" => Ok((Mode::Present, p)),
            mode => Err(Error::new(format!("unknown mode '{}'", mode))),
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
            "q!" => Ok((Command::ForceQuit, p)),
            "qa!" => Ok((Command::ForceQuitAll, p)),
            "w" => {
                if p.is_empty() {
                    Ok((Command::Write(None), p))
                } else {
                    let (path, p) = p.word()?;
                    Ok((Command::Write(Some(path.to_string())), p))
                }
            }
            "e" => {
                if p.is_empty() {
                    Ok((Command::Edit(Vec::with_capacity(0)), p))
                } else {
                    let mut q = p;
                    let mut edits = Vec::new();

                    loop {
                        if let Ok((path, p)) = q.clone().word() {
                            edits.push(path.to_string());
                            let (_, p) = p.whitespace()?;
                            q = p;
                        } else {
                            break;
                        }
                    }
                    Ok((Command::Edit(edits), q))
                }
            }
            "help" => Ok((Command::Help, p)),
            "fullscreen" => Ok((Command::FullScreen, p)),
            "set" => {
                let (k, p) = p.identifier()?;
                let (_, p) = p.whitespace()?;

                if p.is_empty() {
                    Ok((Command::Set(k.to_string(), Value::Bool(true)), p))
                } else {
                    let (_, p) = p.sigil('=')?;
                    let (_, p) = p.whitespace()?;
                    let (v, p) = p.value()?;
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
                let (v, p) = p.value()?;
                Ok((Command::Echo(v), p))
            }
            "sleep" => {
                let (ms, p) = p.parse::<u32>()?;
                Ok((Command::Sleep(time::Duration::from_millis(ms as u64)), p))
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
                let (path, p) = p.word()?;
                Ok((Command::Source(path.to_string()), p))
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
            "brush" => Ok((Command::Noop, p)),
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
            "mode" => {
                let (mode, p) = p.parse::<Mode>()?;
                Ok((Command::Mode(mode), p))
            }
            "sampler" => Ok((Command::Sampler(true), p)),
            "sampler/off" => Ok((Command::Sampler(false), p)),
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
            "map" => {
                let (key, p) = p.parse::<Key>()?;
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
                Ok((Command::Map(key, Box::new((press, release))), p))
            }
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
            "f/clone" => {
                match p.clone().parse::<i32>().or_else(|_| Ok((-1, p))) {
                    Ok((index, p)) => Ok((Command::CloneFrame(index), p)),
                    Err(e) => Err(e),
                }
            }
            "f/remove" => Ok((Command::RemoveFrame, p)),
            "f/resize" => {
                let ((w, h), p) = p.parse::<(u32, u32)>()?;
                Ok((Command::ResizeFrame(w, h), p))
            }
            "swap" => Ok((Command::SwapColors, p)),
            unrecognized => Err(Error::new(format!(
                "unrecognized command ':{}'",
                unrecognized
            ))),
        }
    }
}

type Result<'a, T> = result::Result<(T, Parser<'a>), Error>;

#[derive(Debug, Clone)]
struct Parser<'a> {
    input: &'a str,
}

trait Parse<'a>: Sized {
    fn parse(input: Parser<'a>) -> Result<'a, Self>;
}

impl<'a> Parse<'a> for u32 {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (s, rest) = p.word()?;

        match u32::from_str(s) {
            Ok(u) => Ok((u, rest)),
            Err(_) => Err(Error::new("error parsing u32")),
        }
    }
}

impl<'a> Parse<'a> for i32 {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (s, rest) = p.word()?;

        match i32::from_str(s) {
            Ok(u) => Ok((u, rest)),
            Err(_) => Err(Error::new("error parsing i32")),
        }
    }
}

impl<'a> Parse<'a> for f64 {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (s, rest) = p.word()?;

        match f64::from_str(s) {
            Ok(u) => Ok((u, rest)),
            Err(_) => Err(Error::new("error parsing f64")),
        }
    }
}

impl<'a> Parse<'a> for (u32, u32) {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (w, p) = p.parse::<u32>()?;
        let (_, p) = p.whitespace()?;
        let (h, p) = p.parse::<u32>()?;

        Ok(((w, h), p))
    }
}

impl<'a> Parse<'a> for (i32, i32) {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (w, p) = p.parse::<i32>()?;
        let (_, p) = p.whitespace()?;
        let (h, p) = p.parse::<i32>()?;

        Ok(((w, h), p))
    }
}

impl<'a> Parse<'a> for char {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        if let Some(c) = p.input.chars().next() {
            Ok((c, Parser::new(&p.input[1..])))
        } else {
            Err(Error::new("error parsing char"))
        }
    }
}

impl<'a> Parse<'a> for Key {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        if let Ok((_, p)) = p.clone().sigil('<') {
            let (key, p) = p.alpha()?;
            let (_, p) = p.sigil('>')?;
            let virt = match key {
                "up" => platform::Key::Up,
                "down" => platform::Key::Down,
                "left" => platform::Key::Left,
                "right" => platform::Key::Right,
                "ctrl" => platform::Key::Control,
                "shift" => platform::Key::Shift,
                "space" => platform::Key::Space,
                "return" => platform::Key::Return,
                "backspace" => platform::Key::Backspace,
                "tab" => platform::Key::Tab,
                other => {
                    return Err(Error::new(format!("unknown key <{}>", other)))
                }
            };
            Ok((Key::Virtual(virt), p))
        } else {
            let (k, p) = p.parse::<platform::Key>()?;
            Ok((Key::Virtual(k), p))
        }
    }
}

impl<'a> Parse<'a> for BrushMode {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (id, p) = p.identifier()?;
        match id {
            "erase" => Ok((BrushMode::Erase, p)),
            "multi" => Ok((BrushMode::Multi, p)),
            "perfect" => Ok((BrushMode::Perfect, p)),
            "xsym" => Ok((BrushMode::XSym, p)),
            "ysym" => Ok((BrushMode::YSym, p)),
            mode => Err(Error::new(format!("unknown brush mode '{}'", mode))),
        }
    }
}

impl<'a> Parse<'a> for Rgba8 {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (s, rest) = p.count(7)?; // Expect 7 characters including the '#'

        match Rgba8::from_str(s) {
            Ok(u) => Ok((u, rest)),
            Err(_) => Err(Error::new(format!("malformed color value `{}`", s))),
        }
    }
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn empty() -> Self {
        Self { input: "" }
    }

    fn finish(self) -> Result<'a, ()> {
        let (_, p) = self.whitespace()?;

        if p.is_empty() {
            Ok(((), Parser::empty()))
        } else {
            Err(Error::new(format!("extraneaous input: `{}`", p.input)))
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.chars().nth(0)
    }

    fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    fn sigil(self, c: char) -> Result<'a, char> {
        if self.input.starts_with(c) {
            Ok((c, Parser::new(&self.input[1..])))
        } else {
            Err(Error::new(format!("expected '{}'", c)))
        }
    }

    fn string(self) -> Result<'a, &'a str> {
        let p = self;

        let (_, p) = p.sigil('"')?;
        let (s, p) = p.until(|c| c == '"')?;
        let (_, p) = p.sigil('"')?;

        Ok((s, p))
    }

    fn alpha(self) -> Result<'a, &'a str> {
        self.expect(|c| c.is_alphanumeric())
    }

    fn comment(self) -> Result<'a, &'a str> {
        let p = self;

        let (_, p) = p.whitespace()?;
        let (_, p) = p.sigil('-')?;
        let (_, p) = p.sigil('-')?;
        let (_, p) = p.whitespace()?;
        let (s, p) = p.leftover()?;

        Ok((s, p))
    }

    fn leftover(self) -> Result<'a, &'a str> {
        Ok((self.input, Parser::empty()))
    }

    fn whitespace(self) -> Result<'a, ()> {
        self.consume(|c| c.is_whitespace())
    }

    fn parse<T: Parse<'a>>(self) -> Result<'a, T> {
        T::parse(self)
    }

    fn word(self) -> Result<'a, &'a str> {
        self.expect(|c| !c.is_whitespace())
    }

    fn count(self, n: usize) -> Result<'a, &'a str> {
        if self.input.len() >= n {
            Ok((&self.input[..n], Parser::new(&self.input[n..])))
        } else {
            Err(Error::new("reached end of input"))
        }
    }

    fn identifier(self) -> Result<'a, &'a str> {
        self.expect(|c| {
            (c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || [':', '/', '_', '+', '-', '!', '?'].contains(&c))
        })
    }

    fn value(self) -> Result<'a, Value> {
        let c = self.peek();

        if c == Some('"') {
            let (v, p) = self.string()?;
            Ok((Value::Str(v.to_string()), p))
        } else if c == Some('#') {
            let (v, p) = self.parse::<Rgba8>()?;
            Ok((Value::Rgba8(v), p))
        } else if c.map_or(false, |c| c.is_digit(10)) {
            if let Ok((v, p)) = self.clone().parse::<u32>() {
                Ok((Value::U32(v), p))
            } else if let Ok((v, p)) = self.clone().parse::<f64>() {
                Ok((Value::Float(v), p))
            } else {
                let (input, _) = self.until(|c| c.is_whitespace())?;
                Err(Error::new(format!("malformed number: `{}`", input)))
            }
        } else {
            let (i, p) = self.identifier()?;
            match i {
                "on" => Ok((Value::Bool(true), p)),
                "off" => Ok((Value::Bool(false), p)),
                _ => Ok((Value::Ident(i.to_string()), p)),
            }
        }
    }

    fn consume<P>(self, predicate: P) -> Result<'a, ()>
    where
        P: Fn(char) -> bool,
    {
        match self.input.find(|c| !predicate(c)) {
            Some(i) => {
                let (_, r) = self.input.split_at(i);
                Ok(((), Parser::new(r)))
            }
            None => Ok(((), Parser::empty())),
        }
    }

    fn until<P>(self, predicate: P) -> Result<'a, &'a str>
    where
        P: Fn(char) -> bool,
    {
        if self.input.is_empty() {
            return Err(Error::new("expected input"));
        }
        match self.input.find(predicate) {
            Some(i) => {
                let (l, r) = self.input.split_at(i);
                Ok((l, Parser::new(r)))
            }
            None => Ok((self.input, Parser::empty())),
        }
    }

    fn expect<P>(self, predicate: P) -> Result<'a, &'a str>
    where
        P: Fn(char) -> bool,
    {
        if self.is_empty() {
            return Err(Error::new("expected input"));
        }
        if !self.input.is_ascii() {
            return Err(Error::new("error parsing non-ASCII characters"));
        }

        let mut index = 0;
        for (i, c) in self.input.chars().enumerate() {
            if predicate(c) {
                index = i;
            } else {
                break;
            }
        }
        let (l, r) = self.input.split_at(index + 1);
        Ok((l, Parser::new(r)))
    }
}
