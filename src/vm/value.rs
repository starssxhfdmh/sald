// Sald Runtime Values
// All values in Sald are represented as class instances
// Uses Arc/Mutex for thread-safety (async support)

use crate::compiler::Chunk;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Native function type (for static methods)
pub type NativeStaticFn = fn(&[Value]) -> Result<Value, String>;

/// Native method type (receiver + args -> result, for instance methods)
pub type NativeInstanceFn = fn(&Value, &[Value]) -> Result<Value, String>;

/// Native constructor type (args -> result)
pub type NativeConstructorFn = fn(&[Value]) -> Result<Value, String>;

/// Old native function type for backwards compatibility
pub type NativeFn = fn(&[Value]) -> Value;

/// Future for async operations - wraps a oneshot receiver
pub struct SaldFuture {
    pub receiver: oneshot::Receiver<Result<Value, String>>,
}

/// Runtime value types
/// Uses Arc for shared ownership and Mutex/RwLock for interior mutability
/// This enables thread-safe async operations
#[derive(Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Number(f64),
    String(Arc<String>),
    Array(Arc<Mutex<Vec<Value>>>),
    Dictionary(Arc<Mutex<HashMap<String, Value>>>),
    Function(Arc<Function>),
    /// Native function with class reference (for static method calls)
    NativeFunction {
        func: NativeStaticFn,
        class_name: String,
    },
    /// Instance method bound to a primitive value (native)
    InstanceMethod {
        receiver: Box<Value>,
        method: NativeInstanceFn,
        method_name: String,
    },
    /// User-defined method bound to a receiver (for super calls)
    BoundMethod {
        receiver: Box<Value>,
        method: Arc<Function>,
    },
    Class(Arc<Class>),
    Instance(Arc<Mutex<Instance>>),
    /// Future value for async operations
    Future(Arc<Mutex<Option<SaldFuture>>>),
    /// Namespace value: holds members as a HashMap
    Namespace {
        name: String,
        members: Arc<Mutex<HashMap<String, Value>>>,
    },
    /// Enum value: holds variants as a HashMap
    Enum {
        name: String,
        variants: Arc<HashMap<String, Value>>,
    },
    /// Spread marker: wraps a value that should be spread as multiple arguments
    SpreadMarker(Box<Value>),
}

