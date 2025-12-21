// Sald CLI - Command Line Interface
// Usage: sald [FILE] [OPTIONS]

use clap::Parser;
use colored::*;
use std::fs;
use std::path::PathBuf;

use sald_core::binary;
use sald_core::compiler::Compiler;
use sald_core::error::SaldResult;
use sald_core::lexer::Scanner;
use sald_core::parser;
use sald_core::vm::VM;

/// Sald - A fast, class-based interpreted language
#[derive(Parser)]
#[command(name = "sald")]
#[command(author = "starssxhfdmh")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "A fast, class-based interpreted language", long_about = None)]
struct Cli {
    /// Source file to run (.sald or .saldc)
    file: Option<PathBuf>,

    /// Debug options: tokens, ast, asm, gc (comma-separated)
    #[arg(short = 'd', long = "debug", value_delimiter = ',')]
    debug: Option<Vec<String>>,

    /// Execute inline code
    #[arg(short = 'e', long = "exec")]
    exec: Option<String>,

    /// Compile to .saldc instead of running
    #[arg(short = 'c', long = "compile")]
    compile: bool,

    /// Check for errors without running
    #[arg(long = "check")]
    check: bool,

    /// Run tests (functions with @Test decorator)
    #[arg(short = 't', long = "test")]
    test: bool,

    /// Filter tests by name (requires --test)
    #[arg(short = 'f', long = "filter")]
    filter: Option<String>,

    /// Output path for compiled file (requires -c)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Parse debug flags
    let debug = DebugFlags::from_options(&cli.debug);

    let result = if let Some(code) = cli.exec {
        // Execute inline code
        handle_exec(&code, debug).await
    } else if let Some(path) = cli.file {
        if cli.check {
            // Check mode - only validate, don't run
            handle_check(&path)
        } else if cli.compile {
            // Compile mode
            handle_compile(&path, debug, cli.output)
        } else if cli.test {
            // Test mode - run @Test functions
            handle_test(&path, debug, cli.filter.as_deref()).await
        } else {
            // Run mode
            handle_run(&path, debug).await
        }
    } else {
        // REPL mode
        repl().await
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

#[derive(Default, Clone)]
struct DebugFlags {
    tokens: bool,
    ast: bool,
    asm: bool,
    gc: bool,
}

impl DebugFlags {
    fn from_options(opts: &Option<Vec<String>>) -> Self {
        let mut flags = Self::default();
        if let Some(opts) = opts {
            for opt in opts {
                match opt.as_str() {
                    "tokens" => flags.tokens = true,
                    "ast" => flags.ast = true,
                    "asm" => flags.asm = true,
                    "gc" => flags.gc = true,
                    _ => eprintln!("{} Unknown debug option: {}", "!".yellow(), opt),
                }
            }
        }
        flags
    }
}

/// Check file for errors without running
fn handle_check(path: &PathBuf) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Error reading file '{}': {}", path.display(), e))?;

    let file_name = path.to_string_lossy().to_string();

    // Tokenize
    let mut scanner = Scanner::new(&source, &file_name);
    let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;

    // Parse
    let mut parser = parser::Parser::new(tokens, &file_name, &source);
    let program = parser.parse().map_err(|e| e.to_string())?;

    // Compile (catches semantic errors)
    let mut compiler = Compiler::new(&file_name, &source);
    compiler.compile(&program).map_err(|e| e.to_string())?;

    println!("{} No errors found in {}", "✓".green(), path.display());
    Ok(())
}

fn handle_compile(
    path: &PathBuf,
    debug: DebugFlags,
    output: Option<PathBuf>,
) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Error reading file '{}': {}", path.display(), e))?;

    let file_name = path.to_string_lossy().to_string();

    // Tokenize
    let mut scanner = Scanner::new(&source, &file_name);
    let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;

    // Show tokens if requested
    if debug.tokens {
        println!("{}", "-- Tokens --".cyan());
        for token in &tokens {
            println!("  {:?} '{}'", token.kind, token.lexeme);
        }
        println!();
        return Ok(());
    }

    // Parse
    let mut parser = parser::Parser::new(tokens, &file_name, &source);
    let program = parser.parse().map_err(|e| e.to_string())?;

