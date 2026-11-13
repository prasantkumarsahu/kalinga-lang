//! Walking the typed tree: every expression and place inside statements, for passes that
//! look for something (features a target lacks, the code a program reaches).

use crate::tast::*;

/// What a walk calls. Every method has a default that does nothing.
pub trait Visitor {
    /// An expression, before the expressions inside it.
    fn expr(&mut self, _e: &TExpr) {}
    /// A place assigned to or changed, before the expressions inside it.
    fn place(&mut self, _p: &TPlace) {}
    /// A lambda's body starts (`true`) or ends (`false`).
    fn lambda(&mut self, _entering: bool) {}
}

pub fn walk_stmts(v: &mut impl Visitor, stmts: &[TStmt]) {
    for s in stmts {
        walk_stmt(v, s);
    }
}

pub fn walk_stmt(v: &mut impl Visitor, s: &TStmt) {
    match s {
        TStmt::LetTuple { value, .. } | TStmt::Let { value, .. } => walk_expr(v, value),
        TStmt::Assign { place, value, .. } | TStmt::StructAssign { place, value, .. } => {
            walk_place(v, place);
            walk_expr(v, value);
        }
        TStmt::SetField { recv, value, .. } => {
            walk_expr(v, recv);
            walk_expr(v, value);
        }
        TStmt::MapInsert { map, key, value } => {
            walk_place(v, map);
            walk_expr(v, key);
            walk_expr(v, value);
        }
        TStmt::If { cond: value, then, otherwise } | TStmt::IfLet { value, then, otherwise, .. } => {
            walk_expr(v, value);
            walk_stmts(v, then);
            if let Some(o) = otherwise {
                walk_stmts(v, o);
            }
        }
        TStmt::Match { value, arms } => {
            walk_expr(v, value);
            for a in arms {
                walk_stmts(v, &a.body);
            }
        }
        TStmt::While { cond: value, body } | TStmt::WhileLet { value, body, .. } => {
            walk_expr(v, value);
            walk_stmts(v, body);
        }
        TStmt::ForRange { from, to, body, .. } => {
            walk_expr(v, from);
            walk_expr(v, to);
            walk_stmts(v, body);
        }
        TStmt::ForEach { iter, body, .. } => {
            walk_expr(v, iter);
            walk_stmts(v, body);
        }
        TStmt::Return(Some(e)) | TStmt::Expr(e) => walk_expr(v, e),
        TStmt::SuperInit { args, .. } => walk_exprs(v, args),
        TStmt::Return(None) | TStmt::Break | TStmt::Continue => {}
    }
}

pub fn walk_place(v: &mut impl Visitor, p: &TPlace) {
    v.place(p);
    match p {
        TPlace::StructField { base, .. } => walk_place(v, base),
        TPlace::ObjectField { recv, .. } => walk_expr(v, recv),
        TPlace::Index { base, index, .. } => {
            walk_place(v, base);
            walk_expr(v, index);
        }
        TPlace::Local { .. } | TPlace::SharedLocal { .. } | TPlace::SelfField { .. } | TPlace::Static { .. } => {}
    }
}

pub fn walk_exprs(v: &mut impl Visitor, es: &[TExpr]) {
    for e in es {
        walk_expr(v, e);
    }
}

fn walk_value_block(v: &mut impl Visitor, b: &TValueBlock) {
    walk_stmts(v, &b.stmts);
    walk_expr(v, &b.value);
}

pub fn walk_expr(v: &mut impl Visitor, e: &TExpr) {
    v.expr(e);
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
        TExprKind::Convert(x)
        | TExprKind::Neg(x)
        | TExprKind::Not(x)
        | TExprKind::Try(x)
        | TExprKind::Await(x)
        | TExprKind::TupleField { recv: x, .. }
        | TExprKind::Field { recv: x, .. }
        | TExprKind::IsNone { value: x, .. }
        | TExprKind::Downcast { value: x, .. } => walk_expr(v, x),
        TExprKind::Array(items) | TExprKind::Tuple(items) | TExprKind::Variant { args: items, .. } => {
            walk_exprs(v, items)
        }
        TExprKind::Map(entries) => {
            for (k, x) in entries {
                walk_expr(v, k);
                walk_expr(v, x);
            }
        }
        TExprKind::Interp(parts) => {
            for p in parts {
                if let TInterp::Expr(x) = p {
                    walk_expr(v, x);
                }
            }
        }
        TExprKind::Binary(_, l, r) => {
            walk_expr(v, l);
            walk_expr(v, r);
        }
        TExprKind::StructOp { lhs, rhs, .. } => {
            walk_expr(v, lhs);
            if let Some(r) = rhs {
                walk_expr(v, r);
            }
        }
        TExprKind::Lambda(l) => {
            v.lambda(true);
            match &l.body {
                TLambdaBody::Expr(x) => walk_expr(v, x),
                TLambdaBody::Block(stmts) => walk_stmts(v, stmts),
            }
            v.lambda(false);
        }
        TExprKind::If { cond, then, otherwise, .. } => {
            walk_expr(v, cond);
            walk_value_block(v, then);
            walk_value_block(v, otherwise);
        }
        TExprKind::Match { value, arms } => {
            walk_expr(v, value);
            for (_, b) in arms {
                walk_value_block(v, b);
            }
        }
        TExprKind::Index { base, index } => {
            walk_expr(v, base);
            walk_expr(v, index);
        }
        TExprKind::Slice { base, from, to } => {
            walk_expr(v, base);
            for b in [from, to].into_iter().flatten() {
                walk_expr(v, b);
            }
        }
        TExprKind::Call { callee, args } => {
            if let Callee::Value(r) | Callee::Method { recv: r, .. } | Callee::StructMethod { recv: r, .. } = callee {
                walk_expr(v, r);
            }
            walk_exprs(v, args);
        }
        TExprKind::Builtin { args, .. } => walk_exprs(v, args),
        TExprKind::BuiltinMethod { recv, args, .. } => {
            walk_expr(v, recv);
            walk_exprs(v, args);
        }
        TExprKind::Mutate { place, args, .. } => {
            walk_place(v, place);
            walk_exprs(v, args);
        }
    }
}
