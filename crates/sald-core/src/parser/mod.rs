// Sald Parser
// Recursive descent parser that converts tokens into an AST

use crate::ast::*;
use crate::error::{SaldError, SaldResult, Span};
use crate::lexer::{Token, TokenKind};

/// Recursive descent parser for Sald
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    file: String,
    source: String,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, file: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            tokens,
            current: 0,
            file: file.into(),
            source: source.into(),
        }
    }

    /// Parse the entire program
    pub fn parse(&mut self) -> SaldResult<Program> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.declaration()?);
        }

        Ok(Program::new(statements))
    }

    /// Parse a single expression (for config files like Salad.sald)
    pub fn parse_expression_only(&mut self) -> SaldResult<Expr> {
        self.expression()
    }

    // ==================== Declarations ====================

    fn declaration(&mut self) -> SaldResult<Stmt> {
        // Check for decorators first
        let decorators = self.parse_decorators()?;

        if self.check(&TokenKind::Let) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to variable declarations"));
            }
            self.let_declaration()
        } else if self.check(&TokenKind::Const) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to constant declarations"));
            }
            self.const_declaration()
        } else if self.check(&TokenKind::Async) {
            // async fun name() { }
            self.advance(); // consume 'async'
            if self.check(&TokenKind::Fun) {
                self.function_declaration(false, true, decorators)
            } else {
                Err(self
                    .error("Expected 'fun' after 'async'")
                    .with_help("Use 'async fun name() { }' to declare an async function"))
            }
        } else if self.check(&TokenKind::Fun) {
            self.function_declaration(false, false, decorators)
        } else if self.check(&TokenKind::Class) {
            self.class_declaration(decorators)
        } else if self.check(&TokenKind::Namespace) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to namespace declarations"));
            }
            self.namespace_declaration()
        } else if self.check(&TokenKind::Enum) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to enum declarations"));
            }
            self.enum_declaration()
        } else if self.check(&TokenKind::Interface) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to interface declarations"));
            }
            self.interface_declaration()
        } else if self.check(&TokenKind::Import) {
            if !decorators.is_empty() {
                return Err(self.error("Decorators cannot be applied to import statements"));
            }
            self.import_statement()
        } else {
            if !decorators.is_empty() {
                return Err(self.error("Decorators can only be applied to functions and classes"));
            }
            self.statement()
        }
    }

    /// Parse decorators: @name or @name(args)
    fn parse_decorators(&mut self) -> SaldResult<Vec<Decorator>> {
        let mut decorators = Vec::new();

        while self.check(&TokenKind::At) {
            let start = self.advance().span; // consume @
            let name_token = self.consume_identifier("Expected decorator name after '@'")?;
            let name = name_token.lexeme.clone();

            // Optional arguments
            let args = if self.match_token(&TokenKind::LeftParen) {
                let mut args = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.match_token(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.consume(
                    &TokenKind::RightParen,
                    "Expected ')' after decorator arguments",
                )?;
                args
            } else {
                Vec::new()
            };

            decorators.push(Decorator {
                name,
                args,
                span: start,
            });
        }

        Ok(decorators)
    }

    fn let_declaration(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'let'

        // Check for array destructuring: let [a, b, c] = arr
        if self.check(&TokenKind::LeftBracket) {
            return self.parse_array_destructure(start_span);
        }

        // Check for 'self.property' pattern
        if self.check(&TokenKind::SelfKeyword) {
            self.advance(); // consume 'self'
            self.consume(&TokenKind::Dot, "Expected '.' after 'self'")?;

            let property_token = self.consume_identifier("Expected property name after 'self.'")?;
            let property_name = format!("self.{}", property_token.lexeme);
            let property_span = property_token.span; // Copy span before mutable borrow

            self.consume(&TokenKind::Equal, "Expected '=' after property name")?;
            let initializer = self.expression()?;

            let end_span = self.previous().span;

            return Ok(Stmt::Let {
                name: property_name,
                name_span: property_span,
                initializer: Some(initializer),
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            });
        }

        let name_token = self.consume_identifier("Expected variable name")?;
        let name = name_token.lexeme.clone();
        let name_span = name_token.span; // Copy span before mutable borrow

        let initializer = if self.match_token(&TokenKind::Equal) {
            Some(self.expression()?)
        } else {
            None
        };

        let end_span = self.previous().span;

        Ok(Stmt::Let {
            name,
            name_span,
            initializer,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse array destructuring: let [a, b, ...rest] = arr
    fn parse_array_destructure(&mut self, start_span: Span) -> SaldResult<Stmt> {
        use crate::ast::{ArrayPattern, ArrayPatternElement};

        self.advance(); // consume '['

        let mut elements = Vec::new();

        while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
            // Check for rest pattern: ...name
            if self.match_token(&TokenKind::DotDotDot) {
                let name_token = self.consume_identifier("Expected variable name after '...'")?;
                elements.push(ArrayPatternElement::Rest {
                    name: name_token.lexeme.clone(),
                    span: name_token.span,
                });
            }
            // Check for hole (empty slot): let [a, , b] = arr
            else if self.check(&TokenKind::Comma) {
                elements.push(ArrayPatternElement::Hole);
            }
            // Regular variable
            else {
                let name_token =
                    self.consume_identifier("Expected variable name in destructuring pattern")?;
                elements.push(ArrayPatternElement::Variable {
                    name: name_token.lexeme.clone(),
                    span: name_token.span,
                });
            }

            // Consume comma if not at end
            if !self.check(&TokenKind::RightBracket) {
                self.consume(&TokenKind::Comma, "Expected ',' between pattern elements")?;
            }
        }

        let bracket_span = self.consume(
            &TokenKind::RightBracket,
            "Expected ']' after destructuring pattern",
        )?;

        let pattern = ArrayPattern {
            elements,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                bracket_span.span.end.line,
                bracket_span.span.end.column,
            ),
        };

        self.consume(
            &TokenKind::Equal,
            "Expected '=' after destructuring pattern",
        )?;
        let initializer = self.expression()?;
        let end_span = self.previous().span;

        Ok(Stmt::LetDestructure {
            pattern,
            initializer,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn function_declaration(
        &mut self,
        is_static: bool,
        is_async: bool,
        decorators: Vec<Decorator>,
    ) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'fun'

        let name_token = self.consume_identifier("Expected function name")?;
        let name = name_token.lexeme.clone();

        self.consume(&TokenKind::LeftParen, "Expected '(' after function name")?;

        let params = self.parse_parameters()?;

        self.consume(&TokenKind::RightParen, "Expected ')' after parameters")?;
        self.consume(&TokenKind::LeftBrace, "Expected '{' before function body")?;

        let body = self.block_statements()?;
        let end_span = self.previous().span;

        Ok(Stmt::Function {
            def: FunctionDef {
                name,
                params,
                body,
                is_static,
                is_async,
                decorators,
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            },
        })
    }

    fn parse_parameters(&mut self) -> SaldResult<Vec<FunctionParam>> {
        let mut params = Vec::new();
        let mut found_variadic = false;
        let mut found_default = false; // Track if we've seen a default param

        if !self.check(&TokenKind::RightParen) {
            loop {
                // Check for variadic parameter: ...args
                let is_variadic = self.match_token(&TokenKind::DotDotDot);

                if found_variadic {
                    return Err(self
                        .error("Variadic parameter must be the last parameter")
                        .with_help("Only one variadic parameter is allowed, and it must be last"));
                }

                if is_variadic {
                    found_variadic = true;
                }

                // Allow 'self' as a parameter name for methods
                let (param_name, param_span) = if self.check(&TokenKind::SelfKeyword) {
                    let tok = self.advance();
                    (tok.lexeme.clone(), tok.span)
                } else {
                    let tok = self.consume_identifier("Expected parameter name")?;
                    (tok.lexeme.clone(), tok.span)
                };

                // Check for default value: param = expr
                let default_value = if self.match_token(&TokenKind::Equal) {
                    found_default = true;
                    Some(self.expression()?)
                } else {
                    // Required param after optional is an error
                    if found_default && !is_variadic {
                        return Err(self
                            .error("Required parameter cannot follow optional parameter")
                            .with_help("Move parameters with default values to the end"));
                    }
                    None
                };

                params.push(FunctionParam {
                    name: param_name,
                    is_variadic,
                    default_value,
                    span: param_span,
                });

                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
        }

        Ok(params)
    }

    fn class_declaration(&mut self, decorators: Vec<Decorator>) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'class'

        let name_token = self.consume_identifier("Expected class name")?;
        let name = name_token.lexeme.clone();

        // Check for extends
        let superclass = if self.match_token(&TokenKind::Extends) {
            let superclass_token =
                self.consume_identifier("Expected superclass name after 'extends'")?;
            Some(superclass_token.lexeme.clone())
        } else {
            None
        };

        // Check for implements
        let implements = if self.match_token(&TokenKind::Implements) {
            let mut interfaces = Vec::new();
            loop {
                let interface_token = self.consume_identifier("Expected interface name")?;
                interfaces.push(interface_token.lexeme.clone());
                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
            interfaces
        } else {
            Vec::new()
        };

        self.consume(&TokenKind::LeftBrace, "Expected '{' before class body")?;

        let mut methods = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Parse method decorators
            let method_decorators = self.parse_decorators()?;

            // Check for async fun in class methods
            let is_async = self.match_token(&TokenKind::Async);

            if !self.check(&TokenKind::Fun) {
                return Err(self
                    .error("Expected 'fun' for method definition")
                    .with_help("Class body can only contain method definitions"));
            }

            if let Stmt::Function { def } =
                self.function_declaration(false, is_async, method_decorators)?
            {
                // Auto-detect static: method is static if first param is NOT 'self'
                let is_static = def.params.first().map(|p| p.name != "self").unwrap_or(true);
                methods.push(FunctionDef { is_static, ..def });
            }
        }

        self.consume(&TokenKind::RightBrace, "Expected '}' after class body")?;
        let end_span = self.previous().span;

        Ok(Stmt::Class {
            def: ClassDef {
                name,
                superclass,
                implements,
                methods,
                decorators,
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            },
        })
    }

    fn import_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'import'

        // Expect string literal for path
        let path_token = self.peek().clone();
        let path = match &path_token.kind {
            TokenKind::String(s) => s.clone(),
            _ => return Err(self.error("Expected string literal for import path")),
        };
        self.advance();

        // Check for optional 'as' alias
        let alias = if self.match_token(&TokenKind::As) {
            let alias_token = self.consume_identifier("Expected identifier after 'as'")?;
            Some(alias_token.lexeme.clone())
        } else {
            None
        };

        let end_span = self.previous().span;

        Ok(Stmt::Import {
            path,
            alias,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse const declaration: const NAME = value
    fn const_declaration(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'const'

        let name_token = self.consume_identifier("Expected constant name")?;
        let name = name_token.lexeme.clone();

        self.consume(&TokenKind::Equal, "Expected '=' after constant name")?;
        let value = self.expression()?;
        let end_span = self.previous().span;

        Ok(Stmt::Const {
            name,
            value,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse namespace declaration: namespace Name { ... }
    fn namespace_declaration(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'namespace'

        let name_token = self.consume_identifier("Expected namespace name")?;
        let name = name_token.lexeme.clone();

        self.consume(&TokenKind::LeftBrace, "Expected '{' after namespace name")?;

        // Parse body - can contain let, const, fun, class, namespace
        let mut body = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            body.push(self.declaration()?);
        }

        self.consume(&TokenKind::RightBrace, "Expected '}' after namespace body")?;
        let end_span = self.previous().span;

        Ok(Stmt::Namespace {
            name,
            body,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse enum declaration: enum Name { Variant1, Variant2, ... }
    fn enum_declaration(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'enum'

        let name_token = self.consume_identifier("Expected enum name")?;
        let name = name_token.lexeme.clone();

        self.consume(&TokenKind::LeftBrace, "Expected '{' after enum name")?;

        let mut variants = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let variant_token = self.consume_identifier("Expected enum variant name")?;
            variants.push(variant_token.lexeme.clone());

            // Optional comma between variants
            if !self.check(&TokenKind::RightBrace) {
                self.match_token(&TokenKind::Comma);
            }
        }

        self.consume(&TokenKind::RightBrace, "Expected '}' after enum variants")?;
        let end_span = self.previous().span;

        Ok(Stmt::Enum {
            name,
            variants,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse interface declaration: interface Name { fun method(self, ...) }
    fn interface_declaration(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'interface'

        let name_token = self.consume_identifier("Expected interface name")?;
        let name = name_token.lexeme.clone();

        self.consume(&TokenKind::LeftBrace, "Expected '{' after interface name")?;

        let mut methods = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Parse method signature: fun name(params)
            if !self.check(&TokenKind::Fun) {
                return Err(self
                    .error("Expected 'fun' for method signature in interface")
                    .with_help("Interface can only contain method signatures"));
            }

            let method_start = self.advance().span; // consume 'fun'
            let method_name_token = self.consume_identifier("Expected method name")?;
            let method_name = method_name_token.lexeme.clone();

            self.consume(&TokenKind::LeftParen, "Expected '(' after method name")?;
            let params = self.parse_parameters()?;
            self.consume(&TokenKind::RightParen, "Expected ')' after parameters")?;

            let method_end = self.previous().span;

            methods.push(InterfaceMethodDef {
                name: method_name,
                params,
                span: Span::from_positions(
                    method_start.start.line,
                    method_start.start.column,
                    method_end.end.line,
                    method_end.end.column,
                ),
            });
        }

        self.consume(&TokenKind::RightBrace, "Expected '}' after interface body")?;
        let end_span = self.previous().span;

        Ok(Stmt::Interface {
            def: InterfaceDef {
                name,
                methods,
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            },
        })
    }

    // ==================== Statements ====================

    fn statement(&mut self) -> SaldResult<Stmt> {
        if self.check(&TokenKind::If) {
            self.if_statement()
        } else if self.check(&TokenKind::While) {
            self.while_statement()
        } else if self.check(&TokenKind::Do) {
            self.do_while_statement()
        } else if self.check(&TokenKind::For) {
            self.for_statement()
        } else if self.check(&TokenKind::Return) {
            self.return_statement()
        } else if self.check(&TokenKind::Break) {
            self.break_statement()
        } else if self.check(&TokenKind::Continue) {
            self.continue_statement()
        } else if self.check(&TokenKind::Try) {
            self.try_catch_statement()
        } else if self.check(&TokenKind::Throw) {
            self.throw_statement()
        } else {
            // LeftBrace goes here - parse_dictionary_or_block() handles disambiguation
            self.expression_statement()
        }
    }

    fn break_statement(&mut self) -> SaldResult<Stmt> {
        let token = self.advance(); // consume 'break'
        Ok(Stmt::Break { span: token.span })
    }

    fn continue_statement(&mut self) -> SaldResult<Stmt> {
        let token = self.advance(); // consume 'continue'
        Ok(Stmt::Continue { span: token.span })
    }

    fn if_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'if'

        let condition = self.expression()?;

        self.consume(&TokenKind::LeftBrace, "Expected '{' after if condition")?;
        let then_statements = self.block_statements()?;
        let then_branch = Box::new(Stmt::Block {
            statements: then_statements,
            span: self.previous().span,
        });

        let else_branch = if self.match_token(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                // else if
                Some(Box::new(self.if_statement()?))
            } else {
                // else
                self.consume(&TokenKind::LeftBrace, "Expected '{' after 'else'")?;
                let else_statements = self.block_statements()?;
                Some(Box::new(Stmt::Block {
                    statements: else_statements,
                    span: self.previous().span,
                }))
            }
        } else {
            None
        };

        let end_span = self.previous().span;

        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn while_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'while'

        let condition = self.expression()?;

        self.consume(&TokenKind::LeftBrace, "Expected '{' after while condition")?;
        let body_statements = self.block_statements()?;
        let end_span = self.previous().span;

        Ok(Stmt::While {
            condition,
            body: Box::new(Stmt::Block {
                statements: body_statements,
                span: end_span,
            }),
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn do_while_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'do'

        self.consume(&TokenKind::LeftBrace, "Expected '{' after 'do'")?;
        let body_statements = self.block_statements()?;
        let body = Box::new(Stmt::Block {
            statements: body_statements,
            span: self.previous().span,
        });

        self.consume(&TokenKind::While, "Expected 'while' after do block")?;
        let condition = self.expression()?;
        let end_span = self.previous().span;

        Ok(Stmt::DoWhile {
            body,
            condition,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn for_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'for'

        // Parse: for <variable> in <iterable> { <body> }
        let var_token = self.consume_identifier("Expected variable name after 'for'")?;
        let variable = var_token.lexeme.clone();

        self.consume(&TokenKind::In, "Expected 'in' after loop variable")?;

        let iterable = self.expression()?;

        self.consume(&TokenKind::LeftBrace, "Expected '{' before for loop body")?;
        let body_statements = self.block_statements()?;
        let end_span = self.previous().span;

        let body = Box::new(Stmt::Block {
            statements: body_statements,
            span: end_span,
        });

        Ok(Stmt::For {
            variable,
            iterable,
            body,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn return_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'return'

        let value = if !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Check if there's an expression following
            if !self.check(&TokenKind::Eof) && !self.is_statement_start() {
                Some(self.expression()?)
            } else {
                None
            }
        } else {
            None
        };

        let end_span = self.previous().span;

        Ok(Stmt::Return {
            value,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn expression_statement(&mut self) -> SaldResult<Stmt> {
        let expr = self.expression()?;
        let span = expr.span();
        Ok(Stmt::Expression { expr, span })
    }

    /// Parse try-catch statement: try { ... } catch (e) { ... }
    fn try_catch_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'try'

        // Parse try block
        self.consume(&TokenKind::LeftBrace, "Expected '{' after 'try'")?;
        let try_statements = self.block_statements()?;
        let try_body = Box::new(Stmt::Block {
            statements: try_statements,
            span: self.previous().span,
        });

        // Expect 'catch'
        self.consume(&TokenKind::Catch, "Expected 'catch' after try block")?;

        // Parse catch variable: catch (e) or catch e
        let catch_var = if self.match_token(&TokenKind::LeftParen) {
            let var_name = self
                .consume_identifier("Expected variable name in catch")?
                .lexeme
                .clone();
            self.consume(&TokenKind::RightParen, "Expected ')' after catch variable")?;
            var_name
        } else {
            self.consume_identifier("Expected variable name after 'catch'")?
                .lexeme
                .clone()
        };

        // Parse catch block
        self.consume(&TokenKind::LeftBrace, "Expected '{' before catch block")?;
        let catch_statements = self.block_statements()?;
        let end_span = self.previous().span;
        let catch_body = Box::new(Stmt::Block {
            statements: catch_statements,
            span: end_span,
        });

        Ok(Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    /// Parse throw statement: throw value
    fn throw_statement(&mut self) -> SaldResult<Stmt> {
        let start_span = self.advance().span; // consume 'throw'

        let value = self.expression()?;
        let end_span = value.span();

        Ok(Stmt::Throw {
            value,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.end.line,
                end_span.end.column,
            ),
        })
    }

    fn block_statements(&mut self) -> SaldResult<Vec<Stmt>> {
        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            statements.push(self.declaration()?);
        }

        self.consume(&TokenKind::RightBrace, "Expected '}' after block")?;
        Ok(statements)
    }

    fn is_statement_start(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Let
                | TokenKind::If
                | TokenKind::While
                | TokenKind::Do
                | TokenKind::For
                | TokenKind::Fun
                | TokenKind::Return
                | TokenKind::Class // Note: LeftBrace is NOT here - it can be a dict literal in expression context
        )
    }

    // ==================== Expressions ====================

    fn expression(&mut self) -> SaldResult<Expr> {
        self.assignment()
    }

    fn assignment(&mut self) -> SaldResult<Expr> {
        let expr = self.ternary()?;

        // Check for assignment operators
        if let Some(op) = self.match_assign_op() {
            let value = self.assignment()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                value.span().end.line,
                value.span().end.column,
            );

            if expr.is_lvalue() {
                return Ok(Expr::Assignment {
                    target: Box::new(expr),
                    op,
                    value: Box::new(value),
                    span,
                });
            } else {
                return Err(self
                    .error("Invalid assignment target")
                    .with_help("Can only assign to variables or object properties"));
            }
        }

        Ok(expr)
    }

    fn ternary(&mut self) -> SaldResult<Expr> {
        let expr = self.null_coalesce()?;

        // Check for ternary operator: condition ? then_expr : else_expr
        if self.match_token(&TokenKind::Question) {
            let then_expr = self.ternary()?;
            self.consume(&TokenKind::Colon, "Expected ':' in ternary expression")?;
            let else_expr = self.ternary()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                else_expr.span().end.line,
                else_expr.span().end.column,
            );
            return Ok(Expr::Ternary {
                condition: Box::new(expr),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span,
            });
        }

        Ok(expr)
    }

    fn null_coalesce(&mut self) -> SaldResult<Expr> {
        let mut expr = self.or()?;

        while self.match_token(&TokenKind::QuestionQuestion) {
            let right = self.or()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::NullCoalesce,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn match_assign_op(&mut self) -> Option<AssignOp> {
        let kind = &self.peek().kind;
        let op = AssignOp::from_token(kind);
        if op.is_some() {
            self.advance();
        }
        op
    }

    fn or(&mut self) -> SaldResult<Expr> {
        let mut expr = self.and()?;

        while self.match_token(&TokenKind::Or) {
            let right = self.and()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::Or,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn and(&mut self) -> SaldResult<Expr> {
        let mut expr = self.bit_or()?;

        while self.match_token(&TokenKind::And) {
            let right = self.bit_or()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::And,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    // Bitwise OR has lower precedence than XOR
    fn bit_or(&mut self) -> SaldResult<Expr> {
        let mut expr = self.bit_xor()?;

        while self.match_token(&TokenKind::Pipe) {
            let right = self.bit_xor()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitOr,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    // Bitwise XOR has lower precedence than AND
    fn bit_xor(&mut self) -> SaldResult<Expr> {
        let mut expr = self.bit_and()?;

        while self.match_token(&TokenKind::Caret) {
            let right = self.bit_and()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitXor,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    // Bitwise AND has lower precedence than equality
    fn bit_and(&mut self) -> SaldResult<Expr> {
        let mut expr = self.equality()?;

        while self.match_token(&TokenKind::Ampersand) {
            let right = self.equality()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitAnd,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn equality(&mut self) -> SaldResult<Expr> {
        let mut expr = self.comparison()?;

        while self.check(&TokenKind::EqualEqual) || self.check(&TokenKind::BangEqual) {
            let op = if self.match_token(&TokenKind::EqualEqual) {
                BinaryOp::Equal
            } else {
                self.advance();
                BinaryOp::NotEqual
            };
            let right = self.comparison()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn comparison(&mut self) -> SaldResult<Expr> {
        let mut expr = self.range()?;

        loop {
            let op = if self.match_token(&TokenKind::Less) {
                BinaryOp::Less
            } else if self.match_token(&TokenKind::LessEqual) {
                BinaryOp::LessEqual
            } else if self.match_token(&TokenKind::Greater) {
                BinaryOp::Greater
            } else if self.match_token(&TokenKind::GreaterEqual) {
                BinaryOp::GreaterEqual
            } else {
                break;
            };

            let right = self.range()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    /// Parse range expressions: start..end (inclusive) or start..<end (exclusive)
    fn range(&mut self) -> SaldResult<Expr> {
        let mut expr = self.shift()?;

        // Check for range operators
        if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotLess) {
            let inclusive = self.match_token(&TokenKind::DotDot);
            if !inclusive {
                self.advance(); // consume DotDotLess
            }

            let end = self.shift()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                end.span().end.line,
                end.span().end.column,
            );
            expr = Expr::Range {
                start: Box::new(expr),
                end: Box::new(end),
                inclusive,
                span,
            };
        }

        Ok(expr)
    }

    // Shift operators have lower precedence than term
    fn shift(&mut self) -> SaldResult<Expr> {
        let mut expr = self.term()?;

        loop {
            let op = if self.match_token(&TokenKind::LessLess) {
                BinaryOp::LeftShift
            } else if self.match_token(&TokenKind::GreaterGreater) {
                BinaryOp::RightShift
            } else {
                break;
            };

            let right = self.term()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn term(&mut self) -> SaldResult<Expr> {
        let mut expr = self.factor()?;

        loop {
            let op = if self.match_token(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_token(&TokenKind::Minus) {
                BinaryOp::Sub
            } else {
                break;
            };

            let right = self.factor()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn factor(&mut self) -> SaldResult<Expr> {
        let mut expr = self.unary()?;

        loop {
            let op = if self.match_token(&TokenKind::Star) {
                BinaryOp::Mul
            } else if self.match_token(&TokenKind::Slash) {
                BinaryOp::Div
            } else if self.match_token(&TokenKind::Percent) {
                BinaryOp::Mod
            } else {
                break;
            };

            let right = self.unary()?;
            let span = Span::from_positions(
                expr.span().start.line,
                expr.span().start.column,
                right.span().end.line,
                right.span().end.column,
            );
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn unary(&mut self) -> SaldResult<Expr> {
        // Check for await keyword
        if self.check(&TokenKind::Await) {
            let await_token = self.advance().clone();
            let expr = self.unary()?;
            let span = Span::from_positions(
                await_token.span.start.line,
                await_token.span.start.column,
                expr.span().end.line,
                expr.span().end.column,
            );
            return Ok(Expr::Await {
                expr: Box::new(expr),
                span,
            });
        }

        if self.check(&TokenKind::Bang)
            || self.check(&TokenKind::Minus)
            || self.check(&TokenKind::Tilde)
        {
            let op_token = self.peek().clone();
            let op = UnaryOp::from_token(&op_token.kind).unwrap();
            self.advance();
            let operand = self.unary()?;
            let span = Span::from_positions(
                op_token.span.start.line,
                op_token.span.start.column,
                operand.span().end.line,
                operand.span().end.column,
            );
            return Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
                span,
            });
        }

        self.call()
    }

    fn call(&mut self) -> SaldResult<Expr> {
        let mut expr = self.primary()?;

        loop {
            if self.match_token(&TokenKind::LeftParen) {
                expr = self.finish_call(expr, false)?;
            } else if self.match_token(&TokenKind::Dot) {
                let name_token = self.consume_identifier("Expected property name after '.'")?;
                let span = Span::from_positions(
                    expr.span().start.line,
                    expr.span().start.column,
                    name_token.span.end.line,
                    name_token.span.end.column,
                );
                expr = Expr::Get {
                    object: Box::new(expr),
                    property: name_token.lexeme.clone(),
                    is_optional: false,
                    span,
                };
            } else if self.match_token(&TokenKind::QuestionDot) {
                // Optional chaining: obj?.property
                let name_token = self.consume_identifier("Expected property name after '?.'")?;
                let span = Span::from_positions(
                    expr.span().start.line,
                    expr.span().start.column,
                    name_token.span.end.line,
                    name_token.span.end.column,
                );
                expr = Expr::Get {
                    object: Box::new(expr),
                    property: name_token.lexeme.clone(),
                    is_optional: true,
                    span,
                };
            } else if self.match_token(&TokenKind::LeftBracket) {
                // Index access: arr[i]
                let index = self.expression()?;
                let bracket = self.consume(&TokenKind::RightBracket, "Expected ']' after index")?;
                let span = Span::from_positions(
                    expr.span().start.line,
                    expr.span().start.column,
                    bracket.span.end.line,
                    bracket.span.end.column,
                );
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    is_optional: false,
                    span,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn finish_call(&mut self, callee: Expr, is_optional: bool) -> SaldResult<Expr> {
        let mut args: Vec<CallArg> = Vec::new();
        let mut seen_named = false; // Track if we've seen a named arg

        if !self.check(&TokenKind::RightParen) {
            loop {
                let arg_start_span = self.peek().span;

                // Check for spread operator: ...expr
                if self.match_token(&TokenKind::DotDotDot) {
                    let start_span = self.previous().span;
                    let expr = self.expression()?;
                    let end_span = expr.span();
                    let span = Span::from_positions(
                        start_span.start.line,
                        start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    );
                    args.push(CallArg {
                        name: None,
                        value: Expr::Spread {
                            expr: Box::new(expr),
                            span,
                        },
                        span,
                    });
                } else if self.check_identifier() && self.check_ahead(1, &TokenKind::Colon) {
                    // Named argument: name: value
                    seen_named = true;
                    let name_token = self.advance(); // consume identifier
                    let arg_name = name_token.lexeme.clone();
                    self.advance(); // consume ':'
                    let value = self.expression()?;
                    let end_span = value.span();
                    let span = Span::from_positions(
                        arg_start_span.start.line,
                        arg_start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    );
                    args.push(CallArg {
                        name: Some(arg_name),
                        value,
                        span,
                    });
                } else {
                    // Positional argument
                    if seen_named {
                        return Err(self
                            .error("Positional argument cannot follow named argument")
                            .with_help(
                                "Named arguments must come after all positional arguments",
                            ));
                    }
                    let value = self.expression()?;
                    let span = value.span();
                    args.push(CallArg {
                        name: None,
                        value,
                        span,
                    });
                }

                if !self.match_token(&TokenKind::Comma) {
                    break;
                }
            }
        }

        let paren = self.consume(&TokenKind::RightParen, "Expected ')' after arguments")?;
        let span = Span::from_positions(
            callee.span().start.line,
            callee.span().start.column,
            paren.span.end.line,
            paren.span.end.column,
        );

        Ok(Expr::Call {
            callee: Box::new(callee),
            args,
            is_optional,
            span,
        })
    }

    fn primary(&mut self) -> SaldResult<Expr> {
        let token = self.peek().clone();

        match &token.kind {
            TokenKind::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::Number(n),
                    span: token.span,
                })
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::String(s),
                    span: token.span,
                })
            }
            TokenKind::FormatStringStart(s) => self.parse_format_string(s.clone(), token.span),
            // Raw string literal - same as String but no escape processing (already done in lexer)
            TokenKind::RawString(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::String(s),
                    span: token.span,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::Boolean(true),
                    span: token.span,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::Boolean(false),
                    span: token.span,
                })
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::Null,
                    span: token.span,
                })
            }
            TokenKind::Identifier(_) => {
                self.advance();
                Ok(Expr::Identifier {
                    name: token.lexeme.clone(),
                    span: token.span,
                })
            }
            TokenKind::SelfKeyword => {
                self.advance();
                Ok(Expr::SelfExpr { span: token.span })
            }
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.expression()?;
                let end_token =
                    self.consume(&TokenKind::RightParen, "Expected ')' after expression")?;
                Ok(Expr::Grouping {
                    expr: Box::new(expr),
                    span: Span::from_positions(
                        token.span.start.line,
                        token.span.start.column,
                        end_token.span.end.line,
                        end_token.span.end.column,
                    ),
                })
            }
            TokenKind::LeftBracket => {
                // Array literal: [1, 2, 3]
                self.advance();
                let mut elements = Vec::new();

                if !self.check(&TokenKind::RightBracket) {
                    loop {
                        elements.push(self.expression()?);
                        if !self.match_token(&TokenKind::Comma) {
                            break;
                        }
                    }
                }

                let end_token = self.consume(
                    &TokenKind::RightBracket,
                    "Expected ']' after array elements",
                )?;
                Ok(Expr::Array {
                    elements,
                    span: Span::from_positions(
                        token.span.start.line,
                        token.span.start.column,
                        end_token.span.end.line,
                        end_token.span.end.column,
                    ),
                })
            }
            TokenKind::Pipe => {
                // Lambda: |params| body or |params| { block }
                self.advance(); // consume '|'
                let start_span = token.span;

                let mut params = Vec::new();
                let mut found_variadic = false;
                if !self.check(&TokenKind::Pipe) {
                    loop {
                        // Check for variadic parameter: ...args
                        let is_variadic = self.match_token(&TokenKind::DotDotDot);

                        if found_variadic {
                            return Err(self.error("Variadic parameter must be the last parameter"));
                        }

                        if is_variadic {
                            found_variadic = true;
                        }

                        let param_token = self.consume_identifier("Expected parameter name")?;
                        params.push(FunctionParam {
                            name: param_token.lexeme.clone(),
                            is_variadic,
                            default_value: None,
                            span: param_token.span,
                        });
                        if !self.match_token(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.consume(&TokenKind::Pipe, "Expected '|' after lambda parameters")?;

                // Check if body is a block or expression
                let body = if self.check(&TokenKind::LeftBrace) {
                    // Block body: |x| { statements }
                    self.advance();
                    let statements = self.block_statements()?;
                    LambdaBody::Block(statements)
                } else {
                    // Expression body: |x| expr
                    let expr = self.expression()?;
                    LambdaBody::Expr(Box::new(expr))
                };

                let end_span = self.previous().span;
                Ok(Expr::Lambda {
                    params,
                    body,
                    is_async: false,
                    span: Span::from_positions(
                        start_span.start.line,
                        start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    ),
                })
            }
            TokenKind::Or => {
                // Lambda with empty params: || body or || { block }
                // || is scanned as Or token
                self.advance(); // consume ||
                let start_span = token.span;

                // Check if body is a block or expression
                let body = if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let statements = self.block_statements()?;
                    LambdaBody::Block(statements)
                } else {
                    let expr = self.expression()?;
                    LambdaBody::Expr(Box::new(expr))
                };

                let end_span = self.previous().span;
                Ok(Expr::Lambda {
                    params: Vec::new(),
                    body,
                    is_async: false,
                    span: Span::from_positions(
                        start_span.start.line,
                        start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    ),
                })
            }
            TokenKind::Async => {
                // Async lambda: async || body or async |params| { block }
                let async_token = self.advance().clone();

                // Check for || (Or token - empty params) or | (Pipe token - has params)
                let params = if self.check(&TokenKind::Or) {
                    // async || { } - empty params (|| is scanned as Or token)
                    self.advance(); // consume ||
                    Vec::new()
                } else if self.check(&TokenKind::Pipe) {
                    // async |params| { } - has params
                    self.advance(); // consume first |

                    let mut params = Vec::new();
                    let mut found_variadic = false;
                    if !self.check(&TokenKind::Pipe) {
                        loop {
                            let is_variadic = self.match_token(&TokenKind::DotDotDot);
                            if found_variadic {
                                return Err(
                                    self.error("Variadic parameter must be the last parameter")
                                );
                            }
                            if is_variadic {
                                found_variadic = true;
                            }
                            let param_token = self.consume_identifier("Expected parameter name")?;
                            params.push(FunctionParam {
                                name: param_token.lexeme.clone(),
                                is_variadic,
                                default_value: None,
                                span: param_token.span,
                            });
                            if !self.match_token(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(
                        &TokenKind::Pipe,
                        "Expected '|' after async lambda parameters",
                    )?;
                    params
                } else {
                    return Err(self.error("Expected '|' or '||' after 'async' for async lambda")
                        .with_help("Use 'async |params| { body }' or 'async || { body }' for async lambdas"));
                };

                let body = if self.check(&TokenKind::LeftBrace) {
                    self.advance();
                    let statements = self.block_statements()?;
                    LambdaBody::Block(statements)
                } else {
                    let expr = self.expression()?;
                    LambdaBody::Expr(Box::new(expr))
                };

                let end_span = self.previous().span;
                Ok(Expr::Lambda {
                    params,
                    body,
                    is_async: true,
                    span: Span::from_positions(
                        async_token.span.start.line,
                        async_token.span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    ),
                })
            }
            TokenKind::Super => {
                // super.method()
                self.advance(); // consume 'super'
                let start_span = token.span;
                self.consume(&TokenKind::Dot, "Expected '.' after 'super'")?;
                let method_token =
                    self.consume_identifier("Expected method name after 'super.'")?;
                let end_span = method_token.span;

                Ok(Expr::Super {
                    method: method_token.lexeme.clone(),
                    span: Span::from_positions(
                        start_span.start.line,
                        start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    ),
                })
            }
            TokenKind::Switch => self.parse_switch_expression(),
            TokenKind::LeftBrace => {
                // Could be dictionary literal: {"key": value} or block expression: { stmts; expr }
                // Disambiguate by looking ahead:
                // - If empty `{}` -> empty dictionary
                // - If starts with expression followed by `:` -> dictionary
                // - Otherwise -> block expression
                self.parse_dictionary_or_block()
            }
            _ => Err(self
                .error(&format!("Unexpected token '{}'", token.lexeme))
                .with_help("Expected an expression")),
        }
    }

    /// Parse either a dictionary literal or block expression
    /// Dictionary: {"key": value, ...}
    /// Block: { statements; expr }
    fn parse_dictionary_or_block(&mut self) -> SaldResult<Expr> {
        let start_token = self.advance(); // consume '{'
        let start_span = start_token.span;

        // Empty braces = empty dictionary
        if self.check(&TokenKind::RightBrace) {
            let end_token = self.advance();
            return Ok(Expr::Dictionary {
                entries: Vec::new(),
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_token.span.end.line,
                    end_token.span.end.column,
                ),
            });
        }

        // Look ahead to determine if this is a dictionary
        // If we see a string/expression followed by ':', it's a dictionary
        // Save position for potential backtrack
        let saved_position = self.current;

        // Try to parse as dictionary key
        let is_dictionary = if self.is_dictionary_start() {
            true
        } else {
            // Restore position
            self.current = saved_position;
            false
        };

        if is_dictionary {
            // Restore to parse properly
            self.current = saved_position;
            self.parse_dictionary_entries(start_span)
        } else {
            // Parse as block expression (reuse existing logic)
            self.current = saved_position;
            self.parse_block_expression_contents(start_span)
        }
    }

    /// Check if this looks like a dictionary start
    fn is_dictionary_start(&mut self) -> bool {
        // Check for spread pattern: **expr
        if self.check(&TokenKind::Star) {
            let saved = self.current;
            self.advance(); // consume first *
            if self.check(&TokenKind::Star) {
                self.current = saved;
                return true;
            }
            self.current = saved;
        }

        // Try to parse an expression
        if let Ok(_expr) = self.expression() {
            // Check if followed by colon
            self.check(&TokenKind::Colon)
        } else {
            false
        }
    }

    /// Parse dictionary entries after determining it's a dictionary
    fn parse_dictionary_entries(&mut self, start_span: Span) -> SaldResult<Expr> {
        let mut entries = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Check for dict spread: **dict
            if self.check(&TokenKind::Star) {
                let star_pos = self.current;
                self.advance(); // consume first *
                if self.check(&TokenKind::Star) {
                    self.advance(); // consume second *
                    let spread_expr = self.expression()?;
                    let spread_span = spread_expr.span();
                    // Use null literal as key to indicate spread
                    let key = Expr::Literal {
                        value: Literal::Null,
                        span: spread_span,
                    };
                    let value = Expr::Spread {
                        expr: Box::new(spread_expr),
                        span: spread_span,
                    };
                    entries.push((key, value));
                } else {
                    // Not a spread, restore and parse as normal key
                    self.current = star_pos;
                    let key = self.expression()?;
                    self.consume(&TokenKind::Colon, "Expected ':' after dictionary key")?;
                    let value = self.expression()?;
                    entries.push((key, value));
                }
            } else {
                // Parse key expression
                let key = self.expression()?;

                // Expect colon
                self.consume(&TokenKind::Colon, "Expected ':' after dictionary key")?;

                // Parse value expression
                let value = self.expression()?;

                entries.push((key, value));
            }

            // Optional comma
            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }

        let end_token = self.consume(
            &TokenKind::RightBrace,
            "Expected '}' after dictionary entries",
        )?;

        Ok(Expr::Dictionary {
            entries,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_token.span.end.line,
                end_token.span.end.column,
            ),
        })
    }

    /// Parse block expression contents (after we've determined it's a block)
    fn parse_block_expression_contents(&mut self, start_span: Span) -> SaldResult<Expr> {
        let mut statements = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if self.check(&TokenKind::Let)
                || self.check(&TokenKind::Fun)
                || self.check(&TokenKind::Class)
                || self.check(&TokenKind::If)
                || self.check(&TokenKind::While)
                || self.check(&TokenKind::For)
                || self.check(&TokenKind::Return)
                || self.check(&TokenKind::Break)
                || self.check(&TokenKind::Continue)
                || self.check(&TokenKind::Try)
                || self.check(&TokenKind::Throw)
            {
                let stmt = self.declaration()?;
                statements.push(stmt);
            } else {
                let expr = self.expression()?;
                if self.check(&TokenKind::RightBrace) {
                    final_expr = Some(Box::new(expr));
                } else {
                    let span = expr.span();
                    statements.push(Stmt::Expression { expr, span });
                }
            }
        }

        let end_token = self.consume(
            &TokenKind::RightBrace,
            "Expected '}' after block expression",
        )?;

        Ok(Expr::Block {
            statements,
            expr: final_expr,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_token.span.end.line,
                end_token.span.end.column,
            ),
        })
    }

    /// Parse switch expression: switch value { patterns -> expr, ... }
    fn parse_switch_expression(&mut self) -> SaldResult<Expr> {
        let start_span = self.advance().span; // consume 'switch'

        // Parse the value to match against
        let value = Box::new(self.expression()?);

        // Consume opening brace
        self.consume(&TokenKind::LeftBrace, "Expected '{' after switch value")?;

        let mut arms = Vec::new();
        let mut default_arm: Option<Box<Expr>> = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let arm_start = self.peek().span;

            // Check for 'default' keyword
            if self.check(&TokenKind::Default) {
                self.advance(); // consume 'default'
                self.consume(&TokenKind::ThinArrow, "Expected '->' after 'default'")?;

                let body = self.switch_arm_body()?;
                default_arm = Some(Box::new(body));

                // Optional comma
                self.match_token(&TokenKind::Comma);
            } else {
                // Parse patterns (can be multiple: 1, 2, 3 -> expr)
                let mut patterns = Vec::new();

                loop {
                    let pattern = self.parse_switch_pattern()?;
                    patterns.push(pattern);

                    // Check for comma (multiple patterns) or arrow (end of patterns)
                    if self.check(&TokenKind::ThinArrow) {
                        break;
                    } else if self.match_token(&TokenKind::Comma) {
                        // If next token is arrow, we're done with patterns
                        if self.check(&TokenKind::ThinArrow) {
                            break;
                        }
                        // Otherwise continue collecting patterns
                    } else {
                        return Err(self.error("Expected '->' or ',' in switch arm").with_help(
                            "Use 'pattern -> expression' or 'pattern1, pattern2 -> expression'",
                        ));
                    }
                }

                self.consume(&TokenKind::ThinArrow, "Expected '->' after pattern")?;

                let body = self.switch_arm_body()?;
                let arm_end = body.span();

                arms.push(SwitchArm {
                    patterns,
                    body,
                    span: Span::from_positions(
                        arm_start.start.line,
                        arm_start.start.column,
                        arm_end.end.line,
                        arm_end.end.column,
                    ),
                });

                // Optional comma between arms
                self.match_token(&TokenKind::Comma);
            }
        }

        let end_token = self.consume(&TokenKind::RightBrace, "Expected '}' after switch arms")?;

        Ok(Expr::Switch {
            value,
            arms,
            default: default_arm,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_token.span.end.line,
                end_token.span.end.column,
            ),
        })
    }

    /// Parse a switch pattern
    fn parse_switch_pattern(&mut self) -> SaldResult<Pattern> {
        let start_span = self.peek().span;

        // Check for array pattern [...]
        if self.check(&TokenKind::LeftBracket) {
            return self.parse_switch_array_pattern();
        }

        // Check for dict pattern {...}
        if self.check(&TokenKind::LeftBrace) {
            return self.parse_switch_dict_pattern();
        }

        // Parse as literal or binding
        // Literals: numbers, strings, true, false, null
        match &self.peek().kind {
            TokenKind::Number(n) => {
                let value = *n;
                self.advance();

                // Check for range pattern: num..num or num..<num
                if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotLess) {
                    let inclusive = self.match_token(&TokenKind::DotDot);
                    if !inclusive {
                        self.advance(); // consume DotDotLess
                    }

                    // Parse end value (must be a number for patterns)
                    let end_value = if let TokenKind::Number(n) = &self.peek().kind {
                        *n
                    } else {
                        return Err(self.error("Expected number after range operator in pattern"));
                    };
                    self.advance();

                    let end_span = self.previous().span;
                    return Ok(Pattern::Range {
                        start: Box::new(Expr::Literal {
                            value: Literal::Number(value),
                            span: start_span,
                        }),
                        end: Box::new(Expr::Literal {
                            value: Literal::Number(end_value),
                            span: end_span,
                        }),
                        inclusive,
                        span: Span::from_positions(
                            start_span.start.line,
                            start_span.start.column,
                            end_span.end.line,
                            end_span.end.column,
                        ),
                    });
                }

                Ok(Pattern::Literal {
                    value: Literal::Number(value),
                    span: start_span,
                })
            }
            TokenKind::String(s) => {
                let value = s.clone();
                self.advance();
                Ok(Pattern::Literal {
                    value: Literal::String(value),
                    span: start_span,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Pattern::Literal {
                    value: Literal::Boolean(true),
                    span: start_span,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Pattern::Literal {
                    value: Literal::Boolean(false),
                    span: start_span,
                })
            }
            TokenKind::Null => {
                self.advance();
                Ok(Pattern::Literal {
                    value: Literal::Null,
                    span: start_span,
                })
            }
            TokenKind::Identifier(_) => {
                // Check if this is an expression pattern (e.g., Enum.Member)
                // Look ahead to see if identifier is followed by a dot
                let name = if let TokenKind::Identifier(n) = &self.peek().kind {
                    n.clone()
                } else {
                    return Err(self.error("Expected identifier"));
                };
                self.advance();

                // If followed by a dot, this is an expression pattern (Enum.Member, etc.)
                if self.check(&TokenKind::Dot) {
                    // Build expression starting with the identifier
                    let mut expr = Expr::Identifier {
                        name: name.clone(),
                        span: start_span,
                    };

                    // Keep parsing property accesses
                    while self.match_token(&TokenKind::Dot) {
                        let property = if let TokenKind::Identifier(prop) = &self.peek().kind {
                            prop.clone()
                        } else {
                            return Err(self.error("Expected property name after '.'"));
                        };
                        self.advance();

                        let end_span = self.previous().span;
                        expr = Expr::Get {
                            object: Box::new(expr),
                            property,
                            is_optional: false,
                            span: Span::from_positions(
                                start_span.start.line,
                                start_span.start.column,
                                end_span.end.line,
                                end_span.end.column,
                            ),
                        };
                    }

                    let end_span = self.previous().span;
                    return Ok(Pattern::Expression {
                        expr: Box::new(expr),
                        span: Span::from_positions(
                            start_span.start.line,
                            start_span.start.column,
                            end_span.end.line,
                            end_span.end.column,
                        ),
                    });
                }

                // Variable binding: x, or x if condition
                // Check for guard: x if condition
                let guard = if self.check(&TokenKind::If) {
                    self.advance(); // consume 'if'
                    Some(Box::new(self.expression()?))
                } else {
                    None
                };

                let end_span = self.previous().span;
                Ok(Pattern::Binding {
                    name,
                    guard,
                    span: Span::from_positions(
                        start_span.start.line,
                        start_span.start.column,
                        end_span.end.line,
                        end_span.end.column,
                    ),
                })
            }
            _ => Err(self.error("Expected a pattern (literal, identifier, array, dict, or range)")),
        }
    }

    /// Parse array pattern: [], [a], [a, b], [head, ...tail]
    fn parse_switch_array_pattern(&mut self) -> SaldResult<Pattern> {
        let start_span = self.advance().span; // consume '['

        let mut elements = Vec::new();

        while !self.check(&TokenKind::RightBracket) && !self.is_at_end() {
            // Check for rest element ...name
            if self.check(&TokenKind::DotDotDot) {
                let rest_start = self.advance().span; // consume '...'
                let name = if let TokenKind::Identifier(n) = &self.peek().kind {
                    n.clone()
                } else {
                    return Err(self.error("Expected identifier after '...'"));
                };
                let rest_end = self.advance().span;

                elements.push(SwitchArrayElement::Rest {
                    name,
                    span: Span::from_positions(
                        rest_start.start.line,
                        rest_start.start.column,
                        rest_end.end.line,
                        rest_end.end.column,
                    ),
                });
            } else {
                // Parse a pattern element
                let pattern = self.parse_switch_pattern()?;
                elements.push(SwitchArrayElement::Single(pattern));
            }

            // Check for comma or end
            if !self.check(&TokenKind::RightBracket) {
                self.consume(&TokenKind::Comma, "Expected ',' or ']' in array pattern")?;
            }
        }

        let end_span =
            self.consume(&TokenKind::RightBracket, "Expected ']' after array pattern")?;

        Ok(Pattern::Array {
            elements,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.span.end.line,
                end_span.span.end.column,
            ),
        })
    }

    /// Parse dict pattern: {"key": binding}
    fn parse_switch_dict_pattern(&mut self) -> SaldResult<Pattern> {
        let start_span = self.advance().span; // consume '{'

        let mut entries = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Parse key (must be string literal)
            let key = if let TokenKind::String(s) = &self.peek().kind {
                s.clone()
            } else {
                return Err(self.error("Dict pattern keys must be string literals"));
            };
            self.advance();

            self.consume(&TokenKind::Colon, "Expected ':' after dict key")?;

            // Parse binding pattern
            let pattern = self.parse_switch_pattern()?;

            entries.push((key, pattern));

            // Check for comma or end
            if !self.check(&TokenKind::RightBrace) {
                self.consume(&TokenKind::Comma, "Expected ',' or '}' in dict pattern")?;
            }
        }

        let end_span = self.consume(&TokenKind::RightBrace, "Expected '}' after dict pattern")?;

        Ok(Pattern::Dict {
            entries,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_span.span.end.line,
                end_span.span.end.column,
            ),
        })
    }

    /// Parse a switch arm body - can be a block expression, return/throw/break/continue, or regular expression
    fn switch_arm_body(&mut self) -> SaldResult<Expr> {
        if self.check(&TokenKind::LeftBrace) {
            self.parse_block_expression()
        } else if self.check(&TokenKind::Return) {
            // Parse return as expression
            let start_span = self.advance().span; // consume 'return'
            let value = if !self.check_switch_arm_end() {
                Some(Box::new(self.expression()?))
            } else {
                None
            };
            let end_span = self.previous().span;
            Ok(Expr::Return {
                value,
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            })
        } else if self.check(&TokenKind::Throw) {
            // Parse throw as expression
            let start_span = self.advance().span; // consume 'throw'
            let value = Box::new(self.expression()?);
            let end_span = self.previous().span;
            Ok(Expr::Throw {
                value,
                span: Span::from_positions(
                    start_span.start.line,
                    start_span.start.column,
                    end_span.end.line,
                    end_span.end.column,
                ),
            })
        } else if self.check(&TokenKind::Break) {
            let token = self.advance();
            Ok(Expr::Break { span: token.span })
        } else if self.check(&TokenKind::Continue) {
            let token = self.advance();
            Ok(Expr::Continue { span: token.span })
        } else {
            self.expression()
        }
    }

    /// Check if we're at the end of a switch arm (comma, closing brace, or default keyword)
    fn check_switch_arm_end(&self) -> bool {
        self.check(&TokenKind::Comma)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Default)
            || self.is_at_end()
    }

    /// Parse a block expression: { statements; expr }
    fn parse_block_expression(&mut self) -> SaldResult<Expr> {
        let start_token = self.advance(); // consume '{'
        let start_span = start_token.span;

        let mut statements = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Try to parse a statement
            // If it's an expression statement without semicolon at the end, it's the final expression

            if self.check(&TokenKind::Let)
                || self.check(&TokenKind::Fun)
                || self.check(&TokenKind::Class)
                || self.check(&TokenKind::If)
                || self.check(&TokenKind::While)
                || self.check(&TokenKind::For)
                || self.check(&TokenKind::Return)
                || self.check(&TokenKind::Break)
                || self.check(&TokenKind::Continue)
                || self.check(&TokenKind::Try)
                || self.check(&TokenKind::Throw)
            {
                // It's definitely a statement
                let stmt = self.declaration()?;
                statements.push(stmt);
            } else {
                // Parse as expression
                let expr = self.expression()?;

                // Check if followed by } (final expression) or needs to become expression statement
                if self.check(&TokenKind::RightBrace) {
                    // This is the final expression
                    final_expr = Some(Box::new(expr));
                } else {
                    // Convert to expression statement
                    let span = expr.span();
                    statements.push(Stmt::Expression { expr, span });
                }
            }
        }

        let end_token = self.consume(
            &TokenKind::RightBrace,
            "Expected '}' after block expression",
        )?;

        Ok(Expr::Block {
            statements,
            expr: final_expr,
            span: Span::from_positions(
                start_span.start.line,
                start_span.start.column,
                end_token.span.end.line,
                end_token.span.end.column,
            ),
        })
    }

    // ==================== Helper Methods ====================

    /// Parse format string $"Hello, {name}!" into concatenation expression
    /// FormatStringStart("Hello, ") + expr + FormatStringPart/End("!")
    fn parse_format_string(&mut self, first_part: String, start_span: Span) -> SaldResult<Expr> {
        self.advance(); // consume FormatStringStart

        // Start with the first string literal
        let mut result = Expr::Literal {
            value: Literal::String(first_part),
            span: start_span,
        };

        loop {
            // Parse the expression
            let expr = self.expression()?;
            let expr_span = expr.span();

            // Concatenate: result + expr
            let concat_span = Span::from_positions(
                result.span().start.line,
                result.span().start.column,
                expr_span.end.line,
                expr_span.end.column,
            );
            result = Expr::Binary {
                left: Box::new(result),
                op: BinaryOp::Add,
                right: Box::new(expr),
                span: concat_span,
            };

            // Check what comes next
            let next_token = self.peek().clone();
            match &next_token.kind {
                TokenKind::FormatStringPart(s) => {
                    // More parts to come
                    let s = s.clone();
                    self.advance();

                    // Add the string part
                    let str_expr = Expr::Literal {
                        value: Literal::String(s),
                        span: next_token.span,
                    };
                    let concat_span = Span::from_positions(
                        result.span().start.line,
                        result.span().start.column,
                        next_token.span.end.line,
                        next_token.span.end.column,
                    );
                    result = Expr::Binary {
                        left: Box::new(result),
                        op: BinaryOp::Add,
                        right: Box::new(str_expr),
                        span: concat_span,
                    };
                    // Continue loop for next expression
                }
                TokenKind::FormatStringEnd(s) => {
                    // Last part
                    let s = s.clone();
                    self.advance();

                    if !s.is_empty() {
                        // Add the final string part
                        let str_expr = Expr::Literal {
                            value: Literal::String(s),
                            span: next_token.span,
                        };
                        let concat_span = Span::from_positions(
                            result.span().start.line,
                            result.span().start.column,
                            next_token.span.end.line,
                            next_token.span.end.column,
                        );
                        result = Expr::Binary {
                            left: Box::new(result),
                            op: BinaryOp::Add,
                            right: Box::new(str_expr),
                            span: concat_span,
                        };
                    }
                    break;
                }
                _ => {
                    return Err(self
                        .error("Expected format string part or end")
                        .with_help("Format strings should end with a closing '\"'"));
                }
            }
        }

        Ok(result)
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    /// Check if current token is an identifier
    fn check_identifier(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(self.peek().kind, TokenKind::Identifier(_))
    }

    /// Check token at offset n ahead (0 = current, 1 = next, etc)
    fn check_ahead(&self, n: usize, kind: &TokenKind) -> bool {
        let idx = self.current + n;
        if idx >= self.tokens.len() {
            return false;
        }
        std::mem::discriminant(&self.tokens[idx].kind) == std::mem::discriminant(kind)
    }

    fn match_token(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> SaldResult<&Token> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(message))
        }
    }

    fn consume_identifier(&mut self, message: &str) -> SaldResult<&Token> {
        if matches!(self.peek().kind, TokenKind::Identifier(_)) {
            Ok(self.advance())
        } else {
            Err(self.error(message))
        }
    }

    fn error(&self, message: &str) -> SaldError {
        let token = self.peek();
        SaldError::syntax_error(message, token.span, &self.file).with_source(&self.source)
    }
}
