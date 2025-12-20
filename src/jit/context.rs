// JIT Context - Manages Cranelift JIT state and code cache

use std::collections::HashMap;
use std::sync::Arc;

use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};

use crate::vm::value::Function;

/// Threshold for JIT compilation (number of calls before compiling)
const HOT_THRESHOLD: u32 = 100;

/// JIT-compiled function pointer type
pub type JitFn = extern "C" fn(*mut u8, *const u8, usize) -> u64;

/// JIT Context - manages compilation state and code cache
pub struct JitContext {
    /// Cranelift JIT module for code emission
    module: JITModule,
    /// Reusable function builder context
    builder_ctx: FunctionBuilderContext,
    /// Codegen context
    ctx: Context,
    /// Code cache: function pointer (Arc address) -> native function pointer
    code_cache: HashMap<usize, JitFn>,
    /// Call counts for hot function detection
    call_counts: HashMap<usize, u32>,
    /// Whether JIT is enabled
    enabled: bool,
}

impl JitContext {
    /// Create a new JIT context
    pub fn new() -> Result<Self, String> {
        // Configure Cranelift for the host target
        let mut flag_builder = settings::builder();
        flag_builder.set("opt_level", "speed").map_err(|e| e.to_string())?;
        flag_builder.set("is_pic", "false").map_err(|e| e.to_string())?;
        
        let isa_builder = cranelift_native::builder()
            .map_err(|e| format!("Failed to create ISA builder: {}", e))?;
        
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| format!("Failed to create ISA: {}", e))?;
        
        // Create JIT module
        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        
        Ok(Self {
            module,
            builder_ctx: FunctionBuilderContext::new(),
            ctx: Context::new(),
            code_cache: HashMap::new(),
            call_counts: HashMap::new(),
            enabled: true,
        })
    }
    
    /// Check if JIT is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Enable/disable JIT
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Get cached compiled function if available
    pub fn get_compiled(&self, func: &Arc<Function>) -> Option<JitFn> {
        if !self.enabled {
            return None;
        }
        let key = Arc::as_ptr(func) as usize;
        self.code_cache.get(&key).copied()
    }
    
    /// Increment call count and return true if function should be compiled
    pub fn increment_and_check(&mut self, func: &Arc<Function>) -> bool {
        if !self.enabled {
            return false;
        }
        let key = Arc::as_ptr(func) as usize;
        let count = self.call_counts.entry(key).or_insert(0);
        *count += 1;
        *count == HOT_THRESHOLD
    }
    
    /// Check if function contains Await opcode (should not be JIT compiled)
    pub fn should_skip_jit(func: &Function) -> bool {
        use crate::compiler::OpCode;
        for &byte in &func.chunk.code {
            if byte == OpCode::Await as u8 {
                return true;
            }
        }
        false
    }
    
    /// Compile a function to native code
    pub fn compile(&mut self, func: &Arc<Function>) -> Result<JitFn, String> {
        use super::compiler::compile_function;
        
        // Skip if contains Await
        if Self::should_skip_jit(func) {
            return Err("Function contains Await opcode".to_string());
        }
        
        // Compile bytecode to Cranelift IR
        let native_fn = compile_function(
            &mut self.module,
            &mut self.ctx,
            &mut self.builder_ctx,
            func,
        )?;
        
        // Cache the compiled function
        let key = Arc::as_ptr(func) as usize;
        self.code_cache.insert(key, native_fn);
        
        Ok(native_fn)
    }
    
    /// Get statistics
    pub fn stats(&self) -> JitStats {
        JitStats {
            compiled_functions: self.code_cache.len(),
            tracked_functions: self.call_counts.len(),
        }
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new().expect("Failed to create JIT context")
    }
}

/// JIT statistics
pub struct JitStats {
    pub compiled_functions: usize,
    pub tracked_functions: usize,
}
