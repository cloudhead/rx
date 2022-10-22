use std::fmt;

use memoir::traits::Parse;
use memoir::*;

use crate::gfx::prelude::*;
use crate::script::parsers::*;

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    U32(u32),
    U32Tuple(u32, u32),
    F32Tuple(f32, f32),
    F64(f64),
    Str(String),
    Ident(String),
    Rgba8(Rgba8),
}

impl Value {
    pub fn is_set(&self) -> bool {
        if let Value::Bool(b) = self {
            return *b;
        }
        panic!("expected {:?} to be a `bool`", self);
    }

    pub fn to_f64(&self) -> f64 {
        if let Value::F64(n) = self {
            return *n;
        }
        panic!("expected {:?} to be a `float`", self);
    }

    pub fn to_u64(&self) -> u64 {
        if let Value::U32(n) = self {
            return *n as u64;
        }
        panic!("expected {:?} to be a `uint`", self);
    }

    pub fn to_rgba8(&self) -> Rgba8 {
        if let Value::Rgba8(rgba8) = self {
            return *rgba8;
        }
        panic!("expected {:?} to be a `Rgba8`", self);
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Bool(_) => "on / off",
            Self::U32(_) => "positive integer, eg. 32",
            Self::F64(_) => "float, eg. 1.33",
            Self::U32Tuple(_, _) => "two positive integers, eg. 32, 48",
            Self::F32Tuple(_, _) => "two floats , eg. 32.17, 48.29",
            Self::Str(_) => "string, eg. \"fnord\"",
            Self::Rgba8(_) => "color, eg. #ffff00",
            Self::Ident(_) => "identifier, eg. fnord",
        }
    }
}

impl From<Value> for (u32, u32) {
    fn from(other: Value) -> (u32, u32) {
        if let Value::U32Tuple(x, y) = other {
            return (x, y);
        }
        panic!("expected {:?} to be a `(u32, u32)`", other);
    }
}

impl From<Value> for f32 {
    fn from(other: Value) -> f32 {
        if let Value::F64(x) = other {
            return x as f32;
        }
        panic!("expected {:?} to be a `f64`", other);
    }
}

impl From<Value> for f64 {
    fn from(other: Value) -> f64 {
        if let Value::F64(x) = other {
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
            Value::U32(u) => u.fmt(f),
            Value::F64(x) => x.fmt(f),
            Value::U32Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::F32Tuple(x, y) => write!(f, "{},{}", x, y),
            Value::Str(s) => s.fmt(f),
            Value::Rgba8(c) => c.fmt(f),
            Value::Ident(i) => i.fmt(f),
        }
    }
}

impl Parse for Value {
    fn parser() -> Parser<Self> {
        let str_val = quoted().map(Value::Str).label("<string>");
        let rgba8_val = color().map(Value::Rgba8);
        let u32_tuple_val = tuple::<u32>(natural(), natural()).map(|(x, y)| Value::U32Tuple(x, y));
        let u32_val = natural::<u32>().map(Value::U32);
        let f64_tuple_val =
            tuple::<f32>(rational(), rational()).map(|(x, y)| Value::F32Tuple(x, y));
        let f64_val = rational::<f64>().map(Value::F64).label("0.0 .. 4096.0");
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

        assert_eq!(p.parse("1.0 2.0").unwrap(), (Value::F32Tuple(1.0, 2.0), ""));
        assert_eq!(p.parse("1.0").unwrap(), (Value::F64(1.0), ""));
        assert_eq!(p.parse("1").unwrap(), (Value::U32(1), ""));
        assert_eq!(p.parse("1 2").unwrap(), (Value::U32Tuple(1, 2), ""));
        assert_eq!(p.parse("on").unwrap(), (Value::Bool(true), ""));
        assert_eq!(p.parse("off").unwrap(), (Value::Bool(false), ""));
        assert_eq!(
            p.parse("#ff00ff").unwrap(),
            (Value::Rgba8(Rgba8::new(0xff, 0x0, 0xff, 0xff)), "")
        );
    }
}
