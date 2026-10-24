//! The typed tree: function bodies after type checking.
//!
//! Every expression carries its type, every name is resolved (a local, a field, a function,
//! a constructor, a method, an operator, a built-in), and every implicit conversion is an
//! explicit `Convert` node. Code generation only has to print this tree; it never needs to
//! check anything. Editors use the same tree for hover and navigation.

use std::collections::{HashMap, HashSet};

use kalinga_syntax::ast::{AssignOp, BinOp};
use kalinga_syntax::error::Span;

use crate::types::{Signature, Type};

#[derive(Debug, Clone)]
pub struct TExpr {
    pub kind: TExprKind,
    pub ty: Type,
    pub pos: Span,
}

#[derive(Debug, Clone)]
pub enum TExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    /// `none`.
    None,
    /// A local variable or parameter.
    Local(String),
    /// A `var` that a lambda changes: the lambda and the code around it share it.
    SharedLocal(String),
    /// A top-level function used as a value.
    FnRef(String),
    /// `self` in a struct method: a copy of the value.
    StructSelf,
    /// `self` used as a value in a class method: a reference to the object.
    ObjectSelf,
    /// `value`, converted to this expression's type: wrapped in an optional, upcast to a
    /// base class or interface, or converted element by element.
    Convert(Box<TExpr>),
    Array(Vec<TExpr>),
    /// `(a, b)`
    Tuple(Vec<TExpr>),
    /// `t.0`: a part of a tuple.
    TupleField {
        recv: Box<TExpr>,
        index: usize,
    },
    Map(Vec<(TExpr, TExpr)>),
    Interp(Vec<TInterp>),
    Neg(Box<TExpr>),
    Not(Box<TExpr>),
    /// Arithmetic, comparisons, `&&` / `||`, and `+` joining two strings.
    Binary(BinOp, Box<TExpr>, Box<TExpr>),
    /// `x == none` or `x != none`.
    IsNone {
        value: Box<TExpr>,
        negated: bool,
    },
    /// A struct operator, `op_N` of the struct `lhs` (or `rhs` when `swapped`, for `2.0 * v`);
    /// `rhs` is `None` for unary minus.
    StructOp {
        index: usize,
        lhs: Box<TExpr>,
        rhs: Option<Box<TExpr>>,
        swapped: bool,
    },
    /// `value as Target` (an optional) or `value is Target` (a bool).
    Downcast {
        value: Box<TExpr>,
        target: String,
        via: Cast,
        test_only: bool,
    },
    Lambda(Box<TLambda>),
    /// `await value`, on an `Async<T>` or a `Task<T>`.
    Await(Box<TExpr>),
    /// `value?`, on a `Result` or an optional.
    Try(Box<TExpr>),
    /// `recv.field` on a struct, an object or a generic object.
    Field {
        recv: Box<TExpr>,
        field: String,
    },
    /// `self.field` inside a method; `wrapped` for a generic class's `T` field, stored as
    /// `Option<T>`.
    SelfField {
        field: String,
        wrapped: bool,
    },
    /// A top-level `const` (`owner` empty), or `Type.NAME` for a `const` or `static var`
    /// member.
    Static {
        owner: String,
        name: String,
        mutable: bool,
    },
    /// `Math.PI`, `Math.TAU`, `Math.E`, `Math.INF`.
    MathConst(String),
    /// `if` used as a value.
    If {
        /// `if let NAME = cond`: `cond` is optional, and `then` sees its value as `NAME`.
        binding: Option<String>,
        cond: Box<TExpr>,
        then: TValueBlock,
        otherwise: TValueBlock,
    },
    /// `match` used as a value.
    Match {
        value: Box<TExpr>,
        arms: Vec<(TPattern, TValueBlock)>,
    },
    /// `Enum.Variant` or `Enum.Variant(args)`.
    Variant {
        enum_name: String,
        variant: String,
        args: Vec<TExpr>,
    },
    /// `base[index]` on an array, a map or a string (one character).
    Index {
        base: Box<TExpr>,
        index: Box<TExpr>,
    },
    /// `base[from..to]` on an array or a string; a missing end is the start or the end.
    Slice {
        base: Box<TExpr>,
        from: Option<Box<TExpr>>,
        to: Option<Box<TExpr>>,
    },
    Call {
        callee: Callee,
        args: Vec<TExpr>,
    },
    /// A built-in or library function.
    Builtin {
        op: Builtin,
        args: Vec<TExpr>,
    },
    /// A method of a built-in type (`xs.map(f)`, `task.join()`, ...).
    BuiltinMethod {
        op: BuiltinMethod,
        recv: Box<TExpr>,
        args: Vec<TExpr>,
    },
    /// A method that changes its receiver where it is stored.
    Mutate {
        place: TPlace,
        op: Mutation,
        args: Vec<TExpr>,
    },
}

/// One piece of `"text {expr} text"`.
#[derive(Debug, Clone)]
pub enum TInterp {
    Lit(String),
    Expr(Box<TExpr>),
}

