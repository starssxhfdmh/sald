use super::{check_arity, check_arity_range, get_number_arg};
use crate::vm::value::{Class, Instance, NativeInstanceFn, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

struct ChannelState {
    buffer: VecDeque<Value>,
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

fn channel_new(args: &[Value]) -> Result<Value, String> {
    check_arity_range(0, 1, args.len())?;

    let buffer_size = if args.is_empty() {
        16
    } else {
        let size = get_number_arg(&args[0], "bufferSize")? as usize;
        if size == 0 {
            1
        } else {
            size
        }
    };

    let class = Rc::new(create_channel_class());
    let mut instance = Instance::new(class);

    let state = Rc::new(RefCell::new(ChannelState {
        buffer: VecDeque::with_capacity(buffer_size),
        closed: false,
    }));

    instance.fields.insert(
        "_state".to_string(),
        Value::Number(Rc::into_raw(state) as usize as f64),
    );

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}

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

fn channel_try_receive(recv: &Value, args: &[Value]) -> Result<Value, String> {
    channel_receive(recv, args)
}

fn channel_close(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let mut state_ref = state.borrow_mut();

        state_ref.closed = true;

        Ok(Value::Null)
    } else {
        Err("close() must be called on a Channel instance".to_string())
    }
}

fn channel_is_closed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let state_ref = state.borrow();

        let is_truly_closed = state_ref.closed && state_ref.buffer.is_empty();

        Ok(Value::Boolean(is_truly_closed))
    } else {
        Err("isClosed() must be called on a Channel instance".to_string())
    }
}

fn channel_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let state_ref = state.borrow();

        Ok(Value::Boolean(state_ref.buffer.is_empty()))
    } else {
        Err("isEmpty() must be called on a Channel instance".to_string())
    }
}

fn channel_len(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let state = get_channel_state(&inst)?;
        let state_ref = state.borrow();

        Ok(Value::Number(state_ref.buffer.len() as f64))
    } else {
        Err("len() must be called on a Channel instance".to_string())
    }
}
