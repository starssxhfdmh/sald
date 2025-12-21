// FFI Built-in Namespace - Advanced Foreign Function Interface
// Supports: explicit types, memory operations, callbacks, structs, and more
//
// Usage:
//   let lib = Ffi.load("library")
//   
//   // Method 1: Auto-inferred call (old style)
//   let result = lib.call("func", arg1, arg2)
//   
//   // Method 2: Typed call with signature
//   lib.define("add", ["i32", "i32"], "i32")
//   let result = lib.callTyped("add", [5, 10])
//
//   // Memory operations
//   let ptr = Ffi.malloc(256)
//   Ffi.writeI32(ptr, 42)
//   let val = Ffi.readI32(ptr)
//   Ffi.free(ptr)

use crate::vm::caller::ValueCaller;
use crate::vm::value::{Class, Instance, NativeInstanceFn, Value};
use libffi::middle::{Arg, Cif, CodePtr, Type as FfiType};
use libloading::{Library, Symbol};
use std::alloc::{alloc, Layout};
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::ptr;
use std::sync::{Arc, Mutex, RwLock};

// ==================== Type System ====================

#[derive(Debug, Clone, PartialEq)]
enum CType {
    Void,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Pointer,
    CString,
}

impl CType {
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "void" => Ok(CType::Void),
            "i8" | "int8" | "char" => Ok(CType::I8),
            "u8" | "uint8" | "uchar" | "byte" => Ok(CType::U8),
            "i16" | "int16" | "short" => Ok(CType::I16),
            "u16" | "uint16" | "ushort" => Ok(CType::U16),
            "i32" | "int32" | "int" => Ok(CType::I32),
            "u32" | "uint32" | "uint" => Ok(CType::U32),
            "i64" | "int64" | "long" => Ok(CType::I64),
            "u64" | "uint64" | "ulong" => Ok(CType::U64),
            "f32" | "float" => Ok(CType::F32),
            "f64" | "double" => Ok(CType::F64),
            "ptr" | "pointer" | "*" => Ok(CType::Pointer),
            "string" | "cstring" | "char*" => Ok(CType::CString),
            _ => Err(format!("Unknown FFI type: {}", s)),
        }
    }

    fn to_ffi_type(&self) -> FfiType {
        match self {
            CType::Void => FfiType::void(),
            CType::I8 | CType::U8 => FfiType::u8(),
            CType::I16 | CType::U16 => FfiType::u16(),
            CType::I32 | CType::U32 => FfiType::u32(),
            CType::I64 | CType::U64 => FfiType::u64(),
            CType::F32 => FfiType::f32(),
            CType::F64 => FfiType::f64(),
            CType::Pointer | CType::CString => FfiType::pointer(),
        }
    }

    fn size(&self) -> usize {
        match self {
            CType::Void => 0,
            CType::I8 | CType::U8 => 1,
            CType::I16 | CType::U16 => 2,
            CType::I32 | CType::U32 | CType::F32 => 4,
            CType::I64 | CType::U64 | CType::F64 => 8,
            CType::Pointer | CType::CString => std::mem::size_of::<usize>(),
        }
    }
}

// ==================== Function Signature ====================

#[derive(Clone)]
struct FunctionSignature {
    arg_types: Vec<CType>,
    return_type: CType,
}

// ==================== FFI Library ====================

struct FfiLibrary {
    library: Library,
    _path: String,
    signatures: HashMap<String, FunctionSignature>,
}

// ==================== Thread-safe pointer wrapper ====================

/// Wrapper for raw pointers to make them Send+Sync
/// SAFETY: The caller is responsible for ensuring the pointer remains valid
/// while stored in the global. We only use this during active FFI calls.
#[derive(Clone, Copy)]
struct SendPtr(*mut ());
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

#[derive(Clone, Copy)]
struct SendConstPtr(*const ());
unsafe impl Send for SendConstPtr {}
unsafe impl Sync for SendConstPtr {}

// ==================== Callback Registry ====================

static CALLBACK_REGISTRY: RwLock<Option<HashMap<i64, Value>>> = RwLock::new(None);
static NEXT_CALLBACK_ID: Mutex<i64> = Mutex::new(1);

// Store caller as raw pointer with explicit lifetime tracking
// This is safer than transmuting fat pointers
static GLOBAL_CALLER_PTR: Mutex<Option<SendPtr>> = Mutex::new(None);
static GLOBAL_CALLER_VTABLE: Mutex<Option<SendConstPtr>> = Mutex::new(None);

