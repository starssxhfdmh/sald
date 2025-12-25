//! Thread-safe Channel implementation using crossbeam-channel
//! Allows communication between async workers

use super::{check_arity, check_arity_range, get_number_arg};
use crate::vm::value::{Class, Instance, NativeInstanceFn, NativeStaticFn, SendValue, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};

#[cfg(not(target_arch = "wasm32"))]
struct ChannelState {
    sender: Sender<SendValue>,
    receiver: Receiver<SendValue>,
    closed: std::sync::atomic::AtomicBool,
}

#[cfg(target_arch = "wasm32")]
struct ChannelState {
    buffer: std::collections::VecDeque<Value>,
    closed: bool,
}

pub fn create_channel_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    static_methods.insert("new".to_string(), channel_new);

    instance_methods.insert("send".to_string(), channel_send);
    instance_methods.insert("receive".to_string(), channel_receive);
    instance_methods.insert("tryReceive".to_string(), channel_try_receive);
    instance_methods.insert("close".to_string(), channel_close);
    instance_methods.insert("isClosed".to_string(), channel_is_closed);
    instance_methods.insert("isEmpty".to_string(), channel_is_empty);
    instance_methods.insert("len".to_string(), channel_len);

    let mut class = Class::new_with_instance("Channel", instance_methods, Some(channel_new));
    class.native_static_methods = static_methods;
    class
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_new(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;

    let buffer_size = if args.is_empty() {
        16
    } else {
        let size = get_number_arg(&args[0], "bufferSize")? as usize;
        if size == 0 { 1 } else { size }
    };

    let class = Rc::new(create_channel_class());
    let mut instance = Instance::new(class);

    let (sender, receiver) = bounded(buffer_size);
    let state = Box::new(ChannelState {
        sender,
        receiver,
        closed: std::sync::atomic::AtomicBool::new(false),
    });

    // Store raw pointer to thread-safe state
    instance.fields.insert(
        "_state".to_string(),
        Value::Number(Box::into_raw(state) as usize as f64),
    );

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}

#[cfg(target_arch = "wasm32")]
fn channel_new(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;

    let buffer_size = if args.is_empty() {
        16
    } else {
        let size = get_number_arg(&args[0], "bufferSize")? as usize;
        if size == 0 { 1 } else { size }
    };

    let class = Rc::new(create_channel_class());
    let mut instance = Instance::new(class);

    let state = Rc::new(RefCell::new(ChannelState {
        buffer: std::collections::VecDeque::with_capacity(buffer_size),
        closed: false,
    }));

    instance.fields.insert(
        "_state".to_string(),
        Value::Number(Rc::into_raw(state) as usize as f64),
    );

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}

#[cfg(not(target_arch = "wasm32"))]
fn get_channel_state(inst: &Instance) -> Result<&ChannelState, String> {
    let ptr = inst
        .fields
        .get("_state")
        .and_then(|v| {
            if let Value::Number(n) = v {
                Some(*n as usize)
            } else {
                None
            }
        })
        .ok_or("Invalid channel instance")?;

    // Safe because we control the pointer and it's only dropped when Channel is dropped
    let state = unsafe { &*(ptr as *const ChannelState) };
    Ok(state)
}

#[cfg(target_arch = "wasm32")]
fn get_channel_state(inst: &Instance) -> Result<Rc<RefCell<ChannelState>>, String> {
    let ptr = inst
        .fields
        .get("_state")
        .and_then(|v| {
            if let Value::Number(n) = v {
                Some(*n as usize)
            } else {
                None
            }
        })
        .ok_or("Invalid channel instance")?;

    let state = unsafe { Rc::from_raw(ptr as *const RefCell<ChannelState>) };
    let cloned = Rc::clone(&state);
    std::mem::forget(state);
    Ok(cloned)
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_send(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;

        if state.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Cannot send on closed channel".to_string());
        }

        // Convert to SendValue for thread-safe transfer
        let send_val = SendValue::from_value(&args[0])?;
        
        match state.sender.send(send_val) {
            Ok(()) => Ok(Value::Boolean(true)),
            Err(_) => Err("Channel send failed: receiver dropped".to_string()),
        }
    } else {
        Err("send() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_send(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let mut state_ref = state.borrow_mut();

        if state_ref.closed {
            return Err("Cannot send on closed channel".to_string());
        }

        state_ref.buffer.push_back(args[0].clone());
        Ok(Value::Boolean(true))
    } else {
        Err("send() must be called on a Channel instance".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;

        match state.receiver.recv() {
            Ok(send_val) => Ok(send_val.to_value()),
            Err(_) => Ok(Value::Null), // Channel closed or empty
        }
    } else {
        Err("receive() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let mut state_ref = state.borrow_mut();

        match state_ref.buffer.pop_front() {
            Some(value) => Ok(value),
            None => Ok(Value::Null),
        }
    } else {
        Err("receive() must be called on a Channel instance".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_try_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;

        match state.receiver.try_recv() {
            Ok(send_val) => Ok(send_val.to_value()),
            Err(TryRecvError::Empty) => Ok(Value::Null),
            Err(TryRecvError::Disconnected) => Ok(Value::Null),
        }
    } else {
        Err("tryReceive() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_try_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    channel_receive(recv, args)
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_close(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        state.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Value::Null)
    } else {
        Err("close() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_close(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        state.borrow_mut().closed = true;
        Ok(Value::Null)
    } else {
        Err("close() must be called on a Channel instance".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_is_closed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let is_closed = state.closed.load(std::sync::atomic::Ordering::SeqCst);
        let is_empty = state.receiver.is_empty();
        Ok(Value::Boolean(is_closed && is_empty))
    } else {
        Err("isClosed() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_is_closed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let state_ref = state.borrow();
        Ok(Value::Boolean(state_ref.closed && state_ref.buffer.is_empty()))
    } else {
        Err("isClosed() must be called on a Channel instance".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        Ok(Value::Boolean(state.receiver.is_empty()))
    } else {
        Err("isEmpty() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        Ok(Value::Boolean(state.borrow().buffer.is_empty()))
    } else {
        Err("isEmpty() must be called on a Channel instance".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn channel_len(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        Ok(Value::Number(state.receiver.len() as f64))
    } else {
        Err("len() must be called on a Channel instance".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn channel_len(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        Ok(Value::Number(state.borrow().buffer.len() as f64))
    } else {
        Err("len() must be called on a Channel instance".to_string())
    }
}
