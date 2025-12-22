// Sald Virtual Machine - Threaded Code Implementation
// Stack-based VM with dispatch table for fast execution
// Uses Arc/Mutex for thread-safe async support
// Implements suspend/resume for true non-blocking async

use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use smallvec::SmallVec;

use crate::builtins;
use crate::compiler::chunk::{Chunk, Constant};
use crate::compiler::Compiler;
use crate::error::{ErrorKind, SaldError, SaldResult, Span, StackFrame};
use crate::lexer::Scanner;
use crate::parser::Parser;
use crate::vm::caller::ValueCaller;
use crate::vm::gc::GcHeap;
use crate::vm::value::{Class, Function, Instance, UpvalueObj, Value};

#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::oneshot;

const STACK_MAX: usize = 65536;
const FRAMES_MAX: usize = 4096;

// Start small, grow as needed - avoids over-allocation for simple scripts
const STACK_INIT: usize = 256;
const FRAMES_INIT: usize = 32;

/// Result of VM execution - supports suspend/resume for async
#[cfg(not(target_arch = "wasm32"))]
pub enum ExecutionResult {
    Completed(Value),
    Suspended {
        receiver: oneshot::Receiver<Result<Value, String>>,
    },
    Error(SaldError),
}

#[cfg(target_arch = "wasm32")]
pub enum ExecutionResult {
    Completed(Value),
    Error(SaldError),
}

/// Call frame for function execution
#[derive(Clone)]
struct CallFrame {
    function: Arc<Function>,
    ip: usize,
    slots_start: usize,
    init_instance: Option<Value>,
    /// Class context for private access checking (Some if this is an instance method)
    class_context: Option<String>,
    /// Saved globals for module function calls - restore when this frame pops
    saved_globals: Option<Arc<RwLock<FxHashMap<String, Value>>>>,
}

impl CallFrame {
    fn new(function: Arc<Function>, slots_start: usize) -> Self {
        Self { function, ip: 0, slots_start, init_instance: None, class_context: None, saved_globals: None }
    }

    fn new_with_class(function: Arc<Function>, slots_start: usize, class_name: String) -> Self {
        Self { function, ip: 0, slots_start, init_instance: None, class_context: Some(class_name), saved_globals: None }
    }

    fn new_init_with_class(function: Arc<Function>, slots_start: usize, instance: Value, class_name: String) -> Self {
        Self { function, ip: 0, slots_start, init_instance: Some(instance), class_context: Some(class_name), saved_globals: None }
    }

    #[inline(always)]
    fn read_byte(&mut self) -> u8 {
        let byte = unsafe { *self.function.chunk.code.get_unchecked(self.ip) };
        self.ip += 1;
        byte
    }

    #[inline(always)]
    fn read_u16(&mut self) -> u16 {
        unsafe {
            let high = *self.function.chunk.code.get_unchecked(self.ip) as u16;
            let low = *self.function.chunk.code.get_unchecked(self.ip + 1) as u16;
            self.ip += 2;
            (high << 8) | low
        }
    }

    fn current_span(&self) -> Span {
        self.function.chunk.get_span(self.ip.saturating_sub(1))
    }
}

#[derive(Clone)]
struct ExceptionHandler {
    frame_index: usize,
    stack_size: usize,
    catch_ip: usize,
}

/// The Sald Virtual Machine with Threaded Code Dispatch
pub struct VM {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: Arc<RwLock<FxHashMap<String, Value>>>,
    file: String,
    source: String,
    exception_handlers: SmallVec<[ExceptionHandler; 4]>,
    open_upvalues: Vec<Arc<Mutex<UpvalueObj>>>,
    gc: GcHeap,
    gc_counter: usize,
    gc_stats_enabled: bool,
    pending_module_workspace: Option<std::path::PathBuf>,
    args: Vec<String>,
    /// Stack of namespace names for private access checking
    namespace_context: Vec<String>,
}

// ==================== Dispatch Table ====================

/// Control flow result from opcode handlers
#[cfg(not(target_arch = "wasm32"))]
enum ControlFlow {
    Continue,
    Return(Value),
    Suspend(oneshot::Receiver<Result<Value, String>>),
    Error(SaldError),
}

#[cfg(target_arch = "wasm32")]
enum ControlFlow {
    Continue,
    Return(Value),
    Error(SaldError),
}

/// Opcode handler function type
type OpHandler = fn(&mut VM) -> ControlFlow;

/// Single unified dispatch table - 68 opcodes (0-67)
static DISPATCH: [OpHandler; 68] = [
    op_constant,      // 0
    op_pop,           // 1
    op_dup,           // 2
    op_dup_two,       // 3
    op_swap,          // 4
    op_null,          // 5
    op_true,          // 6
    op_false,         // 7
    op_define_global, // 8
    op_get_global,    // 9
    op_set_global,    // 10
    op_get_local,     // 11
    op_set_local,     // 12
    op_add,           // 13
    op_sub,           // 14
    op_mul,           // 15
    op_div,           // 16
    op_mod,           // 17
    op_negate,        // 18
    op_equal,         // 19
    op_not_equal,     // 20
    op_less,          // 21
    op_less_equal,    // 22
    op_greater,       // 23
    op_greater_equal, // 24
    op_not,           // 25
    op_jump,          // 26
    op_jump_if_false, // 27
    op_jump_if_true,  // 28
    op_jump_if_not_null, // 29
    op_loop,          // 30
    op_call,          // 31
    op_return,        // 32
    op_closure,       // 33
    op_class,         // 34
    op_method,        // 35
    op_static_method, // 36
    op_get_property,  // 37
    op_set_property,  // 38
    op_get_self,      // 39
    op_invoke,        // 40
    op_build_array,   // 41
    op_get_index,     // 42
    op_set_index,     // 43
    op_build_dict,    // 44
    op_build_namespace, // 45
    op_build_enum,    // 46
    op_inherit,       // 47
    op_get_super,     // 48
    op_import,        // 49
    op_import_as,     // 50
    op_get_upvalue,   // 51
    op_set_upvalue,   // 52
    op_close_upvalue, // 53
    op_try_start,     // 54
    op_try_end,       // 55
    op_throw,         // 56
    op_await,         // 57
    op_spread_array,  // 58
    op_bit_and,       // 59
    op_bit_or,        // 60
    op_bit_xor,       // 61
    op_bit_not,       // 62
    op_left_shift,    // 63
    op_right_shift,   // 64
    op_build_range_inclusive, // 65
    op_build_range_exclusive, // 66
    op_nop,           // 67
];

// ==================== Opcode Handlers ====================

#[inline(always)]
fn op_constant(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let constant = vm.read_constant(idx);
    if vm.push_fast(constant).is_err() {
        return ControlFlow::Error(vm.create_error(ErrorKind::RuntimeError, "Stack overflow"));
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_pop(vm: &mut VM) -> ControlFlow {
    vm.stack.pop();
    ControlFlow::Continue
}

#[inline(always)]
fn op_dup(vm: &mut VM) -> ControlFlow {
    if !vm.stack.is_empty() {
        let v = vm.peek_unchecked(0).clone();
        vm.stack.push(v);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_dup_two(vm: &mut VM) -> ControlFlow {
    if vm.stack.len() >= 2 {
        let b = vm.peek_unchecked(0).clone();
        let a = vm.peek_unchecked(1).clone();
        vm.stack.push(a);
        vm.stack.push(b);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_swap(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len >= 2 {
        vm.stack.swap(len - 1, len - 2);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_null(vm: &mut VM) -> ControlFlow {
    vm.stack.push(Value::Null);
    ControlFlow::Continue
}

#[inline(always)]
fn op_true(vm: &mut VM) -> ControlFlow {
    vm.stack.push(Value::Boolean(true));
    ControlFlow::Continue
}

#[inline(always)]
fn op_false(vm: &mut VM) -> ControlFlow {
    vm.stack.push(Value::Boolean(false));
    ControlFlow::Continue
}

#[inline(always)]
fn op_define_global(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => {
            if !vm.stack.is_empty() {
                let value = vm.pop_fast();
                vm.globals.write().insert(name, value);
            }
            ControlFlow::Continue
        }
        Err(e) => ControlFlow::Error(e),
    }
}

#[inline(always)]
fn op_get_global(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => {
            let value = vm.globals.read().get(&name).cloned();
            match value {
                Some(v) => {
                    vm.stack.push(v);
                    ControlFlow::Continue
                }
                None => ControlFlow::Error(vm.create_error(
                    ErrorKind::NameError,
                    &format!("Undefined variable '{}'", name),
                )),
            }
        }
        Err(e) => ControlFlow::Error(e),
    }
}

#[inline(always)]
fn op_set_global(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => {
            if !vm.globals.read().contains_key(&name) {
                return ControlFlow::Error(vm.create_error(
                    ErrorKind::NameError,
                    &format!("Undefined variable '{}'", name),
                ));
            }
            if let Some(value) = vm.peek().cloned() {
                vm.globals.write().insert(name, value);
            }
            ControlFlow::Continue
        }
        Err(e) => ControlFlow::Error(e),
    }
}

#[inline(always)]
fn op_get_local(vm: &mut VM) -> ControlFlow {
    let slot = vm.read_u16() as usize;
    let slots_start = vm.current_frame().slots_start;
    let value = unsafe { vm.stack.get_unchecked(slots_start + slot).clone() };
    vm.stack.push(value);
    ControlFlow::Continue
}

#[inline(always)]
fn op_set_local(vm: &mut VM) -> ControlFlow {
    let slot = vm.read_u16() as usize;
    let slots_start = vm.current_frame().slots_start;
    if !vm.stack.is_empty() {
        let value = vm.peek_unchecked(0).clone();
        unsafe { *vm.stack.get_unchecked_mut(slots_start + slot) = value; }
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_add(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    let result = match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            // Fast path: in-place for numbers
            let r = av + bv;
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(r);
                vm.stack.set_len(len - 1);
            }
            return ControlFlow::Continue;
        }
        // Optimized string concatenation - avoid format! overhead
        (Value::String(a_str), Value::String(b_str)) => {
            // Pre-allocate exact capacity
            let mut result = String::with_capacity(a_str.len() + b_str.len());
            result.push_str(a_str);
            result.push_str(b_str);
            Value::String(Arc::from(result))
        }
        (Value::String(a_str), b) => {
            // Write directly to buffer - no intermediate allocation!
            use std::fmt::Write;
            let mut result = String::with_capacity(a_str.len() + 32); // estimate for non-string
            result.push_str(a_str);
            let _ = write!(result, "{}", b);
            Value::String(Arc::from(result))
        }
        (a, Value::String(b_str)) => {
            // Write directly to buffer - no intermediate allocation!
            use std::fmt::Write;
            let mut result = String::with_capacity(32 + b_str.len()); // estimate for non-string
            let _ = write!(result, "{}", a);
            result.push_str(b_str);
            Value::String(Arc::from(result))
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            return ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot add '{}' and '{}'", a_type, b_type),
            ));
        }
    };
    // Slow path for strings: use in-place mutation too
    unsafe { 
        *vm.stack.get_unchecked_mut(len - 2) = result;
        vm.stack.set_len(len - 1);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_sub(vm: &mut VM) -> ControlFlow {
    binary_num_op(vm, |a, b| a - b)
}

#[inline(always)]
fn op_mul(vm: &mut VM) -> ControlFlow {
    binary_num_op(vm, |a, b| a * b)
}

#[inline(always)]
fn op_div(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            if *bv == 0.0 {
                return ControlFlow::Error(vm.create_error(ErrorKind::DivisionByZero, "Division by zero"));
            }
            let result = av / bv;
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot divide '{}' by '{}'", a_type, b_type),
            ))
        }
    }
}

