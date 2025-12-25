


pub mod ast;
pub mod builtins;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod vm;


#[cfg(not(target_arch = "wasm32"))]
pub mod binary;


#[cfg(target_arch = "wasm32")]
pub mod wasm;

use parking_lot::RwLock;
use std::path::PathBuf;



static PROJECT_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);




static MODULE_WORKSPACE_STACK: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());


pub fn set_project_root(path: &std::path::Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    *PROJECT_ROOT.write() = Some(canonical);
}


pub fn get_project_root() -> Option<PathBuf> {
    PROJECT_ROOT.read().clone()
}



#[cfg(not(target_arch = "wasm32"))]
pub fn push_module_workspace(path: &std::path::Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    MODULE_WORKSPACE_STACK.write().push(canonical);
}



#[cfg(not(target_arch = "wasm32"))]
pub fn pop_module_workspace() {
    MODULE_WORKSPACE_STACK.write().pop();
}



pub fn get_current_workspace() -> PathBuf {
    
    let stack = MODULE_WORKSPACE_STACK.read();
    if let Some(workspace) = stack.last() {
        return workspace.clone();
    }
    drop(stack);

    
    if let Some(root) = get_project_root() {
        return root;
    }

    
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("/virtual")
    }
}




pub fn resolve_script_path(path: &str) -> PathBuf {
    let path_buf = PathBuf::from(path);

    
    if path_buf.is_absolute() {
        return path_buf;
    }

    
    get_current_workspace().join(path)
}




#[cfg(not(target_arch = "wasm32"))]
pub fn set_script_dir(path: &str) {
    let dir = PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    
    let mut root = PROJECT_ROOT.write();
    if root.is_none() {
        *root = Some(dir.canonicalize().unwrap_or(dir));
    }
}


#[cfg(not(target_arch = "wasm32"))]
pub fn push_script_dir(_path: &str) {
    
}


#[cfg(not(target_arch = "wasm32"))]
pub fn pop_script_dir() {
    
}


#[cfg(not(target_arch = "wasm32"))]
pub fn get_script_dir() -> PathBuf {
    get_project_root().unwrap_or_else(|| PathBuf::from("."))
}


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