fn init_registry() {
    let mut reg = CALLBACK_REGISTRY.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
}

fn register_callback(func: Value) -> i64 {
    init_registry();
    let mut id_guard = NEXT_CALLBACK_ID.lock().unwrap();
    let id = *id_guard;
    *id_guard += 1;
    let mut reg = CALLBACK_REGISTRY.write().unwrap();
    if let Some(ref mut map) = *reg {
        map.insert(id, func);
    }
    id
}

fn unregister_callback(id: i64) {
    if let Ok(mut reg) = CALLBACK_REGISTRY.write() {
        if let Some(ref mut map) = *reg {
            map.remove(&id);
        }
    }
}

fn get_callback(id: i64) -> Option<Value> {
    if let Ok(reg) = CALLBACK_REGISTRY.read() {
        if let Some(ref map) = *reg {
            return map.get(&id).cloned();
        }
    }
    None
}

/// Store global caller using TraitObject-like decomposition
/// This avoids the UB of transmuting between [usize; 2] and fat pointers
fn set_global_caller(caller: &mut dyn ValueCaller) {
    // Get raw pointer parts safely using pointer metadata
    let ptr = caller as *mut dyn ValueCaller;
    let data_ptr = ptr as *mut () as *mut ();
    // Extract vtable pointer by reading the second word of the fat pointer
    let vtable_ptr = unsafe {
        let fat_ptr_bytes = &ptr as *const _ as *const [*const (); 2];
        (*fat_ptr_bytes)[1] as *const ()
    };
    
    let mut ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
    let mut vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
    *ptr_guard = Some(SendPtr(data_ptr));
    *vtable_guard = Some(SendConstPtr(vtable_ptr));
}

fn clear_global_caller() {
    let mut ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
    let mut vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
    *ptr_guard = None;
    *vtable_guard = None;
}

