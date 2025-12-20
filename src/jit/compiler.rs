// JIT Compiler - Bytecode to Cranelift IR translation

use std::sync::Arc;

use cranelift_codegen::ir::{types, AbiParam, UserFuncName};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use crate::vm::value::Function;

use super::context::JitFn;
use super::translate::translate_bytecode;

/// Compile a Sald function to native code using Cranelift
pub fn compile_function(
    module: &mut JITModule,
    ctx: &mut Context,
    builder_ctx: &mut FunctionBuilderContext,
    func: &Arc<Function>,
) -> Result<JitFn, String> {
    // Create function signature
    // JIT function signature: fn(vm: *mut u8, stack: *mut u8, frame_base: usize) -> u64
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // vm pointer
    sig.params.push(AbiParam::new(types::I64)); // stack pointer
    sig.params.push(AbiParam::new(types::I64)); // frame base (slots_start)
    sig.returns.push(AbiParam::new(types::I64)); // return value (tagged)
    
    // Declare the function
    let func_name = format!("sald_jit_{}", func.name.replace(|c: char| !c.is_alphanumeric(), "_"));
    let func_id = module
        .declare_function(&func_name, Linkage::Local, &sig)
        .map_err(|e| format!("Failed to declare function: {}", e))?;
    
    // Clear previous compilation context
    ctx.clear();
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    
    // Build function body
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
        
        // Translate bytecode to Cranelift IR
        translate_bytecode(&mut builder, module, func)?;
        
        builder.seal_all_blocks();
        builder.finalize();
    }
    
    // Compile to machine code
    module
        .define_function(func_id, ctx)
        .map_err(|e| format!("Failed to define function: {}", e))?;
    
    module.clear_context(ctx);
    
    // Finalize and get function pointer
    module.finalize_definitions()
        .map_err(|e| format!("Failed to finalize: {}", e))?;
    
    let code_ptr = module.get_finalized_function(func_id);
    
    // SAFETY: We trust Cranelift to generate valid code
    let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };
    
    Ok(jit_fn)
}
