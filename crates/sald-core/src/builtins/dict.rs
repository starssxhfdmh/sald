use super::{check_arity, check_arity_range, get_string_arg};
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

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

fn dict_constructor(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;

    if args.is_empty() {
        Ok(Value::Dictionary(Rc::new(RefCell::new(
            FxHashMap::default(),
        ))))
    } else {
        match &args[0] {
            Value::Dictionary(source) => {
                let copy = source.borrow().clone();
                Ok(Value::Dictionary(Rc::new(RefCell::new(copy))))
            }
            _ => Err(format!(
                "Expected a dictionary, got {}",
                args[0].type_name()
            )),
        }
    }
}

fn dict_length(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => Ok(Value::Number(dict.borrow().len() as f64)),
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_keys(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let keys: Vec<Value> = dict
                .borrow()
                .keys()
                .map(|k| Value::String(Rc::from(k.clone())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(keys))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_values(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let values: Vec<Value> = dict.borrow().values().cloned().collect();
            Ok(Value::Array(Rc::new(RefCell::new(values))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_entries(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let entries: Vec<Value> = dict
                .borrow()
                .iter()
                .map(|(k, v)| {
                    Value::Array(Rc::new(RefCell::new(vec![
                        Value::String(Rc::from(k.clone())),
                        v.clone(),
                    ])))
                })
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(entries))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_get(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity_range(1, 2, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            let dict_ref = dict.borrow();
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

fn dict_set(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            dict.borrow_mut().insert(key, args[1].clone());
            Ok(Value::Null)
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_has(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            Ok(Value::Boolean(dict.borrow().contains_key(&key)))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_remove(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let key = get_string_arg(&args[0], "key")?;
            Ok(dict.borrow_mut().remove(&key).unwrap_or(Value::Null))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_clear(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            dict.borrow_mut().clear();
            Ok(Value::Null)
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => Ok(Value::Boolean(dict.borrow().is_empty())),
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}

fn dict_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    match recv {
        Value::Dictionary(dict) => {
            let dict_ref = dict.borrow();
            let items: Vec<String> = dict_ref
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, v))
                .collect();
            Ok(Value::String(Rc::from(format!("{{{}}}", items.join(", ")))))
        }
        _ => Err("Receiver must be a dictionary".to_string()),
    }
}
