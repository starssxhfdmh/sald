// File built-in class
// Async file system operations using tokio::fs
// All I/O operations return Future values

use super::{check_arity, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, SaldFuture, Value};
use rustc_hash::FxHashMap;
use std::path::Path;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::oneshot;

/// Resolve a path relative to the project root
/// If the path is absolute, it's returned as-is
/// If relative, it's resolved against the project root (or CWD if no project)
fn resolve_path(path: &str) -> String {
    crate::resolve_script_path(path)
        .to_string_lossy()
        .to_string()
}

pub fn create_file_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    // Async Read/Write operations (return Future)
    static_methods.insert("read".to_string(), file_read);
    static_methods.insert("write".to_string(), file_write);
    static_methods.insert("append".to_string(), file_append);
    static_methods.insert("readDir".to_string(), file_read_dir);

    // Sync operations (fast, no I/O blocking)
    static_methods.insert("exists".to_string(), file_exists);
    static_methods.insert("isFile".to_string(), file_is_file);
    static_methods.insert("isDir".to_string(), file_is_dir);
    static_methods.insert("size".to_string(), file_size);

    // Async file operations (return Future)
    static_methods.insert("delete".to_string(), file_delete);
    static_methods.insert("copy".to_string(), file_copy);
    static_methods.insert("rename".to_string(), file_rename);
    static_methods.insert("mkdir".to_string(), file_mkdir);

    // Sync path utilities (no I/O)
    static_methods.insert("join".to_string(), file_join);
    static_methods.insert("dirname".to_string(), file_dirname);
    static_methods.insert("basename".to_string(), file_basename);
    static_methods.insert("ext".to_string(), file_ext);

    let mut class = Class::new("File");
    class.native_static_methods = static_methods;
    class
}

/// File.read(path) - Async read file contents as string
/// Returns: Future<String>
fn file_read(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let _ = tx.send(Ok(Value::String(Arc::from(content))));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.write(path, content) - Async write content to file
/// Returns: Future<Boolean>
fn file_write(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    let content = format!("{}", args[1]);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::write(&path, content).await {
            Ok(_) => {
                let _ = tx.send(Ok(Value::Boolean(true)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.append(path, content) - Async append content to file
/// Returns: Future<Boolean>
fn file_append(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);
    let content = format!("{}", args[1]);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;

        let result = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await;

        match result {
            Ok(mut file) => match file.write_all(content.as_bytes()).await {
                Ok(_) => {
                    let _ = tx.send(Ok(Value::Boolean(true)));
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            },
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.exists(path) - Async check if path exists
/// Returns: Future<Boolean>
fn file_exists(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let exists = tokio::fs::metadata(&path).await.is_ok();
        let _ = tx.send(Ok(Value::Boolean(exists)));
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.isFile(path) - Async check if path is a file
/// Returns: Future<Boolean>
fn file_is_file(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let is_file = match tokio::fs::metadata(&path).await {
            Ok(meta) => meta.is_file(),
            Err(_) => false,
        };
        let _ = tx.send(Ok(Value::Boolean(is_file)));
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.isDir(path) - Async check if path is a directory
/// Returns: Future<Boolean>
fn file_is_dir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let is_dir = match tokio::fs::metadata(&path).await {
            Ok(meta) => meta.is_dir(),
            Err(_) => false,
        };
        let _ = tx.send(Ok(Value::Boolean(is_dir)));
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.size(path) - Async get file size in bytes
/// Returns: Future<Number>
fn file_size(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::metadata(&path).await {
            Ok(meta) => {
                let _ = tx.send(Ok(Value::Number(meta.len() as f64)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.delete(path) - Async delete file or empty directory
/// Returns: Future<Boolean>
fn file_delete(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        // Check if it's a directory first
        let is_dir = match tokio::fs::metadata(&path).await {
            Ok(meta) => meta.is_dir(),
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };

        let result = if is_dir {
            tokio::fs::remove_dir(&path).await
        } else {
            tokio::fs::remove_file(&path).await
        };

        match result {
            Ok(_) => {
                let _ = tx.send(Ok(Value::Boolean(true)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.copy(src, dst) - Async copy file
/// Returns: Future<Number> (bytes copied)
fn file_copy(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let src = resolve_path(&get_string_arg(&args[0], "src")?);
    let dst = resolve_path(&get_string_arg(&args[1], "dst")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::copy(&src, &dst).await {
            Ok(bytes) => {
                let _ = tx.send(Ok(Value::Number(bytes as f64)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.rename(old, new) - Async rename/move file or directory
/// Returns: Future<Boolean>
fn file_rename(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let old = resolve_path(&get_string_arg(&args[0], "old")?);
    let new = resolve_path(&get_string_arg(&args[1], "new")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::rename(&old, &new).await {
            Ok(_) => {
                let _ = tx.send(Ok(Value::Boolean(true)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.mkdir(path) - Async create directory (recursive)
/// Returns: Future<Boolean>
fn file_mkdir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::create_dir_all(&path).await {
            Ok(_) => {
                let _ = tx.send(Ok(Value::Boolean(true)));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.readDir(path) - Async list directory contents
/// Returns: Future<Array<String>>
fn file_read_dir(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = resolve_path(&get_string_arg(&args[0], "path")?);

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        match tokio::fs::read_dir(&path).await {
            Ok(mut entries) => {
                let mut items = Vec::new();
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(name) = entry.file_name().into_string() {
                        items.push(Value::String(Arc::from(name)));
                    }
                }
                let _ = tx.send(Ok(Value::Array(Arc::new(Mutex::new(items)))));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    Ok(Value::Future(Arc::new(Mutex::new(Some(SaldFuture {
        receiver: rx,
    })))))
}

/// File.join(parts...) - Sync join path components
fn file_join(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }

    let mut path = std::path::PathBuf::new();
    for arg in args {
        let part = format!("{}", arg);
        path.push(&part);
    }

    Ok(Value::String(Arc::from(path.to_string_lossy().to_string())))
}

/// File.dirname(path) - Sync get directory name
fn file_dirname(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).parent() {
        Some(parent) => Ok(Value::String(Arc::from(
            parent.to_string_lossy().to_string(),
        ))),
        None => Ok(Value::String(Arc::from(String::new()))),
    }
}

/// File.basename(path) - Sync get file name
fn file_basename(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).file_name() {
        Some(name) => Ok(Value::String(Arc::from(name.to_string_lossy().to_string()))),
        None => Ok(Value::String(Arc::from(String::new()))),
    }
}

/// File.ext(path) - Sync get file extension
fn file_ext(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let path = get_string_arg(&args[0], "path")?;

    match Path::new(&path).extension() {
        Some(ext) => Ok(Value::String(Arc::from(ext.to_string_lossy().to_string()))),
        None => Ok(Value::String(Arc::from(String::new()))),
    }
}
