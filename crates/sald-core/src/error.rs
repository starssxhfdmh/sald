// Sald Error Handling Module
// Provides comprehensive error reporting with line numbers, spans, and stack traces

#[cfg(not(target_arch = "wasm32"))]
use colored::*;
use std::fmt;

/// Represents a position in the source code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl Position {
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}

/// Represents a span in the source code (start to end position)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn from_positions(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        Self {
            start: Position::new(start_line, start_col, 0),
            end: Position::new(end_line, end_col, 0),
        }
    }

    pub fn single(line: usize, column: usize, offset: usize) -> Self {
        let pos = Position::new(line, column, offset);
        Self {
            start: pos,
            end: pos,
        }
    }
}

impl Default for Span {
    fn default() -> Self {
        Self {
            start: Position::default(),
            end: Position::default(),
        }
    }
}

/// Types of errors in Sald
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    SyntaxError,
    TypeError,
    NameError,
    ValueError,
    RuntimeError,
    AttributeError,
    IndexError,
    ArgumentError,
    DivisionByZero,
    ImportError,
    AccessError,
    InterfaceError,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::SyntaxError => write!(f, "SyntaxError"),
            ErrorKind::TypeError => write!(f, "TypeError"),
            ErrorKind::NameError => write!(f, "NameError"),
            ErrorKind::ValueError => write!(f, "ValueError"),
            ErrorKind::RuntimeError => write!(f, "RuntimeError"),
            ErrorKind::AttributeError => write!(f, "AttributeError"),
            ErrorKind::IndexError => write!(f, "IndexError"),
            ErrorKind::ArgumentError => write!(f, "ArgumentError"),
            ErrorKind::DivisionByZero => write!(f, "DivisionByZero"),
            ErrorKind::ImportError => write!(f, "ImportError"),
            ErrorKind::AccessError => write!(f, "AccessError"),
            ErrorKind::InterfaceError => write!(f, "InterfaceError"),
        }
    }
}

/// A stack frame for error traces
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function_name: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

impl StackFrame {
    pub fn new(
        function_name: impl Into<String>,
        file: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            function_name: function_name.into(),
            file: file.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for StackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  at {} ({}:{}:{})",
            self.function_name,
            self.file.trim_start_matches(r"\\?\"),
            self.line,
            self.column
        )
    }
}

/// Main error type for Sald
#[derive(Debug, Clone)]
pub struct SaldError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Span,
    pub file: String,
    pub help: Option<String>,
    pub stack_trace: Vec<StackFrame>,
    source_lines: Vec<String>,
}

impl SaldError {
    pub fn new(
        kind: ErrorKind,
        message: impl Into<String>,
        span: Span,
        file: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            file: file.into(),
            help: None,
            stack_trace: Vec::new(),
            source_lines: Vec::new(),
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source_lines = source.lines().map(String::from).collect();
        self
    }

    pub fn with_stack_trace(mut self, trace: Vec<StackFrame>) -> Self {
        self.stack_trace = trace;
        self
    }

    pub fn push_frame(&mut self, frame: StackFrame) {
        self.stack_trace.push(frame);
    }

    /// Format the error for display (with colors for native)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Error header: SyntaxError: message at file:line:column
        let header = format!(
            "{}: {} at {}:{}:{}",
            self.kind.to_string().red().bold(),
            self.message.white().bold(),
            self.file.trim_start_matches(r"\\?\"),
            self.span.start.line,
            self.span.start.column
        );
        output.push_str(&header);
        output.push('\n');

        // Source context (show 3 lines: before, error line, after)
        if !self.source_lines.is_empty() {
            let error_line = self.span.start.line;
            let start_line = if error_line > 1 { error_line - 1 } else { 1 };
            let end_line = (error_line + 1).min(self.source_lines.len());

            output.push('\n');

            for line_num in start_line..=end_line {
                if line_num <= self.source_lines.len() {
                    let line_content = &self.source_lines[line_num - 1];
                    let line_num_str = format!("{:>4} |", line_num);

                    if line_num == error_line {
                        output.push_str(&format!("{} {}\n", line_num_str.red(), line_content));

                        // Add caret pointing to the error
                        let spaces = " ".repeat(6 + self.span.start.column);
                        let caret_len = if self.span.end.column > self.span.start.column {
                            self.span.end.column - self.span.start.column + 1
                        } else {
                            1
                        };
                        let carets = "^".repeat(caret_len);
                        output.push_str(&format!("{}{}\n", spaces, carets.red().bold()));
                    } else {
                        output.push_str(&format!("{} {}\n", line_num_str.dimmed(), line_content));
                    }
                }
            }
        }

        // Help message
        if let Some(ref help) = self.help {
            output.push_str(&format!("\n      {}: {}\n", "Help".cyan().bold(), help));
        }

        // Stack trace (limit to 10 frames)
        if !self.stack_trace.is_empty() {
            output.push_str(&format!("\n{}:\n", "Stack trace".yellow().bold()));
            // Show first few frames
            for frame in self.stack_trace.iter() {
                output.push_str(&format!("{}\n", frame));
            }
        }

        output
    }