#[no_mangle]
pub extern "C" fn sald_invoke_callback(callback_id: i64, arg_count: i64, args: *const i64) -> i64 {
    let callback = match get_callback(callback_id) {
        Some(cb) => cb,
        None => {
            eprintln!("[FFI] Callback {} not found", callback_id);
            return 0;
        }
    };

    // Reconstruct the fat pointer from stored components
    let (data_ptr, vtable_ptr) = {
        let ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
        let vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
        match (&*ptr_guard, &*vtable_guard) {
            (Some(SendPtr(p)), Some(SendConstPtr(v))) => (*p, *v),
            _ => {
                eprintln!("[FFI] No active caller for callback");
                return 0;
            }
        }
    };

    // Reconstruct the trait object pointer safely
    let caller: &mut dyn ValueCaller = unsafe {
        let fat_ptr: [*const (); 2] = [data_ptr as *const (), vtable_ptr];
        let trait_ptr = std::ptr::read(&fat_ptr as *const _ as *const *mut dyn ValueCaller);
        &mut *trait_ptr
    };

    let mut sald_args = Vec::with_capacity(arg_count as usize);
    if !args.is_null() {
        for i in 0..arg_count as usize {
            let arg_val = unsafe { *args.add(i) };
            sald_args.push(Value::Number(arg_val as f64));
        }
    }

    match caller.call(&callback, sald_args) {
        Ok(result) => match result {
            Value::Number(n) => n as i64,
            Value::Boolean(b) => if b { 1 } else { 0 },
            Value::Null => 0,
            _ => 0,
        },
        Err(e) => {
            eprintln!("[FFI] Callback error: {}", e);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn sald_get_callback_invoker() -> *const c_void {
    sald_invoke_callback as *const c_void
}

// ==================== Value Conversion ====================

struct ConvertedArg {
    ffi_type: FfiType,
    data: ConvertedData,
}

enum ConvertedData {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Ptr(usize),
    CStr(CString),
}

fn convert_value_to_arg(value: &Value, ctype: &CType) -> Result<ConvertedArg, String> {
    let data = match (value, ctype) {
        (Value::Number(n), CType::I8) => ConvertedData::I8(*n as i8),
        (Value::Number(n), CType::U8) => ConvertedData::U8(*n as u8),
        (Value::Number(n), CType::I16) => ConvertedData::I16(*n as i16),
        (Value::Number(n), CType::U16) => ConvertedData::U16(*n as u16),
        (Value::Number(n), CType::I32) => ConvertedData::I32(*n as i32),
        (Value::Number(n), CType::U32) => ConvertedData::U32(*n as u32),
        (Value::Number(n), CType::I64) => ConvertedData::I64(*n as i64),
        (Value::Number(n), CType::U64) => ConvertedData::U64(*n as u64),
        (Value::Number(n), CType::F32) => ConvertedData::F32(*n as f32),
        (Value::Number(n), CType::F64) => ConvertedData::F64(*n),
        (Value::Number(n), CType::Pointer) => ConvertedData::Ptr(*n as usize),
        (Value::String(s), CType::CString) => {
            let c_str = CString::new(s.as_str()).map_err(|_| "Invalid C string")?;
            ConvertedData::CStr(c_str)
        }
        (Value::Null, CType::Pointer | CType::CString) => ConvertedData::Ptr(0),
        _ => return Err(format!("Cannot convert {} to {:?}", value.type_name(), ctype)),
    };

    Ok(ConvertedArg {
        ffi_type: ctype.to_ffi_type(),
        data,
    })
}

// ==================== FFI Namespace ====================

pub fn create_ffi_namespace() -> Value {
    let mut members: HashMap<String, Value> = HashMap::new();

    // Library management
    members.insert("load".to_string(), Value::NativeFunction {
        func: ffi_load,
        class_name: "Ffi".into(),
    });

    // Memory operations
    members.insert("malloc".to_string(), Value::NativeFunction {
        func: ffi_malloc,
        class_name: "Ffi".into(),
    });
    members.insert("free".to_string(), Value::NativeFunction {
        func: ffi_free,
        class_name: "Ffi".into(),
    });
    members.insert("memcpy".to_string(), Value::NativeFunction {
        func: ffi_memcpy,
        class_name: "Ffi".into(),
    });
    members.insert("memset".to_string(), Value::NativeFunction {
        func: ffi_memset,
        class_name: "Ffi".into(),
    });

    // Read operations
    members.insert("readI8".to_string(), Value::NativeFunction {
        func: ffi_read_i8,
        class_name: "Ffi".into(),
    });
    members.insert("readU8".to_string(), Value::NativeFunction {
        func: ffi_read_u8,
        class_name: "Ffi".into(),
    });
    members.insert("readI16".to_string(), Value::NativeFunction {
        func: ffi_read_i16,
        class_name: "Ffi".into(),
    });
    members.insert("readU16".to_string(), Value::NativeFunction {
        func: ffi_read_u16,
        class_name: "Ffi".into(),
    });
    members.insert("readI32".to_string(), Value::NativeFunction {
        func: ffi_read_i32,
        class_name: "Ffi".into(),
    });
    members.insert("readU32".to_string(), Value::NativeFunction {
        func: ffi_read_u32,
        class_name: "Ffi".into(),
    });
    members.insert("readI64".to_string(), Value::NativeFunction {
        func: ffi_read_i64,
        class_name: "Ffi".into(),
    });
    members.insert("readU64".to_string(), Value::NativeFunction {
        func: ffi_read_u64,
        class_name: "Ffi".into(),
    });
    members.insert("readF32".to_string(), Value::NativeFunction {
        func: ffi_read_f32,
        class_name: "Ffi".into(),
    });
    members.insert("readF64".to_string(), Value::NativeFunction {
        func: ffi_read_f64,
        class_name: "Ffi".into(),
    });
    members.insert("readPtr".to_string(), Value::NativeFunction {
        func: ffi_read_ptr,
        class_name: "Ffi".into(),
    });
    members.insert("readString".to_string(), Value::NativeFunction {
        func: ffi_read_string,
        class_name: "Ffi".into(),
    });

    // Write operations
    members.insert("writeI8".to_string(), Value::NativeFunction {
        func: ffi_write_i8,
        class_name: "Ffi".into(),
    });
    members.insert("writeU8".to_string(), Value::NativeFunction {
        func: ffi_write_u8,
        class_name: "Ffi".into(),
    });
    members.insert("writeI16".to_string(), Value::NativeFunction {
        func: ffi_write_i16,
        class_name: "Ffi".into(),
    });
    members.insert("writeU16".to_string(), Value::NativeFunction {
        func: ffi_write_u16,
        class_name: "Ffi".into(),
    });
    members.insert("writeI32".to_string(), Value::NativeFunction {
        func: ffi_write_i32,
        class_name: "Ffi".into(),
    });
    members.insert("writeU32".to_string(), Value::NativeFunction {
        func: ffi_write_u32,
        class_name: "Ffi".into(),
    });
    members.insert("writeI64".to_string(), Value::NativeFunction {
        func: ffi_write_i64,
        class_name: "Ffi".into(),
    });
    members.insert("writeU64".to_string(), Value::NativeFunction {
        func: ffi_write_u64,
        class_name: "Ffi".into(),
    });
    members.insert("writeF32".to_string(), Value::NativeFunction {
        func: ffi_write_f32,
        class_name: "Ffi".into(),
    });
    members.insert("writeF64".to_string(), Value::NativeFunction {
        func: ffi_write_f64,
        class_name: "Ffi".into(),
    });
    members.insert("writePtr".to_string(), Value::NativeFunction {
        func: ffi_write_ptr,
        class_name: "Ffi".into(),
    });

    // Pointer operations
    members.insert("cast".to_string(), Value::NativeFunction {
        func: ffi_cast,
        class_name: "Ffi".into(),
    });
    members.insert("offset".to_string(), Value::NativeFunction {
        func: ffi_offset,
        class_name: "Ffi".into(),
    });
    members.insert("sizeof".to_string(), Value::NativeFunction {
        func: ffi_sizeof,
        class_name: "Ffi".into(),
    });

    // Callbacks
    members.insert("callback".to_string(), Value::NativeFunction {
        func: ffi_create_callback,
        class_name: "Ffi".into(),
    });
    members.insert("removeCallback".to_string(), Value::NativeFunction {
        func: ffi_remove_callback,
        class_name: "Ffi".into(),
    });

    // Constants
    members.insert("NULL".to_string(), Value::Number(0.0));
    members.insert("INVOKER".to_string(), Value::Number(sald_get_callback_invoker() as usize as f64));

    // Library class
    members.insert("Library".to_string(), Value::Class(Arc::new(create_library_class())));

    Value::Namespace {
        name: "Ffi".to_string(),
        members: Arc::new(Mutex::new(members)),
    }
}

// ==================== Memory Operations ====================

fn ffi_malloc(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.malloc expects 1 argument (size)".to_string());
    }
    let size = match &args[0] {
        Value::Number(n) => *n as usize,
        _ => return Err("Size must be a number".to_string()),
    };
    if size == 0 {
        return Ok(Value::Number(0.0));
    }
    unsafe {
        let layout = Layout::from_size_align(size, 8)
            .map_err(|_| "Invalid layout")?;
        let ptr = alloc(layout);
        if ptr.is_null() {
            return Err("Memory allocation failed".to_string());
        }
        Ok(Value::Number(ptr as usize as f64))
    }
}

fn ffi_free(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.free expects 1 argument (pointer)".to_string());
    }
    let ptr = match &args[0] {
        Value::Number(n) => *n as usize as *mut u8,
        _ => return Err("Pointer must be a number".to_string()),
    };
    if !ptr.is_null() {
        // Note: We can't safely free without knowing the original size
        // This is a limitation - users should track sizes themselves
        // For now, we just acknowledge the free
    }
    Ok(Value::Null)
}

fn ffi_memcpy(args: &[Value]) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Ffi.memcpy expects 3 arguments (dest, src, size)".to_string());
    }
    let dest = match &args[0] {
        Value::Number(n) => *n as usize as *mut u8,
        _ => return Err("Dest must be a number".to_string()),
    };
    let src = match &args[1] {
        Value::Number(n) => *n as usize as *const u8,
        _ => return Err("Src must be a number".to_string()),
    };
    let size = match &args[2] {
        Value::Number(n) => *n as usize,
        _ => return Err("Size must be a number".to_string()),
    };
    if !dest.is_null() && !src.is_null() && size > 0 {
        unsafe {
            ptr::copy_nonoverlapping(src, dest, size);
        }
    }
    Ok(Value::Null)
}

