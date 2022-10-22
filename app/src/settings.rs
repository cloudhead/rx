use std::collections::HashMap;

use anyhow::{anyhow, Error};

use crate::gfx::prelude::Rgba8;
use crate::script::Value;

/// A dictionary used to store session settings.
#[derive(Debug)]
pub struct Settings {
    map: HashMap<String, Value>,
}

impl Settings {
    /// Lookup a setting.
    pub fn get(&self, setting: &str) -> Option<&Value> {
        self.map.get(setting)
    }

    /// Set an existing setting to a new value. Returns `Err` if there is a type
    /// mismatch or the setting isn't found. Otherwise, returns `Ok` with the
    /// old value.
    pub fn set(&mut self, k: &str, v: Value) -> Result<Value, Error> {
        if let Some(current) = self.get(k) {
            if std::mem::discriminant(&v) == std::mem::discriminant(current) {
                return Ok(self.map.insert(k.to_string(), v).unwrap());
            }
            Err(anyhow!(
                "invalid value `{}` for `{}`, expected {}",
                v,
                k,
                current.description()
            ))
        } else {
            Err(anyhow!("no such setting `{}`", k))
        }
    }
}

impl Default for Settings {
    /// The default settings.
    fn default() -> Self {
        Self {
            map: hashmap! {
                "debug" => Value::Bool(false),
                "checker" => Value::Bool(false),
                "background" => Value::Rgba8(Rgba8::TRANSPARENT),
                "input/mouse" => Value::Bool(true),
                "scale" => Value::F64(1.0),
                "animation" => Value::Bool(true),
                "animation/delay" => Value::U32(160),
                "ui/palette" => Value::Bool(true),
                "ui/status" => Value::Bool(true),
                "ui/cursor" => Value::Bool(true),
                "ui/message" => Value::Bool(true),
                "ui/switcher" => Value::Bool(true),
                "ui/view-info" => Value::Bool(true),

                "grid" => Value::Bool(false),
                "grid/color" => Value::Rgba8(Rgba8::BLUE),
                "grid/spacing" => Value::U32Tuple(8, 8),

                "debug/crosshair" => Value::Bool(false)
            },
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
