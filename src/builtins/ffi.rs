// FFI Built-in Namespace - Foreign Function Interface
//
// Redesigned API (v2) - Clean, consistent, explicit types
//
// Usage:
//   let lib = Ffi.open("library")
//
//   // Unified call with explicit types
//   let result = lib.call("add", {
//       args: [{ type: "i32", value: 5 }, { type: "i32", value: 10 }],
//       returns: "i32"
//   })
//
//   // Memory operations
//   let ptr = Ffi.alloc(256)
//   Ffi.writeI32(ptr, 42)
//   let val = Ffi.readI32(ptr)
//   Ffi.free(ptr)
//
//   // Callbacks
//   let cb = Ffi.Callback({
//       args: ["i32", "i32"],
//       returns: "i32",
//       fn: |a, b| a + b
//   })
//   lib.call("register_callback", {
//       args: [{ type: "ptr", value: cb.ptr() }],
//       returns: "void"
//   })
//   cb.release()

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
    /// Parse type from string - NO ALIASES, only canonical names
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "void" => Ok(CType::Void),
            "i8" => Ok(CType::I8),
            "u8" => Ok(CType::U8),
            "i16" => Ok(CType::I16),
            "u16" => Ok(CType::U16),
            "i32" => Ok(CType::I32),
            "u32" => Ok(CType::U32),
            "i64" => Ok(CType::I64),
            "u64" => Ok(CType::U64),
            "f32" => Ok(CType::F32),
            "f64" => Ok(CType::F64),
            "ptr" => Ok(CType::Pointer),
            "cstr" => Ok(CType::CString),
            _ => Err(format!(
                "Unknown FFI type: '{}'. Valid types: void, i8, u8, i16, u16, i32, u32, i64, u64, f32, f64, ptr, cstr",
                s
            )),
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

// ==================== FFI Library ====================

struct FfiLibrary {
    library: Library,
    _path: String,
}

// ==================== Thread-safe pointer wrapper ====================

#[derive(Clone, Copy)]
struct SendPtr(*mut ());
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

#[derive(Clone, Copy)]
struct SendConstPtr(*const ());
unsafe impl Send for SendConstPtr {}
unsafe impl Sync for SendConstPtr {}

// ==================== Callback Registry ====================

struct CallbackInfo {
    func: Value,
    arg_types: Vec<CType>,
    return_type: CType,
}

static CALLBACK_REGISTRY: RwLock<Option<HashMap<i64, CallbackInfo>>> = RwLock::new(None);
static NEXT_CALLBACK_ID: Mutex<i64> = Mutex::new(1);

static GLOBAL_CALLER_PTR: Mutex<Vec<SendPtr>> = Mutex::new(Vec::new());
static GLOBAL_CALLER_VTABLE: Mutex<Vec<SendConstPtr>> = Mutex::new(Vec::new());

// Track allocation sizes for proper deallocation
static ALLOCATION_SIZES: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);

fn init_allocations() {
    let mut allocs = ALLOCATION_SIZES.lock().unwrap();
    if allocs.is_none() {
        *allocs = Some(HashMap::new());
    }
}

fn init_registry() {
    let mut reg = CALLBACK_REGISTRY.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
}

// Storage for libffi closures - must be kept alive while callback is in use
static CLOSURE_REGISTRY: RwLock<Option<HashMap<i64, ClosureData>>> = RwLock::new(None);

struct ClosureData {
    #[allow(dead_code)]
    cif: Cif,
    #[allow(dead_code)]
    closure: libffi::middle::Closure<'static>,
    #[allow(dead_code)]
    code_ptr: usize,  // Store as usize to avoid lifetime issues
    callback_id_ptr: *mut i64,  // Pointer to the leaked Box<i64> for cleanup
}

impl Drop for ClosureData {
    fn drop(&mut self) {
        // Reclaim the leaked Box<i64>
        if !self.callback_id_ptr.is_null() {
            unsafe {
                let _ = Box::from_raw(self.callback_id_ptr);
            }
        }
    }
}

// Ensure ClosureData is Send+Sync  
unsafe impl Send for ClosureData {}
unsafe impl Sync for ClosureData {}

fn init_closure_registry() {
    let mut reg = CLOSURE_REGISTRY.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
}

