// Sald Compiler
// Compiles AST to bytecode

use super::chunk::{Chunk, Constant, FunctionConstant, UpvalueInfo};
use super::opcode::OpCode;
use crate::ast::*;
use crate::error::{SaldError, SaldResult, Span};

/// Result type for constant folding at compile time
#[derive(Debug, Clone)]
enum FoldedValue {
    Number(f64),
    Boolean(bool),
    String(String),
}

/// Local variable information
#[derive(Debug, Clone)]
struct Local {
    name: String,
    depth: usize,
    initialized: bool,
    is_captured: bool, // true if captured by a closure
}

/// Upvalue reference during compilation
#[derive(Debug, Clone)]
struct Upvalue {
    index: usize,   // Index in enclosing function's locals or upvalues
    is_local: bool, // true = capturing a local, false = capturing an upvalue
}

/// Compiler state for function scope
struct FunctionScope {
    chunk: Chunk,
    locals: Vec<Local>,
    upvalues: Vec<Upvalue>, // Upvalues captured by this function
    scope_depth: usize,
    /// Stack of loop start positions for continue statements
    loop_starts: Vec<usize>,
    /// Stack of break jump placeholders to patch when loop exits
    break_jumps: Vec<Vec<usize>>,
    /// Stack of scope depths at each loop entry (for proper break/continue cleanup)
    loop_scope_depths: Vec<usize>,
}

impl FunctionScope {
    fn new(is_method: bool) -> Self {
        let mut scope = Self {
            chunk: Chunk::new(),
            locals: Vec::new(),
            upvalues: Vec::new(),
            scope_depth: 0,
            loop_starts: Vec::new(),
            break_jumps: Vec::new(),
            loop_scope_depths: Vec::new(),
        };

        // Reserve slot 0 for 'self' in methods or empty in functions
        if is_method {
            scope.locals.push(Local {
                name: "self".to_string(),
                depth: 0,
                initialized: true,
                is_captured: false,
            });
        } else {
            scope.locals.push(Local {
                name: String::new(),
                depth: 0,
                initialized: true,
                is_captured: false,
            });
        }

        scope
    }
}

/// The Sald bytecode compiler
pub struct Compiler {
    scopes: Vec<FunctionScope>,
    file: String,
    source: String,
    had_error: bool,
    class_depth: usize,
}

