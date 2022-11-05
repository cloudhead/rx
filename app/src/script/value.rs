use std::fmt;

use memoir::traits::Parse;
use memoir::*;

use crate::gfx::prelude::*;
use crate::script::parsers::*;

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    Int(u32),
    Int2D(u32, u32),
    Float2D(f32, f32),
    Float(f64),
    Str(String),
    Ident(String),
    Color(Rgba8),
}

impl Value {
    pub fn is_set(&self) -> bool {
        if let Value::Bool(b) = self {
            return *b;
        }
        panic!("expected {:?} to be a `bool`", self);
    }

    pub fn to_f64(&self) -> f64 {
        if let Value::Float(n) = self {
            return *n;
        }
        panic!("expected {:?} to be a `float`", self);
    }

    pub fn to_u64(&self) -> u64 {
        if let Value::Int(n) = self {
            return *n as u64;
        }
        panic!("expected {:?} to be a `uint`", self);
    }

    pub fn to_rgba8(&self) -> Rgba8 {
        if let Value::Color(rgba8) = self {
            return *rgba8;
        }
        panic!("expected {:?} to be a `Rgba8`", self);
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Bool(_) => "on / off",
            Self::Int(_) => "positive integer, eg. 32",
            Self::Float(_) => "float, eg. 1.33",
            Self::Int2D(_, _) => "two positive integers, eg. 32, 48",
            Self::Float2D(_, _) => "two floats , eg. 32.17, 48.29",
            Self::Str(_) => "string, eg. \"fnord\"",
            Self::Color(_) => "color, eg. #ffff00",
            Self::Ident(_) => "identifier, eg. fnord",
        }
    }
}

impl From<Value> for (u32, u32) {
    fn from(other: Value) -> (u32, u32) {
        if let Value::Int2D(x, y) = other {
            return (x, y);
        }
        panic!("expected {:?} to be a `(u32, u32)`", other);
    }
}

impl From<Value> for f32 {
    fn from(other: Value) -> f32 {
        if let Value::Float(x) = other {
            return x as f32;
        }
        panic!("expected {:?} to be a `f64`", other);
    }
}

impl From<Value> for f64 {
    fn from(other: Value) -> f64 {
        if let Value::Float(x) = other {
            return x as f64;
        }
        panic!("expected {:?} to be a `f64`", other);
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(true) => "on".fmt(f),
            Value::Bool(false) => "off".fmt(f),
            Value::Int(u) => u.fmt(f),
            Value::Float(x) => x.fmt(f),
            Value::Int2D(x, y) => write!(f, "{},{}", x, y),
            Value::Float2D(x, y) => write!(f, "{},{}", x, y),
            Value::Str(s) => s.fmt(f),
            Value::Color(c) => c.fmt(f),
            Value::Ident(i) => i.fmt(f),
        }
    }
}

impl Parse for Value {
    fn parser() -> Parser<Self> {
        let str_val = quoted().map(Value::Str).label("<string>");
        let rgba8_val = color().map(Value::Color);
        let u32_tuple_val = tuple::<u32>(natural(), natural()).map(|(x, y)| Value::Int2D(x, y));
        let u32_val = natural::<u32>().map(Value::Int);
        let f64_tuple_val = tuple::<f32>(rational(), rational()).map(|(x, y)| Value::Float2D(x, y));
        let f64_val = rational::<f64>().map(Value::Float).label("0.0 .. 4096.0");
        let bool_val = string("on")
            .value(Value::Bool(true))
            .or(string("off").value(Value::Bool(false)))
            .label("on/off");
        let ident_val = identifier().map(Value::Ident);

        greediest(vec![
            rgba8_val,
            u32_tuple_val,
            f64_tuple_val,
            u32_val,
            f64_val,
            bool_val,
            ident_val,
            str_val,
        ])
        .label("<value>")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tes_value_parser() {
        let p = Value::parser();

        assert_eq!(p.parse("1.0 2.0").unwrap(), (Value::Float2D(1.0, 2.0), ""));
        assert_eq!(p.parse("1.0").unwrap(), (Value::Float(1.0), ""));
        assert_eq!(p.parse("1").unwrap(), (Value::Int(1), ""));
        assert_eq!(p.parse("1 2").unwrap(), (Value::Int2D(1, 2), ""));
        assert_eq!(p.parse("on").unwrap(), (Value::Bool(true), ""));
        assert_eq!(p.parse("off").unwrap(), (Value::Bool(false), ""));
        assert_eq!(
            p.parse("#ff00ff").unwrap(),
            (Value::Color(Rgba8::new(0xff, 0x0, 0xff, 0xff)), "")
        );
    }
}
