// Salad - Package Manager for Sald
// CLI tool for managing Sald projects

use clap::{Parser, Subcommand};
use colored::Colorize;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use reqwest::multipart;
use sald::compiler::Compiler;
use sald::lexer::Scanner;
use sald::parser::Parser as SaldParser;
use sald::vm::VM;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fs::{self, File};
use std::io::{Read, Write, stdout};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;

const WEB_URL: &str = "https://saladpm.vercel.app";
const CLI_CALLBACK_PORT: u16 = 9876;

#[derive(Parser)]
#[command(name = "salad")]
#[command(about = "Salad - Package Manager for Sald", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Sald project
    New {
        /// Project name
        name: String,
    },
    /// Initialize a Sald project in the current directory
    Init,
    /// Run the main script (use -- to pass args to script)
    Run {
        /// Arguments to pass to the script
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Check modules
    Check,
    /// Login to registry
    Login,
    /// Logout from registry
    Logout,
    /// Show current user
    Whoami,
    /// Publish package
    Publish,
    /// Install all modules
    Install,
    /// Add packages (e.g., salad add uuid spark@1.0.0)
    Add {
        /// Packages to add (name or name@version)
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Remove packages
    Remove {
        /// Packages to remove
        #[arg(required = true)]
        packages: Vec<String>,
    },
    /// Remove unused modules
    Prune,
}

/// Project configuration structure (salad.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default = "default_main")]
    pub main: String,
    #[serde(default)]
    pub modules: HashMap<String, String>,
}

fn default_main() -> String {
    "main.sald".to_string()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "my-project".to_string(),
            version: "1.0.0".to_string(),
            description: Some("A Sald package".to_string()),
            author: None,
            license: Some("MIT".to_string()),
            main: default_main(),
            modules: HashMap::new(),
        }
    }
}

/// Credentials stored locally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    pub uid: String,
    pub email: String,
    pub username: String,
}

/// Module check result
#[derive(Debug)]
struct ModuleCheckResult {
    missing: Vec<(String, String)>,
    found: Vec<(String, String, String)>,
    version_mismatch: Vec<(String, String, String)>,
}

impl ModuleCheckResult {
    fn new() -> Self {
        Self {
            missing: Vec::new(),
            found: Vec::new(),
            version_mismatch: Vec::new(),
        }
    }

    fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.version_mismatch.is_empty()
    }
}

// ============================================================================
// Pretty output helpers (pnpm-style)
// ============================================================================

fn print_header() {
    println!();
    println!("{}", "salad".green().bold());
}

fn print_progress(action: &str, name: &str, version: Option<&str>) {
    print!("{} {}", action.green(), name.cyan().bold());
    if let Some(v) = version {
        print!(" {}", v.dimmed());
    }
    stdout().flush().ok();
}

// fn print_done(msg: &str) {
//     println!(" {}", msg.green());
// }

fn print_fail(msg: &str) {
    println!(" {}", msg.red());
}

fn print_info(msg: &str) {
    println!("{} {}", "|".dimmed(), msg);
}

fn print_success(msg: &str) {
    println!("{} {}", "Done".green().bold(), msg.dimmed());
}

fn print_error(msg: &str) {
    eprintln!("{} {}", "ERR".red().bold(), msg);
}

fn print_warn(msg: &str) {
    eprintln!("{} {}", "WARN".yellow().bold(), msg);
}

// ============================================================================
// Utility functions
// ============================================================================

fn get_credentials_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".salad").join("credentials.json")
}

