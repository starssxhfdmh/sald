// System built-in class
// Provides comprehensive system information using sysinfo crate
// Uses Arc for thread-safety

use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use sysinfo::System;

pub fn create_system_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    // Basic info (no sysinfo needed)
    static_methods.insert("os".to_string(), system_os);
    static_methods.insert("arch".to_string(), system_arch);
    static_methods.insert("family".to_string(), system_family);
    static_methods.insert("cpus".to_string(), system_cpus);

    // Extended info (via sysinfo)
    static_methods.insert("hostname".to_string(), system_hostname);
    static_methods.insert("osVersion".to_string(), system_os_version);
    static_methods.insert("kernelVersion".to_string(), system_kernel_version);

    // Memory info
    static_methods.insert("totalMemory".to_string(), system_total_memory);
    static_methods.insert("usedMemory".to_string(), system_used_memory);
    static_methods.insert("freeMemory".to_string(), system_free_memory);
    static_methods.insert("totalSwap".to_string(), system_total_swap);
    static_methods.insert("usedSwap".to_string(), system_used_swap);

    // CPU info
    static_methods.insert("cpuName".to_string(), system_cpu_name);
    static_methods.insert("cpuUsage".to_string(), system_cpu_usage);

    // Uptime
    static_methods.insert("uptime".to_string(), system_uptime);
    static_methods.insert("bootTime".to_string(), system_boot_time);

    // All info as dict
    static_methods.insert("info".to_string(), system_info);

    // Environment variables
    static_methods.insert("getenv".to_string(), system_getenv);
    static_methods.insert("setenv".to_string(), system_setenv);
    static_methods.insert("envs".to_string(), system_envs);

    Class::new_with_static("System", static_methods)
}

// ==================== Basic Info ====================

/// Get operating system name (linux, windows, macos, etc.)
fn system_os(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(std::env::consts::OS.to_string())))
}

/// Get architecture (x86_64, aarch64, etc.)
fn system_arch(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(std::env::consts::ARCH.to_string())))
}

/// Get OS family (unix, windows, wasm, etc.)
fn system_family(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(std::env::consts::FAMILY.to_string())))
}

/// Get number of logical CPUs
fn system_cpus(_args: &[Value]) -> Result<Value, String> {
    let count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    Ok(Value::Number(count as f64))
}

// ==================== Extended Info (sysinfo) ====================

/// Get hostname
fn system_hostname(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(
        System::host_name().unwrap_or_else(|| "unknown".to_string())
    )))
}

/// Get OS version (e.g., "Windows 11 (22H2)")
fn system_os_version(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(
        System::long_os_version().unwrap_or_else(|| "unknown".to_string())
    )))
}

/// Get kernel version
fn system_kernel_version(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(Arc::from(
        System::kernel_version().unwrap_or_else(|| "unknown".to_string())
    )))
}

// ==================== Memory Info ====================

/// Get total memory in bytes
fn system_total_memory(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    Ok(Value::Number(sys.total_memory() as f64))
}

/// Get used memory in bytes
fn system_used_memory(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    Ok(Value::Number(sys.used_memory() as f64))
}

/// Get free memory in bytes
fn system_free_memory(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    Ok(Value::Number(sys.free_memory() as f64))
}

/// Get total swap in bytes
fn system_total_swap(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    Ok(Value::Number(sys.total_swap() as f64))
}

/// Get used swap in bytes
fn system_used_swap(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    Ok(Value::Number(sys.used_swap() as f64))
}

// ==================== CPU Info ====================

/// Get CPU name/model
fn system_cpu_name(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    let name = sys.cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(Value::String(Arc::from(name)))
}

/// Get overall CPU usage percentage (0-100)
fn system_cpu_usage(_args: &[Value]) -> Result<Value, String> {
    let mut sys = System::new_all();
    // Need to refresh twice to get meaningful data
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    
    let total: f32 = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).sum();
    let count = sys.cpus().len() as f32;
    let avg = if count > 0.0 { total / count } else { 0.0 };
    
    Ok(Value::Number(avg as f64))
}

// ==================== Uptime ====================

/// Get system uptime in seconds
fn system_uptime(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(System::uptime() as f64))
}

/// Get boot time as Unix timestamp
fn system_boot_time(_args: &[Value]) -> Result<Value, String> {
    Ok(Value::Number(System::boot_time() as f64))
}

// ==================== All Info ====================

/// Get all system info as dictionary
fn system_info(_args: &[Value]) -> Result<Value, String> {
    let sys = System::new_all();
    
    let mut info: FxHashMap<String, Value> = FxHashMap::default();
    
    // Basic
    info.insert("os".to_string(), Value::String(Arc::from(std::env::consts::OS.to_string())));
    info.insert("arch".to_string(), Value::String(Arc::from(std::env::consts::ARCH.to_string())));
    info.insert("family".to_string(), Value::String(Arc::from(std::env::consts::FAMILY.to_string())));
    info.insert("cpus".to_string(), Value::Number(
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64
    ));
    
    // Extended
    info.insert("hostname".to_string(), Value::String(Arc::from(
        System::host_name().unwrap_or_else(|| "unknown".to_string())
    )));
    info.insert("osVersion".to_string(), Value::String(Arc::from(
        System::long_os_version().unwrap_or_else(|| "unknown".to_string())
    )));
    info.insert("kernelVersion".to_string(), Value::String(Arc::from(
        System::kernel_version().unwrap_or_else(|| "unknown".to_string())
    )));
    
    // Memory (in MB for readability)
    info.insert("totalMemoryMB".to_string(), Value::Number((sys.total_memory() / 1024 / 1024) as f64));
    info.insert("usedMemoryMB".to_string(), Value::Number((sys.used_memory() / 1024 / 1024) as f64));
    info.insert("freeMemoryMB".to_string(), Value::Number((sys.free_memory() / 1024 / 1024) as f64));
    
    // CPU
    let cpu_name = sys.cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    info.insert("cpuName".to_string(), Value::String(Arc::from(cpu_name)));
    
    // Uptime
    info.insert("uptime".to_string(), Value::Number(System::uptime() as f64));
    
    Ok(Value::Dictionary(Arc::new(Mutex::new(info))))
}

// ==================== Environment Variables ====================

/// Get environment variable
fn system_getenv(args: &[Value]) -> Result<Value, String> {
    super::check_arity(1, args.len())?;
    let name = super::get_string_arg(&args[0], "name")?;
    
    match std::env::var(&name) {
        Ok(val) => Ok(Value::String(Arc::from(val))),
        Err(_) => Ok(Value::Null),
    }
}

/// Set environment variable
fn system_setenv(args: &[Value]) -> Result<Value, String> {
    super::check_arity(2, args.len())?;
    let name = super::get_string_arg(&args[0], "name")?;
    
    let value = match &args[1] {
        Value::String(s) => s.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Null => "".to_string(),
        other => return Err(format!("Argument 'value' must be a string, got {}", other.type_name())),
    };
    
    // SAFETY: Setting env vars is inherently racy but acceptable for this use case
    unsafe { std::env::set_var(&name, &value); }
    Ok(Value::Boolean(true))
}

/// Get all environment variables as dictionary
fn system_envs(_args: &[Value]) -> Result<Value, String> {
    let mut envs: FxHashMap<String, Value> = FxHashMap::default();
    
    for (key, value) in std::env::vars() {
        envs.insert(key, Value::String(Arc::from(value)));
    }
    
    Ok(Value::Dictionary(Arc::new(Mutex::new(envs))))
}

