//! Expressions other than calls: literals, names, `self`, collections, interpolation,
//! operators, fields, indexing, `await`, `?` and downcasts.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, texpr};
use crate::model::class_trait;
use crate::tast::*;
use crate::types::{Type, is_map_key, subst};

impl Checker<'_> {
    /// Checks `e`; `expected` is the type the context wants, which decides the type of
    /// `none`, `[]`, `{}`, lambdas, `Ok(...)`, generic calls and more.
    pub(super) fn expr(&self, cx: &mut Ctx, e: &Expr, expected: Option<&Type>) -> Result<TExpr> {
        let pos = e.pos;
        let lit = |kind, ty| Ok(texpr(kind, ty, pos));
        match &e.kind {
            ExprKind::Int(v) => lit(TExprKind::Int(*v), Type::Int),
            ExprKind::Float(v) => lit(TExprKind::Float(*v), Type::Float),
            ExprKind::Str(s) => lit(TExprKind::Str(s.clone()), Type::Str),
            ExprKind::Bool(b) => lit(TExprKind::Bool(*b), Type::Bool),
            ExprKind::None => match expected {
                Some(t @ Type::Optional(_)) => lit(TExprKind::None, t.clone()),
                _ => lit(TExprKind::None, Type::NoneLit),
            },
            ExprKind::SelfRef => self.self_value(cx, pos),
            ExprKind::Downcast { value, class, test_only } => self.downcast(cx, value, class, *test_only, pos),
            ExprKind::Lambda { params, ret, body } => self.lambda(cx, params, ret.as_ref(), body, expected, pos),
            ExprKind::Await(inner) => self.await_expr(cx, inner, pos),
            ExprKind::Try(inner) => self.try_expr(cx, inner, pos),
            ExprKind::Ident(name) => self.ident(cx, name, pos),
            ExprKind::Array(items) => self.array(cx, items, expected, pos),
            ExprKind::Map(entries) => self.map_literal(cx, entries, expected, pos),
            ExprKind::Interp(parts) => {
                let mut out = vec![];
                for p in parts {
                    out.push(match p {
                        InterpPart::Lit(s) => TInterp::Lit(s.clone()),
                        InterpPart::Expr(x) => {
                            let t = self.expr(cx, x, None)?;
                            if !self.m.is_plain(&t.ty) {
                                return err(format!("cannot put a `{}` value into a string", t.ty), x.pos);
                            }
                            TInterp::Expr(Box::new(t))
                        }
                    });
                }
                lit(TExprKind::Interp(out), Type::Str)
            }
            ExprKind::Unary(op, inner) => self.unary(cx, *op, inner, pos),
            ExprKind::Binary(op, l, r) => self.binary(cx, *op, l, r, pos),
            ExprKind::Call(name, targs, args) => self.call(cx, name, targs, args, expected, pos),
            ExprKind::MethodCall(recv, name, args) => self.method_call(cx, recv, name, args, pos),
            ExprKind::Field(recv, f) => self.field(cx, recv, f, pos),
            ExprKind::Index(base, idx) => {
                let base = self.expr(cx, base, None)?;
                let (index, ty) = match base.ty.clone() {
                    Type::Map(k, v) => (self.expr_to(cx, idx, &k)?, *v),
                    Type::Array(elem) => (self.expr_to(cx, idx, &Type::Int)?, *elem),
                    other => return err(format!("cannot index into `{}`", other), pos),
                };
                lit(TExprKind::Index { base: Box::new(base), index: Box::new(index) }, ty)
            }
            ExprKind::SuperInit(_) => err("`super(...)` must be its own statement inside `init`", pos),
            ExprKind::SuperCall(name, args) => self.super_call(cx, name, args, pos),
        }
    }

    fn self_value(&self, cx: &mut Ctx, pos: Span) -> Result<TExpr> {
        if let Some(st) = cx.strukt.clone() {
            if cx.is_static {
                return err("`self` is not available in a `static fn`", pos);
            }
            return Ok(texpr(TExprKind::StructSelf, self.struct_self_type(&st), pos));
        }
        let class = self.self_class(cx, pos)?;
        if cx.in_init {
            return err("`self` cannot be used as a value inside `init` (the object is not finished yet)", pos);
        }
        let ty = self.class_type(&class);
        self.self_values.borrow_mut().insert(class);
        Ok(texpr(TExprKind::ObjectSelf, ty, pos))
    }

    fn ident(&self, cx: &mut Ctx, name: &str, pos: Span) -> Result<TExpr> {
        if let Some(v) = cx.lookup(name) {
            return Ok(texpr(TExprKind::Local(name.to_string()), v.ty, pos));
        }
        if self.m.async_fns.contains(name) {
            return err(
                format!("`{}` is an `async fn`; call it (`{}(...)`) instead of using it as a value", name, name),
                pos,
            );
        }
        if let Some(sig) = self.m.functions.get(name) {
            let ty = Type::Func(sig.params.iter().map(|(_, t)| t.clone()).collect(), Box::new(sig.ret.clone()));
            return Ok(texpr(TExprKind::FnRef(name.to_string()), ty, pos));
        }
        if self.m.classes.contains_key(name) {
            return err(format!("`{}` is a class; create one with `{}(...)`", name, name), pos);
        }
        err(format!("unknown variable `{}`", name), pos)
    }

    fn array(&self, cx: &mut Ctx, items: &[Expr], expected: Option<&Type>, pos: Span) -> Result<TExpr> {
        let elem = match (expected, items.first()) {
            (Some(Type::Array(t)), _) => (**t).clone(),
            (_, Some(first)) => self.expr(cx, first, None)?.ty,
            (_, None) => return err("cannot infer the type of `[]`; add a type, e.g. `let xs: [int] = []`", pos),
        };
        let mut out = vec![];
        for it in items {
            out.push(self.expr_to(cx, it, &elem).map_err(|mut er| {
                er.message = format!(
                    "{} (all array elements must have the same type; annotate the variable to mix subclasses)",
                    er.message
                );
                er
            })?);
        }
        Ok(texpr(TExprKind::Array(out), Type::Array(Box::new(elem)), pos))
    }

    fn map_literal(&self, cx: &mut Ctx, entries: &[(Expr, Expr)], expected: Option<&Type>, pos: Span) -> Result<TExpr> {
        let (kt, vt) = match (expected, entries.first()) {
            (Some(Type::Map(k, v)), _) => ((**k).clone(), (**v).clone()),
            (_, Some((k, v))) => (self.expr(cx, k, None)?.ty, self.expr(cx, v, None)?.ty),
            (_, None) => return err("cannot infer the type of `{}`; add a type, e.g. `let m: {str: int} = {}`", pos),
        };
        if !is_map_key(&kt) {
            return err(format!("map keys must be `int`, `str` or `bool`, not `{}`", kt), pos);
        }
        let mut out = vec![];
        for (k, v) in entries {
            let key = self.expr_to(cx, k, &kt)?;
            let value = self.expr_to(cx, v, &vt).map_err(|mut er| {
                er.message = format!(
                    "{} (all map values must have the same type; annotate the variable to mix subclasses)",
                    er.message
                );
                er
            })?;
            out.push((key, value));
        }
        Ok(texpr(TExprKind::Map(out), Type::Map(Box::new(kt), Box::new(vt)), pos))
    }

    fn unary(&self, cx: &mut Ctx, op: UnOp, inner: &Expr, pos: Span) -> Result<TExpr> {
        let v = self.expr(cx, inner, None)?;
        let ty = v.ty.clone();
        match (op, &ty) {
            (UnOp::Neg, Type::Int | Type::Float) => Ok(texpr(TExprKind::Neg(Box::new(v)), ty, pos)),
            (UnOp::Neg, Type::Struct(st, sargs)) => match self.find_op(st, sargs, BinOp::Sub, None) {
                Some(o) => Ok(texpr(
                    TExprKind::StructOp { index: o.index, lhs: Box::new(v), rhs: None, swapped: false },
                    o.ret,
                    pos,
                )),
                None => err(format!("`{}` has no `op -()` for negation", st), pos),
            },
            (UnOp::Not, Type::Bool) => Ok(texpr(TExprKind::Not(Box::new(v)), ty, pos)),
            (UnOp::Neg, _) => err(format!("cannot negate `{}`", ty), pos),
            (UnOp::Not, _) => err(format!("`!` needs a `bool`, found `{}`", ty), pos),
        }
    }

    fn binary(&self, cx: &mut Ctx, op: BinOp, l: &Expr, r: &Expr, pos: Span) -> Result<TExpr> {
        let lhs = self.expr(cx, l, None)?;
        let rhs = self.expr(cx, r, None)?;
        let (lt, rt) = (lhs.ty.clone(), rhs.ty.clone());
        let arithmetic = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem);
        if let Type::Struct(st, sargs) = &lt
            && arithmetic
        {
            let Some(o) = self.find_op(st, sargs, op, Some(&rt)) else {
                return err(format!("`{}` has no `op {}` taking a `{}`", lt, op.rust(), rt), pos);
            };
            let rhs = self.convert(rhs, o.param.as_ref().unwrap_or(&Type::Void))?;
            let kind =
                TExprKind::StructOp { index: o.index, lhs: Box::new(lhs), rhs: Some(Box::new(rhs)), swapped: false };
            return Ok(texpr(kind, o.ret, pos));
        }
        // `2.0 * v` / `1.0 + v`: a number on the left uses the struct's `*` / `+` operator.
        if let Type::Struct(st, sargs) = &rt
            && matches!(op, BinOp::Add | BinOp::Mul)
            && !matches!(lt, Type::Struct(..))
            && let Some(o) = self.find_op(st, sargs, op, Some(&lt))
        {
            let lhs = self.convert(lhs, o.param.as_ref().unwrap_or(&Type::Void))?;
            let kind =
                TExprKind::StructOp { index: o.index, lhs: Box::new(lhs), rhs: Some(Box::new(rhs)), swapped: true };
            return Ok(texpr(kind, o.ret, pos));
        }
        let ty = self.binary_type(op, &lhs, &rhs, pos)?;
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            let negated = op == BinOp::Ne;
            match (&lt, &rt) {
                (Type::Optional(_), Type::NoneLit) => {
                    return Ok(texpr(TExprKind::IsNone { value: Box::new(lhs), negated }, Type::Bool, pos));
                }
                (Type::NoneLit, Type::Optional(_)) => {
                    return Ok(texpr(TExprKind::IsNone { value: Box::new(rhs), negated }, Type::Bool, pos));
                }
                _ => {}
            }
        }
        Ok(texpr(TExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), ty, pos))
    }

    /// Result type of a built-in operator, or why the operands do not fit it.
    fn binary_type(&self, op: BinOp, l: &TExpr, r: &TExpr, pos: Span) -> Result<Type> {
        let (lt, rt) = (&l.ty, &r.ty);
        use BinOp::*;
        match op {
            And | Or => {
                if *lt != Type::Bool || *rt != Type::Bool {
                    return err(
                        format!("`{}` needs `bool` on both sides, found `{}` and `{}`", op.rust(), lt, rt),
                        pos,
                    );
                }
                Ok(Type::Bool)
            }
            Eq | Ne => {
                if matches!((lt, rt), (Type::Optional(_), Type::NoneLit) | (Type::NoneLit, Type::Optional(_))) {
                    return Ok(Type::Bool);
                }
                if lt != rt {
                    return err(format!("cannot compare `{}` with `{}`", lt, rt), pos);
                }
                if !self.m.is_plain(lt) {
                    return err(format!("`{}` values cannot be compared with `{}`", lt, op.rust()), pos);
                }
                Ok(Type::Bool)
            }
            Lt | Le | Gt | Ge => {
                if lt != rt || !matches!(lt, Type::Int | Type::Float | Type::Str) {
                    return err(format!("cannot compare `{}` with `{}` using `{}`", lt, rt, op.rust()), pos);
                }
                Ok(Type::Bool)
            }
            Add if *lt == Type::Str && *rt == Type::Str => Ok(Type::Str),
            Add | Sub | Mul | Div | Rem => {
                if lt != rt || !matches!(lt, Type::Int | Type::Float) {
                    let hint = if *lt == Type::Str || *rt == Type::Str { " (convert with `str(x)` first)" } else { "" };
                    return err(format!("cannot apply `{}` to `{}` and `{}`{}", op.rust(), lt, rt, hint), pos);
                }
                Ok(lt.clone())
            }
        }
    }

    fn await_expr(&self, cx: &mut Ctx, inner: &Expr, pos: Span) -> Result<TExpr> {
        if !cx.is_async || !cx.lambdas.is_empty() {
            return err("`await` can only be used inside an `async fn` (not inside a lambda)", pos);
        }
        let v = self.expr(cx, inner, None)?;
        let ty = match &v.ty {
            Type::Async(t) | Type::Task(t) => (**t).clone(),
            other => {
                return err(
                    format!("`await` needs an `Async<T>` (from an `async fn`) or a `Task<T>`, not `{}`", other),
                    pos,
                );
            }
        };
        Ok(texpr(TExprKind::Await(Box::new(v)), ty, pos))
    }

    fn try_expr(&self, cx: &mut Ctx, inner: &Expr, pos: Span) -> Result<TExpr> {
        let v = self.expr(cx, inner, None)?;
        let ty = match (&v.ty, &cx.ret) {
            (Type::Result(t, err_ty), Type::Result(_, ret_err)) => {
                if err_ty != ret_err {
                    return err(
                        format!("`?` passes on a `{}` error, but this function returns `{}` errors", err_ty, ret_err),
                        pos,
                    );
                }
                (**t).clone()
            }
            (Type::Optional(t), Type::Optional(_)) => (**t).clone(),
            (Type::Result(..), other) => {
                return err(
                    format!("`?` can only be used in a function that returns a `Result`; this one returns `{}`", other),
                    pos,
                );
            }
            (Type::Optional(_), other) => {
                return err(
                    format!(
                        "`?` on an optional needs a function that returns an optional; this one returns `{}`",
                        other
                    ),
                    pos,
                );
            }
            (other, _) => return err(format!("`?` works on a `Result` or an optional, not on `{}`", other), pos),
        };
        Ok(texpr(TExprKind::Try(Box::new(v)), ty, pos))
    }

    fn downcast(&self, cx: &mut Ctx, value: &Expr, target: &str, test_only: bool, pos: Span) -> Result<TExpr> {
        if !self.m.classes.contains_key(target) {
            return err(format!("`as`/`is` need a class name, and `{}` is not a class", target), pos);
        }
        if !self.m.classes[target].tparams.is_empty() {
            return err(format!("cannot use `as`/`is` with generic class `{}`", target), pos);
        }
        let v = self.expr(cx, value, None)?;
        let via = match &v.ty {
            Type::Class(c) if c == target => Cast::Same,
            Type::Class(c) if self.m.is_subclass(target, c) => {
                self.cast_targets.borrow_mut().insert(target.to_string());
                let root = self.m.ancestors(c).last().cloned().unwrap_or_else(|| c.clone());
                Cast::Hook(class_trait(&root))
            }
            Type::Interface(i) if self.m.all_interfaces(target).contains(i) => {
                self.cast_targets.borrow_mut().insert(target.to_string());
                Cast::Hook(i.clone())
            }
            Type::Class(_) | Type::Interface(_) => {
                return err(format!("a `{}` can never be a `{}`, so this check would always fail", v.ty, target), pos);
            }
            other => return err(format!("`as`/`is` work on objects, not on `{}`", other), pos),
        };
        let ty = if test_only { Type::Bool } else { Type::Optional(Box::new(Type::Class(target.to_string()))) };
        let kind = TExprKind::Downcast { value: Box::new(v), target: target.to_string(), via, test_only };
        Ok(texpr(kind, ty, pos))
    }

    /// `Enum`, used as a receiver (`Shape.Circle`), unless a variable shadows it.
    pub(super) fn type_receiver(&self, cx: &Ctx, recv: &Expr) -> Option<String> {
        match &recv.kind {
            ExprKind::Ident(n) if cx.lookup(n).is_none() && self.m.enums.contains_key(n) => Some(n.clone()),
            _ => None,
        }
    }

    fn field(&self, cx: &mut Ctx, recv: &Expr, f: &str, pos: Span) -> Result<TExpr> {
        if let Some(t) = self.type_receiver(cx, recv) {
            return self.variant_value(cx, &t, f, None, pos);
        }
        if let ExprKind::Ident(n) = &recv.kind
            && cx.lookup(n).is_none()
            && let Some(info) = self.m.structs.get(n)
        {
            if let Some((_, t)) = info.consts.iter().find(|(c, _)| c == f) {
                return Ok(texpr(TExprKind::StructConst { strukt: n.clone(), name: f.to_string() }, t.clone(), pos));
            }
            return err(
                format!("`{}` has no constant `{}` (constants are declared `static {}: Type = value`)", n, f, f),
                pos,
            );
        }
        if let Some(ns) = self.library_receiver(cx, recv) {
            return self.library_value(&ns, f, pos);
        }
        if matches!(recv.kind, ExprKind::SelfRef) {
            return self.self_field(cx, recv, f, pos);
        }
        let recv_t = self.expr(cx, recv, None)?;
        let ty = match &recv_t.ty {
            Type::Struct(st, sargs) => self.struct_field_type(cx, st, sargs, f, pos)?,
            Type::Generic(c, targs) => subst(&self.field_type(cx, c, f, pos)?, &self.m.bindings(c, targs)),
            Type::Class(c) => self.field_type(cx, c, f, pos)?,
            Type::Array(_) | Type::Str | Type::Map(..) if f == "len" => {
                return err("`len` is a method: use `.len()`", pos);
            }
            Type::Interface(i) => {
                return err(format!("interface `{}` has no fields; only methods can be used", i), pos);
            }
            other => return err(format!("`{}` has no field `{}`", other, f), pos),
        };
        Ok(texpr(TExprKind::Field { recv: Box::new(recv_t), field: f.to_string() }, ty, pos))
    }

    fn self_field(&self, cx: &mut Ctx, recv: &Expr, f: &str, pos: Span) -> Result<TExpr> {
        if let Some(st) = cx.strukt.clone() {
            if cx.is_static {
                return err("`self` is not available in a `static fn`", pos);
            }
            let sargs = self.struct_param_args(&st);
            let ty = self.struct_field_type(cx, &st, &sargs, f, pos)?;
            return Ok(texpr(TExprKind::SelfField { field: f.to_string(), wrapped: false }, ty, pos));
        }
        let class = self.self_class(cx, recv.pos)?;
        let ty = self.field_type(cx, &class, f, pos)?;
        let wrapped = self.m.is_wrapped_field(&class, f);
        Ok(texpr(TExprKind::SelfField { field: f.to_string(), wrapped }, ty, pos))
    }

    fn super_call(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let class = self.self_class(cx, pos)?;
        let Some((owner, sig)) = self.m.find_super_method(&class, name) else {
            return err(format!("no base class of `{}` has a method `{}`", class, name), pos);
        };
        self.check_method_access(cx, &owner, name, pos)?;
        if self.m.classes[&owner].abstract_methods.contains(name) {
            return err(format!("`{}.{}` is abstract and has no body to call", owner, name), pos);
        }
        let args = self.args(cx, &sig, args, pos, name)?;
        let callee = Callee::SuperMethod { owner, name: name.to_string() };
        Ok(texpr(TExprKind::Call { callee, args }, sig.ret, pos))
    }
}
