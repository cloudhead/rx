use crate::autocomplete::{self, Autocomplete, FileCompleter, FileCompleterOpts};
use crate::brush::{Brush, BrushMode};
use crate::history::History;
use crate::parser::*;
use crate::platform;
use crate::session::{Direction, Input, Mode, PanState, Tool, VisualState};
use crate::view::layer::LayerId;

use memoir::traits::Parse;
use memoir::*;

use rgx::kit::Rgba8;
use rgx::rect::Rect;

use std::fmt;
use std::path::Path;

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
    // Brush
    Brush,
    BrushSet(BrushMode),
    BrushToggle(BrushMode),
    BrushSize(Op),
    BrushUnset(BrushMode),

    #[allow(dead_code)]
    Crop(Rect<u32>),
    ChangeDir(Option<String>),
    Echo(Value),

    // Files
    Edit(Vec<String>),
    EditFrames(Vec<String>),
    Write(Option<String>),
    WriteFrames(Option<String>),
    WriteQuit,
    Quit,
    QuitAll,
    ForceQuit,
    ForceQuitAll,
    Source(Option<String>),

    // Frames
    FrameAdd,
    FrameClone(i32),
    FrameRemove,
    FramePrev,
    FrameNext,
    FrameResize(u32, u32),

    // Palette
    PaletteAdd(Rgba8),
    PaletteClear,
    PaletteSample,
    PaletteSort,
    PaletteWrite(String),

    // Navigation
    Pan(i32, i32),
    Zoom(Op),

    // TODO: These operate on the active layer. We should have a command
    // to set the active layer.
    PaintColor(Rgba8, i32, i32),
    PaintForeground(i32, i32),
    PaintBackground(i32, i32),
    PaintPalette(usize, i32, i32),

    // Selection
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

    // Settings
    Set(String, Value),
    Toggle(String),
    Reset,
    Map(Box<KeyMapping>),
    MapClear,

    Slice(Option<usize>),
    Fill(Rgba8),

    SwapColors,

    Mode(Mode),
    Tool(Tool),
    ToolPrev,

    Undo,
    Redo,

    // View
    ViewCenter,
    ViewNext,
    ViewPrev,

    // Layers
    LayerAdd,
    LayerRemove(Option<LayerId>),
    LayerExtend(Option<LayerId>),

    Noop,
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
            Self::EditFrames(_) => write!(f, "Edit path(s) as animation frames"),
            Self::Fill(c) => write!(f, "Fill view with {color}", color = c),
            Self::ForceQuit => write!(f, "Quit view without saving"),
            Self::ForceQuitAll => write!(f, "Quit all views without saving"),
            Self::Map(_) => write!(f, "Map a key combination to a command"),
            Self::MapClear => write!(f, "Clear all key mappings"),
            Self::Mode(Mode::Help) => write!(f, "Toggle help"),
            Self::Mode(m) => write!(f, "Switch to {} mode", m),
            Self::FrameAdd => write!(f, "Add a blank frame to the view"),
            Self::FrameClone(i) => write!(f, "Clone frame {} and add it to the view", i),
            Self::FrameRemove => write!(f, "Remove the last frame of the view"),
            Self::Noop => write!(f, "No-op"),
            Self::PaletteAdd(c) => write!(f, "Add {color} to palette", color = c),
            Self::PaletteClear => write!(f, "Clear palette"),
            Self::PaletteSample => write!(f, "Sample palette from view"),
            Self::PaletteSort => write!(f, "Sort palette colors"),
            Self::Pan(x, 0) if *x > 0 => write!(f, "Pan workspace right"),
            Self::Pan(x, 0) if *x < 0 => write!(f, "Pan workspace left"),
            Self::Pan(0, y) if *y > 0 => write!(f, "Pan workspace up"),
            Self::Pan(0, y) if *y < 0 => write!(f, "Pan workspace down"),
            Self::Pan(x, y) => write!(f, "Pan workspace by {},{}", x, y),
            Self::Quit => write!(f, "Quit active view"),
            Self::QuitAll => write!(f, "Quit all views"),
            Self::Redo => write!(f, "Redo view edit"),
            Self::FrameResize(_, _) => write!(f, "Resize active view frame"),
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
            Self::PaintColor(_, x, y) => write!(f, "Paint {:2},{:2}", x, y),
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
            Command::FrameAdd => format!("f/add"),
            Command::FrameClone(i) => format!("f/clone {}", i),
            Command::FrameRemove => format!("f/remove"),
            Command::Noop => format!(""),
            Command::PaletteAdd(c) => format!("p/add {}", c),
            Command::PaletteClear => format!("p/clear"),
            Command::PaletteWrite(_) => format!("p/write"),
            Command::PaletteSample => format!("p/sample"),
            Command::Pan(x, y) => format!("pan {} {}", x, y),
            Command::Quit => format!("q"),
            Command::Redo => format!("redo"),
            Command::FrameResize(w, h) => format!("f/resize {} {}", w, h),
            Command::Set(s, v) => format!("set {} = {}", s, v),
            Command::Slice(Some(n)) => format!("slice {}", n),
            Command::Slice(None) => format!("slice"),
            Command::Source(Some(path)) => format!("source {}", path),
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

