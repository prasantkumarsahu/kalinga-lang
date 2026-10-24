//! Walking a whole program: every statement, expression and written type.

use crate::ast::*;

/// What a walk calls; each method has an empty default.
pub trait Visitor {
    /// Every expression, outer ones before the ones inside them.
    fn expr(&mut self, _e: &Expr) {}
    /// Every type written in the source, outer ones before their type arguments.
    fn ty(&mut self, _t: &TypeExpr) {}
    /// Every statement, before the expressions and statements inside it.
    fn stmt(&mut self, _s: &Stmt) {}
}

pub fn program(prog: &Program, v: &mut impl Visitor) {
    for f in &prog.functions {
        function(f, v);
    }
    for c in &prog.classes {
        for f in &c.fields {
            field(f, v);
        }
        if let Some(init) = &c.init {
            params(&init.params, v);
            block(&init.body, v);
        }
        for m in &c.methods {
            function(m, v);
        }
        statics(&c.statics, v);
    }
    for s in &prog.structs {
        for f in &s.fields {
            field(f, v);
        }
        for m in &s.methods {
            function(m, v);
        }
        for o in &s.ops {
            params(&o.params, v);
            ty(&o.ret, v);
            block(&o.body, v);
        }
        statics(&s.statics, v);
    }
    for i in &prog.interfaces {
        for m in &i.methods {
            params(&m.params, v);
            ty(&m.ret, v);
        }
    }
    for e in &prog.enums {
        for variant in &e.variants {
            params(&variant.fields, v);
        }
    }
    for x in &prog.extern_fns {
        params(&x.params, v);
        ty(&x.ret, v);
    }
    statics(&prog.consts, v);
}

fn function(f: &FnDecl, v: &mut impl Visitor) {
    params(&f.params, v);
    ty(&f.ret, v);
    block(&f.body, v);
}

fn field(f: &FieldDecl, v: &mut impl Visitor) {
    ty(&f.ty, v);
    if let Some(d) = &f.default {
        expr(d, v);
    }
}

fn statics(decls: &[StaticDecl], v: &mut impl Visitor) {
    for d in decls {
        ty(&d.ty, v);
        expr(&d.value, v);
    }
}

fn params(ps: &[Param], v: &mut impl Visitor) {
    for p in ps {
        ty(&p.ty, v);
    }
}

pub fn ty(t: &TypeExpr, v: &mut impl Visitor) {
    v.ty(t);
    match t {
        TypeExpr::Named(_, args, _) => args.iter().for_each(|a| ty(a, v)),
        TypeExpr::Array(inner) | TypeExpr::Optional(inner) => ty(inner, v),
        TypeExpr::Map(k, val) => {
            ty(k, v);
            ty(val, v);
        }
        TypeExpr::Func(ps, r) => {
            ps.iter().for_each(|p| ty(p, v));
            ty(r, v);
        }
        TypeExpr::Tuple(parts) => parts.iter().for_each(|p| ty(p, v)),
        TypeExpr::Int | TypeExpr::Float | TypeExpr::Bool | TypeExpr::Str | TypeExpr::Void => {}
    }
}

pub fn block(stmts: &[Stmt], v: &mut impl Visitor) {
    for s in stmts {
        stmt(s, v);
    }
}

pub fn stmt(s: &Stmt, v: &mut impl Visitor) {
    v.stmt(s);
    match &s.kind {
        StmtKind::Let { ty: t, value, .. } => {
            if let Some(t) = t {
                ty(t, v);
            }
            expr(value, v);
        }
        StmtKind::LetTuple { value, .. } => expr(value, v),
        StmtKind::Assign { target, value, .. } => {
            expr(target, v);
            expr(value, v);
        }
        StmtKind::If { cond, then, otherwise } => {
            expr(cond, v);
            block(then, v);
            block(otherwise.as_deref().unwrap_or_default(), v);
        }
        StmtKind::IfLet { value, then, otherwise, .. } => {
            expr(value, v);
            block(then, v);
            block(otherwise.as_deref().unwrap_or_default(), v);
        }
        StmtKind::While { cond: value, body } | StmtKind::WhileLet { value, body, .. } => {
            expr(value, v);
            block(body, v);
        }
        StmtKind::Match { value, arms } => {
            expr(value, v);
            arms.iter().for_each(|a| block(&a.body, v));
        }
        StmtKind::For { iter, body, .. } => {
            match iter {
                ForIter::Range(a, b, _) => {
                    expr(a, v);
                    expr(b, v);
                }
                ForIter::Each(e) => expr(e, v),
            }
            block(body, v);
        }
        StmtKind::Return(Some(e)) | StmtKind::Expr(e) => expr(e, v),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
    }
}

pub fn expr(e: &Expr, v: &mut impl Visitor) {
    v.expr(e);
    match &e.kind {
        ExprKind::Call(_, targs, _) => targs.iter().for_each(|t| ty(t, v)),
        ExprKind::Lambda { params, ret, body } => {
            params.iter().filter_map(|(_, t, _)| t.as_ref()).for_each(|t| ty(t, v));
            if let Some(r) = ret {
                ty(r, v);
            }
            match body {
                LambdaBody::Expr(x) => expr(x, v),
                LambdaBody::Block(stmts) => block(stmts, v),
            }
        }
        // Branch values are visited with their statements.
        ExprKind::If { cond, then, otherwise, .. } => {
            expr(cond, v);
            block(then, v);
            block(otherwise, v);
            return;
        }
        ExprKind::Match { value, arms } => {
            expr(value, v);
            arms.iter().for_each(|a| block(&a.body, v));
            return;
        }
        _ => {}
    }
    for c in e.children() {
        expr(c, v);
    }
}