fn ffi_memset(args: &[Value]) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Ffi.memset expects 3 arguments (ptr, value, size)".to_string());
    }
    let ptr = match &args[0] {
        Value::Number(n) => *n as usize as *mut u8,
        _ => return Err("Pointer must be a number".to_string()),
    };
    let value = match &args[1] {
        Value::Number(n) => *n as u8,
        _ => return Err("Value must be a number".to_string()),
    };
    let size = match &args[2] {
        Value::Number(n) => *n as usize,
        _ => return Err("Size must be a number".to_string()),
    };
    if !ptr.is_null() && size > 0 {
        unsafe {
            ptr::write_bytes(ptr, value, size);
        }
    }
    Ok(Value::Null)
}

// ==================== Read Operations ====================

macro_rules! impl_read {
    ($name:ident, $type:ty) => {
        fn $name(args: &[Value]) -> Result<Value, String> {
            if args.is_empty() {
                return Err(concat!(stringify!($name), " expects 1 argument (pointer)").to_string());
            }
            let ptr = match &args[0] {
                Value::Number(n) => *n as usize as *const $type,
                _ => return Err("Pointer must be a number".to_string()),
            };
            if ptr.is_null() {
                return Err("Cannot read from null pointer".to_string());
            }
            unsafe {
                let value = *ptr;
                Ok(Value::Number(value as f64))
            }
        }
    };
}