/// Generic callback handler that libffi closures call
/// This function is called by the trampoline with raw args
extern "C" fn closure_handler(
    _cif: &libffi::low::ffi_cif,
    result: &mut u64,
    args: *const *const c_void,
    userdata: &i64,
) {
    let callback_id = *userdata;
    
    // Get callback info
    let (callback_fn, arg_types, return_type) = {
        let reg = CALLBACK_REGISTRY.read().unwrap();
        if let Some(ref map) = *reg {
            if let Some(info) = map.get(&callback_id) {
                (info.func.clone(), info.arg_types.clone(), info.return_type.clone())
            } else {
                eprintln!("[FFI] Callback {} not found", callback_id);
                *result = 0;
                return;
            }
        } else {
            eprintln!("[FFI] Callback registry not initialized");
            *result = 0;
            return;
        }
    };
    
    // Get global caller from top of stack
    let (data_ptr, vtable_ptr) = {
        let ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
        let vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
        match (ptr_guard.last(), vtable_guard.last()) {
            (Some(SendPtr(p)), Some(SendConstPtr(v))) => (*p, *v),
            _ => {
                eprintln!("[FFI] No active caller for callback");
                *result = 0;
                return;
            }
        }
    };
    
    let caller: &mut dyn ValueCaller = unsafe {
        let fat_ptr: [*const (); 2] = [data_ptr as *const (), vtable_ptr];
        let trait_ptr = std::ptr::read(&fat_ptr as *const _ as *const *mut dyn ValueCaller);
        &mut *trait_ptr
    };
    
    // Convert C args to Sald Values based on arg_types
    let mut sald_args = Vec::with_capacity(arg_types.len());
    for (i, arg_type) in arg_types.iter().enumerate() {
        let arg_ptr = unsafe { *args.add(i) };
        let value = match arg_type {
            CType::I8 => Value::Number(unsafe { *(arg_ptr as *const i8) } as f64),
            CType::U8 => Value::Number(unsafe { *(arg_ptr as *const u8) } as f64),
            CType::I16 => Value::Number(unsafe { *(arg_ptr as *const i16) } as f64),
            CType::U16 => Value::Number(unsafe { *(arg_ptr as *const u16) } as f64),
            CType::I32 => Value::Number(unsafe { *(arg_ptr as *const i32) } as f64),
            CType::U32 => Value::Number(unsafe { *(arg_ptr as *const u32) } as f64),
            CType::I64 => Value::Number(unsafe { *(arg_ptr as *const i64) } as f64),
            CType::U64 => Value::Number(unsafe { *(arg_ptr as *const u64) } as f64),
            CType::F32 => Value::Number(unsafe { *(arg_ptr as *const f32) } as f64),
            CType::F64 => Value::Number(unsafe { *(arg_ptr as *const f64) }),
            CType::Pointer => Value::Number(unsafe { *(arg_ptr as *const usize) } as f64),
            CType::CString => {
                let ptr = unsafe { *(arg_ptr as *const *const i8) };
                if ptr.is_null() {
                    Value::Null
                } else {
                    let cstr = unsafe { CStr::from_ptr(ptr) };
                    Value::String(Arc::new(cstr.to_string_lossy().to_string()))
                }
            }
            CType::Void => Value::Null,
        };
        sald_args.push(value);
    }
    
    // Call the Sald function
    match caller.call(&callback_fn, sald_args) {
        Ok(ret_val) => {
            // Convert return value based on return_type
            *result = match (&return_type, &ret_val) {
                (CType::Void, _) => 0,
                (CType::I8, Value::Number(n)) => *n as i8 as u64,
                (CType::U8, Value::Number(n)) => *n as u8 as u64,
                (CType::I16, Value::Number(n)) => *n as i16 as u64,
                (CType::U16, Value::Number(n)) => *n as u16 as u64,
                (CType::I32, Value::Number(n)) => *n as i32 as u64,
                (CType::U32, Value::Number(n)) => *n as u32 as u64,
                (CType::I64, Value::Number(n)) => *n as i64 as u64,
                (CType::U64, Value::Number(n)) => *n as u64,
                (CType::F32, Value::Number(n)) => (*n as f32).to_bits() as u64,
                (CType::F64, Value::Number(n)) => n.to_bits(),
                (CType::Pointer, Value::Number(n)) => *n as usize as u64,
                (_, Value::Boolean(b)) => if *b { 1 } else { 0 },
                (_, Value::Null) => 0,
                _ => 0,
            };
        }
        Err(e) => {
            eprintln!("[FFI] Callback error: {}", e);
            *result = 0;
        }
    }
}