fn load_credentials() -> Option<Credentials> {
    let path = get_credentials_path();
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

fn save_credentials(creds: &Credentials) -> Result<(), String> {
    let path = get_credentials_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(creds).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

fn delete_credentials() -> Result<(), String> {
    let path = get_credentials_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn default_salad_json(name: &str, author: Option<&str>) -> String {
    let config = ProjectConfig {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: Some("A Sald package".to_string()),
        author: author.map(|s| s.to_string()),
        license: Some("MIT".to_string()),
        main: "main.sald".to_string(),
        modules: HashMap::new(),
    };
    serde_json::to_string_pretty(&config).unwrap()
}

fn default_main_sald() -> &'static str {
    r#"// Run with: salad run

Console.println("Hello from Sald!")
"#
}

fn find_project_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join("salad.json").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn parse_config(config_path: &Path) -> Result<ProjectConfig, String> {
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn save_config(project_root: &Path, config: &ProjectConfig) -> Result<(), String> {
    let config_path = project_root.join("salad.json");
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

fn check_modules(project_root: &Path) -> Result<ModuleCheckResult, String> {
    let config = parse_config(&project_root.join("salad.json"))?;
    let sald_modules_dir = project_root.join("sald_modules");
    let mut result = ModuleCheckResult::new();
    let mut checked: HashSet<String> = HashSet::new();
    let mut to_check: Vec<(String, String)> = config.modules.into_iter().collect();
    
    while let Some((module_name, required_version)) = to_check.pop() {
        if checked.contains(&module_name) {
            continue;
        }
        checked.insert(module_name.clone());
        
        let module_dir = sald_modules_dir.join(&module_name);
        let module_config_path = module_dir.join("salad.json");
        
        if !module_dir.exists() || !module_config_path.exists() {
            result.missing.push((module_name, required_version));
            continue;
        }
        
        let module_config = match parse_config(&module_config_path) {
            Ok(c) => c,
            Err(_) => {
                result.missing.push((module_name, required_version));
                continue;
            }
        };
        
        if module_config.version != required_version {
            result.version_mismatch.push((
                module_name.clone(),
                required_version.clone(),
                module_config.version.clone(),
            ));
        }
        
        result.found.push((module_name.clone(), required_version, module_config.version.clone()));
        
        for (dep_name, dep_version) in module_config.modules {
            if !checked.contains(&dep_name) {
                to_check.push((dep_name, dep_version));
            }
        }
    }
    
    Ok(result)
}

fn create_package_zip(project_root: &Path) -> Result<Vec<u8>, String> {
    use std::io::Cursor;
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        add_files_to_zip(&mut zip, project_root, project_root, &options)?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buffer.into_inner())
}

fn add_files_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base_path: &Path,
    current_path: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(current_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.strip_prefix(base_path).unwrap();
        let name_str = name.to_string_lossy();
        
        if name_str.starts_with("sald_modules") 
            || name_str.starts_with("node_modules")
            || name_str.starts_with(".")
            || name_str.starts_with("target") {
            continue;
        }
        
        if path.is_dir() {
            let dir_name = format!("{}/", name.to_string_lossy());
            zip.add_directory(&dir_name, *options).map_err(|e| e.to_string())?;
            add_files_to_zip(zip, base_path, &path, options)?;
        } else {
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).map_err(|e| e.to_string())?;
            zip.start_file(name.to_string_lossy(), *options).map_err(|e| e.to_string())?;
            zip.write_all(&contents).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn parse_package_spec(spec: &str) -> (String, Option<String>) {
    if let Some(at_idx) = spec.find('@') {
        (spec[..at_idx].to_string(), Some(spec[at_idx + 1..].to_string()))
    } else {
        (spec.to_string(), None)
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        Some(Commands::New { name }) => cmd_new(&name),
        Some(Commands::Init) => cmd_init(),
        Some(Commands::Run { args }) => cmd_run(&args).await,
        Some(Commands::Check) => cmd_check(),
        Some(Commands::Login) => cmd_login().await,
        Some(Commands::Logout) => cmd_logout(),
        Some(Commands::Whoami) => cmd_whoami(),
        Some(Commands::Publish) => cmd_publish().await,
        Some(Commands::Install) => cmd_install().await,
        Some(Commands::Add { packages }) => cmd_add(&packages).await,
        Some(Commands::Remove { packages }) => cmd_remove(&packages),
        Some(Commands::Prune) => cmd_prune(),
        None => cmd_run(&[]).await,
    }
}

// ============================================================================
// Commands
// ============================================================================

fn cmd_new(name: &str) {
    let project_dir = PathBuf::from(name);
    
    if project_dir.exists() {
        print_error(&format!("Directory '{}' already exists", name));
        std::process::exit(1);
    }
    
    let author = load_credentials().map(|c| c.username);
    
    fs::create_dir_all(&project_dir).expect("Failed to create directory");
    fs::create_dir_all(project_dir.join("sald_modules")).ok();
    fs::write(project_dir.join("salad.json"), default_salad_json(name, author.as_deref())).ok();
    fs::write(project_dir.join("main.sald"), default_main_sald()).ok();
    
    print_header();
    println!();
    print_info(&format!("Created project {}", name.cyan().bold()));
    if let Some(a) = author {
        print_info(&format!("Author: {}", a.dimmed()));
    }
    println!();
    println!("  {} {}", "cd".dimmed(), name);
    println!("  {} {}", "salad".dimmed(), "run");
    println!();
}

fn cmd_init() {
    if PathBuf::from("salad.json").exists() {
        print_error("salad.json already exists");
        std::process::exit(1);
    }
    
    let name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "my-project".to_string());
    
    let author = load_credentials().map(|c| c.username);
    
    fs::create_dir_all("sald_modules").ok();
    fs::write("salad.json", default_salad_json(&name, author.as_deref())).ok();
    if !PathBuf::from("main.sald").exists() {
        fs::write("main.sald", default_main_sald()).ok();
    }
    
    print_header();
    println!();
    print_info(&format!("Initialized {}", name.cyan().bold()));
    println!();
}

fn cmd_check() {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };
    
    print_header();
    println!();
    
    match check_modules(&project_root) {
        Ok(result) => {
            for (name, _, version) in &result.found {
                println!("{} {} {}", "+".green(), name.cyan(), version.dimmed());
            }
            for (name, required, found) in &result.version_mismatch {
                println!("{} {} {} {}", "~".yellow(), name.cyan(), found.dimmed(), format!("(want {})", required).dimmed());
            }
            for (name, version) in &result.missing {
                println!("{} {} {}", "x".red(), name.cyan(), version.dimmed());
            }
            
            println!();
            if result.is_ok() {
                print_success("All modules OK");
            } else {
                print_error("Missing modules. Run 'salad install'");
                std::process::exit(1);
            }
        }
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    }
    println!();
}

async fn cmd_login() {
    if let Some(creds) = load_credentials() {
        println!();
        print_info(&format!("Already logged in as {}", creds.username.cyan().bold()));
        print_info("Run 'salad logout' first");
        println!();
        return;
    }

    print_header();
    println!();
    print_info("Opening browser...");

    let (tx, rx) = oneshot::channel::<String>();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let addr = SocketAddr::from(([127, 0, 0, 1], CLI_CALLBACK_PORT));
    
    let tx_clone = tx.clone();
    let make_svc = make_service_fn(move |_| {
        let tx = tx_clone.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let tx = tx.clone();
                async move { handle_callback(req, tx).await }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    let login_url = format!("{}/login?cli=true&port={}", WEB_URL, CLI_CALLBACK_PORT);
    
    if opener::open(&login_url).is_err() {
        print_info(&format!("Open: {}", login_url));
    }

    print_info("Waiting for login...");
    println!();

    let graceful = server.with_graceful_shutdown(async { let _ = rx.await; });
    if let Err(e) = graceful.await {
        print_error(&format!("Server error: {}", e));
        std::process::exit(1);
    }
}

async fn handle_callback(
    req: Request<Body>,
    tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<String>>>>,
) -> Result<Response<Body>, Infallible> {
    if req.uri().path() == "/callback" {
        if let Some(query) = req.uri().query() {
            let params: HashMap<String, String> = query
                .split('&')
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();

            if let Some(token) = params.get("token") {
                let client = reqwest::Client::new();
                if let Ok(response) = client
                    .post(&format!("{}/api/auth/verify", WEB_URL))
                    .json(&serde_json::json!({ "token": token }))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        if let Ok(data) = response.json::<serde_json::Value>().await {
                            let creds = Credentials {
                                token: token.clone(),
                                uid: data["uid"].as_str().unwrap_or("").to_string(),
                                email: data["email"].as_str().unwrap_or("").to_string(),
                                username: data["username"].as_str().unwrap_or("").to_string(),
                            };
                            save_credentials(&creds).ok();
                            println!("{} Logged in as {}", "Done".green().bold(), creds.username.cyan().bold());
                            println!();

                            if let Some(tx) = tx.lock().await.take() {
                                let _ = tx.send(token.clone());
                            }

                            tokio::spawn(async {
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                std::process::exit(0);
                            });

                            let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Salad</title><style>body{font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#f5f5f5}.c{text-align:center;padding:2rem;background:#fff;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,.1)}h1{color:#22c55e;margin:0 0 .5rem;font-size:1.5rem}p{color:#666;margin:0}</style></head><body><div class="c"><h1>Success</h1><p>You can close this window.</p></div></body></html>"#;
                            return Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "text/html; charset=utf-8")
                                .body(Body::from(html))
                                .unwrap());
                        }
                    }
                }
            }
        }
        return Ok(Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from("Failed")).unwrap());
    }
    Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Body::from("")).unwrap())
}

