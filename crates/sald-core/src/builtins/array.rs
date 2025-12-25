use super::{check_arity, get_number_arg, get_string_arg};
use crate::vm::caller::{CallableNativeInstanceFn, ValueCaller};
use crate::vm::value::{Class, NativeInstanceFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_array_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();
    let mut callable_methods: FxHashMap<String, CallableNativeInstanceFn> = FxHashMap::default();

    instance_methods.insert("length".to_string(), array_length);
    instance_methods.insert("push".to_string(), array_push);
    instance_methods.insert("pop".to_string(), array_pop);
    instance_methods.insert("shift".to_string(), array_shift);
    instance_methods.insert("unshift".to_string(), array_unshift);
    instance_methods.insert("removeAt".to_string(), array_remove_at);
    instance_methods.insert("splice".to_string(), array_splice);
    instance_methods.insert("first".to_string(), array_first);
    instance_methods.insert("last".to_string(), array_last);
    instance_methods.insert("get".to_string(), array_get);
    instance_methods.insert("set".to_string(), array_set);
    instance_methods.insert("contains".to_string(), array_contains);
    instance_methods.insert("indexOf".to_string(), array_index_of);
    instance_methods.insert("join".to_string(), array_join);
    instance_methods.insert("reverse".to_string(), array_reverse);
    instance_methods.insert("toString".to_string(), array_to_string);
    instance_methods.insert("slice".to_string(), array_slice);
    instance_methods.insert("concat".to_string(), array_concat);
    instance_methods.insert("clear".to_string(), array_clear);
    instance_methods.insert("isEmpty".to_string(), array_is_empty);
    instance_methods.insert("keys".to_string(), array_keys);
    instance_methods.insert("at".to_string(), array_at);
    instance_methods.insert("fill".to_string(), array_fill);
    instance_methods.insert("flat".to_string(), array_flat);
    instance_methods.insert("toReversed".to_string(), array_to_reversed);

    callable_methods.insert("map".to_string(), array_map);
    callable_methods.insert("filter".to_string(), array_filter);
    callable_methods.insert("forEach".to_string(), array_for_each);
    callable_methods.insert("reduce".to_string(), array_reduce);
    callable_methods.insert("find".to_string(), array_find);
    callable_methods.insert("findIndex".to_string(), array_find_index);
    callable_methods.insert("some".to_string(), array_some);
    callable_methods.insert("every".to_string(), array_every);
    callable_methods.insert("sort".to_string(), array_sort);
    callable_methods.insert("flatMap".to_string(), array_flat_map);
    callable_methods.insert("toSorted".to_string(), array_to_sorted);

    let mut class = Class::new_with_instance("Array", instance_methods, Some(array_constructor));
    class.callable_native_instance_methods = callable_methods;
    class
}

fn array_constructor(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        Ok(Value::Array(Rc::new(RefCell::new(Vec::new()))))
    } else if args.len() == 1 {
        if let Value::Number(n) = &args[0] {
            let size = *n as usize;
            let arr: Vec<Value> = vec![Value::Null; size];
            Ok(Value::Array(Rc::new(RefCell::new(arr))))
        } else {
            Err(format!("Expected a number, got {}", args[0].type_name()))
        }
    } else {
        Err(format!("Expected 0-1 arguments but got {}", args.len()))
    }
}

