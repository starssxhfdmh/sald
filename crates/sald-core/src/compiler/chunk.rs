


use super::opcode::OpCode;
use crate::error::Span;
use std::sync::Arc;


#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Number(f64),
    String(Arc<str>), 
    Function(FunctionConstant),
    Class(ClassConstant),
}


#[derive(Debug, Clone, PartialEq)]
pub struct UpvalueInfo {
    pub index: u8,      
    pub is_local: bool, 
}


#[derive(Debug, Clone, PartialEq)]
pub struct FunctionConstant {
    pub name: String,
    pub arity: usize,
    pub is_variadic: bool,          
    pub is_async: bool,             
    pub upvalue_count: usize,       
    pub upvalues: Vec<UpvalueInfo>, 
    pub chunk: Chunk,
    pub file: String,                      
    pub param_names: Vec<String>,          
    pub default_count: usize,              
    pub decorators: Vec<String>,           
    pub namespace_context: Option<String>, 
    pub class_context: Option<String>, 
}


#[derive(Debug, Clone, PartialEq)]
pub struct ClassConstant {
    pub name: String,
    pub methods: Vec<(String, usize, bool)>, 
}


#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chunk {
    
    pub code: Vec<u8>,
    
    pub constants: Vec<Constant>,
    
    pub spans: Vec<Span>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            spans: Vec::new(),
        }
    }

    
    pub fn write(&mut self, byte: u8, span: Span) {
        self.code.push(byte);
        self.spans.push(span);
    }

    
    pub fn write_op(&mut self, op: OpCode, span: Span) {
        self.write(op as u8, span);
    }

    
    pub fn write_u16(&mut self, value: u16, span: Span) {
        self.write((value >> 8) as u8, span);
        self.write((value & 0xFF) as u8, span);
    }

    
    pub fn add_constant(&mut self, constant: Constant) -> usize {
        self.constants.push(constant);
        self.constants.len() - 1
    }

    
    pub fn write_constant(&mut self, constant: Constant, span: Span) -> usize {
        let index = self.add_constant(constant);
        self.write_op(OpCode::Constant, span);
        self.write_u16(index as u16, span);
        index
    }

    
    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    
    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.code.len() - offset - 2;
        self.code[offset] = (jump >> 8) as u8;
        self.code[offset + 1] = (jump & 0xFF) as u8;
    }

    
    pub fn read_u16(&self, offset: usize) -> u16 {
        ((self.code[offset] as u16) << 8) | (self.code[offset + 1] as u16)
    }

    
    pub fn get_span(&self, offset: usize) -> Span {
        if offset < self.spans.len() {
            self.spans[offset]
        } else {
            Span::default()
        }
    }

    
    pub fn get_line(&self, offset: usize) -> usize {
        self.get_span(offset).start.line
    }

    
    pub fn disassemble(&self, name: &str) {
        self.disassemble_with_indent(name, 0);
    }

    
    fn disassemble_with_indent(&self, name: &str, indent: usize) {
        let prefix = "  ".repeat(indent);

        if indent == 0 {
            println!("--- {} ---", name);
        } else {
            println!("\n{}┌── {} ──", prefix, name);
        }
        println!(
            "{}{} bytes, {} constants",
            prefix,
            self.code.len(),
            self.constants.len()
        );
        if indent == 0 {
            println!();
        }

        let mut offset = 0;
        while offset < self.code.len() {
            offset = self.disassemble_instruction_with_indent(offset, indent);
        }

        if indent == 0 {
            println!();
        } else {
            println!("{}└────────────", prefix);
        }

        
        for constant in &self.constants {
            if let Constant::Function(f) = constant {
                f.chunk
                    .disassemble_with_indent(&format!("<fn {}>", f.name), indent + 1);
            }
        }
    }

    
    fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1b' {
                
                if chars.peek() == Some(&'[') {
                    chars.next(); 
                                  
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn format_constant(&self, idx: usize) -> String {
        match self.constants.get(idx) {
            Some(Constant::Number(n)) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Some(Constant::String(s)) => {
                
                let clean = Self::strip_ansi_codes(s);
                
                let char_count = clean.chars().count();
                if char_count > 32 {
                    let truncated: String = clean.chars().take(29).collect();
                    format!("\"{}...\"", truncated)
                } else {
                    format!("\"{}\"", clean)
                }
            }
            Some(Constant::Function(f)) => format!("<fn {}>", f.name),
            Some(Constant::Class(c)) => format!("<class {}>", c.name),
            None => format!("???[{}]", idx),
        }
    }

    fn disassemble_instruction_with_indent(&self, offset: usize, indent: usize) -> usize {
        let prefix = "  ".repeat(indent);
        let span = self.get_span(offset);

        
        if offset > 0 && span.start.line == self.get_span(offset - 1).start.line {
            print!("{}{:04}      ", prefix, offset);
        } else {
            print!("{}{:04} {:4} ", prefix, offset, span.start.line);
        }

        let instruction = OpCode::from(self.code[offset]);
        match instruction {
            
            OpCode::Constant => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("const          {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::True => {
                println!("push           true");
                offset + 1
            }
            OpCode::False => {
                println!("push           false");
                offset + 1
            }
            OpCode::Null => {
                println!("push           null");
                offset + 1
            }

            
            OpCode::DefineGlobal => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("def_global     {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::GetGlobal => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("get_global     {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::SetGlobal => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("set_global     {}", self.format_constant(idx));
                offset + 3
            }

            
            OpCode::GetLocal => {
                let slot = self.read_u16(offset + 1);
                println!("get_local      [{}]", slot);
                offset + 3
            }
            OpCode::SetLocal => {
                let slot = self.read_u16(offset + 1);
                println!("set_local      [{}]", slot);
                offset + 3
            }

            
            OpCode::GetUpvalue => {
                let slot = self.read_u16(offset + 1);
                println!("get_upvalue    [{}]", slot);
                offset + 3
            }
            OpCode::SetUpvalue => {
                let slot = self.read_u16(offset + 1);
                println!("set_upvalue    [{}]", slot);
                offset + 3
            }
            OpCode::CloseUpvalue => {
                println!("close_upvalue");
                offset + 1
            }

            
            OpCode::Pop => {
                println!("pop");
                offset + 1
            }

            
            OpCode::Add => {
                println!("add");
                offset + 1
            }
            OpCode::Sub => {
                println!("sub");
                offset + 1
            }
            OpCode::Mul => {
                println!("mul");
                offset + 1
            }
            OpCode::Div => {
                println!("div");
                offset + 1
            }
            OpCode::Mod => {
                println!("mod");
                offset + 1
            }
            OpCode::Negate => {
                println!("neg");
                offset + 1
            }

            
            OpCode::Not => {
                println!("not");
                offset + 1
            }
            OpCode::Equal => {
                println!("eq");
                offset + 1
            }
            OpCode::NotEqual => {
                println!("neq");
                offset + 1
            }
            OpCode::Less => {
                println!("lt");
                offset + 1
            }
            OpCode::LessEqual => {
                println!("le");
                offset + 1
            }
            OpCode::Greater => {
                println!("gt");
                offset + 1
            }
            OpCode::GreaterEqual => {
                println!("ge");
                offset + 1
            }

            
            OpCode::Jump => {
                let jump = self.read_u16(offset + 1);
                println!("jmp            @{}", offset + 3 + jump as usize);
                offset + 3
            }
            OpCode::JumpIfFalse => {
                let jump = self.read_u16(offset + 1);
                println!("jz             @{}", offset + 3 + jump as usize);
                offset + 3
            }
            OpCode::JumpIfTrue => {
                let jump = self.read_u16(offset + 1);
                println!("jnz            @{}", offset + 3 + jump as usize);
                offset + 3
            }
            OpCode::Loop => {
                let jump = self.read_u16(offset + 1);
                println!("loop           @{}", offset + 3 - jump as usize);
                offset + 3
            }

            
            OpCode::Call => {
                let argc = self.read_u16(offset + 1);
                println!("call           ({})", argc);
                offset + 3
            }
            OpCode::Return => {
                println!("ret");
                offset + 1
            }
            OpCode::Closure => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("closure        {}", self.format_constant(idx));
                offset + 3
            }

            
            OpCode::Class => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("class          {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::Method => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("method         {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::StaticMethod => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("static_method  {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::GetProperty => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("get_prop       {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::SetProperty => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("set_prop       {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::GetSelf => {
                println!("get_self");
                offset + 1
            }
            OpCode::Invoke => {
                let idx = self.read_u16(offset + 1) as usize;
                let argc = self.read_u16(offset + 3);
                println!("invoke         {} ({})", self.format_constant(idx), argc);
                offset + 5
            }

            
            OpCode::Inherit => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("inherit        {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::GetSuper => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("get_super      {}", self.format_constant(idx));
                offset + 3
            }

            
            OpCode::BuildArray => {
                let count = self.read_u16(offset + 1);
                println!("build_array    [{}]", count);
                offset + 3
            }
            OpCode::BuildDict => {
                let count = self.read_u16(offset + 1);
                println!("build_dict     {{{}}}", count);
                offset + 3
            }
            OpCode::GetIndex => {
                println!("get_index");
                offset + 1
            }
            OpCode::SetIndex => {
                println!("set_index");
                offset + 1
            }

            
            OpCode::Import => {
                let idx = self.read_u16(offset + 1) as usize;
                println!("import         {}", self.format_constant(idx));
                offset + 3
            }
            OpCode::ImportAs => {
                let path_idx = self.read_u16(offset + 1) as usize;
                let alias_idx = self.read_u16(offset + 3) as usize;
                println!(
                    "import_as      {} as {}",
                    self.format_constant(path_idx),
                    self.format_constant(alias_idx)
                );
                offset + 5
            }

            
            OpCode::TryStart => {
                let catch_offset = self.read_u16(offset + 1);
                println!(
                    "try_start      catch @{}",
                    offset + 3 + catch_offset as usize
                );
                offset + 3
            }
            OpCode::TryEnd => {
                println!("try_end");
                offset + 1
            }
            OpCode::Throw => {
                println!("throw");
                offset + 1
            }

            
            _ => {
                
                let name = format!("{:?}", instruction).to_lowercase();
                println!("{}", name);
                offset + 1 + instruction.operand_count()
            }
        }
    }
}
