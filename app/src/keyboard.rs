use std::fmt;

use crate::app::command::Command;
use crate::app::Mode;
use crate::framework::platform::{InputState, Key, ModifiersState};

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Input {
    Key(Key),
    Character(char),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(k) => write!(f, "{}", k),
            Self::Character(c) => write!(f, "{}", c),
        }
    }
}

/// A key binding.
#[derive(PartialEq, Clone, Debug)]
pub struct KeyBinding {
    /// The `Mode`s this binding applies to.
    pub modes: Vec<Mode>,
    /// Modifiers which must be held.
    pub modifiers: ModifiersState,
    /// Input expected to trigger the binding.
    pub input: Input,
    /// Whether the key should be pressed or released.
    pub state: InputState,
    /// The `Command` to run when this binding is triggered.
    pub command: Command,
    /// Whether this key binding controls a toggle.
    pub is_toggle: bool,
    /// How this key binding should be displayed to the user.
    /// If `None`, then this binding shouldn't be shown to the user.
    pub display: Option<String>,
}

impl KeyBinding {
    fn is_match(
        &self,
        input: Input,
        state: InputState,
        modifiers: ModifiersState,
        mode: Mode,
    ) -> bool {
        match (input, self.input) {
            (Input::Key(key), Input::Key(k)) => {
                key == k
                    && self.state == state
                    && self.modes.contains(&mode)
                    && (self.modifiers == modifiers
                        || state == InputState::Released
                        || key.is_modifier())
            }
            (Input::Character(a), Input::Character(b)) => {
                // Nb. We only check the <ctrl> modifier with characters,
                // because the others (especially <shift>) will most likely
                // input a different character.
                a == b
                    && self.modes.contains(&mode)
                    && self.state == state
                    && self.modifiers.ctrl == modifiers.ctrl
            }
            _ => false,
        }
    }
}

/// Manages a list of key bindings.
#[derive(Debug, Default)]
pub struct KeyBindings {
    elems: Vec<KeyBinding>,
}

impl KeyBindings {
    /// Add a key binding.
    pub fn add(&mut self, binding: KeyBinding) {
        for mode in binding.modes.iter() {
            self.elems
                .retain(|kb| !kb.is_match(binding.input, binding.state, binding.modifiers, *mode));
        }
        self.elems.push(binding);
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// Find a key binding based on some input state.
    pub fn find(
        &self,
        input: Input,
        modifiers: ModifiersState,
        state: InputState,
        mode: Mode,
    ) -> Option<KeyBinding> {
        self.elems
            .iter()
            .rev()
            .cloned()
            .find(|kb| kb.is_match(input, state, modifiers, mode))
    }

    /// Iterate over all key bindings.
    pub fn iter(&self) -> std::slice::Iter<'_, KeyBinding> {
        self.elems.iter()
    }
}