fn array_length(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(Value::Number(arr.borrow().len() as f64))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_push(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let mut arr = arr.borrow_mut();
        arr.push(args[0].clone());
        Ok(Value::Number(arr.len() as f64))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_pop(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.borrow_mut().pop().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_shift(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let mut guard = arr.borrow_mut();
        if guard.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(guard.remove(0))
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_unshift(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument".to_string());
    }
    if let Value::Array(arr) = recv {
        let mut guard = arr.borrow_mut();

        for (i, arg) in args.iter().enumerate() {
            guard.insert(i, arg.clone());
        }
        Ok(Value::Number(guard.len() as f64))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_remove_at(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let idx = get_number_arg(&args[0], "index")? as i64;
    if let Value::Array(arr) = recv {
        let mut guard = arr.borrow_mut();
        let len = guard.len() as i64;

        let actual_idx = if idx < 0 { len + idx } else { idx };
        if actual_idx < 0 || actual_idx >= len {
            Ok(Value::Null)
        } else {
            Ok(guard.remove(actual_idx as usize))
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_splice(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument (start)".to_string());
    }
    let start = get_number_arg(&args[0], "start")? as i64;

    if let Value::Array(arr) = recv {
        let mut guard = arr.borrow_mut();
        let len = guard.len() as i64;

        let actual_start = if start < 0 {
            (len + start).max(0) as usize
        } else {
            (start as usize).min(len as usize)
        };

        let delete_count = if args.len() > 1 {
            let dc = get_number_arg(&args[1], "deleteCount")? as i64;
            dc.max(0) as usize
        } else {
            guard.len() - actual_start
        };

        let insert_items: Vec<Value> = args.iter().skip(2).cloned().collect();

        let mut removed = Vec::new();
        let end_idx = (actual_start + delete_count).min(guard.len());
        for _ in actual_start..end_idx {
            if actual_start < guard.len() {
                removed.push(guard.remove(actual_start));
            }
        }

        for (i, item) in insert_items.iter().enumerate() {
            guard.insert(actual_start + i, item.clone());
        }

        Ok(Value::Array(Rc::new(RefCell::new(removed))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_first(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.borrow().first().cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_last(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.borrow().last().cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_get(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as usize;
        Ok(arr.borrow().get(idx).cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_set(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as usize;
        let mut arr = arr.borrow_mut();
        if idx < arr.len() {
            arr[idx] = args[1].clone();
            drop(arr);
            Ok(recv.clone())
        } else {
            Err(format!(
                "Index {} out of bounds for array of length {}",
                idx,
                arr.len()
            ))
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_contains(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let found = arr.borrow().iter().any(|v| v == &args[0]);
        Ok(Value::Boolean(found))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_index_of(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = arr.borrow().iter().position(|v| v == &args[0]);
        match idx {
            Some(i) => Ok(Value::Number(i as f64)),
            None => Ok(Value::Number(-1.0)),
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_join(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let sep = get_string_arg(&args[0], "separator")?;
        let items: Vec<String> = arr.borrow().iter().map(|v| format!("{}", v)).collect();
        Ok(Value::String(Rc::from(items.join(&sep))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_reverse(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        arr.borrow_mut().reverse();
        Ok(recv.clone())
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let items: Vec<String> = arr.borrow().iter().map(|v| format!("{}", v)).collect();
        Ok(Value::String(Rc::from(format!("[{}]", items.join(", ")))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_slice(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let len = arr_ref.len() as i64;

        let start = get_number_arg(&args[0], "start")? as i64;
        let start = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;

        let end = if args.len() == 2 {
            let e = get_number_arg(&args[1], "end")? as i64;
            (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
        } else {
            len as usize
        };

        let result: Vec<Value> = if start < end {
            arr_ref[start..end].to_vec()
        } else {
            Vec::new()
        };
        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_concat(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        if let Value::Array(other) = &args[0] {
            let mut result = arr.borrow().clone();
            result.extend(other.borrow().iter().cloned());
            Ok(Value::Array(Rc::new(RefCell::new(result))))
        } else {
            Err(format!(
                "Argument 'other' must be an array, got {}",
                args[0].type_name()
            ))
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_clear(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        arr.borrow_mut().clear();
        Ok(recv.clone())
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(Value::Boolean(arr.borrow().is_empty()))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_keys(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let len = arr.borrow().len();
        let keys: Vec<Value> = (0..len).map(|i| Value::Number(i as f64)).collect();
        Ok(Value::Array(Rc::new(RefCell::new(keys))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_map(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let mut result = Vec::with_capacity(arr_ref.len());

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            result.push(call_result);
        }

        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_filter(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let mut result = Vec::new();

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if call_result.is_truthy() {
                result.push(item.clone());
            }
        }

        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_for_each(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        for item in arr_ref.iter() {
            caller.call(callback, vec![item.clone()])?;
        }

        Ok(Value::Null)
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_reduce(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        if arr_ref.is_empty() && args.len() == 1 {
            return Err("Cannot reduce empty array with no initial value".to_string());
        }

        let (mut accumulator, start_idx) = if args.len() == 2 {
            (args[1].clone(), 0)
        } else {
            (arr_ref[0].clone(), 1)
        };

        for item in arr_ref.iter().skip(start_idx) {
            accumulator = caller.call(callback, vec![accumulator, item.clone()])?;
        }

        Ok(accumulator)
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_find(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if call_result.is_truthy() {
                return Ok(item.clone());
            }
        }

        Ok(Value::Null)
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_find_index(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        for (i, item) in arr_ref.iter().enumerate() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if call_result.is_truthy() {
                return Ok(Value::Number(i as f64));
            }
        }

        Ok(Value::Number(-1.0))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_some(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if call_result.is_truthy() {
                return Ok(Value::Boolean(true));
            }
        }

        Ok(Value::Boolean(false))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_every(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if !call_result.is_truthy() {
                return Ok(Value::Boolean(false));
            }
        }

        Ok(Value::Boolean(true))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_sort(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }

    if let Value::Array(arr) = recv {
        let mut arr_mut = arr.borrow_mut();

        if args.is_empty() {
            arr_mut.sort_by(|a, b| {
                let a_str = format!("{}", a);
                let b_str = format!("{}", b);
                a_str.cmp(&b_str)
            });
        } else {
            let comparator = &args[0];
            let mut items: Vec<Value> = arr_mut.drain(..).collect();

            let len = items.len();
            for i in 0..len {
                for j in 0..(len - 1 - i) {
                    let cmp_result =
                        caller.call(comparator, vec![items[j].clone(), items[j + 1].clone()])?;
                    if let Value::Number(n) = cmp_result {
                        if n > 0.0 {
                            items.swap(j, j + 1);
                        }
                    }
                }
            }

            *arr_mut = items;
        }

        Ok(recv.clone())
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_at(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as i64;
        let arr_ref = arr.borrow();
        let len = arr_ref.len() as i64;
        let actual_idx = if idx < 0 { len + idx } else { idx };
        if actual_idx < 0 || actual_idx >= len {
            Ok(Value::Null)
        } else {
            Ok(arr_ref[actual_idx as usize].clone())
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_fill(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err(format!("Expected 1-3 arguments but got {}", args.len()));
    }
    if let Value::Array(arr) = recv {
        let value = args[0].clone();
        let mut arr_mut = arr.borrow_mut();
        let len = arr_mut.len() as i64;

        let start = if args.len() > 1 {
            let s = get_number_arg(&args[1], "start")? as i64;
            if s < 0 {
                (len + s).max(0) as usize
            } else {
                s.min(len) as usize
            }
        } else {
            0
        };

        let end = if args.len() > 2 {
            let e = get_number_arg(&args[2], "end")? as i64;
            if e < 0 {
                (len + e).max(0) as usize
            } else {
                e.min(len) as usize
            }
        } else {
            len as usize
        };

        for i in start..end {
            if i < arr_mut.len() {
                arr_mut[i] = value.clone();
            }
        }
        Ok(recv.clone())
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn flatten_array(arr: &[Value], depth: i64, result: &mut Vec<Value>) {
    for item in arr {
        if depth > 0 {
            if let Value::Array(inner) = item {
                let inner_ref = inner.borrow();
                flatten_array(&inner_ref, depth - 1, result);
                continue;
            }
        }
        result.push(item.clone());
    }
}

fn array_flat(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }
    if let Value::Array(arr) = recv {
        let depth = if args.is_empty() {
            1
        } else {
            get_number_arg(&args[0], "depth")? as i64
        };
        let arr_ref = arr.borrow();
        let mut result = Vec::new();
        flatten_array(&arr_ref, depth, &mut result);
        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_to_reversed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let mut result: Vec<Value> = arr_ref.clone();
        result.reverse();
        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_flat_map(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let mut result = Vec::new();

        for item in arr_ref.iter() {
            let mapped = caller.call(callback, vec![item.clone()])?;

            if let Value::Array(inner) = mapped {
                let inner_ref = inner.borrow();
                result.extend(inner_ref.iter().cloned());
            } else {
                result.push(mapped);
            }
        }
        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_to_sorted(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }

    if let Value::Array(arr) = recv {
        let arr_ref = arr.borrow();
        let mut result: Vec<Value> = arr_ref.clone();

        if args.is_empty() {
            result.sort_by(|a, b| {
                let a_str = format!("{}", a);
                let b_str = format!("{}", b);
                a_str.cmp(&b_str)
            });
        } else {
            let comparator = &args[0];
            let len = result.len();
            for i in 0..len {
                for j in 0..(len - 1 - i) {
                    let cmp_result =
                        caller.call(comparator, vec![result[j].clone(), result[j + 1].clone()])?;
                    if let Value::Number(n) = cmp_result {
                        if n > 0.0 {
                            result.swap(j, j + 1);
                        }
                    }
                }
            }
        }
        Ok(Value::Array(Rc::new(RefCell::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}
