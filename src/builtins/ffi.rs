// FFI Built-in Namespace - Dynamic Foreign Function Interface with Callback Support
// Uses libffi for dynamic calls and supports callbacks from native code to Sald
//
// Usage:
//   let lib = Ffi.load("./path/to/library.so")
//   let result = lib.call("function_name", arg1, arg2, ..., argN)
//   
//   // Callbacks:
//   lib.callWithCallback("native_func", callback_fn, ...args)
//   
//   lib.close()

use crate::vm::caller::ValueCaller;
use crate::vm::value::{Class, Instance, NativeInstanceFn, Value};
use libffi::middle::{Arg, Cif, CodePtr, Type};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::sync::{Arc, Mutex, RwLock};

/// FFI Library handle
struct FfiLibrary {
    library: Library,
    _path: String,
}

// ==================== Callback Registry ====================

/// Global registry for callbacks - maps callback ID to Sald function
static CALLBACK_REGISTRY: RwLock<Option<HashMap<i64, Value>>> = RwLock::new(None);

/// Counter for generating unique callback IDs
static NEXT_CALLBACK_ID: Mutex<i64> = Mutex::new(1);

/// Global caller storage - using Mutex for cross-thread access
/// Stores fat pointer as raw parts (data ptr, vtable ptr)
/// This allows the callback trampoline to access the caller from any thread
static GLOBAL_CALLER: Mutex<Option<[usize; 2]>> = Mutex::new(None);

/// Initialize the callback registry if needed
fn init_registry() {
    let mut reg = CALLBACK_REGISTRY.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
}

/// Register a Sald function as a callback, returns unique ID
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

/// Unregister a callback by ID
fn unregister_callback(id: i64) {
    if let Ok(mut reg) = CALLBACK_REGISTRY.write() {
        if let Some(ref mut map) = *reg {
            map.remove(&id);
        }
    }
}

/// Get a callback by ID
fn get_callback(id: i64) -> Option<Value> {
    if let Ok(reg) = CALLBACK_REGISTRY.read() {
        if let Some(ref map) = *reg {
            return map.get(&id).cloned();
        }
    }
    None
}

/// Store a caller reference in global storage (as fat pointer parts)
/// WARNING: Caller must remain valid for the entire duration of FFI call!
fn set_global_caller(caller: &mut dyn ValueCaller) {
    let fat_ptr: [usize; 2] = unsafe { std::mem::transmute(caller as *mut dyn ValueCaller) };
    let mut guard = GLOBAL_CALLER.lock().unwrap();
    *guard = Some(fat_ptr);
}

/// Clear the global caller
fn clear_global_caller() {
    let mut guard = GLOBAL_CALLER.lock().unwrap();
    *guard = None;
}

/// C-callable trampoline function that native libraries can use to invoke Sald callbacks
/// Signature: i64 sald_invoke_callback(i64 callback_id, i64 arg_count, i64* args)
/// Returns: i64 result (or 0 on error)
/// 
/// IMPORTANT: This function accesses the global caller, so it works across threads.
/// The caller must be set via set_global_caller before calling native code that uses this.
#[no_mangle]
pub extern "C" fn sald_invoke_callback(callback_id: i64, arg_count: i64, args: *const i64) -> i64 {
    // Get the callback function
    let callback = match get_callback(callback_id) {
        Some(cb) => cb,
        None => {
            eprintln!("[FFI] Callback {} not found", callback_id);
            return 0;
        }
    };

    // Get the caller from global storage
    let fat_ptr = {
        let guard = GLOBAL_CALLER.lock().unwrap();
        match *guard {
            Some(ptr) => ptr,
            None => {
                eprintln!("[FFI] No active caller for callback invocation");
                return 0;
            }
        }
    };

    // Reconstruct the caller reference from fat pointer
    let caller: &mut dyn ValueCaller =
        unsafe { std::mem::transmute::<[usize; 2], &mut dyn ValueCaller>(fat_ptr) };

    // Convert C args to Sald Values
    let mut sald_args = Vec::with_capacity(arg_count as usize);
    if !args.is_null() {
        for i in 0..arg_count as usize {
            let arg_val = unsafe { *args.add(i) };
            sald_args.push(Value::Number(arg_val as f64));
        }
    }

    // Call the Sald function
    match caller.call(&callback, sald_args) {
        Ok(result) => {
            // Convert result back to i64
            match result {
                Value::Number(n) => n as i64,
                Value::Boolean(b) => {
                    if b {
                        1
                    } else {
                        0
                    }
                }
                Value::Null => 0,
                _ => 0,
            }
        }
        Err(e) => {
            eprintln!("[FFI] Callback error: {}", e);
            0
        }
    }
}

