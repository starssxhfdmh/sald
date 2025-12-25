use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::path::Path;
use std::rc::Rc;

pub fn create_path_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("join".to_string(), path_join);
    static_methods.insert("dirname".to_string(), path_dirname);
    static_methods.insert("basename".to_string(), path_basename);
    static_methods.insert("extname".to_string(), path_extname);
    static_methods.insert("isAbsolute".to_string(), path_is_absolute);
    static_methods.insert("exists".to_string(), path_exists);
    static_methods.insert("normalize".to_string(), path_normalize);

    Class::new_with_static("Path", static_methods)
}

fn get_string(args: &[Value], idx: usize, name: &str) -> Result<String, String> {
    if idx >= args.len() {
        return Err(format!("Expected at least {} argument(s)", idx + 1));
    }
    match &args[idx] {
        Value::String(s) => Ok(s.to_string()),
        _ => Err(format!(
            "Argument '{}' must be a string, got {}",
            name,
            args[idx].type_name()
        )),
    }
}

fn path_join(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::String(Rc::from(String::new())));
    }

    let mut path = std::path::PathBuf::new();
    for (i, arg) in args.iter().enumerate() {
        match arg {
            Value::String(s) => path.push(&**s),
            _ => {
                return Err(format!(
                    "Argument {} must be a string, got {}",
                    i,
                    arg.type_name()
                ))
            }
        }
    }

    Ok(Value::String(Rc::from(path.to_string_lossy().to_string())))
}

fn path_dirname(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = Path::new(&path_str);

    match path.parent() {
        Some(parent) => Ok(Value::String(Rc::from(
            parent.to_string_lossy().to_string(),
        ))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn path_basename(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = Path::new(&path_str);

    match path.file_name() {
        Some(name) => Ok(Value::String(Rc::from(name.to_string_lossy().to_string()))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn path_extname(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = Path::new(&path_str);

    match path.extension() {
        Some(ext) => Ok(Value::String(Rc::from(format!(
            ".{}",
            ext.to_string_lossy()
        )))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn path_is_absolute(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = Path::new(&path_str);
    Ok(Value::Boolean(path.is_absolute()))
}

fn path_exists(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = Path::new(&path_str);
    Ok(Value::Boolean(path.exists()))
}

fn path_normalize(args: &[Value]) -> Result<Value, String> {
    let path_str = get_string(args, 0, "path")?;
    let path = std::path::PathBuf::from(&path_str);

    match path.canonicalize() {
        Ok(canonical) => Ok(Value::String(Rc::from(
            canonical.to_string_lossy().to_string(),
        ))),
        Err(_) => {
            let mut components = Vec::new();
            for component in path.components() {
                use std::path::Component;
                match component {
                    Component::ParentDir => {
                        components.pop();
                    }
                    Component::CurDir => {}
                    _ => components.push(component),
                }
            }
            let result: std::path::PathBuf = components.iter().collect();
            Ok(Value::String(Rc::from(
                result.to_string_lossy().to_string(),
            )))
        }
    }
}
