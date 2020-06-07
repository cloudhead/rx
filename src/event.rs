use crate::parser;
use crate::platform;

use memoir::parsers::*;

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
        let result = natural::<u32>()
            .skip(whitespace())
            .then(natural::<u32>())
            .skip(whitespace())
            .then(until(end()))
            .try_map(|((f, d), l)| {
                Event::from_str(&l)
                    .map(|e| TimedEvent {
                        frame: f as u64,
                        delta: time::Duration::from_millis(d as u64),
                        event: e,
                    })
                    .map_err(|e| e.to_string())
            })
            .parse(input);

        match result {
            Ok((out, _)) => Ok(out),
            Err((err, _)) => Err(err),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    MouseInput(platform::MouseButton, platform::InputState),
    MouseWheel(platform::LogicalDelta),
    CursorMoved(platform::LogicalPosition),
    KeyboardInput(platform::KeyboardInput),
    ReceivedCharacter(char, platform::ModifiersState),
    Paste(Option<String>),
}

impl From<Event> for String {
    fn from(event: Event) -> String {
        match event {
            Event::MouseInput(_, platform::InputState::Pressed) => format!("mouse/input pressed"),
            Event::MouseInput(_, platform::InputState::Released) => format!("mouse/input released"),
            Event::MouseInput(_, platform::InputState::Repeated) => unreachable!(),
            Event::MouseWheel(delta) => format!("mouse/wheel {} {}", delta.x, delta.y),
            Event::CursorMoved(platform::LogicalPosition { x, y }) => {
                format!("cursor/moved {} {}", x, y)
            }
            Event::KeyboardInput(platform::KeyboardInput { key, state, .. }) => {
                let state = match state {
                    platform::InputState::Pressed => "pressed",
                    platform::InputState::Released => "released",
                    platform::InputState::Repeated => "repeated",
                };
                format!("keyboard/input {} {}", key.unwrap(), state)
            }
            Event::ReceivedCharacter(c, _) => format!("char/received '{}'", c),
            Event::Paste(Some(s)) => format!("paste '{}'", s),
            Event::Paste(None) => format!("paste ''"),
        }
    }
}

impl FromStr for Event {
    type Err = parser::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (event, p) = parser::identifier()
            .skip(whitespace())
            .parse(input)
            .map_err(|(e, _)| e)?;

        let result: Result<(Self, &str), Self::Err> = match event.as_str() {
            "mouse/input" => {
                let (s, p) = parser::param::<platform::InputState>()
                    .followed_by(end())
                    .parse(p)
                    .map_err(|(e, _)| e)?;
                Ok((Event::MouseInput(platform::MouseButton::Left, s), p))
            }
            "mouse/wheel" => {
                let ((x, y), p) = parser::tuple::<f64>(rational(), rational())
                    .followed_by(end())
                    .parse(p)
                    .map_err(|(e, _)| e)?;
                Ok((Event::MouseWheel(platform::LogicalDelta { x, y }), p))
            }
            "cursor/moved" => {
                let ((x, y), p) = parser::tuple::<f64>(rational(), rational())
                    .followed_by(end())
                    .parse(p)
                    .map_err(|(e, _)| e)?;
                Ok((
                    Event::CursorMoved(platform::LogicalPosition::new(x as f64, y as f64)),
                    p,
                ))
            }
            "keyboard/input" => {
                let ((k, s), p) = parser::param::<platform::Key>()
                    .skip(whitespace())
                    .then(parser::param::<platform::InputState>())
                    .followed_by(end())
                    .parse(p)
                    .map_err(|(e, _)| e)?;
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
                let (c, p) = between('\'', '\'', character())
                    .followed_by(end())
                    .parse(p)
                    .map_err(|(e, _)| e)?;
                Ok((Event::ReceivedCharacter(c, Default::default()), p))
            }
            event => Err(parser::Error::new(format!(
                "unrecognized event {:?}",
                event
            ))),
        };

        let (event, rest) = result?;
        assert!(rest.is_empty());

        Ok(event)
    }
}
