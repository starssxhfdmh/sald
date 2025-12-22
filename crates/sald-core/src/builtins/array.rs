// Array built-in class
// Instance methods for array operations
// Uses Arc/Mutex for thread-safety

use super::{check_arity, get_number_arg, get_string_arg};
use crate::vm::caller::{CallableNativeInstanceFn, ValueCaller};
use crate::vm::value::{Class, NativeInstanceFn, Value};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub fn create_array_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();
    let mut callable_methods: FxHashMap<String, CallableNativeInstanceFn> = FxHashMap::default();

    // Regular instance methods (no closure calls)
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

    // Callable instance methods (can call closures)
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

    // Create class with all method types
    let mut class = Class::new_with_instance("Array", instance_methods, Some(array_constructor));
    class.callable_native_instance_methods = callable_methods;
    class
}

fn array_constructor(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        Ok(Value::Array(Arc::new(Mutex::new(Vec::new()))))
    } else if args.len() == 1 {
        if let Value::Number(n) = &args[0] {
            let size = *n as usize;
            let arr: Vec<Value> = vec![Value::Null; size];
            Ok(Value::Array(Arc::new(Mutex::new(arr))))
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
        Ok(Value::Number(arr.lock().len() as f64))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_push(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let mut arr = arr.lock();
        arr.push(args[0].clone());
        Ok(Value::Number(arr.len() as f64)) // Return new length like JavaScript
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_pop(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.lock().pop().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.shift() - Remove and return first element (in-place mutation)
fn array_shift(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let mut guard = arr.lock();
        if guard.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(guard.remove(0))
        }
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.unshift(value) - Add element to the beginning (in-place mutation)
fn array_unshift(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument".to_string());
    }
    if let Value::Array(arr) = recv {
        let mut guard = arr.lock();
        // Add all arguments to the beginning in order
        for (i, arg) in args.iter().enumerate() {
            guard.insert(i, arg.clone());
        }
        Ok(Value::Number(guard.len() as f64))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.removeAt(index) - Remove element at index (in-place mutation)
fn array_remove_at(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let idx = get_number_arg(&args[0], "index")? as i64;
    if let Value::Array(arr) = recv {
        let mut guard = arr.lock();
        let len = guard.len() as i64;
        // Handle negative indices
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

/// array.splice(start, deleteCount?, ...items) - Remove/replace elements (in-place mutation)
fn array_splice(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument (start)".to_string());
    }
    let start = get_number_arg(&args[0], "start")? as i64;

    if let Value::Array(arr) = recv {
        let mut guard = arr.lock();
        let len = guard.len() as i64;

        // Normalize start index
        let actual_start = if start < 0 {
            (len + start).max(0) as usize
        } else {
            (start as usize).min(len as usize)
        };

        // Get delete count (default: rest of array)
        let delete_count = if args.len() > 1 {
            let dc = get_number_arg(&args[1], "deleteCount")? as i64;
            dc.max(0) as usize
        } else {
            guard.len() - actual_start
        };

        // Items to insert (args[2..])
        let insert_items: Vec<Value> = args.iter().skip(2).cloned().collect();

        // Remove elements
        let mut removed = Vec::new();
        let end_idx = (actual_start + delete_count).min(guard.len());
        for _ in actual_start..end_idx {
            if actual_start < guard.len() {
                removed.push(guard.remove(actual_start));
            }
        }

        // Insert new elements at start position
        for (i, item) in insert_items.iter().enumerate() {
            guard.insert(actual_start + i, item.clone());
        }

        // Return removed elements as new array
        Ok(Value::Array(Arc::new(Mutex::new(removed))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_first(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.lock().first().cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_last(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(arr.lock().last().cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_get(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as usize;
        Ok(arr.lock().get(idx).cloned().unwrap_or(Value::Null))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_set(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as usize;
        let mut arr = arr.lock();
        if idx < arr.len() {
            arr[idx] = args[1].clone();
            drop(arr);
            Ok(recv.clone()) // Return array for chaining
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
        let found = arr.lock().iter().any(|v| v == &args[0]);
        Ok(Value::Boolean(found))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_index_of(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = arr.lock().iter().position(|v| v == &args[0]);
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
        let items: Vec<String> = arr.lock().iter().map(|v| format!("{}", v)).collect();
        Ok(Value::String(Arc::from(items.join(&sep))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_reverse(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        arr.lock().reverse();
        Ok(recv.clone()) // Return the array for method chaining
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let items: Vec<String> = arr.lock().iter().map(|v| format!("{}", v)).collect();
        Ok(Value::String(Arc::from(format!("[{}]", items.join(", ")))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_slice(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
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
        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_concat(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        if let Value::Array(other) = &args[0] {
            let mut result = arr.lock().clone();
            result.extend(other.lock().iter().cloned());
            Ok(Value::Array(Arc::new(Mutex::new(result))))
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
        arr.lock().clear();
        Ok(recv.clone()) // Return array for chaining
    } else {
        Err("Receiver must be an array".to_string())
    }
}

fn array_is_empty(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        Ok(Value::Boolean(arr.lock().is_empty()))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.keys() - Return array of indices [0, 1, 2, ...]
fn array_keys(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let len = arr.lock().len();
        let keys: Vec<Value> = (0..len).map(|i| Value::Number(i as f64)).collect();
        Ok(Value::Array(Arc::new(Mutex::new(keys))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

// ==================== Callable Array Methods (use closures) ====================

/// array.map(fn) - Transform each element, return new array
fn array_map(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
        let mut result = Vec::with_capacity(arr_ref.len());

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            result.push(call_result);
        }

        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.filter(fn) - Filter elements where fn returns truthy
fn array_filter(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
        let mut result = Vec::new();

        for item in arr_ref.iter() {
            let call_result = caller.call(callback, vec![item.clone()])?;
            if call_result.is_truthy() {
                result.push(item.clone());
            }
        }

        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.forEach(fn) - Call fn for each element, return null
fn array_for_each(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();

        for item in arr_ref.iter() {
            caller.call(callback, vec![item.clone()])?;
        }

        Ok(Value::Null)
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.reduce(fn, initial?) - Reduce to single value
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
        let arr_ref = arr.lock();

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

/// array.find(fn) - Find first element where fn returns truthy
fn array_find(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();

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

/// array.findIndex(fn) - Find index of first match, or -1
fn array_find_index(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();

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

/// array.some(fn) - Return true if any element matches
fn array_some(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();

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

/// array.every(fn) - Return true if all elements match
fn array_every(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();

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

/// array.sort(fn?) - Sort in-place with optional comparator
fn array_sort(recv: &Value, args: &[Value], caller: &mut dyn ValueCaller) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }

    if let Value::Array(arr) = recv {
        let mut arr_mut = arr.lock();

        if args.is_empty() {
            // Default sort: convert to string and compare
            arr_mut.sort_by(|a, b| {
                let a_str = format!("{}", a);
                let b_str = format!("{}", b);
                a_str.cmp(&b_str)
            });
        } else {
            // Custom comparator
            let comparator = &args[0];
            let mut items: Vec<Value> = arr_mut.drain(..).collect();

            // Use simple bubble sort to avoid closure issues with sort_by
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

        // Return the array itself for chaining
        Ok(recv.clone())
    } else {
        Err("Receiver must be an array".to_string())
    }
}

// ==================== New Array Methods ====================

/// array.at(index) - Access element with negative index support
fn array_at(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::Array(arr) = recv {
        let idx = get_number_arg(&args[0], "index")? as i64;
        let arr_ref = arr.lock();
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

/// array.fill(value, start?, end?) - Fill array with value (mutating)
fn array_fill(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err(format!("Expected 1-3 arguments but got {}", args.len()));
    }
    if let Value::Array(arr) = recv {
        let value = args[0].clone();
        let mut arr_mut = arr.lock();
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

/// Helper: flatten an array recursively to given depth
fn flatten_array(arr: &[Value], depth: i64, result: &mut Vec<Value>) {
    for item in arr {
        if depth > 0 {
            if let Value::Array(inner) = item {
                let inner_ref = inner.lock();
                flatten_array(&inner_ref, depth - 1, result);
                continue;
            }
        }
        result.push(item.clone());
    }
}

/// array.flat(depth?) - Flatten nested arrays
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
        let arr_ref = arr.lock();
        let mut result = Vec::new();
        flatten_array(&arr_ref, depth, &mut result);
        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.toReversed() - Return new reversed array (non-mutating)
fn array_to_reversed(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
        let mut result: Vec<Value> = arr_ref.clone();
        result.reverse();
        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.flatMap(fn) - Map then flatten one level
fn array_flat_map(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let callback = &args[0];

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
        let mut result = Vec::new();

        for item in arr_ref.iter() {
            let mapped = caller.call(callback, vec![item.clone()])?;
            // Flatten one level if result is array
            if let Value::Array(inner) = mapped {
                let inner_ref = inner.lock();
                result.extend(inner_ref.iter().cloned());
            } else {
                result.push(mapped);
            }
        }
        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}

/// array.toSorted(fn?) - Return new sorted array (non-mutating)
fn array_to_sorted(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn ValueCaller,
) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }

    if let Value::Array(arr) = recv {
        let arr_ref = arr.lock();
        let mut result: Vec<Value> = arr_ref.clone();

        if args.is_empty() {
            // Default sort: convert to string and compare
            result.sort_by(|a, b| {
                let a_str = format!("{}", a);
                let b_str = format!("{}", b);
                a_str.cmp(&b_str)
            });
        } else {
            // Custom comparator with bubble sort
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
        Ok(Value::Array(Arc::new(Mutex::new(result))))
    } else {
        Err("Receiver must be an array".to_string())
    }
}