fn register_callback(func: Value, arg_types: Vec<CType>, return_type: CType) -> i64 {
    init_registry();
    let mut id_guard = NEXT_CALLBACK_ID.lock().unwrap();
    let id = *id_guard;
    *id_guard += 1;
    let mut reg = CALLBACK_REGISTRY.write().unwrap();
    if let Some(ref mut map) = *reg {
        map.insert(id, CallbackInfo { func, arg_types, return_type });
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
            return map.get(&id).map(|info| info.func.clone());
        }
    }
    None
}

fn set_global_caller(caller: &mut dyn ValueCaller) {
    let ptr = caller as *mut dyn ValueCaller;
    let data_ptr = ptr as *mut () as *mut ();
    let vtable_ptr = unsafe {
        let fat_ptr_bytes = &ptr as *const _ as *const [*const (); 2];
        (*fat_ptr_bytes)[1] as *const ()
    };

    // Push onto stack for nested/reentrant call support
    let mut ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
    let mut vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
    ptr_guard.push(SendPtr(data_ptr));
    vtable_guard.push(SendConstPtr(vtable_ptr));
}

fn clear_global_caller() {
    // Pop from stack (restore previous caller if any)
    let mut ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
    let mut vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
    ptr_guard.pop();
    vtable_guard.pop();
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

    let (data_ptr, vtable_ptr) = {
        let ptr_guard = GLOBAL_CALLER_PTR.lock().unwrap();
        let vtable_guard = GLOBAL_CALLER_VTABLE.lock().unwrap();
        match (ptr_guard.last(), vtable_guard.last()) {
            (Some(SendPtr(p)), Some(SendConstPtr(v))) => (*p, *v),
            _ => {
                eprintln!("[FFI] No active caller for callback");
                return 0;
            }
        }
    };

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
    // Note: CString is handled separately via pre-allocation in call_with_types
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
        (Value::String(_), CType::CString) => {
            // CStrings are pre-allocated in call_with_types, this path should not be reached
            return Err("CString conversion should be handled via pre-allocation".to_string());
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
    members.insert("open".to_string(), Value::NativeFunction {
        func: ffi_open,
        class_name: "Ffi".into(),
    });

    // Memory operations
    members.insert("alloc".to_string(), Value::NativeFunction {
        func: ffi_alloc,
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
    members.insert("writeString".to_string(), Value::NativeFunction {
        func: ffi_write_string,
        class_name: "Ffi".into(),
    });

    // Pointer operations
    members.insert("offset".to_string(), Value::NativeFunction {
        func: ffi_offset,
        class_name: "Ffi".into(),
    });
    members.insert("sizeof".to_string(), Value::NativeFunction {
        func: ffi_sizeof,
        class_name: "Ffi".into(),
    });

    // Constants
    members.insert("NULL".to_string(), Value::Number(0.0));

    // Classes
    members.insert("Library".to_string(), Value::Class(Arc::new(create_library_class())));
    members.insert("Callback".to_string(), Value::Class(Arc::new(create_callback_class())));

    Value::Namespace {
        name: "Ffi".to_string(),
        members: Arc::new(Mutex::new(members)),
        module_globals: None,
    }
}

// ==================== Memory Operations ====================

fn ffi_alloc(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.alloc expects 1 argument (size)".to_string());
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
        
        // Track allocation size for proper deallocation
        init_allocations();
        if let Ok(mut allocs) = ALLOCATION_SIZES.lock() {
            if let Some(ref mut map) = *allocs {
                map.insert(ptr as usize, size);
            }
        }
        
        Ok(Value::Number(ptr as usize as f64))
    }
}

fn ffi_free(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.free expects 1 argument (pointer)".to_string());
    }
    let ptr_val = match &args[0] {
        Value::Number(n) => *n as usize,
        _ => return Err("Pointer must be a number".to_string()),
    };
    
    if ptr_val == 0 {
        return Ok(Value::Null); // Ignore null pointer
    }
    
    // Get allocation size and free
    let size = {
        let mut allocs = ALLOCATION_SIZES.lock().unwrap();
        if let Some(ref mut map) = *allocs {
            map.remove(&ptr_val)
        } else {
            None
        }
    };
    
    if let Some(size) = size {
        unsafe {
            let layout = Layout::from_size_align(size, 8)
                .map_err(|_| "Invalid layout for deallocation")?;
            std::alloc::dealloc(ptr_val as *mut u8, layout);
        }
    }
    // If size not found, memory was not allocated by us - silently ignore
    
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

fn ffi_write_string(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("Ffi.writeString expects 2 arguments (pointer, string)".to_string());
    }
    let ptr = match &args[0] {
        Value::Number(n) => *n as usize as *mut u8,
        _ => return Err("Pointer must be a number".to_string()),
    };
    let s = match &args[1] {
        Value::String(s) => s.as_str(),
        _ => return Err("Value must be a string".to_string()),
    };
    if ptr.is_null() {
        return Err("Cannot write to null pointer".to_string());
    }
    let c_str = CString::new(s).map_err(|_| "Invalid C string (contains null byte)")?;
    let bytes = c_str.as_bytes_with_nul();
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    Ok(Value::Null)
}

// ==================== Pointer Operations ====================

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

// ==================== Library Loading ====================

fn ffi_open(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.open expects 1 argument (path)".to_string());
    }
    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Path must be a string, got {}", args[0].type_name())),
    };

    let resolved_path = crate::resolve_script_path(&path);
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

    instance_methods.insert("symbol".to_string(), library_symbol);
    instance_methods.insert("close".to_string(), library_close);
    instance_methods.insert("path".to_string(), library_path);

    callable_methods.insert("call".to_string(), library_call);

    let mut class = Class::new_with_instance("Library", instance_methods, None);
    class.callable_native_instance_methods = callable_methods;
    class
}

