


use crate::compiler::chunk::{Chunk, ClassConstant, Constant, FunctionConstant, UpvalueInfo};
use crate::error::{Position, Span};
use crate::vm::interner::intern;

const MAGIC: &[u8; 4] = b"SALD";
const VERSION: u8 = 4; 


pub fn serialize(chunk: &Chunk) -> Vec<u8> {
    let mut out = Vec::new();

    
    out.extend_from_slice(MAGIC);
    out.push(VERSION);

    
    write_u32(&mut out, chunk.constants.len() as u32);
    for constant in &chunk.constants {
        serialize_constant(&mut out, constant);
    }

    
    write_u32(&mut out, chunk.code.len() as u32);
    out.extend_from_slice(&chunk.code);

    
    write_u32(&mut out, chunk.spans.len() as u32);
    for span in &chunk.spans {
        write_u32(&mut out, span.start.line as u32);
        write_u32(&mut out, span.start.column as u32);
        write_u32(&mut out, span.start.offset as u32);
        write_u32(&mut out, span.end.line as u32);
        write_u32(&mut out, span.end.column as u32);
        write_u32(&mut out, span.end.offset as u32);
    }

    out
}


pub fn deserialize(data: &[u8]) -> Result<Chunk, String> {
    let mut cursor = 0;

    
    if data.len() < 5 {
        return Err("Invalid file: too short".to_string());
    }
    if &data[0..4] != MAGIC {
        return Err("Invalid file: not a .saldc file".to_string());
    }
    cursor += 4;

    
    let version = data[cursor];
    if version != VERSION && version != 1 && version != 2 {
        return Err(format!("Unsupported version: {}", version));
    }
    cursor += 1;

    
    let constant_count = read_u32(data, &mut cursor)? as usize;
    let mut constants = Vec::with_capacity(constant_count);
    for _ in 0..constant_count {
        constants.push(deserialize_constant(data, &mut cursor, version)?);
    }

    
    let code_len = read_u32(data, &mut cursor)? as usize;
    if cursor + code_len > data.len() {
        return Err("Invalid file: truncated code".to_string());
    }
    let code = data[cursor..cursor + code_len].to_vec();
    cursor += code_len;

    
    let spans_len = read_u32(data, &mut cursor)? as usize;
    let mut spans = Vec::with_capacity(spans_len);

    if version == 1 {
        
        for _ in 0..spans_len {
            let line = read_u32(data, &mut cursor)? as usize;
            spans.push(Span::single(line, 1, 0));
        }
    } else {
        
        for _ in 0..spans_len {
            let start_line = read_u32(data, &mut cursor)? as usize;
            let start_column = read_u32(data, &mut cursor)? as usize;
            let start_offset = read_u32(data, &mut cursor)? as usize;
            let end_line = read_u32(data, &mut cursor)? as usize;
            let end_column = read_u32(data, &mut cursor)? as usize;
            let end_offset = read_u32(data, &mut cursor)? as usize;
            spans.push(Span::new(
                Position::new(start_line, start_column, start_offset),
                Position::new(end_line, end_column, end_offset),
            ));
        }
    }

    Ok(Chunk {
        code,
        constants,
        spans,
    })
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Result<u32, String> {
    if *cursor + 4 > data.len() {
        return Err("Unexpected end of file".to_string());
    }
    let bytes = [
        data[*cursor],
        data[*cursor + 1],
        data[*cursor + 2],
        data[*cursor + 3],
    ];
    *cursor += 4;
    Ok(u32::from_le_bytes(bytes))
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
}

fn write_optional_string(out: &mut Vec<u8>, s: &Option<String>) {
    match s {
        Some(str) => {
            out.push(1);
            write_string(out, str);
        }
        None => {
            out.push(0);
        }
    }
}

fn read_optional_string(data: &[u8], cursor: &mut usize) -> Result<Option<String>, String> {
    if *cursor >= data.len() {
        return Err("Unexpected end of file".to_string());
    }
    let has_value = data[*cursor] != 0;
    *cursor += 1;
    if has_value {
        Ok(Some(read_string(data, cursor)?))
    } else {
        Ok(None)
    }
}

fn read_string(data: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = read_u32(data, cursor)? as usize;
    if *cursor + len > data.len() {
        return Err("Unexpected end of file".to_string());
    }
    let s = String::from_utf8(data[*cursor..*cursor + len].to_vec())
        .map_err(|_| "Invalid UTF-8 string")?;
    *cursor += len;
    Ok(s)
}

fn serialize_constant(out: &mut Vec<u8>, constant: &Constant) {
    match constant {
        Constant::Number(n) => {
            out.push(0); 
            out.extend_from_slice(&n.to_le_bytes());
        }
        Constant::String(s) => {
            out.push(1);
            write_string(out, s);
        }
        Constant::Function(f) => {
            out.push(2);
            write_string(out, &f.name);
            write_u32(out, f.arity as u32);
            out.push(if f.is_variadic { 1 } else { 0 });
            out.push(if f.is_async { 1 } else { 0 });
            
            write_u32(out, f.upvalue_count as u32);
            for upvalue in &f.upvalues {
                out.push(upvalue.index);
                out.push(if upvalue.is_local { 1 } else { 0 });
            }
            
            write_u32(out, f.param_names.len() as u32);
            for name in &f.param_names {
                write_string(out, name);
            }
            write_u32(out, f.default_count as u32);
            
            write_u32(out, f.decorators.len() as u32);
            for decorator in &f.decorators {
                write_string(out, decorator);
            }
            
            write_optional_string(out, &f.namespace_context);
            write_optional_string(out, &f.class_context);
            let chunk_bytes = serialize(&f.chunk);
            write_u32(out, chunk_bytes.len() as u32);
            out.extend_from_slice(&chunk_bytes);
        }
        Constant::Class(c) => {
            out.push(3);
            write_string(out, &c.name);
            write_u32(out, c.methods.len() as u32);
            for (name, idx, is_static) in &c.methods {
                write_string(out, name);
                write_u32(out, *idx as u32);
                out.push(if *is_static { 1 } else { 0 });
            }
        }
    }
}

fn deserialize_constant(data: &[u8], cursor: &mut usize, version: u8) -> Result<Constant, String> {
    if *cursor >= data.len() {
        return Err("Unexpected end of file".to_string());
    }

    let tag = data[*cursor];
    *cursor += 1;

    match tag {
        0 => {
            
            if *cursor + 8 > data.len() {
                return Err("Unexpected end of file".to_string());
            }
            let bytes = [
                data[*cursor],
                data[*cursor + 1],
                data[*cursor + 2],
                data[*cursor + 3],
                data[*cursor + 4],
                data[*cursor + 5],
                data[*cursor + 6],
                data[*cursor + 7],
            ];
            *cursor += 8;
            Ok(Constant::Number(f64::from_le_bytes(bytes)))
        }
        1 => {
            
            let s = read_string(data, cursor)?;
            Ok(Constant::String(intern(&s)))
        }
        2 => {
            
            let name = read_string(data, cursor)?;
            let arity = read_u32(data, cursor)? as usize;
            if *cursor >= data.len() {
                return Err("Unexpected end of file".to_string());
            }
            let is_variadic = data[*cursor] != 0;
            *cursor += 1;
            
            if *cursor >= data.len() {
                return Err("Unexpected end of file".to_string());
            }
            let is_async = data[*cursor] != 0;
            *cursor += 1;
            
            let upvalue_count = read_u32(data, cursor)? as usize;
            let mut upvalues = Vec::with_capacity(upvalue_count);
            for _ in 0..upvalue_count {
                if *cursor + 2 > data.len() {
                    return Err("Unexpected end of file".to_string());
                }
                let index = data[*cursor];
                let is_local = data[*cursor + 1] != 0;
                *cursor += 2;
                upvalues.push(UpvalueInfo { index, is_local });
            }
            
            let param_count = read_u32(data, cursor)? as usize;
            let mut param_names = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                param_names.push(read_string(data, cursor)?);
            }
            let default_count = read_u32(data, cursor)? as usize;
            
            if version == 2 {
                if *cursor >= data.len() {
                    return Err("Unexpected end of file".to_string());
                }
                
                *cursor += 1;
            }
            let decorator_count = read_u32(data, cursor)? as usize;
            let mut decorators = Vec::with_capacity(decorator_count);
            for _ in 0..decorator_count {
                decorators.push(read_string(data, cursor)?);
            }
            
            let (namespace_context, class_context) = if version >= 4 {
                let ns = read_optional_string(data, cursor)?;
                let cls = read_optional_string(data, cursor)?;
                (ns, cls)
            } else {
                (None, None)
            };
            let chunk_len = read_u32(data, cursor)? as usize;
            if *cursor + chunk_len > data.len() {
                return Err("Unexpected end of file".to_string());
            }
            let chunk = deserialize(&data[*cursor..*cursor + chunk_len])?;
            *cursor += chunk_len;
            Ok(Constant::Function(FunctionConstant {
                name,
                arity,
                is_variadic,
                is_async,
                upvalue_count,
                upvalues,
                chunk,
                file: String::new(),
                param_names,
                default_count,
                decorators,
                namespace_context,
                class_context,
            }))
        }
        3 => {
            
            let name = read_string(data, cursor)?;
            let method_count = read_u32(data, cursor)? as usize;
            let mut methods = Vec::with_capacity(method_count);
            for _ in 0..method_count {
                let method_name = read_string(data, cursor)?;
                let idx = read_u32(data, cursor)? as usize;
                let is_static = data[*cursor] != 0;
                *cursor += 1;
                methods.push((method_name, idx, is_static));
            }
            Ok(Constant::Class(ClassConstant { name, methods }))
        }
        _ => Err(format!("Unknown constant type: {}", tag)),
    }
}
