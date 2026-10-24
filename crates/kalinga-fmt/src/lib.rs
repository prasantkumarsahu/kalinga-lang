//! `kalinga fmt`: lays out Kalinga code in one standard style.
//!
//! The formatter changes only whitespace and line breaks, never tokens: comments, blank-line
//! groups the user chose, parentheses and the spelling of literals all stay. It
//! - indents with 4 spaces per open bracket level (one level per line that opens brackets,
//!   however many it opens), one more for a continued expression;
//! - spaces tokens the standard way (`a + b`, `f(x, y)`, `x: int`, `xs[0]`, `-1`,
//!   `Result<int, str>`, `{"k": 1}`, `{ return 1 }`);
//! - puts one statement per line (`a; b` becomes two lines);
//! - lines up the trailing `//` comments of consecutive lines;
//! - keeps at most one blank line in a row, none just inside braces, and one after a
//!   top-level declaration that takes several lines;
//! - ends the file with exactly one newline, keeping the file's line endings (`\n` or
//!   `\r\n`, as on its first line).
//!
//! The result is checked to have the same tokens as the input, and formatting it again
//! changes nothing.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

mod spacing;

use kalinga_syntax::error::{Diagnostic, Result, Span};
use kalinga_syntax::lexer::{self, Piece};
use kalinga_syntax::token::{StrPart, Tok};

use spacing::{Item, render};

/// `source` in the standard layout.
pub fn format(source: &str) -> Result<String> {
    let pieces = lexer::layout(source)?;
    let mut out = Formatter::default().run(lines(pieces));
    if source.split('\n').next().is_some_and(|l| l.ends_with('\r')) {
        out = out.replace('\n', "\r\n");
    }
    if tokens(source)? != tokens(&out)? {
        return Err(Diagnostic::error(
            "the formatter would change the program; please report this as a bug",
            Span::default(),
        ));
    }
    Ok(out)
}

/// The tokens that make up the program: statement separators (newlines and `;`) do not
/// count, since the formatter moves them, and neither does where the code inside a
/// string's `{...}` sits, since re-indenting a line moves it.
fn tokens(source: &str) -> Result<Vec<Tok>> {
    let without_position = |p: StrPart| match p {
        StrPart::Code(code, _) => StrPart::Code(code, Span::default()),
        lit => lit,
    };
    Ok(lexer::lex(source, 0)?
        .into_iter()
        .map(|t| match t.tok {
            Tok::InterpStr(parts) => Tok::InterpStr(parts.into_iter().map(without_position).collect()),
            other => other,
        })
        .filter(|t| !matches!(t, Tok::Newline | Tok::Semi))
        .collect())
}

/// One output line before indentation: its tokens and comments.
type Line = Vec<Item>;

/// The source's lines, with lines holding several statements (`a; b`) split. An empty
/// line is a blank line.
fn lines(pieces: Vec<Piece>) -> Vec<Line> {
    let mut out: Vec<Line> = vec![vec![]];
    // Brackets opened on the current line: `;` inside them does not split it.
    let mut depth = 0i32;
    for piece in pieces {
        let current = out.last_mut().expect("there is always a current line");
        match piece {
            Piece::Newline => {
                out.push(vec![]);
                depth = 0;
            }
            Piece::Token(Tok::Semi, _) if depth <= 0 => {
                if !current.is_empty() {
                    out.push(vec![]);
                }
            }
            Piece::Token(tok, text) => {
                match tok {
                    Tok::LParen | Tok::LBracket | Tok::LBrace => depth += 1,
                    Tok::RParen | Tok::RBracket | Tok::RBrace => depth -= 1,
                    _ => {}
                }
                current.push(Item::Token(tok, text));
            }
            Piece::Comment(text) => current.push(Item::Comment(text)),
        }
    }
    out
}

