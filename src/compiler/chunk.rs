// Sald Bytecode Chunk
// Contains bytecode, constants, and debug information

use super::opcode::OpCode;
use crate::error::Span;

/// Constant values stored in the constant pool
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Number(f64),
    String(String),
    Function(FunctionConstant),
    Class(ClassConstant),
}

/// Upvalue metadata - describes how to capture a variable
#[derive(Debug, Clone, PartialEq)]
pub struct UpvalueInfo {
    pub index: u8,      // Index in parent's locals or upvalues
    pub is_local: bool, // true = capture from parent's local, false = from parent's upvalue
}

/// Function constant for the constant pool
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionConstant {
    pub name: String,
    pub arity: usize,
    pub is_variadic: bool,          // true if last param is variadic (...args)
    pub is_async: bool,             // true if async function
    pub upvalue_count: usize,       // Number of upvalues this function captures
    pub upvalues: Vec<UpvalueInfo>, // Upvalue capture info
    pub chunk: Chunk,
    pub file: String,               // Source file path
    pub param_names: Vec<String>,   // Parameter names for named arguments
    pub default_count: usize,       // Number of parameters with defaults (from end)
    pub decorators: Vec<String>,    // Decorator names applied to this function
}

/// Class constant for the constant pool
#[derive(Debug, Clone, PartialEq)]
pub struct ClassConstant {
    pub name: String,
    pub methods: Vec<(String, usize, bool)>, // (name, constant_index, is_static)
}

/// A chunk of bytecode
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chunk {
    /// Raw bytecode
    pub code: Vec<u8>,
    /// Constant pool
    pub constants: Vec<Constant>,
    /// Span information for each byte (line, column, offset) for error reporting
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

    /// Write a single byte to the chunk with span info
    pub fn write(&mut self, byte: u8, span: Span) {
        self.code.push(byte);
        self.spans.push(span);
    }

    /// Write an opcode to the chunk
    pub fn write_op(&mut self, op: OpCode, span: Span) {
        self.write(op as u8, span);
    }

    /// Write a u16 operand (big-endian)
    pub fn write_u16(&mut self, value: u16, span: Span) {
        self.write((value >> 8) as u8, span);
        self.write((value & 0xFF) as u8, span);
    }

    /// Add a constant and return its index
    pub fn add_constant(&mut self, constant: Constant) -> usize {
        self.constants.push(constant);
        self.constants.len() - 1
    }

    /// Write a constant instruction
    pub fn write_constant(&mut self, constant: Constant, span: Span) -> usize {
        let index = self.add_constant(constant);
        self.write_op(OpCode::Constant, span);
        self.write_u16(index as u16, span);
        index
    }

    /// Get the current code offset
    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    /// Patch a jump instruction at the given offset
    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.code.len() - offset - 2;
        self.code[offset] = (jump >> 8) as u8;
        self.code[offset + 1] = (jump & 0xFF) as u8;
    }

    /// Read a u16 at the given offset
    pub fn read_u16(&self, offset: usize) -> u16 {
        ((self.code[offset] as u16) << 8) | (self.code[offset + 1] as u16)
    }

    /// Get span for instruction at offset
    pub fn get_span(&self, offset: usize) -> Span {
        if offset < self.spans.len() {
            self.spans[offset]
        } else {
            Span::default()
        }
    }

    /// Get line number for instruction at offset (for backward compatibility)
    pub fn get_line(&self, offset: usize) -> usize {
        self.get_span(offset).start.line
    }

    /// Disassemble the chunk for debugging
    pub fn disassemble(&self, name: &str) {
        self.disassemble_with_indent(name, 0);
    }

    /// Disassemble with indentation for nested functions
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
        
        // Recursively disassemble nested functions
        for constant in &self.constants {
            if let Constant::Function(f) = constant {
                f.chunk.disassemble_with_indent(&format!("<fn {}>", f.name), indent + 1);
            }
        }
    }

    /// Strip ANSI escape codes from a string for clean disassembly output
    fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip escape sequence: ESC [ ... m (or other terminators)
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    // Skip until we hit a letter (the terminator)
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
                // Strip ANSI escape codes to prevent color bleeding in output
                let clean = Self::strip_ansi_codes(s);
                // Use char_indices to safely truncate Unicode strings
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

        // Line info
        if offset > 0 && span.start.line == self.get_span(offset - 1).start.line {
            print!("{}{:04}      ", prefix, offset);
        } else {
            print!("{}{:04} {:4} ", prefix, offset, span.start.line);
        }

        let instruction = OpCode::from(self.code[offset]);
        match instruction {
            // Constants
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

            // Globals
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

            // Locals
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

            // Upvalues
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

            // Stack
            OpCode::Pop => {
                println!("pop");
                offset + 1
            }

            // Arithmetic
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

            // Logic
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

            // Jumps
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

            // Functions
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

            // Classes & Objects
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

            // Inheritance
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

            // Arrays & Dictionaries
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

            // Modules
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

            // Exception handling
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

            // Iterator protocol (if exists, added as fallback)
            _ => {
                // Use Debug format with lowercase
                let name = format!("{:?}", instruction).to_lowercase();
                println!("{}", name);
                offset + 1 + instruction.operand_count()
            }
        }
    }
}
