



use super::check_arity;
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub fn create_null_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("toString".to_string(), null_to_string);

    Class::new_with_instance("Null", instance_methods, None)
}

fn null_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if matches!(recv, Value::Null) {
        Ok(Value::String(Rc::from("null".to_string())))
    } else {
        Err("Receiver must be null".to_string())
    }
}
