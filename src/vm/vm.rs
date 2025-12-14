// Sald Virtual Machine
// Stack-based VM for executing bytecode
// Uses Arc/Mutex for thread-safe async support
// Implements suspend/resume for true non-blocking async

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::builtins;
use crate::compiler::chunk::{Chunk, Constant};
use crate::compiler::Compiler;
use crate::compiler::OpCode;
use crate::error::{ErrorKind, SaldError, SaldResult, Span, StackFrame};
use crate::lexer::Scanner;
use crate::parser::Parser;
use crate::vm::caller::ValueCaller;
use crate::vm::gc::GcHeap;
use crate::vm::value::{Class, Function, Instance, UpvalueObj, Value};
use tokio::sync::oneshot;

const STACK_MAX: usize = 65536;
const FRAMES_MAX: usize = 4096;

/// Result of VM execution - supports suspend/resume for async
pub enum ExecutionResult {
    /// VM completed execution with a value
    Completed(Value),
    
    /// VM suspended on await - needs to resume after future resolves
    Suspended {
        receiver: oneshot::Receiver<Result<Value, String>>,
    },
}

/// Call frame for function execution
#[derive(Clone)]
struct CallFrame {
    function: Arc<Function>,
    ip: usize,
    slots_start: usize,
    /// For init methods, stores the instance to return
    init_instance: Option<Value>,
}

impl CallFrame {
    fn new(function: Arc<Function>, slots_start: usize) -> Self {
        Self {
            function,
            ip: 0,
            slots_start,
            init_instance: None,
        }
    }

    fn new_init(function: Arc<Function>, slots_start: usize, instance: Value) -> Self {
        Self {
            function,
            ip: 0,
            slots_start,
            init_instance: Some(instance),
        }
    }

    fn read_byte(&mut self) -> u8 {
        let byte = self.function.chunk.code[self.ip];
        self.ip += 1;
        byte
    }

    fn read_u16(&mut self) -> u16 {
        let high = self.function.chunk.code[self.ip] as u16;
        let low = self.function.chunk.code[self.ip + 1] as u16;
        self.ip += 2;
        (high << 8) | low
    }

    fn current_span(&self) -> Span {
        self.function.chunk.get_span(self.ip.saturating_sub(1))
    }
}

/// Exception handler for try-catch
#[derive(Clone)]
struct ExceptionHandler {
    /// Frame index when handler was pushed
    frame_index: usize,
    /// Stack size when handler was pushed
    stack_size: usize,
    /// IP to jump to for catch block
    catch_ip: usize,
}

/// The Sald Virtual Machine
/// Uses suspend/resume model for true non-blocking async
pub struct VM {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    /// Shared globals using Arc<RwLock> for thread-safe access across VMs
    globals: Arc<RwLock<HashMap<String, Value>>>,
    file: String,
    source: String,
    /// Stack of exception handlers (for try-catch)
    exception_handlers: Vec<ExceptionHandler>,
    /// Open upvalues (pointing to stack slots)
    open_upvalues: Vec<Arc<Mutex<UpvalueObj>>>,
    /// Garbage collector heap for cycle detection
    gc: GcHeap,
    /// Instruction counter for periodic GC checks
    gc_counter: usize,
    /// Whether to print GC statistics when collection runs
    gc_stats_enabled: bool,
    /// Pending module workspace to push during import (set by resolve_module_import)
    pending_module_workspace: Option<std::path::PathBuf>,
    /// Command-line arguments passed to the script
    args: Vec<String>,
}

// Implement ValueCaller trait to allow native functions to call closures
impl ValueCaller for VM {
    fn call(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, String> {
        // Save current frame count to know when the call returns
        let frame_count_before = self.frames.len();

        // Push callee onto stack
        self.push(callee.clone()).map_err(|e| e.message)?;

        // Push all arguments
        for arg in args.iter() {
            self.push(arg.clone()).map_err(|e| e.message)?;
        }

        // Call the value (this sets up a new call frame)
        self.call_value(args.len()).map_err(|e| e.message)?;

        // Execute until the call returns (frame count goes back to before)
        // NOTE: This is synchronous - if the callee uses await, we'll get Suspended
        // which we handle by running a mini event loop here
        loop {
            // Check if we've returned from the call
            if self.frames.len() == frame_count_before {
                // The result should be on top of the stack
                if let Some(result) = self.stack.pop() {
                    return Ok(result);
                } else {
                    return Ok(Value::Null);
                }
            }

            // Execute one instruction
            match self.execute_one_suspendable() {
                Ok(None) => continue,
                Ok(Some(ExecutionResult::Completed(v))) => {
                    // Unexpected completion - return the value
                    return Ok(v);
                }
                Ok(Some(ExecutionResult::Suspended { receiver })) => {
                    // Handler uses await - we need to handle this properly in async context
                    // Use tokio::task::block_in_place to safely block within Tokio runtime
                    // This moves the blocking work to a separate thread, preventing deadlocks
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(receiver)
                    });
                    
                    match result {
                        Ok(Ok(value)) => {
                            self.push(value).map_err(|e| e.message)?;
                        }
                        Ok(Err(e)) => {
                            return Err(e);
                        }
                        Err(_) => {
                            return Err("Future was cancelled".to_string());
                        }
                    }
                }
                Err(e) => return Err(e.message),
            }
        }
    }

    fn get_globals(&self) -> HashMap<String, Value> {
        self.globals.read().unwrap().clone()
    }
    
    fn get_shared_globals(&self) -> Arc<RwLock<HashMap<String, Value>>> {
        self.globals.clone()
    }
}

impl VM {
    pub fn new() -> Self {
        // Populate globals with built-in type classes
        let globals = builtins::create_builtin_classes();

        Self {
            stack: Vec::with_capacity(STACK_MAX),
            frames: Vec::with_capacity(FRAMES_MAX),
            globals: Arc::new(RwLock::new(globals)),
            file: String::new(),
            source: String::new(),
            exception_handlers: Vec::new(),
            open_upvalues: Vec::new(),
            gc: GcHeap::new(),
            gc_counter: 0,
            gc_stats_enabled: false,
            pending_module_workspace: None,
            args: Vec::new(),
        }
    }

    /// Create a new VM with SHARED globals (for HTTP handlers)
    /// The Arc<RwLock<HashMap>> is shared directly, so modifications are visible to all VMs
    pub fn new_with_shared_globals(globals: Arc<RwLock<HashMap<String, Value>>>) -> Self {
        Self {
            stack: Vec::with_capacity(STACK_MAX),
            frames: Vec::with_capacity(FRAMES_MAX),
            globals,
            file: String::new(),
            source: String::new(),
            exception_handlers: Vec::new(),
            open_upvalues: Vec::new(),
            gc: GcHeap::new(),
            gc_counter: 0,
            gc_stats_enabled: false,
            pending_module_workspace: None,
            args: Vec::new(),
        }
    }

    /// Set command-line arguments for the script
    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    /// Enable or disable GC statistics printing
    pub fn set_gc_stats_enabled(&mut self, enabled: bool) {
        self.gc_stats_enabled = enabled;
    }

    /// Get a clone of the current globals (for read-only access)
    pub fn get_globals(&self) -> HashMap<String, Value> {
        self.globals.read().unwrap().clone()
    }
    
    /// Get the shared globals Arc for passing to child VMs
    pub fn get_shared_globals(&self) -> Arc<RwLock<HashMap<String, Value>>> {
        self.globals.clone()
    }

    /// Run garbage collection if threshold is reached
    fn maybe_collect_garbage(&mut self) {
        // Check every 100 instructions
        self.gc_counter += 1;
        if self.gc_counter >= 100 {
            self.gc_counter = 0;
            if self.gc.should_collect() {
                self.collect_garbage();
            }
        }
    }

    /// Force garbage collection
    pub fn collect_garbage(&mut self) {
        // Gather roots from stack and globals
        let mut roots: Vec<&Value> = Vec::new();
        
        // Stack values are roots
        for value in &self.stack {
            roots.push(value);
        }
        
        // Global values are roots
        let globals_guard = self.globals.read().unwrap();
        for value in globals_guard.values() {
            roots.push(value);
        }
        
        // Closed upvalues are roots
        for upvalue in &self.open_upvalues {
            if let Ok(uv) = upvalue.lock() {
                if uv.closed.is_some() {
                    // Note: closed upvalues traced through function upvalues
                    // during GC's mark phase
                }
            }
        }
        
        self.gc.collect(roots);
        
        // Print GC stats if enabled
        if self.gc_stats_enabled {
            let stats = self.gc.get_stats();
            eprintln!(
                "[GC] collection #{}: tracked={}, cycles_broken={}, total_tracked={}",
                stats.collections,
                stats.tracked_count,
                stats.cycles_broken,
                stats.total_tracked
            );
        }
    }

