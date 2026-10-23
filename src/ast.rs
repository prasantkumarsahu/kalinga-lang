use crate::error::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    Str,
    Void,
    /// A class, interface or enum name, with type arguments for generic classes.
    Named(String, Vec<TypeExpr>, Pos),
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
    /// `fn(int, str) -> bool`
    Func(Vec<TypeExpr>, Box<TypeExpr>),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    /// Type parameters: `fn first<T>(...)`.
    pub tparams: Vec<String>,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub body: Vec<Stmt>,
    pub is_override: bool,
    pub is_private: bool,
    pub is_static: bool,
    /// `abstract fn` has no body.
    pub is_abstract: bool,
    /// `mut fn` in a struct: the method may change `self`.
    pub is_mut: bool,
    /// `async fn`: calling it gives an `Async<T>` to `await`.
    pub is_async: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
    pub is_private: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct InitDecl {
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    /// Type parameters: `class Stack<T>`.
    pub tparams: Vec<String>,
    /// Names after `:` — at most one class, any number of interfaces.
    pub bases: Vec<(String, Pos)>,
    pub fields: Vec<FieldDecl>,
    pub init: Option<InitDecl>,
    pub methods: Vec<FnDecl>,
    pub is_abstract: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    /// Super-interfaces after `:`.
    pub bases: Vec<(String, Pos)>,
    pub methods: Vec<MethodSig>,
    pub pos: Pos,
}

/// `op +(other: Vec2) -> Vec2 { ... }` inside a struct.
#[derive(Debug, Clone)]
pub struct OpDecl {
    /// `+`, `-`, `*`, `/` or `%`.
    pub op: BinOp,
    /// Empty for unary minus.
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

/// `struct Vec2 { x: float, y: float }`: a value type, copied on assignment.
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    /// Type parameters: `struct Grid<T>`.
    pub tparams: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FnDecl>,
    pub ops: Vec<OpDecl>,
    /// `static NAME: Type = value` constants, used as `Struct.NAME`.
    pub consts: Vec<FieldDecl>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct CrateDep {
    pub name: String,
    pub version: String,
    pub features: Vec<String>,
    pub pos: Pos,
}

/// A Rust function Kalinga can call.
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    /// Rust path to call; defaults to `name` (found in a `use rust` file).
    pub path: Option<String>,
    /// `= rust { ... }`: an inline Rust body.
    pub body: Option<String>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub name: String,
    pub fields: Vec<Param>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<VariantDecl>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct Import {
    /// The path as written.
    pub path: String,
    pub alias: Option<String>,
    pub pos: Pos,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    /// `import "path"` / `import "path" as alias` declarations.
    pub imports: Vec<Import>,
    /// Set by the driver when the program uses `std/game`: `main` must then run on the
    /// process's main thread, which owns the window.
    pub main_thread: bool,
    /// `use rust "file.rs"`: Rust source files compiled into the program. After loading,
    /// the paths are absolute.
    pub rust_files: Vec<(String, Pos)>,
    /// `use crate "name" = "version" [features [...]]`: crates.io dependencies.
    pub crates: Vec<CrateDep>,
    /// `extern type Name`: opaque Rust types.
    pub extern_types: Vec<(String, Pos)>,
    /// `extern fn name(...) -> T [= "rust::path"]`
    pub extern_fns: Vec<ExternFn>,
    pub functions: Vec<FnDecl>,
    pub classes: Vec<ClassDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub enums: Vec<EnumDecl>,
    pub structs: Vec<StructDecl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn rust(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    SelfRef,
    Ident(String),
    Array(Vec<Expr>),
    /// `{k: v, ...}`
    Map(Vec<(Expr, Expr)>),
    /// `"text {expr} text"`
    Interp(Vec<InterpPart>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>),
    /// `name(args)` / `name<T>(args)` — a function call or a constructor.
    Call(String, Vec<TypeExpr>, Vec<Expr>),
    /// `obj.name(args)`
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `obj.name`
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// `super(args)` inside `init`
    SuperInit(Vec<Expr>),
    /// `super.name(args)`
    SuperCall(String, Vec<Expr>),
    /// `x as Dog` (an optional) or `x is Dog` (a bool)
    Downcast { value: Box<Expr>, class: String, test_only: bool },
    /// `await expr`
    Await(Box<Expr>),
    /// `expr?`: unwrap an `Ok` / non-`none` value or return the error / `none` early.
    Try(Box<Expr>),
    /// `fn(x: int) -> int { ... }` or `fn(x) => x * 2`
    Lambda { params: Vec<(String, Option<TypeExpr>, Pos)>, ret: Option<TypeExpr>, body: LambdaBody },
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone)]
pub enum ForIter {
    Range(Expr, Expr),
    Each(Expr),
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    /// `Circle(r)` / `Shape.Circle(r)` / `Empty`; `_` bindings are skipped.
    Variant(String, Vec<String>),
    Int(i64),
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let { name: String, mutable: bool, ty: Option<TypeExpr>, value: Expr },
    Assign { target: Expr, op: AssignOp, value: Expr },
    If { cond: Expr, then: Vec<Stmt>, otherwise: Option<Vec<Stmt>> },
    IfLet { name: String, value: Expr, then: Vec<Stmt>, otherwise: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    Match { value: Expr, arms: Vec<MatchArm> },
    For { var: String, iter: ForIter, body: Vec<Stmt> },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
}