// ==================== Library Methods ====================

/// Unified lib.call() with explicit types
///
/// Usage:
///   lib.call("func_name", {
///       args: [{ type: "i32", value: 5 }, { type: "i32", value: 10 }],
///       returns: "i32"
///   })
fn library_call(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("lib.call expects 2 arguments (function_name, options)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Function name must be a string, got {}", args[0].type_name())),
    };

    // Parse options dict
    let options = if args.len() > 1 {
        match &args[1] {
            Value::Dictionary(dict) => dict.lock().unwrap().clone(),
            _ => return Err("Second argument must be an options dictionary".to_string()),
        }
    } else {
        return Err("lib.call requires options dictionary with 'args' and 'returns'".to_string());
    };

    // Parse return type
    let return_type_str = match options.get("returns") {
        Some(Value::String(s)) => s.to_string(),
        Some(_) => return Err("'returns' must be a string".to_string()),
        None => "void".to_string(),
    };
    let return_type = CType::from_str(&return_type_str)?;

    // Parse arguments
    let (call_values, arg_types) = match options.get("args") {
        Some(Value::Array(arr)) => {
            let arr_guard = arr.lock().unwrap();
            let mut values = Vec::new();
            let mut types = Vec::new();

            for (idx, arg_dict) in arr_guard.iter().enumerate() {
                match arg_dict {
                    Value::Dictionary(d) => {
                        let d_guard = d.lock().unwrap();
                        
                        let type_str = match d_guard.get("type") {
                            Some(Value::String(s)) => s.to_string(),
                            Some(_) => return Err(format!("args[{}].type must be a string", idx)),
                            None => return Err(format!("args[{}] missing 'type' field", idx)),
                        };
                        
                        let value = match d_guard.get("value") {
                            Some(v) => v.clone(),
                            None => return Err(format!("args[{}] missing 'value' field", idx)),
                        };

                        types.push(CType::from_str(&type_str)?);
                        values.push(value);
                    }
                    _ => return Err(format!("args[{}] must be a dictionary with 'type' and 'value'", idx)),
                }
            }

            (values, types)
        }
        Some(_) => return Err("'args' must be an array".to_string()),
        None => (Vec::new(), Vec::new()),
    };

    // Always set global caller for ALL FFI calls - C code can invoke callbacks at any time
    set_global_caller(caller);

    let result = if let Value::Instance(inst) = recv {
        // Get handle pointer and release Instance lock immediately
        let lib_ptr = {
            let inst_guard = inst.lock().unwrap();
            if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
                let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
                if ptr.is_null() {
                    return Err("Library has been closed".to_string());
                }
                ptr
            } else {
                return Err("Invalid library instance".to_string());
            }
            // inst_guard is dropped here, releasing the Instance lock
        };

        unsafe {
            let lib_mutex = &*lib_ptr;
            
            // Get function pointer while holding the library lock
            let func_ptr = {
                let lib_guard = lib_mutex.lock().unwrap();

                let fn_name_c = CString::new(fn_name.as_str())
                    .map_err(|_| "Invalid function name")?;

                let func_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(fn_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                *func_ptr
                // lib_guard is dropped here, releasing the library lock
            };

            // Call FFI function WITHOUT holding ANY locks
            // This allows nested/reentrant calls from callbacks
            call_with_types(func_ptr, &call_values, &arg_types, &return_type)
        }
    } else {
        Err("Invalid library instance".to_string())
    };

    clear_global_caller();

    result
}

