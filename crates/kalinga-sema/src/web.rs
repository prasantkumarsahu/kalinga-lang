//! What a web build (JavaScript in a browser) cannot do: threads, channels, shared values,
//! reading the console, and waiting for async code outside `main`.

use kalinga_syntax::error::{Diagnostic, Span};

use crate::Type;
use crate::tast::*;

/// Errors for the uses of features a web build does not have, in every body of the program,
/// each with the name it is about (errors point at where an expression starts; editors and
/// the compiler move them onto that name).
pub fn web_errors(typed: &TypedProgram) -> Vec<(Diagnostic, String)> {
    let mut w = Walker { errors: vec![], main: false };
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
        w.stmts(&b.stmts);
    }
    w.main = false;
    for e in typed.field_defaults.values().chain(typed.static_values.values()) {
        w.expr(e);
    }
    w.errors.sort_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    w.errors.dedup_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    w.errors
}

struct Walker {
    errors: Vec<(Diagnostic, String)>,
    /// Directly in the body of `main` (not inside a lambda), where `Async.run` may wait.
    main: bool,
}

impl Walker {
    fn not_web(&mut self, what: &str, name: &str, pos: Span) {
        self.errors.push((Diagnostic::error(format!("{} is not available in a web build", what), pos), name.into()));
    }

