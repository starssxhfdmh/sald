// JIT Translator - Bytecode to Cranelift IR translation

use std::collections::HashMap;
use std::sync::Arc;

use cranelift_codegen::ir::{
    types, Block, InstBuilder, MemFlags, Value as ClifValue,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::JITModule;

use crate::compiler::chunk::Constant;
use crate::compiler::OpCode;
use crate::vm::value::Function;

use super::runtime::{
    encode_number, TAG_FALSE, TAG_NULL, TAG_TRUE,
};

/// Translate Sald bytecode to Cranelift IR
pub fn translate_bytecode(
    builder: &mut FunctionBuilder,
    _module: &mut JITModule,
    func: &Arc<Function>,
) -> Result<(), String> {
    let chunk = &func.chunk;
    let code = &chunk.code;
    
    // Create entry block
    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    
    // Get function parameters
    let _vm_ptr = builder.block_params(entry_block)[0];
    let stack_ptr = builder.block_params(entry_block)[1];
    let frame_base = builder.block_params(entry_block)[2];
    
    // Create stack for JIT values (virtual stack)
    let mut value_stack: Vec<ClifValue> = Vec::new();
    
    // Track if current block is terminated
    let mut block_terminated = false;
    
    // Create basic blocks for jump targets
    let mut block_map: HashMap<usize, Block> = HashMap::new();
    
    // First pass: identify jump targets and create blocks
    let mut ip = 0;
    while ip < code.len() {
        let opcode = OpCode::from(code[ip]);
        match opcode {
            OpCode::Jump | OpCode::JumpIfFalse | OpCode::JumpIfTrue | OpCode::JumpIfNotNull => {
                let offset = ((code[ip + 1] as u16) << 8 | code[ip + 2] as u16) as usize;
                let target = ip + 3 + offset;
                if !block_map.contains_key(&target) {
                    block_map.insert(target, builder.create_block());
                }
                ip += 3;
            }
            OpCode::Loop => {
                let offset = ((code[ip + 1] as u16) << 8 | code[ip + 2] as u16) as usize;
                let target = ip + 3 - offset;
                if !block_map.contains_key(&target) {
                    block_map.insert(target, builder.create_block());
                }
                ip += 3;
            }
            _ => {
                ip += 1 + opcode.operand_count();
            }
        }
    }
    
    // Second pass: translate instructions
    ip = 0;
    while ip < code.len() {
        // Check if this IP is a jump target - if so, switch to that block
        if let Some(&block) = block_map.get(&ip) {
            if !block_terminated {
                builder.ins().jump(block, &[]);
            }
            builder.switch_to_block(block);
            block_terminated = false;
        }
        
        let opcode = OpCode::from(code[ip]);
        ip += 1;
        
        match opcode {
            // Constants
            OpCode::Constant => {
                let idx = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                
                let value = match &chunk.constants[idx] {
                    Constant::Number(n) => {
                        let bits = encode_number(*n);
                        builder.ins().iconst(types::I64, bits as i64)
                    }
                    _ => {
                        // For non-numeric constants, fall back to interpreter
                        // TODO: handle strings, functions, etc.
                        builder.ins().iconst(types::I64, TAG_NULL as i64)
                    }
                };
                value_stack.push(value);
            }
            
            // Literals
            OpCode::Null => {
                let v = builder.ins().iconst(types::I64, TAG_NULL as i64);
                value_stack.push(v);
            }
            OpCode::True => {
                let v = builder.ins().iconst(types::I64, TAG_TRUE as i64);
                value_stack.push(v);
            }
            OpCode::False => {
                let v = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                value_stack.push(v);
            }
            
            // Stack operations
            OpCode::Pop => {
                value_stack.pop();
            }
            OpCode::Dup => {
                if let Some(&v) = value_stack.last() {
                    value_stack.push(v);
                }
            }
            
            // Locals
            OpCode::GetLocal => {
                let slot = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                
                // Load from VM stack: stack_ptr + (frame_base + slot) * 8
                let slot_const = builder.ins().iconst(types::I64, (slot * 8) as i64);
                let base_offset = builder.ins().imul_imm(frame_base, 8);
                let offset = builder.ins().iadd(base_offset, slot_const);
                let addr = builder.ins().iadd(stack_ptr, offset);
                let value = builder.ins().load(types::I64, MemFlags::new(), addr, 0);
                value_stack.push(value);
            }
            OpCode::SetLocal => {
                let slot = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                
                if let Some(&value) = value_stack.last() {
                    let slot_const = builder.ins().iconst(types::I64, (slot * 8) as i64);
                    let base_offset = builder.ins().imul_imm(frame_base, 8);
                    let offset = builder.ins().iadd(base_offset, slot_const);
                    let addr = builder.ins().iadd(stack_ptr, offset);
                    builder.ins().store(MemFlags::new(), value, addr, 0);
                }
            }
            
            // Arithmetic (assuming numeric values)
            OpCode::Add => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    // Interpret as f64, add, reinterpret as i64
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let result_f = builder.ins().fadd(a_f, b_f);
                    let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f);
                    value_stack.push(result);
                }
            }
            OpCode::Sub => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let result_f = builder.ins().fsub(a_f, b_f);
                    let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f);
                    value_stack.push(result);
                }
            }
            OpCode::Mul => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let result_f = builder.ins().fmul(a_f, b_f);
                    let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f);
                    value_stack.push(result);
                }
            }
            OpCode::Div => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    // TODO: Add division by zero check
                    let result_f = builder.ins().fdiv(a_f, b_f);
                    let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f);
                    value_stack.push(result);
                }
            }
            OpCode::Negate => {
                if let Some(v) = value_stack.pop() {
                    let v_f = builder.ins().bitcast(types::F64, MemFlags::new(), v);
                    let result_f = builder.ins().fneg(v_f);
                    let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f);
                    value_stack.push(result);
                }
            }
            
            // Comparisons
            OpCode::Less => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::LessThan, a_f, b_f);
                    // Convert bool to tagged value
                    let true_val = builder.ins().iconst(types::I64, TAG_TRUE as i64);
                    let false_val = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                    let result = builder.ins().select(cmp, true_val, false_val);
                    value_stack.push(result);
                }
            }
            OpCode::LessEqual => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual, a_f, b_f);
                    let true_val = builder.ins().iconst(types::I64, TAG_TRUE as i64);
                    let false_val = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                    let result = builder.ins().select(cmp, true_val, false_val);
                    value_stack.push(result);
                }
            }
            OpCode::Greater => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::GreaterThan, a_f, b_f);
                    let true_val = builder.ins().iconst(types::I64, TAG_TRUE as i64);
                    let false_val = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                    let result = builder.ins().select(cmp, true_val, false_val);
                    value_stack.push(result);
                }
            }
            OpCode::GreaterEqual => {
                if value_stack.len() >= 2 {
                    let b = value_stack.pop().unwrap();
                    let a = value_stack.pop().unwrap();
                    let a_f = builder.ins().bitcast(types::F64, MemFlags::new(), a);
                    let b_f = builder.ins().bitcast(types::F64, MemFlags::new(), b);
                    let cmp = builder.ins().fcmp(cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual, a_f, b_f);
                    let true_val = builder.ins().iconst(types::I64, TAG_TRUE as i64);
                    let false_val = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                    let result = builder.ins().select(cmp, true_val, false_val);
                    value_stack.push(result);
                }
            }
            
            // Control flow
            OpCode::Jump => {
                let offset = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                let target = ip + offset;
                if let Some(&block) = block_map.get(&target) {
                    builder.ins().jump(block, &[]);
                    block_terminated = true;
                }
            }
            OpCode::JumpIfFalse => {
                let offset = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                let target = ip + offset;
                
                if let Some(&cond) = value_stack.last() {
                    // Check if value is truthy (not null and not false)
                    let null_val = builder.ins().iconst(types::I64, TAG_NULL as i64);
                    let false_val = builder.ins().iconst(types::I64, TAG_FALSE as i64);
                    let is_null = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, cond, null_val);
                    let is_false = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, cond, false_val);
                    let is_falsy = builder.ins().bor(is_null, is_false);
                    
                    if let Some(&target_block) = block_map.get(&target) {
                        let fallthrough = builder.create_block();
                        builder.ins().brif(is_falsy, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                    }
                }
            }
            OpCode::Loop => {
                let offset = ((code[ip] as u16) << 8 | code[ip + 1] as u16) as usize;
                ip += 2;
                let target = (ip as isize - offset as isize) as usize;
                if let Some(&block) = block_map.get(&target) {
                    builder.ins().jump(block, &[]);
                    block_terminated = true;
                }
            }
            
            // Return
            OpCode::Return => {
                let ret_val = value_stack.pop().unwrap_or_else(|| {
                    builder.ins().iconst(types::I64, TAG_NULL as i64)
                });
                builder.ins().return_(&[ret_val]);
                block_terminated = true;
            }
            
            // TODO: Handle remaining opcodes
            _ => {
                // For unhandled opcodes, we need to fall back to interpreter
                // For now, just skip operands
                ip += opcode.operand_count();
            }
        }
    }
    
    // If we reach the end without a return, return null
    if !block_terminated {
        let null = builder.ins().iconst(types::I64, TAG_NULL as i64);
        builder.ins().return_(&[null]);
    }
    
    Ok(())
}
