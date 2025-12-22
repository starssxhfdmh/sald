// Http built-in namespace
// Provides HTTP client functions and async Server class
//
// Usage:
//   Http.get("url")        - Async GET request
//   Http.post("url", body) - Async POST request
//   Http.put("url", body)  - Async PUT request
//   Http.delete("url")     - Async DELETE request
//   let server = Http.Server()
//   server.get("/", handler)
//   server.listen(8080)

use crate::vm::caller::CallableNativeInstanceFn;
use crate::vm::value::{Class, Instance, NativeInstanceFn, SaldFuture, Value};
use bytes::Bytes;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use tokio::sync::oneshot;

/// Create the Http namespace with client functions and Server class
pub fn create_http_namespace() -> Value {
    let mut members: FxHashMap<String, Value> = FxHashMap::default();

    // Client functions (wrapped as NativeFunction)
    members.insert(
        "get".to_string(),
        Value::NativeFunction {
            func: http_get,
            class_name: "Http".to_string(),
        },
    );
    members.insert(
        "post".to_string(),
        Value::NativeFunction {
            func: http_post,
            class_name: "Http".to_string(),
        },
    );
    members.insert(
        "put".to_string(),
        Value::NativeFunction {
            func: http_put,
            class_name: "Http".to_string(),
        },
    );
    members.insert(
        "delete".to_string(),
        Value::NativeFunction {
            func: http_delete,
            class_name: "Http".to_string(),
        },
    );

    // Server class
    members.insert(
        "Server".to_string(),
        Value::Class(Arc::new(create_server_class())),
    );

    Value::Namespace {
        name: "Http".to_string(),
        members: Arc::new(RwLock::new(members)),
        module_globals: None,
    }
}

// ==================== HTTP Client Functions ====================