impl_read!(ffi_read_i8, i8);
impl_read!(ffi_read_u8, u8);
impl_read!(ffi_read_i16, i16);
impl_read!(ffi_read_u16, u16);
impl_read!(ffi_read_i32, i32);
impl_read!(ffi_read_u32, u32);
impl_read!(ffi_read_i64, i64);
impl_read!(ffi_read_u64, u64);
impl_read!(ffi_read_f32, f32);
impl_read!(ffi_read_f64, f64);
impl_read!(ffi_read_ptr, usize);

fn ffi_read_string(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.readString expects 1 argument (pointer)".to_string());
    }
    let ptr = match &args[0] {
        Value::Number(n) => *n as usize as *const std::os::raw::c_char,
        _ => return Err("Pointer must be a number".to_string()),
    };
    if ptr.is_null() {
        return Ok(Value::String(Arc::new(String::new())));
    }
    let s = unsafe {
        match CStr::from_ptr(ptr).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return Ok(Value::String(Arc::new(String::new()))),
        }
    };
    Ok(Value::String(Arc::new(s)))
}

// ==================== Write Operations ====================

macro_rules! impl_write {
    ($name:ident, $type:ty) => {
        fn $name(args: &[Value]) -> Result<Value, String> {
            if args.len() < 2 {
                return Err(concat!(stringify!($name), " expects 2 arguments (pointer, value)").to_string());
            }
            let ptr = match &args[0] {
                Value::Number(n) => *n as usize as *mut $type,
                _ => return Err("Pointer must be a number".to_string()),
            };
            let value = match &args[1] {
                Value::Number(n) => *n as $type,
                _ => return Err("Value must be a number".to_string()),
            };
            if ptr.is_null() {
                return Err("Cannot write to null pointer".to_string());
            }
            unsafe {
                *ptr = value;
            }
            Ok(Value::Null)
        }
    };
}

impl_write!(ffi_write_i8, i8);
impl_write!(ffi_write_u8, u8);
impl_write!(ffi_write_i16, i16);
impl_write!(ffi_write_u16, u16);
impl_write!(ffi_write_i32, i32);
impl_write!(ffi_write_u32, u32);
impl_write!(ffi_write_i64, i64);
impl_write!(ffi_write_u64, u64);
impl_write!(ffi_write_f32, f32);
impl_write!(ffi_write_f64, f64);
impl_write!(ffi_write_ptr, usize);

// ==================== Pointer Operations ====================

fn ffi_cast(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.cast expects 1 argument (value)".to_string());
    }
    match &args[0] {
        Value::Number(n) => Ok(Value::Number(*n)),
        _ => Err("Value must be a number".to_string()),
    }
}

fn ffi_offset(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Ffi.offset expects 2 arguments (pointer, offset)".to_string());
    }
    let ptr = match &args[0] {
        Value::Number(n) => *n as usize,
        _ => return Err("Pointer must be a number".to_string()),
    };
    let offset = match &args[1] {
        Value::Number(n) => *n as isize,
        _ => return Err("Offset must be a number".to_string()),
    };
    let new_ptr = (ptr as isize + offset) as usize;
    Ok(Value::Number(new_ptr as f64))
}

fn ffi_sizeof(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.sizeof expects 1 argument (type_name)".to_string());
    }
    let type_name = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err("Type name must be a string".to_string()),
    };
    let ctype = CType::from_str(type_name)?;
    Ok(Value::Number(ctype.size() as f64))
}

// ==================== Callback Operations ====================