#[inline(always)]
fn op_mod(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            if *bv == 0.0 {
                return ControlFlow::Error(vm.create_error(ErrorKind::DivisionByZero, "Modulo by zero"));
            }
            let result = av % bv;
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot modulo '{}' by '{}'", a_type, b_type),
            ))
        }
    }
}

#[inline(always)]
fn op_negate(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len == 0 { return ControlFlow::Continue; }
    let v = unsafe { vm.stack.get_unchecked(len - 1) };
    match v {
        Value::Number(n) => {
            // In-place mutation
            unsafe { *vm.stack.get_unchecked_mut(len - 1) = Value::Number(-n); }
            ControlFlow::Continue
        }
        _ => {
            let v_type = v.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot negate '{}'", v_type),
            ))
        }
    }
}

#[inline(always)]
fn op_equal(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    let result = a == b;
    unsafe { 
        *vm.stack.get_unchecked_mut(len - 2) = Value::Boolean(result);
        vm.stack.set_len(len - 1);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_not_equal(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    let result = a != b;
    unsafe { 
        *vm.stack.get_unchecked_mut(len - 2) = Value::Boolean(result);
        vm.stack.set_len(len - 1);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_less(vm: &mut VM) -> ControlFlow {
    comparison_op(vm, |a, b| a < b)
}

#[inline(always)]
fn op_less_equal(vm: &mut VM) -> ControlFlow {
    comparison_op(vm, |a, b| a <= b)
}

#[inline(always)]
fn op_greater(vm: &mut VM) -> ControlFlow {
    comparison_op(vm, |a, b| a > b)
}

#[inline(always)]
fn op_greater_equal(vm: &mut VM) -> ControlFlow {
    comparison_op(vm, |a, b| a >= b)
}

#[inline(always)]
fn op_not(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len > 0 {
        let is_truthy = unsafe { vm.stack.get_unchecked(len - 1).is_truthy() };
        vm.stack.truncate(len - 1);
        vm.stack.push(Value::Boolean(!is_truthy));
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_jump(vm: &mut VM) -> ControlFlow {
    let offset = vm.read_u16() as usize;
    vm.current_frame_mut().ip += offset;
    ControlFlow::Continue
}

#[inline(always)]
fn op_jump_if_false(vm: &mut VM) -> ControlFlow {
    let offset = vm.read_u16() as usize;
    let len = vm.stack.len();
    if len > 0 {
        let is_truthy = unsafe { vm.stack.get_unchecked(len - 1).is_truthy() };
        if !is_truthy {
            vm.current_frame_mut().ip += offset;
        }
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_jump_if_true(vm: &mut VM) -> ControlFlow {
    let offset = vm.read_u16() as usize;
    let len = vm.stack.len();
    if len > 0 {
        let is_truthy = unsafe { vm.stack.get_unchecked(len - 1).is_truthy() };
        if is_truthy {
            vm.current_frame_mut().ip += offset;
        }
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_jump_if_not_null(vm: &mut VM) -> ControlFlow {
    let offset = vm.read_u16() as usize;
    let len = vm.stack.len();
    if len > 0 {
        let is_null = unsafe { vm.stack.get_unchecked(len - 1).is_null() };
        if !is_null {
            vm.current_frame_mut().ip += offset;
        }
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_loop(vm: &mut VM) -> ControlFlow {
    let offset = vm.read_u16() as usize;
    vm.current_frame_mut().ip -= offset;
    ControlFlow::Continue
}

#[inline(always)]
fn op_call(vm: &mut VM) -> ControlFlow {
    let arg_count = vm.read_u16() as usize;
    match vm.expand_spread_args(arg_count) {
        Ok(actual_count) => match vm.call_value(actual_count) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

#[inline(always)]
fn op_return(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    let result = if len > 0 {
        let r = unsafe { vm.stack.get_unchecked(len - 1).clone() };
        vm.stack.truncate(len - 1);
        r
    } else {
        Value::Null
    };
    let returning_frame_index = vm.frames.len() - 1;
    let frame = unsafe { vm.frames.pop().unwrap_unchecked() };

    // Restore saved globals if this frame had them (module function call)
    if let Some(saved_globals) = frame.saved_globals {
        vm.globals = saved_globals;
    }

    if !frame.function.file.is_empty() {
        crate::pop_script_dir();
    }

    while let Some(handler) = vm.exception_handlers.last() {
        if handler.frame_index >= returning_frame_index {
            vm.exception_handlers.pop();
        } else {
            break;
        }
    }

    vm.close_upvalues(frame.slots_start);

    if vm.frames.is_empty() {
        vm.stack.truncate(frame.slots_start);
        vm.stack.push(result.clone());
        return ControlFlow::Return(result);
    }

    vm.stack.truncate(frame.slots_start);

    if let Some(instance) = frame.init_instance {
        vm.stack.push(instance);
    } else {
        vm.stack.push(result);
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_closure(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let constant = vm.current_frame().function.chunk.constants[idx].clone();
    if let Constant::Function(ref func_const) = constant {
        let mut function = Function::from_constant(func_const);
        for upvalue_info in &func_const.upvalues {
            let upvalue = if upvalue_info.is_local {
                let slots_start = vm.current_frame().slots_start;
                let location = slots_start + upvalue_info.index as usize;
                vm.capture_upvalue(location)
            } else {
                vm.current_frame().function.upvalues[upvalue_info.index as usize].clone()
            };
            function.upvalues.push(upvalue);
        }
        vm.stack.push(Value::Function(Arc::new(function)));
    }
    ControlFlow::Continue
}

#[inline(always)]
fn op_class(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => {
            vm.stack.push(Value::Class(Arc::new(Class::new(&name))));
            ControlFlow::Continue
        }
        Err(e) => ControlFlow::Error(e),
    }
}

#[inline(always)]
fn op_method(vm: &mut VM) -> ControlFlow {
    op_method_impl(vm, false)
}

#[inline(always)]
fn op_static_method(vm: &mut VM) -> ControlFlow {
    op_method_impl(vm, true)
}

#[inline(always)]
fn op_method_impl(vm: &mut VM, is_static: bool) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let constant = vm.current_frame().function.chunk.constants[idx].clone();
    if let Constant::Function(ref func_const) = constant {
        let function = Arc::new(Function::from_constant(func_const));
        if let Some(Value::Class(class)) = vm.stack.last().cloned() {
            let class_mut = Arc::as_ptr(&class) as *mut Class;
            unsafe {
                if is_static {
                    (*class_mut).user_static_methods.insert(func_const.name.clone(), Value::Function(function));
                } else {
                    (*class_mut).methods.insert(func_const.name.clone(), Value::Function(function));
                }
            }
        }
    }
    ControlFlow::Continue
}

fn op_get_property(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => match vm.handle_get_property(&name) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_set_property(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => match vm.handle_set_property(&name) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_get_self(vm: &mut VM) -> ControlFlow {
    let slots_start = vm.current_frame().slots_start;
    let value = vm.stack[slots_start].clone();
    vm.stack.push(value);
    ControlFlow::Continue
}

fn op_invoke(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let arg_count = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => match vm.invoke(&name, arg_count) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_build_array(vm: &mut VM) -> ControlFlow {
    let count = vm.read_u16() as usize;
    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        elements.push(vm.stack.pop().unwrap_or(Value::Null));
    }
    elements.reverse();
    let arr = Arc::new(Mutex::new(elements));
    vm.track_array(&arr);
    vm.stack.push(Value::Array(arr));
    ControlFlow::Continue
}

fn op_get_index(vm: &mut VM) -> ControlFlow {
    match vm.handle_get_index() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_set_index(vm: &mut VM) -> ControlFlow {
    match vm.handle_set_index() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_build_dict(vm: &mut VM) -> ControlFlow {
    match vm.handle_build_dict() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_build_namespace(vm: &mut VM) -> ControlFlow {
    match vm.handle_build_namespace() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_build_enum(vm: &mut VM) -> ControlFlow {
    match vm.handle_build_enum() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_inherit(vm: &mut VM) -> ControlFlow {
    let _ = vm.read_u16();
    match vm.handle_inherit() {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_get_super(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(name) => match vm.handle_get_super(&name) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_import(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    match vm.read_string_constant(idx) {
        Ok(path) => match vm.handle_import(&path) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_import_as(vm: &mut VM) -> ControlFlow {
    let path_idx = vm.read_u16() as usize;
    let alias_idx = vm.read_u16() as usize;
    match (vm.read_string_constant(path_idx), vm.read_string_constant(alias_idx)) {
        (Ok(path), Ok(alias)) => match vm.handle_import_as(&path, &alias) {
            Ok(()) => ControlFlow::Continue,
            Err(e) => ControlFlow::Error(e),
        },
        (Err(e), _) | (_, Err(e)) => ControlFlow::Error(e),
    }
}

fn op_get_upvalue(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let upvalue = vm.current_frame().function.upvalues[idx].clone();
    let upvalue_ref = upvalue.lock();
    let value = if let Some(ref closed) = upvalue_ref.closed {
        (**closed).clone()
    } else {
        vm.stack[upvalue_ref.location].clone()
    };
    drop(upvalue_ref);
    vm.stack.push(value);
    ControlFlow::Continue
}

fn op_set_upvalue(vm: &mut VM) -> ControlFlow {
    let idx = vm.read_u16() as usize;
    let value = vm.stack.last().cloned().unwrap_or(Value::Null);
    let upvalue = vm.current_frame().function.upvalues[idx].clone();
    let mut upvalue_ref = upvalue.lock();
    if upvalue_ref.closed.is_some() {
        upvalue_ref.closed = Some(Box::new(value));
    } else {
        let location = upvalue_ref.location;
        drop(upvalue_ref);
        vm.stack[location] = value;
    }
    ControlFlow::Continue
}

fn op_close_upvalue(vm: &mut VM) -> ControlFlow {
    let stack_top = vm.stack.len() - 1;
    vm.close_upvalues(stack_top);
    vm.stack.pop();
    ControlFlow::Continue
}

fn op_try_start(vm: &mut VM) -> ControlFlow {
    let catch_offset = vm.read_u16() as usize;
    let catch_ip = vm.current_frame().ip + catch_offset;
    vm.exception_handlers.push(ExceptionHandler {
        frame_index: vm.frames.len() - 1,
        stack_size: vm.stack.len(),
        catch_ip,
    });
    ControlFlow::Continue
}

fn op_try_end(vm: &mut VM) -> ControlFlow {
    vm.exception_handlers.pop();
    ControlFlow::Continue
}

fn op_throw(vm: &mut VM) -> ControlFlow {
    let exception_value = vm.stack.pop().unwrap_or(Value::Null);
    if let Some(handler) = vm.exception_handlers.pop() {
        while vm.frames.len() > handler.frame_index + 1 {
            vm.frames.pop();
        }
        while vm.stack.len() > handler.stack_size {
            vm.stack.pop();
        }
        vm.stack.push(exception_value);
        vm.current_frame_mut().ip = handler.catch_ip;
        ControlFlow::Continue
    } else {
        let msg = match &exception_value {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        };
        ControlFlow::Error(vm.create_error(ErrorKind::RuntimeError, &format!("Uncaught exception: {}", msg)))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn op_await(vm: &mut VM) -> ControlFlow {
    let value = vm.stack.pop().unwrap_or(Value::Null);
    match value {
        Value::Future(future_arc) => {
            let mut guard = future_arc.lock();
            if let Some(future) = guard.take() {
                drop(guard);
                return ControlFlow::Suspend(future.receiver);
            } else {
                return ControlFlow::Error(vm.create_error(ErrorKind::RuntimeError, "Future has already been consumed"));
            }
        }
        other => {
            vm.stack.push(other);
            ControlFlow::Continue
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn op_await(vm: &mut VM) -> ControlFlow {
    let value = vm.stack.pop().unwrap_or(Value::Null);
    match value {
        Value::Future(_) => {
            ControlFlow::Error(vm.create_error(ErrorKind::RuntimeError, "async/await is not supported in WASM playground"))
        }
        other => {
            vm.stack.push(other);
            ControlFlow::Continue
        }
    }
}

fn op_spread_array(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len > 0 {
        let value = unsafe { vm.stack.get_unchecked(len - 1).clone() };
        vm.stack.truncate(len - 1);
        vm.stack.push(Value::SpreadMarker(Box::new(value)));
    }
    ControlFlow::Continue
}

fn op_bit_and(vm: &mut VM) -> ControlFlow {
    bitwise_op(vm, |a, b| a & b)
}

fn op_bit_or(vm: &mut VM) -> ControlFlow {
    bitwise_op(vm, |a, b| a | b)
}

fn op_bit_xor(vm: &mut VM) -> ControlFlow {
    bitwise_op(vm, |a, b| a ^ b)
}

fn op_bit_not(vm: &mut VM) -> ControlFlow {
    let len = vm.stack.len();
    if len == 0 { return ControlFlow::Continue; }
    let v = unsafe { vm.stack.get_unchecked(len - 1) };
    match v {
        Value::Number(n) => {
            let result = Value::Number((!(*n as i64)) as f64);
            vm.stack.truncate(len - 1);
            vm.stack.push(result);
            ControlFlow::Continue
        }
        _ => {
            let v_type = v.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot perform bitwise NOT on '{}'", v_type),
            ))
        }
    }
}

fn op_left_shift(vm: &mut VM) -> ControlFlow {
    shift_op(vm, |a, b| a << b)
}

fn op_right_shift(vm: &mut VM) -> ControlFlow {
    shift_op(vm, |a, b| a >> b)
}

fn op_build_range_inclusive(vm: &mut VM) -> ControlFlow {
    match vm.handle_build_range(true) {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_build_range_exclusive(vm: &mut VM) -> ControlFlow {
    match vm.handle_build_range(false) {
        Ok(()) => ControlFlow::Continue,
        Err(e) => ControlFlow::Error(e),
    }
}

fn op_nop(_vm: &mut VM) -> ControlFlow {
    ControlFlow::Continue
}

// ==================== Helper Functions ====================

#[inline(always)]
fn binary_num_op(vm: &mut VM, op: fn(f64, f64) -> f64) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            let result = op(*av, *bv);
            // In-place: write result to a's slot, then pop b
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot perform operation on '{}' and '{}'", a_type, b_type),
            ))
        }
    }
}

#[inline(always)]
fn comparison_op(vm: &mut VM, op: fn(f64, f64) -> bool) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            let result = op(*av, *bv);
            // In-place: write result to a's slot, then pop b
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Boolean(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        (Value::String(a), Value::String(b)) => {
            use std::cmp::Ordering;
            let result = match a.cmp(b) {
                Ordering::Less => op(-1.0, 0.0),
                Ordering::Equal => op(0.0, 0.0),
                Ordering::Greater => op(1.0, 0.0),
            };
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Boolean(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot compare '{}' and '{}'", a_type, b_type),
            ))
        }
    }
}

#[inline(always)]
fn bitwise_op(vm: &mut VM, op: fn(i64, i64) -> i64) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            let result = op(*av as i64, *bv as i64) as f64;
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot perform bitwise operation on '{}' and '{}'", a_type, b_type),
            ))
        }
    }
}

#[inline(always)]
fn shift_op(vm: &mut VM, op: fn(i64, u32) -> i64) -> ControlFlow {
    let len = vm.stack.len();
    if len < 2 { return ControlFlow::Continue; }
    let b = unsafe { vm.stack.get_unchecked(len - 1) };
    let a = unsafe { vm.stack.get_unchecked(len - 2) };
    match (a, b) {
        (Value::Number(av), Value::Number(bv)) => {
            let result = op(*av as i64, *bv as u32) as f64;
            unsafe { 
                *vm.stack.get_unchecked_mut(len - 2) = Value::Number(result);
                vm.stack.set_len(len - 1);
            }
            ControlFlow::Continue
        }
        _ => {
            let a_type = a.type_name();
            let b_type = b.type_name();
            ControlFlow::Error(vm.create_error(
                ErrorKind::TypeError,
                &format!("Cannot perform shift on '{}' and '{}'", a_type, b_type),
            ))
        }
    }
}

// ==================== ValueCaller Implementation ====================

#[cfg(not(target_arch = "wasm32"))]
impl ValueCaller for VM {
    fn call(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, String> {
        let frame_count_before = self.frames.len();
        self.push_fast(callee.clone()).map_err(|e| e.message)?;
        for arg in args.iter() {
            self.push_fast(arg.clone()).map_err(|e| e.message)?;
        }
        self.call_value(args.len()).map_err(|e| e.message)?;

        loop {
            if self.frames.len() == frame_count_before {
                return Ok(self.stack.pop().unwrap_or(Value::Null));
            }
            match self.execute_one_threaded() {
                ControlFlow::Continue => continue,
                ControlFlow::Return(v) => return Ok(v),
                ControlFlow::Suspend(receiver) => {
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(receiver)
                    });
                    match result {
                        Ok(Ok(value)) => { self.stack.push(value); }
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err("Future was cancelled".to_string()),
                    }
                }
                ControlFlow::Error(e) => return Err(e.message),
            }
        }
    }

    fn get_globals(&self) -> FxHashMap<String, Value> {
        self.globals.read().clone()
    }

    fn get_shared_globals(&self) -> Arc<RwLock<FxHashMap<String, Value>>> {
        self.globals.clone()
    }
}

#[cfg(target_arch = "wasm32")]
impl ValueCaller for VM {
    fn call(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, String> {
        let frame_count_before = self.frames.len();
        self.push_fast(callee.clone()).map_err(|e| e.message)?;
        for arg in args.iter() {
            self.push_fast(arg.clone()).map_err(|e| e.message)?;
        }
        self.call_value(args.len()).map_err(|e| e.message)?;

        loop {
            if self.frames.len() == frame_count_before {
                return Ok(self.stack.pop().unwrap_or(Value::Null));
            }
            match self.execute_one_threaded() {
                ControlFlow::Continue => continue,
                ControlFlow::Return(v) => return Ok(v),
                ControlFlow::Error(e) => return Err(e.message),
            }
        }
    }

    fn get_globals(&self) -> FxHashMap<String, Value> {
        self.globals.read().clone()
    }

    fn get_shared_globals(&self) -> Arc<RwLock<FxHashMap<String, Value>>> {
        self.globals.clone()
    }
}

// ==================== VM Implementation ====================

impl VM {
    pub fn new() -> Self {
        let globals = builtins::create_builtin_classes();
        Self {
            stack: Vec::with_capacity(STACK_INIT),
            frames: Vec::with_capacity(FRAMES_INIT),
            globals: Arc::new(RwLock::new(globals)),
            file: String::new(),
            source: String::new(),
            exception_handlers: SmallVec::new(),
            open_upvalues: Vec::new(),
            gc: GcHeap::new(),
            gc_counter: 0,
            gc_stats_enabled: false,
            pending_module_workspace: None,
            args: Vec::new(),
            namespace_context: Vec::new(),
        }
    }

    pub fn new_with_shared_globals(globals: Arc<RwLock<FxHashMap<String, Value>>>) -> Self {
        Self {
            stack: Vec::with_capacity(STACK_INIT),
            frames: Vec::with_capacity(FRAMES_INIT),
            globals,
            file: String::new(),
            source: String::new(),
            exception_handlers: SmallVec::new(),
            open_upvalues: Vec::new(),
            gc: GcHeap::new(),
            gc_counter: 0,
            gc_stats_enabled: false,
            pending_module_workspace: None,
            args: Vec::new(),
            namespace_context: Vec::new(),
        }
    }

    /// Reset VM state for reuse (avoids reallocation)
    #[inline]
    pub fn reset(&mut self) {
        self.stack.clear();
        self.frames.clear();
        self.exception_handlers.clear();
        self.open_upvalues.clear();
        self.gc_counter = 0;
    }

    pub fn set_args(&mut self, args: Vec<String>) { self.args = args; }
    pub fn set_gc_stats_enabled(&mut self, enabled: bool) { self.gc_stats_enabled = enabled; }
    pub fn get_globals(&self) -> FxHashMap<String, Value> { self.globals.read().clone() }
    pub fn get_shared_globals(&self) -> Arc<RwLock<FxHashMap<String, Value>>> { self.globals.clone() }
    pub fn gc_stats(&self) -> super::gc::GcStats { self.gc.get_stats() }

    #[inline(always)]
    fn maybe_collect_garbage(&mut self) {
        self.gc_counter += 1;
        if self.gc_counter >= 10000 {
            self.gc_counter = 0;
            if self.gc.should_collect() {
                self.collect_garbage();
            }
        }
    }

    pub fn collect_garbage(&mut self) {
        let mut roots: Vec<&Value> = Vec::new();
        for value in &self.stack { roots.push(value); }
        let globals_guard = self.globals.read();
        for value in globals_guard.values() { roots.push(value); }
        self.gc.collect(roots);
        if self.gc_stats_enabled {
            let stats = self.gc.get_stats();
            eprintln!("[GC] collection #{}: tracked={}, cycles_broken={}", stats.collections, stats.tracked_count, stats.cycles_broken);
        }
    }

    fn track_array(&mut self, arr: &Arc<Mutex<Vec<Value>>>) { 
        self.gc.track_array(arr); 
        self.maybe_collect_garbage();
    }
    fn track_dict(&mut self, dict: &Arc<Mutex<FxHashMap<String, Value>>>) { 
        self.gc.track_dict(dict); 
        self.maybe_collect_garbage();
    }
    fn track_instance(&mut self, inst: &Arc<Mutex<Instance>>) { 
        self.gc.track_instance(inst); 
        self.maybe_collect_garbage();
    }

    // ==================== Stack Helpers ====================

    /// Peek at top value without removing
    #[inline(always)]
    fn peek(&self) -> Option<&Value> {
        self.stack.last()
    }

    /// Peek at value at offset from top (0 = top, 1 = second from top, etc.) - UNSAFE
    #[inline(always)]
    fn peek_unchecked(&self, offset: usize) -> &Value {
        unsafe { self.stack.get_unchecked(self.stack.len() - 1 - offset) }
    }

    /// Pop value from stack - UNSAFE, assumes stack is not empty
    #[inline(always)]
    fn pop_fast(&mut self) -> Value {
        unsafe { self.stack.pop().unwrap_unchecked() }
    }

    // ==================== Private Access Checking ====================

    /// Check if identifier is private (starts with _)
    #[inline(always)]
    fn is_private(name: &str) -> bool {
        name.starts_with('_') && name.len() > 1
    }

    /// Check if we're currently inside the given class (or any parent class in the call stack)
    /// Also checks the function's compile-time class_context for closures defined inside methods
    #[inline(always)]
    fn is_in_class(&self, class_name: &str) -> bool {
        // Check if any frame in the call stack has the matching class context (runtime)
        for frame in self.frames.iter().rev() {
            if let Some(ref ctx) = frame.class_context {
                if ctx == class_name {
                    return true;
                }
            }
        }
        
        // Check if current function was defined in this class (compile-time context)
        // This handles closures/lambdas defined inside class methods
        if let Some(frame) = self.frames.last() {
            if let Some(ref class_ctx) = frame.function.class_context {
                if class_ctx == class_name {
                    return true;
                }
            }
        }
        
        false
    }

    /// Check if we're currently inside the given namespace
    /// Checks both the runtime namespace context stack AND the current function's
    /// compile-time namespace context (for functions defined inside namespaces)
    #[inline(always)]
    fn is_in_namespace(&self, namespace_name: &str) -> bool {
        // Check runtime namespace context stack
        if self.namespace_context.last().map(|s| s == namespace_name).unwrap_or(false) {
            return true;
        }
        
        // Check if current function was defined in this namespace
        // This handles the case when a namespace function is called from outside
        if let Some(frame) = self.frames.last() {
            if let Some(ref ns_ctx) = frame.function.namespace_context {
                // The ns_ctx is fully qualified (e.g., "Spark.Template")
                // The namespace_name might be just "Template" (runtime name)
                // Check various matching conditions:
                // 1. Exact match (e.g., "Template" == "Template")
                // 2. Ends with the namespace (e.g., "Spark.Template" ends with ".Template" or is "Template")
                // 3. Starts with namespace (for child access, e.g., "Template.Inner" can access "Template")
                if ns_ctx == namespace_name 
                    || ns_ctx.ends_with(&format!(".{}", namespace_name))
                    || ns_ctx.starts_with(&format!("{}.", namespace_name)) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Main entry point - async execution with event loop (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run(&mut self, chunk: Chunk, file: &str, source: &str) -> SaldResult<Value> {
        self.file = file.to_string();
        self.source = source.to_string();
        crate::push_script_dir(file);

        let mut main_function = Function::new("<script>", 0, chunk);
        main_function.file = file.to_string();
        let main_function = Arc::new(main_function);

        let slots_start = self.stack.len();
        self.stack.push(Value::Null);
        self.frames.push(CallFrame::new(main_function, slots_start));

        let result = self.run_event_loop().await;
        crate::pop_script_dir();
        result
    }

    /// Main entry point - synchronous execution (WASM)
    #[cfg(target_arch = "wasm32")]
    pub fn run(&mut self, chunk: &Chunk) -> SaldResult<Value> {
        let mut main_function = Function::new("<script>", 0, chunk.clone());
        let main_function = Arc::new(main_function);

        let slots_start = self.stack.len();
        self.stack.push(Value::Null);
        self.frames.push(CallFrame::new(main_function, slots_start));

        self.execute_until_complete()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run_handler(&mut self, handler: Value, arg: Value, script_dir: Option<&str>) -> SaldResult<Value> {
        if let Some(dir) = script_dir {
            crate::push_script_dir(dir);
        }
        self.stack.push(Value::Null);
        self.stack.push(handler); // handler consumed, no clone needed
        self.stack.push(arg);
        self.call_value(1)?;
        let result = self.run_event_loop().await;
        if script_dir.is_some() { crate::pop_script_dir(); }
        result
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn run_event_loop(&mut self) -> SaldResult<Value> {
        loop {
            match self.execute_until_suspend() {
                ExecutionResult::Completed(value) => return Ok(value),
                ExecutionResult::Suspended { receiver } => {
                    match receiver.await {
                        Ok(Ok(value)) => { self.stack.push(value); }
                        Ok(Err(e)) => { self.handle_native_error(e)?; }
                        Err(_) => return Err(self.create_error(ErrorKind::RuntimeError, "Future was cancelled")),
                    }
                }
                ExecutionResult::Error(e) => return Err(e),
            }
        }
    }

    /// Execute with threaded dispatch until suspend or complete (native)
    #[cfg(not(target_arch = "wasm32"))]
    fn execute_until_suspend(&mut self) -> ExecutionResult {
        loop {
            if self.frames.is_empty() {
                return ExecutionResult::Completed(self.stack.pop().unwrap_or(Value::Null));
            }
            match self.execute_one_threaded() {
                ControlFlow::Continue => continue,
                ControlFlow::Return(v) => return ExecutionResult::Completed(v),
                ControlFlow::Suspend(receiver) => return ExecutionResult::Suspended { receiver },
                ControlFlow::Error(e) => {
                    if !self.exception_handlers.is_empty() {
                        if let Err(handler_err) = self.handle_native_error(e.message.clone()) {
                            return ExecutionResult::Error(handler_err);
                        }
                        continue;
                    }
                    return ExecutionResult::Error(e);
                }
            }
        }
    }

    /// Execute until complete (WASM - no suspend support)
    #[cfg(target_arch = "wasm32")]
    fn execute_until_complete(&mut self) -> SaldResult<Value> {
        loop {
            if self.frames.is_empty() {
                return Ok(self.stack.pop().unwrap_or(Value::Null));
            }
            match self.execute_one_threaded() {
                ControlFlow::Continue => continue,
                ControlFlow::Return(v) => return Ok(v),
                ControlFlow::Error(e) => {
                    if !self.exception_handlers.is_empty() {
                        self.handle_native_error(e.message.clone())?;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Core threaded dispatch - single instruction execution
    #[inline(always)]
    fn execute_one_threaded(&mut self) -> ControlFlow {
        self.maybe_collect_garbage();

        let op = self.read_byte();

        // Single dispatch table lookup - no branching
        if op < 68 {
            unsafe { DISPATCH.get_unchecked(op as usize)(self) }
        } else {
            ControlFlow::Error(self.create_error(ErrorKind::RuntimeError, &format!("Unknown opcode: {}", op)))
        }
    }

    // ==================== Stack Operations ====================

    #[inline(always)]
    fn push_fast(&mut self, value: Value) -> SaldResult<()> {
        if self.stack.len() >= STACK_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow"));
        }
        self.stack.push(value);
        Ok(())
    }

    #[inline(always)]
    fn current_frame(&self) -> &CallFrame {
        unsafe { self.frames.last().unwrap_unchecked() }
    }

    #[inline(always)]
    fn current_frame_mut(&mut self) -> &mut CallFrame {
        unsafe { self.frames.last_mut().unwrap_unchecked() }
    }

    #[inline(always)]
    fn read_byte(&mut self) -> u8 {
        self.current_frame_mut().read_byte()
    }

    #[inline(always)]
    fn read_u16(&mut self) -> u16 {
        self.current_frame_mut().read_u16()
    }

    fn read_constant(&self, idx: usize) -> Value {
        let constant = &self.current_frame().function.chunk.constants[idx];
        match constant {
            Constant::Number(n) => Value::Number(*n),
            Constant::String(s) => Value::String(s.clone()),
            Constant::Function(f) => Value::Function(Arc::new(Function::from_constant(f))),
            Constant::Class(c) => Value::Class(Arc::new(Class::new(&c.name))),
        }
    }

    fn read_string_constant(&self, idx: usize) -> SaldResult<String> {
        match &self.current_frame().function.chunk.constants[idx] {
            Constant::String(s) => Ok(s.to_string()),
            _ => Err(self.create_error(ErrorKind::TypeError, "Expected string constant")),
        }
    }

    fn capture_upvalue(&mut self, location: usize) -> Arc<Mutex<UpvalueObj>> {
        for upvalue in &self.open_upvalues {
            if upvalue.lock().location == location {
                return upvalue.clone();
            }
        }
        let upvalue = Arc::new(Mutex::new(UpvalueObj::new(location)));
        self.open_upvalues.push(upvalue.clone());
        upvalue
    }

    fn close_upvalues(&mut self, last: usize) {
        self.open_upvalues.retain(|upvalue| {
            let location = upvalue.lock().location;
            if location >= last {
                let value = self.stack.get(location).cloned().unwrap_or(Value::Null);
                upvalue.lock().closed = Some(Box::new(value));
                false
            } else {
                true
            }
        });
    }

    fn expand_spread_args(&mut self, arg_count: usize) -> SaldResult<usize> {
        if arg_count == 0 { return Ok(0); }
        let stack_len = self.stack.len();
        let args_start = stack_len - arg_count;
        let mut has_spread = false;
        for i in args_start..stack_len {
            if matches!(&self.stack[i], Value::SpreadMarker(_)) { has_spread = true; break; }
        }
        if !has_spread { return Ok(arg_count); }

        let args: Vec<Value> = self.stack[args_start..stack_len].to_vec();
        let mut expanded_args = Vec::new();
        for arg in args {
            match arg {
                Value::SpreadMarker(boxed_value) => {
                    if let Value::Array(arr) = *boxed_value {
                        for elem in arr.lock().iter() { expanded_args.push(elem.clone()); }
                    } else {
                        expanded_args.push(*boxed_value);
                    }
                }
                other => expanded_args.push(other),
            }
        }
        let new_count = expanded_args.len();
        let _ = self.stack.splice(args_start.., expanded_args);
        Ok(new_count)
    }

    // ==================== Call Methods ====================

    fn call_value(&mut self, arg_count: usize) -> SaldResult<()> {
        let callee_idx = self.stack.len() - arg_count - 1;
        let callee = unsafe { self.stack.get_unchecked(callee_idx) };
        
        // Fast path: if callee is a function, handle directly without cloning for type check
        match callee {
            Value::Function(function) => {
                // Check if this is a recursive call to the same function (common in fib)
                // If so, we can reuse the Arc from current frame instead of cloning from stack
                let func_to_call = if !self.frames.is_empty() {
                    let current_func = &self.current_frame().function;
                    if Arc::ptr_eq(current_func, function) {
                        // Recursive call - reuse the Arc from current frame
                        current_func.clone()
                    } else {
                        function.clone()
                    }
                } else {
                    function.clone()
                };
                self.call_function(func_to_call, arg_count)
            }
            Value::Class(class) => {
                let class = class.clone();
                self.call_class(class, arg_count)
            }
            Value::NativeFunction { func, .. } => {
                let func = *func;
                self.call_native(func, arg_count)
            }
            Value::InstanceMethod { receiver, method, .. } => {
                let receiver = (**receiver).clone();
                let method = *method;
                self.call_instance_method(receiver, method, arg_count)
            }
            Value::BoundMethod { receiver, method } => {
                let receiver = (**receiver).clone();
                let method = method.clone();
                self.call_bound_method(receiver, method, arg_count)
            }
            _ => Err(self.create_error(ErrorKind::TypeError, &format!("'{}' is not callable", callee.type_name()))),
        }
    }

    #[inline(always)]
    fn call_function(&mut self, function: Arc<Function>, arg_count: usize) -> SaldResult<()> {
        // Fast path: non-variadic function with exact or under arity (most common case)
        if !function.is_variadic {
            let required_arity = function.arity.saturating_sub(function.default_count);
            if arg_count < required_arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at least {} arguments but got {}", required_arity, arg_count)));
            }
            if arg_count > function.arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at most {} arguments but got {}", function.arity, arg_count)));
            }
            
            // Push null for missing default args
            let missing = function.arity - arg_count;
            for _ in 0..missing { self.stack.push(Value::Null); }
            
            // FRAMES_MAX check inlined for fast path
            if self.frames.len() >= FRAMES_MAX {
                return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
            }
            
            let slots_start = self.stack.len() - function.arity - 1;
            if !function.file.is_empty() { crate::push_script_dir(&function.file); }
            self.frames.push(CallFrame::new(function, slots_start));
            return Ok(());
        }
        
        // Slow path: variadic functions
        let min_arity = function.arity.saturating_sub(1);
        if arg_count < min_arity {
            return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at least {} arguments but got {}", min_arity, arg_count)));
        }
        let variadic_count = arg_count - min_arity;
        let mut variadic_args = Vec::with_capacity(variadic_count);
        for _ in 0..variadic_count { variadic_args.push(self.stack.pop().unwrap_or(Value::Null)); }
        variadic_args.reverse();
        self.stack.push(Value::Array(Arc::new(Mutex::new(variadic_args))));
        let effective_arg_count = min_arity + 1;
        if self.frames.len() >= FRAMES_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
        }
        let slots_start = self.stack.len() - effective_arg_count - 1;
        if !function.file.is_empty() { crate::push_script_dir(&function.file); }
        self.frames.push(CallFrame::new(function, slots_start));
        Ok(())
    }

    /// Call a user-defined function with class context for private access checking
    fn call_function_with_class(&mut self, function: Arc<Function>, arg_count: usize, class_name: String) -> SaldResult<()> {
        // Fast path: non-variadic functions
        if !function.is_variadic {
            let required_arity = function.arity.saturating_sub(function.default_count);
            if arg_count < required_arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at least {} arguments but got {}", required_arity, arg_count)));
            }
            if arg_count > function.arity {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at most {} arguments but got {}", function.arity, arg_count)));
            }
            
            // Push null for missing default args
            let missing = function.arity - arg_count;
            for _ in 0..missing { self.stack.push(Value::Null); }
            
            if self.frames.len() >= FRAMES_MAX {
                return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
            }
            
            let slots_start = self.stack.len() - function.arity - 1;
            if !function.file.is_empty() { crate::push_script_dir(&function.file); }
            self.frames.push(CallFrame::new_with_class(function, slots_start, class_name));
            return Ok(());
        }
        
        // Slow path: variadic functions
        let min_arity = function.arity.saturating_sub(1);
        if arg_count < min_arity {
            return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at least {} arguments but got {}", min_arity, arg_count)));
        }
        let variadic_count = arg_count - min_arity;
        let mut variadic_args = Vec::with_capacity(variadic_count);
        for _ in 0..variadic_count { variadic_args.push(self.stack.pop().unwrap_or(Value::Null)); }
        variadic_args.reverse();
        self.stack.push(Value::Array(Arc::new(Mutex::new(variadic_args))));
        let effective_arg_count = min_arity + 1;
        if self.frames.len() >= FRAMES_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
        }
        let slots_start = self.stack.len() - effective_arg_count - 1;
        if !function.file.is_empty() { crate::push_script_dir(&function.file); }
        self.frames.push(CallFrame::new_with_class(function, slots_start, class_name));
        Ok(())
    }
    fn call_class(&mut self, class: Arc<Class>, arg_count: usize) -> SaldResult<()> {
        if let Some(constructor) = class.constructor {
            let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
            self.stack.pop();
            match constructor(&args) {
                Ok(result) => { self.stack.push(result); Ok(()) }
                Err(e) => { self.handle_native_error(e)?; Ok(()) }
            }
        } else {
            let instance = Arc::new(Mutex::new(Instance::new(class.clone())));
            self.track_instance(&instance);
            let instance_value = Value::Instance(instance.clone());
            let stack_idx = self.stack.len() - arg_count - 1;
            self.stack[stack_idx] = instance_value.clone();
            if let Some(init) = class.methods.get("init") {
                if let Value::Function(init_fn) = init {
                    // Use class context for private access in constructor
                    self.call_function_init_with_class(init_fn.clone(), arg_count, instance_value, class.name.clone())?;
                }
            } else if arg_count > 0 {
                return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected 0 arguments but got {}", arg_count)));
            }
            Ok(())
        }
    }

    fn call_native(&mut self, func: fn(&[Value]) -> Result<Value, String>, arg_count: usize) -> SaldResult<()> {
        let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
        self.stack.pop();
        match func(&args) {
            Ok(result) => { self.stack.push(result); Ok(()) }
            Err(e) => { self.handle_native_error(e)?; Ok(()) }
        }
    }

    fn call_instance_method(&mut self, receiver: Value, method: fn(&Value, &[Value]) -> Result<Value, String>, arg_count: usize) -> SaldResult<()> {
        let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
        self.stack.pop();
        match method(&receiver, &args) {
            Ok(result) => { self.stack.push(result); Ok(()) }
            Err(e) => { self.handle_native_error(e)?; Ok(()) }
        }
    }

    fn call_bound_method(&mut self, receiver: Value, method: Arc<Function>, arg_count: usize) -> SaldResult<()> {
        let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
        self.stack.pop();
        self.stack.push(receiver);
        for arg in args { self.stack.push(arg); }
        self.call_function(method, arg_count)
    }

    /// Call init constructor with class context for private access checking
    fn call_function_init_with_class(&mut self, function: Arc<Function>, arg_count: usize, instance: Value, class_name: String) -> SaldResult<()> {
        let required_arity = function.arity.saturating_sub(function.default_count);
        if arg_count < required_arity {
            return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at least {} arguments but got {}", required_arity, arg_count)));
        }
        if arg_count > function.arity {
            return Err(self.create_error(ErrorKind::ArgumentError, &format!("Expected at most {} arguments but got {}", function.arity, arg_count)));
        }
        for _ in 0..(function.arity - arg_count) { self.stack.push(Value::Null); }
        if self.frames.len() >= FRAMES_MAX {
            return Err(self.create_error(ErrorKind::RuntimeError, "Stack overflow (too many call frames)"));
        }
        let slots_start = self.stack.len() - function.arity - 1;
        if !function.file.is_empty() { crate::push_script_dir(&function.file); }
        self.frames.push(CallFrame::new_init_with_class(function, slots_start, instance, class_name));
        Ok(())
    }

    // ==================== Invoke Method ====================

    fn invoke(&mut self, name: &str, arg_count: usize) -> SaldResult<()> {
        let receiver = self.stack.get(self.stack.len() - arg_count - 1).cloned().unwrap_or(Value::Null);
        match receiver {
            Value::Instance(ref instance) => {
                let class = instance.lock().class.clone();
                
                // Check private access BEFORE calling the method
                if Self::is_private(name) && !self.is_in_class(&class.name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private method '{}' from outside class '{}'", name, class.name)
                    ));
                }
                
                if let Some(field) = instance.lock().fields.get(name).cloned() {
                    let stack_idx = self.stack.len() - arg_count - 1;
                    self.stack[stack_idx] = field;
                    return self.call_value(arg_count);
                }
                if let Some(method) = class.methods.get(name).cloned() {
                    if let Value::Function(func) = method {
                        // Use class context for private access checking
                        return self.call_function_with_class(func, arg_count, class.name.clone());
                    }
                }
                if let Some(callable_method) = class.callable_native_instance_methods.get(name).copied() {
                    let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.stack.pop();
                    match callable_method(&receiver, &args, self) {
                        Ok(result) => { self.stack.push(result); return Ok(()); }
                        Err(e) => { self.handle_native_error(e)?; return Ok(()); }
                    }
                }
                if let Some(method) = class.native_instance_methods.get(name).copied() {
                    let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.stack.pop();
                    match method(&receiver, &args) {
                        Ok(result) => { self.stack.push(result); return Ok(()); }
                        Err(e) => { self.handle_native_error(e)?; return Ok(()); }
                    }
                }
                Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined method '{}' on instance", name)))
            }
            Value::Class(class) => {
                // Check private access for static methods
                if Self::is_private(name) && !self.is_in_class(&class.name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private static method '{}' from outside class '{}'", name, class.name)
                    ));
                }
                
                if let Some(method) = class.user_static_methods.get(name).cloned() {
                    let stack_idx = self.stack.len() - arg_count - 1;
                    self.stack[stack_idx] = Value::Null;
                    if let Value::Function(func) = method {
                        return self.call_function_with_class(func, arg_count, class.name.clone());
                    }
                }
                if let Some(native_fn) = class.native_static_methods.get(name).copied() {
                    let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.stack.pop();
                    match native_fn(&args) {
                        Ok(result) => { self.stack.push(result); return Ok(()); }
                        Err(e) => { self.handle_native_error(e)?; return Ok(()); }
                    }
                }
                Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined static method '{}'", name)))
            }
            Value::String(_) | Value::Number(_) | Value::Boolean(_) | Value::Null | Value::Array(_) | Value::Dictionary(_) => {
                let class_name = builtins::get_builtin_class_name(&receiver);
                let class = if let Some(Value::Class(c)) = self.globals.read().get(class_name).cloned() { c }
                else { return Err(self.create_error(ErrorKind::RuntimeError, &format!("Built-in class '{}' not found", class_name))); };
                if let Some(callable_method) = class.callable_native_instance_methods.get(name).copied() {
                    let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.stack.pop();
                    match callable_method(&receiver, &args, self) {
                        Ok(result) => { self.stack.push(result); return Ok(()); }
                        Err(e) => { self.handle_native_error(e)?; return Ok(()); }
                    }
                }
                if let Some(method) = class.native_instance_methods.get(name).copied() {
                    let args: Vec<Value> = self.stack.drain(self.stack.len() - arg_count..).collect();
                    self.stack.pop();
                    match method(&receiver, &args) {
                        Ok(result) => { self.stack.push(result); Ok(()) }
                        Err(e) => { self.handle_native_error(e)?; Ok(()) }
                    }
                } else {
                    Err(self.create_error(ErrorKind::AttributeError, &format!("'{}' has no method '{}'", class_name, name)))
                }
            }
            Value::Namespace { members, name: ns_name, module_globals } => {
                // Check private access for namespace members
                if Self::is_private(name) && !self.is_in_namespace(&ns_name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private member '{}' from outside namespace '{}'", name, ns_name)
                    ));
                }
                
                if let Some(m) = members.try_read() {
                    if let Some(member) = m.get(name).cloned() {
                        drop(m);
                        let stack_idx = self.stack.len() - arg_count - 1;
                        self.stack[stack_idx] = member.clone();
                        
                        // If this namespace has stored module globals, swap to them for function execution
                        if let Some(ref module_globals_arc) = module_globals {
                            if matches!(member, Value::Function(_)) {
                                let saved_globals = std::mem::replace(&mut self.globals, module_globals_arc.clone());
                                let result = self.call_value(arg_count);
                                // Store saved_globals in the just-pushed frame so Return can restore
                                if result.is_ok() && !self.frames.is_empty() {
                                    self.frames.last_mut().unwrap().saved_globals = Some(saved_globals);
                                }
                                return result;
                            }
                        }
                        
                        return self.call_value(arg_count);
                    }
                    return Err(self.create_error(ErrorKind::AttributeError, &format!("Namespace '{}' has no member '{}'", ns_name, name)));
                }
                Err(self.create_error(ErrorKind::RuntimeError, "Failed to lock namespace"))
            }
            _ => Err(self.create_error(ErrorKind::TypeError, &format!("Only instances have methods, got '{}'", receiver.type_name()))),
        }
    }

    // ==================== Property Handlers ====================

    fn handle_get_property(&mut self, name: &str) -> SaldResult<()> {
        let obj = self.stack.pop().unwrap_or(Value::Null);
        match obj {
            Value::Instance(instance) => {
                let inst_guard = instance.lock();
                let class_name = inst_guard.class.name.clone();
                
                // Check private access for fields and methods
                if Self::is_private(name) && !self.is_in_class(&class_name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private member '{}' from outside class '{}'", name, class_name)
                    ));
                }
                
                if let Some(value) = inst_guard.fields.get(name).cloned() {
                    drop(inst_guard);
                    self.stack.push(value);
                } else if let Some(method) = inst_guard.class.methods.get(name).cloned() {
                    drop(inst_guard);
                    self.stack.push(Value::Instance(instance.clone()));
                    self.stack.push(method);
                } else {
                    drop(inst_guard);
                    return Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined property '{}'", name)));
                }
            }
            Value::Class(class) => {
                // Check private access for static members
                if Self::is_private(name) && !self.is_in_class(&class.name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private member '{}' from outside class '{}'", name, class.name)
                    ));
                }
                
                if let Some(value) = class.native_static_fields.get(name) {
                    self.stack.push(value.clone());
                } else if let Some(method) = class.user_static_methods.get(name).cloned() {
                    self.stack.push(method);
                } else if let Some(method) = class.native_static_methods.get(name) {
                    self.stack.push(Value::NativeFunction { func: *method, class_name: class.name.clone() });
                } else {
                    return Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined property '{}' on class '{}'", name, class.name)));
                }
            }
            Value::String(_) | Value::Number(_) | Value::Boolean(_) | Value::Null | Value::Array(_) | Value::Dictionary(_) => {
                let class_name = builtins::get_builtin_class_name(&obj);
                let method_result = {
                    let globals_guard = self.globals.read();
                    if let Some(Value::Class(class)) = globals_guard.get(class_name) {
                        if let Some(method) = class.native_instance_methods.get(name) {
                            Ok((*method, name.to_string()))
                        } else { Err(format!("'{}' has no method '{}'", class_name, name)) }
                    } else { Err(format!("Built-in class '{}' not found", class_name)) }
                };
                match method_result {
                    Ok((method_fn, method_name)) => {
                        self.stack.push(Value::InstanceMethod { receiver: Box::new(obj), method: method_fn, method_name });
                    }
                    Err(msg) => return Err(self.create_error(ErrorKind::RuntimeError, &msg)),
                }
            }
            Value::Namespace { members, name: ns_name, .. } => {
                // Check private access for namespace members
                if Self::is_private(name) && !self.is_in_namespace(&ns_name) {
                    return Err(self.create_error(
                        ErrorKind::AccessError,
                        &format!("Cannot access private member '{}' from outside namespace '{}'", name, ns_name)
                    ));
                }
                
                {
                    let m = members.read();
                    if let Some(value) = m.get(name) {
                        self.stack.push(value.clone());
                    } else {
                        return Err(self.create_error(ErrorKind::AttributeError, &format!("Namespace '{}' has no member '{}'", ns_name, name)));
                    }
                }
            }
            Value::Enum { variants, name: enum_name } => {
                if let Some(value) = variants.get(name) {
                    self.stack.push(value.clone());
                } else {
                    return Err(self.create_error(ErrorKind::AttributeError, &format!("Enum '{}' has no variant '{}'", enum_name, name)));
                }
            }
            _ => return Err(self.create_error(ErrorKind::TypeError, &format!("Only instances have properties, got '{}'", obj.type_name()))),
        }
        Ok(())
    }

    fn handle_set_property(&mut self, name: &str) -> SaldResult<()> {
        let value = self.stack.pop().unwrap_or(Value::Null);
        let obj = self.stack.pop().unwrap_or(Value::Null);
        if let Value::Instance(instance) = obj {
            let class_name = instance.lock().class.name.clone();
            
            // Check private access
            if Self::is_private(name) && !self.is_in_class(&class_name) {
                return Err(self.create_error(
                    ErrorKind::AccessError,
                    &format!("Cannot access private member '{}' from outside class '{}'", name, class_name)
                ));
            }
            
            instance.lock().fields.insert(name.to_string(), value.clone());
            self.stack.push(value);
            Ok(())
        } else {
            Err(self.create_error(ErrorKind::TypeError, &format!("Only instances have properties, got '{}'", obj.type_name())))
        }
    }

    fn handle_get_index(&mut self) -> SaldResult<()> {
        let index = self.stack.pop().unwrap_or(Value::Null);
        let object = self.stack.pop().unwrap_or(Value::Null);
        match (&object, &index) {
            (Value::Array(arr), Value::Number(idx)) => {
                let idx = *idx as usize;
                let arr = arr.lock();
                if idx < arr.len() { self.stack.push(arr[idx].clone()); }
                else { return Err(self.create_error(ErrorKind::IndexError, &format!("Index {} out of bounds for array of length {}", idx, arr.len()))); }
            }
            (Value::String(s), Value::Number(idx)) => {
                let idx = *idx as usize;
                if idx < s.len() {
                    let ch = s.chars().nth(idx).unwrap_or(' ');
                    self.stack.push(Value::String(Arc::from(ch.to_string())));
                } else { return Err(self.create_error(ErrorKind::IndexError, &format!("Index {} out of bounds for string of length {}", idx, s.len()))); }
            }
            (Value::Dictionary(dict), Value::String(key)) => {
                let value = dict.lock().get(&**key).cloned().unwrap_or(Value::Null);
                self.stack.push(value);
            }
            _ => return Err(self.create_error(ErrorKind::TypeError, &format!("Cannot index '{}' with '{}'", object.type_name(), index.type_name()))),
        }
        Ok(())
    }

    fn handle_set_index(&mut self) -> SaldResult<()> {
        let value = self.stack.pop().unwrap_or(Value::Null);
        let index = self.stack.pop().unwrap_or(Value::Null);
        let object = self.stack.pop().unwrap_or(Value::Null);
        match (&object, &index) {
            (Value::Array(arr), Value::Number(idx)) => {
                let idx = *idx as usize;
                let mut arr = arr.lock();
                if idx < arr.len() { arr[idx] = value.clone(); self.stack.push(value); }
                else { return Err(self.create_error(ErrorKind::IndexError, &format!("Index {} out of bounds for array of length {}", idx, arr.len()))); }
            }
            (Value::Dictionary(dict), Value::String(key)) => {
                dict.lock().insert(key.to_string(), value.clone());
                self.stack.push(value);
            }
            _ => return Err(self.create_error(ErrorKind::TypeError, &format!("Cannot set index on '{}' with '{}'", object.type_name(), index.type_name()))),
        }
        Ok(())
    }

    fn handle_build_dict(&mut self) -> SaldResult<()> {
        let count = self.read_u16() as usize;
        let mut map: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(count, Default::default());
        let mut pairs = Vec::with_capacity(count);
        for _ in 0..count {
            let value = self.stack.pop().unwrap_or(Value::Null);
            let key = self.stack.pop().unwrap_or(Value::Null);
            pairs.push((key, value));
        }
        pairs.reverse();
        for (key, value) in pairs {
            if let (Value::Null, Value::SpreadMarker(spread_value)) = (&key, &value) {
                if let Value::Dictionary(dict) = spread_value.as_ref() {
                    for (k, v) in dict.lock().iter() { map.insert(k.clone(), v.clone()); }
                } else { return Err(self.create_error(ErrorKind::TypeError, "Can only spread dictionaries with **")); }
            } else {
                let key_str = match key {
                    Value::String(s) => s.to_string(),
                    _ => return Err(self.create_error(ErrorKind::TypeError, "Dictionary keys must be strings")),
                };
                map.insert(key_str, value);
            }
        }
        let dict = Arc::new(Mutex::new(map));
        self.track_dict(&dict);
        self.stack.push(Value::Dictionary(dict));
        Ok(())
    }

    fn handle_build_namespace(&mut self) -> SaldResult<()> {
        let count = self.read_u16() as usize;
        let mut members: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(count, Default::default());
        for _ in 0..count {
            let mut value = self.stack.pop().unwrap_or(Value::Null);
            let key = self.stack.pop().unwrap_or(Value::Null);
            if let Value::String(s) = key {
                let key_str = s.to_string();
                // Set the name for nested namespaces/enums
                match &mut value {
                    Value::Namespace { name, .. } if name.is_empty() => {
                        *name = key_str.clone();
                    }
                    Value::Enum { name, .. } if name.is_empty() => {
                        *name = key_str.clone();
                    }
                    _ => {}
                }
                members.insert(key_str, value);
            } else {
                return Err(self.create_error(ErrorKind::TypeError, "Namespace member keys must be strings"));
            }
        }
        self.stack.push(Value::Namespace { name: String::new(), members: Arc::new(RwLock::new(members)), module_globals: None });
        Ok(())
    }

    fn handle_build_enum(&mut self) -> SaldResult<()> {
        let count = self.read_u16() as usize;
        let mut variants: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(count, Default::default());
        for _ in 0..count {
            let value = self.stack.pop().unwrap_or(Value::Null);
            let key = self.stack.pop().unwrap_or(Value::Null);
            if let Value::String(s) = key { variants.insert(s.to_string(), value); }
            else { return Err(self.create_error(ErrorKind::TypeError, "Enum variant keys must be strings")); }
        }
        self.stack.push(Value::Enum { name: String::new(), variants: Arc::new(variants) });
        Ok(())
    }

    fn handle_build_range(&mut self, inclusive: bool) -> SaldResult<()> {
        let end = self.stack.pop().unwrap_or(Value::Null);
        let start = self.stack.pop().unwrap_or(Value::Null);
        let start_num = match &start {
            Value::Number(n) => *n as i64,
            _ => return Err(self.create_error(ErrorKind::TypeError, &format!("Range start must be a number, got {}", start.type_name()))),
        };
        let end_num = match &end {
            Value::Number(n) => *n as i64,
            _ => return Err(self.create_error(ErrorKind::TypeError, &format!("Range end must be a number, got {}", end.type_name()))),
        };
        let mut elements = Vec::new();
        if inclusive {
            if start_num <= end_num { for i in start_num..=end_num { elements.push(Value::Number(i as f64)); } }
            else { for i in (end_num..=start_num).rev() { elements.push(Value::Number(i as f64)); } }
        } else {
            if start_num < end_num { for i in start_num..end_num { elements.push(Value::Number(i as f64)); } }
            else if start_num > end_num { for i in ((end_num + 1)..=start_num).rev() { elements.push(Value::Number(i as f64)); } }
        }
        let arr = Arc::new(Mutex::new(elements));
        self.track_array(&arr);
        self.stack.push(Value::Array(arr));
        Ok(())
    }

    fn handle_inherit(&mut self) -> SaldResult<()> {
        let superclass_val = self.stack.pop().unwrap_or(Value::Null);
        let subclass_val = self.stack.pop().unwrap_or(Value::Null);
        if let (Value::Class(superclass), Value::Class(subclass)) = (&superclass_val, &subclass_val) {
            let mut new_class = Class::new(subclass.name.clone());
            for (name, method) in &superclass.methods { new_class.methods.insert(name.clone(), method.clone()); }
            for (name, method) in &subclass.methods { new_class.methods.insert(name.clone(), method.clone()); }
            for (name, method) in &subclass.user_static_methods { new_class.user_static_methods.insert(name.clone(), method.clone()); }
            new_class.superclass = Some(superclass.clone());
            self.stack.push(Value::Class(Arc::new(new_class)));
            Ok(())
        } else {
            Err(self.create_error(ErrorKind::TypeError, &format!("Superclass must be a class, got '{}'", superclass_val.type_name())))
        }
    }

    fn handle_get_super(&mut self, method_name: &str) -> SaldResult<()> {
        let receiver = self.stack.pop().unwrap_or(Value::Null);
        if let Value::Instance(ref instance) = receiver {
            let class = instance.lock().class.clone();
            if let Some(ref superclass) = class.superclass {
                if let Some(method) = superclass.methods.get(method_name) {
                    if let Value::Function(func) = method {
                        self.stack.push(Value::BoundMethod { receiver: Box::new(receiver), method: func.clone() });
                        return Ok(());
                    }
                    return Err(self.create_error(ErrorKind::TypeError, &format!("Method '{}' is not a function", method_name)));
                }
                return Err(self.create_error(ErrorKind::AttributeError, &format!("Undefined method '{}' in superclass", method_name)));
            }
            return Err(self.create_error(ErrorKind::RuntimeError, "Class has no superclass"));
        }
        Err(self.create_error(ErrorKind::RuntimeError, "'super' can only be used in instance methods"))
    }

    // ==================== Import Handlers ====================

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_import(&mut self, import_path: &str) -> SaldResult<()> {
        let resolved_path = self.resolve_import_path(import_path)?;
        let module_workspace = self.pending_module_workspace.take();
        if let Some(ref workspace) = module_workspace { crate::push_module_workspace(workspace); }
        let imported_globals = self.import_and_execute(&resolved_path)?;
        if module_workspace.is_some() { crate::pop_module_workspace(); }
        for (name, value) in imported_globals {
            let globals_guard = self.globals.read();
            let should_insert = !globals_guard.contains_key(&name) || !matches!(globals_guard.get(&name), Some(Value::Class(_)));
            drop(globals_guard);
            if should_insert { self.globals.write().insert(name, value); }
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_import(&mut self, import_path: &str) -> SaldResult<()> {
        Err(self.create_error(ErrorKind::ImportError, &format!("import is not supported in WASM playground: {}", import_path)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_import_as(&mut self, import_path: &str, alias: &str) -> SaldResult<()> {
        let resolved_path = self.resolve_import_path(import_path)?;
        let module_workspace = self.pending_module_workspace.take();
        if let Some(ref workspace) = module_workspace { crate::push_module_workspace(workspace); }
        
        // Use the Arc version to preserve module globals for function scoping
        let (imported_globals, module_globals_arc) = self.import_and_execute_with_globals(&resolved_path)?;
        
        if module_workspace.is_some() { crate::pop_module_workspace(); }
        let mut module_fields = FxHashMap::default();
        for (name, value) in imported_globals {
            if !matches!(&value, Value::Class(c) if ["String", "Number", "Boolean", "Null", "Array"].contains(&c.name.as_str())) {
                module_fields.insert(name, value);
            }
        }
        // Use Namespace with stored module_globals so functions can access their original scope
        self.globals.write().insert(
            alias.to_string(),
            Value::Namespace {
                name: alias.to_string(),
                members: Arc::new(RwLock::new(module_fields)),
                module_globals: Some(module_globals_arc),
            }
        );
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_import_as(&mut self, import_path: &str, _alias: &str) -> SaldResult<()> {
        Err(self.create_error(ErrorKind::ImportError, &format!("import is not supported in WASM playground: {}", import_path)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_import_path(&mut self, import_path: &str) -> SaldResult<String> {
        if Self::is_module_import(import_path) { return self.resolve_module_import(import_path); }
        self.resolve_file_import(import_path)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn is_module_import(path: &str) -> bool {
        !path.contains('/') && !path.contains('\\') && !path.ends_with(".sald") && !path.ends_with(".saldc")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_module_import(&mut self, module_name: &str) -> SaldResult<String> {
        let project_root = crate::get_project_root()
            .ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Cannot import module '{}': no project root set", module_name)))?;
        let module_dir = project_root.join("sald_modules").join(module_name);
        if !module_dir.exists() { return Err(self.create_error(ErrorKind::ImportError, &format!("Module '{}' not found in sald_modules/", module_name))); }
        let config_path = module_dir.join("salad.json");
        if !config_path.exists() { return Err(self.create_error(ErrorKind::ImportError, &format!("Module '{}' has no salad.json config", module_name))); }
        let main_entry = self.parse_module_config(&config_path, module_name)?;
        let main_path = module_dir.join(&main_entry);
        if !main_path.exists() { return Err(self.create_error(ErrorKind::ImportError, &format!("Module '{}' main file '{}' not found", module_name, main_entry))); }
        self.pending_module_workspace = Some(module_dir);
        main_path.to_str().map(|s| s.to_string()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid module path for '{}'", module_name)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn parse_module_config(&self, config_path: &std::path::Path, module_name: &str) -> SaldResult<String> {
        let content = std::fs::read_to_string(config_path).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Failed to read module '{}' config: {}", module_name, e)))?;
        let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Module '{}' config invalid JSON: {}", module_name, e)))?;
        json.get("main").and_then(|v| v.as_str()).map(|s| s.to_string()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Module '{}' config missing 'main' field", module_name)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_file_import(&self, import_path: &str) -> SaldResult<String> {
        use std::path::PathBuf;
        use std::env;
        let path_with_ext = if !import_path.ends_with(".sald") && !import_path.ends_with(".saldc") { format!("{}.sald", import_path) } else { import_path.to_string() };
        let path_buf = PathBuf::from(&path_with_ext);
        let canonicalize = |p: PathBuf| -> Option<String> { p.canonicalize().ok().and_then(|abs| abs.to_str().map(|s| s.to_string())) };
        if path_buf.is_absolute() && path_buf.exists() { return canonicalize(path_buf.clone()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path))); }
        let current_dir = if self.file.is_empty() { PathBuf::from(".") } else { PathBuf::from(&self.file).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")) };
        let relative_path = current_dir.join(&path_with_ext);
        if relative_path.exists() { return canonicalize(relative_path.clone()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path))); }
        if let Ok(module_path) = env::var("SALD_MODULE") {
            let env_path = PathBuf::from(module_path).join(&path_with_ext);
            if env_path.exists() { return canonicalize(env_path.clone()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path))); }
        }
        relative_path.to_str().map(|s| s.to_string()).ok_or_else(|| self.create_error(ErrorKind::ImportError, &format!("Invalid import path: {}", import_path)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_and_execute(&mut self, path: &str) -> SaldResult<FxHashMap<String, Value>> {
        let chunk = if path.ends_with(".saldc") {
            let data = std::fs::read(path).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e)))?;
            crate::binary::deserialize(&data).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Error deserializing import '{}': {}", path, e)))?
        } else {
            let source = std::fs::read_to_string(path).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e)))?;
            let mut scanner = Scanner::new(&source, path);
            let tokens = scanner.scan_tokens().map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error scanning import '{}': {}", path, e)))?;
            let mut parser = Parser::new(tokens, path, &source);
            let program = parser.parse().map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error parsing import '{}': {}", path, e)))?;
            let mut compiler = Compiler::new(path, &source);
            compiler.compile(&program).map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error compiling import '{}': {}", path, e)))?
        };
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_file = std::mem::replace(&mut self.file, path.to_string());
        let saved_source = std::mem::replace(&mut self.source, String::new());
        crate::push_script_dir(path);
        let import_globals = Arc::new(RwLock::new(builtins::create_builtin_classes()));
        let saved_globals = std::mem::replace(&mut self.globals, import_globals);
        let mut main_function = Function::new("<import>", 0, chunk);
        main_function.file = path.to_string();
        self.stack.push(Value::Null);
        self.frames.push(CallFrame::new(Arc::new(main_function), 0));
        loop {
            match self.execute_until_suspend() {
                ExecutionResult::Completed(_) => break,
                ExecutionResult::Suspended { receiver } => {
                    match futures::executor::block_on(receiver) {
                        Ok(Ok(value)) => { self.stack.push(value); }
                        Ok(Err(e)) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(self.create_error(ErrorKind::ImportError, &format!("Import error: {}", e))); }
                        Err(_) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(self.create_error(ErrorKind::ImportError, "Import future cancelled")); }
                    }
                }
                ExecutionResult::Error(e) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(e); }
            }
        }
        let imported_globals = std::mem::take(&mut *self.globals.write());
        crate::pop_script_dir();
        self.stack = saved_stack;
        self.frames = saved_frames;
        self.file = saved_file;
        self.source = saved_source;
        self.globals = saved_globals;
        Ok(imported_globals)
    }

    /// Import and execute a module, returning both the globals HashMap AND the Arc
    /// This preserves the module's globals context for proper function scoping in import-as
    #[cfg(not(target_arch = "wasm32"))]
    fn import_and_execute_with_globals(&mut self, path: &str) -> SaldResult<(FxHashMap<String, Value>, Arc<RwLock<FxHashMap<String, Value>>>)> {
        let chunk = if path.ends_with(".saldc") {
            let data = std::fs::read(path).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e)))?;
            crate::binary::deserialize(&data).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Error deserializing import '{}': {}", path, e)))?
        } else {
            let source = std::fs::read_to_string(path).map_err(|e| self.create_error(ErrorKind::ImportError, &format!("Cannot read import file '{}': {}", path, e)))?;
            let mut scanner = Scanner::new(&source, path);
            let tokens = scanner.scan_tokens().map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error scanning import '{}': {}", path, e)))?;
            let mut parser = Parser::new(tokens, path, &source);
            let program = parser.parse().map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error parsing import '{}': {}", path, e)))?;
            let mut compiler = Compiler::new(path, &source);
            compiler.compile(&program).map_err(|e| self.create_error(ErrorKind::SyntaxError, &format!("Error compiling import '{}': {}", path, e)))?
        };
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_file = std::mem::replace(&mut self.file, path.to_string());
        let saved_source = std::mem::replace(&mut self.source, String::new());
        crate::push_script_dir(path);
        let import_globals = Arc::new(RwLock::new(builtins::create_builtin_classes()));
        let module_globals_arc = import_globals.clone(); // Keep a clone of the Arc
        let saved_globals = std::mem::replace(&mut self.globals, import_globals);
        let mut main_function = Function::new("<import>", 0, chunk);
        main_function.file = path.to_string();
        self.stack.push(Value::Null);
        self.frames.push(CallFrame::new(Arc::new(main_function), 0));
        loop {
            match self.execute_until_suspend() {
                ExecutionResult::Completed(_) => break,
                ExecutionResult::Suspended { receiver } => {
                    match futures::executor::block_on(receiver) {
                        Ok(Ok(value)) => { self.stack.push(value); }
                        Ok(Err(e)) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(self.create_error(ErrorKind::ImportError, &format!("Import error: {}", e))); }
                        Err(_) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(self.create_error(ErrorKind::ImportError, "Import future cancelled")); }
                    }
                }
                ExecutionResult::Error(e) => { crate::pop_script_dir(); self.stack = saved_stack; self.frames = saved_frames; self.file = saved_file; self.source = saved_source; self.globals = saved_globals; return Err(e); }
            }
        }
        // Clone the globals content but keep the Arc alive
        let imported_globals = self.globals.read().clone();
        crate::pop_script_dir();
        self.stack = saved_stack;
        self.frames = saved_frames;
        self.file = saved_file;
        self.source = saved_source;
        self.globals = saved_globals;
        Ok((imported_globals, module_globals_arc))
    }


    fn create_error(&self, kind: ErrorKind, message: &str) -> SaldError {
        let (span, file) = if !self.frames.is_empty() {
            let frame = self.current_frame();
            let f = if frame.function.file.is_empty() { &self.file } else { &frame.function.file };
            (frame.current_span(), f)
        } else {
            (Span::default(), &self.file)
        };
        let mut error = SaldError::new(kind, message, span, file);
        if file == &self.file { error = error.with_source(&self.source); }
        else if let Ok(source) = std::fs::read_to_string(file) { error = error.with_source(&source); }
        let mut stack_trace = Vec::new();
        for frame in self.frames.iter().rev() {
            let frame_span = frame.current_span();
            stack_trace.push(StackFrame::new(&frame.function.name, if frame.function.file.is_empty() { &self.file } else { &frame.function.file }, frame_span.start.line, frame_span.start.column));
        }
        error.with_stack_trace(stack_trace)
    }

    fn handle_native_error(&mut self, error_msg: String) -> SaldResult<()> {
        if let Some(handler) = self.exception_handlers.pop() {
            while self.frames.len() > handler.frame_index + 1 { self.frames.pop(); }
            while self.stack.len() > handler.stack_size { self.stack.pop(); }
            self.stack.push(Value::String(Arc::from(error_msg)));
            self.current_frame_mut().ip = handler.catch_ip;
            Ok(())
        } else {
            Err(self.create_error(ErrorKind::RuntimeError, &format!("Uncaught exception: {}", error_msg)))
        }
    }

    /// Call a global function by name (for test runner)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn call_global(&mut self, name: &str, args: Vec<Value>) -> SaldResult<Value> {
        // Look up the function in globals
        let func = {
            let globals = self.globals.read();
            globals.get(name).cloned()
        };
        
        match func {
            Some(Value::Function(f)) => {
                // Push null as receiver placeholder
                self.stack.push(Value::Null);
                // Push function
                self.stack.push(Value::Function(f.clone()));
                // Push args
                for arg in &args {
                    self.stack.push(arg.clone());
                }
                // Call it
                self.call_value(args.len())?;
                // Run event loop
                self.run_event_loop().await
            }
            Some(other) => {
                Err(self.create_error(ErrorKind::TypeError, &format!("'{}' is not a function, got {}", name, other.type_name())))
            }
            None => {
                Err(self.create_error(ErrorKind::NameError, &format!("Function '{}' not found", name)))
            }
        }
    }
}

impl Default for VM {
    fn default() -> Self { Self::new() }
}
