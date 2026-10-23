//! Assignments, the places they store into, and field and operator lookups.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx};
use crate::model::StructOp;
use crate::tast::*;
use crate::types::{Type, subst};

impl Checker<'_> {
    pub(super) fn assign(&self, cx: &mut Ctx, target: &Expr, op: AssignOp, value: &Expr) -> Result<TStmt> {
        // A field of a base-class reference: go through the generated setter.
        if let ExprKind::Field(recv, f) = &target.kind
            && !matches!(recv.kind, ExprKind::SelfRef)
        {
            let recv_t = self.expr(cx, recv, None)?;
            if let Type::Class(c) = &recv_t.ty
                && self.m.has_subclasses(c)
            {
                let ft = self.field_type(cx, c, f, target.pos)?;
                let value = self.assign_value(cx, op, &ft, value)?;
                return Ok(TStmt::SetField { recv: recv_t, field: f.clone(), op, value });
            }
        }
        // `m[k] = v` inserts (the key may be new).
        if let (ExprKind::Index(base, key), AssignOp::Set) = (&target.kind, op) {
            let map = self.place(cx, base)?;
            if let Type::Map(k, vt) = map.ty().clone() {
                let value = self.expr_to(cx, value, &vt)?;
                let key = self.expr_to(cx, key, &k)?;
                return Ok(TStmt::MapInsert { map, key, value });
            }
        }
        let place = self.place(cx, target)?;
        if op != AssignOp::Set
            && let Type::Struct(st, sargs) = place.ty().clone()
        {
            return self.struct_assign(cx, place, &st, &sargs, op, value);
        }
        let ty = place.ty().clone();
        let value = self.assign_value(cx, op, &ty, value)?;
        Ok(TStmt::Assign { place, op, value })
    }

    /// `v += w` on a struct, with its `op +`.
    fn struct_assign(
        &self,
        cx: &mut Ctx,
        place: TPlace,
        st: &str,
        sargs: &[Type],
        op: AssignOp,
        value: &Expr,
    ) -> Result<TStmt> {
        let bin = match op {
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            _ => BinOp::Rem,
        };
        let v = self.expr(cx, value, None)?;
        let ty = place.ty().clone();
        let Some(o) = self.find_op(st, sargs, bin, Some(&v.ty)) else {
            return err(format!("`{}` has no `op {}` taking a `{}`", ty, bin.rust(), v.ty), value.pos);
        };
        if o.ret != ty {
            return err(
                format!("`{}=` needs `op {}` to return `{}`, but it returns `{}`", bin.rust(), bin.rust(), st, o.ret),
                value.pos,
            );
        }
        let param = o.param.clone().unwrap_or(Type::Void);
        let value = self.convert(v, &param)?;
        Ok(TStmt::StructAssign { place, index: o.index, value })
    }

    fn assign_value(&self, cx: &mut Ctx, op: AssignOp, ty: &Type, value: &Expr) -> Result<TExpr> {
        if op != AssignOp::Set {
            let ok = match op {
                AssignOp::Add => matches!(ty, Type::Int | Type::Float | Type::Str),
                _ => matches!(ty, Type::Int | Type::Float),
            };
            if !ok {
                return err(format!("`{}=` is not supported for `{}`", op_sym(op), ty), value.pos);
            }
        }
        self.expr_to(cx, value, ty)
    }

    /// Where an assignment (or a method that changes its receiver) stores into.
    pub(super) fn place(&self, cx: &mut Ctx, e: &Expr) -> Result<TPlace> {
        match &e.kind {
            ExprKind::Ident(name) => match cx.lookup(name) {
                Some(v) if v.mutable => Ok(TPlace::Local { name: name.clone(), ty: v.ty }),
                Some(v) if v.captured => err(
                    format!(
                        "cannot change `{}` inside a lambda: lambdas get a read-only copy of outside variables",
                        name
                    ),
                    e.pos,
                ),
                Some(_) => err(
                    format!("cannot change `{}`: it was declared with `let` (use `var` for a mutable variable)", name),
                    e.pos,
                ),
                None => err(format!("unknown variable `{}`", name), e.pos),
            },
            ExprKind::Field(recv, f) => self.field_place(cx, recv, f, e.pos),
            ExprKind::Index(base, idx) => {
                let base = self.place(cx, base)?;
                let (index, ty) = match base.ty().clone() {
                    Type::Map(k, v) => (self.expr_to(cx, idx, &k)?, *v),
                    Type::Array(elem) => (self.expr_to(cx, idx, &Type::Int)?, *elem),
                    other => return err(format!("cannot index into `{}`", other), e.pos),
                };
                Ok(TPlace::Index { base: Box::new(base), index: Box::new(index), ty })
            }
            ExprKind::SelfRef => err("cannot assign to `self`", e.pos),
            _ => err("cannot assign to this expression", e.pos),
        }
    }

    fn field_place(&self, cx: &mut Ctx, recv: &Expr, f: &str, pos: Span) -> Result<TPlace> {
        if self.type_receiver(cx, recv).is_some() {
            return err("cannot assign to an enum variant", pos);
        }
        if matches!(recv.kind, ExprKind::SelfRef) {
            if let Some(st) = cx.strukt.clone() {
                if !cx.self_mut {
                    return err("this method changes `self`; declare it `mut fn`", pos);
                }
                let sargs = self.struct_param_args(&st);
                let ty = self.struct_field_type(cx, &st, &sargs, f, pos)?;
                return Ok(TPlace::SelfField { field: f.to_string(), ty, wrapped: false });
            }
            let class = self.self_class(cx, recv.pos)?;
            let ty = self.field_type(cx, &class, f, pos)?;
            return Ok(TPlace::SelfField { field: f.to_string(), ty, wrapped: self.m.is_wrapped_field(&class, f) });
        }
        let recv_t = self.expr(cx, recv, None)?;
        match recv_t.ty.clone() {
            // A field of a struct value: change it where the struct lives.
            Type::Struct(st, sargs) => {
                let ty = self.struct_field_type(cx, &st, &sargs, f, pos)?;
                let base = self.place(cx, recv)?;
                Ok(TPlace::StructField { base: Box::new(base), field: f.to_string(), ty })
            }
            Type::Class(c) if !self.m.has_subclasses(&c) => {
                let ty = self.field_type(cx, &c, f, pos)?;
                Ok(TPlace::ObjectField { recv: Box::new(recv_t), field: f.to_string(), ty, wrapped: false })
            }
            Type::Generic(c, targs) => {
                let ty = subst(&self.field_type(cx, &c, f, pos)?, &self.m.bindings(&c, &targs));
                let wrapped = self.m.is_wrapped_field(&c, f);
                Ok(TPlace::ObjectField { recv: Box::new(recv_t), field: f.to_string(), ty, wrapped })
            }
            Type::Class(_) => err(
                "cannot modify part of a field through a base-class reference; add a method to the class instead",
                pos,
            ),
            other => err(format!("`{}` has no fields", other), pos),
        }
    }

    /// Type of a class's field, checking `private`.
    pub(super) fn field_type(&self, cx: &Ctx, class: &str, field: &str, pos: Span) -> Result<Type> {
        match self.m.all_fields(class).into_iter().find(|(n, _, _)| n == field) {
            Some((_, t, owner)) => {
                if self.m.classes[&owner].private_fields.contains(field) && cx.class.as_deref() != Some(owner.as_str())
                {
                    return err(format!("field `{}` is private to `{}`", field, owner), pos);
                }
                Ok(t)
            }
            None => err(format!("`{}` has no field `{}`", class, field), pos),
        }
    }

    /// Type of struct field `f` (with the struct's type arguments), checking `private`.
    pub(super) fn struct_field_type(&self, cx: &Ctx, st: &str, args: &[Type], f: &str, pos: Span) -> Result<Type> {
        let info = &self.m.structs[st];
        match info.fields.iter().find(|(n, _)| n == f) {
            Some((_, t)) => {
                if info.private_fields.contains(f) && cx.strukt.as_deref() != Some(st) {
                    return err(format!("field `{}` is private to `{}`", f, st), pos);
                }
                Ok(subst(t, &self.m.struct_bindings(st, args)))
            }
            None if info.methods.contains_key(f) => {
                err(format!("`{}` is a method of `{}`; call it with `()`", f, st), pos)
            }
            None => err(format!("`{}` has no field `{}`", st, f), pos),
        }
    }

    /// A struct's own type parameters, as type arguments (inside its methods).
    pub(super) fn struct_param_args(&self, st: &str) -> Vec<Type> {
        self.m.structs[st].tparams.iter().map(|t| Type::Param(t.clone())).collect()
    }

    /// The operator method of struct `st` for `op` with a right-hand side of type `rhs`.
    pub(super) fn find_op(&self, st: &str, args: &[Type], op: BinOp, rhs: Option<&Type>) -> Option<StructOp> {
        let map = self.m.struct_bindings(st, args);
        let ops: Vec<StructOp> = self.m.structs[st]
            .ops
            .iter()
            .filter(|o| o.op == op)
            .map(|o| StructOp {
                param: o.param.as_ref().map(|p| subst(p, &map)),
                ret: subst(&o.ret, &map),
                ..o.clone()
            })
            .collect();
        match rhs {
            None => ops.into_iter().find(|o| o.param.is_none()),
            Some(rt) => {
                let with_param: Vec<&StructOp> = ops.iter().filter(|o| o.param.is_some()).collect();
                with_param
                    .iter()
                    .find(|o| o.param.as_ref() == Some(rt))
                    .or_else(|| with_param.iter().find(|o| o.param.as_ref().is_some_and(|p| self.m.assignable(rt, p))))
                    .map(|o| (*o).clone())
            }
        }
    }
}

pub(super) fn op_sym(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "",
        AssignOp::Add => "+",
        AssignOp::Sub => "-",
        AssignOp::Mul => "*",
        AssignOp::Div => "/",
        AssignOp::Rem => "%",
    }
}
