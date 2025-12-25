use super::check_arity;
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub fn create_boolean_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("toString".to_string(), boolean_to_string);

    Class::new_with_instance("Boolean", instance_methods, Some(boolean_constructor))
}

fn boolean_constructor(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(args[0].is_truthy()))
}

fn boolean_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Boolean(b) = recv {
        Ok(Value::String(Rc::from(b.to_string())))
    } else {
        Err("Receiver must be a boolean".to_string())
    }
}
