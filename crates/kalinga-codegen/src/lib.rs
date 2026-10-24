//! Prints a type-checked program as Rust source.
//!
//! The input is the typed tree from `kalinga_sema::typeck`, so nothing here can fail: every
//! name is resolved and every conversion is explicit. Object model (see How It Works in
//! the docs): every object is `Rc<RefCell<Struct>>`. A class with subclasses is referred to
//! through `Rc<RefCell<dyn __I_Class>>` so calls dispatch dynamically; leaf classes use the
//! concrete struct for static dispatch.

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

mod expr;
mod items;
mod stmt;

use kalinga_sema::tast::TypedProgram;
use kalinga_sema::{Model, Signature, Type, class_trait};
use kalinga_syntax::ast::Program;

/// What to generate.
pub struct Target<'a> {
    /// A program that runs the analysed file's `test fn`s instead of `main`.
    pub tests: bool,
    /// The source files' names (by `Span::file`), for the places messages point at.
    pub files: &'a [String],
}

/// The Rust source of a checked program.
pub fn generate(prog: &Program, model: &Model, typed: &TypedProgram, target: &Target) -> String {
    Gen { m: model, t: typed, tests: target.tests, files: target.files }.program(prog)
}

struct Gen<'a> {
    m: &'a Model,
    t: &'a TypedProgram,
    tests: bool,
    files: &'a [String],
}

impl Gen<'_> {
    /// `file:line` of a position, for messages.
    fn place_of(&self, pos: kalinga_syntax::error::Span) -> String {
        let file = self.files.get(pos.file as usize).map_or("?", String::as_str);
        format!("{}:{}", file, pos.line)
    }
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "const", "dyn", "enum", "extern", "impl", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "static", "struct", "trait", "type", "unsafe", "use", "where", "yield", "abstract", "become", "box", "do",
    "final", "macro", "priv", "typeof", "unsized", "virtual", "try", "gen", "union",
];

/// Rust identifier for a user-chosen variable, field or method name.
fn rid(name: &str) -> String {
    if name == "crate" || name == "Self" {
        format!("{}_", name)
    } else if RUST_KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else if name.starts_with(|c: char| c.is_uppercase()) {
        // Keep locals out of the type/constructor namespace (`None`, `Some`, ...).
        format!("__u_{}", name)
    } else {
        name.to_string()
    }
}

/// Rust name for a user function.
fn fn_id(name: &str) -> String {
    format!("kl_{}", name)
}

fn clone_if(code: String, ty: &Type) -> String {
    if ty.is_copy() { code } else { format!("{}.clone()", code) }
}

fn indent(n: usize) -> String {
    "    ".repeat(n)
}

