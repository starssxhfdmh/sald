use super::chunk::{Chunk, Constant, FunctionConstant, UpvalueInfo};
use super::opcode::OpCode;
use crate::ast::*;
use crate::error::{SaldError, SaldResult, Span};
use crate::vm::interner::intern;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
enum FoldedValue {
    Number(f64),
    Boolean(bool),
    String(String),
}

#[derive(Debug, Clone)]
struct Local {
    name: String,
    depth: usize,
    initialized: bool,
    is_captured: bool,
}

#[derive(Debug, Clone)]
struct Upvalue {
    index: usize,
    is_local: bool,
}

struct FunctionScope {
    chunk: Chunk,
    locals: Vec<Local>,
    upvalues: Vec<Upvalue>,
    scope_depth: usize,

    loop_starts: Vec<usize>,

    break_jumps: Vec<Vec<usize>>,

    loop_scope_depths: Vec<usize>,

    function_name: Option<String>,
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
            function_name: None,
        };

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

pub struct Compiler {
    scopes: Vec<FunctionScope>,
    file: String,
    source: String,
    had_error: bool,
    class_depth: usize,
    interfaces: FxHashMap<String, InterfaceDef>,
    current_namespace: Option<String>,
    current_class: Option<String>,
}

impl Compiler {
    pub fn new(file: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            scopes: vec![FunctionScope::new(false)],
            file: file.into(),
            source: source.into(),
            had_error: false,
            class_depth: 0,
            interfaces: FxHashMap::default(),
            current_namespace: None,
            current_class: None,
        }
    }

    pub fn compile(&mut self, program: &Program) -> SaldResult<Chunk> {
        for stmt in &program.statements {
            self.compile_stmt(stmt)?;
        }

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

    pub fn compile_repl(&mut self, program: &Program) -> SaldResult<Chunk> {
        let stmts = &program.statements;

        for (i, stmt) in stmts.iter().enumerate() {
            let is_last = i == stmts.len() - 1;

            if is_last {
                match stmt {
                    Stmt::Expression { expr, .. } => {
                        self.compile_expr(expr)?;
                    }
                    _ => {
                        self.compile_stmt(stmt)?;

                        self.emit_op(OpCode::Null, Span::default());
                    }
                }
            } else {
                self.compile_stmt(stmt)?;
            }
        }

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

    fn compile_stmt(&mut self, stmt: &Stmt) -> SaldResult<()> {
        match stmt {
            Stmt::Let {
                name,
                name_span: _,
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
            Stmt::Enum {
                name,
                variants,
                span,
            } => {
                self.compile_enum(name, variants, *span)?;
            }
            Stmt::Interface { def } => {
                self.compile_interface(def)?;
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
        if name.starts_with("self.") {
            let property = &name[5..];
            self.emit_op(OpCode::GetSelf, span);

            if let Some(init) = initializer {
                self.compile_expr(init)?;
            } else {
                self.emit_op(OpCode::Null, span);
            }

            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&property.to_string())));
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
            self.declare_local(name, span)?;
            self.mark_initialized();
        } else {
            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&name.to_string())));
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

        self.compile_expr(initializer)?;

        for (i, elem) in pattern.elements.iter().enumerate() {
            match elem {
                ArrayPatternElement::Variable {
                    name,
                    span: var_span,
                } => {
                    self.emit_op(OpCode::Dup, span);

                    let idx_const = self
                        .current_chunk()
                        .add_constant(Constant::Number(i as f64));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(idx_const as u16, span);

                    self.emit_op(OpCode::GetIndex, span);

                    if self.current_scope().scope_depth > 0 {
                        self.declare_local(name, *var_span)?;
                        self.mark_initialized();
                    } else {
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(intern(&name.to_string())));
                        self.emit_op(OpCode::DefineGlobal, span);
                        self.emit_u16(const_idx as u16, span);
                    }
                }
                ArrayPatternElement::Rest {
                    name,
                    span: var_span,
                } => {
                    self.emit_op(OpCode::Dup, span);

                    let start_const = self
                        .current_chunk()
                        .add_constant(Constant::Number(i as f64));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(start_const as u16, span);

                    let slice_name = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&"slice".to_string())));
                    self.emit_op(OpCode::Invoke, span);
                    self.emit_u16(slice_name as u16, span);
                    self.emit_u16(1, span);

                    if self.current_scope().scope_depth > 0 {
                        self.declare_local(name, *var_span)?;
                        self.mark_initialized();
                    } else {
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(intern(&name.to_string())));
                        self.emit_op(OpCode::DefineGlobal, span);
                        self.emit_u16(const_idx as u16, span);
                    }
                }
                ArrayPatternElement::Hole => {}
            }
        }

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
        self.compile_expr(condition)?;

        let then_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        self.compile_stmt(then_branch)?;

        let else_jump = self.emit_jump(OpCode::Jump, span);

        self.patch_jump(then_jump);
        self.emit_op(OpCode::Pop, span);

        if let Some(else_stmt) = else_branch {
            self.compile_stmt(else_stmt)?;
        }

        self.patch_jump(else_jump);

        Ok(())
    }

    fn compile_while(&mut self, condition: &Expr, body: &Stmt, span: Span) -> SaldResult<()> {
        let loop_start = self.current_chunk().current_offset();

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

    fn compile_for(
        &mut self,
        variable: &str,
        iterable: &Expr,
        body: &Stmt,
        span: Span,
    ) -> SaldResult<()> {
        self.begin_scope();

        self.compile_expr(iterable)?;
        self.declare_local("__iter", span)?;
        self.mark_initialized();

        self.emit_op(OpCode::Constant, span);
        let zero_const = self.current_chunk().add_constant(Constant::Number(0.0));
        self.emit_u16(zero_const as u16, span);
        self.declare_local("__idx", span)?;
        self.mark_initialized();

        self.emit_op(OpCode::Null, span);
        self.declare_local(variable, span)?;
        self.mark_initialized();

        let loop_start = self.current_chunk().current_offset();

        let entry_scope_depth = self.current_scope().scope_depth;
        self.current_scope_mut().loop_starts.push(loop_start);
        self.current_scope_mut().break_jumps.push(Vec::new());
        self.current_scope_mut()
            .loop_scope_depths
            .push(entry_scope_depth);

        if let Some(idx_slot) = self.resolve_local("__idx") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);
        }

        if let Some(iter_slot) = self.resolve_local("__iter") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(iter_slot as u16, span);

            let length_const = self
                .current_chunk()
                .add_constant(Constant::String(intern(&"length".to_string())));
            self.emit_op(OpCode::Invoke, span);
            self.emit_u16(length_const as u16, span);
            self.emit_u16(0, span);
        }

        self.emit_op(OpCode::Less, span);

        let exit_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        if let Some(iter_slot) = self.resolve_local("__iter") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(iter_slot as u16, span);
        }

        if let Some(idx_slot) = self.resolve_local("__idx") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);
        }

        self.emit_op(OpCode::GetIndex, span);

        if let Some(var_slot) = self.resolve_local(variable) {
            self.emit_op(OpCode::SetLocal, span);
            self.emit_u16(var_slot as u16, span);
            self.emit_op(OpCode::Pop, span);
        }

        self.compile_stmt(body)?;

        if let Some(idx_slot) = self.resolve_local("__idx") {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(idx_slot as u16, span);

            let one_const = self.current_chunk().add_constant(Constant::Number(1.0));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(one_const as u16, span);
            self.emit_op(OpCode::Add, span);

            self.emit_op(OpCode::SetLocal, span);
            self.emit_u16(idx_slot as u16, span);
            self.emit_op(OpCode::Pop, span);
        }

        self.emit_loop(loop_start, span);

        self.patch_jump(exit_jump);
        self.emit_op(OpCode::Pop, span);

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

        self.scopes.push(FunctionScope::new(as_method));

        if !as_method {
            self.current_scope_mut().function_name = Some(def.name.clone());
        }

        self.begin_scope();

        for (i, param) in def.params.iter().enumerate() {
            if as_method && !def.is_static && i == 0 && param.name == "self" {
                continue;
            }
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        for param in def.params.iter() {
            if let Some(ref default_expr) = param.default_value {
                let local_slot = self
                    .resolve_local(&param.name)
                    .expect("Parameter should be defined as local");

                self.emit_op(OpCode::GetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);

                let skip_jump = self.emit_jump(OpCode::JumpIfNotNull, param.span);

                self.emit_op(OpCode::Pop, param.span);

                self.compile_expr(default_expr)?;

                self.emit_op(OpCode::SetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);

                self.emit_op(OpCode::Pop, param.span);

                let after_default = self.emit_jump(OpCode::Jump, param.span);

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

        let arity = if as_method && !def.is_static {
            if def.params.first().map(|p| p.name.as_str()) == Some("self") {
                def.params.len().saturating_sub(1)
            } else {
                def.params.len()
            }
        } else {
            def.params.len()
        };

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
            default_count: def
                .params
                .iter()
                .filter(|p| p.default_value.is_some())
                .count(),
            decorators: def.decorators.iter().map(|d| d.name.clone()).collect(),
            namespace_context: self.current_namespace.clone(),
            class_context: self.current_class.clone(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);

        if as_method {
            if def.is_static {
                self.emit_op(OpCode::StaticMethod, func_span);
            } else {
                self.emit_op(OpCode::Method, func_span);
            }
            self.emit_u16(const_idx as u16, func_span);
        } else {
            let user_decorators: Vec<_> = def.decorators.iter().collect();

            for decorator in &user_decorators {
                let decorator_name_const = self
                    .current_chunk()
                    .add_constant(Constant::String(intern(&decorator.name.clone())));
                self.emit_op(OpCode::GetGlobal, func_span);
                self.emit_u16(decorator_name_const as u16, func_span);

                if !decorator.args.is_empty() {
                    for arg in &decorator.args {
                        self.compile_expr(arg)?;
                    }
                    self.emit_op(OpCode::Call, func_span);
                    self.emit_u16(decorator.args.len() as u16, func_span);
                }
            }

            self.emit_op(OpCode::Closure, func_span);
            self.emit_u16(const_idx as u16, func_span);

            for _ in &user_decorators {
                self.emit_op(OpCode::Call, func_span);
                self.emit_u16(1, func_span);
            }

            if self.current_scope().scope_depth == 0 {
                let name_const = self
                    .current_chunk()
                    .add_constant(Constant::String(intern(&def.name.clone())));
                self.emit_op(OpCode::DefineGlobal, func_span);
                self.emit_u16(name_const as u16, func_span);
            }
        }

        Ok(())
    }

    fn compile_return(&mut self, value: Option<&Expr>, span: Span) -> SaldResult<()> {
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

        let previous_class = self.current_class.clone();
        self.current_class = Some(def.name.clone());

        for interface_name in &def.implements {
            if let Some(interface_def) = self.interfaces.get(interface_name).cloned() {
                self.validate_interface_implementation(def, &interface_def)?;
            } else {
                return Err(SaldError::interface_error(
                    format!("Interface '{}' is not defined", interface_name),
                    def.span,
                    &self.file,
                )
                .with_source(&self.source));
            }
        }

        let name_const = self
            .current_chunk()
            .add_constant(Constant::String(intern(&def.name.clone())));
        self.emit_op(OpCode::Class, class_span);
        self.emit_u16(name_const as u16, class_span);

        if let Some(superclass) = &def.superclass {
            let super_const = self
                .current_chunk()
                .add_constant(Constant::String(intern(&superclass.clone())));
            self.emit_op(OpCode::GetGlobal, class_span);
            self.emit_u16(super_const as u16, class_span);

            self.emit_op(OpCode::Inherit, class_span);
            self.emit_u16(0, class_span);
        }

        for method in &def.methods {
            self.compile_function(method, true)?;
        }

        let name_const2 = self
            .current_chunk()
            .add_constant(Constant::String(intern(&def.name.clone())));
        self.emit_op(OpCode::DefineGlobal, class_span);
        self.emit_u16(name_const2 as u16, class_span);

        self.class_depth -= 1;

        self.current_class = previous_class;

        Ok(())
    }

    fn compile_interface(&mut self, def: &InterfaceDef) -> SaldResult<()> {
        self.interfaces.insert(def.name.clone(), def.clone());

        Ok(())
    }

    fn validate_interface_implementation(
        &self,
        class_def: &ClassDef,
        interface_def: &InterfaceDef,
    ) -> SaldResult<()> {
        for interface_method in &interface_def.methods {
            let class_method = class_def
                .methods
                .iter()
                .find(|m| m.name == interface_method.name);

            match class_method {
                None => {
                    let param_list: Vec<&str> = interface_method
                        .params
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect();

                    return Err(SaldError::interface_error(
                        format!(
                            "Class '{}' does not implement method '{}' required by interface '{}'",
                            class_def.name, interface_method.name, interface_def.name
                        ),
                        class_def.span,
                        &self.file,
                    )
                    .with_source(&self.source)
                    .with_help(format!(
                        "Add method: fun {}({})",
                        interface_method.name,
                        param_list.join(", ")
                    )));
                }
                Some(method) => {
                    let interface_param_count = interface_method
                        .params
                        .iter()
                        .filter(|p| p.name != "self")
                        .count();
                    let class_param_count =
                        method.params.iter().filter(|p| p.name != "self").count();

                    if interface_param_count != class_param_count {
                        return Err(SaldError::interface_error(
                            format!(
                                "Method '{}' in class '{}' has {} parameter(s), but interface '{}' requires {}",
                                interface_method.name,
                                class_def.name,
                                class_param_count,
                                interface_def.name,
                                interface_param_count
                            ),
                            method.span,
                            &self.file,
                        ).with_source(&self.source));
                    }
                }
            }
        }

        Ok(())
    }

    fn compile_try_catch(
        &mut self,
        try_body: &Stmt,
        catch_var: &str,
        catch_body: &Stmt,
        span: Span,
    ) -> SaldResult<()> {
        self.emit_op(OpCode::TryStart, span);
        let catch_jump = self.current_chunk().current_offset();
        self.emit_u16(0, span);

        self.compile_stmt(try_body)?;

        self.emit_op(OpCode::TryEnd, span);

        let end_jump = self.emit_jump(OpCode::Jump, span);

        self.current_chunk().patch_jump(catch_jump);

        self.begin_scope();

        let depth = self.current_scope().scope_depth;
        self.current_scope_mut().locals.push(Local {
            name: catch_var.to_string(),
            depth,
            initialized: true,
            is_captured: false,
        });

        self.compile_stmt(catch_body)?;

        self.end_scope();

        self.patch_jump(end_jump);

        Ok(())
    }

    fn compile_throw(&mut self, value: &Expr, span: Span) -> SaldResult<()> {
        self.compile_expr(value)?;

        self.emit_op(OpCode::Throw, span);

        Ok(())
    }

    fn compile_const(&mut self, name: &str, value: &Expr, span: Span) -> SaldResult<()> {
        self.compile_expr(value)?;

        let const_idx = self
            .current_chunk()
            .add_constant(Constant::String(intern(&name.to_string())));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    fn compile_namespace(&mut self, name: &str, body: &[Stmt], span: Span) -> SaldResult<()> {
        let func_name = format!("<namespace {}>", name);

        let previous_namespace = self.current_namespace.clone();
        let full_namespace_name = match &previous_namespace {
            Some(parent) => format!("{}.{}", parent, name),
            None => name.to_string(),
        };
        self.current_namespace = Some(full_namespace_name.clone());

        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        let mut namespace_vars: Vec<(String, Span)> = Vec::new();

        for stmt in body {
            match stmt {
                Stmt::Let {
                    name: var_name,
                    name_span: _,
                    initializer,
                    span: var_span,
                } => {
                    if let Some(init) = initializer {
                        self.compile_expr(init)?;
                    } else {
                        self.emit_op(OpCode::Null, *var_span);
                    }
                    self.declare_local(var_name, *var_span)?;
                    self.mark_initialized();
                    namespace_vars.push((var_name.clone(), *var_span));
                }
                Stmt::Const {
                    name: const_name,
                    value,
                    span: const_span,
                } => {
                    self.compile_expr(value)?;
                    self.declare_local(const_name, *const_span)?;
                    self.mark_initialized();
                    namespace_vars.push((const_name.clone(), *const_span));
                }
                _ => {}
            }
        }

        let mut member_count = 0;

        for (var_name, var_span) in &namespace_vars {
            let key_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&var_name.clone())));
            self.emit_op(OpCode::Constant, *var_span);
            self.emit_u16(key_idx as u16, *var_span);

            if let Some(slot) = self.resolve_local(var_name) {
                self.emit_op(OpCode::GetLocal, *var_span);
                self.emit_u16(slot as u16, *var_span);
            }
            member_count += 1;
        }

        for stmt in body {
            match stmt {
                Stmt::Function { def } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&def.name.clone())));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);

                    self.compile_namespace_function(def)?;
                    member_count += 1;
                }
                Stmt::Class { def } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&def.name.clone())));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);

                    self.compile_namespace_class(def)?;
                    member_count += 1;
                }
                Stmt::Namespace {
                    name: ns_name,
                    body: ns_body,
                    span: ns_span,
                } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&ns_name.clone())));
                    self.emit_op(OpCode::Constant, *ns_span);
                    self.emit_u16(key_idx as u16, *ns_span);

                    self.compile_namespace_inner(ns_name, ns_body, *ns_span)?;
                    member_count += 1;
                }
                Stmt::Enum {
                    name: enum_name,
                    variants,
                    span: enum_span,
                } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&enum_name.clone())));
                    self.emit_op(OpCode::Constant, *enum_span);
                    self.emit_u16(key_idx as u16, *enum_span);

                    self.compile_enum_inner(enum_name, variants, *enum_span)?;
                    member_count += 1;
                }

                Stmt::Let { .. } | Stmt::Const { .. } => {}
                _ => {}
            }
        }

        self.emit_op(OpCode::BuildNamespace, span);
        self.emit_u16(member_count as u16, span);

        self.emit_op(OpCode::Return, span);

        let func_scope = self.scopes.pop().unwrap();

        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();

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
            decorators: Vec::new(),
            namespace_context: None,
            class_context: None,
        });

        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);

        self.emit_op(OpCode::Call, span);
        self.emit_u16(0, span);

        let name_idx = self
            .current_chunk()
            .add_constant(Constant::String(intern(&name.to_string())));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(name_idx as u16, span);

        self.current_namespace = previous_namespace;

        Ok(())
    }

    fn compile_namespace_inner(&mut self, name: &str, body: &[Stmt], span: Span) -> SaldResult<()> {
        let func_name = format!("<namespace {}>", name);

        let previous_namespace = self.current_namespace.clone();
        let full_namespace_name = match &previous_namespace {
            Some(parent) => format!("{}.{}", parent, name),
            None => name.to_string(),
        };
        self.current_namespace = Some(full_namespace_name.clone());

        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        let mut namespace_vars: Vec<(String, Span)> = Vec::new();

        for stmt in body {
            match stmt {
                Stmt::Let {
                    name: var_name,
                    name_span: _,
                    initializer,
                    span: var_span,
                } => {
                    if let Some(init) = initializer {
                        self.compile_expr(init)?;
                    } else {
                        self.emit_op(OpCode::Null, *var_span);
                    }
                    self.declare_local(var_name, *var_span)?;
                    self.mark_initialized();
                    namespace_vars.push((var_name.clone(), *var_span));
                }
                Stmt::Const {
                    name: const_name,
                    value,
                    span: const_span,
                } => {
                    self.compile_expr(value)?;
                    self.declare_local(const_name, *const_span)?;
                    self.mark_initialized();
                    namespace_vars.push((const_name.clone(), *const_span));
                }
                _ => {}
            }
        }

        let mut member_count = 0;

        for (var_name, var_span) in &namespace_vars {
            let key_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&var_name.clone())));
            self.emit_op(OpCode::Constant, *var_span);
            self.emit_u16(key_idx as u16, *var_span);
            if let Some(slot) = self.resolve_local(var_name) {
                self.emit_op(OpCode::GetLocal, *var_span);
                self.emit_u16(slot as u16, *var_span);
            }
            member_count += 1;
        }

        for stmt in body {
            match stmt {
                Stmt::Function { def } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&def.name.clone())));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    self.compile_namespace_function(def)?;
                    member_count += 1;
                }
                Stmt::Class { def } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&def.name.clone())));
                    self.emit_op(OpCode::Constant, def.span);
                    self.emit_u16(key_idx as u16, def.span);
                    self.compile_namespace_class(def)?;
                    member_count += 1;
                }
                Stmt::Namespace {
                    name: ns_name,
                    body: ns_body,
                    span: ns_span,
                } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&ns_name.clone())));
                    self.emit_op(OpCode::Constant, *ns_span);
                    self.emit_u16(key_idx as u16, *ns_span);
                    self.compile_namespace_inner(ns_name, ns_body, *ns_span)?;
                    member_count += 1;
                }
                Stmt::Enum {
                    name: enum_name,
                    variants,
                    span: enum_span,
                } => {
                    let key_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&enum_name.clone())));
                    self.emit_op(OpCode::Constant, *enum_span);
                    self.emit_u16(key_idx as u16, *enum_span);
                    self.compile_enum_inner(enum_name, variants, *enum_span)?;
                    member_count += 1;
                }

                Stmt::Let { .. } | Stmt::Const { .. } => {}
                _ => {}
            }
        }

        self.emit_op(OpCode::BuildNamespace, span);
        self.emit_u16(member_count as u16, span);

        self.emit_op(OpCode::Return, span);

        let func_scope = self.scopes.pop().unwrap();

        let upvalues: Vec<UpvalueInfo> = func_scope
            .upvalues
            .iter()
            .map(|u| UpvalueInfo {
                index: u.index as u8,
                is_local: u.is_local,
            })
            .collect();

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
            decorators: Vec::new(),
            namespace_context: None,
            class_context: None,
        });

        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);

        self.emit_op(OpCode::Call, span);
        self.emit_u16(0, span);

        self.current_namespace = previous_namespace;

        Ok(())
    }

    fn compile_namespace_function(&mut self, def: &FunctionDef) -> SaldResult<()> {
        let func_span = def.span;

        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        for param in &def.params {
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        for param in def.params.iter() {
            if let Some(ref default_expr) = param.default_value {
                let local_slot = self
                    .resolve_local(&param.name)
                    .expect("Parameter should be defined as local");

                self.emit_op(OpCode::GetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);

                let skip_jump = self.emit_jump(OpCode::JumpIfNotNull, param.span);

                self.emit_op(OpCode::Pop, param.span);

                self.compile_expr(default_expr)?;

                self.emit_op(OpCode::SetLocal, param.span);
                self.emit_u16(local_slot as u16, param.span);

                self.emit_op(OpCode::Pop, param.span);

                let after_default = self.emit_jump(OpCode::Jump, param.span);

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
            default_count: def
                .params
                .iter()
                .filter(|p| p.default_value.is_some())
                .count(),
            decorators: def.decorators.iter().map(|d| d.name.clone()).collect(),
            namespace_context: self.current_namespace.clone(),
            class_context: self.current_class.clone(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);
        self.emit_op(OpCode::Closure, func_span);
        self.emit_u16(const_idx as u16, func_span);

        Ok(())
    }

    fn compile_namespace_class(&mut self, def: &ClassDef) -> SaldResult<()> {
        let class_span = def.span;
        self.class_depth += 1;

        let previous_class = self.current_class.clone();
        self.current_class = Some(def.name.clone());

        let name_const = self
            .current_chunk()
            .add_constant(Constant::String(intern(&def.name.clone())));
        self.emit_op(OpCode::Class, class_span);
        self.emit_u16(name_const as u16, class_span);

        if let Some(superclass) = &def.superclass {
            let super_const = self
                .current_chunk()
                .add_constant(Constant::String(intern(&superclass.clone())));
            self.emit_op(OpCode::GetGlobal, class_span);
            self.emit_u16(super_const as u16, class_span);
            self.emit_op(OpCode::Inherit, class_span);
            self.emit_u16(0, class_span);
        }

        for method in &def.methods {
            self.compile_function(method, true)?;
        }

        self.class_depth -= 1;

        self.current_class = previous_class;

        Ok(())
    }

    fn compile_enum(&mut self, name: &str, variants: &[String], span: Span) -> SaldResult<()> {
        self.compile_enum_inner(name, variants, span)?;

        let name_idx = self
            .current_chunk()
            .add_constant(Constant::String(intern(&name.to_string())));
        self.emit_op(OpCode::DefineGlobal, span);
        self.emit_u16(name_idx as u16, span);

        Ok(())
    }

    fn compile_enum_inner(
        &mut self,
        name: &str,
        variants: &[String],
        span: Span,
    ) -> SaldResult<()> {
        for variant in variants {
            let key_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&variant.clone())));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(key_idx as u16, span);

            let value = format!("{}.{}", name, variant);
            let val_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&value)));
            self.emit_op(OpCode::Constant, span);
            self.emit_u16(val_idx as u16, span);
        }

        self.emit_op(OpCode::BuildEnum, span);
        self.emit_u16(variants.len() as u16, span);

        Ok(())
    }

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
            Expr::Call {
                callee,
                args,
                is_optional: _,
                span,
            } => {
                if let Expr::Get {
                    object,
                    property,
                    is_optional,
                    ..
                } = callee.as_ref()
                {
                    self.compile_expr(object)?;

                    if *is_optional {
                        self.emit_op(OpCode::Dup, *span);
                        let normal_jump = self.emit_jump(OpCode::JumpIfNotNull, *span);

                        self.emit_op(OpCode::Pop, *span);
                        let end_jump = self.emit_jump(OpCode::Jump, *span);

                        self.patch_jump(normal_jump);
                        self.emit_op(OpCode::Pop, *span);

                        self.compile_expr(object)?;

                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(intern(&property.clone())));
                        self.emit_op(OpCode::Invoke, *span);
                        self.emit_u16(const_idx as u16, *span);
                        self.emit_u16(args.len() as u16, *span);
                        self.patch_jump(end_jump);
                    } else {
                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        let const_idx = self
                            .current_chunk()
                            .add_constant(Constant::String(intern(&property.clone())));
                        self.emit_op(OpCode::Invoke, *span);
                        self.emit_u16(const_idx as u16, *span);
                        self.emit_u16(args.len() as u16, *span);
                    }
                } else if let Expr::Identifier { name, .. } = callee.as_ref() {
                    let is_self_recursive = self
                        .current_scope()
                        .function_name
                        .as_ref()
                        .map(|fn_name| fn_name == name)
                        .unwrap_or(false);

                    if is_self_recursive {
                        self.emit_op(OpCode::Null, *span);
                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        self.emit_op(OpCode::RecursiveCall, *span);
                        self.emit_u16(args.len() as u16, *span);
                    } else {
                        self.compile_expr(callee)?;
                        for arg in args {
                            self.compile_expr(&arg.value)?;
                        }
                        self.emit_op(OpCode::Call, *span);
                        self.emit_u16(args.len() as u16, *span);
                    }
                } else {
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
                    self.emit_op(OpCode::Dup, *span);

                    let normal_jump = self.emit_jump(OpCode::JumpIfNotNull, *span);

                    self.emit_op(OpCode::Pop, *span);

                    let end_jump = self.emit_jump(OpCode::Jump, *span);

                    self.patch_jump(normal_jump);
                    self.emit_op(OpCode::Pop, *span);

                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&property.clone())));
                    self.emit_op(OpCode::GetProperty, *span);
                    self.emit_u16(const_idx as u16, *span);

                    self.patch_jump(end_jump);
                } else {
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&property.clone())));
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
                if let Some(idx) = self.resolve_local("self") {
                    self.emit_op(OpCode::GetLocal, *span);
                    self.emit_u16(idx as u16, *span);
                } else if let Some(idx) = self.resolve_upvalue(self.scopes.len() - 1, "self") {
                    self.emit_op(OpCode::GetUpvalue, *span);
                    self.emit_u16(idx as u16, *span);
                } else {
                    self.emit_op(OpCode::GetSelf, *span);
                }
            }
            Expr::Array { elements, span } => {
                for elem in elements {
                    self.compile_expr(elem)?;
                }

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
                for stmt in statements {
                    self.compile_stmt(stmt)?;
                }

                if let Some(final_expr) = expr {
                    self.compile_expr(final_expr)?;
                } else {
                    self.emit_op(OpCode::Null, *span);
                }
            }
            Expr::Dictionary { entries, span } => {
                for (key, value) in entries {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                }

                self.emit_op(OpCode::BuildDict, *span);
                self.emit_u16(entries.len() as u16, *span);
            }
            Expr::Await { expr, span } => {
                self.compile_expr(expr)?;

                self.emit_op(OpCode::Await, *span);
            }
            Expr::Return { value, span } => {
                if let Some(v) = value {
                    self.compile_expr(v)?;
                } else {
                    self.emit_op(OpCode::Null, *span);
                }
                self.emit_op(OpCode::Return, *span);
            }
            Expr::Throw { value, span } => {
                self.compile_expr(value)?;
                self.emit_op(OpCode::Throw, *span);
            }
            Expr::Break { span } => {
                self.compile_break(*span)?;
            }
            Expr::Continue { span } => {
                self.compile_continue(*span)?;
            }
            Expr::Spread { expr, span } => {
                self.compile_expr(expr)?;

                self.emit_op(OpCode::SpreadArray, *span);
            }
            Expr::Range {
                start,
                end,
                inclusive,
                span,
            } => {
                self.compile_expr(start)?;
                self.compile_expr(end)?;

                if *inclusive {
                    self.emit_op(OpCode::BuildRangeInclusive, *span);
                } else {
                    self.emit_op(OpCode::BuildRangeExclusive, *span);
                }
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
        self.begin_scope();
        self.compile_expr(value)?;

        let value_slot = self.add_local_unnamed();

        let mut end_jumps = Vec::new();

        for arm in arms {
            if arm.patterns.len() == 1 {
                let pattern = &arm.patterns[0];

                self.begin_scope();

                let locals_before_test = self.current_scope().locals.len();

                let match_result = self.compile_pattern_test(value_slot, pattern, span)?;

                if let Some(success_jump) = match_result {
                    self.emit_op(OpCode::Pop, span);

                    let locals_to_pop = self.current_scope().locals.len() - locals_before_test;
                    for _ in 0..locals_to_pop {
                        self.emit_op(OpCode::Pop, span);
                    }

                    let next_arm_jump = self.emit_jump(OpCode::Jump, span);

                    self.patch_jump(success_jump);
                    self.emit_op(OpCode::Pop, span);

                    self.compile_pattern_bindings(value_slot, pattern, span)?;
                    self.compile_expr(&arm.body)?;

                    self.end_scope_keep_result();
                    end_jumps.push(self.emit_jump(OpCode::Jump, span));

                    self.patch_jump(next_arm_jump);
                } else {
                    self.compile_pattern_bindings(value_slot, pattern, span)?;
                    self.compile_expr(&arm.body)?;
                    self.end_scope_keep_result();
                    end_jumps.push(self.emit_jump(OpCode::Jump, span));
                }
            } else {
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

                let next_arm_jump = self.emit_jump(OpCode::Jump, span);

                for jump in success_jumps {
                    self.patch_jump(jump);
                }
                self.emit_op(OpCode::Pop, span);

                self.begin_scope();
                self.compile_expr(&arm.body)?;
                self.end_scope_keep_result();
                end_jumps.push(self.emit_jump(OpCode::Jump, span));

                self.patch_jump(next_arm_jump);
            }
        }

        if let Some(default_expr) = default {
            self.begin_scope();
            self.compile_expr(default_expr)?;
            self.end_scope_keep_result();
        } else {
            self.emit_op(OpCode::Null, span);
        }

        for jump in end_jumps {
            self.patch_jump(jump);
        }

        self.end_scope_keep_result();

        Ok(())
    }

    fn compile_pattern_test(
        &mut self,
        value_slot: usize,
        pattern: &Pattern,
        span: Span,
    ) -> SaldResult<Option<usize>> {
        match pattern {
            Pattern::Literal { value, .. } => {
                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);
                self.compile_literal(value, span)?;
                self.emit_op(OpCode::Equal, span);
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }

            Pattern::Binding { name, guard, .. } => {
                if let Some(guard_expr) = guard {
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);

                    self.declare_local(name, span)?;
                    self.mark_initialized();

                    self.compile_expr(guard_expr)?;
                    Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
                } else {
                    Ok(None)
                }
            }

            Pattern::Array { elements, .. } => {
                let has_rest = elements
                    .iter()
                    .any(|e| matches!(e, SwitchArrayElement::Rest { .. }));
                let non_rest_count = elements
                    .iter()
                    .filter(|e| matches!(e, SwitchArrayElement::Single(_)))
                    .count();

                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);
                let length_idx = self
                    .current_chunk()
                    .add_constant(Constant::String(intern(&"length".to_string())));
                self.emit_op(OpCode::Invoke, span);
                self.emit_u16(length_idx as u16, span);
                self.emit_u16(0, span);

                let expected_len = if has_rest {
                    non_rest_count
                } else {
                    elements.len()
                };
                let len_const = self
                    .current_chunk()
                    .add_constant(Constant::Number(expected_len as f64));
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
                if entries.is_empty() {
                    self.emit_op(OpCode::True, span);
                } else {
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    let key = &entries[0].0;
                    let key_const = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&key.clone())));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(key_const as u16, span);
                    self.emit_op(OpCode::GetIndex, span);
                    self.emit_op(OpCode::Null, span);
                    self.emit_op(OpCode::NotEqual, span);
                }
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }

            Pattern::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);

                self.compile_expr(start)?;

                self.emit_op(OpCode::GreaterEqual, span);

                let first_test_jump = self.emit_jump(OpCode::JumpIfFalse, span);

                self.emit_op(OpCode::Pop, span);

                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);

                self.compile_expr(end)?;

                if *inclusive {
                    self.emit_op(OpCode::LessEqual, span);
                } else {
                    self.emit_op(OpCode::Less, span);
                }

                let second_test_jump = self.emit_jump(OpCode::JumpIfTrue, span);

                self.patch_jump(first_test_jump);

                Ok(Some(second_test_jump))
            }

            Pattern::Expression { expr, .. } => {
                self.emit_op(OpCode::GetLocal, span);
                self.emit_u16(value_slot as u16, span);
                self.compile_expr(expr)?;
                self.emit_op(OpCode::Equal, span);
                Ok(Some(self.emit_jump(OpCode::JumpIfTrue, span)))
            }
        }
    }

    fn compile_pattern_bindings(
        &mut self,
        value_slot: usize,
        pattern: &Pattern,
        span: Span,
    ) -> SaldResult<()> {
        match pattern {
            Pattern::Literal { .. } => Ok(()),

            Pattern::Binding { name, guard, .. } => {
                if guard.is_none() {
                    self.declare_local(name, span)?;
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    self.mark_initialized();
                }

                Ok(())
            }

            Pattern::Array { elements, .. } => {
                let mut idx = 0;
                for element in elements {
                    match element {
                        SwitchArrayElement::Single(sub_pattern) => {
                            self.emit_op(OpCode::GetLocal, span);
                            self.emit_u16(value_slot as u16, span);
                            let idx_const = self
                                .current_chunk()
                                .add_constant(Constant::Number(idx as f64));
                            self.emit_op(OpCode::Constant, span);
                            self.emit_u16(idx_const as u16, span);
                            self.emit_op(OpCode::GetIndex, span);

                            let temp_slot = self.add_local_unnamed();

                            self.compile_pattern_bindings(temp_slot, sub_pattern, span)?;
                            idx += 1;
                        }
                        SwitchArrayElement::Rest { name, .. } => {
                            self.declare_local(name, span)?;
                            self.emit_op(OpCode::GetLocal, span);
                            self.emit_u16(value_slot as u16, span);
                            let idx_const = self
                                .current_chunk()
                                .add_constant(Constant::Number(idx as f64));
                            self.emit_op(OpCode::Constant, span);
                            self.emit_u16(idx_const as u16, span);
                            let slice_idx = self
                                .current_chunk()
                                .add_constant(Constant::String(intern(&"slice".to_string())));
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
                    self.emit_op(OpCode::GetLocal, span);
                    self.emit_u16(value_slot as u16, span);
                    let key_const = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&key.clone())));
                    self.emit_op(OpCode::Constant, span);
                    self.emit_u16(key_const as u16, span);
                    self.emit_op(OpCode::GetIndex, span);

                    let temp_slot = self.add_local_unnamed();

                    self.compile_pattern_bindings(temp_slot, sub_pattern, span)?;
                }
                Ok(())
            }

            Pattern::Range { .. } => Ok(()),

            Pattern::Expression { .. } => Ok(()),
        }
    }

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
        match value {
            Literal::Number(n) => {
                let const_idx = self.current_chunk().add_constant(Constant::Number(*n));
                self.emit_op(OpCode::Constant, span);
                self.emit_u16(const_idx as u16, span);
            }
            Literal::String(s) => {
                let const_idx = self
                    .current_chunk()
                    .add_constant(Constant::String(intern(&s.clone())));
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
        if let Some(slot) = self.resolve_local(name) {
            self.emit_op(OpCode::GetLocal, span);
            self.emit_u16(slot as u16, span);
        } else if let Some(upvalue) = self.resolve_upvalue(self.scopes.len() - 1, name) {
            self.emit_op(OpCode::GetUpvalue, span);
            self.emit_u16(upvalue as u16, span);
        } else {
            let const_idx = self
                .current_chunk()
                .add_constant(Constant::String(intern(&name.to_string())));
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
                self.compile_expr(left)?;
                let end_jump = self.emit_jump(OpCode::JumpIfNotNull, span);
                self.emit_op(OpCode::Pop, span);
                self.compile_expr(right)?;
                self.patch_jump(end_jump);
                return Ok(());
            }
            _ => {}
        }

        if let Some(result) = self.try_fold_binary(left, op, right) {
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
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&s)));
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
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&s)));
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
        match target {
            Expr::Identifier { name, .. } => {
                if op.is_compound() {
                    self.compile_identifier(name, span)?;
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

                if let Some(slot) = self.resolve_local(name) {
                    self.emit_op(OpCode::SetLocal, span);
                    self.emit_u16(slot as u16, span);
                } else if let Some(upvalue) = self.resolve_upvalue(self.scopes.len() - 1, name) {
                    self.emit_op(OpCode::SetUpvalue, span);
                    self.emit_u16(upvalue as u16, span);
                } else {
                    let const_idx = self
                        .current_chunk()
                        .add_constant(Constant::String(intern(&name.clone())));
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
                        .add_constant(Constant::String(intern(&property.clone())));
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
                    .add_constant(Constant::String(intern(&property.clone())));
                self.emit_op(OpCode::SetProperty, span);
                self.emit_u16(const_idx as u16, span);
            }
            Expr::Index { object, index, .. } => {
                self.compile_expr(object)?;
                self.compile_expr(index)?;

                if op.is_compound() {
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
        self.compile_expr(object)?;
        self.compile_expr(value)?;

        let const_idx = self
            .current_chunk()
            .add_constant(Constant::String(intern(property)));
        self.emit_op(OpCode::SetProperty, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    fn begin_scope(&mut self) {
        self.current_scope_mut().scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.current_scope_mut().scope_depth -= 1;

        while {
            let scope = self.current_scope();
            !scope.locals.is_empty() && scope.locals.last().unwrap().depth > scope.scope_depth
        } {
            let is_captured = self.current_scope().locals.last().unwrap().is_captured;

            if is_captured {
                self.emit_op(OpCode::CloseUpvalue, Span::default());
            } else {
                self.emit_op(OpCode::Pop, Span::default());
            }
            self.current_scope_mut().locals.pop();
        }
    }

    fn end_scope_keep_result(&mut self) {
        self.current_scope_mut().scope_depth -= 1;

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

        for is_captured in locals_to_pop {
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

    fn resolve_upvalue(&mut self, scope_idx: usize, name: &str) -> Option<usize> {
        if scope_idx == 0 {
            return None;
        }

        let enclosing_idx = scope_idx - 1;

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
            self.scopes[enclosing_idx].locals[local].is_captured = true;

            return Some(self.add_upvalue(scope_idx, local, true));
        }

        if let Some(upvalue) = self.resolve_upvalue(enclosing_idx, name) {
            return Some(self.add_upvalue(scope_idx, upvalue, false));
        }

        None
    }

    fn add_upvalue(&mut self, scope_idx: usize, index: usize, is_local: bool) -> usize {
        let scope = &mut self.scopes[scope_idx];

        for (i, upvalue) in scope.upvalues.iter().enumerate() {
            if upvalue.index == index && upvalue.is_local == is_local {
                return i;
            }
        }

        scope.upvalues.push(Upvalue { index, is_local });
        scope.upvalues.len() - 1
    }

    fn compile_break(&mut self, span: Span) -> SaldResult<()> {
        if self.current_scope().break_jumps.is_empty() {
            return Err(SaldError::syntax_error(
                "'break' outside of loop",
                span,
                &self.file,
            ));
        }

        let target_depth = *self.current_scope().loop_scope_depths.last().unwrap();

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

        let break_jump = self.emit_jump(OpCode::Jump, span);

        let scope = self.current_scope_mut();
        if let Some(breaks) = scope.break_jumps.last_mut() {
            breaks.push(break_jump);
        }

        Ok(())
    }

    fn compile_continue(&mut self, span: Span) -> SaldResult<()> {
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

        self.emit_loop(loop_start, span);

        Ok(())
    }

    fn compile_import(&mut self, path: &str, alias: Option<&str>, span: Span) -> SaldResult<()> {
        let path_const = self
            .current_chunk()
            .add_constant(Constant::String(intern(path)));

        if let Some(alias) = alias {
            let alias_const = self
                .current_chunk()
                .add_constant(Constant::String(intern(alias)));
            self.emit_op(OpCode::ImportAs, span);
            self.emit_u16(path_const as u16, span);

            self.emit_u16(alias_const as u16, span);
        } else {
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
        self.compile_expr(condition)?;

        let else_jump = self.emit_jump(OpCode::JumpIfFalse, span);
        self.emit_op(OpCode::Pop, span);

        self.compile_expr(then_expr)?;

        let end_jump = self.emit_jump(OpCode::Jump, span);

        self.patch_jump(else_jump);
        self.emit_op(OpCode::Pop, span);

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
        let lambda_name = format!("<lambda@{}:{}>", span.start.line, span.start.column);

        self.scopes.push(FunctionScope::new(false));
        self.begin_scope();

        for param in params {
            self.declare_local(&param.name, param.span)?;
            self.mark_initialized();
        }

        match body {
            LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.compile_stmt(stmt)?;
                }

                self.emit_op(OpCode::Null, span);
                self.emit_op(OpCode::Return, span);
            }
            LambdaBody::Expr(expr) => {
                self.compile_expr(expr)?;
                self.emit_op(OpCode::Return, span);
            }
        }

        self.end_scope();

        let func_scope = self.scopes.pop().unwrap();
        let arity = params.len();

        let is_variadic = params.last().map(|p| p.is_variadic).unwrap_or(false);

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
            decorators: Vec::new(),
            namespace_context: self.current_namespace.clone(),
            class_context: self.current_class.clone(),
        });

        let const_idx = self.current_chunk().add_constant(func_const);

        self.emit_op(OpCode::Closure, span);
        self.emit_u16(const_idx as u16, span);

        Ok(())
    }

    fn compile_super(&mut self, method: &str, span: Span) -> SaldResult<()> {
        if self.class_depth == 0 {
            return Err(SaldError::syntax_error(
                "'super' used outside of class",
                span,
                &self.file,
            ));
        }

        self.emit_op(OpCode::GetSelf, span);

        let method_const = self
            .current_chunk()
            .add_constant(Constant::String(intern(method)));
        self.emit_op(OpCode::GetSuper, span);
        self.emit_u16(method_const as u16, span);

        Ok(())
    }

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

    fn try_fold_binary(&self, left: &Expr, op: &BinaryOp, right: &Expr) -> Option<FoldedValue> {
        let left_lit = self.extract_literal(left)?;
        let right_lit = self.extract_literal(right)?;

        match (left_lit, right_lit) {
            (FoldedValue::Number(a), FoldedValue::Number(b)) => match op {
                BinaryOp::Add => Some(FoldedValue::Number(a + b)),
                BinaryOp::Sub => Some(FoldedValue::Number(a - b)),
                BinaryOp::Mul => Some(FoldedValue::Number(a * b)),
                BinaryOp::Div if b != 0.0 => Some(FoldedValue::Number(a / b)),
                BinaryOp::Mod if b != 0.0 => Some(FoldedValue::Number(a % b)),

                BinaryOp::Less => Some(FoldedValue::Boolean(a < b)),
                BinaryOp::LessEqual => Some(FoldedValue::Boolean(a <= b)),
                BinaryOp::Greater => Some(FoldedValue::Boolean(a > b)),
                BinaryOp::GreaterEqual => Some(FoldedValue::Boolean(a >= b)),
                BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),

                BinaryOp::BitAnd => Some(FoldedValue::Number((a as i64 & b as i64) as f64)),
                BinaryOp::BitOr => Some(FoldedValue::Number((a as i64 | b as i64) as f64)),
                BinaryOp::BitXor => Some(FoldedValue::Number((a as i64 ^ b as i64) as f64)),
                BinaryOp::LeftShift => Some(FoldedValue::Number(((a as i64) << (b as u32)) as f64)),
                BinaryOp::RightShift => {
                    Some(FoldedValue::Number(((a as i64) >> (b as u32)) as f64))
                }
                _ => None,
            },

            (FoldedValue::String(a), FoldedValue::String(b)) if matches!(op, BinaryOp::Add) => {
                Some(FoldedValue::String(format!("{}{}", a, b)))
            }

            (FoldedValue::Boolean(a), FoldedValue::Boolean(b)) => match op {
                BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),
                _ => None,
            },

            (FoldedValue::String(a), FoldedValue::String(b)) => match op {
                BinaryOp::Equal => Some(FoldedValue::Boolean(a == b)),
                BinaryOp::NotEqual => Some(FoldedValue::Boolean(a != b)),
                _ => None,
            },
            _ => None,
        }
    }

    fn try_fold_unary(&self, op: &UnaryOp, operand: &Expr) -> Option<FoldedValue> {
        let value = self.extract_literal(operand)?;

        match (op, value) {
            (UnaryOp::Negate, FoldedValue::Number(n)) => Some(FoldedValue::Number(-n)),
            (UnaryOp::Not, FoldedValue::Boolean(b)) => Some(FoldedValue::Boolean(!b)),
            (UnaryOp::BitNot, FoldedValue::Number(n)) => {
                Some(FoldedValue::Number(!(n as i64) as f64))
            }
            _ => None,
        }
    }

    fn extract_literal(&self, expr: &Expr) -> Option<FoldedValue> {
        match expr {
            Expr::Literal { value, .. } => match value {
                Literal::Number(n) => Some(FoldedValue::Number(*n)),
                Literal::Boolean(b) => Some(FoldedValue::Boolean(*b)),
                Literal::String(s) => Some(FoldedValue::String(s.clone())),
                Literal::Null => None,
            },
            Expr::Grouping { expr, .. } => self.extract_literal(expr),
            Expr::Unary { op, operand, .. } => self.try_fold_unary(op, operand),
            Expr::Binary {
                left, op, right, ..
            } => self.try_fold_binary(left, op, right),
            _ => None,
        }
    }
}
