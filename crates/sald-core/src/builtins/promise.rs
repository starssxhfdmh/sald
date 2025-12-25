



use super::check_arity;
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_promise_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("all".to_string(), promise_all);
    static_methods.insert("race".to_string(), promise_race);
    static_methods.insert("resolve".to_string(), promise_resolve);
    static_methods.insert("reject".to_string(), promise_reject);

    Class::new_with_static("Promise", static_methods)
}



fn promise_all(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    match &args[0] {
        Value::Array(arr) => {
            let arr_ref = arr.borrow();
            
            let results: Vec<Value> = arr_ref.clone();
            Ok(Value::Array(Rc::new(RefCell::new(results))))
        }
        _ => Err("Promise.all() expects an array".to_string()),
    }
}



fn promise_race(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    match &args[0] {
        Value::Array(arr) => {
            let arr_ref = arr.borrow();
            if arr_ref.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(arr_ref[0].clone())
            }
        }
        _ => Err("Promise.race() expects an array".to_string()),
    }
}


fn promise_resolve(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(args[0].clone())
}


fn promise_reject(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    let error = match &args[0] {
        Value::String(s) => s.to_string(),
        other => format!("{}", other),
    };

    Err(error)
}