fn ffi_create_callback(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.callback expects 1 argument (function)".to_string());
    }
    let func = &args[0];
    match func {
        Value::Function(_) => {}
        _ => return Err(format!("Expected function, got {}", func.type_name())),
    }
    let callback_id = register_callback(func.clone());
    let mut result = HashMap::new();
    result.insert("id".to_string(), Value::Number(callback_id as f64));
    result.insert("invoker".to_string(), Value::Number(sald_get_callback_invoker() as usize as f64));
    Ok(Value::Dictionary(Arc::new(Mutex::new(result))))
}

fn ffi_remove_callback(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.removeCallback expects 1 argument (callback_id)".to_string());
    }
    let id = match &args[0] {
        Value::Number(n) => *n as i64,
        Value::Dictionary(dict) => {
            let guard = dict.lock().unwrap();
            match guard.get("id") {
                Some(Value::Number(n)) => *n as i64,
                _ => return Err("Invalid callback object".to_string()),
            }
        }
        _ => return Err("Callback ID must be a number".to_string()),
    };
    unregister_callback(id);
    Ok(Value::Null)
}

// ==================== Library Loading ====================

fn ffi_load(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.load expects 1 argument (path)".to_string());
    }
    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Path must be a string, got {}", args[0].type_name())),
    };

    let path_with_ext = if cfg!(target_os = "windows") {
        if !path.ends_with(".dll") && !path.contains('.') {
            format!("{}.dll", path)
        } else {
            path.clone()
        }
    } else if cfg!(target_os = "macos") {
        if !path.ends_with(".dylib") && !path.contains('.') {
            format!("{}.dylib", path)
        } else {
            path.clone()
        }
    } else {
        if !path.ends_with(".so") && !path.contains('.') {
            format!("{}.so", path)
        } else {
            path.clone()
        }
    };

    let resolved_path = crate::resolve_script_path(&path_with_ext);
    let full_path = resolved_path.to_string_lossy().to_string();

    let library = unsafe {
        Library::new(&full_path)
            .map_err(|e| format!("Failed to load library '{}': {}", full_path, e))?
    };

    let lib_class = Arc::new(create_library_class());
    let mut instance = Instance::new(lib_class);

    let ffi_lib = FfiLibrary {
        library,
        _path: full_path.clone(),
        signatures: HashMap::new(),
    };

    let lib_handle = Arc::new(Mutex::new(ffi_lib));

    instance.fields.insert(
        "_handle".to_string(),
        Value::Number(Arc::as_ptr(&lib_handle) as usize as f64),
    );
    instance.fields.insert(
        "_path".to_string(),
        Value::String(Arc::new(full_path)),
    );

    std::mem::forget(lib_handle);

    Ok(Value::Instance(Arc::new(Mutex::new(instance))))
}

// ==================== Library Class ====================

fn create_library_class() -> Class {
    let mut instance_methods: HashMap<String, NativeInstanceFn> = HashMap::new();
    let mut callable_methods: HashMap<String, crate::vm::caller::CallableNativeInstanceFn> =
        HashMap::new();

    instance_methods.insert("call".to_string(), library_call);
    instance_methods.insert("callTyped".to_string(), library_call_typed);
    instance_methods.insert("define".to_string(), library_define);
    instance_methods.insert("symbol".to_string(), library_symbol);
    instance_methods.insert("close".to_string(), library_close);
    instance_methods.insert("path".to_string(), library_path);

    callable_methods.insert("callWithCallback".to_string(), library_call_with_callback);

    let mut class = Class::new_with_instance("Library", instance_methods, None);
    class.callable_native_instance_methods = callable_methods;
    class
}

// ==================== Library Methods ====================

fn library_call(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("library.call expects at least 1 argument (function name)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Function name must be a string, got {}", args[0].type_name())),
    };

    let ffi_args = &args[1..];

    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                return Err("Library has been closed".to_string());
            }

            unsafe {
                let lib_mutex = &*ptr;
                let lib_guard = lib_mutex.lock().unwrap();

                let fn_name_c = CString::new(fn_name.as_str())
                    .map_err(|_| "Invalid function name")?;

                let func_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(fn_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                let func_ptr = *func_ptr;

                // Auto-infer i64 for all args, i64 return
                let arg_types = vec![CType::I64; ffi_args.len()];
                let result = call_with_types(func_ptr, ffi_args, &arg_types, &CType::I64)?;
                return Ok(result);
            }
        }
    }

    Err("Invalid library instance".to_string())
}