    // Show AST if requested
    if debug.ast {
        use ptree::TreeBuilder;

        let mut tree = TreeBuilder::new("Program".to_string());
        for stmt in &program.statements {
            build_stmt_tree(&mut tree, stmt);
        }
        let tree = tree.build();
        ptree::print_tree(&tree).map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Compile
    let mut compiler = Compiler::new(&file_name, &source);
    let chunk = compiler.compile(&program).map_err(|e| e.to_string())?;

    // Show disassembly if requested
    if debug.asm {
        chunk.disassemble(&file_name);
        return Ok(());
    }

    // Determine output path
    let output_path = output.unwrap_or_else(|| path.with_extension("saldc"));

    // Serialize and write
    let bytes = binary::serialize(&chunk);
    fs::write(&output_path, bytes).map_err(|e| format!("Error writing file: {}", e))?;
    println!("{} Compiled to {}", "✓".green(), output_path.display());

    Ok(())
}

/// Find project root by looking for salad.json in current or parent directories
fn find_project_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join("salad.json").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

async fn handle_run(path: &PathBuf, debug: DebugFlags) -> Result<(), String> {
    // Auto-detect project root if salad.json exists (enables module imports)
    if let Some(project_root) = find_project_root() {
        sald_core::set_project_root(&project_root);
    }
    
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    let (chunk, source) = match ext {
        "saldc" => {
            // Read compiled bytecode
            let data = fs::read(path)
                .map_err(|e| format!("Error reading file '{}': {}", path.display(), e))?;
            let chunk = binary::deserialize(&data)?;
            (chunk, String::new())
        }
        _ => {
            // Read and compile source
            let source = fs::read_to_string(path)
                .map_err(|e| format!("Error reading file '{}': {}", path.display(), e))?;

            let file_name = path.to_string_lossy().to_string();
            let mut scanner = Scanner::new(&source, &file_name);
            let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;

            // Show tokens if requested
            if debug.tokens {
                println!("{}", "-- Tokens --".cyan());
                for token in &tokens {
                    println!("  {:?} '{}'", token.kind, token.lexeme);
                }
                println!();
                return Ok(());
            }

            let mut parser = parser::Parser::new(tokens, &file_name, &source);
            let program = parser.parse().map_err(|e| e.to_string())?;

            // Show AST if requested
            if debug.ast {
                use ptree::TreeBuilder;

                let mut tree = TreeBuilder::new("Program".to_string());
                for stmt in &program.statements {
                    build_stmt_tree(&mut tree, stmt);
                }
                let tree = tree.build();
                ptree::print_tree(&tree).map_err(|e| e.to_string())?;
                return Ok(());
            }

            let mut compiler = Compiler::new(&file_name, &source);
            let chunk = compiler.compile(&program).map_err(|e| e.to_string())?;
            (chunk, source)
        }
    };

    let file_name = path.to_string_lossy().to_string();

    // Show disassembly if requested
    if debug.asm {
        chunk.disassemble(&file_name);
    }

    // Run with async VM
    let mut vm = VM::new();
    vm.set_gc_stats_enabled(debug.gc);
    vm.run(chunk, &file_name, &source).await
        .map_err(|e| e.format_with_options(true))?;

    Ok(())
}

/// Run tests - collect and execute @Test functions
async fn handle_test(path: &PathBuf, debug: DebugFlags, filter: Option<&str>) -> Result<(), String> {
    use std::time::Instant;
    
    // Auto-detect project root
    if let Some(project_root) = find_project_root() {
        sald_core::set_project_root(&project_root);
    }
    
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Error reading file '{}': {}", path.display(), e))?;
    
    let file_name = path.to_string_lossy().to_string();
    let mut scanner = Scanner::new(&source, &file_name);
    let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;
    
    let mut parser = parser::Parser::new(tokens, &file_name, &source);
    let program = parser.parse().map_err(|e| e.to_string())?;
    
    // Collect @Test functions 
    let mut test_names: Vec<String> = Vec::new();
    for stmt in &program.statements {
        if let sald_core::ast::Stmt::Function { def } = stmt {
            if def.decorators.iter().any(|d| d.name == "Test") {
                // Apply filter if provided
                if let Some(f) = filter {
                    if !def.name.contains(f) {
                        continue;
                    }
                }
                test_names.push(def.name.clone());
            }
        }
    }
    
    if test_names.is_empty() {
        if filter.is_some() {
            println!("\n{}", "running 0 tests (filtered)".yellow());
        } else {
            println!("\n{}", "running 0 tests".yellow());
        }
        println!("\n{}", "test result: ok. 0 passed; 0 failed".green());
        return Ok(());
    }
    
    // Compile the full program
    let mut compiler = Compiler::new(&file_name, &source);
    let chunk = compiler.compile(&program).map_err(|e| e.to_string())?;
    
    if debug.asm {
        chunk.disassemble(&file_name);
    }
    
    // Run program first to define all functions
    let mut vm = VM::new();
    vm.run(chunk, &file_name, &source).await
        .map_err(|e| e.format_with_options(true))?;
    
    // Print test header
    println!();
    println!("running {} test{}", test_names.len(), if test_names.len() == 1 { "" } else { "s" });
    
    let mut passed = 0;
    let mut failed = 0;
    let mut failed_tests: Vec<(String, String)> = Vec::new();
    let start = Instant::now();
    
    // Run each test function
    for name in &test_names {
        let test_start = Instant::now();
        
        // Call the test function using vm.call_global
        let result = vm.call_global(name, vec![]).await;
        
        let duration = test_start.elapsed();
        let duration_str = if duration.as_millis() > 0 {
            format!(" ({:.2}ms)", duration.as_secs_f64() * 1000.0)
        } else {
            String::new()
        };
        
        match result {
            Ok(_) => {
                println!("test {} ... {}{}", name, "ok".green(), duration_str);
                passed += 1;
            }
            Err(e) => {
                println!("test {} ... {}{}", name, "FAILED".red(), duration_str);
                failed_tests.push((name.clone(), e.format_with_options(false)));
                failed += 1;
            }
        }
    }
    
    let total_duration = start.elapsed();
    
    // Print failures detail
    if !failed_tests.is_empty() {
        println!();
        println!("failures:");
        println!();
        for (name, error) in &failed_tests {
            println!("---- {} ----", name);
            // Print just the error message, not the full stack trace
            let first_line = error.lines().next().unwrap_or(error);
            println!("{}", first_line.red());
            println!();
        }
        println!("failures:");
        for (name, _) in &failed_tests {
            println!("    {}", name);
        }
    }
    
    // Print summary
    println!();
    if failed > 0 {
        println!(
            "test result: {}. {} passed; {} failed; finished in {:.2}s",
            "FAILED".red().bold(),
            passed,
            failed,
            total_duration.as_secs_f64()
        );
        return Err(format!("{} test(s) failed", failed));
    } else {
        println!(
            "test result: {}. {} passed; {} failed; finished in {:.2}s",
            "ok".green().bold(),
            passed,
            failed,
            total_duration.as_secs_f64()
        );
    }
    
    Ok(())
}

/// Execute inline code
async fn handle_exec(code: &str, debug: DebugFlags) -> Result<(), String> {
    let mut scanner = Scanner::new(code, "<exec>");
    let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;

    if debug.tokens {
        println!("{}", "-- Tokens --".cyan());
        for token in &tokens {
            println!("  {:?} '{}'", token.kind, token.lexeme);
        }
        println!();
        return Ok(());
    }

    let mut parser = parser::Parser::new(tokens, "<exec>", code);
    let program = parser.parse().map_err(|e| e.to_string())?;

    if debug.ast {
        use ptree::TreeBuilder;

        let mut tree = TreeBuilder::new("Program".to_string());
        for stmt in &program.statements {
            build_stmt_tree(&mut tree, stmt);
        }
        let tree = tree.build();
        ptree::print_tree(&tree).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let mut compiler = Compiler::new("<exec>", code);
    let chunk = compiler.compile(&program).map_err(|e| e.to_string())?;

    if debug.asm {
        chunk.disassemble("<exec>");
    }

    let mut vm = VM::new();
    vm.set_gc_stats_enabled(debug.gc);
    vm.run(chunk, "<exec>", code).await
        .map_err(|e| e.format_with_options(false))?;

    Ok(())
}

async fn repl() -> Result<(), String> {
    use reedline::{Reedline, Signal, FileBackedHistory, Prompt, PromptHistorySearch, PromptHistorySearchStatus};
    use std::borrow::Cow;
    use std::io::Write;

    // Check if code is incomplete (unbalanced delimiters)
    fn is_incomplete(code: &str) -> bool {
        let mut brace_count = 0i32;
        let mut paren_count = 0i32;
        let mut bracket_count = 0i32;
        let mut in_string = false;
        let mut in_raw_string = false;
        let mut chars = code.chars().peekable();
        
        while let Some(c) = chars.next() {
            // Handle string literals
            if c == '"' && !in_raw_string {
                // Check for raw string """
                if chars.peek() == Some(&'"') {
                    chars.next();
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        in_raw_string = !in_raw_string;
                        continue;
                    }
                }
                if !in_raw_string {
                    in_string = !in_string;
                }
                continue;
            }
            
            if in_string || in_raw_string {
                continue;
            }
            
            match c {
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                '[' => bracket_count += 1,
                ']' => bracket_count -= 1,
                _ => {}
            }
        }
        
        brace_count > 0 || paren_count > 0 || bracket_count > 0 || in_string || in_raw_string
    }

    // Custom prompts
    struct MainPrompt;
    struct ContinuePrompt;
    
    impl Prompt for MainPrompt {
        fn render_prompt_left(&self) -> Cow<'_, str> {
            Cow::Borrowed(">>> ")
        }
        fn render_prompt_right(&self) -> Cow<'_, str> {
            Cow::Borrowed("")
        }
        fn render_prompt_indicator(&self, _: reedline::PromptEditMode) -> Cow<'_, str> {
            Cow::Borrowed("")
        }
        fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
            Cow::Borrowed("... ")
        }
        fn render_prompt_history_search_indicator(&self, history_search: PromptHistorySearch) -> Cow<'_, str> {
            let prefix = match history_search.status {
                PromptHistorySearchStatus::Passing => "",
                PromptHistorySearchStatus::Failing => "failing ",
            };
            Cow::Owned(format!("({}reverse-search: {}) ", prefix, history_search.term))
        }
    }

    impl Prompt for ContinuePrompt {
        fn render_prompt_left(&self) -> Cow<'_, str> {
            Cow::Borrowed("... ")
        }
        fn render_prompt_right(&self) -> Cow<'_, str> {
            Cow::Borrowed("")
        }
        fn render_prompt_indicator(&self, _: reedline::PromptEditMode) -> Cow<'_, str> {
            Cow::Borrowed("")
        }
        fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
            Cow::Borrowed("... ")
        }
        fn render_prompt_history_search_indicator(&self, history_search: PromptHistorySearch) -> Cow<'_, str> {
            let prefix = match history_search.status {
                PromptHistorySearchStatus::Passing => "",
                PromptHistorySearchStatus::Failing => "failing ",
            };
            Cow::Owned(format!("({}reverse-search: {}) ", prefix, history_search.term))
        }
    }

    println!();
    println!("  {}  {}", "Sald".cyan().bold(), format!("v{}", env!("CARGO_PKG_VERSION")).bright_black());
    println!("  {}", "Type .help for commands, .exit to quit".bright_black());
    println!();

    // Setup history file
    let history_path = dirs_home().join(".sald_history");
    let history = Box::new(
        FileBackedHistory::with_file(1000, history_path.clone())
            .map_err(|e| e.to_string())?
    );

    // Create reedline editor with history
    let mut line_editor = Reedline::create().with_history(history);
    let main_prompt = MainPrompt;
    let continue_prompt = ContinuePrompt;

    // Create persistent VM to maintain state across lines
    let mut vm = VM::new();
    let mut line_count = 0u32;
    let mut accumulated_input = String::new();

    loop {
        let prompt: &dyn Prompt = if accumulated_input.is_empty() {
            &main_prompt
        } else {
            &continue_prompt
        };

        match line_editor.read_line(prompt) {
            Ok(Signal::Success(line)) => {
                // Handle empty line in multiline mode - execute what we have
                if line.trim().is_empty() && !accumulated_input.is_empty() {
                    let input = accumulated_input.trim().to_string();
                    accumulated_input.clear();
                    
                    line_count += 1;
                    match run_repl_line(&mut vm, &input).await {
                        Ok(value) => {
                            print_repl_result(&value, line_count);
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                        }
                    }
                    continue;
                }

                let input = line.trim();
                
                if input.is_empty() {
                    continue;
                }

                // Handle REPL commands (only on first line)
                if accumulated_input.is_empty() && input.starts_with('.') {
                    match input {
                        ".exit" | ".quit" => break,
                        ".help" => {
                            print_repl_help();
                            continue;
                        }
                        ".clear" => {
                            print!("\x1B[2J\x1B[1;1H");
                            let _ = std::io::stdout().flush();
                            continue;
                        }
                        ".reset" => {
                            vm = VM::new();
                            println!("{}", "  VM state reset".bright_black());
                            continue;
                        }
                        _ => {
                            println!("{} Unknown command: {}", "!".red(), input);
                            println!("  Type {} for available commands", ".help".cyan());
                            continue;
                        }
                    }
                }

                // Accumulate input
                if !accumulated_input.is_empty() {
                    accumulated_input.push('\n');
                }
                accumulated_input.push_str(&line);

                // Check if code is complete
                if is_incomplete(&accumulated_input) {
                    // Need more lines
                    continue;
                }

                // Execute complete code
                let full_input = accumulated_input.trim().to_string();
                accumulated_input.clear();

                line_count += 1;

                match run_repl_line(&mut vm, &full_input).await {
                    Ok(value) => {
                        print_repl_result(&value, line_count);
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                    }
                }
            }
            Ok(Signal::CtrlC) => {
                // Cancel multiline input
                if !accumulated_input.is_empty() {
                    accumulated_input.clear();
                    println!("{}", "^C (input cleared)".bright_black());
                } else {
                    println!("{}", "^C".bright_black());
                }
                continue;
            }
            Ok(Signal::CtrlD) => {
                println!("{}", "^D".bright_black());
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    println!("\n{}", "Goodbye! 👋".bright_black());
    Ok(())
}

/// Get home directory for history file
fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Print REPL help
fn print_repl_help() {
    println!();
    println!("  {}", "REPL Commands:".cyan().bold());
    println!("    {}    Exit the REPL", ".exit".yellow());
    println!("    {}    Clear the screen", ".clear".yellow());
    println!("    {}    Reset VM state", ".reset".yellow());
    println!("    {}    Show this help", ".help".yellow());
    println!();
    println!("  {}", "Keyboard Shortcuts:".cyan().bold());
    println!("    {}      Previous command", "↑".yellow());
    println!("    {}      Next command", "↓".yellow());
    println!("    {}    Search history", "Ctrl+R".yellow());
    println!("    {}    Clear line", "Ctrl+U".yellow());
    println!("    {}    Exit", "Ctrl+D".yellow());
    println!();
}

/// Format and print REPL result with colors (Node.js style)
fn print_repl_result(value: &sald_core::vm::Value, _line: u32) {
    let formatted = format_value(value, 0);
    if !formatted.is_empty() && formatted != "null" {
        println!("{}", formatted);
    }
}

/// Format a value with syntax highlighting
fn format_value(value: &sald_core::vm::Value, depth: usize) -> String {
    use sald_core::vm::Value;
    
    match value {
        Value::Null => "null".bright_black().to_string(),
        Value::Boolean(b) => {
            if *b {
                "true".yellow().to_string()
            } else {
                "false".yellow().to_string()
            }
        }
        Value::Number(n) => {
            if n.fract() == 0.0 {
                format!("{}", (*n as i64)).yellow().to_string()
            } else {
                format!("{}", n).yellow().to_string()
            }
        }
        Value::String(s) => format!("'{}'", s).green().to_string(),
        Value::Array(arr) => {
            if let Ok(arr) = arr.lock() {
                if arr.is_empty() {
                    "[]".to_string()
                } else if arr.len() <= 5 && depth < 2 {
                    let items: Vec<String> = arr.iter()
                        .map(|v| format_value(v, depth + 1))
                        .collect();
                    format!("[ {} ]", items.join(", "))
                } else {
                    format!("[Array({})]", arr.len()).bright_black().to_string()
                }
            } else {
                "[<locked>]".bright_black().to_string()
            }
        }
        Value::Dictionary(dict) => {
            if let Ok(dict) = dict.lock() {
                if dict.is_empty() {
                    "{}".to_string()
                } else if dict.len() <= 3 && depth < 2 {
                    let items: Vec<String> = dict.iter()
                        .map(|(k, v)| format!("{}: {}", k.cyan(), format_value(v, depth + 1)))
                        .collect();
                    format!("{{ {} }}", items.join(", "))
                } else {
                    format!("{{Object({} keys)}}", dict.len()).bright_black().to_string()
                }
            } else {
                "{<locked>}".bright_black().to_string()
            }
        }
        Value::Function(f) => format!("[Function: {}]", f.name).cyan().to_string(),
        Value::NativeFunction { class_name, .. } => {
            format!("[NativeFunction: {}]", class_name).cyan().to_string()
        }
        Value::InstanceMethod { method_name, .. } => {
            format!("[Method: {}]", method_name).cyan().to_string()
        }
        Value::BoundMethod { method, .. } => {
            format!("[BoundMethod: {}]", method.name).cyan().to_string()
        }
        Value::Class(c) => format!("[Class: {}]", c.name).magenta().to_string(),
        Value::Instance(inst) => {
            if let Ok(inst) = inst.lock() {
                if inst.fields.is_empty() {
                    format!("{} {{}}", inst.class_name.magenta())
                } else if inst.fields.len() <= 3 && depth < 1 {
                    let fields: Vec<String> = inst.fields.iter()
                        .map(|(k, v)| format!("{}: {}", k, format_value(v, depth + 1)))
                        .collect();
                    format!("{} {{ {} }}", inst.class_name.magenta(), fields.join(", "))
                } else {
                    format!("{} {{...}}", inst.class_name.magenta())
                }
            } else {
                "<instance locked>".bright_black().to_string()
            }
        }
        Value::Future(_) => "[Future]".bright_black().to_string(),
        Value::Namespace { name, .. } => format!("[Namespace: {}]", name).magenta().to_string(),
        Value::Enum { name, .. } => format!("[Enum: {}]", name).magenta().to_string(),
        Value::SpreadMarker(v) => format!("[Spread: {:?}]", v).bright_black().to_string(),
    }
}

async fn run_repl_line(vm: &mut VM, source: &str) -> SaldResult<sald_core::vm::Value> {
    let mut scanner = Scanner::new(source, "<repl>");
    let tokens = scanner.scan_tokens()?;

    let mut parser = parser::Parser::new(tokens, "<repl>", source);
    let program = parser.parse()?;

    let mut compiler = Compiler::new("<repl>", source);
    // Use compile_repl to keep expression results on stack
    let chunk = compiler.compile_repl(&program)?;

    let result = vm.run(chunk, "<repl>", source).await?;

    Ok(result)
}

// Build statement tree using ptree TreeBuilder
fn build_stmt_tree(tree: &mut ptree::TreeBuilder, stmt: &sald_core::ast::Stmt) {
    use sald_core::ast::Stmt;

    match stmt {
        Stmt::Let {
            name, initializer, ..
        } => {
            tree.begin_child(format!("Let '{}'", name));
            if let Some(init) = initializer {
                build_expr_tree(tree, init);
            }
            tree.end_child();
        }
        Stmt::LetDestructure { pattern, initializer, .. } => {
            let vars: Vec<String> = pattern.elements.iter().map(|e| {
                match e {
                    sald_core::ast::ArrayPatternElement::Variable { name, .. } => name.clone(),
                    sald_core::ast::ArrayPatternElement::Rest { name, .. } => format!("...{}", name),
                    sald_core::ast::ArrayPatternElement::Hole => "_".to_string(),
                }
            }).collect();
            tree.begin_child(format!("LetDestructure [{}]", vars.join(", ")));
            build_expr_tree(tree, initializer);
            tree.end_child();
        }
        Stmt::Expression { expr, .. } => {
            tree.begin_child("Expr".to_string());
            build_expr_tree(tree, expr);
            tree.end_child();
        }
        Stmt::Block { statements, .. } => {
            tree.begin_child("Block".to_string());
            for s in statements {
                build_stmt_tree(tree, s);
            }
            tree.end_child();
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            tree.begin_child("If".to_string());
            tree.begin_child("condition".to_string());
            build_expr_tree(tree, condition);
            tree.end_child();
            tree.begin_child("then".to_string());
            build_stmt_tree(tree, then_branch);
            tree.end_child();
            if let Some(else_b) = else_branch {
                tree.begin_child("else".to_string());
                build_stmt_tree(tree, else_b);
                tree.end_child();
            }
            tree.end_child();
        }
        Stmt::While {
            condition, body, ..
        } => {
            tree.begin_child("While".to_string());
            tree.begin_child("condition".to_string());
            build_expr_tree(tree, condition);
            tree.end_child();
            tree.begin_child("body".to_string());
            build_stmt_tree(tree, body);
            tree.end_child();
            tree.end_child();
        }
        Stmt::DoWhile {
            body, condition, ..
        } => {
            tree.begin_child("DoWhile".to_string());
            tree.begin_child("body".to_string());
            build_stmt_tree(tree, body);
            tree.end_child();
            tree.begin_child("condition".to_string());
            build_expr_tree(tree, condition);
            tree.end_child();
            tree.end_child();
        }
        Stmt::For {
            variable,
            iterable,
            body,
            ..
        } => {
            tree.begin_child(format!("For '{}' in", variable));
            tree.begin_child("iterable".to_string());
            build_expr_tree(tree, iterable);
            tree.end_child();
            tree.begin_child("body".to_string());
            build_stmt_tree(tree, body);
            tree.end_child();
            tree.end_child();
        }
        Stmt::Function { def } => {
            let params: Vec<_> = def.params.iter().map(|p| p.name.as_str()).collect();
            tree.begin_child(format!("Function '{}' ({})", def.name, params.join(", ")));
            for s in &def.body {
                build_stmt_tree(tree, s);
            }
            tree.end_child();
        }
        Stmt::Class { def } => {
            tree.begin_child(format!("Class '{}'", def.name));
            for method in &def.methods {
                let params: Vec<_> = method.params.iter().map(|p| p.name.as_str()).collect();
                let static_str = if method.is_static { "static " } else { "" };
                tree.add_empty_child(format!(
                    "{}method '{}' ({})",
                    static_str,
                    method.name,
                    params.join(", ")
                ));
            }
            tree.end_child();
        }
        Stmt::Return { value, .. } => {
            tree.begin_child("Return".to_string());
            if let Some(val) = value {
                build_expr_tree(tree, val);
            }
            tree.end_child();
        }
        Stmt::Break { .. } => {
            tree.add_empty_child("Break".to_string());
        }
        Stmt::Continue { .. } => {
            tree.add_empty_child("Continue".to_string());
        }
        Stmt::Import { path, alias, .. } => {
            if let Some(a) = alias {
                tree.add_empty_child(format!("Import '{}' as {}", path, a));
            } else {
                tree.add_empty_child(format!("Import '{}'", path));
            }
        }
        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
            ..
        } => {
            tree.begin_child("TryCatch".to_string());
            tree.begin_child("try".to_string());
            build_stmt_tree(tree, try_body);
            tree.end_child();
            tree.begin_child(format!("catch ({})", catch_var));
            build_stmt_tree(tree, catch_body);
            tree.end_child();
            tree.end_child();
        }
        Stmt::Throw { value, .. } => {
            tree.begin_child("Throw".to_string());
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Stmt::Namespace { name, body, .. } => {
            tree.begin_child(format!("Namespace '{}'", name));
            for s in body {
                build_stmt_tree(tree, s);
            }
            tree.end_child();
        }
        Stmt::Const { name, value, .. } => {
            tree.begin_child(format!("Const '{}'", name));
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Stmt::Enum { name, variants, .. } => {
            tree.begin_child(format!("Enum '{}'", name));
            for v in variants {
                tree.add_empty_child(v.clone());
            }
            tree.end_child();
        }
        Stmt::Interface { def } => {
            tree.begin_child(format!("Interface '{}'", def.name));
            for method in &def.methods {
                let params: Vec<_> = method.params.iter().map(|p| p.name.as_str()).collect();
                tree.add_empty_child(format!("fun {}({})", method.name, params.join(", ")));
            }
            tree.end_child();
        }
    }
}

fn build_expr_tree(tree: &mut ptree::TreeBuilder, expr: &sald_core::ast::Expr) {
    use sald_core::ast::{BinaryOp, Expr, Literal, UnaryOp};

    match expr {
        Expr::Literal { value, .. } => {
            let val_str = match value {
                Literal::Number(n) => format!("{}", n),
                Literal::String(s) => format!("\"{}\"", s),
                Literal::Boolean(b) => format!("{}", b),
                Literal::Null => "null".to_string(),
            };
            tree.add_empty_child(val_str);
        }
        Expr::Identifier { name, .. } => {
            tree.add_empty_child(name.clone());
        }
        Expr::Binary {
            left, op, right, ..
        } => {
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Equal => "==",
                BinaryOp::NotEqual => "!=",
                BinaryOp::Less => "<",
                BinaryOp::LessEqual => "<=",
                BinaryOp::Greater => ">",
                BinaryOp::GreaterEqual => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                BinaryOp::NullCoalesce => "??",
                BinaryOp::BitAnd => "&",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
                BinaryOp::LeftShift => "<<",
                BinaryOp::RightShift => ">>",
            };
            tree.begin_child(format!("Binary({})", op_str));
            build_expr_tree(tree, left);
            build_expr_tree(tree, right);
            tree.end_child();
        }
        Expr::Unary { op, operand, .. } => {
            let op_str = match op {
                UnaryOp::Negate => "-",
                UnaryOp::Not => "!",
                UnaryOp::BitNot => "~",
            };
            tree.begin_child(format!("Unary({})", op_str));
            build_expr_tree(tree, operand);
            tree.end_child();
        }
        Expr::Grouping { expr: inner, .. } => {
            tree.begin_child("Group".to_string());
            build_expr_tree(tree, inner);
            tree.end_child();
        }
        Expr::Assignment { target, value, .. } => {
            tree.begin_child("Assign".to_string());
            build_expr_tree(tree, target);
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Expr::Call { callee, args, .. } => {
            tree.begin_child("Call".to_string());
            tree.begin_child("callee".to_string());
            build_expr_tree(tree, callee);
            tree.end_child();
            if !args.is_empty() {
                tree.begin_child("args".to_string());
                for arg in args {
                    build_expr_tree(tree, &arg.value);
                }
                tree.end_child();
            }
            tree.end_child();
        }
        Expr::Get {
            object, property, ..
        } => {
            tree.begin_child(format!("Get '{}'", property));
            build_expr_tree(tree, object);
            tree.end_child();
        }
        Expr::Set {
            object,
            property,
            value,
            ..
        } => {
            tree.begin_child(format!("Set '{}'", property));
            build_expr_tree(tree, object);
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Expr::SelfExpr { .. } => {
            tree.add_empty_child("self".to_string());
        }
        Expr::Array { elements, .. } => {
            tree.begin_child("Array".to_string());
            for elem in elements {
                build_expr_tree(tree, elem);
            }
            tree.end_child();
        }
        Expr::Index { object, index, .. } => {
            tree.begin_child("Index".to_string());
            build_expr_tree(tree, object);
            build_expr_tree(tree, index);
            tree.end_child();
        }
        Expr::IndexSet {
            object,
            index,
            value,
            ..
        } => {
            tree.begin_child("IndexSet".to_string());
            build_expr_tree(tree, object);
            build_expr_tree(tree, index);
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            tree.begin_child("Ternary".to_string());
            tree.begin_child("condition".to_string());
            build_expr_tree(tree, condition);
            tree.end_child();
            tree.begin_child("then".to_string());
            build_expr_tree(tree, then_expr);
            tree.end_child();
            tree.begin_child("else".to_string());
            build_expr_tree(tree, else_expr);
            tree.end_child();
            tree.end_child();
        }
        Expr::Lambda { params, .. } => {
            let param_names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
            tree.add_empty_child(format!("Lambda ({})", param_names.join(", ")));
        }
        Expr::Super { method, .. } => {
            tree.add_empty_child(format!("super.{}", method));
        }
        Expr::Switch {
            value,
            arms,
            default,
            ..
        } => {
            tree.begin_child("Switch".to_string());
            tree.begin_child("value".to_string());
            build_expr_tree(tree, value);
            tree.end_child();
            for (i, arm) in arms.iter().enumerate() {
                tree.begin_child(format!("arm[{}]", i));
                tree.begin_child("patterns".to_string());
                for p in &arm.patterns {
                    build_pattern_tree(tree, p);
                }
                tree.end_child();
                tree.begin_child("body".to_string());
                build_expr_tree(tree, &arm.body);
                tree.end_child();
                tree.end_child();
            }
            if let Some(d) = default {
                tree.begin_child("default".to_string());
                build_expr_tree(tree, d);
                tree.end_child();
            }
            tree.end_child();
        }
        Expr::Block {
            statements, expr, ..
        } => {
            tree.begin_child("Block".to_string());
            for (i, stmt) in statements.iter().enumerate() {
                tree.begin_child(format!("stmt[{}]", i));
                build_stmt_tree(tree, stmt);
                tree.end_child();
            }
            if let Some(e) = expr {
                tree.begin_child("result".to_string());
                build_expr_tree(tree, e);
                tree.end_child();
            }
            tree.end_child();
        }
        Expr::Dictionary { entries, .. } => {
            tree.begin_child("Dictionary".to_string());
            for (i, (key, value)) in entries.iter().enumerate() {
                tree.begin_child(format!("entry[{}]", i));
                tree.begin_child("key".to_string());
                build_expr_tree(tree, key);
                tree.end_child();
                tree.begin_child("value".to_string());
                build_expr_tree(tree, value);
                tree.end_child();
                tree.end_child();
            }
            tree.end_child();
        }
        Expr::Await { expr, .. } => {
            tree.begin_child("Await".to_string());
            build_expr_tree(tree, expr);
            tree.end_child();
        }
        Expr::Return { value, .. } => {
            tree.begin_child("Return".to_string());
            if let Some(v) = value {
                build_expr_tree(tree, v);
            }
            tree.end_child();
        }
        Expr::Throw { value, .. } => {
            tree.begin_child("Throw".to_string());
            build_expr_tree(tree, value);
            tree.end_child();
        }
        Expr::Break { .. } => {
            tree.add_empty_child("Break".to_string());
        }
        Expr::Continue { .. } => {
            tree.add_empty_child("Continue".to_string());
        }
        Expr::Spread { expr, .. } => {
            tree.begin_child("Spread".to_string());
            build_expr_tree(tree, expr);
            tree.end_child();
        }
        Expr::Range { start, end, inclusive, .. } => {
            let op = if *inclusive { ".." } else { "..<" };
            tree.begin_child(format!("Range ({})", op));
            build_expr_tree(tree, start);
            build_expr_tree(tree, end);
            tree.end_child();
        }
    }
}

fn build_pattern_tree(tree: &mut ptree::TreeBuilder, pattern: &sald_core::ast::Pattern) {
    use sald_core::ast::{Pattern, Literal, SwitchArrayElement};
    
    match pattern {
        Pattern::Literal { value, .. } => {
            let val_str = match value {
                Literal::Number(n) => format!("{}", n),
                Literal::String(s) => format!("\"{}\"", s),
                Literal::Boolean(b) => format!("{}", b),
                Literal::Null => "null".to_string(),
            };
            tree.add_empty_child(format!("Literal({})", val_str));
        }
        Pattern::Binding { name, guard, .. } => {
            if guard.is_some() {
                tree.begin_child(format!("Binding '{}' if ...", name));
                if let Some(g) = guard {
                    build_expr_tree(tree, g);
                }
                tree.end_child();
            } else {
                tree.add_empty_child(format!("Binding '{}'", name));
            }
        }
        Pattern::Array { elements, .. } => {
            tree.begin_child("ArrayPattern".to_string());
            for elem in elements {
                match elem {
                    SwitchArrayElement::Single(sub) => {
                        build_pattern_tree(tree, sub);
                    }
                    SwitchArrayElement::Rest { name, .. } => {
                        tree.add_empty_child(format!("...{}", name));
                    }
                }
            }
            tree.end_child();
        }
        Pattern::Dict { entries, .. } => {
            tree.begin_child("DictPattern".to_string());
            for (key, sub) in entries {
                tree.begin_child(format!("'{}':", key));
                build_pattern_tree(tree, sub);
                tree.end_child();
            }
            tree.end_child();
        }
        Pattern::Range { start, end, inclusive, .. } => {
            let op = if *inclusive { ".." } else { "..<" };
            tree.begin_child(format!("RangePattern ({})", op));
            build_expr_tree(tree, start);
            build_expr_tree(tree, end);
            tree.end_child();
        }
        Pattern::Expression { .. } => {
            tree.add_empty_child("ExpressionPattern".to_string());
        }
    }
}