    /// Format the error for display (plain text for WASM)
    #[cfg(target_arch = "wasm32")]
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Error header: SyntaxError: message at file:line:column
        let header = format!(
            "{}: {} at {}:{}:{}",
            self.kind,
            self.message,
            self.file.trim_start_matches(r"\\?\"),
            self.span.start.line,
            self.span.start.column
        );
        output.push_str(&header);
        output.push('\n');

        // Source context
        if !self.source_lines.is_empty() {
            let error_line = self.span.start.line;
            let start_line = if error_line > 1 { error_line - 1 } else { 1 };
            let end_line = (error_line + 1).min(self.source_lines.len());

            output.push('\n');

            for line_num in start_line..=end_line {
                if line_num <= self.source_lines.len() {
                    let line_content = &self.source_lines[line_num - 1];
                    let line_num_str = format!("{:>4} |", line_num);

                    output.push_str(&format!("{} {}\n", line_num_str, line_content));

                    if line_num == error_line {
                        let spaces = " ".repeat(6 + self.span.start.column);
                        let caret_len = if self.span.end.column > self.span.start.column {
                            self.span.end.column - self.span.start.column + 1
                        } else {
                            1
                        };
                        let carets = "^".repeat(caret_len);
                        output.push_str(&format!("{}{}\n", spaces, carets));
                    }
                }
            }
        }

        // Help message
        if let Some(ref help) = self.help {
            output.push_str(&format!("\n      Help: {}\n", help));
        }

        // Stack trace
        if !self.stack_trace.is_empty() {
            output.push_str("\nStack trace:\n");
            for frame in self.stack_trace.iter() {
                output.push_str(&format!("{}\n", frame));
            }
        }

        output
    }

    /// Format the error with options (native with colors)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn format_with_options(&self, full_trace: bool) -> String {
        if full_trace {
            // Use full trace - show all stack frames
            let mut output = String::new();

            // Error header
            let header = format!(
                "{}: {} at {}:{}:{}",
                self.kind.to_string().red().bold(),
                self.message.white().bold(),
                self.file.trim_start_matches(r"\\?\"),
                self.span.start.line,
                self.span.start.column
            );
            output.push_str(&header);
            output.push('\n');

            // Source context
            if !self.source_lines.is_empty() {
                let error_line = self.span.start.line;
                let start = error_line.saturating_sub(2);
                let end = (error_line + 1).min(self.source_lines.len());

                for i in start..end {
                    if let Some(line_content) = self.source_lines.get(i) {
                        let line_num = i + 1;
                        let line_num_str = format!("{:4} |", line_num);

                        if line_num == error_line {
                            output.push_str(&format!("{} {}\n", line_num_str.red(), line_content));
                            let spaces = " ".repeat(7 + self.span.start.column);
                            let caret_len = if self.span.end.column > self.span.start.column {
                                self.span.end.column - self.span.start.column
                            } else {
                                1
                            };
                            let carets = "^".repeat(caret_len);
                            output.push_str(&format!("{}{}\n", spaces, carets.red().bold()));
                        } else {
                            output.push_str(&format!(
                                "{} {}\n",
                                line_num_str.dimmed(),
                                line_content
                            ));
                        }
                    }
                }
            }

            // Help
            if let Some(ref help) = self.help {
                output.push_str(&format!("\n      {}: {}\n", "Help".cyan().bold(), help));
            }

            // Full stack trace
            if !self.stack_trace.is_empty() {
                output.push_str(&format!("\n{}:\n", "Stack trace".yellow().bold()));
                for frame in &self.stack_trace {
                    output.push_str(&format!("{}\n", frame));
                }
            }

            output
        } else {
            self.format()
        }
    }

    /// Format the error with options (WASM plain text)
    #[cfg(target_arch = "wasm32")]
    pub fn format_with_options(&self, _full_trace: bool) -> String {
        self.format()
    }
}

impl fmt::Display for SaldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl std::error::Error for SaldError {}

/// Result type for Sald operations
pub type SaldResult<T> = Result<T, SaldError>;

// Convenience constructors for common errors
impl SaldError {
    pub fn syntax_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::SyntaxError, message, span, file)
    }

    pub fn type_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::TypeError, message, span, file)
    }

    pub fn name_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::NameError, message, span, file)
    }

    pub fn value_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::ValueError, message, span, file)
    }

    pub fn runtime_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::RuntimeError, message, span, file)
    }

    pub fn attribute_error(
        message: impl Into<String>,
        span: Span,
        file: impl Into<String>,
    ) -> Self {
        Self::new(ErrorKind::AttributeError, message, span, file)
    }

    pub fn argument_error(message: impl Into<String>, span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::ArgumentError, message, span, file)
    }

    pub fn division_by_zero(span: Span, file: impl Into<String>) -> Self {
        Self::new(ErrorKind::DivisionByZero, "Division by zero", span, file)
    }

    pub fn interface_error(
        message: impl Into<String>,
        span: Span,
        file: impl Into<String>,
    ) -> Self {
        Self::new(ErrorKind::InterfaceError, message, span, file)
    }
}
