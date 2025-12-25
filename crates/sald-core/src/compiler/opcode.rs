


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    
    Constant, 
    Pop,      
    Dup,      
    DupTwo,   
    Swap,     

    
    Null,  
    True,  
    False, 

    
    DefineGlobal, 
    GetGlobal,    
    SetGlobal,    
    GetLocal,     
    SetLocal,     

    
    Add,    
    Sub,    
    Mul,    
    Div,    
    Mod,    
    Negate, 

    
    Equal,        
    NotEqual,     
    Less,         
    LessEqual,    
    Greater,      
    GreaterEqual, 

    
    Not, 

    
    Jump,          
    JumpIfFalse,   
    JumpIfTrue,    
    JumpIfNotNull, 
    Loop,          

    
    Call,    
    Return,  
    Closure, 

    
    Class,        
    Method,       
    StaticMethod, 
    GetProperty,  
    SetProperty,  
    GetSelf,      
    Invoke,       

    
    BuildArray, 
    GetIndex,   
    SetIndex,   

    
    BuildDict, 

    
    BuildNamespace, 
    BuildEnum,      

    
    Inherit,  
    GetSuper, 

    
    Import,   
    ImportAs, 

    
    GetUpvalue,   
    SetUpvalue,   
    CloseUpvalue, 

    
    TryStart, 
    TryEnd,   
    Throw,    

    
    Await, 

    
    SpreadArray, 

    
    BitAnd,     
    BitOr,      
    BitXor,     
    BitNot,     
    LeftShift,  
    RightShift, 

    
    BuildRangeInclusive, 
    BuildRangeExclusive, 

    
    RecursiveCall, 
}

impl OpCode {
    
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
            | OpCode::TryStart
            | OpCode::RecursiveCall => 2, 

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
