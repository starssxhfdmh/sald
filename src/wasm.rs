// Sald WASM Module
// Entry point for WebAssembly builds

use wasm_bindgen::prelude::*;
use crate::compiler::Compiler;
use crate::lexer::Scanner;
use crate::parser::Parser;
use crate::vm::VM;
use parking_lot::Mutex;

// Captured output for WASM (since we can't use stdout)
static WASM_OUTPUT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Append output to WASM output buffer
pub fn wasm_println(msg: String) {
    if let Ok(mut output) = WASM_OUTPUT.lock() {
        output.push(msg);
    }
}

/// Clear WASM output buffer
fn clear_output() {
    if let Ok(mut output) = WASM_OUTPUT.lock() {
        output.clear();
    }
}

/// Get and clear WASM output
fn take_output() -> String {
    if let Ok(mut output) = WASM_OUTPUT.lock() {
        let result = output.join("\n");
        output.clear();
        result
    } else {
        String::new()
    }
}

/// Run Sald code and return the output
#[wasm_bindgen]
pub fn run_code(source: &str) -> String {
    clear_output();
    
    match run_code_internal(source) {
        Ok(result) => {
            let output = take_output();
            if output.is_empty() {
                result
            } else if result.is_empty() || result == "null" {
                output
            } else {
                format!("{}\n=> {}", output, result)
            }
        }
        Err(e) => {
            let output = take_output();
            if output.is_empty() {
                format!("Error: {}", e)
            } else {
                format!("{}\nError: {}", output, e)
            }
        }
    }
}

fn run_code_internal(source: &str) -> Result<String, String> {
    // Lexer
    let mut scanner = Scanner::new(source, "<wasm>");
    let tokens = scanner.scan_tokens().map_err(|e| e.message)?;
    
    // Parser
    let mut parser = Parser::new(tokens, "<wasm>", source);
    let ast = parser.parse().map_err(|e| e.message)?;
    
    // Compiler
    let mut compiler = Compiler::new("<wasm>", source);
    let chunk = compiler.compile(&ast).map_err(|e| e.message)?;
    
    // VM
    let mut vm = VM::new();
    let result = vm.run(&chunk).map_err(|e| e.message)?;
    
    Ok(result.to_string())
}

/// Get Sald version
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