fn library_symbol(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("lib.symbol expects 1 argument (symbol_name)".to_string());
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

// ==================== Callback Class ====================

fn create_callback_class() -> Class {
    let mut instance_methods: HashMap<String, NativeInstanceFn> = HashMap::new();

    instance_methods.insert("ptr".to_string(), callback_ptr);
    instance_methods.insert("id".to_string(), callback_id);
    instance_methods.insert("release".to_string(), callback_release);

    // Use new_with_instance for the instance methods and set constructor for Ffi.Callback() calls
    let mut class = Class::new_with_instance("Callback", instance_methods, Some(callback_new));
    
    // Also add as static method for Ffi.Callback.new() style calls
    class.native_static_methods.insert("new".to_string(), callback_new);
    
    class
}

/// Create a new callback
///
/// Usage:
///   let cb = Ffi.Callback({
///       args: ["i32", "i32"],
///       returns: "i32",
///       fn: |a, b| a + b
///   })
fn callback_new(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.Callback expects 1 argument (options dictionary)".to_string());
    }

    let options = match &args[0] {
        Value::Dictionary(dict) => dict.lock().unwrap().clone(),
        _ => return Err("Ffi.Callback expects an options dictionary".to_string()),
    };

    // Parse argument types
    let arg_types: Vec<CType> = match options.get("args") {
        Some(Value::Array(arr)) => {
            let arr_guard = arr.lock().unwrap();
            let mut types = Vec::new();
            for (idx, t) in arr_guard.iter().enumerate() {
                match t {
                    Value::String(s) => types.push(CType::from_str(s)?),
                    _ => return Err(format!("args[{}] must be a type string", idx)),
                }
            }
            types
        }
        Some(_) => return Err("'args' must be an array of type strings".to_string()),
        None => Vec::new(),
    };

    // Parse return type
    let return_type = match options.get("returns") {
        Some(Value::String(s)) => CType::from_str(s)?,
        Some(_) => return Err("'returns' must be a type string".to_string()),
        None => CType::Void,
    };

    // Get the function
    let func = match options.get("fn") {
        Some(f @ Value::Function(_)) => f.clone(),
        Some(_) => return Err("'fn' must be a function".to_string()),
        None => return Err("Missing 'fn' field in Callback options".to_string()),
    };

    // Register callback in callback registry
    let callback_id = register_callback(func, arg_types.clone(), return_type.clone());
    
    // Build libffi Cif with the correct argument types
    let ffi_arg_types: Vec<FfiType> = arg_types.iter().map(|t| t.to_ffi_type()).collect();
    let ffi_return_type = return_type.to_ffi_type();
    let cif = Cif::new(ffi_arg_types, ffi_return_type);
    
    // Create the libffi closure with callback_id as userdata
    // The closure will call closure_handler when invoked
    init_closure_registry();
    
    // Box the callback_id so it has a stable address
    let callback_id_box = Box::new(callback_id);
    let callback_id_ptr = Box::into_raw(callback_id_box);  // Get raw pointer for cleanup later
    let callback_id_ref: &'static i64 = unsafe { &*callback_id_ptr };
    
    let closure = libffi::middle::Closure::new(cif.clone(), closure_handler, callback_id_ref);
    // Get the executable code pointer from the closure
    // closure.code_ptr() returns &unsafe extern "C" fn() - we need the address of that function
    let code_ptr_value = unsafe {
        let fn_ptr: unsafe extern "C" fn() = *closure.code_ptr();
        std::mem::transmute::<unsafe extern "C" fn(), usize>(fn_ptr)
    };
    
    // Store closure data to keep it alive
    {
        let mut reg = CLOSURE_REGISTRY.write().unwrap();
        if let Some(ref mut map) = *reg {
            map.insert(callback_id, ClosureData {
                cif,
                closure,
                code_ptr: code_ptr_value,
                callback_id_ptr,  // Track for cleanup in Drop
            });
        }
    }

    // Create callback instance
    let callback_class = Arc::new(create_callback_class());
    let mut instance = Instance::new(callback_class);

    instance.fields.insert("_id".to_string(), Value::Number(callback_id as f64));
    instance.fields.insert("_code_ptr".to_string(), Value::Number(code_ptr_value as f64));
    instance.fields.insert("_released".to_string(), Value::Boolean(false));

    Ok(Value::Instance(Arc::new(Mutex::new(instance))))
}

