use super::{check_arity, get_number_arg, get_string_arg};
use crate::vm::value::{Class, NativeInstanceFn, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_string_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("fromCharCode".to_string(), string_from_char_code);
    static_methods.insert("charCodeAt".to_string(), string_char_code_at);

    instance_methods.insert("length".to_string(), string_length);
    instance_methods.insert("upper".to_string(), string_upper);
    instance_methods.insert("lower".to_string(), string_lower);
    instance_methods.insert("trim".to_string(), string_trim);
    instance_methods.insert("contains".to_string(), string_contains);
    instance_methods.insert("startsWith".to_string(), string_starts_with);
    instance_methods.insert("endsWith".to_string(), string_ends_with);
    instance_methods.insert("charAt".to_string(), string_char_at);
    instance_methods.insert("charCodeAt".to_string(), string_char_at_code);
    instance_methods.insert("indexOf".to_string(), string_index_of);
    instance_methods.insert("replace".to_string(), string_replace);
    instance_methods.insert("split".to_string(), string_split);
    instance_methods.insert("substring".to_string(), string_substring);
    instance_methods.insert("slice".to_string(), string_slice);
    instance_methods.insert("isDigit".to_string(), string_is_digit);
    instance_methods.insert("toString".to_string(), string_to_string);

    instance_methods.insert("padStart".to_string(), string_pad_start);
    instance_methods.insert("padEnd".to_string(), string_pad_end);
    instance_methods.insert("repeat".to_string(), string_repeat);
    instance_methods.insert("trimStart".to_string(), string_trim_start);
    instance_methods.insert("trimLeft".to_string(), string_trim_start);
    instance_methods.insert("trimEnd".to_string(), string_trim_end);
    instance_methods.insert("trimRight".to_string(), string_trim_end);
    instance_methods.insert("lastIndexOf".to_string(), string_last_index_of);
    instance_methods.insert("replaceAll".to_string(), string_replace_all);
    instance_methods.insert("includes".to_string(), string_contains);

    let mut class = Class::new_with_instance("String", instance_methods, Some(string_constructor));
    class.native_static_methods = static_methods;
    class
}