/// What the lines so far left open, and how the last line ended.
#[derive(Default)]
struct Formatter {
    /// The lines written: (indented code, trailing `//` comment); a blank line is empty.
    out: Vec<(String, Option<String>)>,
    /// Open brackets, each with the indentation of the lines inside it.
    open: Vec<usize>,
    /// The previous line ended with an operator, so this one continues its expression.
    continues: bool,
    /// A blank line was seen since the last line written.
    blank: bool,
    /// The last line written closed a top-level declaration.
    closed_item: bool,
    /// The last line written ended by opening a bracket.
    opened: bool,
}

impl Formatter {
    fn run(mut self, lines: Vec<Line>) -> String {
        for line in lines {
            if line.is_empty() {
                self.blank = true;
                continue;
            }
            self.line(&line);
        }
        let mut text = align_comments(&self.out).join("\n");
        text.push('\n');
        text
    }

    fn line(&mut self, line: &Line) {
        let leading_closers = line.iter().take_while(|i| i.is_closer()).count();
        let mut indent = self.open.last().copied().unwrap_or(0);
        for _ in 0..leading_closers {
            indent = self.open.pop().map_or(0, |inner| inner.saturating_sub(1));
        }
        let starts_with_dot = matches!(line.first(), Some(Item::Token(Tok::Dot, _)));
        if leading_closers == 0 && (self.continues || starts_with_dot) {
            indent += 1;
        }
        self.blank_line(leading_closers > 0);
        for item in &line[leading_closers..] {
            if let Item::Token(tok, _) = item {
                match tok {
                    Tok::LParen | Tok::LBracket | Tok::LBrace => self.open.push(indent + 1),
                    Tok::RParen | Tok::RBracket | Tok::RBrace => {
                        self.open.pop();
                    }
                    _ => {}
                }
            }
        }
        let trailing = match line.last() {
            Some(Item::Comment(c)) if c.starts_with("//") && line.len() > 1 => Some(c.clone()),
            _ => None,
        };
        let code = render(&line[..line.len() - usize::from(trailing.is_some())]);
        self.out.push((format!("{}{}", "    ".repeat(indent), code), trailing));
        self.continues = line.iter().rev().find_map(Item::token).is_some_and(continues_expression);
        // A top-level declaration taking several lines ends here: a line that closes it,
        // or a Rust body over several lines.
        let last = line.iter().rev().find_map(Item::token);
        let multiline_rust = matches!(last, Some(Tok::RustBlock(body)) if body.contains('\n'));
        self.closed_item = self.open.is_empty() && (leading_closers > 0 || multiline_rust);
        self.opened = matches!(last, Some(Tok::LBrace | Tok::LParen | Tok::LBracket));
    }

    /// Writes the blank line before the current one, if it gets one: a blank line in the
    /// source (not just inside braces), or the separation after a top-level declaration.
    fn blank_line(&mut self, starts_with_closer: bool) {
        let wanted = (self.blank || self.closed_item) && !self.out.is_empty() && !self.opened && !starts_with_closer;
        if wanted {
            self.out.push((String::new(), None));
        }
        self.blank = false;
        self.closed_item = false;
    }
}

/// The lines as text, the trailing comments of consecutive lines starting in one column.
fn align_comments(lines: &[(String, Option<String>)]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let run = lines[i..].iter().take_while(|(_, c)| c.is_some()).count().max(1);
        let width = lines[i..i + run].iter().map(|(code, _)| code.chars().count()).max().unwrap_or(0);
        for (code, comment) in &lines[i..i + run] {
            match comment {
                Some(c) => out.push(format!("{:width$} {}", code, c, width = width)),
                None => out.push(code.clone()),
            }
        }
        i += run;
    }
    out
}

/// A line ending with this token continues on the next line.
fn continues_expression(t: &Tok) -> bool {
    t.continues_line() && !matches!(t, Tok::Comma | Tok::Colon | Tok::Dot | Tok::Newline)
}

#[cfg(test)]
mod tests;
