use std::fmt;
use std::path::PathBuf;

use memoir::traits::Parse;
use memoir::*;

use crate::app::brush;

use crate::app::keyboard;
use crate::app::script::parsers;
use crate::app::script::parsers::*;
use crate::app::script::value::Value;
use crate::app::{Direction, Mode, Tool, VisualState};

use crate::gfx::prelude::*;

#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    Incr,
    Decr,
    Set(f32),
}

#[derive(PartialEq, Debug, Clone)]
pub struct KeyMapping {
    pub input: keyboard::Input,
    pub press: Command,
    pub release: Option<Command>,
    pub modes: Vec<Mode>,
}

impl KeyMapping {
    pub fn parser(modes: &[Mode]) -> Parser<KeyMapping> {
        let modes = modes.to_vec();

        // Prevent stack overflow.
        let press = Parser::new(move |input| Command::parser().parse(input), "<cmd>");

        // Prevent stack overflow.
        let release = Parser::new(
            move |input| {
                if let Some(i) = input.bytes().position(|c| c == b'}') {
                    match Command::parser().parse(&input[..i]) {
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
            .map(keyboard::Input::Character)
            .skip(whitespace())
            .then(press.clone())
            .map(|(input, press)| ((input, press), None));
        let key = parsers::key()
            .map(keyboard::Input::Key)
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

/// User command. Most of the interactions available to
/// the user are modeled as commands that are processed
/// by the session.
#[derive(PartialEq, Debug, Clone)]
pub enum Command {
    // Brush
    Brush,
    BrushMode(brush::Mode),
    BrushSet(brush::Modifier),
    BrushToggle(brush::Modifier),
    BrushSize(Op),
    BrushUnset(brush::Modifier),

    #[allow(dead_code)]
    Crop(Rect<u32>),
    ChangeDir(Option<PathBuf>),
    Echo(Value),

    // Files
    Edit(Vec<PathBuf>),
    Export(Option<u32>, PathBuf),
    Write(Option<PathBuf>),
    WriteQuit,
    Quit,
    QuitAll,
    ForceQuit,
    ForceQuitAll,
    Source(Option<PathBuf>),

    // Frames
    FrameAdd,
    FrameClone(i32),
    FrameRemove,
    FramePrev,
    FrameNext,
    FrameResize {
        size: Size<u32>,
    },

    // Palette
    PaletteAdd(Rgba8),
    PaletteClear,
    PaletteGradient(Rgba8, Rgba8, usize),
    PaletteSample,
    PaletteSort,
    PaletteWrite(PathBuf),

    // Navigation
    Pan(i32, i32),
    Zoom(Op),

    PaintColor(Rgba8, i32, i32),
    PaintForeground(i32, i32),
    PaintBackground(i32, i32),
    PaintPalette(usize, i32, i32),
    PaintLine(Rgba8, i32, i32, i32, i32),

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
    SelectionFlip(Axis),

    // Settings
    Set(String, Value),
    Toggle(String),
    Reset,
    Map(Box<KeyMapping>),
    MapClear,

    Slice(Option<usize>),
    Fill(Option<Rgba8>),

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

    Noop,
}

impl Command {
    pub fn repeats(&self) -> bool {
        matches!(
            self,
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
                | Self::SelectionOffset(_, _)
        )
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
            Self::Fill(Some(c)) => write!(f, "Fill view with {color}", color = c),
            Self::Fill(None) => write!(f, "Fill view with background color"),
            Self::ForceQuit => write!(f, "Quit view without saving"),
            Self::ForceQuitAll => write!(f, "Quit all views without saving"),
            Self::Map(_) => write!(f, "Map a key combination to a command"),
            Self::MapClear => write!(f, "Clear all key mappings"),
            Self::Mode(Mode::Help) => write!(f, "Toggle help"),
            Self::Mode(m) => write!(f, "Switch to {} mode", m),
            Self::FrameAdd => write!(f, "Add a blank frame to the view"),
            Self::FrameClone(i) => write!(f, "Clone frame {} and add it to the view", i),
            Self::FrameRemove => write!(f, "Remove the last frame of the view"),
            Self::FramePrev => write!(f, "Navigate to previous frame"),
            Self::FrameNext => write!(f, "Navigate to next frame"),
            Self::Noop => write!(f, "No-op"),
            Self::PaletteAdd(c) => write!(f, "Add {color} to palette", color = c),
            Self::PaletteClear => write!(f, "Clear palette"),
            Self::PaletteGradient(cs, ce, n) => {
                write!(f, "Create {n} colors gradient from {cs} to {ce}")
            }

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
            Self::FrameResize { .. } => write!(f, "Resize active view frame"),
            Self::Tool(Tool::Pan { .. }) => write!(f, "Pan tool"),
            Self::Tool(Tool::Brush) => write!(f, "Brush tool"),
            Self::Tool(Tool::Sampler) => write!(f, "Color sampler tool"),
            Self::Tool(Tool::Bucket) => write!(f, "Flood fill tool"),
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
            Self::SelectionFlip(Axis::Horizontal) => write!(f, "Flip selection horizontally"),
            Self::SelectionFlip(Axis::Vertical) => write!(f, "Flip selection vertically"),
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
            Command::Fill(Some(c)) => format!("v/fill {}", c),
            Command::Fill(None) => format!("v/fill"),
            Command::ForceQuit => format!("q!"),
            Command::ForceQuitAll => format!("qa!"),
            Command::Map(_) => format!("map <key> <command> {{<command>}}"),
            Command::Mode(m) => format!("mode {}", m),
            Command::FrameAdd => format!("f/add"),
            Command::FrameClone(i) => format!("f/clone {}", i),
            Command::FrameRemove => format!("f/remove"),
            Command::Export(None, path) => format!("export {}", path.display()),
            Command::Export(Some(s), path) => format!("export @{}x {}", s, path.display()),
            Command::Noop => format!(""),
            Command::PaletteAdd(c) => format!("p/add {}", c),
            Command::PaletteClear => format!("p/clear"),
            Command::PaletteWrite(_) => format!("p/write"),
            Command::PaletteSample => format!("p/sample"),
            Command::PaletteGradient(cs, ce, n) => format!("p/gradient {} {} {}", cs, ce, n),
            Command::Pan(x, y) => format!("pan {} {}", x, y),
            Command::Quit => format!("q"),
            Command::Redo => format!("redo"),
            Command::FrameResize { size } => format!("f/resize {} {}", size.w, size.h),
            Command::Set(s, v) => format!("set {} = {}", s, v),
            Command::Slice(Some(n)) => format!("slice {}", n),
            Command::Slice(None) => format!("slice"),
            Command::Source(Some(path)) => format!("source {}", path.display()),
            Command::SwapColors => format!("swap"),
            Command::Toggle(s) => format!("toggle {}", s),
            Command::Undo => format!("undo"),
            Command::ViewCenter => format!("v/center"),
            Command::ViewNext => format!("v/next"),
            Command::ViewPrev => format!("v/prev"),
            Command::Write(None) => format!("w"),
            Command::Write(Some(path)) => format!("w {}", path.display()),
            Command::WriteQuit => format!("wq"),
            Command::Zoom(Op::Incr) => format!("v/zoom +"),
            Command::Zoom(Op::Decr) => format!("v/zoom -"),
            Command::Zoom(Op::Set(z)) => format!("v/zoom {}", z),
            _ => unimplemented!(),
        }
    }
}

impl Parse for Command {
    fn parser() -> Parser<Command> {
        use std::iter;

        fn command<F>(
            name: &'static str,
            help: &'static str,
            f: F,
        ) -> (&'static str, &'static str, Parser<Command>)
        where
            F: Fn(Parser<String>) -> Parser<Command>,
        {
            let cmd = peek(
                string(name)
                    .followed_by(hush(whitespace()) / end())
                    .skip(optional(whitespace())),
            )
            .label(name);

            (name, help, f(cmd))
        }

        let noop = expect(|s| s.is_empty(), "<empty>").value(Command::Noop);
        let commands = [
            command("q", "Quit view", |p| p.value(Command::Quit)),
            command("q", "Quit view", |p| p.value(Command::Quit)),
            command("qa", "Quit all views", |p| p.value(Command::QuitAll)),
            command("q!", "Force quit view", |p| p.value(Command::ForceQuit)),
            command("qa!", "Force quit all views", |p| {
                p.value(Command::ForceQuitAll)
            }),
            command("export", "Export view", |p| {
                p.then(optional(scale().skip(whitespace())).then(path()))
                    .map(|(_, (scale, path))| Command::Export(scale, path))
            }),
            command("wq", "Write & quit view", |p| p.value(Command::WriteQuit)),
            command("x", "Write & quit view", |p| p.value(Command::WriteQuit)),
            command("w", "Write view", |p| {
                p.then(optional(path()))
                    .map(|(_, path)| Command::Write(path))
            }),
            command("e", "Edit path(s)", |p| {
                p.then(paths()).map(|(_, paths)| Command::Edit(paths))
            }),
            command("help", "Display help", |p| {
                p.value(Command::Mode(Mode::Help))
            }),
            command("set", "Set setting to value", |p| {
                p.then(setting())
                    .skip(optional(whitespace()))
                    .then(optional(
                        symbol('=')
                            .skip(optional(whitespace()))
                            .then(Value::parser())
                            .map(|(_, v)| v),
                    ))
                    .map(|((_, k), v)| Command::Set(k, v.unwrap_or(Value::Bool(true))))
            }),
            command("unset", "Set setting to `off`", |p| {
                p.then(setting())
                    .map(|(_, k)| Command::Set(k, Value::Bool(false)))
            }),
            command("toggle", "Toggle setting", |p| {
                p.then(setting()).map(|(_, k)| Command::Toggle(k))
            }),
            command("echo", "Echo setting or value", |p| {
                p.then(Value::parser()).map(|(_, v)| Command::Echo(v))
            }),
            command("slice", "Slice view into <n> frames", |p| {
                p.then(optional(natural::<usize>().label("<n>")))
                    .map(|(_, n)| Command::Slice(n))
            }),
            command(
                "source",
                "Source an rx script (eg. palette or config)",
                |p| p.then(optional(path())).map(|(_, p)| Command::Source(p)),
            ),
            command("cd", "Change current directory", |p| {
                p.then(optional(path())).map(|(_, p)| Command::ChangeDir(p))
            }),
            command("zoom", "Zoom view", |p| {
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
            }),
            command("brush/size", "Set brush size", |p| {
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
            }),
            command("brush/mode", "Set brush mode, eg. `erase`", |p| {
                p.then(param::<brush::Mode>())
                    .map(|(_, m)| Command::BrushMode(m))
            }),
            command(
                "brush/set",
                "Set brush modifier, eg. `mirror/x` for horizontal mirroring",
                |p| {
                    p.then(param::<brush::Modifier>())
                        .map(|(_, m)| Command::BrushSet(m))
                },
            ),
            command("brush/unset", "Unset brush modifier", |p| {
                p.then(param::<brush::Modifier>())
                    .map(|(_, m)| Command::BrushUnset(m))
            }),
            command("brush/toggle", "Toggle brush modifier", |p| {
                p.then(param::<brush::Modifier>())
                    .map(|(_, m)| Command::BrushToggle(m))
            }),
            command("brush", "Switch to brush", |p| {
                p.value(Command::Tool(Tool::Brush))
            }),
            command("flood", "Switch to paint bucket tool", |p| {
                p.value(Command::Tool(Tool::Bucket))
            }),
            command("mode", "Set session mode, eg. `visual` or `normal`", |p| {
                p.then(param::<Mode>()).map(|(_, m)| Command::Mode(m))
            }),
            command("visual", "Set session mode to visual", |p| {
                p.map(|_| Command::Mode(Mode::Visual(VisualState::default())))
            }),
            command("sampler/off", "Switch the sampler tool off", |p| {
                p.value(Command::ToolPrev)
            }),
            command("sampler", "Switch to the sampler tool", |p| {
                p.value(Command::Tool(Tool::Sampler))
            }),
            command("v/next", "Activate the next view", |p| {
                p.value(Command::ViewNext)
            }),
            command("v/prev", "Activate the previous view", |p| {
                p.value(Command::ViewPrev)
            }),
            command("v/center", "Center the active view", |p| {
                p.value(Command::ViewCenter)
            }),
            command("v/clear", "Clear the active view", |p| {
                p.value(Command::Fill(Some(Rgba8::TRANSPARENT)))
            }),
            command("v/fill", "Fill the active view", |p| {
                p.then(optional(color())).map(|(_, c)| Command::Fill(c))
            }),
            command("pan", "Switch to the pan tool", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::Pan(x, y))
            }),
            command("map", "Map keys to a command in all modes", |p| {
                p.then(KeyMapping::parser(&[
                    Mode::Normal,
                    Mode::Visual(VisualState::selecting()),
                    Mode::Visual(VisualState::Pasting),
                ]))
                .map(|(_, km)| Command::Map(Box::new(km)))
            }),
            command("map/visual", "Map keys to a command in visual mode", |p| {
                p.then(KeyMapping::parser(&[
                    Mode::Visual(VisualState::selecting()),
                    Mode::Visual(VisualState::Pasting),
                ]))
                .map(|(_, km)| Command::Map(Box::new(km)))
            }),
            command("map/normal", "Map keys to a command in normal mode", |p| {
                p.then(KeyMapping::parser(&[Mode::Normal]))
                    .map(|(_, km)| Command::Map(Box::new(km)))
            }),
            command("map/help", "Map keys to a command in help mode", |p| {
                p.then(KeyMapping::parser(&[Mode::Help]))
                    .map(|(_, km)| Command::Map(Box::new(km)))
            }),
            command("map/clear!", "Clear all key mappings", |p| {
                p.value(Command::MapClear)
            }),
            command("p/add", "Add a color to the palette", |p| {
                p.then(color()).map(|(_, rgba)| Command::PaletteAdd(rgba))
            }),
            command("p/clear", "Clear the color palette", |p| {
                p.value(Command::PaletteClear)
            }),
            command("p/gradient", "Add a gradient to the palette", |p| {
                p.then(tuple::<Rgba8>(
                    color().label("<from>"),
                    color().label("<to>"),
                ))
                .skip(whitespace())
                .then(natural::<usize>().label("<count>"))
                .map(|((_, (cs, ce)), n)| Command::PaletteGradient(cs, ce, n))
            }),
            command(
                "p/sample",
                "Sample palette colors from the active view",
                |p| p.value(Command::PaletteSample),
            ),
            command("p/sort", "Sort the palette colors", |p| {
                p.value(Command::PaletteSort)
            }),
            command("p/write", "Write the color palette to a file", |p| {
                p.then(path()).map(|(_, path)| Command::PaletteWrite(path))
            }),
            command("undo", "Undo the last edit", |p| p.value(Command::Undo)),
            command("redo", "Redo the last edit", |p| p.value(Command::Redo)),
            command("f/add", "Add a blank frame to the active view", |p| {
                p.value(Command::FrameAdd)
            }),
            command("f/clone", "Clone a frame and add it to the view", |p| {
                p.then(optional(integer::<i32>().label("<index>")))
                    .map(|(_, index)| Command::FrameClone(index.unwrap_or(-1)))
            }),
            command(
                "f/remove",
                "Remove the last frame from the active view",
                |p| p.value(Command::FrameRemove),
            ),
            command("f/prev", "Navigate to previous frame", |p| {
                p.value(Command::FramePrev)
            }),
            command("f/next", "Navigate to next frame", |p| {
                p.value(Command::FrameNext)
            }),
            command("f/resize", "Resize the active view frame(s)", |p| {
                p.then(tuple::<u32>(
                    natural().label("<width>"),
                    natural().label("<height>"),
                ))
                .map(|(_, (w, h))| Command::FrameResize {
                    size: Size::new(w, h),
                })
            }),
            command("tool", "Switch tool", |p| {
                p.then(word().label("pan/brush/sampler/.."))
                    .try_map(|(_, t)| match t.as_str() {
                        "pan" => Ok(Command::Tool(Tool::Pan { panning: false })),
                        "brush" => Ok(Command::Tool(Tool::Brush)),
                        "sampler" => Ok(Command::Tool(Tool::Sampler)),
                        _ => Err(format!("unknown tool {:?}", t)),
                    })
            }),
            command("tool/prev", "Switch to previous tool", |p| {
                p.value(Command::ToolPrev)
            }),
            command("swap", "Swap foreground and background colors", |p| {
                p.value(Command::SwapColors)
            }),
            command("reset!", "Reset all settings to defaults", |p| {
                p.value(Command::Reset)
            }),
            command("selection/move", "Move selection", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionMove(x, y))
            }),
            command("selection/resize", "Resize selection", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionResize(x, y))
            }),
            command("selection/yank", "Yank/copy selection content", |p| {
                p.value(Command::SelectionYank)
            }),
            command("selection/cut", "Cut selection content", |p| {
                p.value(Command::SelectionCut)
            }),
            command("selection/paste", "Paste into selection", |p| {
                p.value(Command::SelectionPaste)
            }),
            command("selection/expand", "Expand selection", |p| {
                p.value(Command::SelectionExpand)
            }),
            command("selection/erase", "Erase selection contents", |p| {
                p.value(Command::SelectionErase)
            }),
            command("selection/offset", "Offset selection bounds", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::SelectionOffset(x, y))
            }),
            command("selection/jump", "Translate selection by one frame", |p| {
                p.then(param::<Direction>())
                    .map(|(_, dir)| Command::SelectionJump(dir))
            }),
            command("selection/fill", "Fill selection with color", |p| {
                p.then(optional(color()))
                    .map(|(_, rgba)| Command::SelectionFill(rgba))
            }),
            command("selection/flip", "Flip selection", |p| {
                p.then(word().label("x/y"))
                    .try_map(|(_, t)| match t.as_str() {
                        "x" => Ok(Command::SelectionFlip(Axis::Horizontal)),
                        "y" => Ok(Command::SelectionFlip(Axis::Vertical)),
                        _ => Err(format!("unknown axis {:?}, must be 'x' or 'y'", t)),
                    })
            }),
            command("paint/color", "Paint color", |p| {
                p.then(color())
                    .skip(whitespace())
                    .then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|((_, rgba), (x, y))| Command::PaintColor(rgba, x, y))
            }),
            command("paint/line", "Draw a line between two points", |p| {
                p.then(color())
                    .skip(whitespace())
                    .then(tuple::<i32>(
                        integer().label("<x1>"),
                        integer().label("<y1>"),
                    ))
                    .skip(whitespace())
                    .then(tuple::<i32>(
                        integer().label("<x2>"),
                        integer().label("<y2>"),
                    ))
                    .map(|(((_, color), (x1, y1)), (x2, y2))| {
                        Command::PaintLine(color, x1, y1, x2, y2)
                    })
            }),
            command("paint/fg", "Paint foreground color", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::PaintForeground(x, y))
            }),
            command("paint/bg", "Paint background color", |p| {
                p.then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|(_, (x, y))| Command::PaintBackground(x, y))
            }),
            command("paint/p", "Paint palette color", |p| {
                p.then(natural::<usize>())
                    .skip(whitespace())
                    .then(tuple::<i32>(integer().label("<x>"), integer().label("<y>")))
                    .map(|((_, i), (x, y))| Command::PaintPalette(i, x, y))
            }),
        ];
        let commands = commands.iter().map(|(_, _, v)| v.clone());
        let commands = commands.chain(iter::once(noop)).collect::<Vec<_>>();

        choice(commands).or(peek(
            until(hush(whitespace()).or(end()))
                .try_map(|cmd| Err(format!("unknown command: {}", cmd))),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parser() {
        let p = Command::parser();

        assert_eq!(
            p.parse("set foo = value"),
            Ok((
                Command::Set("foo".to_owned(), Value::Ident(String::from("value"))),
                ""
            ))
        );
        assert_eq!(
            p.parse("set scale = 1.0"),
            Ok((Command::Set("scale".to_owned(), Value::Float(1.0)), ""))
        );
        assert_eq!(
            p.parse("set foo=value"),
            Ok((
                Command::Set("foo".to_owned(), Value::Ident(String::from("value"))),
                ""
            ))
        );
        assert_eq!(
            p.parse("set foo"),
            Ok((Command::Set("foo".to_owned(), Value::Bool(true)), ""))
        );

        assert_eq!(
            key().parse("<hello>").unwrap_err().0.to_string(),
            "unknown key <hello>"
        );

        assert_eq!(p.parse("").unwrap(), (Command::Noop, ""));
    }

    #[test]
    fn test_echo_command() {
        let p = Command::parser();

        p.parse("echo 42").unwrap();
        p.parse("echo \"hello.\"").unwrap();
        p.parse("echo \"\"").unwrap();
    }

    #[test]
    fn test_zoom_command() {
        let p = Command::parser();

        assert!(p.parse("zoom -").is_ok());
        assert!(p.parse("zoom 3.0").is_ok());
        assert!(p.parse("zoom -1.0").is_err());
    }

    #[test]
    fn test_vfill_commands() {
        let p = Command::parser();

        p.parse("v/fill").unwrap();
        p.parse("v/fill #ff00ff").unwrap();
    }

    #[test]
    fn test_unknown_command() {
        let p = Command::parser();

        let (err, rest) = p.parse("fnord").unwrap_err();
        assert_eq!(rest, "fnord");
        assert_eq!(err.to_string(), "unknown command: fnord");

        let (err, rest) = p.parse("mode fnord").unwrap_err();
        assert_eq!(rest, "fnord");
        assert_eq!(err.to_string(), "unknown mode: fnord");
    }

    #[test]
    #[ignore]
    fn test_keymapping_parser() {
        let p = string("map")
            .skip(whitespace())
            .then(KeyMapping::parser(&[]));

        let (_, rest) = p.parse("map <tab> q! {q}").unwrap();
        assert_eq!(rest, "");

        let (_, rest) = p
            .parse("map <tab> brush/set erase {brush/unset erase}")
            .unwrap();
        assert_eq!(rest, "");

        let (_, rest) = p.parse("map <ctrl> tool sampler {tool/prev}").unwrap();
        assert_eq!(rest, "");
    }

    #[test]
    fn test_parser_errors() {
        let p = Command::parser();

        let (err, _) = p.parse("map <ctrl> tool sampler {tool/prev").unwrap_err();
        assert_eq!(err.to_string(), "unclosed '{' delimiter".to_string());
    }
}