/// Async HTTP GET - returns Future that resolves to response body
fn http_get(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let url = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'url' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client.get(&url).send().await {
            Ok(response) => match response.text().await {
                Ok(text) => {
                    let _ = tx.send(Ok(Value::String(Arc::new(text))));
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

/// Async HTTP POST - returns Future that resolves to response body
fn http_post(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }

    let url = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'url' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let body = if args.len() > 1 {
        match &args[1] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        String::new()
    };

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client.post(&url).body(body).send().await {
            Ok(response) => match response.text().await {
                Ok(text) => {
                    let _ = tx.send(Ok(Value::String(Arc::new(text))));
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

/// Async HTTP PUT
fn http_put(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }

    let url = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'url' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let body = if args.len() > 1 {
        match &args[1] {
            Value::String(s) => s.to_string(),
            other => format!("{}", other),
        }
    } else {
        String::new()
    };

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client.put(&url).body(body).send().await {
            Ok(response) => match response.text().await {
                Ok(text) => {
                    let _ = tx.send(Ok(Value::String(Arc::new(text))));
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

/// Async HTTP DELETE
fn http_delete(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument but got 0".to_string());
    }

    let url = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Argument 'url' must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client.delete(&url).send().await {
            Ok(response) => match response.text().await {
                Ok(text) => {
                    let _ = tx.send(Ok(Value::String(Arc::new(text))));
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

// ==================== HTTP Server Class ====================

fn create_server_class() -> Class {
    let mut instance_methods: FxHashMap<String, NativeInstanceFn> = FxHashMap::default();
    let mut callable_methods: FxHashMap<String, CallableNativeInstanceFn> = FxHashMap::default();

    // Route registration methods (need ValueCaller to store and call handlers)
    callable_methods.insert("get".to_string(), server_route_get);
    callable_methods.insert("post".to_string(), server_route_post);
    callable_methods.insert("put".to_string(), server_route_put);
    callable_methods.insert("delete".to_string(), server_route_delete);
    callable_methods.insert("patch".to_string(), server_route_patch);
    callable_methods.insert("options".to_string(), server_route_options);
    callable_methods.insert("head".to_string(), server_route_head);
    callable_methods.insert("all".to_string(), server_route_all);
    callable_methods.insert("listen".to_string(), server_listen);

    // Debug method
    instance_methods.insert("routes".to_string(), server_get_routes);

    let mut class = Class::new_with_instance("Server", instance_methods, Some(server_constructor));
    class.callable_native_instance_methods = callable_methods;
    class
}

/// Http.Server() constructor - creates a new server instance
fn server_constructor(_args: &[Value]) -> Result<Value, String> {
    let server_class = Arc::new(create_server_class());
    let mut instance = Instance::new(server_class);

    // Initialize routes storage: { "GET:/path": handler, ... }
    instance.fields.insert(
        "_routes".to_string(),
        Value::Dictionary(Arc::new(Mutex::new(FxHashMap::default()))),
    );

    Ok(Value::Instance(Arc::new(Mutex::new(instance))))
}

/// Helper to register a route with a specific HTTP method
fn register_route(recv: &Value, args: &[Value], http_method: &str) -> Result<Value, String> {
    if args.len() < 2 {
        return Err(format!(
            "Expected 2 arguments (path, handler) but got {}",
            args.len()
        ));
    }

    let path = match &args[0] {
        Value::String(s) => s.to_string(),
        _ => {
            return Err(format!(
                "Path must be a string, got {}",
                args[0].type_name()
            ))
        }
    };

    let handler = &args[1];
    if !matches!(handler, Value::Function(_)) {
        return Err(format!(
            "Handler must be a function, got {}",
            handler.type_name()
        ));
    }

    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock();
        if let Some(Value::Dictionary(routes)) = inst_guard.fields.get("_routes") {
            let mut routes_guard = routes.lock();
            let route_key = format!("{}:{}", http_method, path);
            routes_guard.insert(route_key, handler.clone());
        }
    }

    // Return the instance for chaining
    Ok(recv.clone())
}

fn server_route_get(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "GET")
}

fn server_route_post(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "POST")
}

fn server_route_put(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "PUT")
}

fn server_route_delete(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "DELETE")
}

fn server_route_patch(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "PATCH")
}

fn server_route_options(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "OPTIONS")
}

fn server_route_head(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "HEAD")
}

fn server_route_all(
    recv: &Value,
    args: &[Value],
    _caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    register_route(recv, args, "ALL")
}

/// server.routes() - Get registered routes (for display)
fn server_get_routes(recv: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock();
        if let Some(Value::Dictionary(routes)) = inst_guard.fields.get("_routes") {
            let routes_guard = routes.lock();
            // Build clean array of route strings: ["GET /", "POST /api/users", ...]
            let mut route_list: Vec<Value> = routes_guard
                .keys()
                .map(|key| {
                    // key format is "METHOD:path", convert to "METHOD path"
                    let parts: Vec<&str> = key.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Value::String(Arc::new(format!("{} {}", parts[0], parts[1])))
                    } else {
                        Value::String(Arc::new(key.clone()))
                    }
                })
                .collect();
            route_list.sort_by(|a, b| {
                if let (Value::String(a), Value::String(b)) = (a, b) {
                    a.cmp(b)
                } else {
                    std::cmp::Ordering::Equal
                }
            });
            return Ok(Value::Array(Arc::new(Mutex::new(route_list))));
        }
    }
    Ok(Value::Array(Arc::new(Mutex::new(Vec::new()))))
}

/// server.listen(port) - Start the HTTP server
/// Uses async Tokio TcpListener and spawns a separate task for each request
/// Each task creates its own VM instance with shared globals
fn server_listen(
    recv: &Value,
    args: &[Value],
    caller: &mut dyn crate::vm::caller::ValueCaller,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected 1 argument (port) but got 0".to_string());
    }

    let port = match &args[0] {
        Value::Number(n) => *n as u16,
        _ => {
            return Err(format!(
                "Port must be a number, got {}",
                args[0].type_name()
            ))
        }
    };

    // Extract routes from the instance
    let routes: Arc<FxHashMap<String, Value>> = if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock();
        if let Some(Value::Dictionary(routes_dict)) = inst_guard.fields.get("_routes") {
            Arc::new(routes_dict.lock().clone())
        } else {
            Arc::new(FxHashMap::default())
        }
    } else {
        return Err("Invalid Server instance".to_string());
    };

    if routes.is_empty() {
        return Err("No routes registered. Add routes with server.get(), server.post(), etc.".to_string());
    }

    // Get SHARED globals Arc from the current VM - handlers will share this directly
    let globals = caller.get_shared_globals();
    
    // Capture current script directory for path resolution in handlers
    let script_dir = crate::get_script_dir();
    let script_dir_str = script_dir.to_string_lossy().to_string();

    // Run the async server using block_in_place to not block the Tokio runtime
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            run_async_server(port, routes, globals, script_dir_str).await
        })
    })
}