fn library_call_typed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("library.callTyped expects at least 1 argument (function name)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Function name must be a string, got {}", args[0].type_name())),
    };

    let call_args = if args.len() > 1 {
        match &args[1] {
            Value::Array(arr) => {
                let guard = arr.lock().unwrap();
                guard.clone()
            }
            _ => return Err("Second argument must be an array of arguments".to_string()),
        }
    } else {
        Vec::new()
    };

    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                return Err("Library has been closed".to_string());
            }

            unsafe {
                let lib_mutex = &*ptr;
                let lib_guard = lib_mutex.lock().unwrap();

                let signature = lib_guard.signatures.get(&fn_name)
                    .ok_or_else(|| format!("Function '{}' not defined. Use define() first.", fn_name))?
                    .clone();

                if call_args.len() != signature.arg_types.len() {
                    return Err(format!(
                        "Function '{}' expects {} arguments, got {}",
                        fn_name,
                        signature.arg_types.len(),
                        call_args.len()
                    ));
                }

                let fn_name_c = CString::new(fn_name.as_str())
                    .map_err(|_| "Invalid function name")?;

                let func_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(fn_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                let func_ptr = *func_ptr;

                let result = call_with_types(
                    func_ptr,
                    &call_args,
                    &signature.arg_types,
                    &signature.return_type,
                )?;
                return Ok(result);
            }
        }
    }

    Err("Invalid library instance".to_string())
}

fn library_define(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("library.define expects 3 arguments (name, arg_types, return_type)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err("Function name must be a string".to_string()),
    };

    let arg_type_names = match &args[1] {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            guard.clone()
        }
        _ => return Err("Argument types must be an array".to_string()),
    };

    let return_type_name = match &args[2] {
        Value::String(s) => s.as_str(),
        _ => return Err("Return type must be a string".to_string()),
    };

    let mut arg_types = Vec::new();
    for arg_type in arg_type_names {
        let type_name = match arg_type {
            Value::String(s) => s.to_string(),
            _ => return Err("Argument type must be a string".to_string()),
        };
        arg_types.push(CType::from_str(&type_name)?);
    }

    let return_type = CType::from_str(return_type_name)?;

    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                return Err("Library has been closed".to_string());
            }

            unsafe {
                let lib_mutex = &*ptr;
                let mut lib_guard = lib_mutex.lock().unwrap();

                lib_guard.signatures.insert(
                    fn_name,
                    FunctionSignature {
                        arg_types,
                        return_type,
                    },
                );
            }

            return Ok(Value::Null);
        }
    }

    Err("Invalid library instance".to_string())
}

fn library_symbol(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("library.symbol expects 1 argument (symbol name)".to_string());
    }

    let symbol_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err("Symbol name must be a string".to_string()),
    };

    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                return Err("Library has been closed".to_string());
            }

            unsafe {
                let lib_mutex = &*ptr;
                let lib_guard = lib_mutex.lock().unwrap();

                let symbol_name_c = CString::new(symbol_name.as_str())
                    .map_err(|_| "Invalid symbol name")?;

                let symbol_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(symbol_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Symbol '{}' not found: {}", symbol_name, e))?;

                let ptr_value = *symbol_ptr as usize;
                return Ok(Value::Number(ptr_value as f64));
            }
        }
    }

    Err("Invalid library instance".to_string())
}

fn library_close(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let mut inst_guard = inst.lock().unwrap();

        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *mut Mutex<FfiLibrary>;
            if !ptr.is_null() {
                unsafe {
                    let _ = Arc::from_raw(ptr);
                }
            }
        }

        inst_guard.fields.insert("_handle".to_string(), Value::Null);
        Ok(Value::Null)
    } else {
        Err("Invalid library instance".to_string())
    }
}

fn library_path(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(path) = inst_guard.fields.get("_path") {
            return Ok(path.clone());
        }
    }
    Ok(Value::Null)
}