/// How a downcast asks the object for its class.
#[derive(Debug, Clone)]
pub enum Cast {
    /// The value already has the target class.
    Same,
    /// Through the `__as_Target` hook of this Rust trait (a class hierarchy's root trait, or
    /// an interface).
    Hook(String),
}

#[derive(Debug, Clone)]
pub enum Callee {
    /// A top-level function, with type arguments when it is generic.
    Function { name: String, targs: Vec<Type> },
    /// An `extern fn`, called by its Rust path.
    Extern { path: String },
    /// A function value (a local, a field, ...).
    Value(Box<TExpr>),
    /// `Class(...)`; `targs` for a generic class.
    New { class: String, targs: Vec<Type> },
    /// `Struct(...)`; `args` hold every field, defaults filled in.
    StructNew { strukt: String, targs: Vec<Type> },
    /// `Type.method(...)` for a `static fn` of a class or struct (`targs` for a generic struct).
    Static { owner: String, name: String, targs: Vec<Type> },
    /// `self.method(...)` inside a class.
    SelfMethod(String),
    /// `super.method(...)`; `owner` defines the body that runs.
    SuperMethod { owner: String, name: String },
    /// A method of an object; `dispatch` is the Rust trait to call it through, for base-class
    /// and interface references.
    Method { recv: Box<TExpr>, name: String, dispatch: Option<String> },
    /// A struct method that does not change the struct.
    StructMethod { recv: Box<TExpr>, name: String },
    /// `Ok(value)` / `Ok()`.
    Ok,
    /// `Err(error)`.
    Err,
}

/// Built-in functions and library functions. Arguments are already converted to the types
/// the function needs (`ToFloat` wraps ints passed to float parameters).
#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    Print,
    Println,
    Len,
    ToStr,
    ToInt,
    ToFloat,
    Input,
    /// `chr(code)`: the character with that code point.
    Chr,
    /// `args()`: the program's command-line arguments.
    Args,
    /// `assert(condition)` or `assert(condition, message)`.
    Assert,
    /// `assert_eq(a, b)`.
    AssertEq,
    /// `Math.sqrt` and the other one-argument float functions; the Rust method name.
    MathUnary(String),
    /// `Math.pow`, `Math.atan2`, `Math.hypot`; the Rust method name.
    MathBinary(String),
    Lerp,
    Abs,
    Sign,
    Min,
    Max,
    Clamp,
    RandInt,
    RandFloat,
    Chance,
    Seed,
    Pick,
    Spawn,
    Cores,
    Wait,
    AsyncSpawn,
    AsyncRun,
    AsyncAll,
    Now,
    Millis,
    Sleep,
    NewChannel,
    NewShared,
}

/// Methods of built-in types that do not change their receiver.
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinMethod {
    Join,
    IsDone,
    Send,
    Receive,
    TryReceive,
    Close,
    Get,
    Set,
    Update,
    ParMap,
    IsOk,
    IsErr,
    OkValue,
    ErrValue,
    UnwrapOr,
    Unwrap,
    ArrayLen,
    ArrayMap,
    Filter,
    Find,
    Contains,
    MapLen,
    MapGet,
    MapHas,
    Keys,
    Values,
    StrLen,
    Upper,
    Lower,
    Trim,
    StrContains,
    Split,
    Replace,
    StartsWith,
    EndsWith,
    StrFind,
    CodeAt,
    Chars,
    ParseInt,
    ParseFloat,
    /// `join` on an array of strings.
    ArrayJoin,
}

/// Methods that change their receiver in place.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    Push,
    Pop,
    Sort,
    SortBy,
    Remove,
    Shuffle,
    /// A struct's `mut fn`.
    StructMethod(String),
}

/// Somewhere a value can be stored.
#[derive(Debug, Clone)]
pub enum TPlace {
    Local {
        name: String,
        ty: Type,
    },
    /// A `var` shared with the lambdas that change it.
    SharedLocal {
        name: String,
        ty: Type,
    },
    /// `self.field`; `wrapped` as in `TExprKind::SelfField`.
    SelfField {
        field: String,
        ty: Type,
        wrapped: bool,
    },
    /// A field of a struct stored in another place.
    StructField {
        base: Box<TPlace>,
        field: String,
        ty: Type,
    },
    /// A field of an object whose class has no subclasses, or of a generic object.
    ObjectField {
        recv: Box<TExpr>,
        field: String,
        ty: Type,
        wrapped: bool,
    },
    /// An element of an array, or an entry of a map.
    Index {
        base: Box<TPlace>,
        index: Box<TExpr>,
        ty: Type,
    },
    /// `Type.name` for a `static var`.
    Static {
        owner: String,
        name: String,
        ty: Type,
    },
}

