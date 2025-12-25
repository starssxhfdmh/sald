use crate::compiler::Chunk;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub type NativeStaticFn = fn(&[Value]) -> Result<Value, String>;

pub type NativeInstanceFn = fn(&Value, &[Value]) -> Result<Value, String>;

pub type NativeConstructorFn = fn(&[Value]) -> Result<Value, String>;

pub type NativeFn = fn(&[Value]) -> Value;

pub struct SaldFuture;

/// Thread-safe value for cross-thread communication  
/// Contains only owned data - no Rc/RefCell
#[derive(Clone, Debug)]
pub enum SendValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<SendValue>),
    Dictionary(std::collections::HashMap<String, SendValue>),
}

unsafe impl Send for SendValue {}

#[cfg(not(target_arch = "wasm32"))]
pub type FutureHandle = crossbeam_channel::Receiver<Result<SendValue, String>>;

#[cfg(target_arch = "wasm32")]
pub type FutureHandle = std::rc::Rc<std::cell::RefCell<Option<Result<SendValue, String>>>>;

impl SendValue {
    pub fn from_value(value: &Value) -> Result<SendValue, String> {
        match value {
            Value::Null => Ok(SendValue::Null),
            Value::Boolean(b) => Ok(SendValue::Boolean(*b)),
            Value::Number(n) => Ok(SendValue::Number(*n)),
            Value::String(s) => Ok(SendValue::String(s.to_string())),
            Value::Array(arr) => {
                let arr = arr.borrow();
                let mut result = Vec::with_capacity(arr.len());
                for v in arr.iter() {
                    result.push(SendValue::from_value(v)?);
                }
                Ok(SendValue::Array(result))
            }
            Value::Dictionary(dict) => {
                let dict = dict.borrow();
                let mut result = std::collections::HashMap::with_capacity(dict.len());
                for (k, v) in dict.iter() {
                    result.insert(k.clone(), SendValue::from_value(v)?);
                }
                Ok(SendValue::Dictionary(result))
            }
            _ => Err(format!("Cannot send {} to async worker", value.type_name())),
        }
    }

    pub fn to_value(self) -> Value {
        match self {
            SendValue::Null => Value::Null,
            SendValue::Boolean(b) => Value::Boolean(b),
            SendValue::Number(n) => Value::Number(n),
            SendValue::String(s) => Value::String(std::rc::Rc::from(s)),
            SendValue::Array(arr) => {
                let values: Vec<Value> = arr.into_iter().map(|v| v.to_value()).collect();
                Value::Array(std::rc::Rc::new(std::cell::RefCell::new(values)))
            }
            SendValue::Dictionary(dict) => {
                let mut map = FxHashMap::default();
                for (k, v) in dict {
                    map.insert(k, v.to_value());
                }
                Value::Dictionary(std::rc::Rc::new(std::cell::RefCell::new(map)))
            }
        }
    }
}