fn string_from_char_code(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let code = get_number_arg(&args[0], "code")? as u32;
    match char::from_u32(code) {
        Some(c) => Ok(Value::String(Rc::from(c.to_string()))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn string_char_code_at(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    let s = get_string_arg(&args[0], "string")?;
    let idx = if args.len() == 2 {
        get_number_arg(&args[1], "index")? as usize
    } else {
        0
    };

    match s.chars().nth(idx) {
        Some(c) => Ok(Value::Number(c as u32 as f64)),
        None => Ok(Value::Number(f64::NAN)),
    }
}

fn string_constructor(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    Ok(Value::String(Rc::from(format!("{}", args[0]))))
}

fn string_length(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::Number(s.len() as f64))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_upper(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::from(s.to_uppercase())))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_lower(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::from(s.to_lowercase())))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_trim(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::from(s.trim().to_string())))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_contains(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let substr = get_string_arg(&args[0], "substring")?;
        Ok(Value::Boolean(s.contains(&substr)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_starts_with(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let prefix = get_string_arg(&args[0], "prefix")?;
        Ok(Value::Boolean(s.starts_with(&prefix)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_ends_with(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let suffix = get_string_arg(&args[0], "suffix")?;
        Ok(Value::Boolean(s.ends_with(&suffix)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_char_at(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let idx = get_number_arg(&args[0], "index")? as usize;
        match s.chars().nth(idx) {
            Some(c) => Ok(Value::String(Rc::from(c.to_string()))),
            None => Ok(Value::Null),
        }
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_char_at_code(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("Expected 0-1 arguments but got {}", args.len()));
    }

    if let Value::String(s) = recv {
        let idx = if args.is_empty() {
            0
        } else {
            get_number_arg(&args[0], "index")? as usize
        };

        match s.chars().nth(idx) {
            Some(c) => Ok(Value::Number(c as u32 as f64)),
            None => Ok(Value::Number(f64::NAN)),
        }
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_index_of(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }

    if let Value::String(s) = recv {
        let search = get_string_arg(&args[0], "search")?;

        if search.is_empty() {
            return Ok(Value::Number(0.0));
        }

        let from_char_index = if args.len() == 2 {
            get_number_arg(&args[1], "fromIndex")? as usize
        } else {
            0
        };

        let chars: Vec<char> = s.chars().collect();
        let search_chars: Vec<char> = search.chars().collect();

        if from_char_index >= chars.len() {
            return Ok(Value::Number(-1.0));
        }

        let search_len = search_chars.len();
        for i in from_char_index..=chars.len().saturating_sub(search_len) {
            let mut found = true;
            for j in 0..search_len {
                if chars[i + j] != search_chars[j] {
                    found = false;
                    break;
                }
            }
            if found {
                return Ok(Value::Number(i as f64));
            }
        }

        Ok(Value::Number(-1.0))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_replace(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    if let Value::String(s) = recv {
        let old = get_string_arg(&args[0], "pattern")?;
        let new = get_string_arg(&args[1], "replacement")?;
        Ok(Value::String(Rc::from(s.replace(&old, &new))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_split(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let sep = get_string_arg(&args[0], "separator")?;
        let parts: Vec<Value> = s
            .split(&sep)
            .map(|p| Value::String(Rc::from(p.to_string())))
            .collect();
        Ok(Value::Array(Rc::new(RefCell::new(parts))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_to_string(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::clone(s)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_substring(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }

    if let Value::String(s) = recv {
        let start = get_number_arg(&args[0], "start")? as usize;
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();

        if start > len {
            return Ok(Value::String(Rc::from(String::new())));
        }

        let end = if args.len() == 2 {
            let e = get_number_arg(&args[1], "end")? as usize;
            e.min(len)
        } else {
            len
        };

        if end <= start {
            return Ok(Value::String(Rc::from(String::new())));
        }

        let result: String = chars[start..end].iter().collect();
        Ok(Value::String(Rc::from(result)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_is_digit(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        if s.is_empty() {
            return Ok(Value::Boolean(false));
        }
        let first_char = s.chars().next().unwrap();
        Ok(Value::Boolean(first_char.is_ascii_digit()))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_slice(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }

    if let Value::String(s) = recv {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len() as i64;

        let start_arg = get_number_arg(&args[0], "start")? as i64;
        let start = if start_arg < 0 {
            (len + start_arg).max(0) as usize
        } else {
            (start_arg as usize).min(len as usize)
        };

        let end = if args.len() == 2 {
            let end_arg = get_number_arg(&args[1], "end")? as i64;
            if end_arg < 0 {
                (len + end_arg).max(0) as usize
            } else {
                (end_arg as usize).min(len as usize)
            }
        } else {
            len as usize
        };

        if end <= start {
            return Ok(Value::String(Rc::from(String::new())));
        }

        let result: String = chars[start..end].iter().collect();
        Ok(Value::String(Rc::from(result)))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_pad_start(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    if let Value::String(s) = recv {
        let target_len = get_number_arg(&args[0], "length")? as usize;
        let pad_char = if args.len() == 2 {
            let pad_str = get_string_arg(&args[1], "char")?;
            pad_str.chars().next().unwrap_or(' ')
        } else {
            ' '
        };
        let current_len = s.chars().count();
        if current_len >= target_len {
            return Ok(Value::String(Rc::clone(s)));
        }
        let padding: String = std::iter::repeat(pad_char)
            .take(target_len - current_len)
            .collect();
        Ok(Value::String(Rc::from(format!("{}{}", padding, s))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_pad_end(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    if let Value::String(s) = recv {
        let target_len = get_number_arg(&args[0], "length")? as usize;
        let pad_char = if args.len() == 2 {
            let pad_str = get_string_arg(&args[1], "char")?;
            pad_str.chars().next().unwrap_or(' ')
        } else {
            ' '
        };
        let current_len = s.chars().count();
        if current_len >= target_len {
            return Ok(Value::String(Rc::clone(s)));
        }
        let padding: String = std::iter::repeat(pad_char)
            .take(target_len - current_len)
            .collect();
        Ok(Value::String(Rc::from(format!("{}{}", s, padding))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_repeat(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    if let Value::String(s) = recv {
        let count = get_number_arg(&args[0], "count")? as usize;
        Ok(Value::String(Rc::from(s.repeat(count))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_trim_start(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::from(s.trim_start().to_string())))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_trim_end(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    if let Value::String(s) = recv {
        Ok(Value::String(Rc::from(s.trim_end().to_string())))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_last_index_of(recv: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!("Expected 1-2 arguments but got {}", args.len()));
    }
    if let Value::String(s) = recv {
        let search = get_string_arg(&args[0], "search")?;
        if search.is_empty() {
            return Ok(Value::Number(s.chars().count() as f64));
        }
        let chars: Vec<char> = s.chars().collect();
        let search_chars: Vec<char> = search.chars().collect();
        let from_idx = if args.len() == 2 {
            get_number_arg(&args[1], "from")? as usize
        } else {
            chars.len()
        };
        let search_len = search_chars.len();
        if search_len > chars.len() {
            return Ok(Value::Number(-1.0));
        }

        let max_start = from_idx.min(chars.len().saturating_sub(search_len));
        for i in (0..=max_start).rev() {
            let mut found = true;
            for j in 0..search_len {
                if chars[i + j] != search_chars[j] {
                    found = false;
                    break;
                }
            }
            if found {
                return Ok(Value::Number(i as f64));
            }
        }
        Ok(Value::Number(-1.0))
    } else {
        Err("Receiver must be a string".to_string())
    }
}

fn string_replace_all(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    if let Value::String(s) = recv {
        let old = get_string_arg(&args[0], "pattern")?;
        let new = get_string_arg(&args[1], "replacement")?;
        Ok(Value::String(Rc::from(s.replace(&old, &new))))
    } else {
        Err("Receiver must be a string".to_string())
    }
}
