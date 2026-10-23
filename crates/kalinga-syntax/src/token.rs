use crate::error::Span;

/// Piece of an interpolated string: literal text or the source of a `{...}` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    Code(String, Span),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // literals and names
    Int(i64),
    Float(f64),
    Str(String),
    InterpStr(Vec<StrPart>),
    /// Raw Rust code of `= rust { ... }` (without the outer braces).
    RustBlock(String),
    Ident(String),

    // keywords
    Fn,
    Class,
    Interface,
    Struct,
    Import,
    Use,
    Extern,
    Async,
    Await,
    Enum,
    Match,
    As,
    Is,
    Init,
    Override,
    Private,
    Static,
    Const,
    Abstract,
    Let,
    Var,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    None,
    SelfKw,
    Super,

    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    DotDot,
    Colon,
    Semi,
    Arrow,
    FatArrow,
    Question,

    // operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Not,

    Newline,
    Eof,
}

impl Tok {
    pub fn keyword(word: &str) -> Option<Tok> {
        Some(match word {
            "fn" => Tok::Fn,
            "class" => Tok::Class,
            "interface" => Tok::Interface,
            "struct" => Tok::Struct,
            "use" => Tok::Use,
            "async" => Tok::Async,
            "await" => Tok::Await,
            "extern" => Tok::Extern,
            "import" => Tok::Import,
            "enum" => Tok::Enum,
            "match" => Tok::Match,
            "as" => Tok::As,
            "is" => Tok::Is,
            "init" => Tok::Init,
            "override" => Tok::Override,
            "private" => Tok::Private,
            "static" => Tok::Static,
            "const" => Tok::Const,
            "abstract" => Tok::Abstract,
            "let" => Tok::Let,
            "var" => Tok::Var,
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "for" => Tok::For,
            "in" => Tok::In,
            "return" => Tok::Return,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "true" => Tok::True,
            "false" => Tok::False,
            "none" => Tok::None,
            "self" => Tok::SelfKw,
            "super" => Tok::Super,
            _ => return None,
        })
    }

    /// Human-readable form used in error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Int(n) => format!("number `{}`", n),
            Tok::Float(n) => format!("number `{}`", n),
            Tok::Str(_) | Tok::InterpStr(_) => "string".to_string(),
            Tok::RustBlock(_) => "Rust block".to_string(),
            Tok::Ident(s) => format!("`{}`", s),
            Tok::Newline => "end of line".to_string(),
            Tok::Eof => "end of file".to_string(),
            other => format!("`{}`", other.symbol()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Tok::Fn => "fn",
            Tok::Class => "class",
            Tok::Interface => "interface",
            Tok::Struct => "struct",
            Tok::Use => "use",
            Tok::Async => "async",
            Tok::Await => "await",
            Tok::Extern => "extern",
            Tok::Import => "import",
            Tok::Enum => "enum",
            Tok::Match => "match",
            Tok::As => "as",
            Tok::Is => "is",
            Tok::FatArrow => "=>",
            Tok::Init => "init",
            Tok::Override => "override",
            Tok::Private => "private",
            Tok::Static => "static",
            Tok::Const => "const",
            Tok::Abstract => "abstract",
            Tok::Let => "let",
            Tok::Var => "var",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Return => "return",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::True => "true",
            Tok::False => "false",
            Tok::None => "none",
            Tok::SelfKw => "self",
            Tok::Super => "super",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::Colon => ":",
            Tok::Semi => ";",
            Tok::Arrow => "->",
            Tok::Question => "?",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Assign => "=",
            Tok::PlusAssign => "+=",
            Tok::MinusAssign => "-=",
            Tok::StarAssign => "*=",
            Tok::SlashAssign => "/=",
            Tok::PercentAssign => "%=",
            Tok::Eq => "==",
            Tok::Ne => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::And => "&&",
            Tok::Or => "||",
            Tok::Not => "!",
            _ => "?",
        }
    }

    /// A newline right after one of these tokens continues the statement. (`>` is not one:
    /// a generic type such as `Result<int, str>` often ends a line.)
    pub fn continues_line(&self) -> bool {
        matches!(
            self,
            Tok::Plus
                | Tok::Minus
                | Tok::Star
                | Tok::Slash
                | Tok::Percent
                | Tok::Assign
                | Tok::PlusAssign
                | Tok::MinusAssign
                | Tok::StarAssign
                | Tok::SlashAssign
                | Tok::PercentAssign
                | Tok::Eq
                | Tok::Ne
                | Tok::Lt
                | Tok::Le
                | Tok::Ge
                | Tok::And
                | Tok::Or
                | Tok::Not
                | Tok::Comma
                | Tok::Dot
                | Tok::DotDot
                | Tok::Arrow
                | Tok::FatArrow
                | Tok::Colon
                | Tok::Newline
        )
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub pos: Span,
}
