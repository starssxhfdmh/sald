// Timer built-in class
// Provides: sleep(ms), now()

use crate::vm::value::{Class, NativeStaticFn, SaldFuture, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

pub fn create_timer_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("sleep".to_string(), timer_sleep);
    static_methods.insert("now".to_string(), timer_now);
    static_methods.insert("millis".to_string(), timer_now); // alias

    Class::new_with_static("Timer", static_methods)
}

/// Async sleep - returns a Future that resolves after specified milliseconds
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

    // Create oneshot channel for async result
    let (tx, rx) = oneshot::channel();

    // Get the current tokio runtime handle (from VM's runtime)
    // This works because we're called from within VM execution context
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // We have a runtime context - spawn on it
        handle.spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            let _ = tx.send(Ok(Value::Null));
        });
    } else {
        // Fallback: create a new thread for the sleep (shouldn't happen normally)
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            let _ = tx.send(Ok(Value::Null));
        });
    }

    // Return Future wrapping the receiver
    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// Get current timestamp in milliseconds since Unix epoch (with sub-ms precision)
fn timer_now(_args: &[Value]) -> Result<Value, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;

    // Use as_secs_f64() * 1000 for sub-millisecond precision
    let millis = duration.as_secs_f64() * 1000.0;
    Ok(Value::Number(millis))
}