///////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone)]
pub struct KeyMapping {
    pub input: Input,
    pub press: Command,
    pub release: Option<Command>,
    pub modes: Vec<Mode>,
}

impl KeyMapping {
    pub fn parser(modes: &[Mode]) -> Parser<KeyMapping> {
        let modes = modes.to_vec();

        // Prevent stack overflow.
        let press = Parser::new(
            move |input| Commands::default().parser().parse(input),
            "<cmd>",
        );

        // Prevent stack overflow.
        let release = Parser::new(
            move |input| {
                if let Some(i) = input.bytes().position(|c| c == b'}') {
                    match Commands::default().parser().parse(&input[..i]) {
                        Ok((cmd, rest)) if rest.is_empty() => Ok((cmd, &input[i..])),
                        Ok((_, rest)) => {
                            Err((format!("expected {:?}, got {:?}", '}', rest).into(), rest))
                        }
                        Err(err) => Err(err),
                    }
                } else {
                    Err(("unclosed '{' delimiter".into(), input))
                }
            },
            "<cmd>",
        );

        let character = between('\'', '\'', character())
            .map(|c| Input::Character(c))
            .skip(whitespace())
            .then(press.clone())
            .map(|(input, press)| ((input, press), None));
        let key = param::<platform::Key>()
            .map(|k| Input::Key(k))
            .skip(whitespace())
            .then(press)
            .skip(optional(whitespace()))
            .then(optional(between('{', '}', release)));

        character
            .or(key)
            .map(move |((input, press), release)| KeyMapping {
                input,
                press,
                release,
                modes: modes.clone(),
            })
            .label("<key> <cmd>") // TODO: We should provide the full command somehow.
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    U32(u32),
    U32Tuple(u32, u32),
    F32Tuple(f32, f32),
    F64(f64),
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
            Self::F32Tuple(_, _) => "two floats , eg. 32.17, 48.29",
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

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => "on".fmt(f),
            Value::Bool(false) => "off".fmt(f),
            Value::U32(u) => u.fmt(f),
            Value::F64(x) => x.fmt(f),
            Value::U32Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::F32Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::Str(s) => s.fmt(f),
            Value::Rgba8(c) => c.fmt(f),
            Value::Ident(i) => i.fmt(f),
        }
    }
}

impl Parse for Value {
    fn parser() -> Parser<Self> {
        let str_val = quoted().map(Value::Str).label("<string>");
        let rgba8_val = color().map(Value::Rgba8);
        let u32_tuple_val = tuple::<u32>(natural(), natural()).map(|(x, y)| Value::U32Tuple(x, y));
        let u32_val = natural::<u32>().map(Value::U32);
        let f64_tuple_val =
            tuple::<f32>(rational(), rational()).map(|(x, y)| Value::F32Tuple(x, y));
        let f64_val = rational::<f64>().map(Value::F64).label("0.0 .. 4096.0");
        let bool_val = string("on")
            .value(Value::Bool(true))
            .or(string("off").value(Value::Bool(false)))
            .label("on/off");
        let ident_val = identifier().map(Value::Ident);

        greediest(vec![
            rgba8_val,
            u32_tuple_val,
            f64_tuple_val,
            u32_val,
            f64_val,
            bool_val,
            ident_val,
            str_val,
        ])
        .label("<value>")
    }
}

////////////////////////////////////////////////////////////////////////////////

pub struct CommandLine {
    /// The history of commands entered.
    pub history: History,
    /// Command auto-complete.
    pub autocomplete: Autocomplete<CommandCompleter>,
    /// Input cursor position.
    pub cursor: usize,
    /// Parser.
    pub parser: Parser<Command>,
    /// Commands.
    pub commands: Commands,
    /// The current input string displayed to the user.
    input: String,
    /// File extensions supported.
    extensions: Vec<String>,
}

impl CommandLine {
    const MAX_INPUT: usize = 256;

