//! The spaces between the tokens of one line.

use kalinga_syntax::token::Tok;

/// A token (with its exact text) or a comment on a line.
#[derive(Debug, Clone)]
pub enum Item {
    Token(Tok, String),
    Comment(String),
}

impl Item {
    pub fn token(&self) -> Option<&Tok> {
        match self {
            Item::Token(t, _) => Some(t),
            Item::Comment(_) => None,
        }
    }

    pub fn is_closer(&self) -> bool {
        matches!(self.token(), Some(Tok::RParen | Tok::RBracket | Tok::RBrace))
    }

    fn text(&self) -> &str {
        match self {
            Item::Token(_, s) | Item::Comment(s) => s,
        }
    }
}

/// What a token does here, where its spacing depends on it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Plain,
    /// `-x`, `!x`
    Unary,
    /// `<` and `>` of `Result<int, str>`, `first<int>(...)`
    GenericOpen,
    GenericClose,
    /// `{` / `}` of a map literal or map type: `{"a": 1}`, `{str: int}`
    MapOpen,
    MapClose,
    /// The operator in `op +(other: V)`
    OperatorName,
    /// Spaced on both sides, whatever the token: the `:` of a class header.
    Spaced,
}

/// The line's text with standard spacing.
pub fn render(line: &[Item]) -> String {
    let mut roles = roles(line);
    // `class Dog : Animal, Show` and `interface Pet : Named`
    let header = matches!(line.first().and_then(Item::token), Some(Tok::Class | Tok::Abstract | Tok::Interface));
    if header && let Some(colon) = line.iter().position(|i| i.token() == Some(&Tok::Colon)) {
        roles[colon] = Role::Spaced;
    }
    let mut out = String::new();
    for (i, item) in line.iter().enumerate() {
        if i > 0 && spaced(&line[i - 1], roles[i - 1], item, roles[i]) {
            out.push(' ');
        }
        out.push_str(item.text());
    }
    out
}

fn roles(line: &[Item]) -> Vec<Role> {
    let mut roles = vec![Role::Plain; line.len()];
    // `{` roles on this line, to give the matching `}` the same kind.
    let mut braces: Vec<Role> = vec![];
    for (i, item) in line.iter().enumerate() {
        let Some(tok) = item.token() else { continue };
        let prev = line[..i].iter().rev().find_map(Item::token);
        roles[i] = match tok {
            Tok::Minus | Tok::Not if !prev.is_some_and(ends_operand) => Role::Unary,
            Tok::Lt if is_generic(line, i) => {
                if let Some(close) = matching_gt(line, i) {
                    roles[close] = Role::GenericClose;
                }
                Role::GenericOpen
            }
            Tok::LBrace => {
                let role = if prev.is_none_or(starts_map) { Role::MapOpen } else { Role::Plain };
                braces.push(role);
                role
            }
            Tok::RBrace => match braces.pop() {
                Some(Role::MapOpen) => Role::MapClose,
                _ => Role::Plain,
            },
            _ if matches!(prev, Some(Tok::Ident(n)) if n == "op") && is_operator(tok) => Role::OperatorName,
            _ => roles[i],
        };
    }
    roles
}

/// Can `t` end an operand (so a `-` after it subtracts)?
fn ends_operand(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Ident(_)
            | Tok::Int(_)
            | Tok::Float(_)
            | Tok::Str(_)
            | Tok::InterpStr(_)
            | Tok::RParen
            | Tok::RBracket
            | Tok::RBrace
            | Tok::True
            | Tok::False
            | Tok::None
            | Tok::SelfKw
            | Tok::Question
    )
}

/// Does a `{` after `t` start a map (literal or type) rather than a block?
fn starts_map(t: &Tok) -> bool {
    matches!(t, Tok::LParen | Tok::LBracket | Tok::Comma | Tok::Colon | Tok::Arrow | Tok::Lt | Tok::Return)
        || (t.continues_line() && !matches!(t, Tok::FatArrow | Tok::Newline | Tok::Dot))
}

fn is_operator(t: &Tok) -> bool {
    matches!(t, Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Percent)
}

/// Is the `<` at `i` the start of type arguments? Its `>` must follow on the line with
/// only type tokens between, after a name, and not be followed by an operand
/// (`a < b, c > d` compares).
fn is_generic(line: &[Item], i: usize) -> bool {
    let after_name = matches!(line[..i].iter().rev().find_map(Item::token), Some(Tok::Ident(_)));
    let Some(close) = matching_gt(line, i) else { return false };
    let next = line[close + 1..].iter().find_map(Item::token);
    after_name
        && !next
            .is_some_and(|t| ends_operand(t) && !matches!(t, Tok::RParen | Tok::RBracket | Tok::RBrace | Tok::Question))
}

/// The `>` closing the `<` at `i`, when only type tokens come between.
fn matching_gt(line: &[Item], i: usize) -> Option<usize> {
    let mut depth = 0;
    for (j, item) in line.iter().enumerate().skip(i) {
        match item.token()? {
            Tok::Lt => depth += 1,
            Tok::Gt => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            Tok::Ident(_) | Tok::Comma | Tok::LBracket | Tok::RBracket | Tok::LBrace | Tok::RBrace | Tok::Colon => {}
            Tok::Question | Tok::LParen | Tok::RParen | Tok::Arrow | Tok::Fn => {}
            _ => return None,
        }
    }
    None
}

/// Is there a space between `a` and `b`?
fn spaced(a: &Item, ra: Role, b: &Item, rb: Role) -> bool {
    let (Some(ta), Some(tb)) = (a.token(), b.token()) else { return true };
    if rb == Role::Spaced {
        return true;
    }
    let tight_after = matches!(ta, Tok::LParen | Tok::LBracket | Tok::Dot | Tok::DotDot | Tok::DotDotEq)
        || matches!(ra, Role::Unary | Role::GenericOpen | Role::MapOpen);
    let tight_before =
        matches!(tb, Tok::RParen | Tok::RBracket | Tok::Comma | Tok::Dot | Tok::Question | Tok::Colon | Tok::Semi)
            || matches!(tb, Tok::DotDot | Tok::DotDotEq)
            || matches!(rb, Role::GenericOpen | Role::GenericClose | Role::MapClose);
    if tight_after || tight_before {
        return false;
    }
    match tb {
        // `f(x)`, `fn(x)`, `init(x)`, `super(x)`, `Channel<int>()`, `op +(o: V)`
        Tok::LParen => !(names_call(ta) || ra == Role::GenericClose || ra == Role::OperatorName),
        // `xs[0]`, `f()[0]`, `"abc"[1]`, but `features ["std"]`
        Tok::LBracket => {
            matches!(ta, Tok::Ident(n) if n == "features")
                || !matches!(ta, Tok::Ident(_) | Tok::RParen | Tok::RBracket | Tok::Str(_) | Tok::InterpStr(_))
        }
        // `{}` stays empty.
        Tok::RBrace => *ta != Tok::LBrace,
        _ => true,
    }
}

fn names_call(t: &Tok) -> bool {
    matches!(t, Tok::Ident(_) | Tok::RParen | Tok::RBracket | Tok::Fn | Tok::Init | Tok::Super)
}
