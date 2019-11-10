use crate::cmd;
use crate::parser;
use crate::platform;

use std::str::FromStr;
use std::time;

#[derive(Debug, Clone)]
pub struct TimedEvent {
    pub frame: u64,
    pub delta: time::Duration,
    pub event: Event,
}

impl TimedEvent {
    pub fn new(frame: u64, delta: time::Duration, event: Event) -> Self {
        TimedEvent {
            frame,
            delta,
            event,
        }
    }
}

impl From<TimedEvent> for String {
    fn from(te: TimedEvent) -> String {
        format!(
            "{:05} {:07} {}",
            te.frame,
            te.delta.as_millis(),
            String::from(te.event)
        )
    }
}

impl FromStr for TimedEvent {
    type Err = parser::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let p = parser::Parser::new(input);
        let (f, p) = p.parse::<u32>()?;
        let (_, p) = p.whitespace()?;
        let (d, p) = p.parse::<u32>()?;
        let (_, p) = p.whitespace()?;
        let (l, p) = p.leftover()?;
        let (_, _) = p.finish()?;

        let e = Event::from_str(l)?;

        Ok(TimedEvent {
            frame: f as u64,
            delta: time::Duration::from_millis(d as u64),
            event: e,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    MouseInput(platform::MouseButton, platform::InputState),
    MouseWheel(platform::LogicalDelta),
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
            Event::MouseWheel(delta) => {
                format!("mouse/wheel {} {}", delta.x, delta.y)
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
            "mouse/wheel" => {
                let ((x, y), p) = p.parse::<(f64, f64)>()?;
                Ok((Event::MouseWheel(platform::LogicalDelta { x, y }), p))
            }
            "cursor/moved" => {
                let ((x, y), p) = p.parse::<(f64, f64)>()?;
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