impl TPlace {
    pub fn ty(&self) -> &Type {
        match self {
            TPlace::Local { ty, .. }
            | TPlace::SharedLocal { ty, .. }
            | TPlace::SelfField { ty, .. }
            | TPlace::StructField { ty, .. }
            | TPlace::ObjectField { ty, .. }
            | TPlace::Index { ty, .. }
            | TPlace::Static { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TStmt {
    /// `let (a, b) = value`; each name with whether it is shared with lambdas.
    LetTuple {
        names: Vec<(String, bool)>,
        mutable: bool,
        value: TExpr,
    },
    Let {
        name: String,
        mutable: bool,
        /// Shared with lambdas that change it (`SharedLocal`).
        shared: bool,
        ty: Type,
        value: TExpr,
    },
    /// `place = value` or `place op= value` on a number or string.
    Assign {
        place: TPlace,
        op: AssignOp,
        value: TExpr,
    },
    /// `recv.field op= value` through a base-class reference, which goes through the class's
    /// generated setter.
    SetField {
        recv: TExpr,
        field: String,
        op: AssignOp,
        value: TExpr,
    },
    /// `map[key] = value`.
    MapInsert {
        map: TPlace,
        key: TExpr,
        value: TExpr,
    },
    /// `place op= value` with the struct operator `op_N`.
    StructAssign {
        place: TPlace,
        index: usize,
        value: TExpr,
    },
    If {
        cond: TExpr,
        then: Vec<TStmt>,
        otherwise: Option<Vec<TStmt>>,
    },
    IfLet {
        name: String,
        value: TExpr,
        then: Vec<TStmt>,
        otherwise: Option<Vec<TStmt>>,
    },
    Match {
        value: TExpr,
        arms: Vec<TArm>,
    },
    While {
        cond: TExpr,
        body: Vec<TStmt>,
    },
    WhileLet {
        name: String,
        value: TExpr,
        body: Vec<TStmt>,
    },
    ForRange {
        var: String,
        from: TExpr,
        to: TExpr,
        /// `a..=b`
        inclusive: bool,
        body: Vec<TStmt>,
    },
    /// Over an array's elements, a map's keys or a string's characters; with `index`, also
    /// the position (arrays, strings) or the key, `var` then being the value (maps).
    ForEach {
        index: Option<String>,
        var: String,
        iter: TExpr,
        body: Vec<TStmt>,
    },
    Return(Option<TExpr>),
    Break,
    Continue,
    Expr(TExpr),
    /// `super(args)` in `init`; `owner` defines the `init` that runs.
    SuperInit {
        owner: String,
        args: Vec<TExpr>,
    },
}

/// A branch of an `if` or `match` used as a value: statements, then the value.
#[derive(Debug, Clone)]
pub struct TValueBlock {
    pub stmts: Vec<TStmt>,
    pub value: Box<TExpr>,
}

#[derive(Debug, Clone)]
pub struct TArm {
    pub pattern: TPattern,
    pub body: Vec<TStmt>,
}

#[derive(Debug, Clone)]
pub enum TPattern {
    Wildcard,
    /// `Variant(a, _, c)`; `enum_name` is `Result` for `Ok` / `Err`. `None` binds nothing.
    Variant {
        enum_name: String,
        variant: String,
        binds: Vec<Option<String>>,
    },
    Int(i64),
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct TLambda {
    pub params: Vec<(String, Type)>,
    /// Outer variables the body uses; each gets its own copy.
    pub captures: Vec<(String, Type)>,
    pub body: TLambdaBody,
    pub ret: Type,
}

#[derive(Debug, Clone)]
pub enum TLambdaBody {
    Expr(Box<TExpr>),
    Block(Vec<TStmt>),
}

/// A checked function, method, operator or `init` body.
#[derive(Debug, Clone)]
pub struct TBody {
    pub sig: Signature,
    pub stmts: Vec<TStmt>,
    pub is_async: bool,
}

/// A checked `test fn`.
#[derive(Debug, Clone)]
pub struct TTest {
    pub name: String,
    pub body: TBody,
    /// Where it is declared (its file decides whether `kalinga test` runs it).
    pub pos: Span,
}

/// Everything type checking produces for code generation.
#[derive(Debug, Default)]
pub struct TypedProgram {
    /// The `test fn`s of every file, in order.
    pub tests: Vec<TTest>,
    /// Top-level functions, by name.
    pub functions: HashMap<String, TBody>,
    /// (class, method) → body; abstract methods have none.
    pub methods: HashMap<(String, String), Option<TBody>>,
    /// (class, static method) → body.
    pub statics: HashMap<(String, String), TBody>,
    pub inits: HashMap<String, TBody>,
    /// (class, field) → its default value, for fields that declare one.
    pub field_defaults: HashMap<(String, String), TExpr>,
    /// (struct, method) → body.
    pub struct_methods: HashMap<(String, String), TBody>,
    /// (type, name) → initial value of a `const` or `static var`; the type is empty for
    /// top-level constants.
    pub static_values: HashMap<(String, String), TExpr>,
    /// (struct, operator index) → body.
    pub struct_ops: HashMap<(String, usize), TBody>,
    /// Classes whose methods use `self` as a value.
    pub self_values: HashSet<String>,
    /// Classes used as the target of `as` / `is`.
    pub cast_targets: HashSet<String>,
}
