// Promise built-in namespace
// Provides Promise.all, Promise.race, Promise.resolve, Promise.reject
// For parallel async execution

use super::check_arity;
use crate::vm::value::{Class, NativeStaticFn, SaldFuture, Value};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

pub fn create_promise_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("all".to_string(), promise_all);
    static_methods.insert("race".to_string(), promise_race);
    static_methods.insert("resolve".to_string(), promise_resolve);
    static_methods.insert("reject".to_string(), promise_reject);

    Class::new_with_static("Promise", static_methods)
}

/// Promise.all(futures) - Wait for all futures, return array of results
/// IMPORTANT: All futures are started CONCURRENTLY, not sequentially!
fn promise_all(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    let futures = match &args[0] {
        Value::Array(arr) => {
            let arr = arr.lock();
            arr.clone()
        }
        _ => return Err("Promise.all() expects an array of futures".to_string()),
    };

    let (tx, rx) = oneshot::channel();

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // Step 1: Spawn ALL futures immediately (concurrent execution)
        // Each future gets its own oneshot channel to report results
        let mut item_receivers: Vec<oneshot::Receiver<Result<Value, String>>> =
            Vec::with_capacity(futures.len());

        for future_val in futures {
            let (item_tx, item_rx) = oneshot::channel();

            match future_val {
                Value::Future(fut) => {
                    let receiver = {
                        let mut fut_guard = fut.lock();
                        fut_guard.take()
                    };

                    if let Some(sald_future) = receiver {
                        // Spawn immediately - this starts the future running NOW
                        handle.spawn(async move {
                            match sald_future.receiver.await {
                                Ok(Ok(value)) => {
                                    let _ = item_tx.send(Ok(value));
                                }
                                Ok(Err(e)) => {
                                    let _ = item_tx.send(Err(e));
                                }
                                Err(_) => {
                                    let _ = item_tx.send(Err("Future was cancelled".to_string()));
                                }
                            }
                        });
                    } else {
                        // Future already consumed, treat as null
                        let _ = item_tx.send(Ok(Value::Null));
                    }
                }
                // Non-future values resolve immediately
                other => {
                    let _ = item_tx.send(Ok(other));
                }
            }

            item_receivers.push(item_rx);
        }

        // Step 2: Collect all results in order (after all are spawned)
        handle.spawn(async move {
            let mut results = Vec::with_capacity(item_receivers.len());
            let mut had_error = false;
            let mut error_msg = String::new();

            for item_rx in item_receivers {
                match item_rx.await {
                    Ok(Ok(value)) => {
                        if !had_error {
                            results.push(value);
                        }
                    }
                    Ok(Err(e)) => {
                        if !had_error {
                            had_error = true;
                            error_msg = e;
                        }
                    }
                    Err(_) => {
                        if !had_error {
                            had_error = true;
                            error_msg = "Future was cancelled".to_string();
                        }
                    }
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

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// Promise.race(futures) - Return first completed future's result
fn promise_race(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    let futures = match &args[0] {
        Value::Array(arr) => {
            let arr = arr.lock();
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
                        let mut fut_guard = fut.lock();
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
                            if let Some(sender) = tx_clone.lock().take() {
                                let _ = sender.send(result);
                            }
                        });
                    }
                }
                // Non-future values resolve immediately (and win the race)
                other => {
                    if let Some(sender) = tx_clone.lock().take() {
                        let _ = sender.send(Ok(other));
                    }
                    break;
                }
            }
        }
    } else {
        return Err("No tokio runtime available".to_string());
    }

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// Promise.resolve(value) - Create a resolved future
fn promise_resolve(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    let value = args[0].clone();
    let (tx, rx) = oneshot::channel();

    // Send immediately
    let _ = tx.send(Ok(value));

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
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

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}