// Implement Send and Sync for Value since we use Arc/Mutex
unsafe impl Send for Value {}
unsafe impl Sync for Value {}

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
                if let Ok(inst) = inst.lock() {
                    if !inst.class_name.is_empty() {
                        return "Instance";
                    }
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
                if let Ok(arr) = arr.lock() {
                    !arr.is_empty()
                } else {
                    true
                }
            }
            Value::Dictionary(dict) => {
                if let Ok(dict) = dict.lock() {
                    !dict.is_empty()
                } else {
                    true
                }
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
            Value::String(s) => Some(s.as_str()),
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
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Instance(a), Value::Instance(b)) => Arc::ptr_eq(a, b),
            (Value::Class(a), Value::Class(b)) => Arc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => Arc::ptr_eq(a, b),
            (Value::Dictionary(a), Value::Dictionary(b)) => Arc::ptr_eq(a, b),
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
                if let Ok(arr) = arr.lock() {
                    let items: Vec<String> = arr.iter().map(|v| format!("{}", v)).collect();
                    write!(f, "[{}]", items.join(", "))
                } else {
                    write!(f, "[<locked>]")
                }
            }
            Value::Dictionary(dict) => {
                if let Ok(dict) = dict.lock() {
                    let items: Vec<String> = dict
                        .iter()
                        .map(|(k, v)| format!("\"{}\": {}", k, v))
                        .collect();
                    write!(f, "{{{}}}", items.join(", "))
                } else {
                    write!(f, "{{<locked>}}")
                }
            }
            Value::Function(func) => write!(f, "<fn {}>", func.name),
            Value::NativeFunction { class_name, .. } => write!(f, "<native fn {}>", class_name),
            Value::InstanceMethod { method_name, .. } => write!(f, "<method {}>", method_name),
            Value::BoundMethod { method, .. } => write!(f, "<bound method {}>", method.name),
            Value::Class(class) => write!(f, "<class {}>", class.name),
            Value::Instance(inst) => {
                if let Ok(inst) = inst.lock() {
                    write!(f, "<{} instance>", inst.class_name)
                } else {
                    write!(f, "<instance locked>")
                }
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

/// Upvalue object - holds a captured variable
/// When open, points to a stack slot. When closed, holds the value directly.
#[derive(Clone, Debug)]
pub struct UpvalueObj {
    pub location: usize,            // Stack slot (while open)
    pub closed: Option<Box<Value>>, // Captured value (when closed)
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

/// Function object (closure)
#[derive(Clone)]
pub struct Function {
    pub name: String,
    pub arity: usize,
    pub is_variadic: bool,
    pub upvalue_count: usize,
    pub chunk: Chunk,
    pub file: String, // Source file where function was defined
    /// Captured upvalues at runtime
    pub upvalues: Vec<Arc<Mutex<UpvalueObj>>>,
    /// Parameter names for named argument matching
    pub param_names: Vec<String>,
    /// Number of parameters with default values (from end)
    pub default_count: usize,
    /// Decorator names applied to this function
    pub decorators: Vec<String>,
    /// Namespace this function was defined in (for private access)
    pub namespace_context: Option<String>,
    /// Class this function was defined in (for private access from closures)
    pub class_context: Option<String>,
}

impl Function {
    pub fn new(name: impl Into<String>, arity: usize, chunk: Chunk) -> Self {
        Self {
            name: name.into(),
            arity,
            is_variadic: false,
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

    /// Create function from FunctionConstant (preserves all metadata)
    pub fn from_constant(fc: &crate::compiler::chunk::FunctionConstant) -> Self {
        Self {
            name: fc.name.clone(),
            arity: fc.arity,
            is_variadic: fc.is_variadic,
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


/// Class object
#[derive(Clone)]
pub struct Class {
    pub name: String,
    /// User-defined instance methods
    pub methods: HashMap<String, Value>,
    /// User-defined static methods
    pub user_static_methods: HashMap<String, Value>,
    /// Native static methods (Console.println, Date.now)
    pub native_static_methods: HashMap<String, NativeStaticFn>,
    /// Native instance methods (simple, no closure calls needed)
    pub native_instance_methods: HashMap<String, NativeInstanceFn>,
    /// Callable native instance methods (can call closures: map, filter, forEach, etc.)
    pub callable_native_instance_methods: HashMap<String, super::caller::CallableNativeInstanceFn>,
    /// Native static fields (Math.PI, Math.E)
    pub native_static_fields: HashMap<String, Value>,
    /// Constructor for type conversion (e.g., String(42) -> "42")
    pub constructor: Option<NativeConstructorFn>,
    /// Superclass for inheritance
    pub superclass: Option<Arc<Class>>,
}

impl Class {
    /// Create empty class
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            methods: HashMap::new(),
            user_static_methods: HashMap::new(),
            native_static_methods: HashMap::new(),
            native_instance_methods: HashMap::new(),
            callable_native_instance_methods: HashMap::new(),
            native_static_fields: HashMap::new(),
            constructor: None,
            superclass: None,
        }
    }

    /// Create class with native static methods (Console, Type, Date)
    pub fn new_with_static(
        name: impl Into<String>,
        native_static_methods: HashMap<String, NativeStaticFn>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: HashMap::new(),
            user_static_methods: HashMap::new(),
            native_static_methods,
            native_instance_methods: HashMap::new(),
            callable_native_instance_methods: HashMap::new(),
            native_static_fields: HashMap::new(),
            constructor: None,
            superclass: None,
        }
    }

    /// Create class with native instance methods (String, Number, Array)
    pub fn new_with_instance(
        name: impl Into<String>,
        native_instance_methods: HashMap<String, NativeInstanceFn>,
        constructor: Option<NativeConstructorFn>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: HashMap::new(),
            user_static_methods: HashMap::new(),
            native_static_methods: HashMap::new(),
            native_instance_methods,
            callable_native_instance_methods: HashMap::new(),
            native_static_fields: HashMap::new(),
            constructor,
            superclass: None,
        }
    }

    /// Create class with native static methods and static fields (Math with PI, E)
    pub fn new_with_static_and_fields(
        name: impl Into<String>,
        native_static_methods: HashMap<String, NativeStaticFn>,
        native_static_fields: HashMap<String, Value>,
    ) -> Self {
        Self {
            name: name.into(),
            methods: HashMap::new(),
            user_static_methods: HashMap::new(),
            native_static_methods,
            native_instance_methods: HashMap::new(),
            callable_native_instance_methods: HashMap::new(),
            native_static_fields,
            constructor: None,
            superclass: None,
        }
    }
}

/// Instance object
#[derive(Clone)]
pub struct Instance {
    pub class_name: String,
    pub class: Arc<Class>,
    pub fields: HashMap<String, Value>,
}

impl Instance {
    pub fn new(class: Arc<Class>) -> Self {
        Self {
            class_name: class.name.clone(),
            class,
            fields: HashMap::new(),
        }
    }
}
