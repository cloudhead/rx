use std::error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::app::autocomplete::Completer;
use crate::app::autocomplete::{self, Autocomplete, FileCompleter, FileCompleterOpts};
use crate::app::command::Command;
use crate::app::history::History;
use crate::app::script::parsers::*;
use crate::app::ui::theme;
use crate::gfx::prelude::*;

use memoir::traits::Parse;
use memoir::*;

#[derive(Debug)]
pub struct Line {
    command: Command,
}

impl Parse for Line {
    fn parser() -> Parser<Self> {
        Command::parser()
            .skip(optional(whitespace()))
            .skip(optional(comment()))
            .end()
            .map(|command| Self { command })
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
            MessageType::Info => Rgba8::LIGHT_GREY,
            MessageType::Hint => Rgba8::DARK_GREY,
            MessageType::Echo => Rgba8::LIGHT_GREEN,
            MessageType::Error => theme::RED,
            MessageType::Warning => Rgba8::YELLOW,
            MessageType::Execution => Rgba8::GREY,
            MessageType::Debug => Rgba8::LIGHT_GREEN,
            MessageType::Okay => Rgba8::GREEN,
        }
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
    pub parser: Parser<Line>,
    /// Message displayed to user.
    pub message: Option<Message>,
    /// The current input string displayed to the user.
    input: String,
    /// File extensions supported.
    extension: OsString,
}

impl CommandLine {
    const MAX_INPUT: usize = 256;

    pub fn new<P: AsRef<Path>, S: AsRef<OsStr>>(cwd: P, history_path: P, extension: S) -> Self {
        Self {
            message: None,
            input: String::with_capacity(Self::MAX_INPUT),
            cursor: 0,
            parser: Line::parser(),
            history: History::new(history_path, 1024),
            autocomplete: Autocomplete::new(CommandCompleter::new(cwd, &[extension.as_ref()])),
            extension: extension.as_ref().into(),
        }
    }

    /// Display a message to the user. Also logs.
    pub fn message<D: fmt::Display>(&mut self, msg: D, t: MessageType) {
        match t {
            MessageType::Info => info!("{}", msg),
            MessageType::Hint => {}
            MessageType::Echo => info!("{}", msg),
            MessageType::Error => error!("{}", msg),
            MessageType::Warning => warn!("{}", msg),
            MessageType::Execution => {}
            MessageType::Okay => info!("{}", msg),
            MessageType::Debug => debug!("{}", msg),
        }
        self.message = Some(Message::new(msg, t));
    }

    /// Display an error message to the user.
    pub fn error(&mut self, err: impl error::Error) {
        self.message(format!("Error: {}", err), MessageType::Error)
    }

    /// Display an informational message to the user.
    pub fn info<D: fmt::Display>(&mut self, msg: D) {
        self.message(msg, MessageType::Info)
    }

    pub fn set_cwd(&mut self, path: &Path) {
        self.autocomplete =
            Autocomplete::new(CommandCompleter::new(path, &[self.extension.as_os_str()]));
    }

