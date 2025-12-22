// Dictionary built-in class
// Instance methods for dictionary operations
// Uses Arc/Mutex for thread-safety

use super::{check_arity, check_arity_range, get_string_arg};
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::Mutex;

pub fn create_dict_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("length".to_string(), dict_length);
    instance_methods.insert("keys".to_string(), dict_keys);
    instance_methods.insert("values".to_string(), dict_values);
    instance_methods.insert("entries".to_string(), dict_entries);
    instance_methods.insert("get".to_string(), dict_get);
    instance_methods.insert("set".to_string(), dict_set);
    instance_methods.insert("has".to_string(), dict_has);
    instance_methods.insert("remove".to_string(), dict_remove);
    instance_methods.insert("clear".to_string(), dict_clear);
    instance_methods.insert("isEmpty".to_string(), dict_is_empty);
    instance_methods.insert("toString".to_string(), dict_to_string);

    Class::new_with_instance("Dict", instance_methods, Some(dict_constructor))
}

/// Dict() or Dict(source) - constructor
fn dict_constructor(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;

    if args.is_empty() {
        Ok(Value::Dictionary(Arc::new(Mutex::new(FxHashMap::default()))))
    } else {
        match &args[0] {
            Value::Dictionary(source) => {
                let copy = source.lock().clone();
                Ok(Value::Dictionary(Arc::new(Mutex::new(copy))))
            }
            _ => Err(format!(
                "Expected a dictionary, got {}",
                args[0].type_name()
            )),
        }
    }
}

/// dict.length() - get number of key-value pairs
fn dict_length(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => Ok(Value::Number(dict.lock().len() as f64)),
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.keys() - get array of all keys
fn dict_keys(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let keys: Vec<Value> = dict
                .lock()
                .keys()
                .map(|k| Value::String(Arc::from(k.clone())))
                .collect();
            Ok(Value::Array(Arc::new(Mutex::new(keys))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.values() - get array of all values
fn dict_values(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let values: Vec<Value> = dict.lock().values().cloned().collect();
            Ok(Value::Array(Arc::new(Mutex::new(values))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.entries() - get array of [key, value] pairs
fn dict_entries(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let entries: Vec<Value> = dict
                .lock()
                .iter()
                .map(|(k, v)| {
                    Value::Array(Arc::new(Mutex::new(vec![
                        Value::String(Arc::from(k.clone())),
                        v.clone(),
                    ])))
                })
                .collect();
            Ok(Value::Array(Arc::new(Mutex::new(entries))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.get(key, default?) - get value for key with optional default
fn dict_get(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity_range(1, 2, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            let dict_ref = dict.lock();
            match dict_ref.get(&key) {
                Some(value) => Ok(value.clone()),
                None => {
                    if args.len() == 2 {
                        Ok(args[1].clone())
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.set(key, value) - set a key-value pair
fn dict_set(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            dict.lock().insert(key, args[1].clone());
            Ok(Value::Null)
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.has(key) - check if key exists
fn dict_has(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            Ok(Value::Boolean(dict.lock().contains_key(&key)))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.remove(key) - remove key and return its value
fn dict_remove(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            Ok(dict.lock().remove(&key).unwrap_or(Value::Null))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.clear() - remove all key-value pairs
fn dict_clear(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            dict.lock().clear();
            Ok(Value::Null)
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.isEmpty() - return true if empty
fn dict_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => Ok(Value::Boolean(dict.lock().is_empty())),
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

/// dict.toString() - string representation
fn dict_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let dict_ref = dict.lock();
            let items: Vec<String> = dict_ref
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, v))
                .collect();
            Ok(Value::String(Arc::from(format!("{{{}}}", items.join(", ")))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}
