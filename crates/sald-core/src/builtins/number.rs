// Number built-in class
// Instance methods for number operations
// Uses Arc for thread-safety

use super::check_arity;
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub fn create_number_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("abs".to_string(), number_abs);
    instance_methods.insert("floor".to_string(), number_floor);
    instance_methods.insert("ceil".to_string(), number_ceil);
    instance_methods.insert("round".to_string(), number_round);
    instance_methods.insert("toFixed".to_string(), number_to_fixed);
    instance_methods.insert("toString".to_string(), number_to_string);

    Class::new_with_instance("Number", instance_methods, Some(number_constructor))
}

fn number_constructor(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    match &args[0] {
        Value::Number(n) => Ok(Value::Number(*n)),
        Value::String(s) => s
            .parse::<f64>()
            .map(Value::Number)
            .map_err(|_| format!("Cannot convert '{}' to number", s)),
        Value::Boolean(b) => Ok(Value::Number(if *b { 1.0 } else { 0.0 })),
        Value::Null => Ok(Value::Number(0.0)),
        _ => Err(format!("Cannot convert {} to number", args[0].type_name())),
    }
}

fn number_abs(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Number(n) = recv {
        Ok(Value::Number(n.abs()))
    } else {
        Err("Receiver must be a number".to_string())
    }
}

fn number_floor(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Number(n) = recv {
        Ok(Value::Number(n.floor()))
    } else {
        Err("Receiver must be a number".to_string())
    }
}

fn number_ceil(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Number(n) = recv {
        Ok(Value::Number(n.ceil()))
    } else {
        Err("Receiver must be a number".to_string())
    }
}

fn number_round(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Number(n) = recv {
        Ok(Value::Number(n.round()))
    } else {
        Err("Receiver must be a number".to_string())
    }
}

/// toFixed(decimals) - Format number to string with fixed decimal places
fn number_to_fixed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Number(n) = recv {
        let decimals = match &args[0] {
            Value::Number(d) => *d as usize,
            _ => {
                return Err(format!(
                    "Argument 'decimals' must be a number, got {}",
                    args[0].type_name()
                ))
            }
        };

        // Cap at reasonable precision
        let decimals = decimals.min(20);

        Ok(Value::String(Arc::from(format!(
            "{:.prec$}",
            n,
            prec = decimals
        ))))
    } else {
        Err("Receiver must be a number".to_string())
    }
}

fn number_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Number(n) = recv {
        if n.fract() == 0.0 {
            Ok(Value::String(Arc::from(format!("{}", *n as i64))))
        } else {
            Ok(Value::String(Arc::from(format!("{}", n))))
        }
    } else {
        Err("Receiver must be a number".to_string())
    }
}