    /// Track a newly created array for GC
    fn track_array(&mut self, arr: &Arc<Mutex<Vec<Value>>>) {
        self.gc.track_array(arr);
    }

    /// Track a newly created dictionary for GC
    fn track_dict(&mut self, dict: &Arc<Mutex<HashMap<String, Value>>>) {
        self.gc.track_dict(dict);
    }

    /// Track a newly created instance for GC
    fn track_instance(&mut self, inst: &Arc<Mutex<Instance>>) {
        self.gc.track_instance(inst);
    }

    /// Get GC statistics
    pub fn gc_stats(&self) -> super::gc::GcStats {
        self.gc.get_stats()
    }

    /// Run bytecode chunk - async entry point with event loop
    /// This is the main entry point that handles suspend/resume
    pub async fn run(&mut self, chunk: Chunk, file: &str, source: &str) -> SaldResult<Value> {
        self.file = file.to_string();
        self.source = source.to_string();
        
        // Push script directory for this file (will be popped when done)
        crate::push_script_dir(file);

        let mut main_function = Function::new("<script>", 0, chunk);
    main_function.file = file.to_string();
    let main_function = Arc::new(main_function);

        // Push a placeholder for the reserved slot 0 (like how functions have
        // their closure in slot 0). This keeps slot indices in sync.
        self.stack.push(Value::Null);

        self.frames.push(CallFrame::new(main_function, 0));

        // Run the event loop - this handles all suspend/resume
        let result = self.run_event_loop().await;
        
        // Pop script directory when done
        crate::pop_script_dir();
        
        result
    }

    /// Run a handler function with a single argument (used by HTTP server)
    /// Each request gets its own VM instance for true concurrent handling
    /// script_dir should be the directory of the main script for path resolution
    pub async fn run_handler(&mut self, handler: Value, arg: Value, script_dir: Option<&str>) -> SaldResult<Value> {
        self.file = "<handler>".to_string();
        self.source = String::new();

        // Push script directory for path resolution in handlers
        // We use the main script's directory (passed from HTTP server)
        if let Some(dir) = script_dir {
            // Create a fake file path to push as script dir
            let fake_path = format!("{}/handler.sald", dir);
            crate::push_script_dir(&fake_path);
        }

        // Push placeholder for slot 0
        self.stack.push(Value::Null);

        // Push the handler function
        self.stack.push(handler.clone());
        
        // Push the argument
        self.stack.push(arg);

        // Call the handler with 1 argument
        self.call_value(1)?;

        // Run the event loop to execute the handler
        let result = self.run_event_loop().await;
        
        // Pop script directory if we pushed one
        if script_dir.is_some() {
            crate::pop_script_dir();
        }
        
        result
    }

    /// Event loop - handles suspend/resume for async operations
    /// This is TRUE non-blocking async - no block_on anywhere!
    async fn run_event_loop(&mut self) -> SaldResult<Value> {
        loop {
            match self.execute_until_suspend() {
                ExecutionResult::Completed(value) => {
                    return Ok(value);
                }
                ExecutionResult::Suspended { receiver } => {
                    // TRUE ASYNC AWAIT - no blocking!
                    match receiver.await {
                        Ok(Ok(value)) => {
                            // Push the resolved value and continue execution
                            self.push(value)?;
                        }
                        Ok(Err(e)) => {
                            // Handle error through exception system
                            self.handle_native_error(e)?;
                        }
                        Err(_) => {
                            return Err(self.create_error(ErrorKind::RuntimeError, "Future was cancelled"));
                        }
                    }
                }
            }
        }
    }

    /// Execute bytecode until we hit await or complete
    /// Returns ExecutionResult to indicate if we suspended or completed
    fn execute_until_suspend(&mut self) -> ExecutionResult {
        loop {
            if self.frames.is_empty() {
                // Program completed - result is on top of stack
                return ExecutionResult::Completed(
                    self.stack.pop().unwrap_or(Value::Null)
                );
            }

            match self.execute_one_suspendable() {
                Ok(None) => continue, // Keep executing
                Ok(Some(result)) => return result, // Suspend or complete
                Err(e) => {
                    // Convert error to completed with error
                    // The caller will handle this
                    eprintln!("{}", e);
                    return ExecutionResult::Completed(Value::Null);
                }
            }
        }
    }

    /// Execute a single bytecode instruction
    /// Returns Ok(None) to continue, Ok(Some(...)) to suspend/complete, Err on error
    fn execute_one_suspendable(&mut self) -> SaldResult<Option<ExecutionResult>> {
        if self.frames.is_empty() {
            return Ok(Some(ExecutionResult::Completed(Value::Null)));
        }

        // Periodic GC check
        self.maybe_collect_garbage();

        let op = self.read_byte();
        let opcode = OpCode::from(op);

        match opcode {
            OpCode::Constant => {
                let idx = self.read_u16() as usize;
                let constant = self.read_constant(idx);
                self.push(constant)?;
            }

            OpCode::Null => self.push(Value::Null)?,
            OpCode::True => self.push(Value::Boolean(true))?,
            OpCode::False => self.push(Value::Boolean(false))?,

            OpCode::Pop => {
                self.pop()?;
            }

            OpCode::Dup => {
                let value = self.peek(0)?.clone();
                self.push(value)?;
            }

            OpCode::DupTwo => {
                // Duplicate top two elements: [a, b] -> [a, b, a, b]
                let b = self.peek(0)?.clone();
                let a = self.peek(1)?.clone();
                self.push(a)?;
                self.push(b)?;
            }

            OpCode::DefineGlobal => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                let value = self.pop()?;
                self.globals.write().unwrap().insert(name, value);
            }

            OpCode::GetGlobal => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                let value =
                    self.globals.read().unwrap().get(&name).cloned().ok_or_else(|| {
                        self.create_error(ErrorKind::NameError, &format!("Undefined variable '{}'", name))
                    })?;
                self.push(value)?;
            }

