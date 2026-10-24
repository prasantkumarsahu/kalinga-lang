//! Where the cursor is, read from the text: completion must work on half-written code,
//! which the compiler cannot parse.

use kalinga_syntax::lexer;
use kalinga_syntax::token::Tok;

use crate::text;

/// What kind of place the cursor is at.
#[derive(Debug, Clone, PartialEq)]
pub enum Context {
    /// Inside a string or a comment: nothing to offer.
    Nothing,
    /// Inside `import "`.
    ImportPath,
    /// Where a type is written.
    Type,
    /// After `class Name :`.
    ClassHeader,
    /// After `value.`; `recv` is the text before the dot.
    Member { recv: String },
    /// At the start of a line in a class, struct or interface body.
    Body(Opener),
    /// At the start of a match arm; `scrutinee` is the matched expression's text.
    MatchArm { scrutinee: String },
    /// At the start of a line outside every declaration.
    TopLevel,
    /// Anywhere an expression or statement can go.
    Expression,
}

/// What opened the innermost `{` around the cursor.
#[derive(Debug, Clone, PartialEq)]
pub enum Opener {
    Class { name: String, bases: Vec<String>, is_abstract: bool },
    Struct(String),
    Interface,
    Enum,
    Match(String),
    Block,
}

/// The cursor's context. `offset` is a byte offset; `word_start` is where the identifier
/// being typed starts.
pub fn context(text: &str, offset: usize, word_start: usize) -> Context {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..word_start];
    let trimmed = before.trim_start();
    if trimmed.starts_with("import ") && before.matches('"').count() == 1 {
        return Context::ImportPath;
    }
    let Some(code) = code_part(before) else { return Context::Nothing };
    if code != before {
        // The cursor is in a `{...}` of a string: an expression.
        return member_or_expression(code);
    }
    if is_class_header(trimmed) {
        return Context::ClassHeader;
    }
    if trimmed.is_empty() {
        return match openers(&text[..line_start]).last() {
            None => Context::TopLevel,
            Some(Opener::Match(s)) => Context::MatchArm { scrutinee: s.clone() },
            Some(o @ (Opener::Class { .. } | Opener::Struct(_) | Opener::Interface)) => Context::Body(o.clone()),
            Some(Opener::Enum) => Context::Nothing,
            Some(Opener::Block) => Context::Expression,
        };
    }
    let in_type_body = matches!(openers(&text[..line_start]).last(), Some(Opener::Class { .. } | Opener::Struct(_)));
    if is_type_position(trimmed, in_type_body) {
        return Context::Type;
    }
    member_or_expression(before)
}

fn member_or_expression(before: &str) -> Context {
    match before.strip_suffix('.') {
        Some(recv) if !recv.ends_with('.') => Context::Member { recv: receiver(recv) },
        _ => Context::Expression,
    }
}

/// The expression before a `.`: the trailing run of identifiers, dots, calls and indexes.
fn receiver(text: &str) -> String {
    let mut depth = 0i32;
    let mut start = text.len();
    for (i, c) in text.char_indices().rev() {
        match c {
            ')' | ']' => depth += 1,
            '(' | '[' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            c if c.is_alphanumeric() || c == '_' || c == '.' => {}
            _ => break,
        }
        start = i;
    }
    text[start..].to_string()
}

/// The part of a line that is code: `None` inside a comment or a string's text; inside
/// a string's `{...}`, the code there.
pub fn code_part(line: &str) -> Option<&str> {
    // (in a string, start of the current `{...}` code when inside one)
    let mut string: Option<Option<usize>> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match (string, c) {
            (None, '/') if chars.peek().map(|(_, n)| *n) == Some('/') => return None,
            (None, '"') => string = Some(None),
            (Some(None), '\\') => {
                chars.next();
            }
            (Some(None), '"') => string = None,
            (Some(None), '{') => string = Some(Some(i + 1)),
            (Some(Some(_)), '}') => string = Some(None),
            _ => {}
        }
    }
    match string {
        None => Some(line),
        Some(None) => None,
        Some(Some(start)) => Some(&line[start..]),
    }
}

fn is_class_header(line: &str) -> bool {
    let rest = line.strip_prefix("abstract ").unwrap_or(line);
    let Some(rest) = rest.strip_prefix("class ").or_else(|| rest.strip_prefix("interface ")) else { return false };
    match rest.split_once(':') {
        Some((name, bases)) => !name.contains('{') && !bases.contains('{'),
        None => false,
    }
}

/// After `let x:`, `-> `, a parameter's `:`, a field's `:` in a type body, `as`, `is`, or
/// inside type brackets.
fn is_type_position(line: &str, in_type_body: bool) -> bool {
    let t = line.trim_end();
    if t.ends_with("->") || t.ends_with(" as") || t.ends_with(" is") {
        return true;
    }
    // Strip a type being written: `[`, `{`, `<`, `,` and names inside them.
    let core = t.trim_end_matches(|c: char| "[{<, ".contains(c));
    let Some(head) = core.strip_suffix(':') else { return false };
    let head = head.trim_end();
    let name_start = text::name_start(head);
    let (lead, name) = head.split_at(name_start);
    if name.is_empty() {
        return false;
    }
    let lead = lead.trim();
    let after_keyword = ["let", "var", "const", "static var", "private"].iter().any(|k| lead.ends_with(k));
    after_keyword || (lead.is_empty() && in_type_body) || in_parameter_list(lead)
}

