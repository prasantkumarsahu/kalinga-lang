//! What a web build (JavaScript in a browser) cannot do: threads, channels, shared values,
//! reading the console, and waiting for async code outside `main`.

use kalinga_syntax::error::{Diagnostic, Span};

use crate::Type;
use crate::tast::*;
use crate::visit::{Visitor, walk_expr, walk_stmts};

/// Errors for the uses of features a web build does not have, in every body of the program,
/// each with the name it is about (errors point at where an expression starts; editors and
/// the compiler move them onto that name).
pub fn web_errors(typed: &TypedProgram) -> Vec<(Diagnostic, String)> {
    let mut w = Finder { errors: vec![], main: false, lambdas: 0 };
    let mut bodies: Vec<(&str, &TBody)> = vec![];
    bodies.extend(typed.functions.iter().map(|(n, b)| (n.as_str(), b)));
    bodies.extend(typed.methods.values().flatten().map(|b| ("", b)));
    bodies.extend(typed.statics.values().map(|b| ("", b)));
    bodies.extend(typed.inits.values().map(|b| ("", b)));
    bodies.extend(typed.struct_methods.values().map(|b| ("", b)));
    bodies.extend(typed.iface_methods.values().map(|b| ("", b)));
    bodies.extend(typed.struct_ops.values().map(|b| ("", b)));
    bodies.extend(typed.tests.iter().map(|t| ("", &t.body)));
    for (name, b) in bodies {
        w.main = name == "main";
        walk_stmts(&mut w, &b.stmts);
    }
    w.main = false;
    for e in typed.field_defaults.values().chain(typed.static_values.values()) {
        walk_expr(&mut w, e);
    }
    w.errors.sort_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    w.errors.dedup_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    w.errors
}

struct Finder {
    errors: Vec<(Diagnostic, String)>,
    /// In the body of `main`, where `Async.run` may wait...
    main: bool,
    /// ...but not inside a lambda there.
    lambdas: usize,
}

impl Visitor for Finder {
    fn expr(&mut self, e: &TExpr) {
        match &e.kind {
            TExprKind::Await(v) if matches!(v.ty, Type::Task(_)) => {
                self.not_web("waiting for a thread's `Task`", "await", e.pos)
            }
            TExprKind::Builtin { op, .. } => self.builtin_use(op, e.pos),
            TExprKind::BuiltinMethod { op, .. } => self.method_use(op, e.pos),
            _ => {}
        }
    }

    fn lambda(&mut self, entering: bool) {
        if entering {
            self.lambdas += 1;
        } else {
            self.lambdas -= 1;
        }
    }
}

impl Finder {
    fn not_web(&mut self, what: &str, name: &str, pos: Span) {
        self.errors.push((Diagnostic::error(format!("{} is not available in a web build", what), pos), name.into()));
    }

    /// A built-in function a web build does not have, or `Async.run` outside `main`.
    fn builtin_use(&mut self, op: &Builtin, pos: Span) {
        let what = match op {
            Builtin::Input => Some(("`input()` (reading the console)", "input")),
            Builtin::Spawn => Some(("`Thread.spawn`", "spawn")),
            Builtin::Cores => Some(("`Thread.cores`", "cores")),
            Builtin::NewChannel => Some(("`Channel`", "Channel")),
            Builtin::NewShared => Some(("`Shared`", "Shared")),
            _ => None,
        };
        if let Some((what, name)) = what {
            self.not_web(what, name, pos);
        }
        if *op == Builtin::AsyncRun && !(self.main && self.lambdas == 0) {
            self.errors.push((
                Diagnostic::error(
                    "a web build can wait with `Async.run` only in `main`; make this function `async` and use `await`",
                    pos,
                ),
                "run".into(),
            ));
        }
    }

    /// A method of threads, channels and shared values.
    fn method_use(&mut self, op: &BuiltinMethod, pos: Span) {
        use BuiltinMethod::*;
        if matches!(op, Join | IsDone | Send | Receive | TryReceive | Close | Get | Set | Update | ParMap) {
            let name = if *op == ParMap { "par_map" } else { method_name(op) };
            let what = match op {
                ParMap => "`par_map`".to_string(),
                _ => format!("`{}` on threads, channels and shared values", name),
            };
            self.not_web(&what, name, pos);
        }
    }
}

fn method_name(op: &BuiltinMethod) -> &'static str {
    match op {
        BuiltinMethod::Join => "join",
        BuiltinMethod::IsDone => "is_done",
        BuiltinMethod::Send => "send",
        BuiltinMethod::Receive => "receive",
        BuiltinMethod::TryReceive => "try_receive",
        BuiltinMethod::Close => "close",
        BuiltinMethod::Get => "get",
        BuiltinMethod::Set => "set",
        BuiltinMethod::Update => "update",
        _ => "this method",
    }
}
