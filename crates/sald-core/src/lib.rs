// Sald Programming Language
// A fast, class-based interpreter

pub mod ast;
pub mod builtins;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod vm;

// Native-only modules
#[cfg(not(target_arch = "wasm32"))]
pub mod binary;

// WASM module
#[cfg(target_arch = "wasm32")]
pub mod wasm;

use parking_lot::RwLock;
use std::path::PathBuf;

// Project root - set by salad CLI when running a project
// All relative paths are resolved from this root
static PROJECT_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

// Module workspace stack - for resolving paths within modules
// When a module is being imported, its directory is pushed onto this stack
// Paths are resolved relative to the top of the stack (current module)
static MODULE_WORKSPACE_STACK: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Set the project root directory (called by salad CLI)
pub fn set_project_root(path: &std::path::Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    *PROJECT_ROOT.write() = Some(canonical);
}

/// Get the project root directory
pub fn get_project_root() -> Option<PathBuf> {
    PROJECT_ROOT.read().clone()
}

/// Push a module workspace onto the stack
/// Called when entering a module during import
#[cfg(not(target_arch = "wasm32"))]
pub fn push_module_workspace(path: &std::path::Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    MODULE_WORKSPACE_STACK.write().push(canonical);
}

/// Pop the module workspace from the stack
/// Called when leaving a module after import
#[cfg(not(target_arch = "wasm32"))]
pub fn pop_module_workspace() {
    MODULE_WORKSPACE_STACK.write().pop();
}

/// Get the current workspace directory
/// Returns the top of module workspace stack, or project root, or CWD
pub fn get_current_workspace() -> PathBuf {
    // First check module workspace stack
    let stack = MODULE_WORKSPACE_STACK.read();
    if let Some(workspace) = stack.last() {
        return workspace.clone();
    }
    drop(stack);

    // Fall back to project root
    if let Some(root) = get_project_root() {
        return root;
    }

    // Fall back to CWD (or virtual path for WASM)
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("/virtual")
    }
}

/// Resolve a path relative to current workspace
/// Uses module workspace if inside a module, otherwise project root
/// If the path is absolute, return it as-is
pub fn resolve_script_path(path: &str) -> PathBuf {
    let path_buf = PathBuf::from(path);

    // If already absolute, return as-is
    if path_buf.is_absolute() {
        return path_buf;
    }

    // Resolve relative to current workspace
    get_current_workspace().join(path)
}

// Legacy functions for backward compatibility (used by sald CLI without salad)

/// Legacy: Set script directory (for backward compatibility with sald CLI)
#[cfg(not(target_arch = "wasm32"))]
pub fn set_script_dir(path: &str) {
    let dir = PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // If no project root set, use script directory as fallback
    let mut root = PROJECT_ROOT.write();
    if root.is_none() {
        *root = Some(dir.canonicalize().unwrap_or(dir));
    }
}

/// Legacy: Push script directory (no-op in new system)
#[cfg(not(target_arch = "wasm32"))]
pub fn push_script_dir(_path: &str) {
    // No-op - kept for compatibility
}

/// Legacy: Pop script directory (no-op in new system)
#[cfg(not(target_arch = "wasm32"))]
pub fn pop_script_dir() {
    // No-op - kept for compatibility
}

/// Legacy: Get script directory
#[cfg(not(target_arch = "wasm32"))]
pub fn get_script_dir() -> PathBuf {
    get_project_root().unwrap_or_else(|| PathBuf::from("."))
}

// WASM no-op versions
#[cfg(target_arch = "wasm32")]
pub fn set_script_dir(_path: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn push_script_dir(_path: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn pop_script_dir() {}

#[cfg(target_arch = "wasm32")]
pub fn push_module_workspace(_path: &std::path::Path) {}

#[cfg(target_arch = "wasm32")]
pub fn pop_module_workspace() {}
