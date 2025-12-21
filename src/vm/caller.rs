// Sald Value Caller
// Allows native functions to call Sald functions/closures

use crate::vm::Value;
use rustc_hash::FxHashMap;
use std::sync::{Arc, RwLock};

/// Trait for calling Values (functions/closures) from native functions
pub trait ValueCaller {
    /// Call a callable value with given arguments
    /// Returns the result or an error message
    fn call(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, String>;
    
    /// Get a copy of the VM's globals (for sharing with child VMs)
    fn get_globals(&self) -> FxHashMap<String, Value>;
    
    /// Get the shared globals Arc for true sharing between VMs
    fn get_shared_globals(&self) -> Arc<RwLock<FxHashMap<String, Value>>>;
}

/// Callable native static function type
/// Receives args and a caller that can invoke closures
pub type CallableNativeStaticFn = fn(&[Value], &mut dyn ValueCaller) -> Result<Value, String>;

/// Callable native instance function type
/// Receives receiver, args, and a caller that can invoke closures
pub type CallableNativeInstanceFn =
    fn(&Value, &[Value], &mut dyn ValueCaller) -> Result<Value, String>;