impl Compiler {
    pub fn new(file: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            scopes: vec![FunctionScope::new(false)],
            file: file.into(),
            source: source.into(),
            had_error: false,
            class_depth: 0,
        }
    }

    /// Compile a program to bytecode
    pub fn compile(&mut self, program: &Program) -> SaldResult<Chunk> {
        for stmt in &program.statements {
            self.compile_stmt(stmt)?;
        }

        // End with return null
        self.emit_op(OpCode::Null, Span::default());
        self.emit_op(OpCode::Return, Span::default());

        if self.had_error {
            Err(SaldError::syntax_error(
                "Compilation failed due to previous errors",
                Span::default(),
                &self.file,
            ))
        } else {
            Ok(self.current_scope().chunk.clone())
        }
    }

    /// Compile for REPL mode - keeps last expression value on stack
    pub fn compile_repl(&mut self, program: &Program) -> SaldResult<Chunk> {
        let stmts = &program.statements;
        
        // Compile all but the last statement normally
        for (i, stmt) in stmts.iter().enumerate() {
            let is_last = i == stmts.len() - 1;
            
            if is_last {
                // For the last statement, if it's an expression, don't pop the result
                match stmt {
                    Stmt::Expression { expr, .. } => {
                        self.compile_expr(expr)?;
                        // Don't emit Pop - keep result on stack
                    }
                    _ => {
                        self.compile_stmt(stmt)?;
                        // Push null as result for non-expression statements
                        self.emit_op(OpCode::Null, Span::default());
                    }
                }
            } else {
                self.compile_stmt(stmt)?;
            }
        }

        // If no statements, push null
        if stmts.is_empty() {
            self.emit_op(OpCode::Null, Span::default());
        }

        self.emit_op(OpCode::Return, Span::default());

        if self.had_error {
            Err(SaldError::syntax_error(
                "Compilation failed due to previous errors",
                Span::default(),
                &self.file,
            ))
        } else {
            Ok(self.current_scope().chunk.clone())
        }
    }

    fn current_scope(&self) -> &FunctionScope {
        self.scopes.last().unwrap()
    }

    fn current_scope_mut(&mut self) -> &mut FunctionScope {
        self.scopes.last_mut().unwrap()
    }

    fn current_chunk(&mut self) -> &mut Chunk {
        &mut self.current_scope_mut().chunk
    }

    // ==================== Statements ====================

    fn compile_stmt(&mut self, stmt: &Stmt) -> SaldResult<()> {
        match stmt {
            Stmt::Let {
                name,
                initializer,
                span,
            } => {
                self.compile_let(name, initializer.as_ref(), *span)?;
            }
            Stmt::LetDestructure {
                pattern,
                initializer,
                span,
            } => {
                self.compile_let_destructure(pattern, initializer, *span)?;
            }
            Stmt::Expression { expr, .. } => {
                self.compile_expr(expr)?;
                self.emit_op(OpCode::Pop, expr.span());
            }
            Stmt::Block { statements, .. } => {
                self.begin_scope();
                for s in statements {
                    self.compile_stmt(s)?;
                }
                self.end_scope();
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.compile_if(condition, then_branch, else_branch.as_deref(), *span)?;
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                self.compile_while(condition, body, *span)?;
            }
            Stmt::DoWhile {
                body,
                condition,
                span,
            } => {
                self.compile_do_while(body, condition, *span)?;
            }
            Stmt::Function { def } => {
                self.compile_function(def, false)?;
            }
            Stmt::Return { value, span } => {
                self.compile_return(value.as_ref(), *span)?;
            }
            Stmt::Class { def } => {
                self.compile_class(def)?;
            }
            Stmt::For {
                variable,
                iterable,
                body,
                span,
            } => {
                self.compile_for(variable, iterable, body, *span)?;
            }
            Stmt::Break { span } => {
                self.compile_break(*span)?;
            }
            Stmt::Continue { span } => {
                self.compile_continue(*span)?;
            }
            Stmt::Import { path, alias, span } => {
                self.compile_import(path, alias.as_deref(), *span)?;
            }
            Stmt::TryCatch {
                try_body,
                catch_var,
                catch_body,
                span,
            } => {
                self.compile_try_catch(try_body, catch_var, catch_body, *span)?;
            }
            Stmt::Throw { value, span } => {
                self.compile_throw(value, *span)?;
            }
            Stmt::Const { name, value, span } => {
                self.compile_const(name, value, *span)?;
            }
            Stmt::Namespace { name, body, span } => {
                self.compile_namespace(name, body, *span)?;
            }
            Stmt::Enum { name, variants, span } => {
                self.compile_enum(name, variants, *span)?;
            }
        }
        Ok(())
    }

    fn compile_let(
        &mut self,
        name: &str,
        initializer: Option<&Expr>,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Handle 'self.property' assignment in methods
        if name.starts_with("self.") {
            let property = &name[5..]; // Remove "self." prefix
            self.emit_op(OpCode::GetSelf, span);

            if let Some(init) = initializer {
                self.compile_expr(init)?;
            } else {
                self.emit_op(OpCode::Null, span);
            }

            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(property.to_string()));
            self.emit_op(OpCode::SetProperty, span);
            self.emit_u16(const_idx as u16, span);
            self.emit_op(OpCode::Pop, span);
            return Ok(());
        }

        if let Some(init) = initializer {
            self.compile_expr(init)?;
        } else {
            self.emit_op(OpCode::Null, span);
        }

        if self.current_scope().scope_depth > 0 {
            // Local variable
            self.declare_local(name, span)?;
            self.mark_initialized();
        } else {
            // Global variable
            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(name.to_string()));
            self.emit_op(OpCode::DefineGlobal, span);
            self.emit_u16(const_idx as u16, span);
        }

        Ok(())
    }

    fn compile_let_destructure(
        &mut self,
        pattern: &crate::ast::ArrayPattern,
        initializer: &Expr,
        span: Span,
    ) -> SaldResult<()> {
        use crate::ast::ArrayPatternElement;

        // Compile the initializer (should be an array)
        self.compile_expr(initializer)?;

        // Extract elements from array
        for (i, elem) in pattern.elements.iter().enumerate() {
            match elem {
                ArrayPatternElement::Variable { name, span: var_span } => {
                    // Duplicate the array for each extraction
                    self.emit_op(OpCode::Dup, span);
                    // Push the index
                    let idx_const = self.current_chunk().add_constant(Constant::Number(i as f64));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(idx_const as u16, span);
                    // Get element at index
                    self.emit_op(OpCode::GetIndex, span);

                    // Declare as local variable
                    if self.current_scope().scope_depth > 0 {
                        self.declare_local(name, *var_span)?;
                        self.mark_initialized();
                    } else {
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(name.to_string()));
                        self.emit_op(OpCode::DefineGlobal, span);
                        self.emit_u16(const_idx as u16, span);
                    }
                }
                ArrayPatternElement::Rest { name, span: var_span } => {
                    // For rest: slice from current index to end
                    // Duplicate array
                    self.emit_op(OpCode::Dup, span);
                    // Push start index
                    let start_const = self.current_chunk().add_constant(Constant::Number(i as f64));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(start_const as u16, span);
                    // Call slice method - need to invoke the slice builtin
                    // For simplicity, we'll use GetIndex with a special marker
                    // Actually, let's just call the slice method directly
                    let slice_name = self.current_chunk().add_constant(Constant::String("slice".to_string()));
                    self.emit_op(OpCode::Invoke, span);
                    self.emit_u16(slice_name as u16, span);
                    self.emit_u16(1, span); // 1 arg (start index only, to end)

                    // Declare as local variable
                    if self.current_scope().scope_depth > 0 {
                        self.declare_local(name, *var_span)?;
                        self.mark_initialized();
                    } else {
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(name.to_string()));
                        self.emit_op(OpCode::DefineGlobal, span);
                        self.emit_u16(const_idx as u16, span);
                    }
                }
                ArrayPatternElement::Hole => {
                    // Skip this element, don't bind to anything
                }
            }
        }

        // Pop the original array from stack
        self.emit_op(OpCode::Pop, span);

        Ok(())
    }

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        self.compile_expr(condition)?;

        let then_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span); // Pop condition

        self.compile_stmt(then_branch)?;

        let else_jump = self.emit_jump(OpCode::Jump, span);

        self.patch_jump(then_jump);
        self.emit_op(OpCode::Pop, span); // Pop condition

        if let Some(else_stmt) = else_branch {
            self.compile_stmt(else_stmt)?;
        }

        self.patch_jump(else_jump);

        Ok(())
    }

    fn compile_while(&mut self, condition: &Expr, body: &Stmt, span: Span) -> SaldResult<()> {
        // Use span directly
        let loop_start = self.current_chunk().current_offset();

        // Set up loop tracking for break/continue
        // Track scope depth at loop entry for proper break/continue cleanup
        let entry_scope_depth = self.current_scope().scope_depth;
        self.current_scope_mut().loop_starts.push(loop_start);
        self.current_scope_mut().break_jumps.push(Vec::new());
        self.current_scope_mut()
            .loop_scope_depths
            .push(entry_scope_depth);

        self.compile_expr(condition)?;

        let exit_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        self.compile_stmt(body)?;

        self.emit_loop(loop_start, span);

        self.patch_jump(exit_jump);
        self.emit_op(OpCode::Pop, span);

        // Patch all break jumps to exit point
        self.current_scope_mut().loop_starts.pop();
        self.current_scope_mut().loop_scope_depths.pop();
        let break_jumps = self
            .current_scope_mut()
            .break_jumps
            .pop()
            .unwrap_or_default();
        for break_jump in break_jumps {
            self.patch_jump(break_jump);
        }

        Ok(())
    }

    fn compile_do_while(&mut self, body: &Stmt, condition: &Expr, span: Span) -> SaldResult<()> {
        // Use span directly
        let loop_start = self.current_chunk().current_offset();

        self.compile_stmt(body)?;

        self.compile_expr(condition)?;

        let exit_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        self.emit_loop(loop_start, span);

        self.patch_jump(exit_jump);
        self.emit_op(OpCode::Pop, span);

        Ok(())
    }

    /// Compile a for-in loop by desugaring to while loop:
    /// ```text
    /// for item in arr { body }
    /// ```
    /// becomes:
    /// ```text
    /// {
    ///     let __iter = arr
    ///     let __idx = 0
    ///     while __idx < __iter.length() {
    ///         let item = __iter[__idx]
    ///         body
    ///         __idx = __idx + 1
    ///     }
    /// }
    /// ```
    fn compile_for(
        &mut self,
        variable: &str,
        iterable: &Expr,
        body: &Stmt,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Begin scope for loop internals
        self.begin_scope();

        // let __iter = iterable
        self.compile_expr(iterable)?;
        self.declare_local("__iter", span)?;
        self.mark_initialized();

        // let __idx = 0
        self.emit_op(OpCode::Constant, span);
        let zero_const = self.current_chunk().add_constant(Constant::Number(0.0));
        self.emit_u16(zero_const as u16, span);
        self.declare_local("__idx", span)?;
        self.mark_initialized();

        // Declare loop variable (initialized to null, will be set each iteration)
        self.emit_op(OpCode::Null, span);
        self.declare_local(variable, span)?;
        self.mark_initialized();

        // Loop start
        let loop_start = self.current_chunk().current_offset();

        // Set up loop tracking for break/continue
        // Track scope depth at loop entry for proper break/continue cleanup
        let entry_scope_depth = self.current_scope().scope_depth;
        self.current_scope_mut().loop_starts.push(loop_start);
        self.current_scope_mut().break_jumps.push(Vec::new());
        self.current_scope_mut()
            .loop_scope_depths
            .push(entry_scope_depth);

        // __idx < __iter.length()
        // Get __idx
        if let Some(idx_slot) = self.resolve_local("__idx") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);
        }

        // Get __iter.length()
        if let Some(iter_slot) = self.resolve_local("__iter") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(iter_slot as u16, span);

            // Call .length() via Invoke
            let length_const = self
                .current_chunk()
                .add_constant(Constant::String("length".to_string()));
            self.emit_op(OpCode::Invoke, span);
            self.emit_u16(length_const as u16, span);
            self.emit_u16(0, span); // 0 arguments
        }

        // __idx < length
        self.emit_op(OpCode::Less, span);

        // Exit if condition is false
        let exit_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        // Get __iter[__idx] and assign to the loop variable
        // Get __iter
        if let Some(iter_slot) = self.resolve_local("__iter") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(iter_slot as u16, span);
        }

        // Get __idx
        if let Some(idx_slot) = self.resolve_local("__idx") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);
        }

        // Get index value
        self.emit_op(OpCode::GetIndex, span);

        // Assign to the loop variable via SetLocal (it was declared earlier)
        if let Some(var_slot) = self.resolve_local(variable) {
            self.emit_op(OpCode::SetLocal, span);
            self.emit_u16(var_slot as u16, span);
            self.emit_op(OpCode::Pop, span);
        }

        // Compile body
        self.compile_stmt(body)?;

        // __idx = __idx + 1
        if let Some(idx_slot) = self.resolve_local("__idx") {
            // Get current __idx
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);

            // Add 1
            let one_const = self.current_chunk().add_constant(Constant::Number(1.0));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(one_const as u16, span);
            self.emit_op(OpCode::Add, span);

            // Set __idx
            self.emit_op(OpCode::SetLocal, span);
            self.emit_u16(idx_slot as u16, span);
            self.emit_op(OpCode::Pop, span);
        }

        // Loop back
        self.emit_loop(loop_start, span);

        // Exit point
        self.patch_jump(exit_jump);
        self.emit_op(OpCode::Pop, span);

        // Patch all break jumps to exit point
        self.current_scope_mut().loop_starts.pop();
        self.current_scope_mut().loop_scope_depths.pop();
        let break_jumps = self
            .current_scope_mut()
            .break_jumps
            .pop()
            .unwrap_or_default();
        for break_jump in break_jumps {
            self.patch_jump(break_jump);
        }

        self.end_scope();

        Ok(())
    }

    fn compile_function(&mut self, def: &FunctionDef, as_method: bool) -> SaldResult<()> {
        let func_span = def.span;

        // Create new function scope
        self.scopes.push(FunctionScope::new(as_method));
        self.begin_scope();

        // Add parameters as locals
        // For methods, skip 'self' since FunctionScope::new already added it at slot 0
        for (i, param) in def.params.iter().enumerate() {
            if as_method && !def.is_static && i == 0 && param.name == "self" {
                continue;
            }
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        // Generate null-check code for parameters with default values
        // For each param with default: if local is null, evaluate default and set local
        for param in def.params.iter() {
            if let Some(ref default_expr) = param.default_value {
                // Get the actual local slot for this parameter
                let local_slot = self.resolve_local(&param.name)
                    .expect("Parameter should be defined as local");
                
                // Get current local value
                self.emit_op(OpCode::GetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);
                
                // Check if null: if NOT null, skip default assignment
                let skip_jump = self.emit_jump(OpCode::JumpIfNotNull, param.span);
                
                // Pop the null value
                self.emit_op(OpCode::Pop, param.span);
                
                // Evaluate default expression
                self.compile_expr(default_expr)?;
                
                // Set local to the default value
                self.emit_op(OpCode::SetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);
                
                // Pop the result of SetLocal (it leaves value on stack)
                self.emit_op(OpCode::Pop, param.span);
                
                // Jump over the "not null" pop
                let after_default = self.emit_jump(OpCode::Jump, param.span);
                
                // Jump target when not null - just pop the checked value
                self.patch_jump(skip_jump);
                self.emit_op(OpCode::Pop, param.span);
                
                self.patch_jump(after_default);
            }
        }

        // Compile function body
        for stmt in &def.body {
            self.compile_stmt(stmt)?;
        }

        // Implicit return null
        self.emit_op(OpCode::Null, func_span);
        self.emit_op(OpCode::Return, func_span);

        // Pop function scope
        let func_scope = self.scopes.pop().unwrap();

        // Calculate arity - for methods, exclude 'self' from the count
        // since the VM implicitly provides the instance as the first slot
        let arity = if as_method && !def.is_static {
            // Check if first param is 'self' and exclude it from arity
            if def.params.first().map(|p| p.name.as_str()) == Some("self") {
                def.params.len().saturating_sub(1)
            } else {
                def.params.len()
            }
        } else {
            def.params.len()
        };

        // Check if function has variadic parameter
        let is_variadic = def.params.last().map(|p| p.is_variadic).unwrap_or(false);

        // Create function constant with upvalue info
        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();

        let func_const = Constant::Function(FunctionConstant {
            name: def.name.clone(),
            arity,
            is_variadic,
            is_async: def.is_async,
            upvalue_count: upvalues.len(),
            upvalues,
            chunk: func_scope.chunk,
            file: self.file.clone(),
            param_names: def.params.iter().map(|p| p.name.clone()).collect(),
            default_count: def.params.iter().filter(|p| p.default_value.is_some()).count(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);

        if as_method {
            // Emit method/static method instruction
            if def.is_static {
                self.emit_op(OpCode::StaticMethod, func_span);
            } else {
                self.emit_op(OpCode::Method, func_span);
            }
            self.emit_u16(const_idx as u16, func_span);
        } else {
            // Emit closure instruction and define global
            self.emit_op(OpCode::Closure, func_span);
            self.emit_u16(const_idx as u16, func_span);

            if self.current_scope().scope_depth == 0 {
                let name_const = self
                    .current_chunk()
                    .add_constant(Constant::String(def.name.clone()));
                self.emit_op(OpCode::DefineGlobal, func_span);
                self.emit_u16(name_const as u16, func_span);
            }
        }

        Ok(())
    }

    fn compile_return(&mut self, value: Option<&Expr>, span: Span) -> SaldResult<()> {
        // Use span directly

        if let Some(expr) = value {
            self.compile_expr(expr)?;
        } else {
            self.emit_op(OpCode::Null, span);
        }

        self.emit_op(OpCode::Return, span);
        Ok(())
    }

    fn compile_class(&mut self, def: &ClassDef) -> SaldResult<()> {
        let class_span = def.span;
        self.class_depth += 1;

        // Emit class instruction
        let name_const = self
            .current_chunk()
            .add_constant(Constant::String(def.name.clone()));
        self.emit_op(OpCode::Class, class_span);
        self.emit_u16(name_const as u16, class_span);

        // Keep class on stack for building (don't define global yet)

        // Handle inheritance first
        if let Some(superclass) = &def.superclass {
            // Get superclass
            let super_const = self
                .current_chunk()
                .add_constant(Constant::String(superclass.clone()));
            self.emit_op(OpCode::GetGlobal, class_span);
            self.emit_u16(super_const as u16, class_span);

            // Inherit (pops superclass, pops subclass, pushes merged class)
            self.emit_op(OpCode::Inherit, class_span);
            self.emit_u16(0, class_span);
        }

        // Compile methods (adds to class on stack)
        for method in &def.methods {
            self.compile_function(method, true)?;
        }

        // Now define/update global with the fully constructed class
        let name_const2 = self
            .current_chunk()
            .add_constant(Constant::String(def.name.clone()));
        self.emit_op(OpCode::DefineGlobal, class_span);
        self.emit_u16(name_const2 as u16, class_span);

        self.class_depth -= 1;

        Ok(())
    }

    fn compile_try_catch(
        &mut self,
        try_body: &Stmt,
        catch_var: &str,
        catch_body: &Stmt,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Emit TryStart with placeholder for catch jump offset
        self.emit_op(OpCode::TryStart, span);
        let catch_jump = self.current_chunk().current_offset();
        self.emit_u16(0, span); // Placeholder

        // Compile try body
        self.compile_stmt(try_body)?;

        // Try block succeeded, pop exception handler
        self.emit_op(OpCode::TryEnd, span);

        // Jump over catch block
        let end_jump = self.emit_jump(OpCode::Jump, span);

        // Patch catch jump to here
        self.current_chunk().patch_jump(catch_jump);

        // Catch block: exception value is on stack
        // Create scope and bind catch variable
        self.begin_scope();

        // Add catch variable as local (exception is on stack)
        let depth = self.current_scope().scope_depth;
        self.current_scope_mut().locals.push(Local {
            name: catch_var.to_string(),
            depth,
            initialized: true,
            is_captured: false,
        });

        // Compile catch body
        self.compile_stmt(catch_body)?;

        self.end_scope();

        // Patch end jump
        self.patch_jump(end_jump);

        Ok(())
    }

    fn compile_throw(&mut self, value: &Expr, span: Span) -> SaldResult<()> {
        // Use span directly

        // Compile the value to throw
        self.compile_expr(value)?;

        // Emit throw opcode
        self.emit_op(OpCode::Throw, span);

        Ok(())
    }

    /// Compile const declaration: const NAME = value
    /// Const is compiled the same as a global variable
    fn compile_const(&mut self, name: &str, value: &Expr, span: Span) -> SaldResult<()> {
        self.compile_expr(value)?;

        // Const is always global (for export/namespace purposes)
        let const_idx = self
            .current_chunk()
            .add_constant(Constant::String(name.to_string()));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    /// Compile namespace: namespace Name { body }
    /// Uses IIFE (Immediately Invoked Function Expression) pattern so functions can capture 
    /// namespace variables as upvalues.
    /// 
    /// Conceptually compiles:
    ///   namespace Foo { let x = 1; fun bar() { return x; } }
    /// to:
    ///   Foo = (() => { let x = 1; return { x: x, bar: fun() { return x; } } })()
    fn compile_namespace(&mut self, name: &str, body: &[Stmt], span: Span) -> SaldResult<()> {
        // Create an anonymous function that will:
        // 1. Declare namespace variables as locals
        // 2. Compile nested functions (which can capture locals as upvalues)
        // 3. Return a dict containing all namespace members
        
        let func_name = format!("<namespace {}>", name);
        
        // Push new function scope
        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();
        
        // Track namespace variable names for building the return dict
        let mut namespace_vars: Vec<(String, Span)> = Vec::new();
        
        // First pass: compile let/const as locals
        for stmt in body {
            match stmt {
                Stmt::Let { name: var_name, initializer, span: var_span } => {
                    if let Some(init) = initializer {
                        self.compile_expr(init)?;
                    } else {
                        self.emit_op(OpCode::Null, *var_span);
                    }
                    self.declare_local(var_name, *var_span)?;
                    self.mark_initialized();
                    namespace_vars.push((var_name.clone(), *var_span));
                }
                Stmt::Const { name: const_name, value, span: const_span } => {
                    self.compile_expr(value)?;
                    self.declare_local(const_name, *const_span)?;
                    self.mark_initialized();
                    namespace_vars.push((const_name.clone(), *const_span));
                }
                _ => {}
            }
        }
        
        // Now build the namespace dict to return
        let mut member_count = 0;
        
        // Add let/const variables (get from locals)
        for (var_name, var_span) in &namespace_vars {
            // Push key
            let key_idx = self.current_chunk().add_constant(Constant::String(var_name.clone()));
            self.emit_op(OpCode::Constant, *var_span);
            self.emit_u16(key_idx as u16, *var_span);
            // Get value from local
            if let Some(slot) = self.resolve_local(var_name) {
                self.emit_op(OpCode::GetLocal, *var_span);
                self.emit_u16(slot as u16, *var_span);
            }
            member_count += 1;
        }

        // Second pass: compile functions, classes, nested namespaces, enums
        for stmt in body {
            match stmt {
                Stmt::Function { def } => {
                    // Push key
                    let key_idx = self.current_chunk().add_constant(Constant::String(def.name.clone()));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    // Compile function as closure (can capture namespace locals as upvalues)
                    self.compile_namespace_function(def)?;
                    member_count += 1;
                }
                Stmt::Class { def } => {
                    // Push key
                    let key_idx = self.current_chunk().add_constant(Constant::String(def.name.clone()));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    // Compile class and leave on stack
                    self.compile_namespace_class(def)?;
                    member_count += 1;
                }
                Stmt::Namespace { name: ns_name, body: ns_body, span: ns_span } => {
                    // Push key
                    let key_idx = self.current_chunk().add_constant(Constant::String(ns_name.clone()));
                    self.emit_op(OpCode::Constant, *ns_span);
                    self.emit_u16(key_idx as u16, *ns_span);
                    // Recursively compile nested namespace (builds dict, leaves on stack)
                    self.compile_namespace_inner(ns_name, ns_body, *ns_span)?;
                    member_count += 1;
                }
                Stmt::Enum { name: enum_name, variants, span: enum_span } => {
                    // Push key
                    let key_idx = self.current_chunk().add_constant(Constant::String(enum_name.clone()));
                    self.emit_op(OpCode::Constant, *enum_span);
                    self.emit_u16(key_idx as u16, *enum_span);
                    // Compile enum as dict, leave on stack
                    self.compile_enum_inner(enum_name, variants, *enum_span)?;
                    member_count += 1;
                }
                // Skip let/const - already handled above
                Stmt::Let { .. } | Stmt::Const { .. } => {}
                _ => {}
            }
        }
        
        // Build namespace dict
        self.emit_op(OpCode::BuildNamespace, span);
        self.emit_u16(member_count as u16, span);
        
        // Return the namespace dict
        self.emit_op(OpCode::Return, span);
        
        // Pop the function scope
        let func_scope = self.scopes.pop().unwrap();
        
        // Create upvalue info
        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();
        
        // Create the function constant
        let func_const = Constant::Function(FunctionConstant {
            name: func_name,
            arity: 0,
            is_variadic: false,
            is_async: false,
            upvalue_count: upvalues.len(),
            upvalues,
            chunk: func_scope.chunk,
            file: self.file.clone(),
            param_names: Vec::new(),
            default_count: 0,
        });
        
        // Emit closure and immediately call it (IIFE pattern)
        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);
        
        // Call with 0 arguments
        self.emit_op(OpCode::Call, span);
        self.emit_u16(0, span);
        
        // Define the result as global
        let name_idx = self.current_chunk().add_constant(Constant::String(name.to_string()));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(name_idx as u16, span);

        Ok(())
    }

    /// Compile namespace inner (for nested namespaces) - uses IIFE, leaves result on stack
    fn compile_namespace_inner(&mut self, name: &str, body: &[Stmt], span: Span) -> SaldResult<()> {
        // Same IIFE pattern as compile_namespace, but doesn't define as global
        let func_name = format!("<namespace {}>", name);
        
        // Push new function scope
        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();
        
        // Track namespace variable names for building the return dict
        let mut namespace_vars: Vec<(String, Span)> = Vec::new();
        
        // First pass: compile let/const as locals
        for stmt in body {
            match stmt {
                Stmt::Let { name: var_name, initializer, span: var_span } => {
                    if let Some(init) = initializer {
                        self.compile_expr(init)?;
                    } else {
                        self.emit_op(OpCode::Null, *var_span);
                    }
                    self.declare_local(var_name, *var_span)?;
                    self.mark_initialized();
                    namespace_vars.push((var_name.clone(), *var_span));
                }
                Stmt::Const { name: const_name, value, span: const_span } => {
                    self.compile_expr(value)?;
                    self.declare_local(const_name, *const_span)?;
                    self.mark_initialized();
                    namespace_vars.push((const_name.clone(), *const_span));
                }
                _ => {}
            }
        }
        
        // Now build the namespace dict to return
        let mut member_count = 0;
        
        // Add let/const variables (get from locals)
        for (var_name, var_span) in &namespace_vars {
            let key_idx = self.current_chunk().add_constant(Constant::String(var_name.clone()));
            self.emit_op(OpCode::Constant, *var_span);
            self.emit_u16(key_idx as u16, *var_span);
            if let Some(slot) = self.resolve_local(var_name) {
                self.emit_op(OpCode::GetLocal, *var_span);
                self.emit_u16(slot as u16, *var_span);
            }
            member_count += 1;
        }

        // Second pass: compile functions, classes, nested namespaces, enums
        for stmt in body {
            match stmt {
                Stmt::Function { def } => {
                    let key_idx = self.current_chunk().add_constant(Constant::String(def.name.clone()));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    self.compile_namespace_function(def)?;
                    member_count += 1;
                }
                Stmt::Class { def } => {
                    let key_idx = self.current_chunk().add_constant(Constant::String(def.name.clone()));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    self.compile_namespace_class(def)?;
                    member_count += 1;
                }
                Stmt::Namespace { name: ns_name, body: ns_body, span: ns_span } => {
                    let key_idx = self.current_chunk().add_constant(Constant::String(ns_name.clone()));
                    self.emit_op(OpCode::Constant, *ns_span);
                    self.emit_u16(key_idx as u16, *ns_span);
                    self.compile_namespace_inner(ns_name, ns_body, *ns_span)?;
                    member_count += 1;
                }
                Stmt::Enum { name: enum_name, variants, span: enum_span } => {
                    let key_idx = self.current_chunk().add_constant(Constant::String(enum_name.clone()));
                    self.emit_op(OpCode::Constant, *enum_span);
                    self.emit_u16(key_idx as u16, *enum_span);
                    self.compile_enum_inner(enum_name, variants, *enum_span)?;
                    member_count += 1;
                }
                // Skip let/const - already handled above
                Stmt::Let { .. } | Stmt::Const { .. } => {}
                _ => {}
            }
        }
        
        // Build namespace dict
        self.emit_op(OpCode::BuildNamespace, span);
        self.emit_u16(member_count as u16, span);
        
        // Return the namespace dict
        self.emit_op(OpCode::Return, span);
        
        // Pop the function scope
        let func_scope = self.scopes.pop().unwrap();
        
        // Create upvalue info
        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();
        
        // Create the function constant
        let func_const = Constant::Function(FunctionConstant {
            name: func_name,
            arity: 0,
            is_variadic: false,
            is_async: false,
            upvalue_count: upvalues.len(),
            upvalues,
            chunk: func_scope.chunk,
            file: self.file.clone(),
            param_names: Vec::new(),
            default_count: 0,
        });
        
        // Emit closure and immediately call it (IIFE pattern)
        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);
        
        // Call with 0 arguments - result stays on stack for parent namespace
        self.emit_op(OpCode::Call, span);
        self.emit_u16(0, span);

        Ok(())
    }

    /// Compile function for namespace (leaves closure on stack, doesn't define global)
    fn compile_namespace_function(&mut self, def: &FunctionDef) -> SaldResult<()> {
        let func_span = def.span;

        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        for param in &def.params {
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        // Generate null-check code for parameters with default values
        // For each param with default: if local is null, evaluate default and set local
        for param in def.params.iter() {
            if let Some(ref default_expr) = param.default_value {
                // Get the actual local slot for this parameter
                let local_slot = self.resolve_local(&param.name)
                    .expect("Parameter should be defined as local");
                
                // Get current local value
                self.emit_op(OpCode::GetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);
                
                // Check if null: if NOT null, skip default assignment
                let skip_jump = self.emit_jump(OpCode::JumpIfNotNull, param.span);
                
                // Pop the null value
                self.emit_op(OpCode::Pop, param.span);
                
                // Evaluate default expression
                self.compile_expr(default_expr)?;
                
                // Set local to the default value
                self.emit_op(OpCode::SetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);
                
                // Pop the result of SetLocal (it leaves value on stack)
                self.emit_op(OpCode::Pop, param.span);
                
                // Jump over the "not null" pop
                let after_default = self.emit_jump(OpCode::Jump, param.span);
                
                // Jump target when not null - just pop the checked value
                self.patch_jump(skip_jump);
                self.emit_op(OpCode::Pop, param.span);
                
                self.patch_jump(after_default);
            }
        }

        for stmt in &def.body {
            self.compile_stmt(stmt)?;
        }

        self.emit_op(OpCode::Null, func_span);
        self.emit_op(OpCode::Return, func_span);

        let func_scope = self.scopes.pop().unwrap();
        let arity = def.params.len();
        let is_variadic = def.params.last().map(|p| p.is_variadic).unwrap_or(false);

        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();

        let func_const = Constant::Function(FunctionConstant {
            name: def.name.clone(),
            arity,
            is_variadic,
            is_async: def.is_async,
            upvalue_count: upvalues.len(),
            upvalues,
            chunk: func_scope.chunk,
            file: self.file.clone(),
            param_names: def.params.iter().map(|p| p.name.clone()).collect(),
            default_count: def.params.iter().filter(|p| p.default_value.is_some()).count(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, func_span);
        self.emit_u16(const_idx as u16, func_span);

        Ok(())
    }

    /// Compile class for namespace (leaves class on stack, doesn't define global)
    fn compile_namespace_class(&mut self, def: &ClassDef) -> SaldResult<()> {
        let class_span = def.span;
        self.class_depth += 1;

        let name_const = self.current_chunk().add_constant(Constant::String(def.name.clone()));
        self.emit_op(OpCode::Class, class_span);
        self.emit_u16(name_const as u16, class_span);

        if let Some(superclass) = &def.superclass {
            let super_const = self.current_chunk().add_constant(Constant::String(superclass.clone()));
            self.emit_op(OpCode::GetGlobal, class_span);
            self.emit_u16(super_const as u16, class_span);
            self.emit_op(OpCode::Inherit, class_span);
            self.emit_u16(0, class_span);
        }

        for method in &def.methods {
            self.compile_function(method, true)?;
        }

        self.class_depth -= 1;

        // Don't define as global - leave class on stack
        Ok(())
    }

    /// Compile enum: enum Name { Variant1, Variant2 }
    /// Creates a dictionary with string values like "EnumName.Variant"
    fn compile_enum(&mut self, name: &str, variants: &[String], span: Span) -> SaldResult<()> {
        self.compile_enum_inner(name, variants, span)?;

        // Define as global
        let name_idx = self.current_chunk().add_constant(Constant::String(name.to_string()));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(name_idx as u16, span);

        Ok(())
    }

    /// Compile enum inner (leaves dict on stack)
    fn compile_enum_inner(&mut self, name: &str, variants: &[String], span: Span) -> SaldResult<()> {
        for variant in variants {
            // Push key (variant name)
            let key_idx = self.current_chunk().add_constant(Constant::String(variant.clone()));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(key_idx as u16, span);

            // Push value ("EnumName.Variant")
            let value = format!("{}.{}", name, variant);
            let val_idx = self.current_chunk().add_constant(Constant::String(value));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(val_idx as u16, span);
        }

        // Build enum
        self.emit_op(OpCode::BuildEnum, span);
        self.emit_u16(variants.len() as u16, span);

        Ok(())
    }

    // ==================== Expressions ====================

    fn compile_expr(&mut self, expr: &Expr) -> SaldResult<()> {
        match expr {
            Expr::Literal { value, span } => {
                self.compile_literal(value, *span)?;
            }
            Expr::Identifier { name, span } => {
                self.compile_identifier(name, *span)?;
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                self.compile_binary(left, op, right, *span)?;
            }
            Expr::Unary { op, operand, span } => {
                self.compile_unary(op, operand, *span)?;
            }
            Expr::Grouping { expr, .. } => {
                self.compile_expr(expr)?;
            }
            Expr::Assignment {
                target,
                op,
                value,
                span,
            } => {
                self.compile_assignment(target, op, value, *span)?;
            }
            Expr::Call { callee, args, is_optional: _, span } => {
                // Check for method invocation optimization: obj.method(args) or obj?.method(args)
                if let Expr::Get {
                    object, property, is_optional, ..
                } = callee.as_ref()
                {
                    // Compile receiver first
                    self.compile_expr(object)?;
                    
                    if *is_optional {
                        // Optional method call: obj?.method(args)
                        // If obj is null, skip the method call and return null
                        self.emit_op(OpCode::Dup, *span);
                        let normal_jump = self.emit_jump(OpCode::JumpIfNotNull, *span);
                        // Null path: pop the dup'd null, skip invoke
                        self.emit_op(OpCode::Pop, *span);
                        let end_jump = self.emit_jump(OpCode::Jump, *span);
                        // Normal path: pop duplicate, proceed with invoke
                        self.patch_jump(normal_jump);
                        self.emit_op(OpCode::Pop, *span);
                        // Re-compile receiver for the actual call
                        self.compile_expr(object)?;
                        // Compile arguments
                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(property.clone()));
                        self.emit_op(OpCode::Invoke, *span);
                        self.emit_u16(const_idx as u16, *span);
                        self.emit_u16(args.len() as u16, *span);
                        self.patch_jump(end_jump);
                    } else {
                        // Non-optional method call
                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(property.clone()));
                        self.emit_op(OpCode::Invoke, *span);
                        self.emit_u16(const_idx as u16, *span);
                        self.emit_u16(args.len() as u16, *span);
                    }
                } else {
                    // Regular function call
                    self.compile_expr(callee)?;
                    for arg in args {
                        self.compile_expr(&arg.value)?;
                    }
                    self.emit_op(OpCode::Call, *span);
                    self.emit_u16(args.len() as u16, *span);
                }
            }
            Expr::Get {
                object,
                property,
                is_optional,
                span,
            } => {
                self.compile_expr(object)?;
                
                if *is_optional {
                    // Optional chaining: obj?.property
                    // Stack: [obj]
                    // 1. Dup the object to check for null
                    self.emit_op(OpCode::Dup, *span);
                    // Stack: [obj, obj]
                    // 2. Jump to normal path if not null (JumpIfNotNull peeks, doesn't pop)
                    let normal_jump = self.emit_jump(OpCode::JumpIfNotNull, *span);
                    // Stack if null: [null, null]
                    // 3. Object is null - pop both, push one null
                    self.emit_op(OpCode::Pop, *span);
                    // Stack: [null]
                    // 4. Skip over the property access
                    let end_jump = self.emit_jump(OpCode::Jump, *span);
                    // 5. Normal path - stack has [obj, obj], pop duplicate
                    self.patch_jump(normal_jump);
                    self.emit_op(OpCode::Pop, *span);
                    // Stack: [obj]
                    // 6. Do normal property access on obj already on stack
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(property.clone()));
                    self.emit_op(OpCode::GetProperty, *span);
                    self.emit_u16(const_idx as u16, *span);
                    // Stack: [prop_value]
                    // 7. End
                    self.patch_jump(end_jump);
                } else {
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(property.clone()));
                    self.emit_op(OpCode::GetProperty, *span);
                    self.emit_u16(const_idx as u16, *span);
                }
            }
            Expr::Set {
                object,
                property,
                value,
                span,
            } => {
                self.compile_set(object, property, value, *span)?;
            }
            Expr::SelfExpr { span } => {
                // First, try to resolve 'self' as a local (if we're in the method itself)
                if let Some(idx) = self.resolve_local("self") {
                    self.emit_op(OpCode::GetLocal, *span);
                    self.emit_u16(idx as u16, *span);
                } else if let Some(idx) = self.resolve_upvalue(self.scopes.len() - 1, "self") {
                    // 'self' is captured from an enclosing method - get it as upvalue
                    self.emit_op(OpCode::GetUpvalue, *span);
                    self.emit_u16(idx as u16, *span);
                } else {
                    // Fallback to GetSelf (shouldn't normally happen)
                    self.emit_op(OpCode::GetSelf, *span);
                }
            }
            Expr::Array { elements, span } => {
                // Compile each element
                for elem in elements {
                    self.compile_expr(elem)?;
                }
                // Emit array creation with element count
                // Use span directly
                let count = elements.len();
                self.emit_op(OpCode::BuildArray, *span);
                self.emit_u16(count as u16, *span);
            }
            Expr::Index {
                object,
                index,
                is_optional: _,
                span,
            } => {
                // Get index
                self.compile_expr(object)?;
                self.compile_expr(index)?;
                self.emit_op(OpCode::GetIndex, *span);
            }
            Expr::IndexSet {
                object,
                index,
                value,
                span,
            } => {
                // Set index
                self.compile_expr(object)?;
                self.compile_expr(index)?;
                self.compile_expr(value)?;
                self.emit_op(OpCode::SetIndex, *span);
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                span,
            } => {
                self.compile_ternary(condition, then_expr, else_expr, *span)?;
            }
            Expr::Lambda {
                params,
                body,
                is_async,
                span,
            } => {
                self.compile_lambda(params, body, *is_async, *span)?;
            }
            Expr::Super { method, span } => {
                self.compile_super(method, *span)?;
            }
            Expr::Switch {
                value,
                arms,
                default,
                span,
            } => {
                self.compile_switch(value, arms, default.as_deref(), *span)?;
            }
            Expr::Block {
                statements,
                expr,
                span,
            } => {
                // Use span directly

                // Compile all statements (they should handle their own pops)
                for stmt in statements {
                    self.compile_stmt(stmt)?;
                }

                // Compile the final expression if present, else push null
                if let Some(final_expr) = expr {
                    self.compile_expr(final_expr)?;
                } else {
                    self.emit_op(OpCode::Null, *span);
                }
            }
            Expr::Dictionary { entries, span } => {
                // Compile each key-value pair onto stack
                for (key, value) in entries {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                }
                // Emit BuildDict with count
                self.emit_op(OpCode::BuildDict, *span);
                self.emit_u16(entries.len() as u16, *span);
            }
            Expr::Await { expr, span } => {
                // Compile the expression to await
                self.compile_expr(expr)?;
                // Emit Await opcode
                self.emit_op(OpCode::Await, *span);
            }
            Expr::Return { value, span } => {
                // Compile return as expression (for switch arms, etc.)
                if let Some(v) = value {
                    self.compile_expr(v)?;
                } else {
                    self.emit_op(OpCode::Null, *span);
                }
                self.emit_op(OpCode::Return, *span);
            }
            Expr::Throw { value, span } => {
                // Compile throw as expression
                self.compile_expr(value)?;
                self.emit_op(OpCode::Throw, *span);
            }
            Expr::Break { span } => {
                // Compile break as expression
                self.compile_break(*span)?;
            }
            Expr::Continue { span } => {
                // Compile continue as expression
                self.compile_continue(*span)?;
            }
            Expr::Spread { expr, span } => {
                // Spread expression outside of a call context - just compile the expression
                // The actual spreading is handled in compile_call
                self.compile_expr(expr)?;
                // Mark that this needs to be spread at runtime
                self.emit_op(OpCode::SpreadArray, *span);
            }
        }
        Ok(())
    }

    fn compile_switch(
        &mut self,
        value: &Expr,
        arms: &[SwitchArm],
        default: Option<&Expr>,
        span: Span,
    ) -> SaldResult<()> {
        // First, compile the value - it stays on stack as value_slot's value
        self.begin_scope();
        self.compile_expr(value)?;
        // Value is now on top of stack - add_local_unnamed marks this position as a local
        let value_slot = self.add_local_unnamed();
        // DON'T SetLocal/Pop - the value is already at the correct stack position!

        // Track all end jumps (to jump to after each arm succeeds)
        let mut end_jumps = Vec::new();

        for arm in arms {
            // For single-pattern arms (most common), simple flow
            // For multi-pattern arms (1, 2, 3 -> ...), test each; first match wins
            
            if arm.patterns.len() == 1 {
                // Single pattern - test, then compile body
                let pattern = &arm.patterns[0];
                
                // Begin scope for this arm (covers both test and body)
                self.begin_scope();
                
                // Remember how many locals before pattern test (for failure cleanup)
                let locals_before_test = self.current_scope().locals.len();
                
                let match_result = self.compile_pattern_test(value_slot, pattern, span)?;
                
                if let Some(success_jump) = match_result {
                    // Conditional pattern - JumpIfTrue jumps on TRUE
                    // 
                    // IMPORTANT: At compile time, we emit code for BOTH paths sequentially.
                    // We must NOT modify compiler state (locals, scope_depth) on failure path
                    // because success path still needs that state!
                    
                    // Failure path: emit runtime cleanup, then jump away
                    self.emit_op(OpCode::Pop, span); // pop the false
                    
                    // Pop any locals added during pattern test at RUNTIME
                    // But DON'T remove them from compiler tracking - success path needs them
                    let locals_to_pop = self.current_scope().locals.len() - locals_before_test;
                    for _ in 0..locals_to_pop {
                        self.emit_op(OpCode::Pop, span);
                    }
                    
                    let next_arm_jump = self.emit_jump(OpCode::Jump, span);
                    
                    // Success path - locals are still in compiler's list
                    self.patch_jump(success_jump);
                    self.emit_op(OpCode::Pop, span); // pop the true
                    
                    // Bind and compile body
                    // compile_pattern_bindings skips if guard already bound the variable
                    self.compile_pattern_bindings(value_slot, pattern, span)?;
                    self.compile_expr(&arm.body)?;
                    
                    // end_scope_keep_result pops all locals including guard bindings
                    self.end_scope_keep_result();
                    end_jumps.push(self.emit_jump(OpCode::Jump, span));
                    
                    // Next arm starts here (after scope ended, so depth is correct)
                    self.patch_jump(next_arm_jump);
                } else {
                    // Unconditional match - always succeeds
                    self.compile_pattern_bindings(value_slot, pattern, span)?;
                    self.compile_expr(&arm.body)?;
                    self.end_scope_keep_result();
                    end_jumps.push(self.emit_jump(OpCode::Jump, span));
                    // Note: following arms are unreachable
                }
            } else {
                // Multi-pattern arm (e.g., 1, 2, 3 -> ...)
                // These are literals without bindings, no scope needed for test
                let mut success_jumps = Vec::new();
                
                for pattern in &arm.patterns {
                    let match_result = self.compile_pattern_test(value_slot, pattern, span)?;
                    if let Some(success_jump) = match_result {
                        success_jumps.push(success_jump);
                        self.emit_op(OpCode::Pop, span);
                    } else {
                        success_jumps.push(self.emit_jump(OpCode::Jump, span));
                    }
                }
                
                // All patterns failed
                let next_arm_jump = self.emit_jump(OpCode::Jump, span);
                
                // Patch successes to body
                for jump in success_jumps {
                    self.patch_jump(jump);
                }
                self.emit_op(OpCode::Pop, span);
                
                // Body in its own scope
                self.begin_scope();
                self.compile_expr(&arm.body)?;
                self.end_scope_keep_result();
                end_jumps.push(self.emit_jump(OpCode::Jump, span));
                
                self.patch_jump(next_arm_jump);
            }
        }

        // Compile default arm or push null
        if let Some(default_expr) = default {
            self.begin_scope();
            self.compile_expr(default_expr)?;
            self.end_scope_keep_result();
        } else {
            self.emit_op(OpCode::Null, span);
        }

        // Patch all end jumps
        for jump in end_jumps {
            self.patch_jump(jump);
        }

        // End outer scope (value slot)
        self.end_scope_keep_result();

        Ok(())
    }

    /// Test if a pattern matches (without binding variables)
    /// Returns Some(jump_offset) for conditional patterns (JumpIfTrue), None for unconditional
    fn compile_pattern_test(
        &mut self,
        value_slot: usize,
        pattern: &Pattern,
        span: Span,
    ) -> SaldResult<Option<usize>> {
        match pattern {
            Pattern::Literal { value, .. } => {
                // Push value, push literal, compare
                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);
                self.compile_literal(value, span)?;
                self.emit_op(OpCode::Equal, span);
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }
            
            Pattern::Binding { name, guard, .. } => {
                // If there's a guard, we need to bind the value so guard can reference it
                if let Some(guard_expr) = guard {
                    // Push the value onto stack FIRST
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    // NOW declare the local - the value is at the top of stack = this local's slot
                    self.declare_local(name, span)?;
                    self.mark_initialized();
                    
                    // Compile guard (now it can reference the binding by name via GetLocal)
                    self.compile_expr(guard_expr)?;
                    Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
                } else {
                    // Unconditional match - no guard, binding will be done in compile_pattern_bindings
                    Ok(None)
                }
            }
            
            Pattern::Array { elements, .. } => {
                // Check length matches
                let has_rest = elements.iter().any(|e| matches!(e, SwitchArrayElement::Rest { .. }));
                let non_rest_count = elements.iter().filter(|e| matches!(e, SwitchArrayElement::Single(_))).count();
                
                // Get array length
                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);
                let length_idx = self.current_chunk().add_constant(Constant::String("length".to_string()));
                self.emit_op(OpCode::Invoke, span);
                self.emit_u16(length_idx as u16, span);
                self.emit_u16(0, span);
                
                let expected_len = if has_rest { non_rest_count } else { elements.len() };
                let len_const = self.current_chunk().add_constant(Constant::Number(expected_len as f64));
                self.emit_op(OpCode::Constant, span);
                self.emit_u16(len_const as u16, span);
                
                if has_rest {
                    self.emit_op(OpCode::GreaterEqual, span);
                } else {
                    self.emit_op(OpCode::Equal, span);
                }
                
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }
            
            Pattern::Dict { entries, .. } => {
                // Check all keys exist
                if entries.is_empty() {
                    // Empty dict pattern - always matches dicts
                    self.emit_op(OpCode::True, span);
                } else {
                    // Check first key exists (simplified - full check would need more)
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    let key = &entries[0].0;
                    let key_const = self.current_chunk().add_constant(Constant::String(key.clone()));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(key_const as u16, span);
                    self.emit_op(OpCode::GetIndex, span);
                    self.emit_op(OpCode::Null, span);
                    self.emit_op(OpCode::NotEqual, span);
                }
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }
        }
    }

    /// Bind pattern variables (called after successful match)
    fn compile_pattern_bindings(
        &mut self,
        value_slot: usize,
        pattern: &Pattern,
        span: Span,
    ) -> SaldResult<()> {
        match pattern {
            Pattern::Literal { .. } => {
                // No bindings for literal patterns
                Ok(())
            }
            
            Pattern::Binding { name, guard, .. } => {
                // If pattern has a guard, the binding was already declared in compile_pattern_test
                // Only declare if there was no guard
                if guard.is_none() {
                    self.declare_local(name, span)?;
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    self.mark_initialized();
                }
                // If there was a guard, binding already exists and has the value
                Ok(())
            }
            
            Pattern::Array { elements, .. } => {
                let mut idx = 0;
                for element in elements {
                    match element {
                        SwitchArrayElement::Single(sub_pattern) => {
                            // Get arr[idx] and store temporarily
                            self.emit_op(OpCode::GetLocal, span);
                            self.emit_u16(value_slot as u16, span);
                            let idx_const = self.current_chunk().add_constant(Constant::Number(idx as f64));
                            self.emit_op(OpCode::Constant, span);
                            self.emit_u16(idx_const as u16, span);
                            self.emit_op(OpCode::GetIndex, span);
                            
                            // Value is now on top of stack - mark it as a local
                            let temp_slot = self.add_local_unnamed();
                            // DON'T SetLocal/Pop - value is already at correct stack position!
                            
                            self.compile_pattern_bindings(temp_slot, sub_pattern, span)?;
                            idx += 1;
                        }
                        SwitchArrayElement::Rest { name, .. } => {
                            // arr.slice(idx)
                            self.declare_local(name, span)?;
                            self.emit_op(OpCode::GetLocal, span);
                            self.emit_u16(value_slot as u16, span);
                            let idx_const = self.current_chunk().add_constant(Constant::Number(idx as f64));
                            self.emit_op(OpCode::Constant, span);
                            self.emit_u16(idx_const as u16, span);
                            let slice_idx = self.current_chunk().add_constant(Constant::String("slice".to_string()));
                            self.emit_op(OpCode::Invoke, span);
                            self.emit_u16(slice_idx as u16, span);
                            self.emit_u16(1, span);
                            self.mark_initialized();
                        }
                    }
                }
                Ok(())
            }
            
            Pattern::Dict { entries, .. } => {
                for (key, sub_pattern) in entries {
                    // Get dict[key]
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    let key_const = self.current_chunk().add_constant(Constant::String(key.clone()));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(key_const as u16, span);
                    self.emit_op(OpCode::GetIndex, span);
                    
                    // Value is now on top of stack - mark it as a local
                    let temp_slot = self.add_local_unnamed();
                    // DON'T SetLocal/Pop - value is already at correct stack position!
                    
                    self.compile_pattern_bindings(temp_slot, sub_pattern, span)?;
                }
                Ok(())
            }
        }
    }

    /// Helper to add an unnamed local for switch value
    fn add_local_unnamed(&mut self) -> usize {
        let scope = self.current_scope_mut();
        let slot = scope.locals.len();
        scope.locals.push(Local {
            name: String::new(),
            depth: scope.scope_depth,
            initialized: true,
            is_captured: false,
        });
        slot
    }


    fn compile_literal(&mut self, value: &Literal, span: Span) -> SaldResult<()> {
        // Use span directly
        match value {
            Literal::Number(n) => {
                let const_idx = self.current_chunk().add_constant(Constant::Number(*n));
                self.emit_op(OpCode::Constant, span);
                self.emit_u16(const_idx as u16, span);
            }
            Literal::String(s) => {
                let const_idx = self
                    .current_chunk()
                    .add_constant(Constant::String(s.clone()));
                self.emit_op(OpCode::Constant, span);
                self.emit_u16(const_idx as u16, span);
            }
            Literal::Boolean(b) => {
                self.emit_op(if *b { OpCode::True } else { OpCode::False }, span);
            }
            Literal::Null => {
                self.emit_op(OpCode::Null, span);
            }
        }
        Ok(())
    }

    fn compile_identifier(&mut self, name: &str, span: Span) -> SaldResult<()> {
        // Use span directly

        // Check for local variable first
        if let Some(slot) = self.resolve_local(name) {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(slot as u16, span);
        } else if let Some(upvalue) = self.resolve_upvalue(self.scopes.len() - 1, name) {
            // Check for upvalue (captured from enclosing scope)
            self.emit_op(OpCode::GetUpvalue, span);
            self.emit_u16(upvalue as u16, span);
        } else {
            // Global variable
            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(name.to_string()));
            self.emit_op(OpCode::GetGlobal, span);
            self.emit_u16(const_idx as u16, span);
        }

        Ok(())
    }

    fn compile_binary(
        &mut self,
        left: &Expr,
        op: &BinaryOp,
        right: &Expr,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Short-circuit for &&, ||, and ??
        match op {
            BinaryOp::And => {
                self.compile_expr(left)?;
                let end_jump = self.emit_jump(OpCode::JumpIfFalse, span);
                self.emit_op(OpCode::Pop, span);
                self.compile_expr(right)?;
                self.patch_jump(end_jump);
                return Ok(());
            }
            BinaryOp::Or => {
                self.compile_expr(left)?;
                let else_jump = self.emit_jump(OpCode::JumpIfFalse, span);
                let end_jump = self.emit_jump(OpCode::Jump, span);
                self.patch_jump(else_jump);
                self.emit_op(OpCode::Pop, span);
                self.compile_expr(right)?;
                self.patch_jump(end_jump);
                return Ok(());
            }
            BinaryOp::NullCoalesce => {
                // Short-circuit: if left is not null, use left; otherwise use right
                self.compile_expr(left)?;
                let end_jump = self.emit_jump(OpCode::JumpIfNotNull, span);
                self.emit_op(OpCode::Pop, span);
                self.compile_expr(right)?;
                self.patch_jump(end_jump);
                return Ok(());
            }
            _ => {}
        }

        // ===== CONSTANT FOLDING =====
        // Try to evaluate constant expressions at compile time
        if let Some(result) = self.try_fold_binary(left, op, right) {
            // Emit the folded constant
            match result {
                FoldedValue::Number(n) => {
                    let const_idx = self.current_chunk().add_constant(Constant::Number(n));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(const_idx as u16, span);
                }
                FoldedValue::Boolean(b) => {
                    self.emit_op(if b { OpCode::True } else { OpCode::False }, span);
                }
                FoldedValue::String(s) => {
                    let const_idx = self.current_chunk().add_constant(Constant::String(s));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(const_idx as u16, span);
                }
            }
            return Ok(());
        }

        self.compile_expr(left)?;
        self.compile_expr(right)?;

        match op {
            BinaryOp::Add => self.emit_op(OpCode::Add, span),
            BinaryOp::Sub => self.emit_op(OpCode::Sub, span),
            BinaryOp::Mul => self.emit_op(OpCode::Mul, span),
            BinaryOp::Div => self.emit_op(OpCode::Div, span),
            BinaryOp::Mod => self.emit_op(OpCode::Mod, span),
            BinaryOp::Equal => self.emit_op(OpCode::Equal, span),
            BinaryOp::NotEqual => self.emit_op(OpCode::NotEqual, span),
            BinaryOp::Less => self.emit_op(OpCode::Less, span),
            BinaryOp::LessEqual => self.emit_op(OpCode::LessEqual, span),
            BinaryOp::Greater => self.emit_op(OpCode::Greater, span),
            BinaryOp::GreaterEqual => self.emit_op(OpCode::GreaterEqual, span),
            // Bitwise operators
            BinaryOp::BitAnd => self.emit_op(OpCode::BitAnd, span),
            BinaryOp::BitOr => self.emit_op(OpCode::BitOr, span),
            BinaryOp::BitXor => self.emit_op(OpCode::BitXor, span),
            BinaryOp::LeftShift => self.emit_op(OpCode::LeftShift, span),
            BinaryOp::RightShift => self.emit_op(OpCode::RightShift, span),
            BinaryOp::And | BinaryOp::Or | BinaryOp::NullCoalesce => unreachable!(),
        }

        Ok(())
    }

    fn compile_unary(&mut self, op: &UnaryOp, operand: &Expr, span: Span) -> SaldResult<()> {
        // Use span directly

        // ===== CONSTANT FOLDING =====
        // Try to evaluate constant expressions at compile time
        if let Some(result) = self.try_fold_unary(op, operand) {
            match result {
                FoldedValue::Number(n) => {
                    let const_idx = self.current_chunk().add_constant(Constant::Number(n));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(const_idx as u16, span);
                }
                FoldedValue::Boolean(b) => {
                    self.emit_op(if b { OpCode::True } else { OpCode::False }, span);
                }
                FoldedValue::String(s) => {
                    let const_idx = self.current_chunk().add_constant(Constant::String(s));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(const_idx as u16, span);
                }
            }
            return Ok(());
        }

        self.compile_expr(operand)?;

        match op {
            UnaryOp::Negate => self.emit_op(OpCode::Negate, span),
            UnaryOp::Not => self.emit_op(OpCode::Not, span),
            UnaryOp::BitNot => self.emit_op(OpCode::BitNot, span),
        }

        Ok(())
    }

    fn compile_assignment(
        &mut self,
        target: &Expr,
        op: &AssignOp,
        value: &Expr,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        match target {
            Expr::Identifier { name, .. } => {
                // For compound assignment, get current value first
                if op.is_compound() {
                    self.compile_identifier(name, span)?;
                }

                self.compile_expr(value)?;

                // Apply compound operator
                match op {
                    AssignOp::AddAssign => self.emit_op(OpCode::Add, span),
                    AssignOp::SubAssign => self.emit_op(OpCode::Sub, span),
                    AssignOp::MulAssign => self.emit_op(OpCode::Mul, span),
                    AssignOp::DivAssign => self.emit_op(OpCode::Div, span),
                    AssignOp::ModAssign => self.emit_op(OpCode::Mod, span),
                    AssignOp::Assign => {}
                }

                // Set variable
                if let Some(slot) = self.resolve_local(name) {
                    self.emit_op(OpCode::SetLocal, span);
                    self.emit_u16(slot as u16, span);
                } else if let Some(upvalue) = self.resolve_upvalue(self.scopes.len() - 1, name) {
                    self.emit_op(OpCode::SetUpvalue, span);
                    self.emit_u16(upvalue as u16, span);
                } else {
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(name.clone()));
                    self.emit_op(OpCode::SetGlobal, span);
                    self.emit_u16(const_idx as u16, span);
                }
            }
            Expr::Get {
                object, property, ..
            } => {
                self.compile_expr(object)?;

                if op.is_compound() {
                    self.emit_op(OpCode::Dup, span);
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(property.clone()));
                    self.emit_op(OpCode::GetProperty, span);
                    self.emit_u16(const_idx as u16, span);
                }

                self.compile_expr(value)?;

                match op {
                    AssignOp::AddAssign => self.emit_op(OpCode::Add, span),
                    AssignOp::SubAssign => self.emit_op(OpCode::Sub, span),
                    AssignOp::MulAssign => self.emit_op(OpCode::Mul, span),
                    AssignOp::DivAssign => self.emit_op(OpCode::Div, span),
                    AssignOp::ModAssign => self.emit_op(OpCode::Mod, span),
                    AssignOp::Assign => {}
                }

                let const_idx = self
                    .current_chunk()
                    .add_constant(Constant::String(property.clone()));
                self.emit_op(OpCode::SetProperty, span);
                self.emit_u16(const_idx as u16, span);
            }
            Expr::Index { object, index, .. } => {
                // Handle array index assignment: arr[i] = value
                self.compile_expr(object)?;
                self.compile_expr(index)?;

                if op.is_compound() {
                    // For compound assignment, we need: arr index -> arr index arr[index]
                    // Duplicate the object and index
                    self.emit_op(OpCode::DupTwo, span);
                    self.emit_op(OpCode::GetIndex, span);
                }

                self.compile_expr(value)?;

                match op {
                    AssignOp::AddAssign => self.emit_op(OpCode::Add, span),
                    AssignOp::SubAssign => self.emit_op(OpCode::Sub, span),
                    AssignOp::MulAssign => self.emit_op(OpCode::Mul, span),
                    AssignOp::DivAssign => self.emit_op(OpCode::Div, span),
                    AssignOp::ModAssign => self.emit_op(OpCode::Mod, span),
                    AssignOp::Assign => {}
                }

                self.emit_op(OpCode::SetIndex, span);
            }
            _ => {
                return Err(
                    SaldError::syntax_error("Invalid assignment target", span, &self.file)
                        .with_source(&self.source),
                );
            }
        }

        Ok(())
    }

    fn compile_set(
        &mut self,
        object: &Expr,
        property: &str,
        value: &Expr,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        self.compile_expr(object)?;
        self.compile_expr(value)?;

        let const_idx = self
            .current_chunk()
            .add_constant(Constant::String(property.to_string()));
        self.emit_op(OpCode::SetProperty, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    // ==================== Scope Management ====================

    fn begin_scope(&mut self) {
        self.current_scope_mut().scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.current_scope_mut().scope_depth -= 1;

        // Pop locals off the stack as we exit the scope
        // This is necessary for consecutive blocks at the same level
        // to correctly allocate new locals to clean slots
        // IMPORTANT: For captured locals, we must emit CloseUpvalue instead of Pop
        // This ensures closures created in loops get their own captured values
        while {
            let scope = self.current_scope();
            !scope.locals.is_empty() && scope.locals.last().unwrap().depth > scope.scope_depth
        } {
            // Check if this local was captured by a closure
            let is_captured = self.current_scope().locals.last().unwrap().is_captured;
            
            if is_captured {
                // Emit CloseUpvalue to properly close the captured variable
                self.emit_op(OpCode::CloseUpvalue, Span::default());
            } else {
                // Just pop the local off the stack
                self.emit_op(OpCode::Pop, Span::default());
            }
            self.current_scope_mut().locals.pop();
        }
    }

    /// End a scope while keeping the result value on the stack.
    /// Used for expression scopes like switch arms where we need to preserve the result.
    /// We swap the result with each local before popping it.
    fn end_scope_keep_result(&mut self) {
        self.current_scope_mut().scope_depth -= 1;

        // Count how many locals need to be popped
        let mut locals_to_pop = Vec::new();
        {
            let scope = self.current_scope();
            for local in scope.locals.iter().rev() {
                if local.depth > scope.scope_depth {
                    locals_to_pop.push(local.is_captured);
                } else {
                    break;
                }
            }
        }

        // For each local, we need to:
        // 1. Swap the result (at TOS) with the local below it
        // 2. Pop or CloseUpvalue the local (now at TOS)
        // This preserves the result after all locals are cleaned up
        for is_captured in locals_to_pop {
            // Swap result with local below
            self.emit_op(OpCode::Swap, Span::default());
            
            if is_captured {
                self.emit_op(OpCode::CloseUpvalue, Span::default());
            } else {
                self.emit_op(OpCode::Pop, Span::default());
            }
            self.current_scope_mut().locals.pop();
        }
    }


    fn declare_local(&mut self, name: &str, span: Span) -> SaldResult<()> {
        let scope = self.current_scope();

        // Check for duplicate in current scope
        for local in scope.locals.iter().rev() {
            if local.depth < scope.scope_depth {
                break;
            }
            if local.name == name {
                return Err(SaldError::syntax_error(
                    &format!("Variable '{}' already declared in this scope", name),
                    span,
                    &self.file,
                )
                .with_source(&self.source));
            }
        }

        let depth = self.current_scope().scope_depth;
        self.current_scope_mut().locals.push(Local {
            name: name.to_string(),
            depth,
            initialized: false,
            is_captured: false,
        });

        Ok(())
    }

    fn mark_initialized(&mut self) {
        if let Some(local) = self.current_scope_mut().locals.last_mut() {
            local.initialized = true;
        }
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        let scope = self.current_scope();
        for (i, local) in scope.locals.iter().enumerate().rev() {
            if local.name == name {
                return Some(i);
            }
        }
        None
    }

    /// Resolve an upvalue - a variable captured from an enclosing function scope
    fn resolve_upvalue(&mut self, scope_idx: usize, name: &str) -> Option<usize> {
        // If we're at the top scope, no upvalue possible
        if scope_idx == 0 {
            return None;
        }

        let enclosing_idx = scope_idx - 1;

        // Check if the variable is a local in the enclosing scope
        let local_idx = {
            let enclosing = &self.scopes[enclosing_idx];
            enclosing
                .locals
                .iter()
                .enumerate()
                .rev()
                .find(|(_, local)| local.name == name)
                .map(|(i, _)| i)
        };

        if let Some(local) = local_idx {
            // Mark the local as captured
            self.scopes[enclosing_idx].locals[local].is_captured = true;
            // Add as upvalue capturing a local
            return Some(self.add_upvalue(scope_idx, local, true));
        }

        // Check if it's an upvalue in the enclosing scope (recursive case)
        if let Some(upvalue) = self.resolve_upvalue(enclosing_idx, name) {
            // Add as upvalue capturing an upvalue
            return Some(self.add_upvalue(scope_idx, upvalue, false));
        }

        None
    }

    /// Add an upvalue to the current function scope, returning its index
    fn add_upvalue(&mut self, scope_idx: usize, index: usize, is_local: bool) -> usize {
        let scope = &mut self.scopes[scope_idx];

        // Check if we already have this upvalue
        for (i, upvalue) in scope.upvalues.iter().enumerate() {
            if upvalue.index == index && upvalue.is_local == is_local {
                return i;
            }
        }

        // Add new upvalue
        scope.upvalues.push(Upvalue { index, is_local });
        scope.upvalues.len() - 1
    }

    // ==================== New Feature Compile Methods ====================

    fn compile_break(&mut self, span: Span) -> SaldResult<()> {
        // Use span directly

        if self.current_scope().break_jumps.is_empty() {
            return Err(SaldError::syntax_error(
                "'break' outside of loop",
                span,
                &self.file,
            ));
        }

        // Get the scope depth at loop entry
        let target_depth = *self.current_scope().loop_scope_depths.last().unwrap();

        // Pop all locals from scopes deeper than the loop entry scope
        // This cleans up locals declared inside the loop body before we jump out
        let mut pops_needed = 0;
        for local in self.current_scope().locals.iter().rev() {
            if local.depth > target_depth {
                pops_needed += 1;
            } else {
                break;
            }
        }

        for _ in 0..pops_needed {
            self.emit_op(OpCode::Pop, span);
        }

        // Emit a jump that we'll patch later when the loop ends
        let break_jump = self.emit_jump(OpCode::Jump, span);

        // Record this break to patch later
        let scope = self.current_scope_mut();
        if let Some(breaks) = scope.break_jumps.last_mut() {
            breaks.push(break_jump);
        }

        Ok(())
    }

    fn compile_continue(&mut self, span: Span) -> SaldResult<()> {
        // Use span directly

        let (loop_start, target_depth) = {
            let scope = self.current_scope();
            if scope.loop_starts.is_empty() {
                return Err(SaldError::syntax_error(
                    "'continue' outside of loop",
                    span,
                    &self.file,
                ));
            }
            (
                *scope.loop_starts.last().unwrap(),
                *scope.loop_scope_depths.last().unwrap(),
            )
        };

        // Pop all locals from scopes deeper than the loop entry scope
        let mut pops_needed = 0;
        for local in self.current_scope().locals.iter().rev() {
            if local.depth > target_depth {
                pops_needed += 1;
            } else {
                break;
            }
        }

        for _ in 0..pops_needed {
            self.emit_op(OpCode::Pop, span);
        }

        // Jump back to loop start
        self.emit_loop(loop_start, span);

        Ok(())
    }

    fn compile_import(&mut self, path: &str, alias: Option<&str>, span: Span) -> SaldResult<()> {
        // Use span directly

        // Add path as constant
        let path_const = self
            .current_chunk()
            .add_constant(Constant::String(path.to_string()));

        if let Some(alias) = alias {
            // Import with alias: import "file.sald" as Module
            let alias_const = self
                .current_chunk()
                .add_constant(Constant::String(alias.to_string()));
            self.emit_op(OpCode::ImportAs, span);
            self.emit_u16(path_const as u16, span);
            // Store alias constant in next u16
            self.emit_u16(alias_const as u16, span);
        } else {
            // Global import: import "file.sald"
            self.emit_op(OpCode::Import, span);
            self.emit_u16(path_const as u16, span);
        }

        Ok(())
    }

    fn compile_ternary(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Compile condition
        self.compile_expr(condition)?;

        // Jump to else if false
        let else_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span); // Pop condition

        // Compile then expression
        self.compile_expr(then_expr)?;

        // Jump over else
        let end_jump = self.emit_jump(OpCode::Jump, span);

        // Else branch
        self.patch_jump(else_jump);
        self.emit_op(OpCode::Pop, span); // Pop condition

        // Compile else expression
        self.compile_expr(else_expr)?;

        self.patch_jump(end_jump);

        Ok(())
    }

    fn compile_lambda(
        &mut self,
        params: &[FunctionParam],
        body: &LambdaBody,
        is_async: bool,
        span: Span,
    ) -> SaldResult<()> {
        // Use span directly

        // Create anonymous function name
        let lambda_name = format!("<lambda@{}:{}>", span.start.line, span.start.column);

        // Create new function scope
        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        // Declare parameters as locals
        for param in params {
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        // Compile body
        match body {
            LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.compile_stmt(stmt)?;
                }
                // Implicit return null if no explicit return
                self.emit_op(OpCode::Null, span);
                self.emit_op(OpCode::Return, span);
            }
            LambdaBody::Expr(expr) => {
                // Expression body returns the expression value
                self.compile_expr(expr)?;
                self.emit_op(OpCode::Return, span);
            }
        }

        self.end_scope();

        // Get compiled function scope
        let func_scope = self.scopes.pop().unwrap();
        let arity = params.len();

        // Check if lambda has variadic parameter
        let is_variadic = params.last().map(|p| p.is_variadic).unwrap_or(false);

        // Create function constant with upvalue info
        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();

        let func_const = Constant::Function(FunctionConstant {
            name: lambda_name,
            arity,
            is_variadic,
            is_async,
            upvalue_count: upvalues.len(),
            upvalues,
            chunk: func_scope.chunk,
            file: self.file.clone(),
            param_names: params.iter().map(|p| p.name.clone()).collect(),
            default_count: params.iter().filter(|p| p.default_value.is_some()).count(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);

        // Emit closure instruction
        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    fn compile_super(&mut self, method: &str, span: Span) -> SaldResult<()> {
        // Use span directly

        if self.class_depth == 0 {
            return Err(SaldError::syntax_error(
                "'super' used outside of class",
                span,
                &self.file,
            ));
        }

        // Push self onto stack (receiver for super method)
        self.emit_op(OpCode::GetSelf, span);

        // Get super method
        let method_const = self
            .current_chunk()
            .add_constant(Constant::String(method.to_string()));
        self.emit_op(OpCode::GetSuper, span);
        self.emit_u16(method_const as u16, span);

        Ok(())
    }

    // ==================== Emit Helpers ====================

    fn emit_op(&mut self, op: OpCode, span: Span) {
        self.current_chunk().write_op(op, span);
    }

    fn emit_u16(&mut self, value: u16, span: Span) {
        self.current_chunk().write_u16(value, span);
    }

    fn emit_jump(&mut self, op: OpCode, span: Span) -> usize {
        self.emit_op(op, span);
        self.emit_u16(0xFFFF, span);
        self.current_chunk().current_offset() - 2
    }

    fn patch_jump(&mut self, offset: usize) {
        self.current_chunk().patch_jump(offset);
    }

    fn emit_loop(&mut self, loop_start: usize, span: Span) {
        self.emit_op(OpCode::Loop, span);
        let offset = self.current_chunk().current_offset() - loop_start + 2;
        self.emit_u16(offset as u16, span);
    }

    // ==================== Constant Folding ====================

    /// Try to evaluate a binary expression at compile time if both operands are literals
    fn try_fold_binary(&self, left: &Expr, op: &BinaryOp, right: &Expr) -> Option<FoldedValue> {
        // Extract literal values from expressions
        let left_lit = self.extract_literal(left)?;
        let right_lit = self.extract_literal(right)?;

        match (left_lit, right_lit) {
            // Number operations
            (FoldedValue::Number(a), FoldedValue::Number(b)) => {
                match op {
                    BinaryOp::Add => Some(FoldedValue::Number(a + b)),
                    BinaryOp::Sub => Some(FoldedValue::Number(a - b)),
                    BinaryOp::Mul => Some(FoldedValue::Number(a * b)),
                    BinaryOp::Div if b != 0.0 => Some(FoldedValue::Number(a / b)),
                    BinaryOp::Mod if b != 0.0 => Some(FoldedValue::Number(a % b)),
                    // Comparison
                    BinaryOp::Less => Some(FoldedValue::Boolean(a < b)),
                    BinaryOp::LessEqual => Some(FoldedValue::Boolean(a <= b)),
                    BinaryOp::Greater => Some(FoldedValue::Boolean(a > b)),
                    BinaryOp::GreaterEqual => Some(FoldedValue::Boolean(a >= b)),
                    BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                    BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),
                    // Bitwise (convert to integers)
                    BinaryOp::BitAnd => Some(FoldedValue::Number((a as i64 & b as i64) as f64)),
                    BinaryOp::BitOr => Some(FoldedValue::Number((a as i64 | b as i64) as f64)),
                    BinaryOp::BitXor => Some(FoldedValue::Number((a as i64 ^ b as i64) as f64)),
                    BinaryOp::LeftShift => Some(FoldedValue::Number(((a as i64) << (b as u32)) as f64)),
                    BinaryOp::RightShift => Some(FoldedValue::Number(((a as i64) >> (b as u32)) as f64)),
                    _ => None,
                }
            }
            // String concatenation
            (FoldedValue::String(a), FoldedValue::String(b)) if matches!(op, BinaryOp::Add) => {
                Some(FoldedValue::String(format!("{}{}", a, b)))
            }
            // Boolean comparisons
            (FoldedValue::Boolean(a), FoldedValue::Boolean(b)) => {
                match op {
                    BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                    BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),
                    _ => None,
                }
            }
            // String equality
            (FoldedValue::String(a), FoldedValue::String(b)) => {
                match op {
                    BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                    BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Try to evaluate a unary expression at compile time
    fn try_fold_unary(&self, op: &UnaryOp, operand: &Expr) -> Option<FoldedValue> {
        let value = self.extract_literal(operand)?;

        match (op, value) {
            (UnaryOp::Negate, FoldedValue::Number(n)) => Some(FoldedValue::Number(-n)),
            (UnaryOp::Not, FoldedValue::Boolean(b)) => Some(FoldedValue::Boolean(!b)),
            (UnaryOp::BitNot, FoldedValue::Number(n)) => Some(FoldedValue::Number(!(n as i64) as f64)),
            _ => None,
        }
    }

    /// Extract a compile-time constant from an expression
    fn extract_literal(&self, expr: &Expr) -> Option<FoldedValue> {
        match expr {
            Expr::Literal { value, .. } => {
                match value {
                    Literal::Number(n) => Some(FoldedValue::Number(*n)),
                    Literal::Boolean(b) => Some(FoldedValue::Boolean(*b)),
                    Literal::String(s) => Some(FoldedValue::String(s.clone())),
                    Literal::Null => None, // Don't fold null
                }
            }
            Expr::Grouping { expr, .. } => self.extract_literal(expr),
            Expr::Unary { op, operand, .. } => self.try_fold_unary(op, operand),
            Expr::Binary { left, op, right, .. } => self.try_fold_binary(left, op, right),
            _ => None,
        }
    }
}