/// Async HTTP server implementation
async fn run_async_server(
    port: u16,
    routes: Arc<FxHashMap<String, Value>>,
    globals: Arc<RwLock<FxHashMap<String, Value>>>,
    script_dir: String,
) -> Result<Value, String> {
    use tokio::net::TcpListener;

    // Bind to 0.0.0.0 for IPv4 support (Windows doesn't support dual-stack on [::])
    let addr = format!("0.0.0.0:{}", port);
    
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Failed to bind to port {}: {}", port, e))?;

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
            let routes_clone = routes.clone();
                let globals_clone = globals.clone();
                let script_dir_clone = script_dir.clone();
                
                // Spawn a new task for each connection - TRUE CONCURRENCY!
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, routes_clone, globals_clone, script_dir_clone).await {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }
}

/// Handle HTTP connection with keepalive support - handles multiple requests per connection
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    routes: Arc<FxHashMap<String, Value>>,
    globals: Arc<RwLock<FxHashMap<String, Value>>>,
    script_dir: String,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use crate::vm::VM;

    // Disable Nagle's algorithm for lower latency
    let _ = stream.set_nodelay(true);

    // Reuse VM for this connection - avoids allocation per request
    let mut vm = VM::new_with_shared_globals(globals);

    let mut buffer = [0u8; 8192];

    // Keepalive loop - handle multiple requests on same connection
    loop {
        // Read request
        let bytes_read = match stream.read(&mut buffer).await {
            Ok(0) => return Ok(()), // Connection closed
            Ok(n) => n,
            Err(_) => return Ok(()), // Read error, close connection
        };

        let request_str = String::from_utf8_lossy(&buffer[..bytes_read]);
        let lines: Vec<&str> = request_str.lines().collect();

        if lines.is_empty() {
            continue;
        }

        // Parse request line: "GET /path HTTP/1.1"
        let request_line: Vec<&str> = lines[0].split_whitespace().collect();
        if request_line.len() < 2 {
            continue;
        }

        let method = request_line[0].to_string();
        let full_path = request_line[1].to_string();

        // Split path and query string
        let (path, query_string) = if let Some(idx) = full_path.find('?') {
            (full_path[..idx].to_string(), full_path[idx + 1..].to_string())
        } else {
            (full_path.clone(), String::new())
        };

        // Parse headers (pre-allocate for typical ~8 headers)
        let mut headers: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(8, Default::default());
        let mut body_start = 0;
        let mut connection_close = false;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.is_empty() {
                body_start = i + 1;
                break;
            }
            if let Some(colon_idx) = line.find(':') {
                let key = line[..colon_idx].trim().to_lowercase();
                let value = line[colon_idx + 1..].trim();
                // Check if client wants to close connection
                if key == "connection" && value.eq_ignore_ascii_case("close") {
                    connection_close = true;
                }
                headers.insert(key, Value::String(Arc::new(value.to_string())));
            }
        }

        // Get body
        let body = if body_start > 0 && body_start < lines.len() {
            lines[body_start..].join("\n")
        } else {
            String::new()
        };

        // Parse query parameters (pre-allocate for typical ~4 params)
        let mut query_params: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(4, Default::default());
        if !query_string.is_empty() {
            for pair in query_string.split('&') {
                if let Some(eq_idx) = pair.find('=') {
                    let key = &pair[..eq_idx];
                    let value = &pair[eq_idx + 1..];
                    query_params.insert(
                        key.to_string(),
                        Value::String(Arc::new(value.to_string())),
                    );
                }
            }
        }

        // Find matching route and extract params
        let (handler, route_params) = find_matching_route(&routes, &method, &path);

        if let Some(handler_fn) = handler {
            // Build request dictionary (exactly 6 fields)
            let mut req_fields: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(6, Default::default());
            req_fields.insert("method".to_string(), Value::String(Arc::new(method)));
            req_fields.insert("path".to_string(), Value::String(Arc::new(path)));
            req_fields.insert("body".to_string(), Value::String(Arc::new(body)));
            req_fields.insert("params".to_string(), Value::Dictionary(Arc::new(Mutex::new(route_params))));
            req_fields.insert("query".to_string(), Value::Dictionary(Arc::new(Mutex::new(query_params))));
            req_fields.insert("headers".to_string(), Value::Dictionary(Arc::new(Mutex::new(headers))));

            let request = Value::Dictionary(Arc::new(Mutex::new(req_fields)));

            // Reset VM state for clean execution (reuse allocation)
            vm.reset();
            
            // Call the handler using the reused VM
            match vm.run_handler(handler_fn, request, Some(&script_dir)).await {
                Ok(result) => {
                    // ZERO-COPY SPLIT WRITE: Get headers and body as Bytes, write directly
                    let (headers_bytes, body_bytes) = build_http_response_parts(&result);
                    
                    // Write headers first (Bytes derefs to &[u8])
                    if stream.write_all(&headers_bytes).await.is_err() {
                        return Ok(());
                    }
                    // Write body directly - ZERO COPY!
                    if stream.write_all(&body_bytes).await.is_err() {
                        return Ok(());
                    }
                    if stream.flush().await.is_err() {
                        return Ok(());
                    }
                }
                Err(e) => {
                    eprintln!("\n{}", e);
                    let error_response = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\n\r\nHandler error: {}",
                        e
                    );
                    if stream.write_all(error_response.as_bytes()).await.is_err() {
                        return Ok(());
                    }
                    if stream.flush().await.is_err() {
                        return Ok(());
                    }
                }
            }
        } else {
            let not_found = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nNot Found: {} {}",
                method, full_path
            );
            if stream.write_all(not_found.as_bytes()).await.is_err() {
                return Ok(());
            }
            if stream.flush().await.is_err() {
                return Ok(());
            }
        }

        // If client requested connection close, break the loop
        if connection_close {
            return Ok(());
        }
    }
}

