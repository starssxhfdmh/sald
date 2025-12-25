use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

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

fn process_args(_args: &[Value]) -> Result<Value, String> {
    let args: Vec<Value> = std::env::args()
        .skip(1)
        .map(|s| Value::String(Rc::from(s)))
        .collect();

    Ok(Value::Array(Rc::new(RefCell::new(args))))
}

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
        Ok(val) => Ok(Value::String(Rc::from(val))),
        Err(_) => Ok(Value::Null),
    }
}

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

            if !stdout.is_empty() {
                Ok(Value::String(Rc::from(stdout)))
            } else if !stderr.is_empty() {
                Ok(Value::String(Rc::from(stderr)))
            } else {
                Ok(Value::String(Rc::from(String::new())))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn process_cwd(_args: &[Value]) -> Result<Value, String> {
    match std::env::current_dir() {
        Ok(path) => Ok(Value::String(Rc::from(path.to_string_lossy().to_string()))),
        Err(e) => Err(e.to_string()),
    }
}

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
