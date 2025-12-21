// Channel built-in class
// Go-style channels for async communication between coroutines
// Uses tokio::sync::mpsc for buffered channels

use super::{check_arity, check_arity_range, get_number_arg};
use crate::vm::value::{Class, Instance, NativeInstanceFn, NativeStaticFn, SaldFuture, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// Shared channel state
/// Uses tokio::sync::Mutex for receiver since it's used across await points
struct ChannelState {
    sender: Option<mpsc::Sender<Value>>,
    receiver: Option<Arc<tokio::sync::Mutex<mpsc::Receiver<Value>>>>,
    closed: bool,
    /// Number of messages that have been sent but not yet received
    /// This is used to make isClosed() return false if there are pending messages
    pending_count: Arc<AtomicUsize>,
}

pub fn create_channel_class() -> Class {
    let mut static_methods: HashMap<String, NativeStaticFn> = HashMap::new();
    let mut instance_methods: HashMap<String, NativeInstanceFn> = HashMap::new();

    // Static constructor (also available as Channel.new())
    static_methods.insert("new".to_string(), channel_new);

    // Instance methods
    instance_methods.insert("send".to_string(), channel_send);
    instance_methods.insert("receive".to_string(), channel_receive);
    instance_methods.insert("tryReceive".to_string(), channel_try_receive);
    instance_methods.insert("close".to_string(), channel_close);
    instance_methods.insert("isClosed".to_string(), channel_is_closed);

    // Use constructor so Channel() works directly
    let mut class = Class::new_with_instance("Channel", instance_methods, Some(channel_new));
    class.native_static_methods = static_methods;
    class
}

/// Channel() or Channel(bufferSize) - Create a new channel
fn channel_new(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;
    
    let buffer_size = if args.is_empty() {
        16 // Default buffer size
    } else {
        let size = get_number_arg(&args[0], "bufferSize")? as usize;
        if size == 0 { 1 } else { size } // Minimum 1
    };

    let (tx, rx) = mpsc::channel::<Value>(buffer_size);

    let class = Arc::new(create_channel_class());
    let mut instance = Instance::new(class);
    
    // Store channel state in a single struct wrapped in Arc<Mutex<>>
    let state = Arc::new(Mutex::new(ChannelState {
        sender: Some(tx),
        receiver: Some(Arc::new(tokio::sync::Mutex::new(rx))),
        closed: false,
        pending_count: Arc::new(AtomicUsize::new(0)),
    }));
    
    // Store the state arc pointer as a number (hacky but works)
    instance.fields.insert("_state".to_string(), Value::Number(Arc::into_raw(state) as usize as f64));

    Ok(Value::Instance(Arc::new(Mutex::new(instance))))
}

/// Helper to get channel state from instance
fn get_channel_state(inst: &Instance) -> Result<Arc<Mutex<ChannelState>>, String> {
    let ptr = inst.fields.get("_state")
        .and_then(|v| if let Value::Number(n) = v { Some(*n as usize) } else { None })
        .ok_or("Invalid channel instance")?;
    
    // SAFETY: We're reconstructing the Arc from the raw pointer we stored
    // We use Arc::clone to keep the original alive
    let state = unsafe { Arc::from_raw(ptr as *const Mutex<ChannelState>) };
    let cloned = Arc::clone(&state);
    std::mem::forget(state); // Don't drop the original
    
    Ok(cloned)
}

/// channel.send(value) - Async send value to channel
fn channel_send(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    
    if let Value::Instance(inst) = recv {
        let inst = inst.lock().unwrap();
        let state = get_channel_state(&inst)?;
        
        // Get sender and pending_count BEFORE entering async (to avoid holding lock across await)
        let (sender, pending_count) = {
            let state_guard = state.lock().unwrap();
            if state_guard.closed {
                return Err("Cannot send on closed channel".to_string());
            }
            (state_guard.sender.clone(), state_guard.pending_count.clone())
        };
        
        let Some(sender) = sender else {
            return Err("Channel sender dropped".to_string());
        };
        
        let value = args[0].clone();
        let (tx, rx) = oneshot::channel();
        
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // Increment pending count BEFORE spawning (so it's visible immediately)
            pending_count.fetch_add(1, Ordering::SeqCst);
            let pending_count_clone = pending_count.clone();
            
            handle.spawn(async move {
                match sender.send(value).await {
                    Ok(()) => { let _ = tx.send(Ok(Value::Null)); }
                    Err(_) => {
                        // Send failed, decrement pending count
                        pending_count_clone.fetch_sub(1, Ordering::SeqCst);
                        let _ = tx.send(Err("Channel closed".to_string()));
                    }
                }
            });
        } else {
            return Err("No tokio runtime available".to_string());
        }
        
        Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
    } else {
        Err("send() must be called on a Channel instance".to_string())
    }
}