/// Find a matching route handler and extract path parameters
fn find_matching_route(
    routes: &FxHashMap<String, Value>,
    method: &str,
    path: &str,
) -> (Option<Value>, FxHashMap<String, Value>) {
    // Pre-allocate for typical ~2-3 route params
    let mut params: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(4, Default::default());

    // Build route key on stack to avoid allocation 
    // (method max ~7 chars + ":" + path typically < 256 chars)
    let mut key_buf = String::with_capacity(method.len() + 1 + path.len());
    key_buf.push_str(method);
    key_buf.push(':');
    key_buf.push_str(path);
    
    // Try exact match first
    if let Some(handler) = routes.get(&key_buf) {
        return (Some(handler.clone()), params);
    }

    // Try ALL method - reuse buffer
    key_buf.clear();
    key_buf.push_str("ALL:");
    key_buf.push_str(path);
    if let Some(handler) = routes.get(&key_buf) {
        return (Some(handler.clone()), params);
    }

    // Try pattern matching with :param
    for (key, handler) in routes {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }

        let route_method = parts[0];
        let route_pattern = parts[1];

        // Skip if method doesn't match (unless ALL)
        if route_method != method && route_method != "ALL" {
            continue;
        }

        // Check if pattern matches
        if let Some(extracted_params) = match_route_pattern(route_pattern, path) {
            params = extracted_params;
            return (Some(handler.clone()), params);
        }
    }

    (None, params)
}

