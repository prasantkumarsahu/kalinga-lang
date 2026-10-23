//! Source positions and compiler messages.

use std::fmt;

/// Where something is in the source: a 1-based line and column (in characters), and how
/// many characters it covers on that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    /// Length in characters; 0 for an empty or unknown extent.
    pub len: usize,
    /// Index into the driver's source map (0 = the file being compiled).
    pub file: u32,
}

impl Span {
    /// A position with no known length (shown as a single `^`).
    pub fn at(line: usize, col: usize, file: u32) -> Self {
        Span { line, col, len: 0, file }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// A message about the program, pointing at a place in the source.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    /// A hint on how to fix it, shown after the source line.
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { severity: Severity::Error, message: message.into(), span, help: None }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Pretty-prints the message with the offending source line, underlined.
    pub fn render(&self, file: &str, source: &str) -> String {
        let Span { line, col, len, .. } = self.span;
        let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
        let gutter = line.to_string();
        let pad = " ".repeat(gutter.len());
        let mut out = format!(
            "{}:{}:{}: {}: {}\n{} | {}\n{} | {}{}",
            file,
            line,
            col,
            self.severity,
            self.message,
            gutter,
            line_text,
            pad,
            " ".repeat(col.saturating_sub(1)),
            "^".repeat(len.max(1))
        );
        if let Some(help) = &self.help {
            out.push_str(&format!("\n{} = help: {}", pad, help));
        }
        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

impl std::error::Error for Diagnostic {}

pub type Result<T> = std::result::Result<T, Diagnostic>;

pub fn err<T>(message: impl Into<String>, span: Span) -> Result<T> {
    Err(Diagnostic::error(message, span))
}