            OpCode::SetGlobal => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                if !self.globals.read().unwrap().contains_key(&name) {
                    return Err(self.create_error(ErrorKind::NameError, &format!("Undefined variable '{}'", name)));
                }
                let value = self.peek(0)?.clone();
                self.globals.write().unwrap().insert(name, value);
            }

            OpCode::GetLocal => {
                let slot = self.read_u16() as usize;
                let slots_start = self.current_frame().slots_start;
                let value = self.stack[slots_start + slot].clone();
                self.push(value)?;
            }

            OpCode::SetLocal => {
                let slot = self.read_u16() as usize;
                let slots_start = self.current_frame().slots_start;
                let value = self.peek(0)?.clone();
                self.stack[slots_start + slot] = value;
            }

            OpCode::GetUpvalue => {
                let idx = self.read_u16() as usize;
                let upvalue = self.current_frame().function.upvalues[idx].clone();
                let upvalue_ref = upvalue.lock().unwrap();
                let value = if let Some(ref closed) = upvalue_ref.closed {
                    // Closed upvalue - value is stored in the upvalue itself
                    (**closed).clone()
                } else {
                    // Open upvalue - value is on the stack
                    self.stack[upvalue_ref.location].clone()
                };
                drop(upvalue_ref);
                self.push(value)?;
            }

            OpCode::SetUpvalue => {
                let idx = self.read_u16() as usize;
                let value = self.peek(0)?.clone();
                let upvalue = self.current_frame().function.upvalues[idx].clone();
                let mut upvalue_ref = upvalue.lock().unwrap();
                if upvalue_ref.closed.is_some() {
                    // Closed upvalue - store in the upvalue
                    upvalue_ref.closed = Some(Box::new(value));
                } else {
                    // Open upvalue - store on the stack
                    let location = upvalue_ref.location;
                    drop(upvalue_ref);
                    self.stack[location] = value;
                }
            }

            OpCode::CloseUpvalue => {
                // Close the upvalue at the top of the stack
                // This is called when a local goes out of scope that has been captured
                let stack_top = self.stack.len() - 1;
                self.close_upvalues(stack_top);
                self.pop()?;
            }

            OpCode::Add => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match (&a, &b) {
                    (Value::Number(a), Value::Number(b)) => Value::Number(a + b),
                    (Value::String(a), Value::String(b)) => {
                        Value::String(Arc::new(format!("{}{}", a, b)))
                    }
                    (Value::String(a), b) => Value::String(Arc::new(format!("{}{}", a, b))),
                    (a, Value::String(b)) => Value::String(Arc::new(format!("{}{}", a, b))),
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Cannot add '{}' and '{}'",
                            a.type_name(),
                            b.type_name()
                        )));
                    }
                };
                self.push(result)?;
            }

            OpCode::Sub => self.binary_number_op(|a, b| a - b)?,
            OpCode::Mul => self.binary_number_op(|a, b| a * b)?,
            OpCode::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                match (&a, &b) {
                    (Value::Number(a), Value::Number(b)) => {
                        if *b == 0.0 {
                            return Err(self.create_error(ErrorKind::DivisionByZero, "Division by zero"));
                        }
                        self.push(Value::Number(a / b))?;
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Cannot divide '{}' by '{}'",
                            a.type_name(),
                            b.type_name()
                        )));
                    }
                }
            }
            OpCode::Mod => {
                let b = self.pop()?;
                let a = self.pop()?;
                match (&a, &b) {
                    (Value::Number(a), Value::Number(b)) => {
                        if *b == 0.0 {
                            return Err(self.create_error(ErrorKind::DivisionByZero, "Modulo by zero"));
                        }
                        self.push(Value::Number(a % b))?;
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Cannot modulo '{}' by '{}'",
                            a.type_name(),
                            b.type_name()
                        )));
                    }
                }
            }
            OpCode::Negate => {
                let value = self.pop()?;
                match value {
                    Value::Number(n) => self.push(Value::Number(-n))?,
                    _ => {
                        return Err(
                            self.create_error(ErrorKind::TypeError, &format!("Cannot negate '{}'", value.type_name()))
                        );
                    }
                }
            }

            OpCode::Equal => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(Value::Boolean(a == b))?;
            }

            OpCode::NotEqual => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(Value::Boolean(a != b))?;
            }

            OpCode::Less => self.comparison_op(|a, b| a < b)?,
            OpCode::LessEqual => self.comparison_op(|a, b| a <= b)?,
            OpCode::Greater => self.comparison_op(|a, b| a > b)?,
            OpCode::GreaterEqual => self.comparison_op(|a, b| a >= b)?,

            OpCode::Not => {
                let value = self.pop()?;
                self.push(Value::Boolean(!value.is_truthy()))?;
            }

            OpCode::Jump => {
                let offset = self.read_u16() as usize;
                self.current_frame_mut().ip += offset;
            }

            OpCode::JumpIfFalse => {
                let offset = self.read_u16() as usize;
                if !self.peek(0)?.is_truthy() {
                    self.current_frame_mut().ip += offset;
                }
            }

            OpCode::JumpIfTrue => {
                let offset = self.read_u16() as usize;
                if self.peek(0)?.is_truthy() {
                    self.current_frame_mut().ip += offset;
                }
            }

            OpCode::JumpIfNotNull => {
                let offset = self.read_u16() as usize;
                if !self.peek(0)?.is_null() {
                    self.current_frame_mut().ip += offset;
                }
            }

            OpCode::Loop => {
                let offset = self.read_u16() as usize;
                self.current_frame_mut().ip -= offset;
            }

            OpCode::Call => {
                let arg_count = self.read_u16() as usize;
                // Expand any SpreadMarker values in the arguments
                let actual_arg_count = self.expand_spread_args(arg_count)?;
                self.call_value(actual_arg_count)?;
            }

            OpCode::Closure => {
                let idx = self.read_u16() as usize;
                let constant = self.current_frame().function.chunk.constants[idx].clone();
                if let Constant::Function(ref func_const) = constant {
                    let mut function = Function::from_constant(func_const);

                    // Capture upvalues based on compile-time upvalue info
                    for upvalue_info in &func_const.upvalues {
                        let upvalue = if upvalue_info.is_local {
                            // Capture from enclosing function's local (on stack)
                            let slots_start = self.current_frame().slots_start;
                            let location = slots_start + upvalue_info.index as usize;
                            self.capture_upvalue(location)
                        } else {
                            // Capture from enclosing function's upvalue
                            self.current_frame().function.upvalues[upvalue_info.index as usize]
                                .clone()
                        };
                        function.upvalues.push(upvalue);
                    }

                    self.push(Value::Function(Arc::new(function)))?;
                }
            }

            OpCode::Return => {
                let result = self.pop()?;
                let returning_frame_index = self.frames.len() - 1;
                let frame = self.frames.pop().unwrap();
                
                // Pop the script directory if this function pushed one
                if !frame.function.file.is_empty() {
                    crate::pop_script_dir();
                }

                // CRITICAL: Clean up any exception handlers that belong to this frame
                // If we don't do this, stale handlers with invalid catch_ip values
                // will cause panics when exceptions occur later
                while let Some(handler) = self.exception_handlers.last() {
                    if handler.frame_index >= returning_frame_index {
                        self.exception_handlers.pop();
                    } else {
                        break;
                    }
                }

                // Close any upvalues that point to the current frame's locals
                // This must happen BEFORE we truncate the stack
                self.close_upvalues(frame.slots_start);

                if self.frames.is_empty() {
                    // Push result to stack for caller to retrieve
                    self.stack.truncate(frame.slots_start);
                    let return_value = result.clone();
                    self.push(result)?;
                    return Ok(Some(ExecutionResult::Completed(return_value)));
                }

                self.stack.truncate(frame.slots_start);

                // For init methods, return the instance instead of the result
                if let Some(instance) = frame.init_instance {
                    self.push(instance)?;
                } else {
                    self.push(result)?;
                }
            }

            OpCode::Class => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                let class = Arc::new(Class::new(&name));
                self.push(Value::Class(class))?;
            }

            OpCode::Method | OpCode::StaticMethod => {
                let idx = self.read_u16() as usize;
                let constant = self.current_frame().function.chunk.constants[idx].clone();

                if let Constant::Function(ref func_const) = constant {
                    let function = Arc::new(Function::from_constant(func_const));

                    // Get the class from stack
                    if let Value::Class(class) = self.peek(0)?.clone() {
                        let class_mut = Arc::as_ptr(&class) as *mut Class;
                        unsafe {
                            if opcode == OpCode::StaticMethod {
                                (*class_mut)
                                    .user_static_methods
                                    .insert(func_const.name.clone(), Value::Function(function));
                            } else {
                                (*class_mut)
                                    .methods
                                    .insert(func_const.name.clone(), Value::Function(function));
                            }
                        }
                    }
                }
            }

            OpCode::GetProperty => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                let obj = self.pop()?;

                match obj {
                    Value::Instance(instance) => {
                        // Check fields first
                        if let Some(value) = instance.lock().unwrap().fields.get(&name).cloned() {
                            self.push(value)?;
                        } else if let Some(method) =
                            instance.lock().unwrap().class.methods.get(&name).cloned()
                        {
                            // Bind method to instance
                            self.push(Value::Instance(instance.clone()))?;
                            self.push(method)?;
                        } else {
                            return Err(
                                self.create_error(ErrorKind::AttributeError, &format!("Undefined property '{}'", name))
                            );
                        }
                    }
                    Value::Class(class) => {
                        // Static field access (Math.PI, Math.E)
                        if let Some(value) = class.native_static_fields.get(&name) {
                            self.push(value.clone())?;
                        // Static method access - check user methods first, then native
                        } else if let Some(method) = class.user_static_methods.get(&name).cloned() {
                            self.push(method)?;
                        } else if let Some(method) = class.native_static_methods.get(&name) {
                            // Wrap native static method
                            self.push(Value::NativeFunction {
                                func: *method,
                                class_name: class.name.clone(),
                            })?;
                        } else {
                            return Err(
                                self.create_error(ErrorKind::AttributeError, &format!("Undefined property or method '{}' on class '{}'", name, class.name))
                            );
                        }
                    }
                    // Handle primitive types and arrays - look up their class's native_instance_methods
                    Value::String(_)
                    | Value::Number(_)
                    | Value::Boolean(_)
                    | Value::Null
                    | Value::Array(_)
                    | Value::Dictionary(_) => {
                        let class_name = builtins::get_builtin_class_name(&obj);
                        // Look up method FIRST, release the borrow before pushing
                        let method_result = {
                            let globals_guard = self.globals.read().unwrap();
                            if let Some(Value::Class(class)) = globals_guard.get(class_name) {
                                if let Some(method) = class.native_instance_methods.get(&name) {
                                    Ok((*method, name.clone()))
                                } else {
                                    Err(format!("'{}' has no method '{}'", class_name, name))
                                }
                            } else {
                                Err(format!("Built-in class '{}' not found", class_name))
                            }
                        };
                        // Now we can call self.push safely
                        match method_result {
                            Ok((method_fn, method_name)) => {
                                let receiver = obj.clone();
                                self.push(Value::InstanceMethod {
                                    receiver: Box::new(receiver),
                                    method: method_fn,
                                    method_name,
                                })?;
                            }
                            Err(msg) => {
                                return Err(self.create_error(ErrorKind::RuntimeError, &msg));
                            }
                        }
                    }
                    // Namespace member access
                    Value::Namespace { members, name: ns_name } => {
                        if let Ok(members) = members.lock() {
                            if let Some(value) = members.get(&name) {
                                self.push(value.clone())?;
                            } else {
                                return Err(self.create_error(ErrorKind::AttributeError, &format!(
                                    "Namespace '{}' has no member '{}'",
                                    ns_name, name
                                )));
                            }
                        } else {
                            return Err(self.create_error(ErrorKind::RuntimeError, "Failed to lock namespace members"));
                        }
                    }
                    // Enum variant access
                    Value::Enum { variants, name: enum_name } => {
                        if let Some(value) = variants.get(&name) {
                            self.push(value.clone())?;
                        } else {
                            return Err(self.create_error(ErrorKind::AttributeError, &format!(
                                "Enum '{}' has no variant '{}'",
                                enum_name, name
                            )));
                        }
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Only instances have properties, got '{}'",
                            obj.type_name()
                        )));
                    }
                }
            }

            OpCode::SetProperty => {
                let idx = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;
                let value = self.pop()?;
                let obj = self.pop()?;

                match obj {
                    Value::Instance(instance) => {
                        instance.lock().unwrap().fields.insert(name, value.clone());
                        self.push(value)?;
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Only instances have properties, got '{}'",
                            obj.type_name()
                        )));
                    }
                }
            }

            OpCode::GetSelf => {
                let slots_start = self.current_frame().slots_start;
                let value = self.stack[slots_start].clone();
                self.push(value)?;
            }

            OpCode::Invoke => {
                let idx = self.read_u16() as usize;
                let arg_count = self.read_u16() as usize;
                let name = self.read_string_constant(idx)?;

                self.invoke(&name, arg_count)?;
            }

            OpCode::BuildArray => {
                let count = self.read_u16() as usize;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count {
                    elements.push(self.pop()?);
                }
                elements.reverse(); // Stack order is reversed
                let arr = Arc::new(Mutex::new(elements));
                self.track_array(&arr);
                self.push(Value::Array(arr))?;
            }

            OpCode::BuildDict => {
                let count = self.read_u16() as usize;
                let mut map = std::collections::HashMap::new();
                // Pop key-value pairs in reverse order
                for _ in 0..count {
                    let value = self.pop()?;
                    let key = self.pop()?;
                    let key_str = match key {
                        Value::String(s) => s.to_string(),
                        _ => return Err(self.create_error(ErrorKind::TypeError, "Dictionary keys must be strings")),
                    };
                    map.insert(key_str, value);
                }
                let dict = Arc::new(Mutex::new(map));
                self.track_dict(&dict);
                self.push(Value::Dictionary(dict))?;
            }

            OpCode::BuildNamespace => {
                let count = self.read_u16() as usize;
                let mut members = std::collections::HashMap::new();
                // Pop key-value pairs in reverse order
                for _ in 0..count {
                    let value = self.pop()?;
                    let key = self.pop()?;
                    let key_str = match key {
                        Value::String(s) => s.to_string(),
                        _ => return Err(self.create_error(ErrorKind::TypeError, "Namespace member keys must be strings")),
                    };
                    members.insert(key_str, value);
                }
                // Create namespace with empty name (will be set by DefineGlobal)
                self.push(Value::Namespace {
                    name: String::new(),
                    members: Arc::new(Mutex::new(members)),
                })?;
            }

            OpCode::BuildEnum => {
                let count = self.read_u16() as usize;
                let mut variants = std::collections::HashMap::new();
                // Pop key-value pairs in reverse order
                for _ in 0..count {
                    let value = self.pop()?;
                    let key = self.pop()?;
                    let key_str = match key {
                        Value::String(s) => s.to_string(),
                        _ => return Err(self.create_error(ErrorKind::TypeError, "Enum variant keys must be strings")),
                    };
                    variants.insert(key_str, value);
                }
                // Create enum with empty name (will be set by DefineGlobal)
                self.push(Value::Enum {
                    name: String::new(),
                    variants: Arc::new(variants),
                })?;
            }

            OpCode::GetIndex => {
                let index = self.pop()?;
                let object = self.pop()?;

                match (&object, &index) {
                    (Value::Array(arr), Value::Number(idx)) => {
                        let idx = *idx as usize;
                        let arr = arr.lock().unwrap();
                        if idx < arr.len() {
                            self.push(arr[idx].clone())?;
                        } else {
                            return Err(self.create_error(ErrorKind::IndexError, &format!(
                                "Index {} out of bounds for array of length {}",
                                idx,
                                arr.len()
                            )));
                        }
                    }
                    (Value::String(s), Value::Number(idx)) => {
                        let idx = *idx as usize;
                        if idx < s.len() {
                            let ch = s.chars().nth(idx).unwrap_or(' ');
                            self.push(Value::String(Arc::new(ch.to_string())))?;
                        } else {
                            return Err(self.create_error(ErrorKind::IndexError, &format!(
                                "Index {} out of bounds for string of length {}",
                                idx,
                                s.len()
                            )));
                        }
                    }
                    (Value::Dictionary(dict), Value::String(key)) => {
                        let dict = dict.lock().unwrap();
                        let value = dict.get(key.as_str()).cloned().unwrap_or(Value::Null);
                        self.push(value)?;
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Cannot index '{}' with '{}'",
                            object.type_name(),
                            index.type_name()
                        )));
                    }
                }
            }

            OpCode::SetIndex => {
                let value = self.pop()?;
                let index = self.pop()?;
                let object = self.pop()?;

                match (&object, &index) {
                    (Value::Array(arr), Value::Number(idx)) => {
                        let idx = *idx as usize;
                        let mut arr = arr.lock().unwrap();
                        if idx < arr.len() {
                            arr[idx] = value.clone();
                            self.push(value)?;
                        } else {
                            return Err(self.create_error(ErrorKind::IndexError, &format!(
                                "Index {} out of bounds for array of length {}",
                                idx,
                                arr.len()
                            )));
                        }
                    }
                    (Value::Dictionary(dict), Value::String(key)) => {
                        dict.lock().unwrap().insert(key.to_string(), value.clone());
                        self.push(value)?;
                    }
                    _ => {
                        return Err(self.create_error(ErrorKind::TypeError, &format!(
                            "Cannot set index on '{}' with '{}'",
                            object.type_name(),
                            index.type_name()
                        )));
                    }
                }
            }

            OpCode::Inherit => {
                let _ = self.read_u16(); // Read operand (not used)

                // Stack: [subclass, superclass]
                let superclass_val = self.pop()?;
                let subclass_val = self.pop()?;

                if let (Value::Class(superclass), Value::Class(subclass)) =
                    (&superclass_val, &subclass_val)
                {
                    // Create new class with inherited methods
                    let mut new_class = Class::new(subclass.name.clone());

                    // Copy parent methods first
                    for (name, method) in &superclass.methods {
                        new_class.methods.insert(name.clone(), method.clone());
                    }

                    // Then copy child methods (override parent)
                    for (name, method) in &subclass.methods {
                        new_class.methods.insert(name.clone(), method.clone());
                    }

                    // Copy static methods
                    for (name, method) in &subclass.user_static_methods {
                        new_class
                            .user_static_methods
                            .insert(name.clone(), method.clone());
                    }

                    // Set superclass reference
                    new_class.superclass = Some(superclass.clone());

                    self.push(Value::Class(Arc::new(new_class)))?;
                } else {
                    return Err(self.create_error(ErrorKind::TypeError, &format!(
                        "Superclass must be a class, got '{}'",
                        superclass_val.type_name()
                    )));
                }
            }

            OpCode::GetSuper => {
                let idx = self.read_u16() as usize;
                let method_name = self.read_string_constant(idx)?;

                // Self should be on stack
                let receiver = self.pop()?;

                // Find method in superclass
                if let Value::Instance(ref instance) = receiver {
                    let inst = instance.lock().unwrap();
                    // Use the class stored in the instance directly
                    let class = inst.class.clone();
                    drop(inst);

                    // Access superclass from the instance's class
                    if let Some(ref superclass) = class.superclass {
                        if let Some(method) = superclass.methods.get(&method_name) {
                            if let Value::Function(func) = method {
                                // Create bound method so that
                                // when called, receiver is properly passed as 'self'
                                self.push(Value::BoundMethod {
                                    receiver: Box::new(receiver),
                                    method: func.clone(),
                                })?;
                            } else {
                                return Err(self.create_error(ErrorKind::TypeError, &format!(
                                    "Method '{}' is not a function",
                                    method_name
                                )));
                            }
                        } else {
                            return Err(self.create_error(ErrorKind::AttributeError, &format!(
                                "Undefined method '{}' in superclass",
                                method_name
                            )));
                        }
                    } else {
                        return Err(self.create_error(ErrorKind::RuntimeError, "Class has no superclass"));
                    }
                } else {
                    return Err(self.create_error(ErrorKind::RuntimeError, "'super' can only be used in instance methods"));
                }
            }

            OpCode::Import => {
                let idx = self.read_u16() as usize;
                let import_path = self.read_string_constant(idx)?;

                // Resolve path relative to current file (may set pending_module_workspace)
                let resolved_path = self.resolve_import_path(&import_path)?;

                // Check if this is a module import (has pending workspace)
                let module_workspace = self.pending_module_workspace.take();
                
                // Push module workspace if this is a module import
                if let Some(ref workspace) = module_workspace {
                    crate::push_module_workspace(workspace);
                }

                // Read, compile, and execute imported file
                let imported_globals = self.import_and_execute(&resolved_path)?;

                // Pop module workspace after import
                if module_workspace.is_some() {
                    crate::pop_module_workspace();
                }

                // Merge all globals from imported file into current globals
                for (name, value) in imported_globals {
                    // Skip built-in types that already exist
                    let globals_guard = self.globals.read().unwrap();
                    let should_insert = !globals_guard.contains_key(&name)
                        || !matches!(globals_guard.get(&name), Some(Value::Class(_)));
                    drop(globals_guard);
                    if should_insert {
                        self.globals.write().unwrap().insert(name, value);
                    }
                }
            }

            OpCode::ImportAs => {
                let path_idx = self.read_u16() as usize;
                let alias_idx = self.read_u16() as usize;
                let import_path = self.read_string_constant(path_idx)?;
                let alias = self.read_string_constant(alias_idx)?;

                // Resolve path relative to current file (may set pending_module_workspace)
                let resolved_path = self.resolve_import_path(&import_path)?;

                // Check if this is a module import (has pending workspace)
                let module_workspace = self.pending_module_workspace.take();
                
                // Push module workspace if this is a module import
                if let Some(ref workspace) = module_workspace {
                    crate::push_module_workspace(workspace);
                }

                // Read, compile, and execute imported file
                let imported_globals = self.import_and_execute(&resolved_path)?;

                // Pop module workspace after import
                if module_workspace.is_some() {
                    crate::pop_module_workspace();
                }

                // Create a module-like instance with all imported globals as fields
                let mut module_fields = HashMap::new();
                for (name, value) in imported_globals {
                    // Skip built-in types
                    if !matches!(&value, Value::Class(c) if
                            ["String", "Number", "Boolean", "Null", "Array"].contains(&c.name.as_str()))
                    {
                        module_fields.insert(name, value);
                    }
                }

                // Create a dummy class for the module
                let module_class = Arc::new(Class::new(&alias));

                // Create instance to hold module fields
                let module = Instance {
                    class_name: alias.clone(),
                    class: module_class,
                    fields: module_fields,
                };

                // Define module as global with alias name
                self.globals.write().unwrap()
                    .insert(alias, Value::Instance(Arc::new(Mutex::new(module))));
            }

            OpCode::TryStart => {
                let catch_offset = self.read_u16() as usize;

                // Calculate absolute catch IP
                let catch_ip = self.current_frame().ip + catch_offset;

                // Push exception handler
                self.exception_handlers.push(ExceptionHandler {
                    frame_index: self.frames.len() - 1,
                    stack_size: self.stack.len(),
                    catch_ip,
                });
            }

            OpCode::TryEnd => {
                // Try block completed successfully, pop handler
                self.exception_handlers.pop();
            }

            OpCode::Throw => {
                let exception_value = self.pop()?;

                // Look for exception handler
                if let Some(handler) = self.exception_handlers.pop() {
                    // Unwind stack to handler's frame
                    while self.frames.len() > handler.frame_index + 1 {
                        self.frames.pop();
                    }

                    // Restore stack size
                    while self.stack.len() > handler.stack_size {
                        self.stack.pop();
                    }

                    // Push exception value onto stack (will be bound to catch var)
                    self.push(exception_value)?;

                    // Jump to catch block
                    self.current_frame_mut().ip = handler.catch_ip;
                } else {
                    // No handler, convert to runtime error
                    let msg = match &exception_value {
                        Value::String(s) => s.to_string(),
                        other => format!("{}", other),
                    };
                    return Err(self.create_error(ErrorKind::RuntimeError, &format!("Uncaught exception: {}", msg)));
                }
            }

            OpCode::Await => {
                let value = self.pop()?;

                match value {
                    Value::Future(future_arc) => {
                        // Take the future receiver out of the Option
                        let mut guard = future_arc.lock().unwrap();
                        if let Some(future) = guard.take() {
                            drop(guard); // Release the lock

                            // SUSPEND - return to event loop with the receiver
                            // The event loop will await this and push the result
                            return Ok(Some(ExecutionResult::Suspended {
                                receiver: future.receiver,
                            }));
                        } else {
                            return Err(self.create_error(ErrorKind::RuntimeError, "Future has already been consumed"));
                        }
                    }
                    // Non-future values pass through unchanged
                    other => self.push(other)?,
                }
            }

            OpCode::SpreadArray => {
                // Pop the value and wrap it in a SpreadMarker for special handling in Call
                let value = self.pop()?;
                self.push(Value::SpreadMarker(Box::new(value)))?;
            }
        }

        Ok(None) // Continue execution
    }

    // ==================== Helper Methods ====================

    fn push(&mut self, value: Value) -> SaldResult<()> {
        if self.stack.len() >= STACK_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow"));
        }
        self.stack.push(value);
        Ok(())
    }

    fn pop(&mut self) -> SaldResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| self.create_error(ErrorKind::RuntimeError, "Stack underflow"))
    }

    fn peek(&self, distance: usize) -> SaldResult<&Value> {
        let idx = self
            .stack
            .len()
            .checked_sub(1 + distance)
            .ok_or_else(|| self.create_error(ErrorKind::RuntimeError, "Stack underflow"))?;
        Ok(&self.stack[idx])
    }

    /// Capture an upvalue for a given stack location
    /// If an open upvalue already exists for this location, reuse it
    fn capture_upvalue(&mut self, location: usize) -> Arc<Mutex<UpvalueObj>> {
        // Check if we already have an open upvalue for this location
        for upvalue in &self.open_upvalues {
            if upvalue.lock().unwrap().location == location {
                return upvalue.clone();
            }
        }

        // Create a new upvalue
        let upvalue = Arc::new(Mutex::new(UpvalueObj::new(location)));
        self.open_upvalues.push(upvalue.clone());
        upvalue
    }

    /// Close all upvalues at or above the given stack location
    /// This moves the value from the stack into the upvalue
    fn close_upvalues(&mut self, last: usize) {
        // Close any open upvalues pointing at or above 'last'
        self.open_upvalues.retain(|upvalue| {
            let location = upvalue.lock().unwrap().location;
            if location >= last {
                // Close this upvalue by capturing the value
                // Use get() to safely handle edge cases where stack might have been modified
                let value = self.stack.get(location).cloned().unwrap_or(Value::Null);
                upvalue.lock().unwrap().closed = Some(Box::new(value));
                false // Remove from open_upvalues list
            } else {
                true // Keep in list
            }
        });
    }

    fn current_frame(&self) -> &CallFrame {
        self.frames.last().unwrap()
    }

    fn current_frame_mut(&mut self) -> &mut CallFrame {
        self.frames.last_mut().unwrap()
    }

    fn read_byte(&mut self) -> u8 {
        self.current_frame_mut().read_byte()
    }

    fn read_u16(&mut self) -> u16 {
        self.current_frame_mut().read_u16()
    }

    fn read_constant(&self, idx: usize) -> Value {
        let constant = &self.current_frame().function.chunk.constants[idx];
        match constant {
            Constant::Number(n) => Value::Number(*n),
            Constant::String(s) => Value::String(Arc::new(s.clone())),
            Constant::Function(f) => Value::Function(Arc::new(Function::from_constant(f))),
            Constant::Class(c) => {
                let class = Class::new(&c.name);
                // Methods will be added via Method/StaticMethod opcodes
                Value::Class(Arc::new(class))
            }
        }
    }

    fn read_string_constant(&self, idx: usize) -> SaldResult<String> {
        let constant = &self.current_frame().function.chunk.constants[idx];
        match constant {
            Constant::String(s) => Ok(s.clone()),
            _ => Err(self.create_error(ErrorKind::TypeError, "Expected string constant")),
        }
    }

    fn binary_number_op(&mut self, op: fn(f64, f64) -> f64) -> SaldResult<()> {
        let b = self.pop()?;
        let a = self.pop()?;

        match (&a, &b) {
            (Value::Number(a), Value::Number(b)) => {
                self.push(Value::Number(op(*a, *b)))?;
            }
            _ => {
                return Err(self.create_error(ErrorKind::TypeError, &format!(
                    "Cannot perform operation on '{}' and '{}'",
                    a.type_name(),
                    b.type_name()
                )));
            }
        }
        Ok(())
    }

    fn comparison_op(&mut self, op: fn(f64, f64) -> bool) -> SaldResult<()> {
        let b = self.pop()?;
        let a = self.pop()?;

        match (&a, &b) {
            (Value::Number(a), Value::Number(b)) => {
                self.push(Value::Boolean(op(*a, *b)))?;
            }
            (Value::String(a), Value::String(b)) => {
                // Use lexicographic comparison for strings (like JavaScript/Rust)
                use std::cmp::Ordering;
                let cmp = a.cmp(b);
                let result = match cmp {
                    Ordering::Less => op(-1.0, 0.0),   // a < b
                    Ordering::Equal => op(0.0, 0.0),   // a == b
                    Ordering::Greater => op(1.0, 0.0), // a > b
                };
                self.push(Value::Boolean(result))?;
            }
            _ => {
                return Err(self.create_error(ErrorKind::TypeError, &format!(
                    "Cannot compare '{}' and '{}'",
                    a.type_name(),
                    b.type_name()
                )));
            }
        }
        Ok(())
    }

    /// Expand any SpreadMarker values in the top `arg_count` stack positions into individual values.
    /// Returns the new actual argument count after expansion.
    /// Uses splice to replace args in-place, preserving stack positions below args_start.
    fn expand_spread_args(&mut self, arg_count: usize) -> SaldResult<usize> {
        if arg_count == 0 {
            return Ok(0);
        }
        
        let stack_len = self.stack.len();
        let args_start = stack_len - arg_count;
        
        // First pass: check if there are any SpreadMarkers
        let mut has_spread = false;
        for i in args_start..stack_len {
            if matches!(&self.stack[i], Value::SpreadMarker(_)) {
                has_spread = true;
                break;
            }
        }
        
        if !has_spread {
            return Ok(arg_count);
        }
        
        // Collect all args values (clone to avoid borrow issues)
        let args: Vec<Value> = self.stack[args_start..stack_len].to_vec();
        
        // Build expanded args list
        let mut expanded_args = Vec::new();
        for arg in args {
            match arg {
                Value::SpreadMarker(boxed_value) => {
                    match *boxed_value {
                        Value::Array(arr) => {
                            // Expand array elements
                            let arr_guard = arr.lock().unwrap();
                            for elem in arr_guard.iter() {
                                expanded_args.push(elem.clone());
                            }
                        }
                        other => {
                            // Non-array values just pass through
                            expanded_args.push(other);
                        }
                    }
                }
                other => {
                    expanded_args.push(other);
                }
            }
        }
        
        let new_arg_count = expanded_args.len();
        
        // Use splice to replace the args range in-place
        // This preserves all stack elements before args_start
        let _ = self.stack.splice(args_start.., expanded_args);
        
        Ok(new_arg_count)
    }

    fn call_value(&mut self, arg_count: usize) -> SaldResult<()> {
        let callee = self.peek(arg_count)?.clone();

        match callee {
            Value::Function(function) => {
                self.call_function(function, arg_count)?;
            }
            Value::Class(class) => {
                // Check if this is a built-in class with a constructor (type conversion)
                if let Some(constructor) = class.constructor {
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.pop()?; // Pop the class

                    match constructor(&args) {
                        Ok(result) => self.push(result)?,
                        Err(e) => {
                            self.handle_native_error(e)?;
                            return Ok(()); // Execution continues from catch block
                        }
                    }
                } else {
                    // User-defined class - create instance
                    let instance = Arc::new(Mutex::new(Instance::new(class.clone())));
                    self.track_instance(&instance);
                    let instance_value = Value::Instance(instance.clone());

                    // Replace class with instance on stack
                    let stack_idx = self.stack.len() - arg_count - 1;
                    self.stack[stack_idx] = instance_value.clone();

                    // Call init if it exists
                    if let Some(init) = class.methods.get("init") {
                        if let Value::Function(init_fn) = init {
                            // Use special init call that will return instance
                            self.call_function_init(init_fn.clone(), arg_count, instance_value)?;
                        }
                    } else if arg_count > 0 {
                        return Err(self.create_error(ErrorKind::ArgumentError, &format!(
                            "Expected 0 arguments but got {}",
                            arg_count
                        )));
                    }
                }
            }
            Value::NativeFunction { func, .. } => {
                let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                self.pop()?; // Pop the native function

                match func(&args) {
                    Ok(result) => self.push(result)?,
                    Err(e) => {
                        self.handle_native_error(e)?;
                        return Ok(()); // Execution continues from catch block
                    }
                }
            }
            Value::InstanceMethod {
                receiver, method, ..
            } => {
                // Call native instance method on primitive
                let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                self.pop()?; // Pop the instance method

                match method(&receiver, &args) {
                    Ok(result) => self.push(result)?,
                    Err(e) => {
                        self.handle_native_error(e)?;
                        return Ok(()); // Execution continues from catch block
                    }
                }
            }
            Value::BoundMethod { receiver, method } => {
                // User-defined method bound to a receiver (used for super calls)
                // We need to set up the call frame so that 'self' (receiver) is at slot 0
                let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                self.pop()?; // Pop the bound method

                // Push receiver first (will be at slot 0)
                self.push((*receiver).clone())?;
                // Push arguments
                for arg in args {
                    self.push(arg)?;
                }

                // Call the function
                self.call_function(method.clone(), arg_count)?;
            }
            _ => {
                return Err(
                    self.create_error(ErrorKind::TypeError, &format!("'{}' is not callable", callee.type_name()))
                );
            }
        }

        Ok(())
    }

    fn call_function(&mut self, function: Arc<Function>, arg_count: usize) -> SaldResult<()> {
        // For variadic functions, arity is minimum required args (excluding variadic param itself)
        let min_arity = if function.is_variadic {
            function.arity.saturating_sub(1)
        } else {
            function.arity
        };

        if function.is_variadic {
            // Variadic: need at least (arity-1) args, extra args packed into array
            if arg_count < min_arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!(
                    "Expected at least {} arguments but got {}",
                    min_arity, arg_count
                )));
            }

            // Pack variadic args into array
            let variadic_count = arg_count - min_arity;
            let mut variadic_args = Vec::with_capacity(variadic_count);

            // Pop variadic args from stack (in reverse order)
            for _ in 0..variadic_count {
                variadic_args.push(self.pop()?);
            }
            variadic_args.reverse();

            // Push the array onto stack as the variadic parameter
            let array = Value::Array(Arc::new(Mutex::new(variadic_args)));
            self.push(array)?;

            // Now arg_count on stack is min_arity + 1 (for the array)
            let effective_arg_count = min_arity + 1;

            if self.frames.len() >= FRAMES_MAX {
                return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
            }

            let slots_start = self.stack.len() - effective_arg_count - 1;
            
            // Push the function's file directory for path resolution
            // This ensures relative paths are resolved based on where the function is defined
            if !function.file.is_empty() {
                crate::push_script_dir(&function.file);
            }
            
            self.frames.push(CallFrame::new(function, slots_start));
        } else {
            // Non-variadic: check arity with default params support
            // Required params = arity - default_count
            let required_arity = function.arity.saturating_sub(function.default_count);
            
            if arg_count < required_arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!(
                    "Expected at least {} arguments but got {}",
                    required_arity, arg_count
                )));
            }
            
            if arg_count > function.arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!(
                    "Expected at most {} arguments but got {}",
                    function.arity, arg_count
                )));
            }
            
            // Push Null values for missing optional arguments
            let missing_args = function.arity - arg_count;
            for _ in 0..missing_args {
                self.push(Value::Null)?;
            }

            if self.frames.len() >= FRAMES_MAX {
                return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
            }

            let slots_start = self.stack.len() - function.arity - 1;
            
            // Push the function's file directory for path resolution
            if !function.file.is_empty() {
                crate::push_script_dir(&function.file);
            }
            
            self.frames.push(CallFrame::new(function, slots_start));
        }

        Ok(())
    }

    fn call_function_init(
        &mut self,
        function: Arc<Function>,
        arg_count: usize,
        instance: Value,
    ) -> SaldResult<()> {
        if arg_count != function.arity {
            return Err(self.create_error(ErrorKind::ArgumentError, &format!(
                "Expected {} arguments but got {}",
                function.arity, arg_count
            )));
        }

        if self.frames.len() >= FRAMES_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
        }

        let slots_start = self.stack.len() - arg_count - 1;
        
        // Push the function's file directory for path resolution
        if !function.file.is_empty() {
            crate::push_script_dir(&function.file);
        }
        
        self.frames
            .push(CallFrame::new_init(function, slots_start, instance));

        Ok(())
    }

    fn invoke(&mut self, name: &str, arg_count: usize) -> SaldResult<()> {
        let receiver = self.peek(arg_count)?.clone();

        match receiver {
            Value::Instance(ref instance) => {
                // Check for field first
                if let Some(field) = instance.lock().unwrap().fields.get(name).cloned() {
                    // Replace receiver with field value and call
                    let stack_idx = self.stack.len() - arg_count - 1;
                    self.stack[stack_idx] = field.clone();
                    return self.call_value(arg_count);
                }

                // Get class reference for method lookup
                let class = instance.lock().unwrap().class.clone();

                // Check user-defined methods
                if let Some(method) = class.methods.get(name).cloned() {
                    if let Value::Function(func) = method {
                        return self.call_function(func, arg_count);
                    }
                }

                // Check callable native instance methods (methods that need ValueCaller)
                if let Some(callable_method) = class.callable_native_instance_methods.get(name).copied() {
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.pop()?; // Pop receiver

                    match callable_method(&receiver, &args, self) {
                        Ok(result) => {
                            self.push(result)?;
                            return Ok(());
                        }
                        Err(e) => {
                            self.handle_native_error(e)?;
                            return Ok(());
                        }
                    }
                }

                // Check regular native instance methods
                if let Some(method) = class.native_instance_methods.get(name).copied() {
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.pop()?; // Pop receiver

                    match method(&receiver, &args) {
                        Ok(result) => {
                            self.push(result)?;
                            return Ok(());
                        }
                        Err(e) => {
                            self.handle_native_error(e)?;
                            return Ok(());
                        }
                    }
                }

                Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined method '{}' on instance", name)))
            }
            Value::Class(class) => {

                // Static method call - check user methods first, then native
                if let Some(method) = class.user_static_methods.get(name).cloned() {
                    // Pop class and push null for static methods (no self)
                    let stack_idx = self.stack.len() - arg_count - 1;
                    self.stack[stack_idx] = Value::Null;

                    if let Value::Function(func) = method {
                        return self.call_function(func, arg_count);
                    }
                }

                // Check native static methods
                if let Some(native_fn) = class.native_static_methods.get(name).copied() {
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.pop()?; // Pop class

                    match native_fn(&args) {
                        Ok(result) => {
                            self.push(result)?;
                            return Ok(());
                        }
                        Err(e) => {
                            self.handle_native_error(e)?;
                            return Ok(()); // Execution continues from catch block
                        }
                    }
                }

                Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined static method '{}'", name)))
            }
            // Handle primitive types - look up native methods from class
            Value::String(_)
            | Value::Number(_)
            | Value::Boolean(_)
            | Value::Null
            | Value::Array(_)
            | Value::Dictionary(_) => {
                let class_name = builtins::get_builtin_class_name(&receiver);

                // Get class reference first
                let class = if let Some(Value::Class(c)) = self.globals.read().unwrap().get(class_name).cloned() {
                    c
                } else {
                    return Err(
                        self.create_error(ErrorKind::RuntimeError, &format!("Built-in class '{}' not found", class_name))
                    );
                };

                // First check callable native methods (map, filter, forEach, etc.)
                if let Some(callable_method) =
                    class.callable_native_instance_methods.get(name).copied()
                {
                    // Pop args from stack
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    // Pop receiver
                    self.pop()?;

                    // Call callable native method with self as the caller
                    match callable_method(&receiver, &args, self) {
                        Ok(result) => {
                            self.push(result)?;
                            return Ok(());
                        }
                        Err(e) => {
                            self.handle_native_error(e)?;
                            return Ok(()); // Execution continues from catch block
                        }
                    }
                }

                // Then check regular native methods
                if let Some(method) = class.native_instance_methods.get(name).copied() {
                    // Pop args from stack
                    let args: Vec<Value> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    // Pop receiver
                    self.pop()?;

                    // Call native method
                    match method(&receiver, &args) {
                        Ok(result) => {
                            self.push(result)?;
                            Ok(())
                        }
                        Err(e) => {
                            self.handle_native_error(e)?;
                            Ok(()) // Execution continues from catch block
                        }
                    }
                } else {
                    Err(self.create_error(ErrorKind::AttributeError, &format!("'{}' has no method '{}'", class_name, name)))
                }
            }
            // Namespace member invocation
            Value::Namespace { members, name: ns_name } => {
                if let Ok(members) = members.lock() {
                    if let Some(member) = members.get(name).cloned() {
                        drop(members); // Release lock before call
                        // Replace receiver (namespace) with the member value on stack
                        let stack_idx = self.stack.len() - arg_count - 1;
                        self.stack[stack_idx] = member;
                        return self.call_value(arg_count);
                    } else {
                        return Err(self.create_error(ErrorKind::AttributeError, &format!(
                            "Namespace '{}' has no member '{}'",
                            ns_name, name
                        )));
                    }
                } else {
                    return Err(self.create_error(ErrorKind::RuntimeError, "Failed to lock namespace members"));
                }
            }
            _ => Err(self.create_error(ErrorKind::TypeError, &format!(
                "Only instances have methods, got '{}'",
                receiver.type_name()
            ))),
        }
    }

    fn create_error(&self, kind: ErrorKind, message: &str) -> SaldError {
        let (span, file) = if !self.frames.is_empty() {
            let frame = self.current_frame();
            let f = if frame.function.file.is_empty() {
                &self.file
            } else {
                &frame.function.file
            };
            (frame.current_span(), f)
        } else {
            (Span::default(), &self.file)
        };

        let mut error = SaldError::new(kind, message, span, file);

        // Attach source
        if file == &self.file {
            error = error.with_source(&self.source);
        } else {
            // Try to read source from file
            if let Ok(source) = std::fs::read_to_string(file) {
                error = error.with_source(&source);
            }
        }

        // Add stack trace with span info
        let mut stack_trace = Vec::new();
        for frame in self.frames.iter().rev() {
            let frame_span = frame.current_span();
            stack_trace.push(StackFrame::new(
                &frame.function.name,
                if frame.function.file.is_empty() {
                    &self.file
                } else {
                    &frame.function.file
                },
                frame_span.start.line,
                frame_span.start.column,
            ));
        }
        error = error.with_stack_trace(stack_trace);

        error
    }

    fn handle_native_error(&mut self, error_msg: String) -> SaldResult<()> {
        // Check if there's an exception handler
        if let Some(handler) = self.exception_handlers.pop() {
            // Unwind stack to handler's frame
            while self.frames.len() > handler.frame_index + 1 {
                self.frames.pop();
            }

            // Restore stack size
            while self.stack.len() > handler.stack_size {
                self.stack.pop();
            }

            // Push error message as the exception value
            let exception = Value::String(Arc::new(error_msg));
            self.push(exception)?;

            // Jump to catch block
            self.current_frame_mut().ip = handler.catch_ip;

            Ok(())
        } else {
            // No handler, return as runtime error
            Err(self.create_error(ErrorKind::RuntimeError, &format!("Uncaught exception: {}", error_msg)))
        }
    }

    // ==================== Import Helper Methods ====================

    fn resolve_import_path(&mut self, import_path: &str) -> SaldResult<String> {
        // Check if this is a module import (no path separators, no .sald extension)
        if Self::is_module_import(import_path) {
            return self.resolve_module_import(import_path);
        }

        // Regular file import
        self.resolve_file_import(import_path)
    }

    /// Check if import path is a module name (not a file path)
    /// Module imports: "uuid", "spark" 
    /// File imports: "./file.sald", "path/file.sald", "file.sald"
    fn is_module_import(path: &str) -> bool {
        !path.contains('/') && !path.contains('\\') && !path.ends_with(".sald") && !path.ends_with(".saldc")
    }

    /// Resolve a module import from sald_modules/<name>/
    fn resolve_module_import(&mut self, module_name: &str) -> SaldResult<String> {
        // Get project root (where sald_modules should be)
        let project_root = crate::get_project_root()
            .ok_or_else(|| self.create_error(ErrorKind::ImportError, 
                &format!("Cannot import module '{}': no project root set", module_name)))?;

        // Look for module in sald_modules/
        let module_dir = project_root.join("sald_modules").join(module_name);
        
        if !module_dir.exists() {
            return Err(self.create_error(ErrorKind::ImportError, 
                &format!("Module '{}' not found in sald_modules/", module_name)));
        }

        // Look for salad.json config
        let config_path = module_dir.join("salad.json");
        if !config_path.exists() {
            return Err(self.create_error(ErrorKind::ImportError, 
                &format!("Module '{}' has no salad.json config", module_name)));
        }

        // Parse salad.json to get main entry
        let main_entry = self.parse_module_config(&config_path, module_name)?;

        // Resolve main entry relative to module directory
        let main_path = module_dir.join(&main_entry);
        
        if !main_path.exists() {
            return Err(self.create_error(ErrorKind::ImportError, 
                &format!("Module '{}' main file '{}' not found", module_name, main_entry)));
        }

        // Store module directory for workspace push during import
        self.pending_module_workspace = Some(module_dir);

        main_path.to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| self.create_error(ErrorKind::ImportError, 
                &format!("Invalid module path for '{}'", module_name)))
    }

    /// Parse salad.json to get the main entry file
    fn parse_module_config(&self, config_path: &std::path::Path, module_name: &str) -> SaldResult<String> {
        // Read and parse JSON directly
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| self.create_error(ErrorKind::ImportError, 
                &format!("Failed to read module '{}' config: {}", module_name, e)))?;

        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| self.create_error(ErrorKind::ImportError, 
                &format!("Module '{}' config invalid JSON: {}", module_name, e)))?;

        json.get("main")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| self.create_error(ErrorKind::ImportError, 
                &format!("Module '{}' config missing 'main' field", module_name)))
    }

    /// Resolve a file import (original behavior)
    fn resolve_file_import(&self, import_path: &str) -> SaldResult<String> {
        use std::path::PathBuf;
        use std::env;

        // 1. Normalize extension: Append .sald if not present
        let path_with_ext = if !import_path.ends_with(".sald") && !import_path.ends_with(".saldc") {
            format!("{}.sald", import_path)
        } else {
            import_path.to_string()
        };
        
        let path_buf = PathBuf::from(&path_with_ext);

        // Helper to canonicalize path (convert to absolute)
        let canonicalize = |p: PathBuf| -> Option<String> {
            p.canonicalize()
                .ok()
                .and_then(|abs| abs.to_str().map(|s| s.to_string()))
        };

        // 2. If valid absolute path, return it if it exists
        if path_buf.is_absolute() {
             if path_buf.exists() {
                 return canonicalize(path_buf.clone())
                    .ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path)));
             }
        }

        // 3. Try relative to current file
        let current_dir = if self.file.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(&self.file)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };

        let relative_path = current_dir.join(&path_with_ext);
        if relative_path.exists() {
            return canonicalize(relative_path.clone())
                .ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path)));
        }

        // 4. Try SALD_MODULE env var
        if let Ok(module_path) = env::var("SALD_MODULE") {
            let env_path = PathBuf::from(module_path).join(&path_with_ext);
            if env_path.exists() {
                 return canonicalize(env_path.clone())
                    .ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path)));
            }
        }

        // 5. Fallback: Return relative path (so error message reflects local path)
        relative_path
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path)))
    }

    fn import_and_execute(&mut self, path: &str) -> SaldResult<HashMap<String, Value>> {
        // Check if this is a compiled .saldc file
        let chunk = if path.ends_with(".saldc") {
            // Read binary file
            let data = std::fs::read(path).map_err(|e| {
                self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e))
            })?;

            // Deserialize bytecode
            crate::binary::deserialize(&data).map_err(|e| {
                self.create_error(ErrorKind::ImportError, &format!("Error deserializing import '{}': {}", path, e))
            })?
        } else {
            // Read source file
            let source = std::fs::read_to_string(path).map_err(|e| {
                self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e))
            })?;

            // Scan tokens
            let mut scanner = Scanner::new(&source, path);
            let tokens = scanner.scan_tokens().map_err(|e| {
                self.create_error(ErrorKind::SyntaxError, &format!("Error scanning import '{}': {}", path, e))
            })?;

            // Parse AST
            let mut parser = Parser::new(tokens, path, &source);
            let program = parser.parse().map_err(|e| {
                self.create_error(ErrorKind::SyntaxError, &format!("Error parsing import '{}': {}", path, e))
            })?;

            // Compile to bytecode
            let mut compiler = Compiler::new(path, &source);
            compiler.compile(&program).map_err(|e| {
                self.create_error(ErrorKind::SyntaxError, &format!("Error compiling import '{}': {}", path, e))
            })?
        };

        // Save current VM state
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_file = std::mem::replace(&mut self.file, path.to_string());
        let saved_source = std::mem::replace(&mut self.source, String::new());
        
        // Push script directory for this imported file
        crate::push_script_dir(path);

        // Create fresh globals for import (but keep built-ins) - use NEW Arc for isolation
        let import_globals = Arc::new(RwLock::new(builtins::create_builtin_classes()));
        let saved_globals = std::mem::replace(&mut self.globals, import_globals);

        // Execute imported code synchronously
        let mut main_function = Function::new("<import>", 0, chunk);
        main_function.file = path.to_string();
        let main_function = Arc::new(main_function);
        self.stack.push(Value::Null);
        self.frames.push(CallFrame::new(main_function, 0));

        // Execute until done (imports shouldn't use await at top level)
        loop {
            match self.execute_until_suspend() {
                ExecutionResult::Completed(_) => break,
                ExecutionResult::Suspended { receiver } => {
                    // If import uses await, block on it (rare case)
                    match futures::executor::block_on(receiver) {
                        Ok(Ok(value)) => {
                            let _ = self.push(value);
                        }
                        Ok(Err(e)) => {
                            // Pop script_dir and restore state
                            crate::pop_script_dir();
                            self.stack = saved_stack;
                            self.frames = saved_frames;
                            self.file = saved_file;
                            self.source = saved_source;
                            self.globals = saved_globals;
                            return Err(self.create_error(ErrorKind::ImportError, &format!("Import error: {}", e)));
                        }
                        Err(_) => {
                            crate::pop_script_dir();
                            self.stack = saved_stack;
                            self.frames = saved_frames;
                            self.file = saved_file;
                            self.source = saved_source;
                            self.globals = saved_globals;
                            return Err(self.create_error(ErrorKind::ImportError, "Import future cancelled"));
                        }
                    }
                }
            }
        }

        // Capture imported globals - take from the RwLock, not the Arc
        let imported_globals = std::mem::take(&mut *self.globals.write().unwrap());

        // Pop script directory for this import (back to caller's dir)
        crate::pop_script_dir();

        // Restore VM state
        self.stack = saved_stack;
        self.frames = saved_frames;
        self.file = saved_file;
        self.source = saved_source;
        self.globals = saved_globals;

        Ok(imported_globals)
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}