    pub fn new<P: AsRef<Path>>(cwd: P, history_path: P, extensions: &[&str]) -> Self {
        let cmds = Commands::default();

        Self {
            input: String::with_capacity(Self::MAX_INPUT),
            cursor: 0,
            parser: cmds.line_parser(),
            commands: cmds,
            history: History::new(history_path, 1024),
            autocomplete: Autocomplete::new(CommandCompleter::new(cwd, extensions)),
            extensions: extensions.iter().map(|e| (*e).into()).collect(),
        }
    }

    pub fn set_cwd(&mut self, path: &Path) {
        let exts: Vec<_> = self.extensions.iter().map(|s| s.as_str()).collect();
        self.autocomplete = Autocomplete::new(CommandCompleter::new(path, exts.as_slice()));
    }

    pub fn parse(&self, input: &str) -> Result<Command, Error> {
        match self.parser.parse(input) {
            Ok((cmd, _)) => Ok(cmd),
            Err((err, _)) => Err(err),
        }
    }

    pub fn input(&self) -> String {
        self.input.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    pub fn history_prev(&mut self) {
        let prefix = self.prefix();

        if let Some(entry) = self.history.prev(&prefix).map(str::to_owned) {
            self.replace(&entry);
        }
    }

    pub fn history_next(&mut self) {
        let prefix = self.prefix();

        if let Some(entry) = self.history.next(&prefix).map(str::to_owned) {
            self.replace(&entry);
        } else {
            self.reset();
        }
    }

    pub fn completion_next(&mut self) {
        let prefix = self.prefix();

        if let Some((completion, range)) = self.autocomplete.next(&prefix, self.cursor) {
            // Replace old completion with new one.
            self.cursor = range.start + completion.len();
            self.input.replace_range(range, &completion);
        }
    }

    pub fn cursor_backward(&mut self) -> Option<char> {
        if let Some(c) = self.peek_back() {
            let cursor = self.cursor - c.len_utf8();

            // Don't allow deleting the `:` prefix of the command.
            if c != ':' || cursor > 0 {
                self.cursor = cursor;
                self.autocomplete.invalidate();
                return Some(c);
            }
        }
        None
    }

    pub fn cursor_forward(&mut self) -> Option<char> {
        if let Some(c) = self.input[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
            self.autocomplete.invalidate();
            Some(c)
        } else {
            None
        }
    }

    pub fn putc(&mut self, c: char) {
        if self.input.len() + c.len_utf8() > self.input.capacity() {
            return;
        }
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.autocomplete.invalidate();
    }

    pub fn puts(&mut self, s: &str) {
        // TODO: Check capacity.
        self.input.push_str(s);
        self.cursor += s.len();
        self.autocomplete.invalidate();
    }

    pub fn delc(&mut self) {
        match self.peek_back() {
            // Don't allow deleting the ':' unless it's the last remaining character.
            Some(c) if self.cursor > 1 || self.input.len() == 1 => {
                self.cursor -= c.len_utf8();
                self.input.remove(self.cursor);
                self.autocomplete.invalidate();
            }
            _ => {}
        }
    }

    pub fn clear(&mut self) {
        self.cursor = 0;
        self.input.clear();
        self.history.reset();
        self.autocomplete.invalidate();
    }

    ////////////////////////////////////////////////////////////////////////////

    fn replace(&mut self, s: &str) {
        // We don't re-assign `input` here, because it
        // has a fixed capacity we want to preserve.
        self.input.clear();
        self.input.push_str(s);
        self.autocomplete.invalidate();
    }

    fn reset(&mut self) {
        self.clear();
        self.putc(':');
    }

    fn prefix(&self) -> String {
        self.input[..self.cursor].to_owned()
    }

    #[cfg(test)]
    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn peek_back(&self) -> Option<char> {
        self.input[..self.cursor].chars().next_back()
    }
}

pub struct Commands {
    commands: Vec<(&'static str, &'static str, Parser<Command>)>,
}

impl Commands {
    pub fn new() -> Self {
        Self {
            commands: vec![(
                "#",
                "Add color to palette",
                color().map(Command::PaletteAdd),
            )],
        }
    }

    pub fn parser(&self) -> Parser<Command> {
        use std::iter;

        let noop = expect(|s| s.is_empty(), "<empty>").value(Command::Noop);
        let commands = self.commands.iter().map(|(_, _, v)| v.clone());
        let choices = commands.chain(iter::once(noop)).collect();

        symbol(':')
            .then(
                choice(choices).or(peek(
                    until(hush(whitespace()).or(end()))
                        .try_map(|cmd| Err(format!("unknown command: {}", cmd))),
                )),
            )
            .map(|(_, cmd)| cmd)
    }