fn cmd_logout() {
    delete_credentials().ok();
    print_header();
    println!();
    print_success("Logged out");
    println!();
}

fn cmd_whoami() {
    print_header();
    println!();
    match load_credentials() {
        Some(creds) => {
            println!("{} {}", "user".dimmed(), creds.username.cyan().bold());
            println!("{} {}", "email".dimmed(), creds.email);
        }
        None => {
            print_info("Not logged in");
        }
    }
    println!();
}

async fn cmd_publish() {
    let creds = match load_credentials() {
        Some(c) => c,
        None => {
            print_error("Not logged in. Run 'salad login'");
            std::process::exit(1);
        }
    };

    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };

    let config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    print_header();
    println!();
    print_progress("Publishing", &config.name, Some(&config.version));
    println!();

    let zip_data = match create_package_zip(&project_root) {
        Ok(d) => d,
        Err(e) => {
            print_fail(&e);
            std::process::exit(1);
        }
    };

    let client = reqwest::Client::new();
    let form = multipart::Form::new()
        .text("config", serde_json::to_string(&config).unwrap())
        .part("package", multipart::Part::bytes(zip_data)
            .file_name(format!("{}-{}.zip", config.name, config.version))
            .mime_str("application/zip").unwrap());

    match client
        .post(&format!("{}/api/packages", WEB_URL))
        .header("Authorization", format!("Bearer {}", creds.token))
        .multipart(form)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!();
                print_success(&format!("Published {}@{}", config.name, config.version));
            } else {
                let body = response.text().await.unwrap_or_default();
                let err: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                print_error(err["error"].as_str().unwrap_or("Failed"));
                std::process::exit(1);
            }
        }
        Err(e) => {
            print_error(&e.to_string());
            std::process::exit(1);
        }
    }
    println!();
}

