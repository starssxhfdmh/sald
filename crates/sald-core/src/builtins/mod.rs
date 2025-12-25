




mod array;
mod boolean;
mod console;
mod dict;
mod json;
mod math;
mod null;
mod number;
mod regex;
mod string;
mod types;


#[cfg(not(target_arch = "wasm32"))]
mod channel;
#[cfg(not(target_arch = "wasm32"))]
mod crypto;
#[cfg(not(target_arch = "wasm32"))]
mod date;
#[cfg(not(target_arch = "wasm32"))]
mod ffi;
#[cfg(not(target_arch = "wasm32"))]
mod file;
#[cfg(not(target_arch = "wasm32"))]
mod path;
#[cfg(not(target_arch = "wasm32"))]
mod process;
#[cfg(not(target_arch = "wasm32"))]
mod promise;
#[cfg(not(target_arch = "wasm32"))]
mod system;
#[cfg(not(target_arch = "wasm32"))]
mod test;
#[cfg(not(target_arch = "wasm32"))]
mod timer;

use crate::vm::value::Value;
use rustc_hash::FxHashMap;
use std::rc::Rc;


pub use array::create_array_class;
pub use boolean::create_boolean_class;
pub use console::create_console_class;
pub use dict::create_dict_class;
pub use json::create_json_class;
pub use math::create_math_class;
pub use null::create_null_class;
pub use number::create_number_class;
pub use regex::create_regex_class;
pub use string::create_string_class;
pub use types::create_type_class;


#[cfg(not(target_arch = "wasm32"))]
pub use channel::create_channel_class;
#[cfg(not(target_arch = "wasm32"))]
pub use crypto::create_crypto_class;
#[cfg(not(target_arch = "wasm32"))]
pub use date::create_date_class;
#[cfg(not(target_arch = "wasm32"))]
pub use ffi::create_ffi_namespace;
#[cfg(not(target_arch = "wasm32"))]
pub use file::create_file_class;
#[cfg(not(target_arch = "wasm32"))]

#[cfg(not(target_arch = "wasm32"))]
pub use path::create_path_class;
#[cfg(not(target_arch = "wasm32"))]
pub use process::create_process_class;
#[cfg(not(target_arch = "wasm32"))]
pub use promise::create_promise_class;
#[cfg(not(target_arch = "wasm32"))]
pub use system::create_system_class;
#[cfg(not(target_arch = "wasm32"))]
pub use test::create_test_class;
#[cfg(not(target_arch = "wasm32"))]
pub use timer::create_timer_class;


pub type NativeStaticFn = fn(&[Value]) -> Result<Value, String>;


pub type NativeInstanceFn = fn(&Value, &[Value]) -> Result<Value, String>;


pub fn create_builtin_classes() -> FxHashMap<String, Value> {
    let mut classes = FxHashMap::default();

    
    classes.insert(
        "String".to_string(),
        Value::Class(Rc::new(create_string_class())),
    );
    classes.insert(
        "Number".to_string(),
        Value::Class(Rc::new(create_number_class())),
    );
    classes.insert(
        "Boolean".to_string(),
        Value::Class(Rc::new(create_boolean_class())),
    );
    classes.insert(
        "Null".to_string(),
        Value::Class(Rc::new(create_null_class())),
    );
    classes.insert(
        "Array".to_string(),
        Value::Class(Rc::new(create_array_class())),
    );
    classes.insert(
        "Dict".to_string(),
        Value::Class(Rc::new(create_dict_class())),
    );
    classes.insert(
        "Console".to_string(),
        Value::Class(Rc::new(create_console_class())),
    );
    classes.insert(
        "Type".to_string(),
        Value::Class(Rc::new(create_type_class())),
    );
    classes.insert(
        "Math".to_string(),
        Value::Class(Rc::new(create_math_class())),
    );
    classes.insert(
        "Json".to_string(),
        Value::Class(Rc::new(create_json_class())),
    );
    classes.insert(
        "Regex".to_string(),
        Value::Class(Rc::new(create_regex_class())),
    );

    
    #[cfg(not(target_arch = "wasm32"))]
    {
        classes.insert(
            "Date".to_string(),
            Value::Class(Rc::new(create_date_class())),
        );
        classes.insert(
            "File".to_string(),
            Value::Class(Rc::new(create_file_class())),
        );
        classes.insert(
            "Timer".to_string(),
            Value::Class(Rc::new(create_timer_class())),
        );
        classes.insert(
            "Path".to_string(),
            Value::Class(Rc::new(create_path_class())),
        );
        classes.insert(
            "Process".to_string(),
            Value::Class(Rc::new(create_process_class())),
        );
        
        classes.insert("Ffi".to_string(), create_ffi_namespace());
        classes.insert(
            "System".to_string(),
            Value::Class(Rc::new(create_system_class())),
        );
        classes.insert(
            "Crypto".to_string(),
            Value::Class(Rc::new(create_crypto_class())),
        );
        classes.insert(
            "Channel".to_string(),
            Value::Class(Rc::new(create_channel_class())),
        );
        classes.insert(
            "Promise".to_string(),
            Value::Class(Rc::new(create_promise_class())),
        );
        classes.insert(
            "Test".to_string(),
            Value::Class(Rc::new(create_test_class())),
        );
    }

    classes
}


pub fn get_builtin_class_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "String",
        Value::Number(_) => "Number",
        Value::Boolean(_) => "Boolean",
        Value::Null => "Null",
        Value::Array(_) => "Array",
        Value::Dictionary(_) => "Dict",
        Value::Function(_) => "Function",
        Value::NativeFunction { .. } => "Function",
        Value::InstanceMethod { .. } => "Function",
        Value::BoundMethod { .. } => "Function",
        Value::Class(_) => "Class",
        Value::Instance(inst) => {
            let _ = inst;
            "Instance"
        }
        Value::Future(_) => "Future",
        Value::Namespace { .. } => "Namespace",
        Value::Enum { .. } => "Enum",
        Value::SpreadMarker(_) => "SpreadMarker",
    }
}


pub fn check_arity(expected: usize, got: usize) -> Result<(), String> {
    if expected != got {
        Err(format!(
            "Expected {} argument{} but got {}",
            expected,
            if expected == 1 { "" } else { "s" },
            got
        ))
    } else {
        Ok(())
    }
}


pub fn check_arity_range(min: usize, max: usize, got: usize) -> Result<(), String> {
    if got < min || got > max {
        Err(format!(
            "Expected {}-{} arguments but got {}",
            min, max, got
        ))
    } else {
        Ok(())
    }
}


pub fn check_arity_min(min: usize, got: usize) -> Result<(), String> {
    if got < min {
        Err(format!(
            "Expected at least {} argument{} but got {}",
            min,
            if min == 1 { "" } else { "s" },
            got
        ))
    } else {
        Ok(())
    }
}


pub fn get_string_arg(value: &Value, arg_name: &str) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.to_string()),
        _ => Err(format!(
            "Argument '{}' must be a string, got {}",
            arg_name,
            value.type_name()
        )),
    }
}


pub fn get_number_arg(value: &Value, arg_name: &str) -> Result<f64, String> {
    match value {
        Value::Number(n) => Ok(*n),
        _ => Err(format!(
            "Argument '{}' must be a number, got {}",
            arg_name,
            value.type_name()
        )),
    }
}


pub fn get_bool_arg(value: &Value, arg_name: &str) -> Result<bool, String> {
    match value {
        Value::Boolean(b) => Ok(*b),
        _ => Err(format!(
            "Argument '{}' must be a boolean, got {}",
            arg_name,
            value.type_name()
        )),
    }
}
