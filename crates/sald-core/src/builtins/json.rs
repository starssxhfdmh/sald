use super::{check_arity, check_arity_range, get_number_arg, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_json_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("parse".to_string(), json_parse);
    static_methods.insert("stringify".to_string(), json_stringify);

    Class::new_with_static("Json", static_methods)
}

fn json_parse(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let json_str = get_string_arg(&args[0], "json")?;

    serde_json::from_str::<serde_json::Value>(&json_str)
        .map_err(|e| e.to_string())
        .and_then(|json_value| json_to_sald_value(&json_value))
}

fn json_stringify(args: &[Value]) -> Result<Value, String> {
    check_arity_range(1, 2, args.len())?;

    let json_string = if args.len() == 2 {
        let json_value = sald_value_to_json(&args[0])?;
        let indent = get_number_arg(&args[1], "indent")? as usize;
        let indent_str = " ".repeat(indent);

        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        json_value.serialize(&mut ser).map_err(|e| e.to_string())?;
        String::from_utf8(buf).map_err(|e| e.to_string())?
    } else {
        let mut buf = String::with_capacity(256);
        write_json_value(&args[0], &mut buf)?;
        buf
    };

    Ok(Value::String(Rc::from(json_string)))
}

fn json_to_sald_value(json: &serde_json::Value) -> Result<Value, String> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| "Cannot convert JSON number".to_string()),
        serde_json::Value::String(s) => Ok(Value::String(Rc::from(s.clone()))),
        serde_json::Value::Array(arr) => {
            let sald_arr: Result<Vec<Value>, String> = arr.iter().map(json_to_sald_value).collect();
            Ok(Value::Array(Rc::new(RefCell::new(sald_arr?))))
        }
        serde_json::Value::Object(obj) => {
            let mut map = FxHashMap::default();
            for (key, value) in obj {
                map.insert(key.clone(), json_to_sald_value(value)?);
            }
            Ok(Value::Dictionary(Rc::new(RefCell::new(map))))
        }
    }
}

fn sald_value_to_json(value: &Value) -> Result<serde_json::Value, String> {
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "Cannot convert number to JSON".to_string()),
        Value::String(s) => Ok(serde_json::Value::String(s.to_string())),
        Value::Array(arr) => {
            let guard = arr.borrow();
            let json_arr: Result<Vec<serde_json::Value>, String> =
                guard.iter().map(sald_value_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        Value::Dictionary(dict) => {
            let mut map = serde_json::Map::new();
            let guard = dict.borrow();
            for (key, val) in guard.iter() {
                map.insert(key.clone(), sald_value_to_json(val)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Err(format!("Cannot convert {} to JSON", value.type_name())),
    }
}

fn write_json_value(value: &Value, buf: &mut String) -> Result<(), String> {
    use std::fmt::Write;
    match value {
        Value::Null => {
            buf.push_str("null");
            Ok(())
        }
        Value::Boolean(b) => {
            buf.push_str(if *b { "true" } else { "false" });
            Ok(())
        }
        Value::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                return Err("Cannot convert NaN/Infinity to JSON".to_string());
            }
            if n.fract() == 0.0 && n.abs() < 1e15 {
                let _ = write!(buf, "{}", *n as i64);
            } else {
                let _ = write!(buf, "{}", n);
            }
            Ok(())
        }
        Value::String(s) => {
            buf.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => buf.push_str("\\\""),
                    '\\' => buf.push_str("\\\\"),
                    '\n' => buf.push_str("\\n"),
                    '\r' => buf.push_str("\\r"),
                    '\t' => buf.push_str("\\t"),
                    c if c.is_control() => {
                        let _ = write!(buf, "\\u{:04x}", c as u32);
                    }
                    c => buf.push(c),
                }
            }
            buf.push('"');
            Ok(())
        }
        Value::Array(arr) => {
            buf.push('[');
            let guard = arr.borrow();
            let mut first = true;
            for item in guard.iter() {
                if !first {
                    buf.push(',');
                }
                first = false;
                write_json_value(item, buf)?;
            }
            buf.push(']');
            Ok(())
        }
        Value::Dictionary(dict) => {
            buf.push('{');
            let guard = dict.borrow();
            let mut first = true;
            for (key, val) in guard.iter() {
                if !first {
                    buf.push(',');
                }
                first = false;

                buf.push('"');
                for ch in key.chars() {
                    match ch {
                        '"' => buf.push_str("\\\""),
                        '\\' => buf.push_str("\\\\"),
                        c => buf.push(c),
                    }
                }
                buf.push_str("\":");
                write_json_value(val, buf)?;
            }
            buf.push('}');
            Ok(())
        }
        _ => Err(format!("Cannot convert {} to JSON", value.type_name())),
    }
}