/// channel.receive() - Async receive value from channel
fn channel_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    
    if let Value::Instance(inst) = recv {
        let inst = inst.lock().unwrap();
        let state = get_channel_state(&inst)?;
        
        let (tx, rx) = oneshot::channel();
        
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let (receiver_arc, pending_count) = {
                    let state_guard = state.lock().unwrap();
                    (state_guard.receiver.clone(), state_guard.pending_count.clone())
                };
                
                if let Some(receiver_arc) = receiver_arc {
                    let mut receiver = receiver_arc.lock().await;
                    match receiver.recv().await {
                        Some(value) => {
                            // Successfully received, decrement pending count
                            pending_count.fetch_sub(1, Ordering::SeqCst);
                            let _ = tx.send(Ok(value));
                        }
                        None => { let _ = tx.send(Ok(Value::Null)); } // Channel closed and empty
                    }
                } else {
                    let _ = tx.send(Err("Channel receiver dropped".to_string()));
                }
            });
        } else {
            return Err("No tokio runtime available".to_string());
        }
        
        Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture { receiver: rx })))))
    } else {
        Err("receive() must be called on a Channel instance".to_string())
    }
}

/// channel.tryReceive() - Non-blocking receive, returns value or null
fn channel_try_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    
    if let Value::Instance(inst) = recv {
        let inst = inst.lock().unwrap();
        let state = get_channel_state(&inst)?;
        let state_guard = state.lock().unwrap();
        
        if let Some(receiver_arc) = &state_guard.receiver {
            // Try to get lock without blocking
            match receiver_arc.try_lock() {
                Ok(mut receiver) => {
                    match receiver.try_recv() {
                        Ok(value) => Ok(value),
                        Err(_) => Ok(Value::Null), // Empty or closed
                    }
                }
                Err(_) => Ok(Value::Null), // Lock held by another
            }
        } else {
            Ok(Value::Null)
        }
    } else {
        Err("tryReceive() must be called on a Channel instance".to_string())
    }
}

/// channel.close() - Close the channel
fn channel_close(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    
    if let Value::Instance(inst) = recv {
        let inst = inst.lock().unwrap();
        let state = get_channel_state(&inst)?;
        let mut state_guard = state.lock().unwrap();
        
        state_guard.closed = true;
        state_guard.sender = None; // Drop sender to close channel
        
        Ok(Value::Null)
    } else {
        Err("close() must be called on a Channel instance".to_string())
    }
}

/// channel.isClosed() - Check if channel is closed AND has no pending messages
/// Returns true only when there are no more messages to receive
fn channel_is_closed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    
    if let Value::Instance(inst) = recv {
        let inst = inst.lock().unwrap();
        let state = get_channel_state(&inst)?;
        let state_guard = state.lock().unwrap();
        
        // Channel is truly "closed" only when:
        // 1. close() has been called (closed flag is true)
        // 2. AND there are no pending messages to receive
        let is_truly_closed = state_guard.closed && 
            state_guard.pending_count.load(Ordering::SeqCst) == 0;
        
        Ok(Value::Boolean(is_truly_closed))
    } else {
        Err("isClosed() must be called on a Channel instance".to_string())
    }
}