    pub fn parse(&self, input: &str) -> Result<Command, Error> {
        match self.parser.parse(input) {
            Ok((line, _)) => Ok(line.command),
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

            self.cursor = cursor;
            self.autocomplete.invalidate();
            return Some(c);
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

    pub fn cursor_back(&mut self) {
        if self.cursor > 0 {
            self.cursor = 0;
            self.autocomplete.invalidate();
        }
    }

    pub fn cursor_front(&mut self) {
        self.cursor = self.input.len();
    }

    pub fn putc(&mut self, c: char) {
        if self.input.len() + c.len_utf8() > self.input.capacity() {
            return;
        }
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.autocomplete.invalidate();
        self.message = None;
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

#[derive(Debug, Default)]
pub struct CommandCompleter {
    file_completer: FileCompleter,
}

impl CommandCompleter {
    fn new<P: AsRef<Path>>(cwd: P, exts: &[&OsStr]) -> Self {
        Self {
            file_completer: FileCompleter::new(cwd, exts),
        }
    }
}

impl autocomplete::Completer for CommandCompleter {
    type Options = ();

    fn complete(&self, input: &str, _opts: ()) -> Vec<String> {
        let p = Command::parser();

        match p.parse(input) {
            Ok((cmd, _)) => match cmd {
                Command::ChangeDir(path) => {
                    self.complete_path(path, input, FileCompleterOpts { directories: true })
                }
                Command::Source(path) | Command::Write(path) => {
                    self.complete_path(path, input, FileCompleterOpts::default())
                }
                Command::Edit(paths) => {
                    self.complete_path(paths.last().cloned(), input, FileCompleterOpts::default())
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
        path: Option<PathBuf>,
        input: &str,
        opts: FileCompleterOpts,
    ) -> Vec<String> {
        let path_str = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        // If there's whitespace between the path and the cursor, don't complete the path.
        // Instead, complete as if the input was empty.
        match input.chars().next_back() {
            Some(c) if c.is_whitespace() => self.file_completer.complete("", opts),
            _ => self.file_completer.complete(path_str.as_str(), opts),
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

        for file_name in &["one.png", "two.png", "three.png"] {
            let path = tmp.path().join(file_name);
            File::create(path).unwrap();
        }

        let cc = CommandCompleter::new(tmp.path(), &[OsStr::new("png")]);
        let mut auto = Autocomplete::new(cc);

        assert_eq!(auto.next("e |", 2), Some(("one.png".to_owned(), 2..2)));
        auto.invalidate();
        assert_eq!(
            auto.next("e |one.png", 2),
            Some(("one.png".to_owned(), 2..2))
        );

        auto.invalidate();
        assert_eq!(
            auto.next("e one.png | two.png", 10),
            Some(("one.png".to_owned(), 10..10))
        );
        assert_eq!(
            auto.next("e one.png one.png| two.png", 19),
            Some(("three.png".to_owned(), 10..17))
        );
        assert_eq!(
            auto.next("e one.png three.png| two.png", 17),
            Some(("two.png".to_owned(), 10..19))
        );

        fs::create_dir(tmp.path().join("assets")).unwrap();
        for file_name in &["four.png", "five.png", "six.png"] {
            let path = tmp.path().join("assets").join(file_name);
            File::create(path).unwrap();
        }

        auto.invalidate();
        assert_eq!(
            auto.next("e assets/|", 9),
            Some(("five.png".to_owned(), 9..9))
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

        let mut cli = CommandLine::new(tmp.path(), &tmp.path().join(".history"), "png");

        cli.puts("e one");
        cli.completion_next();
        assert_eq!(cli.input(), "e one.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e one.png");

        cli.clear();
        cli.puts("e ");
        cli.completion_next();
        assert_eq!(cli.input(), "e assets");

        cli.completion_next();
        assert_eq!(cli.input(), "e one.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e three.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e two.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets");

        cli.putc('/');
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/four.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png");

        cli.putc(' ');
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png assets");
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png one.png");

        cli.putc(' ');
        cli.putc('t');
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png one.png three.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png one.png two.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png one.png three.png");

        for _ in 0..10 {
            cli.cursor_backward();
        }
        cli.putc(' ');
        cli.putc('o');
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png one.png one.png three.png");

        cli.clear();
        cli.puts("e assets");
        cli.completion_next();
        assert_eq!(cli.input(), "e assets/");

        cli.clear();
        cli.puts("e asset");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/");

        cli.completion_next();
        assert_eq!(cli.input(), "e assets/five.png");
    }

    #[test]
    fn test_command_line_change_dir() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        for file_name in &["four.png", "five.png"] {
            let path = tmp.path().join("assets").join(file_name);
            File::create(path).unwrap();
        }

        let mut cli = CommandLine::new(tmp.path(), Path::new("/dev/null"), "png");

        cli.set_cwd(tmp.path().join("assets/").as_path());
        cli.puts("e ");

        cli.completion_next();
        assert_eq!(cli.input(), "e five.png");

        cli.completion_next();
        assert_eq!(cli.input(), "e four.png");
    }

    #[test]
    fn test_command_line_cd() {
        let tmp = tempfile::tempdir().unwrap();

        fs::create_dir(tmp.path().join("assets")).unwrap();
        fs::create_dir(tmp.path().join("assets").join("1")).unwrap();
        fs::create_dir(tmp.path().join("assets").join("2")).unwrap();
        File::create(tmp.path().join("assets").join("rx.png")).unwrap();

        let mut cli = CommandLine::new(tmp.path(), Path::new("/dev/null"), "png");

        cli.clear();
        cli.puts("cd assets/");

        cli.completion_next();
        assert_eq!(cli.input(), "cd assets/1");

        cli.completion_next();
        assert_eq!(cli.input(), "cd assets/2");

        cli.completion_next();
        assert_eq!(cli.input(), "cd assets/1");
    }

    #[test]
    fn test_command_line_cursor() {
        let mut cli = CommandLine::new("/dev/null", "/dev/null", "rgba");

        cli.puts("echo");
        cli.delc();
        assert_eq!(cli.input(), "ech");
        cli.delc();
        assert_eq!(cli.input(), "ec");
        cli.delc();
        assert_eq!(cli.input(), "e");
        cli.delc();
        assert_eq!(cli.input(), "");

        cli.clear();
        cli.puts("ec");

        assert_eq!(cli.peek(), None);
        assert_eq!(cli.peek_back(), Some('c'));
        cli.cursor_backward();

        assert_eq!(cli.cursor, 1);
        assert_eq!(cli.peek(), Some('c'));
        assert_eq!(cli.peek_back(), Some('e'));
        cli.cursor_backward();

        assert_eq!(cli.cursor, 0);
        assert_eq!(cli.peek(), Some('e'));
        assert_eq!(cli.peek_back(), None);

        cli.delc();
        assert_eq!(cli.input(), "ec");

        cli.clear();
        cli.puts("echo");

        assert_eq!(cli.peek(), None);
        cli.cursor_back();

        assert_eq!(cli.peek(), Some('e'));
        assert_eq!(cli.peek_back(), None);

        cli.cursor_front();
        assert_eq!(cli.peek(), None);
        assert_eq!(cli.peek_back(), Some('o'));
    }
}
