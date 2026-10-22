use std::fmt;

/// Position in the source file (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub msg: String,
    pub pos: Pos,
}

impl CompileError {
    pub fn new(msg: impl Into<String>, pos: Pos) -> Self {
        CompileError { msg: msg.into(), pos }
    }

    /// Pretty-prints the error with the offending source line and a caret.
    pub fn render(&self, file: &str, source: &str) -> String {
        let line_text = source.lines().nth(self.pos.line.saturating_sub(1)).unwrap_or("");
        let gutter = format!("{}", self.pos.line);
        let caret_pad = " ".repeat(self.pos.col.saturating_sub(1));
        format!(
            "{}:{}:{}: error: {}\n{} | {}\n{} | {}^",
            file,
            self.pos.line,
            self.pos.col,
            self.msg,
            gutter,
            line_text,
            " ".repeat(gutter.len()),
            caret_pad
        )
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.pos.line, self.pos.col, self.msg)
    }
}

pub type Result<T> = std::result::Result<T, CompileError>;

pub fn err<T>(msg: impl Into<String>, pos: Pos) -> Result<T> {
    Err(CompileError::new(msg, pos))
}
