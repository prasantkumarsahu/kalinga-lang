use crate::error::{err, Pos, Result};
use crate::token::{StrPart, Tok, Token};

/// Lexes a whole file; `file` is its index in the driver's source map.
pub fn lex(source: &str, file: u32) -> Result<Vec<Token>> {
    lex_at(source, Pos { line: 1, col: 1, file })
}

/// Lexes `source` as if it started at `start` (used for `{...}` inside strings).
pub fn lex_at(source: &str, start: Pos) -> Result<Vec<Token>> {
    Lexer { chars: source.chars().collect(), i: 0, line: start.line, col: start.col, file: start.file, brackets: Vec::new(), out: Vec::new() }.run()
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    file: u32,
    /// Open brackets. Newlines are ignored while the innermost one is `(` or `[`,
    /// but count again inside a `{` (e.g. a lambda body passed as an argument).
    brackets: Vec<char>,
    out: Vec<Token>,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.i + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn next_line_starts_with_dot(&self) -> bool {
        let mut j = self.i;
        while let Some(&c) = self.chars.get(j) {
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                j += 1;
            } else {
                return c == '.' && self.chars.get(j + 1) != Some(&'.');
            }
        }
        false
    }

    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col, file: self.file }
    }

    fn push(&mut self, tok: Tok, pos: Pos) {
        if tok == Tok::Newline {
            // Collapse blank lines and drop newlines that cannot end a statement.
            if matches!(self.brackets.last(), Some('(') | Some('[')) {
                return;
            }
            match self.out.last() {
                None => return,
                Some(t) if t.tok.continues_line() || t.tok == Tok::LBrace => return,
                _ => {}
            }
        }
        self.out.push(Token { tok, pos });
    }

    fn run(mut self) -> Result<Vec<Token>> {
        while let Some(c) = self.peek() {
            let pos = self.pos();
            match c {
                ' ' | '\t' | '\r' => {
                    self.bump();
                }
                '\n' => {
                    self.bump();
                    // A line starting with `.` continues the previous one (method chains).
                    if !self.next_line_starts_with_dot() {
                        self.push(Tok::Newline, pos);
                    }
                }
                '/' if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                '/' if self.peek2() == Some('*') => {
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            None => return err("unterminated block comment", pos),
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                '"' => self.string(pos)?,
                c if c.is_ascii_digit() => self.number(pos)?,
                c if c.is_alphabetic() || c == '_' => self.word(pos)?,
                _ => self.symbol(pos)?,
            }
        }
        let pos = self.pos();
        self.push(Tok::Newline, pos);
        self.out.push(Token { tok: Tok::Eof, pos });
        Ok(self.out)
    }

    /// Raw source of a `{...}` interpolation, after the opening `{`.
    fn interpolation(&mut self, pos: Pos) -> Result<String> {
        let mut code = String::new();
        let mut depth = 0;
        loop {
            match self.peek() {
                None | Some('\n') => return err("unterminated `{` in string", pos),
                Some('}') if depth == 0 => {
                    self.bump();
                    break;
                }
                Some('"') => {
                    // A string inside the interpolation, e.g. "{m["key"]}".
                    code.push(self.bump().unwrap());
                    loop {
                        match self.bump() {
                            None | Some('\n') => return err("unterminated string", pos),
                            Some('\\') => {
                                code.push('\\');
                                if let Some(c) = self.bump() {
                                    code.push(c);
                                }
                            }
                            Some('"') => {
                                code.push('"');
                                break;
                            }
                            Some(c) => code.push(c),
                        }
                    }
                }
                Some(c) => {
                    if c == '{' {
                        depth += 1;
                    } else if c == '}' {
                        depth -= 1;
                    }
                    code.push(c);
                    self.bump();
                }
            }
        }
        if code.trim().is_empty() {
            return err("empty `{}` in string (write `{{` for a literal brace)", pos);
        }
        Ok(code)
    }

    fn string(&mut self, pos: Pos) -> Result<()> {
        self.bump();
        let mut s = String::new();
        let mut parts: Vec<StrPart> = Vec::new();
        loop {
            match self.bump() {
                None | Some('\n') => return err("unterminated string", pos),
                Some('"') => break,
                Some('{') if self.peek() == Some('{') => {
                    self.bump();
                    s.push('{');
                }
                Some('}') if self.peek() == Some('}') => {
                    self.bump();
                    s.push('}');
                }
                Some('{') => {
                    let code_pos = self.pos();
                    let code = self.interpolation(code_pos)?;
                    if !s.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut s)));
                    }
                    parts.push(StrPart::Code(code, code_pos));
                }
                Some('}') => return err("unmatched `}` in string (write `}}` for a literal brace)", self.pos()),
                Some('\\') => {
                    let esc = self.bump();
                    s.push(match esc {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('0') => '\0',
                        Some('\\') => '\\',
                        Some('"') => '"',
                        _ => return err("unknown escape sequence in string", self.pos()),
                    });
                }
                Some(c) => s.push(c),
            }
        }
        if parts.is_empty() {
            self.push(Tok::Str(s), pos);
        } else {
            if !s.is_empty() {
                parts.push(StrPart::Lit(s));
            }
            self.push(Tok::InterpStr(parts), pos);
        }
        Ok(())
    }

    fn number(&mut self, pos: Pos) -> Result<()> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                if c != '_' {
                    text.push(c);
                }
                self.bump();
            } else {
                break;
            }
        }
        // `1.5` is a float, but `0..10` is a range.
        let is_float = self.peek() == Some('.') && self.peek2().is_some_and(|c| c.is_ascii_digit());
        if is_float {
            text.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            let v: f64 = text.parse().map_err(|_| crate::error::CompileError::new("bad float literal", pos))?;
            self.push(Tok::Float(v), pos);
        } else {
            let v: i64 = match text.parse() {
                Ok(v) => v,
                Err(_) => return err("integer literal too large for int (64-bit)", pos),
            };
            self.push(Tok::Int(v), pos);
        }
        Ok(())
    }

    fn word(&mut self, pos: Pos) -> Result<()> {
        let mut w = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                w.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if w.starts_with("__") {
            return err("names starting with `__` are reserved", pos);
        }
        // `= rust { ... }`: the body is Rust, so take it as raw text.
        if w == "rust" && self.out.last().is_some_and(|t| t.tok == Tok::Assign) {
            let mut j = self.i;
            while matches!(self.chars.get(j), Some(' ' | '\t')) {
                j += 1;
            }
            if self.chars.get(j) == Some(&'{') {
                while self.i < j {
                    self.bump();
                }
                let body = self.rust_block(pos)?;
                self.push(Tok::RustBlock(body), pos);
                return Ok(());
            }
        }
        let tok = Tok::keyword(&w).unwrap_or(Tok::Ident(w));
        self.push(tok, pos);
        Ok(())
    }

    /// Reads a balanced `{ ... }` of Rust code, skipping Rust strings, chars and comments.
    fn rust_block(&mut self, pos: Pos) -> Result<String> {
        self.bump(); // `{`
        let mut out = String::new();
        let mut depth = 1;
        loop {
            let c = match self.bump() {
                Some(c) => c,
                None => return err("unterminated `rust { ... }` block", pos),
            };
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                }
                '"' => {
                    out.push(c);
                    while let Some(d) = self.bump() {
                        out.push(d);
                        if d == '\\' {
                            if let Some(e) = self.bump() {
                                out.push(e);
                            }
                        } else if d == '"' {
                            break;
                        }
                    }
                    continue;
                }
                '\'' => {
                    // A char literal ('x', '\n', '{') or a lifetime ('a).
                    out.push(c);
                    let is_char = matches!((self.peek(), self.peek2()), (Some('\\'), _) | (Some(_), Some('\'')));
                    if is_char {
                        while let Some(d) = self.bump() {
                            out.push(d);
                            if d == '\\' {
                                if let Some(e) = self.bump() {
                                    out.push(e);
                                }
                            } else if d == '\'' {
                                break;
                            }
                        }
                    }
                    continue;
                }
                '/' if self.peek() == Some('/') => {
                    out.push(c);
                    while let Some(d) = self.peek() {
                        if d == '\n' {
                            break;
                        }
                        out.push(d);
                        self.bump();
                    }
                    continue;
                }
                '/' if self.peek() == Some('*') => {
                    out.push(c);
                    while let Some(d) = self.bump() {
                        out.push(d);
                        if d == '*' && self.peek() == Some('/') {
                            out.push(self.bump().unwrap());
                            break;
                        }
                    }
                    continue;
                }
                _ => {}
            }
            out.push(c);
        }
    }

    fn symbol(&mut self, pos: Pos) -> Result<()> {
        let c = self.bump().unwrap();
        let next = self.peek();
        let two = |l: &mut Self, t: Tok| {
            l.bump();
            t
        };
        let tok = match (c, next) {
            ('-', Some('>')) => two(self, Tok::Arrow),
            ('=', Some('>')) => two(self, Tok::FatArrow),
            ('.', Some('.')) => two(self, Tok::DotDot),
            ('+', Some('=')) => two(self, Tok::PlusAssign),
            ('-', Some('=')) => two(self, Tok::MinusAssign),
            ('*', Some('=')) => two(self, Tok::StarAssign),
            ('/', Some('=')) => two(self, Tok::SlashAssign),
            ('%', Some('=')) => two(self, Tok::PercentAssign),
            ('=', Some('=')) => two(self, Tok::Eq),
            ('!', Some('=')) => two(self, Tok::Ne),
            ('<', Some('=')) => two(self, Tok::Le),
            ('>', Some('=')) => two(self, Tok::Ge),
            ('&', Some('&')) => two(self, Tok::And),
            ('|', Some('|')) => two(self, Tok::Or),
            ('(', _) => {
                self.brackets.push('(');
                Tok::LParen
            }
            (')', _) => {
                self.brackets.pop();
                Tok::RParen
            }
            ('[', _) => {
                self.brackets.push('[');
                Tok::LBracket
            }
            (']', _) => {
                self.brackets.pop();
                Tok::RBracket
            }
            ('{', _) => {
                self.brackets.push('{');
                Tok::LBrace
            }
            ('}', _) => {
                self.brackets.pop();
                Tok::RBrace
            }
            (',', _) => Tok::Comma,
            ('.', _) => Tok::Dot,
            (':', _) => Tok::Colon,
            (';', _) => Tok::Semi,
            ('?', _) => Tok::Question,
            ('+', _) => Tok::Plus,
            ('-', _) => Tok::Minus,
            ('*', _) => Tok::Star,
            ('/', _) => Tok::Slash,
            ('%', _) => Tok::Percent,
            ('=', _) => Tok::Assign,
            ('<', _) => Tok::Lt,
            ('>', _) => Tok::Gt,
            ('!', _) => Tok::Not,
            _ => return err(format!("unexpected character `{}`", c), pos),
        };
        self.push(tok, pos);
        Ok(())
    }
}