#[derive(Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Number(f64),
    String(Rc<str>),
    Array(Rc<RefCell<Vec<Value>>>),
    Dictionary(Rc<RefCell<FxHashMap<String, Value>>>),
    Function(Rc<Function>),

    NativeFunction {
        func: NativeStaticFn,
        class_name: String,
    },

    InstanceMethod {
        receiver: Box<Value>,
        method: NativeInstanceFn,
        method_name: String,
    },

    BoundMethod {
        receiver: Box<Value>,
        method: Rc<Function>,
    },
    Class(Rc<Class>),
    Instance(Rc<RefCell<Instance>>),

    Future(Rc<RefCell<Option<FutureHandle>>>),

    Namespace {
        name: String,
        members: Rc<RefCell<FxHashMap<String, Value>>>,

        module_globals: Option<Rc<RefCell<FxHashMap<String, Value>>>>,
    },

    Enum {
        name: String,
        variants: Rc<FxHashMap<String, Value>>,
    },

    SpreadMarker(Box<Value>),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "Null",
            Value::Boolean(_) => "Boolean",
            Value::Number(_) => "Number",
            Value::String(_) => "String",
            Value::Array(_) => "Array",
            Value::Dictionary(_) => "Dict",
            Value::Function(_) => "Function",
            Value::NativeFunction { .. } => "NativeFunction",
            Value::InstanceMethod { .. } => "InstanceMethod",
            Value::BoundMethod { .. } => "BoundMethod",
            Value::Class(_) => "Class",
            Value::Instance(inst) => {
                let inst = inst.borrow();
                if !inst.class_name.is_empty() {
                    return "Instance";
                }
                "Instance"
            }
            Value::Future(_) => "Future",
            Value::Namespace { .. } => "Namespace",
            Value::Enum { .. } => "Enum",
            Value::SpreadMarker(_) => "SpreadMarker",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Boolean(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => {
                let arr = arr.borrow();
                !arr.is_empty()
            }
            Value::Dictionary(dict) => {
                let dict = dict.borrow();
                !dict.is_empty()
            }
            _ => true,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(&**s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => Rc::ptr_eq(a, b) || a == b,
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Class(a), Value::Class(b)) => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Dictionary(a), Value::Dictionary(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Number(n) => {
                if n.fract() == 0.0 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Array(arr) => {
                let arr = arr.borrow();
                let items: Vec<String> = arr.iter().map(|v| format!("{}", v)).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Dictionary(dict) => {
                let dict = dict.borrow();
                let items: Vec<String> = dict
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v))
                    .collect();
                write!(f, "{{{}}}", items.join(", "))
            }
            Value::Function(func) => write!(f, "<fn {}>", func.name),
            Value::NativeFunction { class_name, .. } => write!(f, "<native fn {}>", class_name),
            Value::InstanceMethod { method_name, .. } => write!(f, "<method {}>", method_name),
            Value::BoundMethod { method, .. } => write!(f, "<bound method {}>", method.name),
            Value::Class(class) => write!(f, "<class {}>", class.name),
            Value::Instance(inst) => {
                let inst = inst.borrow();
                write!(f, "<{} instance>", inst.class_name)
            }
            Value::Future(_) => write!(f, "<Future>"),
            Value::Namespace { name, .. } => write!(f, "<namespace {}>", name),
            Value::Enum { name, .. } => write!(f, "<enum {}>", name),
            Value::SpreadMarker(v) => write!(f, "<spread {:?}>", v),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Clone, Debug)]
pub struct UpvalueObj {
    pub location: usize,
    pub closed: Option<Box<Value>>,
}

impl UpvalueObj {
    pub fn new(location: usize) -> Self {
        Self {
            location,
            closed: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }
}

#[derive(Clone)]
pub struct Function {
    pub name: String,
    pub arity: usize,
    pub is_variadic: bool,

    pub is_async: bool,
    pub upvalue_count: usize,
    pub chunk: Chunk,
    pub file: String,

    pub upvalues: Vec<Rc<RefCell<UpvalueObj>>>,

    pub param_names: Vec<String>,

    pub default_count: usize,

    pub decorators: Vec<String>,

    pub namespace_context: Option<String>,

    pub class_context: Option<String>,
}

impl Function {
    pub fn new(name: impl Into<String>, arity: usize, chunk: Chunk) -> Self {
        Self {
            name: name.into(),
            arity,
            is_variadic: false,
            is_async: false,
            upvalue_count: 0,
            chunk,
            file: String::new(),
            upvalues: Vec::new(),
            param_names: Vec::new(),
            default_count: 0,
            decorators: Vec::new(),
            namespace_context: None,
            class_context: None,
        }
    }

    pub fn new_variadic(
        name: impl Into<String>,
        arity: usize,
        is_variadic: bool,
        chunk: Chunk,
    ) -> Self {
        Self {
            name: name.into(),
            arity,
            is_variadic,
            is_async: false,
            upvalue_count: 0,
            chunk,
            file: String::new(),
            upvalues: Vec::new(),
            param_names: Vec::new(),
            default_count: 0,
            decorators: Vec::new(),
            namespace_context: None,
            class_context: None,
        }
    }

