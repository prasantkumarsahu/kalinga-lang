//! Type checking of function, method, operator and `init` bodies (and default values),
//! producing the typed tree (`crate::tast`).
//!
//! Each body is checked on its own: an error in one does not stop the others from being
//! checked, so a program's independent mistakes are all reported at once.

mod branch;
mod call;
mod expr;
mod lambda;
mod library;
mod method;
mod note;
mod place;
mod stmt;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Diagnostic, Result, Span, err};

use crate::index::Index;
use crate::model::Model;
use crate::tast::*;
use crate::types::{Signature, Type};

/// Checks every body in `prog`, which `m` describes.
pub fn check(prog: &Program, m: &Model) -> std::result::Result<TypedProgram, Vec<Diagnostic>> {
    analyze(prog, m).0
}

/// Like [`check`], and also returns what every name refers to, for editors.
pub fn analyze(prog: &Program, m: &Model) -> (std::result::Result<TypedProgram, Vec<Diagnostic>>, Index) {
    let c = Checker {
        m,
        self_values: RefCell::new(HashSet::new()),
        cast_targets: RefCell::new(HashSet::new()),
        index: RefCell::new(Index::default()),
    };
    let mut out = TypedProgram::default();
    let mut errors = vec![];
    for cname in &m.class_order {
        c.class_bodies(cname, &mut out, &mut errors);
    }
    for sname in &m.struct_order {
        c.struct_bodies(sname, &mut out, &mut errors);
    }
    c.static_values(prog, &mut out, &mut errors);
    for t in &prog.tests {
        let mut cx = Ctx::new(None, Type::Void, false);
        let sig = Signature { params: vec![], ret: Type::Void };
        match c.body(&mut cx, sig, BodyDecl::of_fn(t)) {
            Ok(body) => out.tests.push(TTest { name: t.name.clone(), body, pos: t.pos }),
            Err(e) => errors.push(e),
        }
    }
    for f in &prog.functions {
        let sig = m.functions[&f.name].clone();
        let mut cx = Ctx::new(None, sig.ret.clone(), false);
        cx.tparams = f.tparams.clone();
        cx.is_async = f.is_async;
        match c.body(&mut cx, sig, BodyDecl::of_fn(f)) {
            Ok(b) => {
                out.functions.insert(f.name.clone(), b);
            }
            Err(e) => errors.push(e),
        }
    }
    out.self_values = c.self_values.into_inner();
    out.cast_targets = c.cast_targets.into_inner();
    let index = c.index.into_inner();
    (if errors.is_empty() { Ok(out) } else { Err(errors) }, index)
}

/// What checking a body needs from its declaration.
struct BodyDecl<'a> {
    params: &'a [Param],
    stmts: &'a [Stmt],
    pos: Span,
    /// For messages: the function's name, `init` or `op +`.
    name: String,
    is_async: bool,
}

impl<'a> BodyDecl<'a> {
    fn of_fn(f: &'a FnDecl) -> Self {
        BodyDecl { params: &f.params, stmts: &f.body, pos: f.pos, name: f.name.clone(), is_async: f.is_async }
    }
}

#[derive(Clone)]
struct Var {
    ty: Type,
    mutable: bool,
    /// Seen from inside a lambda that captured it.
    captured: bool,
    /// Where it is declared.
    decl: Span,
}

/// Variables from outside a lambda that its body uses.
struct LambdaFrame {
    /// Scopes at or above this index belong to the lambda.
    boundary: usize,
    captures: RefCell<Vec<(String, Type)>>,
}

/// Where in the program a body is being checked.
struct Ctx {
    scopes: Vec<HashMap<String, Var>>,
    /// Class whose method / init body this is.
    class: Option<String>,
    ret: Type,
    in_init: bool,
    /// Struct whose method or operator body this is.
    strukt: Option<String>,
    /// Inside a struct's `mut fn`: `self` may be changed.
    self_mut: bool,
    /// Inside a `static fn`: no `self`.
    is_static: bool,
    /// Inside an `async fn`: `await` is allowed.
    is_async: bool,
    loops: usize,
    /// Lambdas being checked, innermost last.
    lambdas: Vec<LambdaFrame>,
    /// Type parameters in scope (generic function or type).
    tparams: Vec<String>,
}

impl Ctx {
    fn new(class: Option<String>, ret: Type, in_init: bool) -> Ctx {
        Ctx {
            scopes: vec![HashMap::new()],
            class,
            ret,
            in_init,
            strukt: None,
            self_mut: false,
            is_static: false,
            is_async: false,
            loops: 0,
            lambdas: vec![],
            tparams: vec![],
        }
    }

    /// Finds a variable. Variables from outside the current lambda are recorded as captures
    /// and are read-only inside it.
    fn lookup(&self, name: &str) -> Option<Var> {
        let idx = (0..self.scopes.len()).rev().find(|i| self.scopes[*i].contains_key(name))?;
        let mut var = self.scopes[idx][name].clone();
        for frame in &self.lambdas {
            if idx < frame.boundary {
                var.mutable = false;
                var.captured = true;
                let mut caps = frame.captures.borrow_mut();
                if !caps.iter().any(|(n, _)| n == name) {
                    caps.push((name.to_string(), var.ty.clone()));
                }
            }
        }
        Some(var)
    }

