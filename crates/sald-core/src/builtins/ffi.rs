































use crate::vm::caller::ValueCaller;
use crate::vm::value::{Class, Instance, NativeInstanceFn, Value};
use libffi::middle::{Arg, Cif, CodePtr, Type as FfiType};
use libloading::{Library, Symbol};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::alloc::{alloc, Layout};
use std::cell::RefCell;
use std::ffi::{c_void, CStr, CString};
use std::ptr;
use std::rc::Rc;



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
        match s.to_lowercase().as_str() {
            "void" => Ok(CType::Void),
            "i8" | "int8" | "char" => Ok(CType::I8),
            "u8" | "uint8" | "uchar" | "byte" => Ok(CType::U8),
            "i16" | "int16" | "short" => Ok(CType::I16),
            "u16" | "uint16" | "ushort" => Ok(CType::U16),
            "i32" | "int32" | "int" => Ok(CType::I32),
            "u32" | "uint32" | "uint" => Ok(CType::U32),
            "i64" | "int64" | "long" | "longlong" => Ok(CType::I64),
            "u64" | "uint64" | "ulong" | "ulonglong" | "size_t" => Ok(CType::U64),
            "f32" | "float" => Ok(CType::F32),
            "f64" | "double" => Ok(CType::F64),
            "ptr" | "pointer" | "void*" | "voidptr" => Ok(CType::Pointer),
            "string" | "cstring" | "char*" | "str" => Ok(CType::CString),
            _ => Err(format!("Unknown C type: {}", s)),
        }
    }

    fn to_ffi_type(&self) -> FfiType {
        match self {
            CType::Void => FfiType::void(),
            CType::I8 => FfiType::i8(),
            CType::U8 => FfiType::u8(),
            CType::I16 => FfiType::i16(),
            CType::U16 => FfiType::u16(),
            CType::I32 => FfiType::i32(),
            CType::U32 => FfiType::u32(),
            CType::I64 => FfiType::i64(),
            CType::U64 => FfiType::u64(),
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



struct FfiLibrary {
    library: Library,
    _path: String,
}



#[derive(Clone, Copy)]
struct SendPtr(*mut ());
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

#[derive(Clone, Copy)]
struct SendConstPtr(*const ());
unsafe impl Send for SendConstPtr {}
unsafe impl Sync for SendConstPtr {}



struct CallbackInfo {
    func: Value,
    arg_types: Vec<CType>,
    return_type: CType,
}


thread_local! {
    static CALLBACK_REGISTRY: RefCell<FxHashMap<i64, CallbackInfo>> = RefCell::new(FxHashMap::default());
    static NEXT_CALLBACK_ID: RefCell<i64> = const { RefCell::new(1) };
    static GLOBAL_CALLER_PTR: RefCell<Vec<SendPtr>> = RefCell::new(Vec::new());
    static GLOBAL_CALLER_VTABLE: RefCell<Vec<SendConstPtr>> = RefCell::new(Vec::new());
    static ALLOCATION_SIZES: RefCell<FxHashMap<usize, usize>> = RefCell::new(FxHashMap::default());
    static CLOSURE_REGISTRY: RefCell<FxHashMap<i64, ClosureData>> = RefCell::new(FxHashMap::default());
}


static FFI_LIB_HANDLES: Mutex<Option<FxHashMap<usize, ()>>> = Mutex::new(None);


#[allow(dead_code)]
struct ClosureData {
    cif: Cif,
    closure: libffi::middle::Closure<'static>,
    code_ptr: usize,           
    callback_id_ptr: *mut i64, 
}

impl Drop for ClosureData {
    fn drop(&mut self) {
        
        if !self.callback_id_ptr.is_null() {
            unsafe {
                let _ = Box::from_raw(self.callback_id_ptr);
            }
        }
    }
}


unsafe impl Send for ClosureData {}
unsafe impl Sync for ClosureData {}



extern "C" fn closure_handler(
    _cif: &libffi::low::ffi_cif,
    result: &mut u64,
    args: *const *const c_void,
    userdata: &i64,
) {
    let callback_id = *userdata;

    
    let callback_result = CALLBACK_REGISTRY.with(|reg| {
        let map = reg.borrow();
        map.get(&callback_id).map(|info| {
            (
                info.func.clone(),
                info.arg_types.clone(),
                info.return_type.clone(),
            )
        })
    });

    let (callback_fn, arg_types, return_type) = match callback_result {
        Some(info) => info,
        None => {
            eprintln!("[FFI] Callback {} not found", callback_id);
            *result = 0;
            return;
        }
    };

    
    let caller_result = GLOBAL_CALLER_PTR.with(|ptr_cell| {
        GLOBAL_CALLER_VTABLE.with(|vtable_cell| {
            let ptr_vec = ptr_cell.borrow();
            let vtable_vec = vtable_cell.borrow();
            match (ptr_vec.last(), vtable_vec.last()) {
                (Some(SendPtr(p)), Some(SendConstPtr(v))) => Some((*p, *v)),
                _ => None,
            }
        })
    });

    let (data_ptr, vtable_ptr) = match caller_result {
        Some(ptrs) => ptrs,
        None => {
            eprintln!("[FFI] No active caller for callback");
            *result = 0;
            return;
        }
    };

    let caller: &mut dyn ValueCaller = unsafe {
        let fat_ptr: [*const (); 2] = [data_ptr as *const (), vtable_ptr];
        let trait_ptr = std::ptr::read(&fat_ptr as *const _ as *const *mut dyn ValueCaller);
        &mut *trait_ptr
    };

    
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
                    Value::String(Rc::from(cstr.to_string_lossy().to_string()))
                }
            }
            CType::Void => Value::Null,
        };
        sald_args.push(value);
    }

    
    match caller.call(&callback_fn, sald_args) {
        Ok(ret_val) => {
            
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
                (_, Value::Boolean(b)) => {
                    if *b {
                        1
                    } else {
                        0
                    }
                }
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
    NEXT_CALLBACK_ID.with(|id_cell| {
        let mut id_ref = id_cell.borrow_mut();
        let id = *id_ref;
        *id_ref += 1;
        
        CALLBACK_REGISTRY.with(|reg_cell| {
            let mut map = reg_cell.borrow_mut();
            map.insert(
                id,
                CallbackInfo {
                    func,
                    arg_types,
                    return_type,
                },
            );
        });
        
        id
    })
}

fn unregister_callback(id: i64) {
    CALLBACK_REGISTRY.with(|reg_cell| {
        let mut map = reg_cell.borrow_mut();
        map.remove(&id);
    });
}

fn get_callback(id: i64) -> Option<Value> {
    CALLBACK_REGISTRY.with(|reg_cell| {
        let map = reg_cell.borrow();
        map.get(&id).map(|info| info.func.clone())
    })
}

fn set_global_caller(caller: &mut dyn ValueCaller) {
    let ptr = caller as *mut dyn ValueCaller;
    let data_ptr = ptr as *mut () as *mut ();
    let vtable_ptr = unsafe {
        let fat_ptr_bytes = &ptr as *const _ as *const [*const (); 2];
        (*fat_ptr_bytes)[1] as *const ()
    };

    
    GLOBAL_CALLER_PTR.with(|ptr_cell| {
        ptr_cell.borrow_mut().push(SendPtr(data_ptr));
    });
    GLOBAL_CALLER_VTABLE.with(|vtable_cell| {
        vtable_cell.borrow_mut().push(SendConstPtr(vtable_ptr));
    });
}

fn clear_global_caller() {
    GLOBAL_CALLER_PTR.with(|ptr_cell| {
        ptr_cell.borrow_mut().pop();
    });
    GLOBAL_CALLER_VTABLE.with(|vtable_cell| {
        vtable_cell.borrow_mut().pop();
    });
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

    let caller_result = GLOBAL_CALLER_PTR.with(|ptr_cell| {
        GLOBAL_CALLER_VTABLE.with(|vtable_cell| {
            let ptr_vec = ptr_cell.borrow();
            let vtable_vec = vtable_cell.borrow();
            match (ptr_vec.last(), vtable_vec.last()) {
                (Some(SendPtr(p)), Some(SendConstPtr(v))) => Some((*p, *v)),
                _ => None,
            }
        })
    });

    let (data_ptr, vtable_ptr) = match caller_result {
        Some(ptrs) => ptrs,
        None => {
            eprintln!("[FFI] No active caller for callback");
            return 0;
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
            Value::Boolean(b) => {
                if b {
                    1
                } else {
                    0
                }
            }
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
            
            return Err("CString conversion should be handled via pre-allocation".to_string());
        }
        (Value::Null, CType::Pointer | CType::CString) => ConvertedData::Ptr(0),
        _ => {
            return Err(format!(
                "Cannot convert {} to {:?}",
                value.type_name(),
                ctype
            ))
        }
    };

    Ok(ConvertedArg {
        ffi_type: ctype.to_ffi_type(),
        data,
    })
}



pub fn create_ffi_namespace() -> Value {
    let mut members: FxHashMap<String, Value> = FxHashMap::default();

    
    members.insert(
        "open".to_string(),
        Value::NativeFunction {
            func: ffi_open,
            class_name: "Ffi".into(),
        },
    );

    
    members.insert(
        "alloc".to_string(),
        Value::NativeFunction {
            func: ffi_alloc,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "free".to_string(),
        Value::NativeFunction {
            func: ffi_free,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "memcpy".to_string(),
        Value::NativeFunction {
            func: ffi_memcpy,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "memset".to_string(),
        Value::NativeFunction {
            func: ffi_memset,
            class_name: "Ffi".into(),
        },
    );

    
    members.insert(
        "readI8".to_string(),
        Value::NativeFunction {
            func: ffi_read_i8,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readU8".to_string(),
        Value::NativeFunction {
            func: ffi_read_u8,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readI16".to_string(),
        Value::NativeFunction {
            func: ffi_read_i16,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readU16".to_string(),
        Value::NativeFunction {
            func: ffi_read_u16,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readI32".to_string(),
        Value::NativeFunction {
            func: ffi_read_i32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readU32".to_string(),
        Value::NativeFunction {
            func: ffi_read_u32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readI64".to_string(),
        Value::NativeFunction {
            func: ffi_read_i64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readU64".to_string(),
        Value::NativeFunction {
            func: ffi_read_u64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readF32".to_string(),
        Value::NativeFunction {
            func: ffi_read_f32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readF64".to_string(),
        Value::NativeFunction {
            func: ffi_read_f64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readPtr".to_string(),
        Value::NativeFunction {
            func: ffi_read_ptr,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "readString".to_string(),
        Value::NativeFunction {
            func: ffi_read_string,
            class_name: "Ffi".into(),
        },
    );

    
    members.insert(
        "writeI8".to_string(),
        Value::NativeFunction {
            func: ffi_write_i8,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeU8".to_string(),
        Value::NativeFunction {
            func: ffi_write_u8,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeI16".to_string(),
        Value::NativeFunction {
            func: ffi_write_i16,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeU16".to_string(),
        Value::NativeFunction {
            func: ffi_write_u16,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeI32".to_string(),
        Value::NativeFunction {
            func: ffi_write_i32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeU32".to_string(),
        Value::NativeFunction {
            func: ffi_write_u32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeI64".to_string(),
        Value::NativeFunction {
            func: ffi_write_i64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeU64".to_string(),
        Value::NativeFunction {
            func: ffi_write_u64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeF32".to_string(),
        Value::NativeFunction {
            func: ffi_write_f32,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeF64".to_string(),
        Value::NativeFunction {
            func: ffi_write_f64,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writePtr".to_string(),
        Value::NativeFunction {
            func: ffi_write_ptr,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "writeString".to_string(),
        Value::NativeFunction {
            func: ffi_write_string,
            class_name: "Ffi".into(),
        },
    );

    
    members.insert(
        "offset".to_string(),
        Value::NativeFunction {
            func: ffi_offset,
            class_name: "Ffi".into(),
        },
    );
    members.insert(
        "sizeof".to_string(),
        Value::NativeFunction {
            func: ffi_sizeof,
            class_name: "Ffi".into(),
        },
    );

    
    members.insert("NULL".to_string(), Value::Number(0.0));

    
    members.insert(
        "Library".to_string(),
        Value::Class(Rc::new(create_library_class())),
    );
    members.insert(
        "Callback".to_string(),
        Value::Class(Rc::new(create_callback_class())),
    );

    Value::Namespace {
        name: "Ffi".to_string(),
        members: Rc::new(RefCell::new(members)),
        module_globals: None,
    }
}



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
        let layout = Layout::from_size_align(size, 8).map_err(|_| "Invalid layout")?;
        let ptr = alloc(layout);
        if ptr.is_null() {
            return Err("Memory allocation failed".to_string());
        }

        
        ALLOCATION_SIZES.with(|allocs_cell| {
            let mut map = allocs_cell.borrow_mut();
            map.insert(ptr as usize, size);
        });

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
        return Ok(Value::Null); 
    }

    
    let size = ALLOCATION_SIZES.with(|allocs_cell| {
        let mut map = allocs_cell.borrow_mut();
        map.remove(&ptr_val)
    });

    if let Some(size) = size {
        unsafe {
            let layout =
                Layout::from_size_align(size, 8).map_err(|_| "Invalid layout for deallocation")?;
            std::alloc::dealloc(ptr_val as *mut u8, layout);
        }
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
        return Ok(Value::String(Rc::from(String::new())));
    }
    let s = unsafe {
        match CStr::from_ptr(ptr).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return Ok(Value::String(Rc::from(String::new()))),
        }
    };
    Ok(Value::String(Rc::from(s)))
}



macro_rules! impl_write {
    ($name:ident, $type:ty) => {
        fn $name(args: &[Value]) -> Result<Value, String> {
            if args.len() < 2 {
                return Err(
                    concat!(stringify!($name), " expects 2 arguments (pointer, value)").to_string(),
                );
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
        Value::String(s) => &**s,
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
        Value::String(s) => &**s,
        _ => return Err("Type name must be a string".to_string()),
    };
    let ctype = CType::from_str(type_name)?;
    Ok(Value::Number(ctype.size() as f64))
}



fn ffi_open(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.open expects 1 argument (path)".to_string());
    }
    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Path must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let resolved_path = crate::resolve_script_path(&path);
    let full_path = resolved_path.to_string_lossy().to_string();

    let library = unsafe {
        Library::new(&full_path)
            .map_err(|e| format!("Failed to load library '{}': {}", full_path, e))?
    };

    let lib_class = Rc::new(create_library_class());
    let mut instance = Instance::new(lib_class);

    let ffi_lib = FfiLibrary {
        library,
        _path: full_path.clone(),
    };

    
    let lib_handle = Box::new(Mutex::new(ffi_lib));
    let lib_ptr = Box::into_raw(lib_handle);

    instance.fields.insert(
        "_handle".to_string(),
        Value::Number(lib_ptr as usize as f64),
    );
    instance
        .fields
        .insert("_path".to_string(), Value::String(Rc::from(full_path)));

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}



fn create_library_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();
    let mut callable_methods: FxHashMap<String, crate::vm::caller::CallableNativeInstanceFn> =
        FxHashMap::default();

    instance_methods.insert("symbol".to_string(), library_symbol);
    instance_methods.insert("close".to_string(), library_close);
    instance_methods.insert("path".to_string(), library_path);

    callable_methods.insert("call".to_string(), library_call);

    let mut class = Class::new_with_instance("Library", instance_methods, None);
    class.callable_native_instance_methods = callable_methods;
    class
}










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
        _ => {
            return Err(format!(
                "Function name must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    
    let options = if args.len() > 1 {
        match &args[1] {
            Value::Dictionary(dict) => dict.borrow().clone(),
            _ => return Err("Second argument must be an options dictionary".to_string()),
        }
    } else {
        return Err("lib.call requires options dictionary with 'args' and 'returns'".to_string());
    };

    
    let return_type_str = match options.get("returns") {
        Some(Value::String(s)) => s.to_string(),
        Some(_) => return Err("'returns' must be a string".to_string()),
        None => "void".to_string(),
    };
    let return_type = CType::from_str(&return_type_str)?;

    
    let (call_values, arg_types) = match options.get("args") {
        Some(Value::Array(arr)) => {
            let arr_guard = arr.borrow();
            let mut values = Vec::new();
            let mut types = Vec::new();

            for (idx, arg_dict) in arr_guard.iter().enumerate() {
                match arg_dict {
                    Value::Dictionary(d) => {
                        let d_guard = d.borrow();

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
                    _ => {
                        return Err(format!(
                            "args[{}] must be a dictionary with 'type' and 'value'",
                            idx
                        ))
                    }
                }
            }

            (values, types)
        }
        Some(_) => return Err("'args' must be an array".to_string()),
        None => (Vec::new(), Vec::new()),
    };

    
    set_global_caller(caller);

    let result = if let Value::Instance(inst) = recv {
        
        let lib_ptr = {
            let inst_guard = inst.borrow();
            if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
                let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
                if ptr.is_null() {
                    return Err("Library has been closed".to_string());
                }
                ptr
            } else {
                return Err("Invalid library instance".to_string());
            }
            
        };

        unsafe {
            let lib_mutex = &*lib_ptr;

            
            let func_ptr = {
                let lib_guard = lib_mutex.lock();

                let fn_name_c =
                    CString::new(fn_name.as_str()).map_err(|_| "Invalid function name")?;

                let func_ptr: Symbol<*const c_void> = lib_guard
                    .library
                    .get(fn_name_c.as_bytes_with_nul())
                    .map_err(|e| format!("Function '{}' not found: {}", fn_name, e))?;

                *func_ptr
                
            };

            
            
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
        let inst_guard = inst.borrow();
        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *const Mutex<FfiLibrary>;
            if ptr.is_null() {
                return Err("Library has been closed".to_string());
            }

            unsafe {
                let lib_mutex = &*ptr;
                let lib_guard = lib_mutex.lock();

                let symbol_name_c =
                    CString::new(symbol_name.as_str()).map_err(|_| "Invalid symbol name")?;

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
        let mut inst_guard = inst.borrow_mut();

        if let Some(Value::Number(ptr)) = inst_guard.fields.get("_handle") {
            let ptr = *ptr as usize as *mut Mutex<FfiLibrary>;
            if !ptr.is_null() {
                unsafe {
                    let _ = Box::from_raw(ptr);
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
        let inst_guard = inst.borrow();
        if let Some(path) = inst_guard.fields.get("_path") {
            return Ok(path.clone());
        }
    }
    Ok(Value::Null)
}



fn create_callback_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("ptr".to_string(), callback_ptr);
    instance_methods.insert("id".to_string(), callback_id);
    instance_methods.insert("release".to_string(), callback_release);

    
    let mut class = Class::new_with_instance("Callback", instance_methods, Some(callback_new));

    
    class
        .native_static_methods
        .insert("new".to_string(), callback_new);

    class
}









fn callback_new(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Ffi.Callback expects 1 argument (options dictionary)".to_string());
    }

    let options = match &args[0] {
        Value::Dictionary(dict) => dict.borrow().clone(),
        _ => return Err("Ffi.Callback expects an options dictionary".to_string()),
    };

    
    let arg_types: Vec<CType> = match options.get("args") {
        Some(Value::Array(arr)) => {
            let arr_guard = arr.borrow();
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

    
    let return_type = match options.get("returns") {
        Some(Value::String(s)) => CType::from_str(s)?,
        Some(_) => return Err("'returns' must be a type string".to_string()),
        None => CType::Void,
    };

    
    let func = match options.get("fn") {
        Some(f @ Value::Function(_)) => f.clone(),
        Some(_) => return Err("'fn' must be a function".to_string()),
        None => return Err("Missing 'fn' field in Callback options".to_string()),
    };

    
    let callback_id = register_callback(func, arg_types.clone(), return_type.clone());

    
    let ffi_arg_types: Vec<FfiType> = arg_types.iter().map(|t| t.to_ffi_type()).collect();
    let ffi_return_type = return_type.to_ffi_type();
    let cif = Cif::new(ffi_arg_types, ffi_return_type);

    
    
    

    
    let callback_id_box = Box::new(callback_id);
    let callback_id_ptr = Box::into_raw(callback_id_box); 
    let callback_id_ref: &'static i64 = unsafe { &*callback_id_ptr };

    let closure = libffi::middle::Closure::new(cif.clone(), closure_handler, callback_id_ref);
    
    
    let code_ptr_value = unsafe {
        let fn_ptr: unsafe extern "C" fn() = *closure.code_ptr();
        std::mem::transmute::<unsafe extern "C" fn(), usize>(fn_ptr)
    };

    
    CLOSURE_REGISTRY.with(|reg_cell| {
        let mut map = reg_cell.borrow_mut();
        map.insert(
            callback_id,
            ClosureData {
                cif,
                closure,
                code_ptr: code_ptr_value,
                callback_id_ptr, 
            },
        );
    });

    
    let callback_class = Rc::new(create_callback_class());
    let mut instance = Instance::new(callback_class);

    instance
        .fields
        .insert("_id".to_string(), Value::Number(callback_id as f64));
    instance.fields.insert(
        "_code_ptr".to_string(),
        Value::Number(code_ptr_value as f64),
    );
    instance
        .fields
        .insert("_released".to_string(), Value::Boolean(false));

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}


fn callback_ptr(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.borrow();

        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Err("Callback has been released".to_string());
        }

        
        if let Some(Value::Number(code_ptr)) = inst_guard.fields.get("_code_ptr") {
            return Ok(Value::Number(*code_ptr));
        }
    }
    Err("Invalid callback instance".to_string())
}


fn callback_id(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.borrow();

        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Err("Callback has been released".to_string());
        }

        if let Some(id) = inst_guard.fields.get("_id") {
            return Ok(id.clone());
        }
    }
    Err("Invalid callback instance".to_string())
}


fn callback_release(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let mut inst_guard = inst.borrow_mut();

        if let Some(Value::Boolean(true)) = inst_guard.fields.get("_released") {
            return Ok(Value::Null); 
        }

        if let Some(Value::Number(id)) = inst_guard.fields.get("_id") {
            let callback_id = *id as i64;
            unregister_callback(callback_id);

            
            CLOSURE_REGISTRY.with(|reg_cell| {
                let mut map = reg_cell.borrow_mut();
                map.remove(&callback_id);
            });
        }

        inst_guard
            .fields
            .insert("_released".to_string(), Value::Boolean(true));
        return Ok(Value::Null);
    }
    Err("Invalid callback instance".to_string())
}



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

    
    
    let mut cstrings: Vec<CString> = Vec::new();
    for (val, typ) in values.iter().zip(arg_types.iter()) {
        if *typ == CType::CString {
            if let Value::String(s) = val {
                let c_str =
                    CString::new(&**s).map_err(|_| "Invalid C string (contains null byte)")?;
                cstrings.push(c_str);
            }
        }
    }

    
    let mut cstring_idx = 0;
    let mut converted_args: Vec<ConvertedArg> = Vec::new();
    for (idx, (val, typ)) in values.iter().zip(arg_types.iter()).enumerate() {
        let converted = match val {
            
            Value::Instance(inst) => {
                let inst_guard = inst.borrow();
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