async fn cmd_install() {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };

    let config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    if config.modules.is_empty() {
        println!();
        print_info("No modules to install");
        println!();
        return;
    }

    let start = Instant::now();
    print_header();
    println!();

    let sald_modules_dir = project_root.join("sald_modules");
    fs::create_dir_all(&sald_modules_dir).ok();

    let client = reqwest::Client::new();
    let mut installed = 0;
    let mut failed = 0;
    let mut to_install: Vec<(String, String)> = config.modules.into_iter().collect();
    let mut done: HashSet<String> = HashSet::new();

    while let Some((name, version)) = to_install.pop() {
        if done.contains(&name) { continue; }

        print!("{} {} {}", "+".green(), name.cyan().bold(), version.dimmed());
        stdout().flush().ok();

        if install_package(&client, &sald_modules_dir, &name, &version).await.is_some() {
            println!();
            installed += 1;
            done.insert(name.clone());
            
            // Add transitive deps
            let dep_config_path = sald_modules_dir.join(&name).join("salad.json");
            if let Ok(dep_config) = parse_config(&dep_config_path) {
                for (dep_name, dep_version) in dep_config.modules {
                    if !done.contains(&dep_name) {
                        to_install.push((dep_name, dep_version));
                    }
                }
            }
        } else {
            println!(" {}", "failed".red());
            failed += 1;
        }
    }

    let elapsed = start.elapsed();
    println!();
    if failed == 0 {
        print_success(&format!("{} packages in {:.1}s", installed, elapsed.as_secs_f64()));
    } else {
        print_warn(&format!("{} installed, {} failed", installed, failed));
    }
    println!();
}

