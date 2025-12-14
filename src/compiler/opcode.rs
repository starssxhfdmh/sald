// Sald Bytecode Instructions

/// Bytecode operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    // Constants and stack operations
    Constant, // Push constant onto stack
    Pop,      // Pop top of stack
    Dup,      // Duplicate top of stack
    DupTwo,   // Duplicate top two elements: [a, b] -> [a, b, a, b]

    // Literals
    Null,  // Push null
    True,  // Push true
    False, // Push false

    // Variables
    DefineGlobal, // Define global variable
    GetGlobal,    // Get global variable
    SetGlobal,    // Set global variable
    GetLocal,     // Get local variable
    SetLocal,     // Set local variable

    // Arithmetic operations
    Add,    // a + b
    Sub,    // a - b
    Mul,    // a * b
    Div,    // a / b
    Mod,    // a % b
    Negate, // -a

    // Comparison operations
    Equal,        // a == b
    NotEqual,     // a != b
    Less,         // a < b
    LessEqual,    // a <= b
    Greater,      // a > b
    GreaterEqual, // a >= b

    // Logical operations
    Not, // !a

    // Control flow
    Jump,          // Unconditional jump
    JumpIfFalse,   // Jump if top of stack is false
    JumpIfTrue,    // Jump if top of stack is true
    JumpIfNotNull, // Jump if top of stack is not null (for default params)
    Loop,          // Jump backward (for loops)

    // Functions
    Call,    // Call function
    Return,  // Return from function
    Closure, // Create closure

    // Classes and objects
    Class,        // Define class
    Method,       // Define method
    StaticMethod, // Define static method
    GetProperty,  // Get object property
    SetProperty,  // Set object property
    GetSelf,      // Get 'self' reference
    Invoke,       // Optimized method call

    // Arrays
    BuildArray, // Build array from stack elements
    GetIndex,   // Get array/string element by index
    SetIndex,   // Set array element by index

    // Dictionaries
    BuildDict, // Build dictionary from stack key-value pairs

    // Namespaces and Enums
    BuildNamespace, // Build namespace from stack key-value pairs
    BuildEnum,      // Build enum from stack key-value pairs

    // Inheritance
    Inherit,  // Copy parent class methods to child
    GetSuper, // Get method from superclass

    // Imports
    Import,   // Import file into global scope
    ImportAs, // Import file as module with alias

    // Upvalues (closures)
    GetUpvalue,   // Get captured variable from closure
    SetUpvalue,   // Set captured variable in closure
    CloseUpvalue, // Close upvalue when variable goes out of scope

    // Exception handling
    TryStart, // Start try block, push exception handler (operand: catch jump offset)
    TryEnd,   // End try block successfully, pop exception handler
    Throw,    // Throw exception (value on stack)

    // Async/Await
    Await, // Await a Future value, block until resolved

    // Spread
    SpreadArray, // Mark value to be spread as arguments
}

impl OpCode {
    /// Get the number of bytes this opcode reads as operands
    pub fn operand_count(&self) -> usize {
        match self {
            OpCode::Constant
            | OpCode::DefineGlobal
            | OpCode::GetGlobal
            | OpCode::SetGlobal
            | OpCode::GetLocal
            | OpCode::SetLocal
            | OpCode::Jump
            | OpCode::JumpIfFalse
            | OpCode::JumpIfTrue
            | OpCode::JumpIfNotNull
            | OpCode::Loop
            | OpCode::Call
            | OpCode::Closure
            | OpCode::Class
            | OpCode::Method
            | OpCode::StaticMethod
            | OpCode::GetProperty
            | OpCode::SetProperty
            | OpCode::Invoke
            | OpCode::BuildArray
            | OpCode::BuildDict
            | OpCode::BuildNamespace
            | OpCode::BuildEnum
            | OpCode::Inherit
            | OpCode::GetSuper
            | OpCode::Import
            | OpCode::ImportAs
            | OpCode::GetUpvalue
            | OpCode::SetUpvalue
            | OpCode::TryStart => 2, // u16 operand

            _ => 0,
        }
    }
}

impl From<u8> for OpCode {
    fn from(byte: u8) -> Self {
        unsafe { std::mem::transmute(byte) }
    }
}

impl From<OpCode> for u8 {
    fn from(op: OpCode) -> Self {
        op as u8
    }
}
