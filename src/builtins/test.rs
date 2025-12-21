// Test built-in class
// Provides assertion functions for testing: Test.assert, Test.assert_eq, etc.
// Also serves as a decorator: @Test marks a function as a test

use super::check_arity;
use super::check_arity_min;
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;

pub fn create_test_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("assert".to_string(), test_assert);
    static_methods.insert("assert_eq".to_string(), test_assert_eq);
    static_methods.insert("assert_ne".to_string(), test_assert_ne);
    static_methods.insert("fail".to_string(), test_fail);

    let mut class = Class::new_with_static("Test", static_methods);
    
    // Add constructor that acts as identity decorator
    // When used as @Test, it receives the function and returns it unchanged
    class.constructor = Some(test_decorator);
    
    class
}

/// Test decorator - identity function for marking test functions
/// When used as @Test, this receives the decorated function and returns it unchanged
fn test_decorator(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(args[0].clone())
}

/// Test.assert(condition, ?message)
/// Fails if condition is falsy
fn test_assert(args: &[Value]) -> Result<Value, String> {
    check_arity_min(1, args.len())?;
    
    let condition = &args[0];
    let message = if args.len() > 1 {
        match &args[1] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        "Assertion failed".to_string()
    };
    
    if !condition.is_truthy() {
        return Err(format!("AssertionError: {}", message));
    }
    
    Ok(Value::Null)
}

/// Test.assert_eq(actual, expected, ?message)
/// Fails if actual != expected
fn test_assert_eq(args: &[Value]) -> Result<Value, String> {
    check_arity_min(2, args.len())?;
    
    let actual = &args[0];
    let expected = &args[1];
    let message = if args.len() > 2 {
        match &args[2] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        format!("Expected {} but got {}", expected, actual)
    };
    
    if actual != expected {
        return Err(format!("AssertionError: {}", message));
    }
    
    Ok(Value::Null)
}

/// Test.assert_ne(actual, expected, ?message)
/// Fails if actual == expected
fn test_assert_ne(args: &[Value]) -> Result<Value, String> {
    check_arity_min(2, args.len())?;
    
    let actual = &args[0];
    let expected = &args[1];
    let message = if args.len() > 2 {
        match &args[2] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        format!("Expected {} to not equal {}", actual, expected)
    };
    
    if actual == expected {
        return Err(format!("AssertionError: {}", message));
    }
    
    Ok(Value::Null)
}

/// Test.fail(?message)
/// Unconditionally fails the test
fn test_fail(args: &[Value]) -> Result<Value, String> {
    let message = if !args.is_empty() {
        match &args[0] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        "Test failed".to_string()
    };
    
    Err(format!("AssertionError: {}", message))
}