/// Get the pointer to pass to C code
fn callback_ptr(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        
        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Err("Callback has been released".to_string());
        }

        // Return the closure's code pointer - this is the trampoline address
        if let Some(Value::Number(code_ptr)) = inst_guard.fields.get("_code_ptr") {
            return Ok(Value::Number(*code_ptr));
        }
    }
    Err("Invalid callback instance".to_string())
}

/// Get the callback ID
fn callback_id(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        
        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Err("Callback has been released".to_string());
        }

        if let Some(id) = inst_guard.fields.get("_id") {
            return Ok(id.clone());
        }
    }
    Err("Invalid callback instance".to_string())
}

/// Release the callback (unregister from registry)
fn callback_release(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let mut inst_guard = inst.lock().unwrap();
        
        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Ok(Value::Null); // Already released
        }

        if let Some(Value::Number(id)) = inst_guard.fields.get("_id") {
            let callback_id = *id as i64;
            unregister_callback(callback_id);
            
            // Also remove from closure registry to free the closure
            if let Ok(mut reg) = CLOSURE_REGISTRY.write() {
                if let Some(ref mut map) = *reg {
                    map.remove(&callback_id);
                }
            }
        }

        inst_guard.fields.insert("_released".to_string(), Value::Boolean(true));
        return Ok(Value::Null);
    }
    Err("Invalid callback instance".to_string())
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

    // FIRST: Pre-allocate all CStrings to ensure they live long enough
    // This vector owns all the CStrings for the duration of the call
    let mut cstrings: Vec<CString> = Vec::new();
    for (val, typ) in values.iter().zip(arg_types.iter()) {
        if *typ == CType::CString {
            if let Value::String(s) = val {
                let c_str = CString::new(s.as_str())
                    .map_err(|_| "Invalid C string (contains null byte)")?;
                cstrings.push(c_str);
            }
        }
    }

    // Convert values, using pre-allocated CStrings
    let mut cstring_idx = 0;
    let mut converted_args: Vec<ConvertedArg> = Vec::new();
    for (idx, (val, typ)) in values.iter().zip(arg_types.iter()).enumerate() {
        let converted = match val {
            // Handle callback instance - extract the id for passing to C
            Value::Instance(inst) => {
                let inst_guard = inst.lock().unwrap();
                if let Some(Value::Number(id)) = inst_guard.fields.get("_id") {
                    ConvertedArg {
                        ffi_type: FfiType::u64(),
                        data: ConvertedData::I64(*id as i64),
                    }
                } else {
                    return Err(format!("args[{}]: Invalid callback instance", idx));
                }
            }
            Value::String(_) if *typ == CType::CString => {
                // Use pointer to pre-allocated CString
                let ptr = cstrings[cstring_idx].as_ptr() as usize;
                cstring_idx += 1;
                ConvertedArg {
                    ffi_type: typ.to_ffi_type(),
                    data: ConvertedData::Ptr(ptr),
                }
            }
            _ => convert_value_to_arg(val, typ)?,
        };
        converted_args.push(converted);
    }

    let ffi_types: Vec<FfiType> = converted_args.iter().map(|a| a.ffi_type.clone()).collect();

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
            // Note: CString handled via pre-allocation, ConvertedData::CStr removed
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
