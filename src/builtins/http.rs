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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use parking_lot::RwLock;
use std::time::Instant;
use tokio::sync::oneshot;

/// Create the Http namespace with client functions and Server class
pub fn create_http_namespace() -> Value {
    let mut members: HashMap<String, Value> = HashMap::new();

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
        members: Arc::new(Mutex::new(members)),
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
    let mut instance_methods: HashMap<String, NativeInstanceFn> = HashMap::new();
    let mut callable_methods: HashMap<String, CallableNativeInstanceFn> = HashMap::new();

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
        Value::Dictionary(Arc::new(Mutex::new(HashMap::new()))),
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
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Dictionary(routes)) = inst_guard.fields.get("_routes") {
            let mut routes_guard = routes.lock().unwrap();
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
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Dictionary(routes)) = inst_guard.fields.get("_routes") {
            let routes_guard = routes.lock().unwrap();
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
    let routes: Arc<HashMap<String, Value>> = if let Value::Instance(inst) = recv {
        let inst_guard = inst.lock().unwrap();
        if let Some(Value::Dictionary(routes_dict)) = inst_guard.fields.get("_routes") {
            Arc::new(routes_dict.lock().unwrap().clone())
        } else {
            Arc::new(HashMap::new())
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
    routes: Arc<HashMap<String, Value>>,
    globals: Arc<RwLock<HashMap<String, Value>>>,
    script_dir: String,
) -> Result<Value, String> {
    use tokio::net::TcpListener;

    // Bind to [::] for dual-stack (IPv4 + IPv6) support
    // This fixes "localhost" being slow due to IPv6 timeout
    let addr = format!("[::]:{}", port);
    
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

/// Handle a single HTTP connection in its own task
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    routes: Arc<HashMap<String, Value>>,
    globals: Arc<RwLock<HashMap<String, Value>>>,
    script_dir: String,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use crate::vm::VM;

    // Disable Nagle's algorithm for lower latency
    let _ = stream.set_nodelay(true);

    // Read request
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await
        .map_err(|e| format!("Read error: {}", e))?;

    if bytes_read == 0 {
        return Ok(());
    }

    let request_str = String::from_utf8_lossy(&buffer[..bytes_read]);
    let lines: Vec<&str> = request_str.lines().collect();

    if lines.is_empty() {
        return Ok(());
    }

    // Parse request line: "GET /path HTTP/1.1"
    let request_line: Vec<&str> = lines[0].split_whitespace().collect();
    if request_line.len() < 2 {
        return Ok(());
    }

    let method = request_line[0].to_string();
    let full_path = request_line[1].to_string();

    // Split path and query string
    let (path, query_string) = if let Some(idx) = full_path.find('?') {
        (full_path[..idx].to_string(), full_path[idx + 1..].to_string())
    } else {
        (full_path.clone(), String::new())
    };

    // Parse headers
    let mut headers: HashMap<String, Value> = HashMap::new();
    let mut body_start = 0;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.is_empty() {
            body_start = i + 1;
            break;
        }
        if let Some(colon_idx) = line.find(':') {
            let key = line[..colon_idx].trim().to_lowercase();
            let value = line[colon_idx + 1..].trim();
            headers.insert(key, Value::String(Arc::new(value.to_string())));
        }
    }

    // Get body
    let body = if body_start > 0 && body_start < lines.len() {
        lines[body_start..].join("\n")
    } else {
        String::new()
    };

    // Parse query parameters
    let mut query_params: HashMap<String, Value> = HashMap::new();
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

    // Start timing
    let start_time = Instant::now();

    // Find matching route and extract params
    let (handler, route_params) = find_matching_route(&routes, &method, &path);

    let response = if let Some(handler_fn) = handler {
        // Build request dictionary
        let mut req_fields: HashMap<String, Value> = HashMap::new();
        req_fields.insert(
            "method".to_string(),
            Value::String(Arc::new(method.clone())),
        );
        req_fields.insert(
            "path".to_string(),
            Value::String(Arc::new(path.clone())),
        );
        req_fields.insert("body".to_string(), Value::String(Arc::new(body)));
        req_fields.insert(
            "params".to_string(),
            Value::Dictionary(Arc::new(Mutex::new(route_params))),
        );
        req_fields.insert(
            "query".to_string(),
            Value::Dictionary(Arc::new(Mutex::new(query_params))),
        );
        req_fields.insert(
            "headers".to_string(),
            Value::Dictionary(Arc::new(Mutex::new(headers))),
        );

        let request = Value::Dictionary(Arc::new(Mutex::new(req_fields)));

        // Create a NEW VM for this request with SHARED globals Arc
        // The Arc<RwLock<HashMap>> is shared directly - modifications are visible everywhere!
        let mut vm = VM::new_with_shared_globals(globals.clone());
        
        // Call the handler using the VM's run_handler method with script_dir for path resolution
        match vm.run_handler(handler_fn, request, Some(&script_dir)).await {
            Ok(result) => build_http_response(&result),
            Err(e) => {
                // Log error to terminal
                eprintln!("\n{}", e);
                format!(
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\n\r\nHandler error: {}",
                    e
                )
            }
        }
    } else {
        format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nNot Found: {} {}",
            method, path
        )
    };

    // Calculate response time (for middleware use)
    let _duration = start_time.elapsed();
    
    // Extract status code from response
    let _status_code = if response.starts_with("HTTP/1.1 ") {
        response[9..12].to_string()
    } else {
        "???".to_string()
    };

    // Note: Logging removed - use Spark.Middleware.logger() for colored logs

    stream.write_all(response.as_bytes()).await
        .map_err(|e| format!("Write error: {}", e))?;
    stream.flush().await
        .map_err(|e| format!("Flush error: {}", e))?;

    Ok(())
}

