use super::{check_arity, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

fn resolve_path(path: &str) -> String {
    crate::resolve_script_path(path)
        .to_string_lossy()
        .to_string()
}

pub fn create_file_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("read".to_string(), file_read);
    static_methods.insert("write".to_string(), file_write);
    static_methods.insert("append".to_string(), file_append);
    static_methods.insert("readDir".to_string(), file_read_dir);

    static_methods.insert("exists".to_string(), file_exists);
    static_methods.insert("isFile".to_string(), file_is_file);
    static_methods.insert("isDir".to_string(), file_is_dir);
    static_methods.insert("size".to_string(), file_size);

    static_methods.insert("delete".to_string(), file_delete);
    static_methods.insert("copy".to_string(), file_copy);
    static_methods.insert("rename".to_string(), file_rename);
    static_methods.insert("mkdir".to_string(), file_mkdir);

    static_methods.insert("join".to_string(), file_join);
    static_methods.insert("dirname".to_string(), file_dirname);
    static_methods.insert("basename".to_string(), file_basename);
    static_methods.insert("ext".to_string(), file_ext);

    let mut class = Class::new("File");
    class.native_static_methods = static_methods;
    class
}

fn file_read(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Value::String(Rc::from(content))),
        Err(e) => Err(format!("Failed to read file '{}': {}", path, e)),
    }
}

fn file_write(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    let content = format!("{}", args[1]);

    match std::fs::write(&path, content) {
        Ok(_) => Ok(Value::Boolean(true)),
        Err(e) => Err(format!("Failed to write file '{}': {}", path, e)),
    }
}

fn file_append(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    let content = format!("{}", args[1]);

    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open file '{}': {}", path, e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to append to file '{}': {}", path, e))?;

    Ok(Value::Boolean(true))
}

fn file_exists(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    Ok(Value::Boolean(Path::new(&path).exists()))
}

fn file_is_file(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    Ok(Value::Boolean(Path::new(&path).is_file()))
}

fn file_is_dir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    Ok(Value::Boolean(Path::new(&path).is_dir()))
}

fn file_size(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    match std::fs::metadata(&path) {
        Ok(meta) => Ok(Value::Number(meta.len() as f64)),
        Err(e) => Err(format!("Failed to get size of '{}': {}", path, e)),
    }
}

fn file_delete(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let path_obj = Path::new(&path);
    if path_obj.is_dir() {
        std::fs::remove_dir(&path)
            .map_err(|e| format!("Failed to delete directory '{}': {}", path, e))?;
    } else {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete file '{}': {}", path, e))?;
    }

    Ok(Value::Boolean(true))
}

fn file_copy(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let src = resolve_path(&get_string_arg(&args[0], "src")?);
    let dst = resolve_path(&get_string_arg(&args[1], "dst")?);

    match std::fs::copy(&src, &dst) {
        Ok(bytes) => Ok(Value::Number(bytes as f64)),
        Err(e) => Err(format!("Failed to copy '{}' to '{}': {}", src, dst, e)),
    }
}

fn file_rename(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let old = resolve_path(&get_string_arg(&args[0], "old")?);
    let new = resolve_path(&get_string_arg(&args[1], "new")?);

    match std::fs::rename(&old, &new) {
        Ok(_) => Ok(Value::Boolean(true)),
        Err(e) => Err(format!("Failed to rename '{}' to '{}': {}", old, new, e)),
    }
}

fn file_mkdir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    match std::fs::create_dir_all(&path) {
        Ok(_) => Ok(Value::Boolean(true)),
        Err(e) => Err(format!("Failed to create directory '{}': {}", path, e)),
    }
}

fn file_read_dir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let entries = std::fs::read_dir(&path)
        .map_err(|e| format!("Failed to read directory '{}': {}", path, e))?;

    let mut items = Vec::new();
    for entry in entries.flatten() {
        if let Ok(name) = entry.file_name().into_string() {
            items.push(Value::String(Rc::from(name)));
        }
    }

    Ok(Value::Array(Rc::new(RefCell::new(items))))
}

fn file_join(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }

    let mut path = std::path::PathBuf::new();
    for arg in args {
        let part = format!("{}", arg);
        path.push(&part);
    }

    Ok(Value::String(Rc::from(path.to_string_lossy().to_string())))
}

fn file_dirname(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).parent() {
        Some(parent) => Ok(Value::String(Rc::from(
            parent.to_string_lossy().to_string(),
        ))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn file_basename(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).file_name() {
        Some(name) => Ok(Value::String(Rc::from(name.to_string_lossy().to_string()))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}

fn file_ext(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).extension() {
        Some(ext) => Ok(Value::String(Rc::from(ext.to_string_lossy().to_string()))),
        None => Ok(Value::String(Rc::from(String::new()))),
    }
}
