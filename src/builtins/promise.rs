// Promise built-in namespace
// Provides Promise.all, Promise.race, Promise.resolve, Promise.reject
// For parallel async execution

use super::check_arity;
use crate::vm::value::{Class, NativeStaticFn, SaldFuture, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub fn create_promise_class() -> Class {
    let mut static_methods: HashMap<String, NativeStaticFn> = HashMap::new();

    static_methods.insert("all".to_string(), promise_all);
    static_methods.insert("race".to_string(), promise_race);
    static_methods.insert("resolve".to_string(), promise_resolve);
    static_methods.insert("reject".to_string(), promise_reject);

    Class::new_with_static("Promise", static_methods)
}

/// Promise.all(futures) - Wait for all futures, return array of results
fn promise_all(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    
    let futures = match &args[0] {
        Value::Array(arr) => {
            let arr = arr.lock().unwrap();
            arr.clone()
        }
        _ => return Err("Promise.all() expects an array of futures".to_string()),
    };

    let (tx, rx) = oneshot::channel();

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let mut results = Vec::with_capacity(futures.len());
            let mut had_error = false;
            let mut error_msg = String::new();

            for future_val in futures {
                match future_val {
                    Value::Future(fut) => {
                        // Take the future receiver
                        let receiver = {
                            let mut fut_guard = fut.lock().unwrap();
                            fut_guard.take()
                        };

                        if let Some(sald_future) = receiver {
                            match sald_future.receiver.await {
                                Ok(Ok(value)) => results.push(value),
                                Ok(Err(e)) => {
                                    had_error = true;
                                    error_msg = e;
                                    break;
                                }
                                Err(_) => {
                                    had_error = true;
                                    error_msg = "Future was cancelled".to_string();
                                    break;
                                }
                            }
                        } else {
                            // Future already consumed, treat as null
                            results.push(Value::Null);
                        }
                    }
                    // Non-future values are passed through directly
                    other => results.push(other),
                }
            }

            if had_error {
                let _ = tx.send(Err(error_msg));
            } else {
                let _ = tx.send(Ok(Value::Array(Arc::new(Mutex::new(results)))));
            }
        });
    } else {
        return Err("No tokio runtime available".to_string());
    }

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
}

/// Promise.race(futures) - Return first completed future's result
fn promise_race(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    
    let futures = match &args[0] {
        Value::Array(arr) => {
            let arr = arr.lock().unwrap();
            arr.clone()
        }
        _ => return Err("Promise.race() expects an array of futures".to_string()),
    };

    if futures.is_empty() {
        return Ok(Value::Null);
    }

    let (tx, rx) = oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        for future_val in futures {
            let tx_clone = Arc::clone(&tx);
            
            match future_val {
                Value::Future(fut) => {
                    let receiver = {
                        let mut fut_guard = fut.lock().unwrap();
                        fut_guard.take()
                    };

                    if let Some(sald_future) = receiver {
                        handle.spawn(async move {
                            let result = match sald_future.receiver.await {
                                Ok(Ok(value)) => Ok(value),
                                Ok(Err(e)) => Err(e),
                                Err(_) => Err("Future was cancelled".to_string()),
                            };

                            // Try to send result (first one wins)
                            if let Some(sender) = tx_clone.lock().unwrap().take() {
                                let _ = sender.send(result);
                            }
                        });
                    }
                }
                // Non-future values resolve immediately (and win the race)
                other => {
                    if let Some(sender) = tx_clone.lock().unwrap().take() {
                        let _ = sender.send(Ok(other));
                    }
                    break;
                }
            }
        }
    } else {
        return Err("No tokio runtime available".to_string());
    }

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
}

/// Promise.resolve(value) - Create a resolved future
fn promise_resolve(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    
    let value = args[0].clone();
    let (tx, rx) = oneshot::channel();
    
    // Send immediately
    let _ = tx.send(Ok(value));
    
    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
}

/// Promise.reject(error) - Create a rejected future
fn promise_reject(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    
    let error = match &args[0] {
        Value::String(s) => s.to_string(),
        other => format!("{}", other),
    };
    
    let (tx, rx) = oneshot::channel();
    
    // Send error immediately
    let _ = tx.send(Err(error));
    
    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
}
