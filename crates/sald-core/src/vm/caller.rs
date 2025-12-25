use crate::vm::Value;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub trait ValueCaller {
    fn call(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, String>;

    fn get_globals(&self) -> FxHashMap<String, Value>;

    fn get_shared_globals(&self) -> Rc<RefCell<FxHashMap<String, Value>>>;
}

pub type CallableNativeStaticFn = fn(&[Value], &mut dyn ValueCaller) -> Result<Value, String>;

pub type CallableNativeInstanceFn =
    fn(&Value, &[Value], &mut dyn ValueCaller) -> Result<Value, String>;
