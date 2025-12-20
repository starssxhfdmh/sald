// Sald Scanner (Lexer)
// Converts source code into tokens

use crate::error::{SaldError, SaldResult, Span};
use crate::lexer::token::{Token, TokenKind};

/// Scanner that tokenizes Sald source code
pub struct Scanner {
    source: Vec<char>,
    tokens: Vec<Token>,
    start: usize,
    current: usize,
    line: usize,
    column: usize,
    start_column: usize,
    file: String,
}

impl Scanner {
    pub fn new(source: &str, file: impl Into<String>) -> Self {
        Self {
            source: source.chars().collect(),
            tokens: Vec::new(),
            start: 0,
            current: 0,
            line: 1,
            column: 1,
            start_column: 1,
            file: file.into(),
        }
    }

    /// Scan all tokens from the source
    pub fn scan_tokens(&mut self) -> SaldResult<Vec<Token>> {
        while !self.is_at_end() {
            self.start = self.current;
            self.start_column = self.column;
            self.scan_token()?;
        }

        // Add EOF token
        self.tokens.push(Token::new(
            TokenKind::Eof,
            "",
            Span::single(self.line, self.column, self.current),
        ));

        Ok(self.tokens.clone())
    }

    fn scan_token(&mut self) -> SaldResult<()> {
        let c = self.advance();

        match c {
            // Single character tokens
            '(' => self.add_token(TokenKind::LeftParen),
            ')' => self.add_token(TokenKind::RightParen),
            '{' => self.add_token(TokenKind::LeftBrace),
            '}' => self.add_token(TokenKind::RightBrace),
            '[' => self.add_token(TokenKind::LeftBracket),
            ']' => self.add_token(TokenKind::RightBracket),
            ',' => self.add_token(TokenKind::Comma),
            '.' => {
                if self.match_char('.') {
                    if self.match_char('<') {
                        self.add_token(TokenKind::DotDotLess);  // ..<
                    } else if self.match_char('.') {
                        self.add_token(TokenKind::DotDotDot);   // ...
                    } else {
                        self.add_token(TokenKind::DotDot);      // ..
                    }
                } else {
                    self.add_token(TokenKind::Dot);
                }
            }
            ';' => self.add_token(TokenKind::Semicolon),
            ':' => self.add_token(TokenKind::Colon),
            '?' => {
                if self.match_char('?') {
                    self.add_token(TokenKind::QuestionQuestion);
                } else if self.match_char('.') {
                    self.add_token(TokenKind::QuestionDot);
                } else {
                    self.add_token(TokenKind::Question);
                }
            }
            '|' => {
                if self.match_char('|') {
                    self.add_token(TokenKind::Or);
                } else {
                    self.add_token(TokenKind::Pipe);
                }
            }

            // Operators (potentially multi-character)
            '+' => {
                let kind = if self.match_char('=') {
                    TokenKind::PlusEqual
                } else {
                    TokenKind::Plus
                };
                self.add_token(kind);
            }
            '-' => {
                let kind = if self.match_char('=') {
                    TokenKind::MinusEqual
                } else if self.match_char('>') {
                    TokenKind::ThinArrow
                } else {
                    TokenKind::Minus
                };
                self.add_token(kind);
            }
            '*' => {
                let kind = if self.match_char('=') {
                    TokenKind::StarEqual
                } else {
                    TokenKind::Star
                };
                self.add_token(kind);
            }
            '/' => {
                if self.match_char('/') {
                    // Single-line comment
                    while self.peek() != '\n' && !self.is_at_end() {
                        self.advance();
                    }
                } else if self.match_char('*') {
                    // Multi-line comment
                    self.block_comment()?;
                } else if self.match_char('=') {
                    self.add_token(TokenKind::SlashEqual);
                } else {
                    self.add_token(TokenKind::Slash);
                }
            }
            '%' => {
                let kind = if self.match_char('=') {
                    TokenKind::PercentEqual
                } else {
                    TokenKind::Percent
                };
                self.add_token(kind);
            }

            // Comparison operators
            '!' => {
                let kind = if self.match_char('=') {
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                };
                self.add_token(kind);
            }
            '=' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::EqualEqual);
                } else if self.match_char('>') {
                    self.add_token(TokenKind::Arrow);
                } else {
                    self.add_token(TokenKind::Equal);
                }
            }
            '<' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::LessEqual);
                } else if self.match_char('<') {
                    self.add_token(TokenKind::LessLess);
                } else {
                    self.add_token(TokenKind::Less);
                }
            }
            '>' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::GreaterEqual);
                } else if self.match_char('>') {
                    self.add_token(TokenKind::GreaterGreater);
                } else {
                    self.add_token(TokenKind::Greater);
                }
            }

            // Logical and Bitwise operators
            '&' => {
                if self.match_char('&') {
                    self.add_token(TokenKind::And);
                } else {
                    self.add_token(TokenKind::Ampersand);
                }
            }
            '^' => self.add_token(TokenKind::Caret),
            '@' => self.add_token(TokenKind::At),
            '~' => self.add_token(TokenKind::Tilde),

            // Whitespace
            ' ' | '\r' | '\t' => {}
            '\n' => {
                self.line += 1;
                self.column = 1;
            }

            // String literals (double or single quotes) - check for triple quotes first
            '"' => {
                if self.peek() == '"' && self.peek_next() == '"' {
                    // Triple double-quote multiline string """..."""
                    self.advance(); // consume second "
                    self.advance(); // consume third "
                    self.multiline_string('"')?;
                } else {
                    self.string('"')?;
                }
            }
            '\'' => {
                if self.peek() == '\'' && self.peek_next() == '\'' {
                    // Triple single-quote multiline string '''...'''
                    self.advance(); // consume second '
                    self.advance(); // consume third '
                    self.multiline_string('\'')?;
                } else {
                    self.string('\'')?;
                }
            }

            // Format string $"..." or $'...' or format multiline $"""...""" or $'''...'''
            '$' => {
                if self.peek() == '"' && self.peek_next() == '"' {
                    self.advance(); // consume first "
                    if self.peek() == '"' {
                        self.advance(); // consume second "
                        self.advance(); // consume third "
                        self.format_multiline_string('"')?;
                    } else {
                        // Just $" - regular format string, but we already consumed one "
                        self.format_string('"')?;
                    }
                } else if self.peek() == '\'' && self.peek_next() == '\'' {
                    self.advance(); // consume first '
                    if self.peek() == '\'' {
                        self.advance(); // consume second '
                        self.advance(); // consume third '
                        self.format_multiline_string('\'')?;
                    } else {
                        self.format_string('\'')?;
                    }
                } else if self.match_char('"') {
                    self.format_string('"')?;
                } else if self.match_char('\'') {
                    self.format_string('\'')?;
                } else {
                    return Err(self
                        .error("Expected '\"' or \"'\" after '$' for format string")
                        .with_help("Use $\"...\" or $'...' for string interpolation"));
                }
            }

            // Number literals or identifiers
            c if c.is_ascii_digit() => self.number()?,
            c if c.is_alphabetic() || c == '_' => {
                // Check for raw string prefix: r"..." or r'...'
                if c == 'r' && (self.peek() == '"' || self.peek() == '\'') {
                    let quote_char = self.advance(); // consume the quote
                    if self.peek() == quote_char && self.peek_next() == quote_char {
                        // r"""...""" or r'''...''' - raw multiline
                        self.advance(); // consume second quote
                        self.advance(); // consume third quote
                        self.raw_string(quote_char)?;
                    } else {
                        // r"..." or r'...' - raw single-line
                        self.raw_string_single(quote_char)?;
                    }
                } else {
                    self.identifier();
                }
            }

            // Unknown character
            _ => {
                return Err(self
                    .error(&format!("Unexpected character '{}'", c))
                    .with_help("Remove this character or check for typos"));
            }
        }

        Ok(())
    }

    fn string(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        while self.peek() != quote_char && !self.is_at_end() {
            if self.peek() == '\\' {
                // Skip escape sequence - consume backslash and next char
                self.advance();
                if !self.is_at_end() {
                    if self.peek() == '\n' {
                        self.line += 1;
                        self.column = 1;
                    }
                    self.advance();
                }
            } else if self.peek() == '\n' {
                self.line += 1;
                self.column = 1;
                self.advance();
            } else {
                self.advance();
            }
        }

        if self.is_at_end() {
            let quote_name = if quote_char == '"' {
                "double"
            } else {
                "single"
            };
            return Err(SaldError::syntax_error(
                "Unterminated string",
                Span::from_positions(start_line, start_col, self.line, self.column),
                &self.file,
            )
            .with_help(&format!(
                "Add a closing {} quote to terminate the string",
                quote_name
            )));
        }

        // Consume the closing quote
        self.advance();

        // Get the string value (without quotes)
        let value: String = self.source[self.start + 1..self.current - 1]
            .iter()
            .collect();

        // Handle escape sequences
        let processed = self.process_escapes(&value, quote_char)?;
        self.add_token(TokenKind::String(processed));
        Ok(())
    }

    /// Parse format string $"Hello, {name}!" or $'Hello, {name}!'
    /// Emits: FormatStringStart("Hello, "), tokens for expr, FormatStringEnd("!")
    fn format_string(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        let mut current_text = String::new();
        let mut is_first = true;

        while !self.is_at_end() {
            let c = self.peek();

            if c == quote_char {
                // End of format string
                self.advance();

                // Emit final part
                let processed = self.process_escapes(&current_text, quote_char)?;
                if is_first {
                    // Simple string, no interpolation - just emit as regular string
                    self.add_token(TokenKind::String(processed));
                } else {
                    self.add_token(TokenKind::FormatStringEnd(processed));
                }
                return Ok(());
            } else if c == '{' {
                // Start of expression interpolation
                self.advance();

                // Check for {{ escape
                if self.peek() == '{' {
                    self.advance();
                    current_text.push('{');
                    continue;
                }

                // Emit current text part
                let processed = self.process_escapes(&current_text, quote_char)?;
                if is_first {
                    self.add_token(TokenKind::FormatStringStart(processed));
                    is_first = false;
                } else {
                    self.add_token(TokenKind::FormatStringPart(processed));
                }
                current_text.clear();

                // Scan tokens until matching }
                let mut brace_depth = 1;
                while brace_depth > 0 && !self.is_at_end() {
                    self.start = self.current;
                    self.start_column = self.column;

                    let expr_char = self.peek();
                    if expr_char == '}' {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            self.advance();
                            break;
                        }
                    } else if expr_char == '{' {
                        brace_depth += 1;
                    }

                    self.scan_token()?;
                }

                if brace_depth > 0 {
                    return Err(SaldError::syntax_error(
                        "Unterminated expression in format string",
                        Span::from_positions(start_line, start_col, self.line, self.column),
                        &self.file,
                    )
                    .with_help("Add a closing '}' to end the expression"));
                }
            } else if c == '}' {
                // Check for }} escape
                self.advance();
                if self.peek() == '}' {
                    self.advance();
                    current_text.push('}');
                } else {
                    return Err(self
                        .error("Unexpected '}' in format string")
                        .with_help("Use '}}' to include a literal '}'"));
                }
            } else if c == '\\' {
                // Handle escape sequences - consume backslash and next char together
                current_text.push(self.advance()); // push '\'
                if !self.is_at_end() {
                    current_text.push(self.advance()); // push escaped char
                }
            } else if c == '\n' {
                self.line += 1;
                self.column = 1;
                current_text.push(self.advance());
            } else {
                current_text.push(self.advance());
            }
        }

        let quote_name = if quote_char == '"' {
            "double"
        } else {
            "single"
        };
        Err(SaldError::syntax_error(
            "Unterminated format string",
            Span::from_positions(start_line, start_col, self.line, self.column),
            &self.file,
        )
        .with_help(&format!(
            "Add a closing {} quote to terminate the format string",
            quote_name
        )))
    }

    /// Parse raw string r"""...""" or r'''...''' - no escape processing (multiline)
    fn raw_string(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        let mut value = String::new();

        while !self.is_at_end() {
            let c = self.peek();

            // Check for closing triple quotes
            if c == quote_char && self.peek_next() == quote_char {
                // Check third char
                if self.current + 2 < self.source.len() && self.source[self.current + 2] == quote_char {
                    // Found closing triple quotes
                    self.advance(); // consume first quote
                    self.advance(); // consume second quote 
                    self.advance(); // consume third quote
                    self.add_token(TokenKind::RawString(value));
                    return Ok(());
                }
            }

            if c == '\n' {
                self.line += 1;
                self.column = 1;
            }
            value.push(self.advance());
        }

        let quote = if quote_char == '"' { "\"\"\"" } else { "'''" };
        Err(SaldError::syntax_error(
            "Unterminated raw string",
            Span::from_positions(start_line, start_col, self.line, self.column),
            &self.file,
        )
        .with_help(&format!("Add {} to close the raw string", quote)))
    }

    /// Parse raw string r"..." or r'...' - no escape processing (single line)
    fn raw_string_single(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        let mut value = String::new();

        while !self.is_at_end() && self.peek() != quote_char {
            let c = self.peek();
            if c == '\n' {
                // Single-line raw string cannot contain newlines
                return Err(SaldError::syntax_error(
                    "Unterminated raw string",
                    Span::from_positions(start_line, start_col, self.line, self.column),
                    &self.file,
                )
                .with_help("Use r\"\"\"...\"\"\" for multiline raw strings"));
            }
            value.push(self.advance());
        }

        if self.is_at_end() {
            let quote_name = if quote_char == '"' { "double" } else { "single" };
            return Err(SaldError::syntax_error(
                "Unterminated raw string",
                Span::from_positions(start_line, start_col, self.line, self.column),
                &self.file,
            )
            .with_help(&format!("Add a closing {} quote to terminate the raw string", quote_name)));
        }

        // Consume closing quote
        self.advance();
        self.add_token(TokenKind::RawString(value));
        Ok(())
    }

    /// Parse multiline string """...""" or '''...''' WITH escape processing
    fn multiline_string(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        let mut value = String::new();

        while !self.is_at_end() {
            let c = self.peek();

            // Check for closing triple quotes
            if c == quote_char && self.peek_next() == quote_char {
                // Check third char
                if self.current + 2 < self.source.len() && self.source[self.current + 2] == quote_char {
                    // Found closing triple quotes
                    self.advance(); // consume first quote
                    self.advance(); // consume second quote 
                    self.advance(); // consume third quote
                    
                    // Process escape sequences
                    let processed = self.process_escapes(&value, quote_char)?;
                    self.add_token(TokenKind::String(processed));
                    return Ok(());
                }
            }

            if c == '\\' {
                // Handle escape sequences - consume backslash and next char together
                value.push(self.advance()); // push '\'
                if !self.is_at_end() {
                    if self.peek() == '\n' {
                        self.line += 1;
                        self.column = 1;
                    }
                    value.push(self.advance()); // push escaped char
                }
            } else if c == '\n' {
                self.line += 1;
                self.column = 1;
                value.push(self.advance());
            } else {
                value.push(self.advance());
            }
        }

        let quote = if quote_char == '"' { "\"\"\"" } else { "'''" };
        Err(SaldError::syntax_error(
            "Unterminated multiline string",
            Span::from_positions(start_line, start_col, self.line, self.column),
            &self.file,
        )
        .with_help(&format!("Add {} to close the multiline string", quote)))
    }

    /// Parse format multiline string $"""...""" or $'''...''' with interpolation AND escape processing
    fn format_multiline_string(&mut self, quote_char: char) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;

        let mut current_text = String::new();
        let mut is_first = true;

        while !self.is_at_end() {
            let c = self.peek();

            // Check for closing triple quotes
            if c == quote_char && self.peek_next() == quote_char {
                if self.current + 2 < self.source.len() && self.source[self.current + 2] == quote_char {
                    // Found closing triple quotes
                    self.advance();
                    self.advance();
                    self.advance();

                    // Emit final part WITH escape processing
                    let processed = self.process_escapes(&current_text, quote_char)?;
                    if is_first {
                        // Entire string has no interpolation - emit as regular string
                        self.add_token(TokenKind::String(processed));
                    } else {
                        self.add_token(TokenKind::FormatStringEnd(processed));
                    }
                    return Ok(());
                }
            }

            if c == '{' {
                // Check for {{ escape
                self.advance();
                if self.peek() == '{' {
                    self.advance();
                    current_text.push('{');
                } else {
                    // Start of expression - emit current text WITH escape processing
                    let processed = self.process_escapes(&current_text, quote_char)?;
                    if is_first {
                        self.add_token(TokenKind::FormatStringStart(processed));
                        is_first = false;
                    } else {
                        self.add_token(TokenKind::FormatStringPart(processed));
                    }
                    current_text.clear();

                    // Scan expression tokens until matching }
                    let mut brace_depth = 1;
                    while brace_depth > 0 && !self.is_at_end() {
                        if self.peek() == '{' {
                            brace_depth += 1;
                        } else if self.peek() == '}' {
                            brace_depth -= 1;
                            if brace_depth == 0 {
                                self.advance();
                                break;
                            }
                        }
                        self.start = self.current;
                        self.start_column = self.column;
                        self.scan_token()?;
                    }

                    if brace_depth > 0 {
                        return Err(SaldError::syntax_error(
                            "Unterminated expression in format string",
                            Span::from_positions(start_line, start_col, self.line, self.column),
                            &self.file,
                        )
                        .with_help("Add a closing '}' to end the expression"));
                    }
                }
            } else if c == '}' {
                // Check for }} escape
                self.advance();
                if self.peek() == '}' {
                    self.advance();
                    current_text.push('}');
                } else {
                    return Err(self
                        .error("Unexpected '}' in format string")
                        .with_help("Use '}}' to include a literal '}'"));
                }
            } else if c == '\\' {
                // Handle escape sequences - consume backslash and next char together
                current_text.push(self.advance()); // push '\'
                if !self.is_at_end() {
                    if self.peek() == '\n' {
                        self.line += 1;
                        self.column = 1;
                    }
                    current_text.push(self.advance()); // push escaped char
                }
            } else if c == '\n' {
                self.line += 1;
                self.column = 1;
                current_text.push(self.advance());
            } else {
                current_text.push(self.advance());
            }
        }

        let quote = if quote_char == '"' { "\"\"\"" } else { "'''" };
        Err(SaldError::syntax_error(
            "Unterminated format string",
            Span::from_positions(start_line, start_col, self.line, self.column),
            &self.file,
        )
        .with_help(&format!("Add {} to close the format string", quote)))
    }

    fn process_escapes(&self, s: &str, quote_char: char) -> SaldResult<String> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('r') => result.push('\r'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some('\'') => result.push('\''),
                    Some('0') => result.push('\0'),
                    Some('b') => result.push('\x08'), // backspace
                    Some('f') => result.push('\x0C'), // form feed
                    Some('v') => result.push('\x0B'), // vertical tab
                    Some('x') => {
                        // Hex escape: \xHH (exactly 2 hex digits)
                        let mut hex = String::new();
                        for _ in 0..2 {
                            match chars.next() {
                                Some(h) if h.is_ascii_hexdigit() => hex.push(h),
                                Some(h) => {
                                    return Err(self
                                        .error(&format!(
                                            "Invalid hex escape: expected hex digit, got '{}'",
                                            h
                                        ))
                                        .with_help("Use \\xHH where H is 0-9, a-f, or A-F"));
                                }
                                None => {
                                    return Err(self
                                        .error("Incomplete hex escape sequence")
                                        .with_help("Use \\xHH where H is 0-9, a-f, or A-F"));
                                }
                            }
                        }
                        let code = u8::from_str_radix(&hex, 16)
                            .map_err(|_| self.error(&format!("Invalid hex value: {}", hex)))?;
                        result.push(code as char);
                    }
                    Some('u') => {
                        // Unicode escape: \u{HHHH} or \uHHHH
                        if chars.peek() == Some(&'{') {
                            // \u{HHHH...} format (1-6 hex digits)
                            chars.next(); // consume '{'
                            let mut hex = String::new();
                            loop {
                                match chars.next() {
                                    Some('}') => break,
                                    Some(h) if h.is_ascii_hexdigit() => {
                                        if hex.len() >= 6 {
                                            return Err(self
                                                .error("Unicode escape too long")
                                                .with_help(
                                                    "Unicode escape \\u{...} allows 1-6 hex digits",
                                                ));
                                        }
                                        hex.push(h);
                                    }
                                    Some(h) => {
                                        return Err(self.error(&format!(
                                            "Invalid unicode escape: expected hex digit or '}}', got '{}'", h
                                        )));
                                    }
                                    None => {
                                        return Err(self
                                            .error("Unclosed unicode escape")
                                            .with_help("Close with '}', e.g., \\u{1F600}"));
                                    }
                                }
                            }
                            if hex.is_empty() {
                                return Err(self
                                    .error("Empty unicode escape")
                                    .with_help("Provide at least one hex digit, e.g., \\u{0}"));
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                                self.error(&format!("Invalid unicode value: {}", hex))
                            })?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                self.error(&format!("Invalid unicode code point: U+{:X}", code))
                            })?;
                            result.push(ch);
                        } else {
                            // \uHHHH format (exactly 4 hex digits)
                            let mut hex = String::new();
                            for _ in 0..4 {
                                match chars.next() {
                                    Some(h) if h.is_ascii_hexdigit() => hex.push(h),
                                    Some(h) => {
                                        return Err(self.error(&format!(
                                            "Invalid unicode escape: expected hex digit, got '{}'", h
                                        )).with_help("Use \\uHHHH (4 hex digits) or \\u{HHHH}"));
                                    }
                                    None => {
                                        return Err(self
                                            .error("Incomplete unicode escape")
                                            .with_help("Use \\uHHHH (4 hex digits) or \\u{HHHH}"));
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                                self.error(&format!("Invalid unicode value: {}", hex))
                            })?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                self.error(&format!("Invalid unicode code point: U+{:X}", code))
                            })?;
                            result.push(ch);
                        }
                    }
                    Some(c) => {
                        return Err(self.error(&format!("Invalid escape sequence '\\{}'", c))
                            .with_help("Valid escapes: \\n, \\t, \\r, \\0, \\\\, \\\", \\', \\xHH, \\uHHHH, \\u{HHHH}"));
                    }
                    None => {
                        return Err(self
                            .error("Unexpected end of string after '\\'")
                            .with_help("Add a valid escape character after '\\'"));
                    }
                }
            } else {
                result.push(c);
            }
        }

        let _ = quote_char; // Silence unused warning
        Ok(result)
    }

    fn number(&mut self) -> SaldResult<()> {
        while self.peek().is_ascii_digit() {
            self.advance();
        }

        // Look for decimal part
        if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            // Consume the '.'
            self.advance();

            while self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        let lexeme: String = self.source[self.start..self.current].iter().collect();
        let value: f64 = lexeme
            .parse()
            .map_err(|_| self.error(&format!("Invalid number '{}'", lexeme)))?;

        self.add_token(TokenKind::Number(value));
        Ok(())
    }

    fn identifier(&mut self) {
        while self.peek().is_alphanumeric() || self.peek() == '_' {
            self.advance();
        }

        let text: String = self.source[self.start..self.current].iter().collect();
        let kind = self.keyword_or_identifier(&text);
        self.add_token(kind);
    }

    fn keyword_or_identifier(&self, text: &str) -> TokenKind {
        match text {
            "let" => TokenKind::Let,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "fun" => TokenKind::Fun,
            "return" => TokenKind::Return,
            "class" => TokenKind::Class,
            "extends" => TokenKind::Extends,
            "super" => TokenKind::Super,
            "self" => TokenKind::SelfKeyword,
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "throw" => TokenKind::Throw,
            "switch" => TokenKind::Switch,
            "default" => TokenKind::Default,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "namespace" => TokenKind::Namespace,
            "const" => TokenKind::Const,
            "enum" => TokenKind::Enum,
            "interface" => TokenKind::Interface,
            "implements" => TokenKind::Implements,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            _ => TokenKind::Identifier(text.to_string()),
        }
    }

    fn block_comment(&mut self) -> SaldResult<()> {
        let start_line = self.line;
        let start_col = self.start_column;
        let mut depth = 1;

        while depth > 0 && !self.is_at_end() {
            if self.peek() == '/' && self.peek_next() == '*' {
                self.advance();
                self.advance();
                depth += 1;
            } else if self.peek() == '*' && self.peek_next() == '/' {
                self.advance();
                self.advance();
                depth -= 1;
            } else {
                if self.peek() == '\n' {
                    self.line += 1;
                    self.column = 1;
                }
                self.advance();
            }
        }

        if depth > 0 {
            return Err(SaldError::syntax_error(
                "Unterminated block comment",
                Span::from_positions(start_line, start_col, self.line, self.column),
                &self.file,
            )
            .with_help("Add '*/' to close the block comment"));
        }

        Ok(())
    }

    // Helper methods
    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn advance(&mut self) -> char {
        let c = self.source[self.current];
        self.current += 1;
        self.column += 1;
        c
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.source[self.current]
        }
    }

    fn peek_next(&self) -> char {
        if self.current + 1 >= self.source.len() {
            '\0'
        } else {
            self.source[self.current + 1]
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.source[self.current] != expected {
            false
        } else {
            self.current += 1;
            self.column += 1;
            true
        }
    }

    fn add_token(&mut self, kind: TokenKind) {
        let lexeme: String = self.source[self.start..self.current].iter().collect();
        let span = Span::from_positions(self.line, self.start_column, self.line, self.column - 1);
        self.tokens.push(Token::new(kind, lexeme, span));
    }

    fn error(&self, message: &str) -> SaldError {
        SaldError::syntax_error(
            message,
            Span::from_positions(self.line, self.start_column, self.line, self.column),
            &self.file,
        )
    }
}