    fn stmts(&mut self, stmts: &[TStmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &TStmt) {
        match s {
            TStmt::LetTuple { value, .. } | TStmt::Let { value, .. } => self.expr(value),
            TStmt::Assign { place, value, .. } | TStmt::StructAssign { place, value, .. } => {
                self.place(place);
                self.expr(value);
            }
            TStmt::SetField { recv, value, .. } => {
                self.expr(recv);
                self.expr(value);
            }
            TStmt::MapInsert { map, key, value } => {
                self.place(map);
                self.expr(key);
                self.expr(value);
            }
            TStmt::If { cond: value, then, otherwise } | TStmt::IfLet { value, then, otherwise, .. } => {
                self.expr(value);
                self.stmts(then);
                if let Some(o) = otherwise {
                    self.stmts(o);
                }
            }
            TStmt::Match { value, arms } => {
                self.expr(value);
                for a in arms {
                    self.stmts(&a.body);
                }
            }
            TStmt::While { cond: value, body } | TStmt::WhileLet { value, body, .. } => {
                self.expr(value);
                self.stmts(body);
            }
            TStmt::ForRange { from, to, body, .. } => {
                self.expr(from);
                self.expr(to);
                self.stmts(body);
            }
            TStmt::ForEach { iter, body, .. } => {
                self.expr(iter);
                self.stmts(body);
            }
            TStmt::Return(v) => {
                if let Some(v) = v {
                    self.expr(v);
                }
            }
            TStmt::Expr(e) => self.expr(e),
            TStmt::SuperInit { args, .. } => self.exprs(args),
            TStmt::Break | TStmt::Continue => {}
        }
    }

    fn place(&mut self, p: &TPlace) {
        match p {
            TPlace::StructField { base, .. } => self.place(base),
            TPlace::ObjectField { recv, .. } => self.expr(recv),
            TPlace::Index { base, index, .. } => {
                self.place(base);
                self.expr(index);
            }
            TPlace::Local { .. } | TPlace::SharedLocal { .. } | TPlace::SelfField { .. } | TPlace::Static { .. } => {}
        }
    }

    fn exprs(&mut self, es: &[TExpr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn value_block(&mut self, b: &TValueBlock) {
        self.stmts(&b.stmts);
        self.expr(&b.value);
    }

    fn expr(&mut self, e: &TExpr) {
        match &e.kind {
            TExprKind::Int(_)
            | TExprKind::Float(_)
            | TExprKind::Str(_)
            | TExprKind::Bool(_)
            | TExprKind::None
            | TExprKind::Local(_)
            | TExprKind::SharedLocal(_)
            | TExprKind::FnRef(_)
            | TExprKind::StructSelf
            | TExprKind::ObjectSelf
            | TExprKind::SelfField { .. }
            | TExprKind::Static { .. }
            | TExprKind::MathConst(_) => {}
            TExprKind::Convert(v)
            | TExprKind::Neg(v)
            | TExprKind::Not(v)
            | TExprKind::Try(v)
            | TExprKind::TupleField { recv: v, .. }
            | TExprKind::Field { recv: v, .. }
            | TExprKind::IsNone { value: v, .. }
            | TExprKind::Downcast { value: v, .. } => self.expr(v),
            TExprKind::Await(v) => {
                if matches!(v.ty, Type::Task(_)) {
                    self.not_web("waiting for a thread's `Task`", "await", e.pos);
                }
                self.expr(v);
            }
            TExprKind::Array(items) | TExprKind::Tuple(items) | TExprKind::Variant { args: items, .. } => {
                self.exprs(items)
            }
            TExprKind::Map(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            TExprKind::Interp(parts) => {
                for p in parts {
                    if let TInterp::Expr(x) = p {
                        self.expr(x);
                    }
                }
            }
            TExprKind::Binary(_, l, r) => {
                self.expr(l);
                self.expr(r);
            }
            TExprKind::StructOp { lhs, rhs, .. } => {
                self.expr(lhs);
                if let Some(r) = rhs {
                    self.expr(r);
                }
            }
            TExprKind::Lambda(l) => self.lambda(l),
            TExprKind::If { cond, then, otherwise, .. } => {
                self.expr(cond);
                self.value_block(then);
                self.value_block(otherwise);
            }
            TExprKind::Match { value, arms } => {
                self.expr(value);
                for (_, b) in arms {
                    self.value_block(b);
                }
            }
            TExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            TExprKind::Slice { base, from, to } => {
                self.expr(base);
                for b in [from, to].into_iter().flatten() {
                    self.expr(b);
                }
            }
            TExprKind::Call { callee, args } => {
                if let Callee::Value(r) | Callee::Method { recv: r, .. } | Callee::StructMethod { recv: r, .. } = callee
                {
                    self.expr(r);
                }
                self.exprs(args);
            }
            TExprKind::Builtin { op, args } => {
                self.builtin_use(op, e);
                self.exprs(args);
            }
            TExprKind::BuiltinMethod { op, recv, args } => {
                self.method_use(op, e);
                self.expr(recv);
                self.exprs(args);
            }
            TExprKind::Mutate { place, args, .. } => {
                self.place(place);
                self.exprs(args);
            }
        }
    }

    /// A lambda's body, which is not `main`'s own (it cannot wait with `Async.run`).
    fn lambda(&mut self, l: &TLambda) {
        let main = std::mem::replace(&mut self.main, false);
        match &l.body {
            TLambdaBody::Expr(x) => self.expr(x),
            TLambdaBody::Block(stmts) => self.stmts(stmts),
        }
        self.main = main;
    }

    /// A built-in function a web build does not have, or `Async.run` outside `main`.
    fn builtin_use(&mut self, op: &Builtin, e: &TExpr) {
        let what = match op {
            Builtin::Input => Some(("`input()` (reading the console)", "input")),
            Builtin::Spawn => Some(("`Thread.spawn`", "spawn")),
            Builtin::Cores => Some(("`Thread.cores`", "cores")),
            Builtin::NewChannel => Some(("`Channel`", "Channel")),
            Builtin::NewShared => Some(("`Shared`", "Shared")),
            _ => None,
        };
        if let Some((what, name)) = what {
            self.not_web(what, name, e.pos);
        }
        if *op == Builtin::AsyncRun && !self.main {
            self.errors.push((
                Diagnostic::error(
                    "a web build can wait with `Async.run` only in `main`; make this function `async` and use `await`",
                    e.pos,
                ),
                "run".into(),
            ));
        }
    }

    /// A method of threads, channels and shared values.
    fn method_use(&mut self, op: &BuiltinMethod, e: &TExpr) {
        use BuiltinMethod::*;
        if matches!(op, Join | IsDone | Send | Receive | TryReceive | Close | Get | Set | Update | ParMap) {
            let name = if *op == ParMap { "par_map" } else { method_name(op) };
            let what = match op {
                ParMap => "`par_map`".to_string(),
                _ => format!("`{}` on threads, channels and shared values", name),
            };
            self.not_web(&what, name, e.pos);
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