/// Get pointer to the sald_invoke_callback function (for passing to native code)
#[no_mangle]
pub extern "C" fn sald_get_callback_invoker() -> *const c_void {
    sald_invoke_callback as *const c_void
}

// ==================== Ffi Namespace ====================

/// Create the Ffi namespace
pub fn create_ffi_namespace() -> Value {
    let mut members: HashMap<String, Value> = HashMap::new();

    members.insert(
        "load".to_string(),
        Value::NativeFunction {
            func: ffi_load,
            class_name: "Ffi".into(),
        },
    );

    members.insert(
        "callback".to_string(),
        Value::NativeFunction {
            func: ffi_create_callback,
            class_name: "Ffi".into(),
        },
    );

    members.insert(
        "removeCallback".to_string(),
        Value::NativeFunction {
            func: ffi_remove_callback,
            class_name: "Ffi".into(),
        },
    );

    // Read a C string from a pointer
    members.insert(
        "readString".to_string(),
        Value::NativeFunction {
            func: ffi_read_string,
            class_name: "Ffi".into(),
        },
    );

    members.insert("NULL".to_string(), Value::Number(0.0));

    // Export the invoker function pointer for native libraries
    members.insert(
        "INVOKER".to_string(),
        Value::Number(sald_get_callback_invoker() as usize as f64),
    );

    members.insert(
        "Library".to_string(),
        Value::Class(Arc::new(create_library_class())),
    );

    Value::Namespace {
        name: "Ffi".to_string(),
        members: Arc::new(Mutex::new(members)),
    }
}

/// Ffi.callback(fn) - Register a Sald function as a callback
/// Returns a dictionary with { id: callback_id, invoker: fn_ptr }
fn ffi_create_callback(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.callback expects 1 argument (function)".to_string());
    }

    let func = &args[0];
    
    // Validate that it's a callable
    match func {
        Value::Function(_) => {}
        _ => return Err(format!("Ffi.callback expects a function, got {}", func.type_name())),
    }

    let callback_id = register_callback(func.clone());
    
    // Return dictionary with callback info
    let mut result = HashMap::new();
    result.insert("id".to_string(), Value::Number(callback_id as f64));
    result.insert(
        "invoker".to_string(),
        Value::Number(sald_get_callback_invoker() as usize as f64),
    );

    Ok(Value::Dictionary(Arc::new(Mutex::new(result))))
}

/// Ffi.removeCallback(id) - Unregister a callback
fn ffi_remove_callback(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.removeCallback expects 1 argument (callback_id)".to_string());
    }

    let id = match &args[0] {
        Value::Number(n) => *n as i64,
        Value::Dictionary(dict) => {
            // Also accept { id: ... } dictionary
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

/// Ffi.readString(ptr) - Read a C string from a pointer
/// Returns the string, or empty string if invalid
fn ffi_read_string(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.readString expects 1 argument (pointer)".to_string());
    }

    let ptr = match &args[0] {
        Value::Number(n) => *n as usize as *const std::os::raw::c_char,
        _ => return Err(format!("Pointer must be a number, got {}", args[0].type_name())),
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

/// Ffi.load(path) - Load a dynamic library
fn ffi_load(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.load expects 1 argument (path)".to_string());
    }

    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => return Err(format!("Path must be a string, got {}", args[0].type_name())),
    };

    // Auto-add extension if missing
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

    // Resolve path relative to current script directory
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

fn create_library_class() -> Class {
    let mut instance_methods: HashMap<String, NativeInstanceFn> = HashMap::new();
    let mut callable_methods: HashMap<String, crate::vm::caller::CallableNativeInstanceFn> =
        HashMap::new();

    instance_methods.insert("call".to_string(), library_call);
    instance_methods.insert("close".to_string(), library_close);
    instance_methods.insert("path".to_string(), library_path);

    // callWithCallback needs ValueCaller to invoke callbacks
    callable_methods.insert("callWithCallback".to_string(), library_call_with_callback);

    let mut class = Class::new_with_instance("Library", instance_methods, None);
    class.callable_native_instance_methods = callable_methods;
    class
}

/// Dynamic FFI call using libffi
fn library_call(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("library.call expects at least 1 argument (function name)".to_string());
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Function name must be a string, got {}",
                args[0].type_name()
            ))
        }
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

                let fn_name_c =
                    CString::new(fn_name.as_str()).map_err(|_| "Invalid function name")?;

                let func_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(fn_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                let func_ptr = *func_ptr;

                let result = call_dynamic(func_ptr, ffi_args)?;
                return Ok(Value::Number(result as f64));
            }
        }
    }

    Err("Invalid library instance".to_string())
}

