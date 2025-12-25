use crate::vm::value::Value;

pub type NativeFunction = fn(&[Value]) -> Value;