impl Gen<'_> {
    fn rty(&self, t: &Type) -> String {
        match t {
            Type::Int => "i64".into(),
            Type::Float => "f64".into(),
            Type::Bool => "bool".into(),
            Type::Str => "String".into(),
            Type::Void | Type::NoneLit => "()".into(),
            Type::Array(i) => format!("Vec<{}>", self.rty(i)),
            Type::Map(k, v) => format!("BTreeMap<{}, {}>", self.rty(k), self.rty(v)),
            Type::Optional(i) => format!("Option<{}>", self.rty(i)),
            Type::Result(t, e) => format!("Result<{}, {}>", self.rty(t), self.rty(e)),
            Type::Channel(t) => format!("__Channel<{}>", self.rty(t)),
            Type::Shared(t) => format!("__Shared<{}>", self.rty(t)),
            Type::Task(t) => format!("__Task<{}>", self.rty(t)),
            Type::Async(t) => format!("__Async<{}>", self.rty(t)),
            Type::Class(c) if self.m.has_subclasses(c) => format!("Rc<RefCell<dyn {}>>", class_trait(c)),
            Type::Class(c) => format!("Rc<RefCell<{}>>", c),
            Type::Interface(i) => format!("Rc<RefCell<dyn {}>>", i),
            Type::Enum(e) | Type::Extern(e) => e.clone(),
            Type::Struct(n, args) if args.is_empty() => n.clone(),
            Type::Struct(n, args) => format!("{}<{}>", n, self.rtys(args)),
            Type::Func(ps, r) if **r == Type::Void => format!("Rc<dyn Fn({})>", self.rtys(ps)),
            Type::Func(ps, r) => format!("Rc<dyn Fn({}) -> {}>", self.rtys(ps), self.rty(r)),
            Type::Unknown => "_".into(),
            Type::Param(n) => n.clone(),
            Type::Generic(n, args) => format!("Rc<RefCell<{}<{}>>>", n, self.rtys(args)),
        }
    }

    fn rtys(&self, ts: &[Type]) -> String {
        ts.iter().map(|t| self.rty(t)).collect::<Vec<_>>().join(", ")
    }

    /// `::<A, B>` for explicit type arguments (empty when there are none).
    fn turbofish(&self, types: &[Type]) -> String {
        if types.is_empty() { String::new() } else { format!("::<{}>", self.rtys(types)) }
    }

    /// Return type of a generated function; `async fn`s return `__Async<T>`.
    fn fn_ret(&self, t: &Type, is_async: bool) -> String {
        if is_async { format!(" -> {}", self.rty(&Type::Async(Box::new(t.clone())))) } else { self.ret_suffix(t) }
    }

    fn ret_suffix(&self, t: &Type) -> String {
        if *t == Type::Void { String::new() } else { format!(" -> {}", self.rty(t)) }
    }

    fn params_decl(&self, sig: &Signature) -> String {
        sig.params.iter().map(|(n, t)| format!("{}: {}", rid(n), self.rty(t))).collect::<Vec<_>>().join(", ")
    }

    fn params_names(&self, sig: &Signature) -> String {
        sig.params.iter().map(|(n, _)| rid(n)).collect::<Vec<_>>().join(", ")
    }

    fn comma_params(&self, sig: &Signature) -> String {
        if sig.params.is_empty() { String::new() } else { format!(", {}", self.params_decl(sig)) }
    }

    /// `code` (of type `from`) as type `to`: optional wrapping, upcasts, element-wise
    /// conversions. Type checking has made sure `from` can be used as `to`.
    fn convert(&self, code: String, from: &Type, to: &Type) -> String {
        if from == to {
            return code;
        }
        match (from, to) {
            (Type::NoneLit, Type::Optional(_)) => "None".into(),
            (Type::Result(a, _), Type::Result(b, _)) | (Type::Optional(a), Type::Optional(b)) => {
                format!("({}).map(|__v| {})", code, self.convert("__v".into(), a, b))
            }
            (f, Type::Optional(inner)) => format!("Some({})", self.convert(code, f, inner)),
            (Type::Class(_) | Type::Interface(_), Type::Class(_) | Type::Interface(_)) => {
                format!("({} as {})", code, self.rty(to))
            }
            (Type::Map(_, a), Type::Map(_, b)) => format!(
                "({}).into_iter().map(|(__k, __v)| (__k, {})).collect::<BTreeMap<_, _>>()",
                code,
                self.convert("__v".into(), a, b)
            ),
            (Type::Array(a), Type::Array(b)) => {
                format!("({}).into_iter().map(|__v| {}).collect::<Vec<_>>()", code, self.convert("__v".into(), a, b))
            }
            _ => code,
        }
    }
}

/// The start of every generated program.
const HEADER: &str = r#"// Generated by the Kalinga compiler. Do not edit.
#![allow(warnings)]
#![allow(arithmetic_overflow, unconditional_panic)]
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

"#;

/// The runtime support code pasted into every generated program, after `HEADER`. It lives
/// in `runtime/*.rs` as ordinary Rust, so it is compiled and linted on its own (see the
/// `runtime` test module below).
const RUNTIME: &str = concat!(
    include_str!("../runtime/show.rs"),
    include_str!("../runtime/collections.rs"),
    include_str!("../runtime/random.rs"),
    include_str!("../runtime/threads.rs"),
    include_str!("../runtime/async_rt.rs"),
    include_str!("../runtime/channels.rs"),
    include_str!("../runtime/time.rs"),
    include_str!("../runtime/io.rs"),
    include_str!("../runtime/main.rs"),
);

/// Compiles the runtime as one module, so rustc and clippy check it like any other code.
#[cfg(test)]
#[allow(dead_code)]
mod runtime {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    include!("../runtime/show.rs");
    include!("../runtime/collections.rs");
    include!("../runtime/random.rs");
    include!("../runtime/threads.rs");
    include!("../runtime/async_rt.rs");
    include!("../runtime/channels.rs");
    include!("../runtime/time.rs");
    include!("../runtime/io.rs");
    include!("../runtime/main.rs");
}
