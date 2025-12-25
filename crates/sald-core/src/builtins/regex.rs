use super::{check_arity, check_arity_range, get_string_arg};
use crate::vm::value::{Class, Instance, NativeInstanceFn, NativeStaticFn, Value};
use regex::Regex as RustRegex;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_regex_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();

    static_methods.insert("new".to_string(), regex_new);

    instance_methods.insert("test".to_string(), regex_test);
    instance_methods.insert("match".to_string(), regex_match);
    instance_methods.insert("matchAll".to_string(), regex_match_all);
    instance_methods.insert("replace".to_string(), regex_replace);
    instance_methods.insert("replaceAll".to_string(), regex_replace_all);
    instance_methods.insert("split".to_string(), regex_split);
    instance_methods.insert("pattern".to_string(), regex_pattern);
    instance_methods.insert("flags".to_string(), regex_flags);

    let mut class = Class::new_with_instance("Regex", instance_methods, None);
    class.native_static_methods = static_methods;
    class
}

fn get_regex_from_instance(inst: &Instance) -> Result<RustRegex, String> {
    let pattern = inst
        .fields
        .get("_pattern")
        .and_then(|v| {
            if let Value::String(s) = v {
                Some(s.to_string())
            } else {
                None
            }
        })
        .ok_or("Invalid regex instance: missing pattern")?;

    let flags = inst
        .fields
        .get("_flags")
        .and_then(|v| {
            if let Value::String(s) = v {
                Some(s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    build_regex(&pattern, &flags)
}

fn build_regex(pattern: &str, flags: &str) -> Result<RustRegex, String> {
    let case_insensitive = flags.contains('i');
    let multiline = flags.contains('m');
    let dot_all = flags.contains('s');

    let mut regex_pattern = String::new();

    if case_insensitive || multiline || dot_all {
        regex_pattern.push_str("(?");
        if case_insensitive {
            regex_pattern.push('i');
        }
        if multiline {
            regex_pattern.push('m');
        }
        if dot_all {
            regex_pattern.push('s');
        }
        regex_pattern.push(')');
    }

    regex_pattern.push_str(pattern);

    RustRegex::new(&regex_pattern).map_err(|e| format!("Invalid regex pattern: {}", e))
}

fn regex_new(args: &[Value]) -> Result<Value, String> {
    check_arity_range(1, 2, args.len())?;
    let pattern = get_string_arg(&args[0], "pattern")?;
    let flags = if args.len() > 1 {
        get_string_arg(&args[1], "flags")?
    } else {
        String::new()
    };

    build_regex(&pattern, &flags)?;

    let class = Rc::new(create_regex_class());
    let mut instance = Instance::new(class);
    instance
        .fields
        .insert("_pattern".to_string(), Value::String(Rc::from(pattern)));
    instance
        .fields
        .insert("_flags".to_string(), Value::String(Rc::from(flags)));

    Ok(Value::Instance(Rc::new(RefCell::new(instance))))
}

fn regex_test(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;

        Ok(Value::Boolean(regex.is_match(&input)))
    } else {
        Err("test() must be called on a Regex instance".to_string())
    }
}

fn regex_match(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;

        if let Some(captures) = regex.captures(&input) {
            let mut matches = Vec::new();
            for cap in captures.iter() {
                if let Some(m) = cap {
                    matches.push(Value::String(Rc::from(m.as_str().to_string())));
                } else {
                    matches.push(Value::Null);
                }
            }
            Ok(Value::Array(Rc::new(RefCell::new(matches))))
        } else {
            Ok(Value::Null)
        }
    } else {
        Err("match() must be called on a Regex instance".to_string())
    }
}

fn regex_match_all(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;

        let mut all_matches = Vec::new();

        for captures in regex.captures_iter(&input) {
            let mut match_group = Vec::new();
            for cap in captures.iter() {
                if let Some(m) = cap {
                    match_group.push(Value::String(Rc::from(m.as_str().to_string())));
                } else {
                    match_group.push(Value::Null);
                }
            }
            all_matches.push(Value::Array(Rc::new(RefCell::new(match_group))));
        }

        Ok(Value::Array(Rc::new(RefCell::new(all_matches))))
    } else {
        Err("matchAll() must be called on a Regex instance".to_string())
    }
}

fn regex_replace(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;
        let replacement = get_string_arg(&args[1], "replacement")?;

        let result = regex.replace(&input, replacement.as_str());
        Ok(Value::String(Rc::from(result.into_owned())))
    } else {
        Err("replace() must be called on a Regex instance".to_string())
    }
}

fn regex_replace_all(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;
        let replacement = get_string_arg(&args[1], "replacement")?;

        let result = regex.replace_all(&input, replacement.as_str());
        Ok(Value::String(Rc::from(result.into_owned())))
    } else {
        Err("replaceAll() must be called on a Regex instance".to_string())
    }
}

fn regex_split(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        let regex = get_regex_from_instance(&inst)?;
        let input = get_string_arg(&args[0], "string")?;

        let parts: Vec<Value> = regex
            .split(&input)
            .map(|s| Value::String(Rc::from(s.to_string())))
            .collect();

        Ok(Value::Array(Rc::new(RefCell::new(parts))))
    } else {
        Err("split() must be called on a Regex instance".to_string())
    }
}

fn regex_pattern(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        if let Some(pattern) = inst.fields.get("_pattern") {
            Ok(pattern.clone())
        } else {
            Ok(Value::String(Rc::from(String::new())))
        }
    } else {
        Err("pattern() must be called on a Regex instance".to_string())
    }
}

fn regex_flags(recv: &Value, args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;

    if let Value::Instance(inst) = recv {
        let inst = inst.borrow();
        if let Some(flags) = inst.fields.get("_flags") {
            Ok(flags.clone())
        } else {
            Ok(Value::String(Rc::from(String::new())))
        }
    } else {
        Err("flags() must be called on a Regex instance".to_string())
    }
}