    pub fn line_parser(&self) -> Parser<Command> {
        self.parser()
            .skip(optional(whitespace()))
            .skip(optional(comment()))
            .end()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(&'static str, &'static str, Parser<Command>)> {
        self.commands.iter()
    }

    ///////////////////////////////////////////////////////////////////////////

    fn command<F>(mut self, name: &'static str, help: &'static str, f: F) -> Self
    where
        F: Fn(Parser<String>) -> Parser<Command>,
    {
        let cmd = peek(
            string(name)
                .followed_by(hush(whitespace()) / end())
                .skip(optional(whitespace())),
        )
        .label(name);

        self.commands.push((name, help, f(cmd)));
        self
    }
}

impl Default for Commands {
    fn default() -> Self {
        Self::new()
            .command("q", "Quit view", |p| p.value(Command::Quit))
            .command("qa", "Quit all views", |p| p.value(Command::QuitAll))
            .command("q!", "Force quit view", |p| p.value(Command::ForceQuit))
            .command("qa!", "Force quit all views", |p| {
                p.value(Command::ForceQuitAll)
            })
            .command("wq", "Write & quit view", |p| p.value(Command::WriteQuit))
            .command("x", "Write & quit view", |p| p.value(Command::WriteQuit))
            .command("w", "Write view", |p| {
                p.then(optional(path()))
                    .map(|(_, path)| Command::Write(path))
            })
            .command("w/frames", "Write view as individual frames", |p| {
                p.then(optional(path()))
                    .map(|(_, dir)| Command::WriteFrames(dir))
            })
            .command("e", "Edit path(s)", |p| {
                p.then(paths()).map(|(_, paths)| Command::Edit(paths))
            })
            .command("e/frames", "Edit frames as view", |p| {
                p.then(paths()).map(|(_, paths)| Command::EditFrames(paths))
            })
            .command("help", "Display help", |p| {
                p.value(Command::Mode(Mode::Help))
            })
            .command("set", "Set setting to value", |p| {
                p.then(setting())
                    .skip(optional(whitespace()))
                    .then(optional(
                        symbol('=')
                            .skip(optional(whitespace()))
                            .then(Value::parser())
                            .map(|(_, v)| v),
                    ))
                    .map(|((_, k), v)| Command::Set(k, v.unwrap_or(Value::Bool(true))))
            })
            .command("unset", "Set setting to `off`", |p| {
                p.then(setting())
                    .map(|(_, k)| Command::Set(k, Value::Bool(false)))
            })
            .command("toggle", "Toggle setting", |p| {
                p.then(setting()).map(|(_, k)| Command::Toggle(k))
            })
            .command("echo", "Echo setting or value", |p| {
                p.then(Value::parser()).map(|(_, v)| Command::Echo(v))
            })
            .command("slice", "Slice view into <n> frames", |p| {
                p.then(optional(natural::<usize>().label("<n>")))
                    .map(|(_, n)| Command::Slice(n))
            })
            .command(
                "source",
                "Source an rx script (eg. palette or config)",
                |p| p.then(optional(path())).map(|(_, p)| Command::Source(p)),
            )
            .command("cd", "Change current directory", |p| {
                p.then(optional(path())).map(|(_, p)| Command::ChangeDir(p))
            })
            .command("zoom", "Zoom view", |p| {
                p.then(
                    peek(rational::<f32>().label("<level>"))
                        .try_map(|z| {
                            if z >= 1.0 {
                                Ok(Command::Zoom(Op::Set(z)))
                            } else {
                                Err("zoom level must be >= 1.0")
                            }
                        })
                        .or(symbol('+')
                            .value(Command::Zoom(Op::Incr))
                            .or(symbol('-').value(Command::Zoom(Op::Decr)))
                            .or(fail("couldn't parse zoom parameter")))
                        .label("+/-"),
                )
                .map(|(_, cmd)| cmd)
            })
            .command("brush/size", "Set brush size", |p| {
                p.then(
                    natural::<usize>()
                        .label("<size>")
                        .map(|z| Command::BrushSize(Op::Set(z as f32)))
                        .or(symbol('+')
                            .value(Command::BrushSize(Op::Incr))
                            .or(symbol('-').value(Command::BrushSize(Op::Decr)))
                            .or(fail("couldn't parse brush size parameter")))
                        .label("+/-"),
                )
                .map(|(_, cmd)| cmd)
            })
            .command(
                "brush/set",
                "Set brush mode, eg. `xsym` for x-symmetry",
                |p| {
                    p.then(param::<BrushMode>())
                        .map(|(_, m)| Command::BrushSet(m))
                },
            )
            .command("brush/unset", "Unset brush mode", |p| {
                p.then(param::<BrushMode>())
                    .map(|(_, m)| Command::BrushUnset(m))
            })
            .command("brush/toggle", "Toggle brush mode", |p| {
                p.then(param::<BrushMode>())
                    .map(|(_, m)| Command::BrushToggle(m))
            })
            .command("brush", "Switch to default brush", |p| {
                p.value(Command::Tool(Tool::Brush(Brush::default())))
            })
            .command("mode", "Set session mode, eg. `visual` or `normal`", |p| {
                p.then(param::<Mode>()).map(|(_, m)| Command::Mode(m))
            })
            .command("visual", "Set session mode to visual", |p| {
                p.map(|_| Command::Mode(Mode::Visual(VisualState::default())))
            })
            .command("sampler/off", "Switch the sampler tool off", |p| {
                p.value(Command::ToolPrev)
            })
            .command("sampler", "Switch to the sampler tool", |p| {
                p.value(Command::Tool(Tool::Sampler))
            })
            .command("v/next", "Activate the next view", |p| {
                p.value(Command::ViewNext)
            })
            .command("v/prev", "Activate the previous view", |p| {
                p.value(Command::ViewPrev)
            })
            .command("v/center", "Center the active view", |p| {
                p.value(Command::ViewCenter)
            })
            .command("v/clear", "Clear the active view", |p| {
                choice(vec![
                    peek(p.clone().then(color()).map(|(_, rgba)| Command::Fill(rgba))),
                    p.value(Command::Fill(Rgba8::TRANSPARENT)),
                ])
            })
            .command("pan", "Switch to the pan tool", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::Pan(x, y))
            })
            .command("map", "Map keys to a command in all modes", |p| {
                p.then(KeyMapping::parser(&[
                    Mode::Normal,
                    Mode::Visual(VisualState::selecting()),
                    Mode::Visual(VisualState::Pasting),
                ]))
                .map(|(_, km)| Command::Map(Box::new(km)))
            })
            .command("map/visual", "Map keys to a command in visual mode", |p| {
                p.then(KeyMapping::parser(&[
                    Mode::Visual(VisualState::selecting()),
                    Mode::Visual(VisualState::Pasting),
                ]))
                .map(|(_, km)| Command::Map(Box::new(km)))
            })
            .command("map/normal", "Map keys to a command in normal mode", |p| {
                p.then(KeyMapping::parser(&[Mode::Normal]))
                    .map(|(_, km)| Command::Map(Box::new(km)))
            })
            .command("map/help", "Map keys to a command in help mode", |p| {
                p.then(KeyMapping::parser(&[Mode::Help]))
                    .map(|(_, km)| Command::Map(Box::new(km)))
            })
            .command("map/clear!", "Clear all key mappings", |p| {
                p.value(Command::MapClear)
            })
            .command("p/add", "Add a color to the palette", |p| {
                p.then(color()).map(|(_, rgba)| Command::PaletteAdd(rgba))
            })
            .command("p/clear", "Clear the color palette", |p| {
                p.value(Command::PaletteClear)
            })
            .command(
                "p/sample",
                "Sample palette colors from the active view",
                |p| p.value(Command::PaletteSample),
            )
            .command("p/sort", "Sort the palette colors", |p| {
                p.value(Command::PaletteSort)
            })
            .command("p/write", "Write the color palette to a file", |p| {
                p.then(path()).map(|(_, path)| Command::PaletteWrite(path))
            })
            .command("undo", "Undo the last edit", |p| p.value(Command::Undo))
            .command("redo", "Redo the last edit", |p| p.value(Command::Redo))
            .command("f/add", "Add a blank frame to the active view", |p| {
                p.value(Command::FrameAdd)
            })
            .command("f/clone", "Clone a frame and add it to the view", |p| {
                p.then(optional(integer::<i32>().label("<index>")))
                    .map(|(_, index)| Command::FrameClone(index.unwrap_or(-1)))
            })
            .command(
                "f/remove",
                "Remove the last frame from the active view",
                |p| p.value(Command::FrameRemove),
            )
            .command("f/prev", "Navigate to previous frame", |p| {
                p.value(Command::FramePrev)
            })
            .command("f/next", "Navigate to next frame", |p| {
                p.value(Command::FrameNext)
            })
            .command("f/resize", "Resize the active view frame(s)", |p| {
                p.then(tuple::<u32>(
                    natural().label("<width>"),
                    natural().label("<height>"),
                ))
                .map(|(_, (w, h))| Command::FrameResize(w, h))
            })
            .command("l/add", "Add a new layer to the active view", |p| {
                p.value(Command::LayerAdd)
            })
            .command("tool", "Switch tool", |p| {
                p.then(word().label("pan/brush/sampler/.."))
                    .try_map(|(_, t)| match t.as_str() {
                        "pan" => Ok(Command::Tool(Tool::Pan(PanState::default()))),
                        "brush" => Ok(Command::Tool(Tool::Brush(Brush::default()))),
                        "sampler" => Ok(Command::Tool(Tool::Sampler)),
                        _ => Err(format!("unknown tool {:?}", t)),
                    })
            })
            .command("tool/prev", "Switch to previous tool", |p| {
                p.value(Command::ToolPrev)
            })
            .command("swap", "Swap foreground and background colors", |p| {
                p.value(Command::SwapColors)
            })
            .command("reset!", "Reset all settings to defaults", |p| {
                p.value(Command::Reset)
            })
            .command("selection/move", "Move selection", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionMove(x, y))
            })
            .command("selection/resize", "Resize selection", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionResize(x, y))
            })
            .command("selection/yank", "Yank/copy selection content", |p| {
                p.value(Command::SelectionYank)
            })
            .command("selection/cut", "Cut selection content", |p| {
                p.value(Command::SelectionCut)
            })
            .command("selection/paste", "Paste into selection", |p| {
                p.value(Command::SelectionPaste)
            })
            .command("selection/expand", "Expand selection", |p| {
                p.value(Command::SelectionExpand)
            })
            .command("selection/erase", "Erase selection contents", |p| {
                p.value(Command::SelectionErase)
            })
            .command("selection/offset", "Offset selection bounds", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionOffset(x, y))
            })
            .command("selection/jump", "Translate selection by one frame", |p| {
                p.then(param::<Direction>())
                    .map(|(_, dir)| Command::SelectionJump(dir))
            })
            .command("selection/fill", "Fill selection with color", |p| {
                p.then(optional(color()))
                    .map(|(_, rgba)| Command::SelectionFill(rgba))
            })
            .command("paint/color", "Paint color", |p| {
                p.then(color())
                    .skip(whitespace())
                    .then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|((_, rgba), (x, y))| Command::PaintColor(rgba, x, y))
            })
            .command("paint/fg", "Paint foreground color", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::PaintForeground(x, y))
            })
            .command("paint/bg", "Paint background color", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::PaintBackground(x, y))
            })
            .command("paint/p", "Paint palette color", |p| {
                p.then(natural::<usize>())
                    .skip(whitespace())
                    .then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|((_, i), (x, y))| Command::PaintPalette(i, x, y))
            })
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct CommandCompleter {
    file_completer: FileCompleter,
}

