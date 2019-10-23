//! String parser.

use crate::brush::BrushMode;
use crate::platform;
use crate::session::Mode;

use std::fmt;
use std::path::{Path, PathBuf};
use std::result;
use std::str::FromStr;

use directories as dirs;
use rgx::core::Rgba8;

pub type Result<'a, T> = result::Result<(T, Parser<'a>), Error>;

#[derive(Debug, Clone)]
pub struct Error {
    msg: String,
}

impl Error {
    pub fn new<S: Into<String>>(msg: S) -> Self {
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

pub trait Parse<'a>: Sized {
    fn parse(input: Parser<'a>) -> Result<'a, Self>;
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

impl<'a> Parse<'a> for (f64, f64) {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (x, p) = p.parse::<f64>()?;
        let (_, p) = p.whitespace()?;
        let (y, p) = p.parse::<f64>()?;

        Ok(((x, y), p))
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

impl<'a> Parse<'a> for platform::InputState {
    fn parse(p: Parser<'a>) -> Result<'a, Self> {
        let (w, p) = p.word()?;
        match w {
            "pressed" => Ok((platform::InputState::Pressed, p)),
            "released" => Ok((platform::InputState::Released, p)),
            other => Err(Error::new(format!("unkown input state {:?}", other))),
        }
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

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    input: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    pub fn empty() -> Self {
        Self { input: "" }
    }

    pub fn finish(self) -> Result<'a, ()> {
        let (_, p) = self.whitespace()?;

        if p.is_empty() {
            Ok(((), Parser::empty()))
        } else {
            Err(Error::new(format!("extraneaous input: `{}`", p.input)))
        }
    }

    pub fn path(self) -> Result<'a, String> {
        let (path, parser) = self.word()?;

        if path == "" {
            return Ok((String::from(""), parser));
        }

        let mut path = PathBuf::from(path);

        // Linux and BSD and MacOS use ~ to infer the home directory of a given user
        if cfg!(unix) {
            if let Ok(suffix) = path.strip_prefix("~") {
                if let Some(base_dirs) = dirs::BaseDirs::new() {
                    path = base_dirs.home_dir().join(suffix);
                }
            }
        }

        let directory = &path
            .parent()
            .and_then(|directory| directory.canonicalize().ok());

        if let (Some(directory), Some(filename)) = (directory, &path.file_name()) {
            path = Path::new(&directory).join(Path::new(&filename));
        }

        match path.to_str() {
            Some(path_as_str) => {
                Ok((path_as_str.to_string(), parser))
            },
            None => {
                Err(Error::new(format!("unable to convert Path into a String: `{:?}`", path)))
            }
        }
    }

    pub fn peek(&self) -> Option<char> {
        self.input.chars().nth(0)
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    pub fn sigil(self, c: char) -> Result<'a, char> {
        if self.input.starts_with(c) {
            Ok((c, Parser::new(&self.input[1..])))
        } else {
            Err(Error::new(format!("expected '{}'", c)))
        }
    }

    pub fn string(self) -> Result<'a, &'a str> {
        let p = self;

        let (_, p) = p.sigil('"')?;
        let (s, p) = p.until(|c| c == '"')?;
        let (_, p) = p.sigil('"')?;

        Ok((s, p))
    }

    pub fn character(self) -> Result<'a, char> {
        let p = self;

        let (_, p) = p.sigil('\'')?;
        let (c, p) = p.parse::<char>()?;
        let (_, p) = p.sigil('\'')?;

        Ok((c, p))
    }

    pub fn alpha(self) -> Result<'a, &'a str> {
        self.expect(|c| c.is_alphanumeric())
    }

    pub fn comment(self) -> Result<'a, &'a str> {
        let p = self;

        let (_, p) = p.whitespace()?;
        let (_, p) = p.sigil('-')?;
        let (_, p) = p.sigil('-')?;
        let (_, p) = p.whitespace()?;
        let (s, p) = p.leftover()?;

        Ok((s, p))
    }

    pub fn leftover(self) -> Result<'a, &'a str> {
        Ok((self.input, Parser::empty()))
    }

    pub fn whitespace(self) -> Result<'a, ()> {
        self.consume(|c| c.is_whitespace())
    }

    pub fn parse<T: Parse<'a>>(self) -> Result<'a, T> {
        T::parse(self)
    }

    pub fn word(self) -> Result<'a, &'a str> {
        self.expect(|c| !c.is_whitespace())
    }

    pub fn count(self, n: usize) -> Result<'a, &'a str> {
        if self.input.len() >= n {
            Ok((&self.input[..n], Parser::new(&self.input[n..])))
        } else {
            Err(Error::new("reached end of input"))
        }
    }

    pub fn identifier(self) -> Result<'a, &'a str> {
        self.expect(|c| {
            (c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || [':', '/', '_', '+', '-', '!', '?'].contains(&c))
        })
    }

    pub fn consume<P>(self, predicate: P) -> Result<'a, ()>
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

    pub fn until<P>(self, predicate: P) -> Result<'a, &'a str>
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

    pub fn expect<P>(self, predicate: P) -> Result<'a, &'a str>
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
