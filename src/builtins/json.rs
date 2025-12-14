// Json built-in class
// Static methods for JSON parsing and serialization
// Uses Arc/Mutex for thread-safety

use super::{check_arity, check_arity_range, get_number_arg, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, Value};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn create_json_class() -> Class {
    let mut static_methods: HashMap<String, NativeStaticFn> = HashMap::new();

    static_methods.insert("parse".to_string(), json_parse);
    static_methods.insert("stringify".to_string(), json_stringify);

    Class::new_with_static("Json", static_methods)
}

/// Json.parse(string) - Parse JSON string to Dict/Array/primitive
fn json_parse(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let json_str = get_string_arg(&args[0], "json")?;

    serde_json::from_str::<serde_json::Value>(&json_str)
        .map_err(|e| e.to_string())
        .and_then(|json_value| json_to_sald_value(&json_value))
}

/// Json.stringify(value) or Json.stringify(value, indent) - Convert to JSON string
fn json_stringify(args: &[Value]) -> Result<Value, String> {
    check_arity_range(1, 2, args.len())?;

    let json_value = sald_value_to_json(&args[0])?;

    let json_string = if args.len() == 2 {
        let indent = get_number_arg(&args[1], "indent")? as usize;
        let indent_str = " ".repeat(indent);

        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        json_value.serialize(&mut ser).map_err(|e| e.to_string())?;
        String::from_utf8(buf).map_err(|e| e.to_string())?
    } else {
        serde_json::to_string(&json_value).map_err(|e| e.to_string())?
    };

    Ok(Value::String(Arc::new(json_string)))
}

/// Convert serde_json::Value to Sald Value
fn json_to_sald_value(json: &serde_json::Value) -> Result<Value, String> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| "Cannot convert JSON number".to_string()),
        serde_json::Value::String(s) => Ok(Value::String(Arc::new(s.clone()))),
        serde_json::Value::Array(arr) => {
            let sald_arr: Result<Vec<Value>, String> = arr.iter().map(json_to_sald_value).collect();
            Ok(Value::Array(Arc::new(Mutex::new(sald_arr?))))
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (key, value) in obj {
                map.insert(key.clone(), json_to_sald_value(value)?);
            }
            Ok(Value::Dictionary(Arc::new(Mutex::new(map))))
        }
    }
}

/// Convert Sald Value to serde_json::Value
fn sald_value_to_json(value: &Value) -> Result<serde_json::Value, String> {
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "Cannot convert number to JSON".to_string()),
        Value::String(s) => Ok(serde_json::Value::String(s.to_string())),
        Value::Array(arr) => {
            let json_arr: Result<Vec<serde_json::Value>, String> =
                arr.lock().unwrap().iter().map(sald_value_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        Value::Dictionary(dict) => {
            let mut map = serde_json::Map::new();
            for (key, val) in dict.lock().unwrap().iter() {
                map.insert(key.clone(), sald_value_to_json(val)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Err(format!("Cannot convert {} to JSON", value.type_name())),
    }
}
