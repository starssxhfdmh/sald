use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::time::Duration;

pub fn create_timer_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("sleep".to_string(), timer_sleep);
    static_methods.insert("now".to_string(), timer_now);
    static_methods.insert("millis".to_string(), timer_now);

    Class::new_with_static("Timer", static_methods)
}

fn timer_sleep(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let ms = match &args[0] {
        Value::Number(n) => *n as u64,
        _ => {
            return Err(format!(
                "Argument 'ms' must be a number, got {}",
                args[0].type_name()
            ))
        }
    };

    std::thread::sleep(Duration::from_millis(ms));

    Ok(Value::Null)
}

fn timer_now(_args: &[Value]) -> Result<Value, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;

    let millis = duration.as_secs_f64() * 1000.0;
    Ok(Value::Number(millis))
}
