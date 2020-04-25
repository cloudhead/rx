use memoir::traits::Parse;
use memoir::*;

use directories as dirs;

use rgx::kit::Rgba8;

use crate::brush::BrushMode;
use crate::platform;
use crate::session::{Direction, Mode, VisualState};

use std::ffi::OsString;
use std::str::FromStr;

pub type Error = memoir::result::Error;

pub fn identifier() -> Parser<String> {
    many::<_, String>(satisfy(
        |c: char| c.is_ascii_alphabetic() || c == '/' || c == '-',
        "<identifier>",
    ))
    .label("<identifier>")
}

pub fn word() -> Parser<String> {
    many(letter())
}

pub fn comment() -> Parser<String> {
    string("--")
        .skip(optional(whitespace()))
        .then(until(end()))
        .map(|(_, comment)| comment)
}

pub fn path() -> Parser<String> {
    many::<_, String>(satisfy(|c| !c.is_whitespace(), "!<whitespace>"))
        .map(|input: String| {
            let mut path: OsString = input.clone().into();

            // Linux and BSD and MacOS use `~` to infer the home directory of a given user.
            if cfg!(unix) {
                // We have to do this dance because `Path::join` doesn't do what we want
                // if the input is for eg. "~/". We also can't use `Path::strip_prefix`
                // because it drops our trailing slash.
                if let Some('~') = input.chars().next() {
                    if let Some(base_dirs) = dirs::BaseDirs::new() {
                        path = base_dirs.home_dir().into();
                        path.push(&input['~'.len_utf8()..]);
                    }
                }
            }

            match path.to_str() {
                Some(p) => p.to_string(),
                None => panic!("invalid path: {:?}", path),
            }
        })
        .label("<path>")
}

impl Parse for platform::Key {
    fn parser() -> Parser<Self> {
        let alphanum = character().try_map(|c| {
            let key: platform::Key = c.into();

            if key == platform::Key::Unknown {
                return Err(format!("unknown key {:?}", c));
            }
            Ok(key)
        });

        let control = between('<', '>', any::<_, String>(letter())).try_map(|key| {
            let key = match key.as_str() {
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
                other => return Err(format!("unknown key <{}>", other)),
            };
            Ok(key)
        });

        control.or(alphanum).label("<key>")
    }
}

impl Parse for platform::InputState {
    fn parser() -> Parser<Self> {
        word().try_map(|w| match w.as_str() {
            "pressed" => Ok(platform::InputState::Pressed),
            "released" => Ok(platform::InputState::Released),
            "repeated" => Ok(platform::InputState::Repeated),
            other => Err(format!("unknown input state: {}", other)),
        })
    }
}

impl Parse for Direction {
    fn parser() -> Parser<Self> {
        character()
            .try_map(|c| match c {
                '+' => Ok(Direction::Forward),
                '-' => Ok(Direction::Backward),
                _ => Err("direction must be either `+` or `-`"),
            })
            .label("+/-")
    }
}

pub fn param<T: Parse>() -> Parser<T> {
    T::parser()
}

pub fn color() -> Parser<Rgba8> {
    let label = "<color>";

    Parser::new(
        move |input| {
            if input.is_empty() {
                return Err((
                    memoir::result::Error::new(format!("expected {:?}", label)),
                    input,
                ));
            }
            if input.len() < 7 {
                return Err((
                    memoir::result::Error::new(format!("{:?} is not a valid color value", input)),
                    input,
                ));
            }
            let (s, alpha) = input.split_at(7);

            match Rgba8::from_str(s) {
                Ok(color) => {
                    if alpha.is_empty() {
                        Ok((color, alpha))
                    } else {
                        let (a, rest) = symbol('/')
                            .then(rational::<f64>())
                            .map(|(_, a)| a)
                            .parse(alpha)?;
                        Ok((color.alpha((a * std::u8::MAX as f64) as u8), rest))
                    }
                }
                Err(_) => Err((
                    memoir::result::Error::new(format!("malformed color value `{}`", s)),
                    input,
                )),
            }
        },
        label,
    )
}

impl Parse for BrushMode {
    fn parser() -> Parser<Self> {
        Parser::new(
            |input| {
                let (id, p) = identifier().parse(input)?;
                match id.as_str() {
                    "erase" => Ok((BrushMode::Erase, p)),
                    "multi" => Ok((BrushMode::Multi, p)),
                    "perfect" => Ok((BrushMode::Perfect, p)),
                    "xsym" => Ok((BrushMode::XSym, p)),
                    "ysym" => Ok((BrushMode::YSym, p)),
                    "xray" => Ok((BrushMode::XRay, p)),
                    mode => Err((
                        memoir::result::Error::new(format!("unknown brush mode '{}'", mode)),
                        input,
                    )),
                }
            },
            "<mode>",
        )
    }
}

impl Parse for Mode {
    fn parser() -> Parser<Self> {
        Parser::new(
            |input| {
                let (id, p) = identifier().parse(input)?;
                match id.as_str() {
                    "command" => Ok((Mode::Command, p)),
                    "normal" => Ok((Mode::Normal, p)),
                    "visual" => Ok((Mode::Visual(VisualState::default()), p)),
                    "present" => Ok((Mode::Present, p)),
                    mode => Err((
                        memoir::result::Error::new(format!("unknown mode: {}", mode)),
                        input,
                    )),
                }
            },
            "<mode>",
        )
    }
}

pub fn quoted() -> Parser<String> {
    between('"', '"', until(symbol('"')))
}

pub fn paths() -> Parser<Vec<String>> {
    any::<_, Vec<String>>(path().skip(optional(whitespace()))).label("<path>..")
}

pub fn setting() -> Parser<String> {
    identifier().label("<setting>")
}

pub fn tuple<O>(x: Parser<O>, y: Parser<O>) -> Parser<(O, O)> {
    x.skip(whitespace()).then(y)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_paths() {
        let p = paths();

        let (out, rest) = p.parse("path/one.png path/two.png path/three.png").unwrap();

        assert_eq!(rest, "");
        assert_eq!(out, vec!["path/one.png", "path/two.png", "path/three.png"]);
    }
}
