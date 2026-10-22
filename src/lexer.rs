use crate::error::{err, Pos, Result};
use crate::token::{Tok, Token};

pub fn lex(source: &str) -> Result<Vec<Token>> {
    Lexer { chars: source.chars().collect(), i: 0, line: 1, col: 1, depth: 0, out: Vec::new() }.run()
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    /// Nesting of `(` and `[`; newlines inside them are ignored.
    depth: usize,
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

    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col }
    }

    fn push(&mut self, tok: Tok, pos: Pos) {
        if tok == Tok::Newline {
            // Collapse blank lines and drop newlines that cannot end a statement.
            if self.depth > 0 {
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
                    self.push(Tok::Newline, pos);
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

    fn string(&mut self, pos: Pos) -> Result<()> {
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                None | Some('\n') => return err("unterminated string", pos),
                Some('"') => break,
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
        self.push(Tok::Str(s), pos);
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
        let tok = Tok::keyword(&w).unwrap_or(Tok::Ident(w));
        self.push(tok, pos);
        Ok(())
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
                self.depth += 1;
                Tok::LParen
            }
            (')', _) => {
                self.depth = self.depth.saturating_sub(1);
                Tok::RParen
            }
            ('[', _) => {
                self.depth += 1;
                Tok::LBracket
            }
            (']', _) => {
                self.depth = self.depth.saturating_sub(1);
                Tok::RBracket
            }
            ('{', _) => Tok::LBrace,
            ('}', _) => Tok::RBrace,
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