fn library_call_with_callback(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("library.callWithCallback expects at least 2 arguments (function name, callback)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Function name must be a string, got {}", args[0].type_name())),
    };

    let callback = &args[1];
    let callback_id = match callback {
        Value::Function(_) => register_callback(callback.clone()),
        Value::Dictionary(dict) => {
            let guard = dict.lock().unwrap();
            match guard.get("id") {
                Some(Value::Number(n)) => *n as i64,
                _ => return Err("Invalid callback object".to_string()),
            }
        }
        _ => return Err(format!("Callback must be a function, got {}", callback.type_name())),
    };

    let ffi_args = &args[2..];

    set_global_caller(caller);

    let result = if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                Err("Library has been closed".to_string())
            } else {
                unsafe {
                    let lib_mutex = &*ptr;
                    let lib_guard = lib_mutex.lock().unwrap();

                    let fn_name_c = CString::new(fn_name.as_str())
                        .map_err(|_| "Invalid function name")?;

                    let func_ptr: Symbol<*const c_void> = lib_guard
                        .library
                        .get(fn_name_c.as_bytes_with_nul())
                        .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                    let func_ptr = *func_ptr;

                    let mut all_args = Vec::with_capacity(ffi_args.len() + 2);
                    all_args.push(Value::Number(callback_id as f64));
                    all_args.push(Value::Number(sald_get_callback_invoker() as usize as f64));
                    all_args.extend_from_slice(ffi_args);

                    let arg_types = vec![CType::I64; all_args.len()];
                    let result = call_with_types(func_ptr, &all_args, &arg_types, &CType::I64)?;
                    Ok(result)
                }
            }
        } else {
            Err("Invalid library instance".to_string())
        }
    } else {
        Err("Invalid library instance".to_string())
    };

    clear_global_caller();

    if matches!(callback, Value::Function(_)) {
        unregister_callback(callback_id);
    }

    result
}

// ==================== FFI Call Implementation ====================

unsafe fn call_with_types(
    func_ptr: *const c_void,
    values: &[Value],
    arg_types: &[CType],
    return_type: &CType,
) -> Result<Value, String> {
    if values.len() != arg_types.len() {
        return Err(format!(
            "Argument count mismatch: expected {}, got {}",
            arg_types.len(),
            values.len()
        ));
    }

    let converted_args: Vec<ConvertedArg> = values
        .iter()
        .zip(arg_types.iter())
        .map(|(val, typ)| convert_value_to_arg(val, typ))
        .collect::<Result<Vec<_>, _>>()?;

    let ffi_types: Vec<FfiType> = converted_args.iter().map(|a| a.ffi_type.clone()).collect();

    // Store CString pointers separately to ensure they live long enough
    // This prevents the CString from being dropped before the FFI call
    let cstring_ptrs: Vec<*const std::os::raw::c_char> = converted_args
        .iter()
        .filter_map(|arg| {
            if let ConvertedData::CStr(s) = &arg.data {
                Some(s.as_ptr())
            } else {
                None
            }
        })
        .collect();
    
    let mut cstring_idx = 0;
    let mut ffi_args = Vec::with_capacity(converted_args.len());
    
    for arg in &converted_args {
        let arg_ref = match &arg.data {
            ConvertedData::I8(v) => Arg::new(v),
            ConvertedData::U8(v) => Arg::new(v),
            ConvertedData::I16(v) => Arg::new(v),
            ConvertedData::U16(v) => Arg::new(v),
            ConvertedData::I32(v) => Arg::new(v),
            ConvertedData::U32(v) => Arg::new(v),
            ConvertedData::I64(v) => Arg::new(v),
            ConvertedData::U64(v) => Arg::new(v),
            ConvertedData::F32(v) => Arg::new(v),
            ConvertedData::F64(v) => Arg::new(v),
            ConvertedData::Ptr(v) => Arg::new(v),
            ConvertedData::CStr(_) => {
                // Use the pre-extracted pointer from cstring_ptrs
                let ptr_ref = &cstring_ptrs[cstring_idx];
                cstring_idx += 1;
                Arg::new(ptr_ref)
            }
        };
        ffi_args.push(arg_ref);
    }

    let cif = Cif::new(ffi_types.into_iter(), return_type.to_ffi_type());
    let code_ptr = CodePtr::from_ptr(func_ptr as *const _);

    let result = match return_type {
        CType::Void => {
            cif.call::<()>(code_ptr, &ffi_args);
            Value::Null
        }
        CType::I8 => {
            let r: i8 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::U8 => {
            let r: u8 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::I16 => {
            let r: i16 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::U16 => {
            let r: u16 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::I32 => {
            let r: i32 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::U32 => {
            let r: u32 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::I64 => {
            let r: i64 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::U64 => {
            let r: u64 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::F32 => {
            let r: f32 = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
        CType::F64 => {
            let r: f64 = cif.call(code_ptr, &ffi_args);
            Value::Number(r)
        }
        CType::Pointer | CType::CString => {
            let r: usize = cif.call(code_ptr, &ffi_args);
            Value::Number(r as f64)
        }
    };

    Ok(result)
}