impl CommandCompleter {
    fn new<P: AsRef<Path>>(cwd: P, exts: &[&str]) -> Self {
        Self {
            file_completer: FileCompleter::new(cwd, exts),
        }
    }
}

impl autocomplete::Completer for CommandCompleter {
    type Options = ();

    fn complete(&self, input: &str, _opts: ()) -> Vec<String> {
        let p = Commands::default().parser();

        match p.parse(input) {
            Ok((cmd, _)) => match cmd {
                Command::ChangeDir(path) | Command::WriteFrames(path) => self.complete_path(
                    path.as_ref(),
                    input,
                    FileCompleterOpts { directories: true },
                ),
                Command::Source(path) | Command::Write(path) => {
                    self.complete_path(path.as_ref(), input, Default::default())
                }
                Command::Edit(paths) | Command::EditFrames(paths) => {
                    self.complete_path(paths.last(), input, Default::default())
                }
                _ => vec![],
            },
            Err(_) => vec![],
        }
    }
}

impl CommandCompleter {
    fn complete_path(
        &self,
        path: Option<&String>,
        input: &str,
        opts: FileCompleterOpts,
    ) -> Vec<String> {
        use crate::autocomplete::Completer;

        let empty = "".to_owned();
        let path = path.unwrap_or(&empty);

        // If there's whitespace between the path and the cursor, don't complete the path.
        // Instead, complete as if the input was empty.
        match input.chars().next_back() {
            Some(c) if c.is_whitespace() => self.file_completer.complete("", opts),
            _ => self.file_completer.complete(path, opts),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{fs, fs::File};

    #[test]
    fn test_command_completer() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        for file_name in &["one.png", "two.png", "three.png"] {
            let path = tmp.path().join(file_name);
            File::create(path).unwrap();
        }
        for file_name in &["four.png", "five.png", "six.png"] {
            let path = tmp.path().join("assets").join(file_name);
            File::create(path).unwrap();
        }

        let cc = CommandCompleter::new(tmp.path(), &["png"]);
        let mut auto = Autocomplete::new(cc);

        assert_eq!(auto.next(":e |", 3), Some(("three.png".to_owned(), 3..3)));
        auto.invalidate();
        assert_eq!(
            auto.next(":e |one.png", 3),
            Some(("three.png".to_owned(), 3..3))
        );

        auto.invalidate();
        assert_eq!(
            auto.next(":e one.png | two.png", 11),
            Some(("three.png".to_owned(), 11..11))
        );
        assert_eq!(
            auto.next(":e one.png three.png| two.png", 20),
            Some(("two.png".to_owned(), 11..20))
        );
        assert_eq!(
            auto.next(":e one.png two.png| two.png", 18),
            Some(("one.png".to_owned(), 11..18))
        );

        auto.invalidate();
        assert_eq!(
            auto.next(":e assets/|", 10),
            Some(("six.png".to_owned(), 10..10))
        );
    }

    #[test]
    fn test_command_line() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        for file_name in &["one.png", "two.png", "three.png"] {
            let path = tmp.path().join(file_name);
            File::create(path).unwrap();
        }
        for file_name in &["four.png", "five.png"] {
            let path = tmp.path().join("assets").join(file_name);
            File::create(path).unwrap();
        }

        let mut cli = CommandLine::new(tmp.path(), &tmp.path().join(".history"), &["png"]);

        cli.puts(":e one");
        cli.completion_next();
        assert_eq!(cli.input(), ":e one.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e one.png");

        cli.clear();
        cli.puts(":e ");
        cli.completion_next();
        assert_eq!(cli.input(), ":e three.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e two.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e one.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets");

        cli.putc('/');
        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/four.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png");

        cli.putc(' ');
        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png three.png");

        cli.putc(' ');
        cli.putc('t');
        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png three.png three.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png three.png two.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png three.png three.png");

        for _ in 0..10 {
            cli.cursor_backward();
        }
        cli.putc(' ');
        cli.putc('o');
        cli.completion_next();
        assert_eq!(
            cli.input(),
            ":e assets/five.png three.png one.png three.png"
        );

        cli.clear();
        cli.puts(":e assets");
        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/");

        cli.clear();
        cli.puts(":e asset");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/");

        cli.completion_next();
        assert_eq!(cli.input(), ":e assets/five.png");
    }

