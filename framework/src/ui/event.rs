// use crate::parser;
use std::time;

use crate::gfx::prelude::{Point, Size};
use crate::platform;

pub use platform::InputState;

// use memoir::parsers::*;

#[derive(Debug, Clone)]
pub enum WidgetEvent {
    MouseDown(platform::MouseButton),
    MouseUp(platform::MouseButton),
    MouseScroll(platform::LogicalDelta),
    MouseMove(Point),
    Resized(Size),
    MouseEnter(Point),
    MouseExit,
    KeyDown {
        key: platform::Key,
        modifiers: platform::ModifiersState,
        repeat: bool,
    },
    KeyUp {
        key: platform::Key,
        modifiers: platform::ModifiersState,
    },
    CharacterReceived(char, platform::ModifiersState),
    Paste(Option<String>),
    Tick(time::Duration),
    Frame,
}

#[derive(Debug, Clone)]
pub enum WindowEvent {
    CloseRequested,
    FocusGained,
    FocusLost,
    Resized(Size),
}

// impl From<Event> for String {
//     fn from(event: Event) -> String {
//         match event {
//             Event::MouseInput(_, platform::InputState::Pressed) => {
//                 "mouse/input pressed".to_string()
//             }
//             Event::MouseInput(_, platform::InputState::Released) => {
//                 "mouse/input released".to_string()
//             }
//             Event::MouseInput(_, platform::InputState::Repeated) => unreachable!(),
//             Event::MouseWheel(delta) => format!("mouse/wheel {} {}", delta.x, delta.y),
//             Event::CursorMoved(platform::LogicalPosition { x, y }) => {
//                 format!("cursor/moved {} {}", x, y)
//             }
//             Event::KeyboardInput(platform::KeyboardInput { key, state, .. }) => {
//                 let state = match state {
//                     platform::InputState::Pressed => "pressed",
//                     platform::InputState::Released => "released",
//                     platform::InputState::Repeated => "repeated",
//                 };
//                 format!("keyboard/input {} {}", key.unwrap(), state)
//             }
//             Event::ReceivedCharacter(c, _) => format!("char/received '{}'", c),
//             Event::Paste(Some(s)) => format!("paste '{}'", s),
//             Event::Paste(None) => "paste ''".to_string(),
//         }
//     }
// }
