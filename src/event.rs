use crate::cmd;
use crate::parser;
use crate::platform;

use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum Event {
    MouseInput(platform::MouseButton, platform::InputState),
    CursorMoved(platform::LogicalPosition),
    KeyboardInput(platform::KeyboardInput),
    ReceivedCharacter(char),
}

impl From<Event> for String {
    fn from(event: Event) -> String {
        match event {
            Event::MouseInput(_, platform::InputState::Pressed) => {
                format!("mouse/input pressed")
            }
            Event::MouseInput(_, platform::InputState::Released) => {
                format!("mouse/input released")
            }
            Event::CursorMoved(platform::LogicalPosition { x, y }) => {
                format!("cursor/moved {} {}", x, y)
            }
            Event::KeyboardInput(platform::KeyboardInput {
                key,
                state,
                ..
            }) => {
                let state = match state {
                    platform::InputState::Pressed => "pressed",
                    platform::InputState::Released => "released",
                };
                format!("keyboard/input {} {}", key.unwrap(), state)
            }
            Event::ReceivedCharacter(c) => format!("char/received '{}'", c),
        }
    }
}

impl FromStr for Event {
    type Err = parser::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let p = parser::Parser::new(input);
        let (event, p) = p.word()?;
        let (_, p) = p.whitespace()?;

        let result: Result<(Self, parser::Parser), Self::Err> = match event {
            "mouse/input" => {
                let (s, p) = p.parse::<platform::InputState>()?;
                Ok((Event::MouseInput(platform::MouseButton::Left, s), p))
            }
            "cursor/moved" => {
                let ((x, y), p) = p.parse::<(f64, f64)>()?;
                let (_, p) = p.whitespace()?;
                Ok((
                    Event::CursorMoved(platform::LogicalPosition::new(
                        x as f64, y as f64,
                    )),
                    p,
                ))
            }
            "keyboard/input" => {
                let (k, p) = p.parse::<cmd::Key>()?;
                let (_, p) = p.whitespace()?;
                let (s, p) = p.parse::<platform::InputState>()?;
                let cmd::Key::Virtual(k) = k;
                Ok((
                    Event::KeyboardInput(platform::KeyboardInput {
                        state: s,
                        key: Some(k),
                        modifiers: platform::ModifiersState::default(),
                    }),
                    p,
                ))
            }
            "char/received" => {
                let (c, p) = p.character()?;
                Ok((Event::ReceivedCharacter(c), p))
            }
            event => Err(parser::Error::new(format!(
                "unrecognized event {:?}",
                event
            ))),
        };

        let (event, p) = result?;
        p.finish()?;

        Ok(event)
    }
}