    pub fn new_with_upvalues(
        name: impl Into<String>,
        arity: usize,
        is_variadic: bool,
        upvalue_count: usize,
        chunk: Chunk,
    ) -> Self {
        Self {
            name: name.into(),
            arity,
            is_variadic,
            is_async: false,
            upvalue_count,
            chunk,
            file: String::new(),
            upvalues: Vec::with_capacity(upvalue_count),
            param_names: Vec::new(),
            default_count: 0,
            decorators: Vec::new(),
            namespace_context: None,
            class_context: None,
        }
    }

    pub fn from_constant(fc: &crate::compiler::chunk::FunctionConstant) -> Self {
        Self {
            name: fc.name.clone(),
            arity: fc.arity,
            is_variadic: fc.is_variadic,
            is_async: fc.is_async,
            upvalue_count: fc.upvalue_count,
            chunk: fc.chunk.clone(),
            file: fc.file.clone(),
            upvalues: Vec::with_capacity(fc.upvalue_count),
            param_names: fc.param_names.clone(),
            default_count: fc.default_count,
            decorators: fc.decorators.clone(),
            namespace_context: fc.namespace_context.clone(),
            class_context: fc.class_context.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Class {
    pub name: String,

    pub methods: FxHashMap<String, Value>,

    pub user_static_methods: FxHashMap<String, Value>,

    pub native_static_methods: FxHashMap<String, NativeStaticFn>,

    pub native_instance_methods: FxHashMap<String, NativeInstanceFn>,

    pub callable_native_instance_methods:
        FxHashMap<String, super::caller::CallableNativeInstanceFn>,

    pub native_static_fields: FxHashMap<String, Value>,

    pub constructor: Option<NativeConstructorFn>,

    pub superclass: Option<Rc<Class>>,
}

impl Class {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            methods: FxHashMap::default(),
            user_static_methods: FxHashMap::default(),
            native_static_methods: FxHashMap::default(),
            native_instance_methods: FxHashMap::default(),
            callable_native_instance_methods: FxHashMap::default(),
            native_static_fields: FxHashMap::default(),
            constructor: None,
            superclass: None,
        }
    }

    pub fn new_with_static(
        name: impl Into<String>,
        native_static_methods: FxHashMap<String, NativeStaticFn>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: FxHashMap::default(),
            user_static_methods: FxHashMap::default(),
            native_static_methods,
            native_instance_methods: FxHashMap::default(),
            callable_native_instance_methods: FxHashMap::default(),
            native_static_fields: FxHashMap::default(),
            constructor: None,
            superclass: None,
        }
    }

    pub fn new_with_instance(
        name: impl Into<String>,
        native_instance_methods: FxHashMap<String, NativeInstanceFn>,
        constructor: Option<NativeConstructorFn>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: FxHashMap::default(),
            user_static_methods: FxHashMap::default(),
            native_static_methods: FxHashMap::default(),
            native_instance_methods,
            callable_native_instance_methods: FxHashMap::default(),
            native_static_fields: FxHashMap::default(),
            constructor,
            superclass: None,
        }
    }

    pub fn new_with_static_and_fields(
        name: impl Into<String>,
        native_static_methods: FxHashMap<String, NativeStaticFn>,
        native_static_fields: FxHashMap<String, Value>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: FxHashMap::default(),
            user_static_methods: FxHashMap::default(),
            native_static_methods,
            native_instance_methods: FxHashMap::default(),
            callable_native_instance_methods: FxHashMap::default(),
            native_static_fields,
            constructor: None,
            superclass: None,
        }
    }
}

#[derive(Clone)]
pub struct Instance {
    pub class_name: String,
    pub class: Rc<Class>,
    pub fields: FxHashMap<String, Value>,
}

impl Instance {
    pub fn new(class: Rc<Class>) -> Self {
        Self {
            class_name: class.name.clone(),
            class,
            fields: FxHashMap::default(),
        }
    }
}