/// Match a route pattern against a path, extracting parameters
/// Supports wildcard patterns like /:filepath* which captures remaining path segments
fn match_route_pattern(pattern: &str, path: &str) -> Option<FxHashMap<String, Value>> {
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Check if last pattern part is a wildcard (ends with *)
    let has_wildcard = pattern_parts.last()
        .map(|p| p.starts_with(':') && p.ends_with('*'))
        .unwrap_or(false);

    if has_wildcard {
        // Wildcard pattern: must have at least as many path parts as pattern parts (minus 1)
        if path_parts.len() < pattern_parts.len() - 1 {
            return None;
        }
    } else {
        // Exact match: must have same number of parts
        if pattern_parts.len() != path_parts.len() {
            return None;
        }
    }

    // Pre-allocate for typical ~2 route params
    let mut params: FxHashMap<String, Value> = FxHashMap::with_capacity_and_hasher(4, Default::default());

    for (i, pattern_part) in pattern_parts.iter().enumerate() {
        if pattern_part.starts_with(':') {
            // Check if this is a wildcard parameter
            if pattern_part.ends_with('*') {
                // Extract parameter name (remove : and *)
                let param_name = &pattern_part[1..pattern_part.len() - 1];
                
                // Capture all remaining path parts
                let remaining_parts: Vec<&str> = path_parts[i..].to_vec();
                let captured_path = remaining_parts.join("/");
                
                params.insert(
                    param_name.to_string(),
                    Value::String(Arc::new(captured_path)),
                );
                
                // Wildcard consumes all remaining parts, so we're done
                break;
            } else {
                // Regular parameter
                if i >= path_parts.len() {
                    return None;
                }
                let param_name = &pattern_part[1..];
                params.insert(
                    param_name.to_string(),
                    Value::String(Arc::new(path_parts[i].to_string())),
                );
            }
        } else if i >= path_parts.len() || *pattern_part != path_parts[i] {
            // Literal segment must match exactly
            return None;
        }
    }

    Some(params)
}

/// Build HTTP response headers and extract body
/// Returns (headers_bytes, body_bytes) for zero-copy socket writes
fn build_http_response_parts(result: &Value) -> (Bytes, Bytes) {
    use std::fmt::Write;
    
    // Extract status, body, and headers - avoid cloning body when possible
    let (status, body, headers): (u16, String, FxHashMap<String, String>) = match result {
        Value::Dictionary(dict) => {
            let dict_guard = dict.lock();

            let status = match dict_guard.get("status") {
                Some(Value::Number(n)) => *n as u16,
                _ => 200,
            };

            // Get body - only clone if needed for non-string types
            let body = match dict_guard.get("body") {
                Some(Value::String(s)) => (**s).clone(), // Clone the inner String from Arc
                Some(other) => format!("{}", other),
                None => String::new(),
            };

            let headers: FxHashMap<String, String> =
                if let Some(Value::Dictionary(hdrs)) = dict_guard.get("headers") {
                    let hdrs_guard = hdrs.lock();
                    let mut h = FxHashMap::with_capacity_and_hasher(hdrs_guard.len(), Default::default());
                    for (k, v) in hdrs_guard.iter() {
                        h.insert(k.clone(), match v {
                            Value::String(s) => (**s).clone(),
                            other => format!("{}", other),
                        });
                    }
                    h
                } else {
                    FxHashMap::default()
                };

            (status, body, headers)
        }
        Value::String(s) => (200, (**s).clone(), FxHashMap::default()),
        other => (200, format!("{}", other), FxHashMap::default()),
    };

    let status_text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };

    // Build headers only (body will be written separately)
    let mut header_buf = String::with_capacity(256);
    
    let _ = write!(header_buf, "HTTP/1.1 {} {}\r\n", status, status_text);
    let _ = write!(header_buf, "Content-Length: {}\r\n", body.len());

    for (key, value) in &headers {
        let _ = write!(header_buf, "{}: {}\r\n", key, value);
    }

    if !headers.contains_key("Content-Type") && !headers.contains_key("content-type") {
        header_buf.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    }

    header_buf.push_str("Connection: keep-alive\r\n");
    header_buf.push_str("\r\n");

    // Convert to Bytes - takes ownership of the underlying buffer (zero-copy!)
    (Bytes::from(header_buf), Bytes::from(body))
}