async fn install_package(client: &reqwest::Client, modules_dir: &Path, name: &str, version: &str) -> Option<()> {
    let url = format!("{}/api/packages/{}/{}", WEB_URL, name, version);
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() { return None; }
    
    let data = response.json::<serde_json::Value>().await.ok()?;
    let file_url = data["fileUrl"].as_str()?;
    
    let zip_response = client.get(file_url).send().await.ok()?;
    if !zip_response.status().is_success() { return None; }
    
    let bytes = zip_response.bytes().await.ok()?;
    let module_dir = modules_dir.join(name);
    
    if module_dir.exists() { fs::remove_dir_all(&module_dir).ok(); }
    fs::create_dir_all(&module_dir).ok();
    
    let cursor = std::io::Cursor::new(bytes.as_ref());
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    
    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let outpath = module_dir.join(file.name());
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath).ok();
            } else {
                if let Some(p) = outpath.parent() { fs::create_dir_all(p).ok(); }
                if let Ok(mut outfile) = File::create(&outpath) {
                    std::io::copy(&mut file, &mut outfile).ok();
                }
            }
        }
    }
    
    Some(())
}

async fn cmd_add(packages: &[String]) {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };

    let mut config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    let start = Instant::now();
    print_header();
    println!();

    let client = reqwest::Client::new();
    let sald_modules_dir = project_root.join("sald_modules");
    fs::create_dir_all(&sald_modules_dir).ok();

    let mut added = 0;
    let mut failed = 0;

    for spec in packages {
        let (name, version_opt) = parse_package_spec(spec);
        
        let had_version = version_opt.is_some();
        
        // Get version (latest if not specified)
        let version = match version_opt {
            Some(v) => v,
            None => {
                print!("{} {} ", "+".green(), name.cyan().bold());
                stdout().flush().ok();
                
                let url = format!("{}/api/packages/{}", WEB_URL, name);
                match client.get(&url).send().await {
                    Ok(r) if r.status().is_success() => {
                        if let Ok(data) = r.json::<serde_json::Value>().await {
                            let v = data["latestVersion"].as_str().unwrap_or("1.0.0").to_string();
                            print!("{}", v.dimmed());
                            stdout().flush().ok();
                            v
                        } else {
                            println!("{}", "failed".red());
                            failed += 1;
                            continue;
                        }
                    }
                    _ => {
                        println!("{}", "not found".red());
                        failed += 1;
                        continue;
                    }
                }
            }
        };

        if had_version {
            print!("{} {} {}", "+".green(), name.cyan().bold(), version.dimmed());
            stdout().flush().ok();
        }

        // Install the main package
        if install_package(&client, &sald_modules_dir, &name, &version).await.is_some() {
            config.modules.insert(name.clone(), version.clone());
            println!();
            added += 1;
            
            // Install transitive dependencies
            let mut done: HashSet<String> = HashSet::new();
            done.insert(name.clone());
            
            let dep_config_path = sald_modules_dir.join(&name).join("salad.json");
            if let Ok(dep_config) = parse_config(&dep_config_path) {
                let mut to_install: Vec<(String, String)> = dep_config.modules.into_iter().collect();
                
                while let Some((dep_name, dep_version)) = to_install.pop() {
                    if done.contains(&dep_name) { continue; }
                    
                    print!("{} {} {}", "+".green(), dep_name.cyan(), dep_version.dimmed());
                    stdout().flush().ok();
                    
                    if install_package(&client, &sald_modules_dir, &dep_name, &dep_version).await.is_some() {
                        done.insert(dep_name.clone());
                        println!();
                        added += 1;
                        
                        // Check for more transitive deps
                        let nested_config = sald_modules_dir.join(&dep_name).join("salad.json");
                        if let Ok(nested) = parse_config(&nested_config) {
                            for (n, v) in nested.modules {
                                if !done.contains(&n) {
                                    to_install.push((n, v));
                                }
                            }
                        }
                    } else {
                        println!(" {}", "failed".red());
                        failed += 1;
                    }
                }
            }
        } else {
            println!(" {}", "failed".red());
            failed += 1;
        }
    }

    // Save config
    save_config(&project_root, &config).ok();

    let elapsed = start.elapsed();
    println!();
    if failed == 0 {
        print_success(&format!("{} packages in {:.1}s", added, elapsed.as_secs_f64()));
    } else {
        print_warn(&format!("{} added, {} failed", added, failed));
    }
    println!();
}