    fn declare(&mut self, name: &str, ty: Type, mutable: bool, pos: Span) -> Result<()> {
        let scope = self.scopes.last_mut().expect("a body always has a scope");
        if scope.contains_key(name) {
            return err(format!("`{}` is already declared in this scope", name), pos);
        }
        scope.insert(name.to_string(), Var { ty, mutable, captured: false, decl: pos });
        Ok(())
    }

    /// Finds a variable without recording it as a lambda capture.
    fn peek(&self, name: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// Every variable in scope (inner ones hide outer ones of the same name), sorted.
    fn visible(&self) -> Vec<(String, Type)> {
        let mut out: Vec<(String, Type)> = vec![];
        for scope in self.scopes.iter().rev() {
            for (n, v) in scope {
                if !out.iter().any(|(o, _)| o == n) {
                    out.push((n.clone(), v.ty.clone()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Runs `f` in a new innermost scope.
    fn scoped<T>(&mut self, f: impl FnOnce(&mut Ctx) -> Result<T>) -> Result<T> {
        self.scopes.push(HashMap::new());
        let r = f(self);
        self.scopes.pop();
        r
    }
}

struct Checker<'a> {
    m: &'a Model,
    /// Classes whose methods use `self` as a value.
    self_values: RefCell<HashSet<String>>,
    /// Classes used as the target of `as` / `is`.
    cast_targets: RefCell<HashSet<String>>,
    /// What names refer to, for editors.
    index: RefCell<Index>,
}

impl Checker<'_> {
    fn class_bodies(&self, cname: &str, out: &mut TypedProgram, errors: &mut Vec<Diagnostic>) {
        let info = &self.m.classes[cname];
        for f in &info.decl.fields {
            let (Some(e), Some((_, ty))) = (&f.default, info.fields.iter().find(|(n, _)| n == &f.name)) else {
                continue;
            };
            let mut cx = Ctx::new(None, Type::Void, false);
            cx.tparams = info.tparams.clone();
            match self.expr_to(&mut cx, e, ty) {
                Ok(v) => {
                    out.field_defaults.insert((cname.to_string(), f.name.clone()), v);
                }
                Err(e) => errors.push(e),
            }
        }
        for meth in &info.decl.methods {
            let key = (cname.to_string(), meth.name.clone());
            if meth.is_abstract {
                out.methods.insert(key, None);
                continue;
            }
            let sig = if meth.is_static { info.statics[&meth.name].clone() } else { info.methods[&meth.name].clone() };
            let mut cx = Ctx::new(Some(cname.to_string()), sig.ret.clone(), false);
            cx.tparams = info.tparams.clone();
            cx.is_static = meth.is_static;
            cx.is_async = meth.is_async;
            match self.body(&mut cx, sig, BodyDecl::of_fn(meth)) {
                Ok(b) if meth.is_static => {
                    out.statics.insert(key, b);
                }
                Ok(b) => {
                    out.methods.insert(key, Some(b));
                }
                Err(e) => errors.push(e),
            }
        }
        if let (Some(init), Some(sig)) = (&info.decl.init, &info.init) {
            let mut cx = Ctx::new(Some(cname.to_string()), Type::Void, true);
            cx.tparams = info.tparams.clone();
            let decl = BodyDecl {
                params: &init.params,
                stmts: &init.body,
                pos: init.pos,
                name: "init".into(),
                is_async: false,
            };
            match self.body(&mut cx, sig.clone(), decl) {
                Ok(b) => {
                    out.inits.insert(cname.to_string(), b);
                }
                Err(e) => errors.push(e),
            }
        }
    }

    /// Initial values of `const`s and `static var`s, checked like the body of a `static fn`
    /// of their type.
    fn static_values(&self, prog: &Program, out: &mut TypedProgram, errors: &mut Vec<Diagnostic>) {
        let owned = prog
            .classes
            .iter()
            .map(|c| (c.name.as_str(), &c.statics))
            .chain(prog.structs.iter().map(|s| (s.name.as_str(), &s.statics)))
            .chain(std::iter::once(("", &prog.consts)));
        for (owner, decls) in owned {
            for d in decls {
                let ty = match self.m.static_value(owner, &d.name) {
                    Some(s) => s.ty.clone(),
                    None => self.m.consts[&d.name].clone(),
                };
                let mut cx = Ctx::new(self.m.classes.get(owner).map(|_| owner.to_string()), Type::Void, false);
                cx.strukt = self.m.structs.get(owner).map(|_| owner.to_string());
                cx.is_static = true;
                match self.expr_to(&mut cx, &d.value, &ty) {
                    Ok(v) => {
                        out.static_values.insert((owner.to_string(), d.name.clone()), v);
                    }
                    Err(e) => errors.push(e),
                }
            }
        }
    }

    fn struct_bodies(&self, name: &str, out: &mut TypedProgram, errors: &mut Vec<Diagnostic>) {
        let info = &self.m.structs[name];
        let struct_cx = |ret: Type| {
            let mut cx = Ctx::new(None, ret, false);
            cx.tparams = info.tparams.clone();
            cx.strukt = Some(name.to_string());
            cx
        };
        for meth in &info.decl.methods {
            let m = &info.methods[&meth.name];
            let mut cx = struct_cx(m.sig.ret.clone());
            cx.self_mut = m.is_mut;
            cx.is_static = m.is_static;
            cx.is_async = m.is_async;
            match self.body(&mut cx, m.sig.clone(), BodyDecl::of_fn(meth)) {
                Ok(b) => {
                    out.struct_methods.insert((name.to_string(), meth.name.clone()), b);
                }
                Err(e) => errors.push(e),
            }
        }
        for (o, decl) in info.ops.iter().zip(&info.decl.ops) {
            let sig = Signature {
                params: decl.params.iter().map(|p| p.name.clone()).zip(o.param.iter().cloned()).collect(),
                ret: o.ret.clone(),
            };
            let mut cx = struct_cx(o.ret.clone());
            let body = BodyDecl {
                params: &decl.params,
                stmts: &decl.body,
                pos: decl.pos,
                name: format!("op {}", o.op.rust()),
                is_async: false,
            };
            match self.body(&mut cx, sig, body) {
                Ok(b) => {
                    out.struct_ops.insert((name.to_string(), o.index), b);
                }
                Err(e) => errors.push(e),
            }
        }
    }

    /// Checks a function, method, operator or init body.
    fn body(&self, cx: &mut Ctx, sig: Signature, decl: BodyDecl) -> Result<TBody> {
        for ((p, t), param) in sig.params.iter().zip(decl.params) {
            self.declare(cx, p, t.clone(), false, param.pos)?;
        }
        if sig.ret != Type::Void && !always_returns(decl.stmts) {
            return err(format!("`{}` must return a `{}` on every path", decl.name, sig.ret), decl.pos);
        }
        let stmts = self.stmts(cx, decl.stmts)?;
        Ok(TBody { sig, stmts, is_async: decl.is_async })
    }

    /// Type of `self` inside a class (generic classes use their own parameters).
    fn class_type(&self, class: &str) -> Type {
        let tps = &self.m.classes[class].tparams;
        if tps.is_empty() {
            Type::Class(class.to_string())
        } else {
            Type::Generic(class.to_string(), tps.iter().map(|t| Type::Param(t.clone())).collect())
        }
    }

    /// Type of `self` inside struct `st` (generic structs use their own parameters).
    fn struct_self_type(&self, st: &str) -> Type {
        Type::Struct(st.to_string(), self.m.structs[st].tparams.iter().map(|t| Type::Param(t.clone())).collect())
    }

    /// The class whose method this is, if `self` may be used here.
    fn self_class(&self, cx: &Ctx, pos: Span) -> Result<String> {
        if cx.is_static {
            return err("`self` is not available in a `static fn`", pos);
        }
        if !cx.lambdas.is_empty() && cx.class.is_some() {
            return err("`self` cannot be used inside a lambda; copy it first (`let me = self`) and use `me`", pos);
        }
        cx.class.clone().ok_or_else(|| Diagnostic::error("`self` is only available inside class methods", pos))
    }

    /// `e` converted to type `to`, or an error when it cannot be used as one.
    fn convert(&self, e: TExpr, to: &Type) -> Result<TExpr> {
        if &e.ty == to {
            return Ok(e);
        }
        if !self.m.assignable(&e.ty, to) {
            return err(format!("expected `{}`, found `{}`", to, e.ty), e.pos);
        }
        let pos = e.pos;
        Ok(TExpr { kind: TExprKind::Convert(Box::new(e)), ty: to.clone(), pos })
    }

    /// `e` checked with `to` as the expected type, then converted to it.
    fn expr_to(&self, cx: &mut Ctx, e: &Expr, to: &Type) -> Result<TExpr> {
        let t = self.expr(cx, e, Some(to))?;
        self.convert(t, to)
    }
}

fn texpr(kind: TExprKind, ty: Type, pos: Span) -> TExpr {
    TExpr { kind, ty, pos }
}

/// Conservative check that a body returns on every path.
fn always_returns(stmts: &[Stmt]) -> bool {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Return(_)) => true,
        Some(StmtKind::If { then, otherwise: Some(o), .. })
        | Some(StmtKind::IfLet { then, otherwise: Some(o), .. }) => always_returns(then) && always_returns(o),
        Some(StmtKind::While { cond: Expr { kind: ExprKind::Bool(true), .. }, body }) => !has_break(body),
        // Non-exhaustive matches are rejected when the match is checked.
        Some(StmtKind::Match { arms, .. }) => arms.iter().all(|a| always_returns(&a.body)),
        _ => false,
    }
}

/// Does a loop body contain a `break` for this loop (not nested loops)?
fn has_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::If { then, otherwise, .. } | StmtKind::IfLet { then, otherwise, .. } => {
            has_break(then) || otherwise.as_deref().is_some_and(has_break)
        }
        StmtKind::Match { arms, .. } => arms.iter().any(|a| has_break(&a.body)),
        _ => false,
    })
}
