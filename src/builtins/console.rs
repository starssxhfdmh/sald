// Console built-in class
// Provides: print, println, input, clear

use super::check_arity;
use crate::vm::value::{Class, NativeStaticFn, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub fn create_console_class() -> Class {
    let mut static_methods: HashMap<String, NativeStaticFn> = HashMap::new();

    static_methods.insert("print".to_string(), console_print);
    static_methods.insert("println".to_string(), console_println);
    static_methods.insert("input".to_string(), console_input);
    static_methods.insert("clear".to_string(), console_clear);

    Class::new_with_static("Console", static_methods)
}

fn console_print(args: &[Value]) -> Result<Value, String> {
    let mut output = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            output.push(' ');
        }
        output.push_str(&arg.to_string());
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        print!("{}", output);
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        crate::wasm::wasm_println(output);
    }
    
    Ok(Value::Null)
}

fn console_println(args: &[Value]) -> Result<Value, String> {
    let mut output = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            output.push(' ');
        }
        output.push_str(&arg.to_string());
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("{}", output);
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        crate::wasm::wasm_println(output);
    }
    
    Ok(Value::Null)
}

fn console_input(args: &[Value]) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Optional prompt
        if !args.is_empty() {
            print!("{}", args[0]);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;

        let trimmed = input.trim_end_matches('\n').trim_end_matches('\r');
        Ok(Value::String(Arc::new(trimmed.to_string())))
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        let _ = args;
        Err("Console.input() is not available in WASM playground".to_string())
    }
}

fn console_clear(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    
    #[cfg(not(target_arch = "wasm32"))]
    {
        print!("\x1B[2J\x1B[H");
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
    
    // In WASM, clear is a no-op (could be handled by JS)
    
    Ok(Value::Null)
}