fn cmd_remove(packages: &[String]) {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };

    let mut config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    print_header();
    println!();

    let mut removed = 0;

    for name in packages {
        if config.modules.remove(name).is_some() {
            let module_dir = project_root.join("sald_modules").join(name);
            if module_dir.exists() {
                fs::remove_dir_all(&module_dir).ok();
            }
            println!("{} {}", "-".red(), name.cyan().bold());
            removed += 1;
        } else {
            println!("{} {} {}", "x".dimmed(), name, "(not found)".dimmed());
        }
    }

    save_config(&project_root, &config).ok();

    println!();
    print_success(&format!("{} packages removed", removed));
    println!();
}

async fn cmd_run(args: &[String]) {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };
    
    sald::set_project_root(&project_root);
    
    if let Ok(result) = check_modules(&project_root) {
        if !result.is_ok() {
            print_error("Missing modules. Run 'salad install'");
            std::process::exit(1);
        }
    }
    
    let config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };
    
    let full_path = project_root.join(&config.main);
    if !full_path.exists() {
        print_error(&format!("File not found: {}", config.main));
        std::process::exit(1);
    }
    
    run_script(&full_path, args).await;
}

async fn run_script(path: &Path, args: &[String]) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            print_error(&format!("Failed to read file: {}", e));
            std::process::exit(1);
        }
    };
    
    let path_str = path.to_string_lossy().to_string();
    
    let mut scanner = Scanner::new(&source, &path_str);
    let tokens = match scanner.scan_tokens() {
        Ok(t) => t,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    
    let mut parser = SaldParser::new(tokens, &path_str, &source);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    
    let mut compiler = Compiler::new(&path_str, &source);
    let chunk = match compiler.compile(&program) {
        Ok(c) => c,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    
    let mut vm = VM::new();
    // Set args for the script
    vm.set_args(args.to_vec());
    if let Err(e) = vm.run(chunk, &path_str, &source).await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn cmd_prune() {
    let project_root = match find_project_root() {
        Some(r) => r,
        None => {
            print_error("No salad.json found");
            std::process::exit(1);
        }
    };

    let config = match parse_config(&project_root.join("salad.json")) {
        Ok(c) => c,
        Err(e) => {
            print_error(&e);
            std::process::exit(1);
        }
    };

    let sald_modules_dir = project_root.join("sald_modules");
    if !sald_modules_dir.exists() {
        println!();
        print_info("No modules installed");
        println!();
        return;
    }

    // Collect all required modules (direct + transitive)
    let mut required: HashSet<String> = HashSet::new();
    let mut to_check: Vec<String> = config.modules.keys().cloned().collect();

    while let Some(name) = to_check.pop() {
        if required.contains(&name) {
            continue;
        }
        required.insert(name.clone());

        // Check transitive deps
        let dep_config_path = sald_modules_dir.join(&name).join("salad.json");
        if let Ok(dep_config) = parse_config(&dep_config_path) {
            for dep_name in dep_config.modules.keys() {
                if !required.contains(dep_name) {
                    to_check.push(dep_name.clone());
                }
            }
        }
    }

    // Find installed modules
    let mut installed: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&sald_modules_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    installed.push(name.to_string());
                }
            }
        }
    }

    // Find orphaned
    let orphaned: Vec<String> = installed
        .into_iter()
        .filter(|name| !required.contains(name))
        .collect();

    print_header();
    println!();

    if orphaned.is_empty() {
        print_info("No unused modules");
    } else {
        for name in &orphaned {
            let module_dir = sald_modules_dir.join(name);
            if fs::remove_dir_all(&module_dir).is_ok() {
                println!("{} {}", "-".red(), name.cyan());
            }
        }
        println!();
        print_success(&format!("{} modules removed", orphaned.len()));
    }
    println!();
}
