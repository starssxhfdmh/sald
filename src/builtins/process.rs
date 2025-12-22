// Process built-in class
// Provides: args, env, exit, exec
// Uses Arc/Mutex for thread-safety

use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::Mutex;

pub fn create_process_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("args".to_string(), process_args);
    static_methods.insert("env".to_string(), process_env);
    static_methods.insert("exit".to_string(), process_exit);
    static_methods.insert("exec".to_string(), process_exec);
    static_methods.insert("cwd".to_string(), process_cwd);
    static_methods.insert("chdir".to_string(), process_chdir);

    Class::new_with_static("Process", static_methods)
}

/// Get command line arguments as array
fn process_args(_args: &[Value]) -> Result<Value, String> {
    let args: Vec<Value> = std::env::args()
        .skip(1) // Skip the executable name
        .map(|s| Value::String(Arc::new(s)))
        .collect();

    Ok(Value::Array(Arc::new(Mutex::new(args))))
}

/// Get environment variable
fn process_env(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let var_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'name' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    match std::env::var(&var_name) {
        Ok(val) => Ok(Value::String(Arc::new(val))),
        Err(_) => Ok(Value::Null),
    }
}

/// Exit the process with given code
fn process_exit(args: &[Value]) -> Result<Value, String> {
    let code = if args.is_empty() {
        0
    } else {
        match &args[0] {
            Value::Number(n) => *n as i32,
            _ => {
                return Err(format!(
                    "Argument 'code' must be a number, got {}",
                    args[0].type_name()
                ))
            }
        }
    };

    std::process::exit(code);
}

/// Execute a shell command and return output
fn process_exec(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let command = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'command' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    // Use shell to execute command
    #[cfg(windows)]
    let output = std::process::Command::new("cmd")
        .args(["/C", &command])
        .output();

    #[cfg(not(windows))]
    let output = std::process::Command::new("sh")
        .args(["-c", &command])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Return stdout, or stderr if stdout is empty
            if !stdout.is_empty() {
                Ok(Value::String(Arc::new(stdout)))
            } else if !stderr.is_empty() {
                Ok(Value::String(Arc::new(stderr)))
            } else {
                Ok(Value::String(Arc::new(String::new())))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Get current working directory
fn process_cwd(_args: &[Value]) -> Result<Value, String> {
    match std::env::current_dir() {
        Ok(path) => Ok(Value::String(Arc::new(path.to_string_lossy().to_string()))),
        Err(e) => Err(e.to_string()),
    }
}

/// Change current working directory
fn process_chdir(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'path' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    match std::env::set_current_dir(&path) {
        Ok(_) => Ok(Value::Boolean(true)),
        Err(e) => Err(e.to_string()),
    }
}
