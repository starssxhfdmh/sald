// Sald Native Functions
// Reserved for future use - native functions are currently handled inline in the compiler

use crate::vm::value::Value;

/// Native function signature
pub type NativeFunction = fn(&[Value]) -> Value;
