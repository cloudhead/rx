use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::gfx::prelude::Rgba8;
use crate::script::Value;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("no such setting `{0}`")]
    InvalidSetting(String),
    #[error("invalid value `{0}` for `{1}`, expected {2}")]
    InvalidValue(Value, String, &'static str),
}

/// A dictionary used to store session settings.
#[derive(Debug)]
pub struct Settings {
    current: HashMap<String, Value>,
    changed: HashSet<String>,
}

impl Settings {
    /// Lookup a setting.
    pub fn get(&self, setting: &str) -> Option<&Value> {
        self.current.get(setting)
    }

    /// Returns changed settings since last time, removing each changed key from the set.
    pub fn changed(&mut self) -> impl Iterator<Item = String> + '_ {
        self.changed.drain()
    }

    /// Set an existing setting to a new value. Returns `Err` if there is a type
    /// mismatch or the setting isn't found. Otherwise, returns `Ok` with the
    /// old value.
    pub fn set(&mut self, key: impl Into<String>, val: impl Into<Value>) -> Result<Value, Error> {
        let key = key.into();
        let val = val.into();

        match self.current.entry(key) {
            Entry::Occupied(mut e) => {
                if std::mem::discriminant(&val) == std::mem::discriminant(e.get()) {
                    self.changed.insert(e.key().to_owned());
                    Ok(e.insert(val))
                } else {
                    Err(Error::InvalidValue(
                        val,
                        e.key().to_owned(),
                        e.get().description(),
                    ))
                }
            }
            Entry::Vacant(e) => Err(Error::InvalidSetting(e.into_key())),
        }
    }
}

impl Default for Settings {
    /// The default settings.
    fn default() -> Self {
        Self {
            current: hashmap! {
                "debug" => Value::Bool(false),
                "checker" => Value::Bool(false),
                "background" => Value::Color(Rgba8::TRANSPARENT),
                "input/mouse" => Value::Bool(true),
                "scale" => Value::Float(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::Int(160),
                "ui/palette" => Value::Bool(true),
                "ui/status" => Value::Bool(true),
                "ui/cursor" => Value::Bool(true),
                "ui/message" => Value::Bool(true),
                "ui/switcher" => Value::Bool(true),
                "ui/view-info" => Value::Bool(true),
                "ui/font" => Value::Str(String::from("default")),
                "grid" => Value::Bool(false),
                "grid/color" => Value::Color(Rgba8::BLUE),
                "grid/spacing" => Value::Int2D(8, 8),
                "debug/crosshair" => Value::Bool(false)
            },
            changed: HashSet::default(),
        }
    }
}

impl std::ops::Index<&str> for Settings {
    type Output = Value;

    fn index(&self, setting: &str) -> &Self::Output {
        self.get(setting)
            .unwrap_or_else(|| panic!("setting {:?} should exist", setting))
    }
}
