//! Prints a type-checked program as JavaScript, for web builds.
//!
//! It reads the same typed tree as the Rust backend (`kalinga-codegen`), so every program
//! that type checks can be written out; features a browser does not have are rejected
//! before this runs. How Kalinga's values look in JavaScript:
//!
//! - `int` and `float` are numbers; int arithmetic goes through `$kl.add` and friends, which
//!   stop the program when a result leaves the range a number holds exactly (±2^53).
//! - Arrays, maps (`$kl.KlMap`, iterated in key order), tuples (arrays), structs (classes)
//!   and enums (`$kl.E`) are values: they are copied with `$kl.clone` wherever the Rust
//!   backend clones. Objects of classes are shared references.
//! - `none` is `null`; an optional value is the value itself.
//! - Printing is chosen from the static type (`$kl.sF` prints `3.0`, `$kl.sI` prints `3`).

// Library code reports problems instead of panicking.
#![warn(clippy::unwrap_used)]

mod expr;
mod items;
mod stmt;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use kalinga_sema::tast::TypedProgram;
use kalinga_sema::{Model, Type};
use kalinga_syntax::ast::Program;

/// What to generate.
pub struct Target<'a> {
    /// A program that runs the analysed file's `test fn`s instead of `main`.
    pub tests: bool,
    /// The source files' names (by `Span::file`), for the places messages point at.
    pub files: &'a [String],
}

/// The JavaScript source of a checked program.
pub fn generate(prog: &Program, model: &Model, typed: &TypedProgram, target: &Target) -> String {
    let g = Gen {
        m: model,
        t: typed,
        tests: target.tests,
        files: target.files,
        fresh: Cell::new(0),
        shared: RefCell::new(HashSet::new()),
        in_async: Cell::new(false),
        block_on: Cell::new(false),
        used_block_on: Cell::new(false),
    };
    g.program(prog)
}

struct Gen<'a> {
    m: &'a Model,
    t: &'a TypedProgram,
    tests: bool,
    files: &'a [String],
    /// Numbers temporaries (`$t1`, `$t2`, ...).
    fresh: Cell<usize>,
    /// Locals of the function being written that lambdas change (shared, not copied).
    shared: RefCell<HashSet<String>>,
    /// Writing the body of an `async fn` (or of a `main` that waits for async code).
    in_async: Cell<bool>,
    /// Writing `main`, where `Async.run` may wait for async code.
    block_on: Cell<bool>,
    /// `main` used `Async.run`, so it has to be an async function.
    used_block_on: Cell<bool>,
}

impl Gen<'_> {
    /// `file:line` of a position, for messages.
    fn place_of(&self, pos: kalinga_syntax::error::Span) -> String {
        let file = self.files.get(pos.file as usize).map_or("?", String::as_str);
        format!("{}:{}", file, pos.line)
    }

    /// A new temporary name.
    fn temp(&self) -> String {
        self.fresh.set(self.fresh.get() + 1);
        format!("$t{}", self.fresh.get())
    }

    /// A JavaScript function that prints a value of type `t` the Kalinga way.
    fn shower(&self, t: &Type) -> String {
        match t {
            Type::Int => "$kl.sI".into(),
            Type::Float => "$kl.sF".into(),
            Type::Bool => "$kl.sB".into(),
            Type::Str => "$kl.sS".into(),
            Type::Void => "$kl.sV".into(),
            Type::NoneLit => "(() => \"none\")".into(),
            Type::Array(i) => format!("$kl.sArr({})", self.shower(i)),
            Type::Map(k, v) => format!("$kl.sMap({}, {})", self.shower(k), self.shower(v)),
            Type::Optional(i) => format!("$kl.sOpt({})", self.shower(i)),
            Type::Tuple(parts) => {
                format!("$kl.sTup([{}])", parts.iter().map(|p| self.shower(p)).collect::<Vec<_>>().join(", "))
            }
            Type::Result(a, b) => format!("$kl.sRes({}, {})", self.shower(a), self.shower(b)),
            Type::Class(_) | Type::Interface(..) | Type::Generic(..) | Type::Struct(..) => "$kl.sObj".into(),
            Type::Enum(name, _) => format!("show${}", name),
            Type::Func(..) => "$kl.sFn".into(),
            _ => "$kl.sAny".into(),
        }
    }

    /// `show(code)` for a value of type `t`.
    fn show(&self, code: &str, t: &Type) -> String {
        match t {
            Type::Str => code.to_string(),
            Type::Class(_) | Type::Interface(..) | Type::Generic(..) | Type::Struct(..) => {
                format!("({}).__show()", code)
            }
            _ => format!("{}({})", self.shower(t), code),
        }
    }
}

/// Values of these types are copied when read: arrays, maps, structs, and what may hold them.
fn needs_clone(t: &Type) -> bool {
    match t {
        Type::Array(_) | Type::Map(..) | Type::Struct(..) | Type::Param(_) | Type::Tuple(_) | Type::Enum(..) => true,
        Type::Result(..) => true,
        Type::Optional(i) => needs_clone(i),
        _ => false,
    }
}

fn clone_if(code: String, ty: &Type) -> String {
    if needs_clone(ty) { format!("$kl.clone({})", code) } else { code }
}

/// Words a local may not be called in JavaScript (reserved words and globals the generated
/// code relies on).
const JS_RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "NaN",
    "Infinity",
    "Math",
    "Number",
    "String",
    "Object",
    "Array",
    "Promise",
    "console",
    "process",
    "globalThis",
    "window",
    "document",
    "Symbol",
    "Error",
    "JSON",
    "Map",
    "Set",
    "BigInt",
    "Date",
];

/// JavaScript name for a variable or parameter.
fn jid(name: &str) -> String {
    if name == "self" {
        return "this".into();
    }
    if JS_RESERVED.contains(&name) { format!("{}$", name) } else { name.to_string() }
}

/// JavaScript name for a top-level function.
fn fn_id(name: &str) -> String {
    format!("f${}", name)
}

/// JavaScript name for a class or struct.
fn type_id(name: &str) -> String {
    format!("T${}", name)
}

/// JavaScript name for a method (fields keep their own names; a field and a method may share
/// one).
fn method_id(name: &str) -> String {
    format!("${}", name)
}

fn indent(n: usize) -> String {
    "  ".repeat(n)
}

/// A JavaScript string literal.
fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The runtime every generated program starts with.
const RUNTIME: &str = include_str!("../runtime/runtime.js");
