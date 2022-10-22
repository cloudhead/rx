use std::borrow::Borrow;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::ui::{text, TextureId};

use super::TextureInfo;

#[derive(Debug)]
pub enum Value {
    Font(text::Font),
    Texture(TextureId),
    TextureInfo(TextureInfo),
}

#[derive(thiserror::Error, Debug)]
pub enum ValueTypeError {
    #[error("wrong type")]
    WrongType,
}

/// Values which can be stored in an environment.
pub trait ValueType: Sized + Clone + Into<Value> {
    /// Attempt to convert the generic `Value` into this type.
    fn try_from_value(v: &Value) -> Result<Self, ValueTypeError>;
}

impl From<TextureId> for Value {
    fn from(id: TextureId) -> Self {
        Self::Texture(id)
    }
}

impl ValueType for TextureId {
    fn try_from_value(v: &Value) -> Result<Self, ValueTypeError> {
        if let Value::Texture(id) = v {
            Ok(*id)
        } else {
            Err(ValueTypeError::WrongType)
        }
    }
}

impl From<TextureInfo> for Value {
    fn from(info: TextureInfo) -> Self {
        Self::TextureInfo(info)
    }
}

impl ValueType for TextureInfo {
    fn try_from_value(v: &Value) -> Result<Self, ValueTypeError> {
        if let Value::TextureInfo(info) = v {
            Ok(*info)
        } else {
            Err(ValueTypeError::WrongType)
        }
    }
}

#[derive(Default, Debug)]
pub struct Env {
    /// Map of user-defined data.
    map: HashMap<&'static str, Value>,
}

impl Env {
    pub fn get<V: ValueType>(&self, key: impl Borrow<Key<V>>) -> Option<V> {
        self.map
            .get(key.borrow().key)
            .map(|v| V::try_from_value(v).unwrap())
    }

    pub fn set<V: ValueType>(&mut self, key: Key<V>, value: impl Into<V>) {
        let key = key.into();
        let value = value.into().into();

        self.map.insert(key, value);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key<V> {
    key: &'static str,
    value: PhantomData<V>,
}

impl<V> Key<V> {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            value: PhantomData,
        }
    }
}

impl<V> From<Key<V>> for &'static str {
    fn from(key: Key<V>) -> Self {
        key.key
    }
}
