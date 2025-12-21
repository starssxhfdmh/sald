// Type built-in class
// Provides: of, isString, isNumber, etc.
// Uses Arc for thread-safety

use super::check_arity;
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub fn create_type_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("of".to_string(), type_of);
    static_methods.insert("isString".to_string(), type_is_string);
    static_methods.insert("isNumber".to_string(), type_is_number);
    static_methods.insert("isBoolean".to_string(), type_is_boolean);
    static_methods.insert("isNull".to_string(), type_is_null);
    static_methods.insert("isArray".to_string(), type_is_array);
    static_methods.insert("isFunction".to_string(), type_is_function);
    static_methods.insert("isClass".to_string(), type_is_class);
    static_methods.insert("isInstance".to_string(), type_is_instance);
    static_methods.insert("isDict".to_string(), type_is_dictonary);
    Class::new_with_static("Type", static_methods)
}

fn type_of(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::String(Arc::new(args[0].type_name().to_string())))
}

fn type_is_string(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::String(_))))
}

fn type_is_number(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Number(_))))
}

fn type_is_boolean(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Boolean(_))))
}

fn type_is_null(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Null)))
}

fn type_is_array(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Array(_))))
}

fn type_is_function(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(
        args[0],
        Value::Function(_) | Value::NativeFunction { .. }
    )))
}

fn type_is_class(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Class(_))))
}

fn type_is_instance(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Instance(_))))
}

fn type_is_dictonary(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::Boolean(matches!(args[0], Value::Dictionary(_))))
}