/// Is the text inside the parameter list of `fn name(`, `fn(` or `init(`?
fn in_parameter_list(lead: &str) -> bool {
    let mut depth = 0i32;
    for (i, c) in lead.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let before = lead[..i].trim_end();
                let word_start = text::name_start(before);
                let word = &before[word_start..];
                let prev = before[..word_start].trim_end();
                return word == "fn" || word == "init" || prev.ends_with("fn") || prev.ends_with("op");
            }
            _ => {}
        }
    }
    false
}

/// The `{`s still open at the end of `text`, outermost first.
pub fn openers(text: &str) -> Vec<Opener> {
    let Ok(tokens) = lexer::lex(text, 0) else { return vec![] };
    let mut stack: Vec<Opener> = vec![];
    for (i, t) in tokens.iter().enumerate() {
        match t.tok {
            Tok::LBrace => {
                let start = tokens[..i]
                    .iter()
                    .rposition(|p| matches!(p.tok, Tok::Newline | Tok::Semi | Tok::LBrace | Tok::RBrace))
                    .map_or(0, |j| j + 1);
                stack.push(opener(&tokens[start..i], text));
            }
            Tok::RBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack
}

/// What a `{` opens, from the tokens before it on its line.
fn opener(header: &[kalinga_syntax::token::Token], text: &str) -> Opener {
    let toks: Vec<&Tok> = header.iter().map(|t| &t.tok).collect();
    let ident = |t: Option<&&Tok>| match t {
        Some(Tok::Ident(n)) => Some(n.clone()),
        _ => None,
    };
    match toks.as_slice() {
        [Tok::Abstract, Tok::Class, rest @ ..] | [Tok::Class, rest @ ..] => {
            let name = ident(rest.first()).unwrap_or_default();
            let colon = rest.iter().position(|t| **t == Tok::Colon);
            let bases =
                colon.map(|c| rest[c + 1..].iter().filter_map(|t| ident(Some(t))).collect()).unwrap_or_default();
            Opener::Class { name, bases, is_abstract: toks.first() == Some(&&Tok::Abstract) }
        }
        [Tok::Struct, rest @ ..] => Opener::Struct(ident(rest.first()).unwrap_or_default()),
        [Tok::Interface, ..] => Opener::Interface,
        [Tok::Enum, ..] => Opener::Enum,
        _ => match header.iter().position(|t| t.tok == Tok::Match) {
            // `match` can follow `let x =` or `return`.
            Some(m) => Opener::Match(source_between(text, header, m + 1)),
            None => Opener::Block,
        },
    }
}

/// The source text of the tokens from `from` to the end of `header`, on their line.
fn source_between(text: &str, header: &[kalinga_syntax::token::Token], from: usize) -> String {
    let (Some(first), Some(last)) = (header.get(from), header.last()) else { return String::new() };
    let line = crate::text::line(text, first.pos.line - 1);
    line.chars().skip(first.pos.col - 1).take(last.pos.col + last.pos.len - first.pos.col).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> Context {
        let offset = src.find('|').unwrap_or(src.len());
        let text = src.replace('|', "");
        let word_start = text[..offset].trim_end_matches(|c: char| c.is_alphanumeric() || c == '_').len();
        context(&text, offset, word_start)
    }

    #[test]
    fn contexts() {
        assert_eq!(at("fn main() {\n    print(\"a.|\")\n}"), Context::Nothing);
        assert_eq!(at("fn main() {\n    // d.|\n}"), Context::Nothing);
        assert_eq!(at("fn main() {\n    let x: |\n}"), Context::Type);
        assert_eq!(at("fn f(a: int, b: |"), Context::Type);
        assert_eq!(at("fn main() {\n    clamp(low: |"), Context::Expression);
        assert_eq!(at("class Cat : |"), Context::ClassHeader);
        assert_eq!(at("fn main() {\n    d.sp|"), Context::Member { recv: "d".into() });
        assert_eq!(at("fn main() {\n    p.pets[0].|"), Context::Member { recv: "p.pets[0]".into() });
        assert_eq!(at("fn main() {\n    print(\"hi {n.|}\")"), Context::Member { recv: "n".into() });
        assert_eq!(at("\n|"), Context::TopLevel);
        assert_eq!(at("import \"|"), Context::ImportPath);
        assert_eq!(at("struct V {\n    x: |"), Context::Type);
        assert_eq!(at("fn main() {\n    match s {\n        |"), Context::MatchArm { scrutinee: "s".into() });
        let class = at("class Cat : Animal, Show {\n    |");
        let expected =
            Opener::Class { name: "Cat".into(), bases: vec!["Animal".into(), "Show".into()], is_abstract: false };
        assert_eq!(class, Context::Body(expected));
    }
}
