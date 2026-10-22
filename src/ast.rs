use crate::error::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Named(String, Pos),
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
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
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub body: Vec<Stmt>,
    pub is_override: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
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
    /// Names after `:` — at most one class, any number of interfaces.
    pub bases: Vec<(String, Pos)>,
    pub fields: Vec<FieldDecl>,
    pub init: Option<InitDecl>,
    pub methods: Vec<FnDecl>,
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
    pub methods: Vec<MethodSig>,
    pub pos: Pos,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    /// `import "path"` declarations: (path as written, position).
    pub imports: Vec<(String, Pos)>,
    pub functions: Vec<FnDecl>,
    pub classes: Vec<ClassDecl>,
    pub interfaces: Vec<InterfaceDecl>,
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
    /// `name(args)` — a function call or a constructor.
    Call(String, Vec<Expr>),
    /// `obj.name(args)`
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `obj.name`
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// `super(args)` inside `init`
    SuperInit(Vec<Expr>),
    /// `super.name(args)`
    SuperCall(String, Vec<Expr>),
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
    For { var: String, iter: ForIter, body: Vec<Stmt> },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
}