/// Find a matching route handler and extract path parameters
fn find_matching_route(
    routes: &HashMap<String, Value>,
    method: &str,
    path: &str,
) -> (Option<Value>, HashMap<String, Value>) {
    let mut params: HashMap<String, Value> = HashMap::new();

    // Try exact match first
    let route_key = format!("{}:{}", method, path);
    if let Some(handler) = routes.get(&route_key) {
        return (Some(handler.clone()), params);
    }

    // Try ALL method
    let all_key = format!("ALL:{}", path);
    if let Some(handler) = routes.get(&all_key) {
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
fn match_route_pattern(pattern: &str, path: &str) -> Option<HashMap<String, Value>> {
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

    let mut params: HashMap<String, Value> = HashMap::new();

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

/// Build HTTP response from handler return value
fn build_http_response(result: &Value) -> String {
    let (status, body, headers) = match result {
        Value::Dictionary(dict) => {
            let dict_guard = dict.lock().unwrap();

            let status = match dict_guard.get("status") {
                Some(Value::Number(n)) => *n as u16,
                _ => 200,
            };

            let body = match dict_guard.get("body") {
                Some(Value::String(s)) => s.to_string(),
                Some(other) => format!("{}", other),
                None => String::new(),
            };

            let headers: HashMap<String, String> =
                if let Some(Value::Dictionary(hdrs)) = dict_guard.get("headers") {
                    let hdrs_guard = hdrs.lock().unwrap();
                    hdrs_guard
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                match v {
                                    Value::String(s) => s.to_string(),
                                    other => format!("{}", other),
                                },
                            )
                        })
                        .collect()
                } else {
                    HashMap::new()
                };

            (status, body, headers)
        }
        Value::String(s) => (200, s.to_string(), HashMap::new()),
        other => (200, format!("{}", other), HashMap::new()),
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

    let mut response = format!("HTTP/1.1 {} {}\r\n", status, status_text);
    response.push_str(&format!("Content-Length: {}\r\n", body.len()));

    for (key, value) in &headers {
        response.push_str(&format!("{}: {}\r\n", key, value));
    }

    if !headers.contains_key("Content-Type") && !headers.contains_key("content-type") {
        response.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    }

    // Close connection after response (no keep-alive for simplicity)
    response.push_str("Connection: close\r\n");

    response.push_str("\r\n");
    response.push_str(&body);

    response
}