/// library.callWithCallback(fn_name, callback, ...args)
/// Sets up the caller context so callbacks can invoke Sald functions
fn library_call_with_callback(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err(
            "library.callWithCallback expects at least 2 arguments (function name, callback)"
                .to_string(),
        );
    }

    let fn_name = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Function name must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    // Register the callback
    let callback = &args[1];
    let callback_id = match callback {
        Value::Function(_) => register_callback(callback.clone()),
        Value::Dictionary(dict) => {
            // Already registered callback
            let guard = dict.lock().unwrap();
            match guard.get("id") {
                Some(Value::Number(n)) => *n as i64,
                _ => return Err("Invalid callback object".to_string()),
            }
        }
        _ => return Err(format!("Callback must be a function, got {}", callback.type_name())),
    };

    let ffi_args = &args[2..];

    // Set up the global caller for cross-thread callback access
    set_global_caller(caller);

    // Make the FFI call
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

                    let fn_name_c =
                        CString::new(fn_name.as_str()).map_err(|_| "Invalid function name")?;

                    let func_ptr: Symbol<*const c_void> = lib_guard
                        .library
                        .get(fn_name_c.as_bytes_with_nul())
                        .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                    let func_ptr = *func_ptr;

                    // Build args including callback_id and invoker pointer
                    let mut all_args = Vec::with_capacity(ffi_args.len() + 2);
                    all_args.push(Value::Number(callback_id as f64));
                    all_args.push(Value::Number(sald_get_callback_invoker() as usize as f64));
                    all_args.extend_from_slice(ffi_args);

                    let result = call_dynamic(func_ptr, &all_args)?;
                    Ok(Value::Number(result as f64))
                }
            }
        } else {
            Err("Invalid library instance".to_string())
        }
    } else {
        Err("Invalid library instance".to_string())
    };

    // Clear the global caller
    clear_global_caller();

    // Clean up the callback if it was auto-registered
    if matches!(callback, Value::Function(_)) {
        unregister_callback(callback_id);
    }

    result
}

/// Helper: Call a function dynamically with libffi
unsafe fn call_dynamic(func_ptr: *const c_void, ffi_args: &[Value]) -> Result<i64, String> {
    let mut arg_types: Vec<Type> = Vec::with_capacity(ffi_args.len());
    let mut c_strings: Vec<CString> = Vec::new();
    let mut i64_values: Vec<i64> = Vec::new();
    let mut ptr_values: Vec<*const i8> = Vec::new();

    // First pass: determine types and collect values
    for arg in ffi_args.iter() {
        match arg {
            Value::Number(n) => {
                arg_types.push(Type::i64());
                i64_values.push(*n as i64);
            }
            Value::String(s) => {
                arg_types.push(Type::pointer());
                let c_str =
                    CString::new(s.as_str()).map_err(|_| "Invalid string argument")?;
                c_strings.push(c_str);
            }
            Value::Boolean(b) => {
                arg_types.push(Type::i64());
                i64_values.push(if *b { 1 } else { 0 });
            }
            Value::Null => {
                arg_types.push(Type::i64());
                i64_values.push(0);
            }
            Value::Dictionary(dict) => {
                // For callback dictionaries, pass the ID
                let guard = dict.lock().unwrap();
                if let Some(Value::Number(id)) = guard.get("id") {
                    arg_types.push(Type::i64());
                    i64_values.push(*id as i64);
                } else {
                    return Err("Cannot pass dictionary to FFI".to_string());
                }
            }
            _ => {
                return Err(format!(
                    "Unsupported FFI argument type: {}",
                    arg.type_name()
                ));
            }
        }
    }

    // Get pointers for strings
    for c_str in c_strings.iter() {
        ptr_values.push(c_str.as_ptr());
    }

    // Build libffi Arguments
    let mut libffi_args: Vec<Arg> = Vec::with_capacity(ffi_args.len());
    let mut i64_idx = 0;
    let mut ptr_idx = 0;

    for arg in ffi_args.iter() {
        match arg {
            Value::String(_) => {
                libffi_args.push(Arg::new(&ptr_values[ptr_idx]));
                ptr_idx += 1;
            }
            _ => {
                libffi_args.push(Arg::new(&i64_values[i64_idx]));
                i64_idx += 1;
            }
        }
    }

    // Create CIF and call
    let cif = Cif::new(arg_types.into_iter(), Type::i64());
    let code_ptr = CodePtr::from_ptr(func_ptr as *const _);

    let result: i64 = cif.call(code_ptr, &libffi_args);

    Ok(result)
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
