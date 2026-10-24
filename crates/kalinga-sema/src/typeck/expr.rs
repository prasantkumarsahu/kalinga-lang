//! Expressions other than calls: literals, names, `self`, collections, interpolation,
//! operators, fields, indexing, `await`, `?` and downcasts.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, bound_hint, texpr};
use crate::index::PROBE;
use crate::model::class_trait;
use crate::suggest::unknown;
use crate::tast::*;
use crate::types::{Type, is_map_key, subst};

impl Checker<'_> {
    /// Checks `e`; `expected` is the type the context wants, which decides the type of
    /// `none`, `[]`, `{}`, lambdas, `Ok(...)`, generic calls and more.
    pub(super) fn expr(&self, cx: &mut Ctx, e: &Expr, expected: Option<&Type>) -> Result<TExpr> {
        if matches!(&e.kind, ExprKind::Ident(n) if n == PROBE) {
            return self.probe_scope(cx, expected, e.pos);
        }
        let t = self.expr_kind(cx, e, expected)?;
        self.note_expr(cx, e, &t);
        Ok(t)
    }

    fn expr_kind(&self, cx: &mut Ctx, e: &Expr, expected: Option<&Type>) -> Result<TExpr> {
        let pos = e.pos;
        let lit = |kind, ty| Ok(texpr(kind, ty, pos));
        match &e.kind {
            // A whole number where a float is wanted is that float: `Vec2(600, 300)`.
            ExprKind::Int(v) if expected == Some(&Type::Float) => lit(TExprKind::Float(*v as f64), Type::Float),
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
            ExprKind::If { cond, then, otherwise } => self.if_expr(cx, cond, then, otherwise, expected, pos),
            ExprKind::Named(name, _) => err(
                format!(
                    "`{}: ...` names a parameter, which only works when calling a function, method or constructor that declares it",
                    name
                ),
                pos,
            ),
            ExprKind::Match { value, arms } => self.match_expr(cx, value, arms, expected, pos),
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
                            if !self.showable(&t.ty) {
                                let hint = bound_hint(&t.ty, "Show");
                                return err(format!("cannot put a `{}` value into a string{}", t.ty, hint), x.pos);
                            }
                            TInterp::Expr(Box::new(t))
                        }
                    });
                }
                lit(TExprKind::Interp(out), Type::Str)
            }
            ExprKind::Unary(op, inner) => self.unary(cx, *op, inner, expected, pos),
            ExprKind::Binary(op, l, r) => self.binary(cx, *op, l, r, pos),
            ExprKind::Call(name, targs, args) => self.call(cx, name, targs, args, expected, pos),
            // `Tree.Node(...)` and `Tree.Leaf` see the expected type, for generic enums.
            ExprKind::MethodCall(recv, name, args) => match self.type_receiver(cx, recv) {
                Some(t) if name != PROBE => self.variant_value(cx, &t, name, Some(args), expected, pos),
                _ => self.method_call(cx, recv, name, args, pos),
            },
            ExprKind::Field(recv, f) => match self.type_receiver(cx, recv) {
                Some(t) if f != PROBE => self.variant_value(cx, &t, f, None, expected, pos),
                _ => self.field(cx, recv, f, pos),
            },
            ExprKind::Index(base, idx) => {
                let base = self.expr(cx, base, None)?;
                let (index, ty) = match base.ty.clone() {
                    Type::Map(k, v) => (self.expr_to(cx, idx, &k)?, *v),
                    Type::Array(elem) => (self.expr_to(cx, idx, &Type::Int)?, *elem),
                    Type::Str => (self.expr_to(cx, idx, &Type::Int)?, Type::Str),
                    other => return err(format!("cannot index into `{}`", other), pos),
                };
                lit(TExprKind::Index { base: Box::new(base), index: Box::new(index) }, ty)
            }
            ExprKind::Slice(base, from, to) => {
                let base = self.expr(cx, base, None)?;
                if !matches!(base.ty, Type::Array(_) | Type::Str) {
                    return err(format!("cannot slice `{}` (slices work on arrays and strings)", base.ty), pos);
                }
                let mut bound = |b: &Option<Box<Expr>>| -> Result<Option<Box<TExpr>>> {
                    b.as_ref().map(|x| self.expr_to(cx, x, &Type::Int).map(Box::new)).transpose()
                };
                let (from, to) = (bound(from)?, bound(to)?);
                let ty = base.ty.clone();
                lit(TExprKind::Slice { base: Box::new(base), from, to }, ty)
            }
            ExprKind::SuperInit(_) => err("`super(...)` must be its own statement inside `init`", pos),
            ExprKind::SuperCall(name, args) => self.super_call(cx, name, args, pos),
        }
    }

    pub(super) fn self_value(&self, cx: &mut Ctx, pos: Span) -> Result<TExpr> {
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
        if let Some(ty) = self.m.consts.get(name) {
            let kind = TExprKind::Static { owner: String::new(), name: name.to_string(), mutable: false };
            return Ok(texpr(kind, ty.clone(), pos));
        }
        if self.m.classes.contains_key(name) {
            return err(format!("`{}` is a class; create one with `{}(...)`", name, name), pos);
        }
        let locals = cx.visible();
        let names = locals.iter().map(|(n, _)| n.as_str());
        let globals = self.m.functions.keys().chain(self.m.consts.keys()).map(String::as_str);
        unknown(format!("unknown variable `{}`", name), pos, name, names.chain(globals))
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

    fn unary(&self, cx: &mut Ctx, op: UnOp, inner: &Expr, expected: Option<&Type>, pos: Span) -> Result<TExpr> {
        let wants_float = op == UnOp::Neg && expected == Some(&Type::Float);
        let v = self.expr(cx, inner, if wants_float { expected } else { None })?;
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
        let (lhs, rhs) = self.operands(cx, l, r)?;
        let (lt, rt) = (lhs.ty.clone(), rhs.ty.clone());
        let arithmetic = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem);
        if let Type::Struct(st, sargs) = &lt
            && arithmetic
        {
            // `v * 2` uses an `op *` taking an int if there is one, else one taking a float.
            let (rhs, o) = match self.find_op(st, sargs, op, Some(&rt)) {
                Some(o) => (rhs, o),
                None => {
                    let as_float = int_literal_as_float(rhs);
                    match self.find_op(st, sargs, op, Some(&as_float.ty)) {
                        Some(o) if as_float.ty == Type::Float => (as_float, o),
                        _ => return err(format!("`{}` has no `op {}` taking a `{}`", lt, op.rust(), rt), pos),
                    }
                }
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
            && let Some((lhs, o)) = self.find_op(st, sargs, op, Some(&lt)).map(|o| (lhs.clone(), o)).or_else(|| {
                let as_float = int_literal_as_float(lhs.clone());
                let o = self.find_op(st, sargs, op, Some(&Type::Float)).filter(|_| as_float.ty == Type::Float)?;
                Some((as_float, o))
            })
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

    /// Both operands of a binary operator. A whole-number literal next to a float is a
    /// float: `speed * 2`, `x > 0`.
    fn operands(&self, cx: &mut Ctx, l: &Expr, r: &Expr) -> Result<(TExpr, TExpr)> {
        let lhs = self.expr(cx, l, None)?;
        let rhs = self.expr(cx, r, None)?;
        let floaty = |t: &Type| *t == Type::Float;
        Ok(match (floaty(&lhs.ty), floaty(&rhs.ty)) {
            (true, false) => (lhs, int_literal_as_float(rhs)),
            (false, true) => (int_literal_as_float(lhs), rhs),
            _ => (lhs, rhs),
        })
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
                if !self.comparable(lt) {
                    let hint = bound_hint(lt, "Eq");
                    return err(format!("`{}` values cannot be compared with `{}`{}", lt, op.rust(), hint), pos);
                }
                Ok(Type::Bool)
            }
            Lt | Le | Gt | Ge => {
                if lt != rt || !(matches!(lt, Type::Int | Type::Float | Type::Str) || self.ordered_param(lt)) {
                    let hint = if lt == rt { bound_hint(lt, "Ord") } else { String::new() };
                    return err(format!("cannot compare `{}` with `{}` using `{}`{}", lt, rt, op.rust(), hint), pos);
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
            Type::Interface(i, _) if self.m.all_interfaces(target).contains(i) => {
                self.cast_targets.borrow_mut().insert(target.to_string());
                Cast::Hook(i.clone())
            }
            Type::Class(_) | Type::Interface(..) => {
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
        if f == PROBE {
            return self.probe_members(cx, recv, pos);
        }
        if let Some(t) = self.type_receiver(cx, recv) {
            return self.variant_value(cx, &t, f, None, None, pos);
        }
        if let Some(n) = self.static_owner(cx, recv) {
            let Some(s) = self.m.static_value(n, f) else {
                let values = self.m.static_values_of(n).unwrap_or_default().iter().map(|(v, _)| v.as_str());
                return unknown(format!("`{}` has no `const` or `static var` named `{}`", n, f), pos, f, values);
            };
            let kind = TExprKind::Static { owner: n.clone(), name: f.to_string(), mutable: s.mutable };
            return Ok(texpr(kind, s.ty.clone(), pos));
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
            Type::Interface(i, _) => {
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

/// `e` as a float literal when it is a whole-number literal (`2` or `-2`).
fn int_literal_as_float(e: TExpr) -> TExpr {
    match e.kind {
        TExprKind::Int(v) => texpr(TExprKind::Float(v as f64), Type::Float, e.pos),
        TExprKind::Neg(inner) if matches!(inner.kind, TExprKind::Int(_)) => {
            texpr(TExprKind::Neg(Box::new(int_literal_as_float(*inner))), Type::Float, e.pos)
        }
        kind => TExpr { kind, ..e },
    }
}