    #[test]
    fn test_command_line_change_dir() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        for file_name in &["four.png", "five.png"] {
            let path = tmp.path().join("assets").join(file_name);
            File::create(path).unwrap();
        }

        let mut cli = CommandLine::new(tmp.path(), Path::new("/dev/null"), &["png"]);

        cli.set_cwd(tmp.path().join("assets/").as_path());
        cli.puts(":e ");

        cli.completion_next();
        assert_eq!(cli.input(), ":e five.png");

        cli.completion_next();
        assert_eq!(cli.input(), ":e four.png");
    }

    #[test]
    fn test_command_line_cd() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        fs::create_dir(tmp.path().join("assets").join("1")).unwrap();
        fs::create_dir(tmp.path().join("assets").join("2")).unwrap();
        File::create(tmp.path().join("assets").join("rx.png")).unwrap();

        let mut cli = CommandLine::new(tmp.path(), Path::new("/dev/null"), &["png"]);

        cli.clear();
        cli.puts(":cd assets/");

        cli.completion_next();
        assert_eq!(cli.input(), ":cd assets/2");

        cli.completion_next();
        assert_eq!(cli.input(), ":cd assets/1");

        cli.completion_next();
        assert_eq!(cli.input(), ":cd assets/2");
    }

    #[test]
    fn test_command_line_cursor() {
        let mut cli = CommandLine::new("/dev/null", "/dev/null", &[]);

        cli.puts(":echo");
        cli.delc();
        assert_eq!(cli.input(), ":ech");
        cli.delc();
        assert_eq!(cli.input(), ":ec");
        cli.delc();
        assert_eq!(cli.input(), ":e");
        cli.delc();
        assert_eq!(cli.input(), ":");
        cli.delc();
        assert_eq!(cli.input(), "");

        cli.clear();
        cli.puts(":e");

        assert_eq!(cli.peek(), None);
        cli.cursor_backward();

        assert_eq!(cli.peek(), Some('e'));
        cli.cursor_backward();

        assert_eq!(cli.peek(), Some('e'));
        assert_eq!(cli.peek_back(), Some(':'));

        cli.delc();
        assert_eq!(cli.input(), ":e");
    }

    #[test]
    fn test_parser() {
        let p = Commands::default().line_parser();

        assert_eq!(
            p.parse(":set foo = value"),
            Ok((
                Command::Set("foo".to_owned(), Value::Ident(String::from("value"))),
                ""
            ))
        );
        assert_eq!(
            p.parse(":set scale = 1.0"),
            Ok((Command::Set("scale".to_owned(), Value::F64(1.0)), ""))
        );
        assert_eq!(
            p.parse(":set foo=value"),
            Ok((
                Command::Set("foo".to_owned(), Value::Ident(String::from("value"))),
                ""
            ))
        );
        assert_eq!(
            p.parse(":set foo"),
            Ok((Command::Set("foo".to_owned(), Value::Bool(true)), ""))
        );

        assert_eq!(
            param::<platform::Key>()
                .parse("<hello>")
                .unwrap_err()
                .0
                .to_string(),
            "unknown key <hello>"
        );

        assert_eq!(p.parse(":").unwrap(), (Command::Noop, ""));
    }

    #[test]
    fn test_echo_command() {
        let p = Commands::default().line_parser();

        p.parse(":echo 42").unwrap();
        p.parse(":echo \"hello.\"").unwrap();
        p.parse(":echo \"\"").unwrap();
    }

    #[test]
    fn test_zoom_command() {
        let p = Commands::default().line_parser();

        assert!(p.parse(":zoom -").is_ok());
        assert!(p.parse(":zoom 3.0").is_ok());
        assert!(p.parse(":zoom -1.0").is_err());
    }

    #[test]
    fn test_vclear_commands() {
        let p = Commands::default().line_parser();

        p.parse(":v/clear").unwrap();
        p.parse(":v/clear #ff00ff").unwrap();
    }

    #[test]
    fn test_unknown_command() {
        let p = Commands::default().line_parser();

        let (err, rest) = p.parse(":fnord").unwrap_err();
        assert_eq!(rest, "fnord");
        assert_eq!(err.to_string(), "unknown command: fnord");

        let (err, rest) = p.parse(":mode fnord").unwrap_err();
        assert_eq!(rest, "fnord");
        assert_eq!(err.to_string(), "unknown mode: fnord");
    }

    #[test]
    fn test_keymapping_parser() {
        let p = string("map")
            .skip(whitespace())
            .then(KeyMapping::parser(&[]));

        let (_, rest) = p.parse("map <tab> :q! {:q}").unwrap();
        assert_eq!(rest, "");

        let (_, rest) = p
            .parse("map <tab> :brush/set erase {:brush/unset erase}")
            .unwrap();
        assert_eq!(rest, "");

        let (_, rest) = p.parse("map <ctrl> :tool sampler {:tool/prev}").unwrap();
        assert_eq!(rest, "");
    }

    #[test]
    fn tes_value_parser() {
        let p = Value::parser();

        assert_eq!(p.parse("1.0 2.0").unwrap(), (Value::F32Tuple(1.0, 2.0), ""));
        assert_eq!(p.parse("1.0").unwrap(), (Value::F64(1.0), ""));
        assert_eq!(p.parse("1").unwrap(), (Value::U32(1), ""));
        assert_eq!(p.parse("1 2").unwrap(), (Value::U32Tuple(1, 2), ""));
        assert_eq!(p.parse("on").unwrap(), (Value::Bool(true), ""));
        assert_eq!(p.parse("off").unwrap(), (Value::Bool(false), ""));
        assert_eq!(
            p.parse("#ff00ff").unwrap(),
            (Value::Rgba8(Rgba8::new(0xff, 0x0, 0xff, 0xff)), "")
        );
    }

    #[test]
    fn test_parser_errors() {
        let p = Commands::default().line_parser();

        let (err, _) = p
            .parse(":map <ctrl> :tool sampler {:tool/prev")
            .unwrap_err();
        assert_eq!(err.to_string(), "unclosed '{' delimiter".to_string());

        let (err, _) = p.parse(":map <ctrl> :tool sampler :tool/prev").unwrap_err();
        assert_eq!(
            err.to_string(),
            "extraneous input found: :tool/prev".to_string()
        );
    }
}
