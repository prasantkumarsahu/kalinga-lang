//! Method calls: on objects, structs and `self`, static methods, and the methods of
//! arrays, maps, strings, `Result`, `Task`, `Channel` and `Shared`.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, texpr};
use crate::tast::*;
use crate::types::{Type, subst_sig};

impl Checker<'_> {
    pub(super) fn method_call(&self, cx: &mut Ctx, recv: &Expr, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        if let Some(t) = self.type_receiver(cx, recv) {
            return self.variant_value(cx, &t, name, Some(args), pos);
        }
        if let Some(ns) = self.library_receiver(cx, recv) {
            return self.library_call(cx, &ns, name, args, pos);
        }
        if let ExprKind::Ident(n) = &recv.kind
            && cx.lookup(n).is_none()
        {
            if self.m.classes.contains_key(n) {
                return self.static_call(cx, n, name, args, pos);
            }
            if self.m.structs.contains_key(n) {
                return self.struct_static_call(cx, n, name, args, pos);
            }
        }
        if matches!(recv.kind, ExprKind::SelfRef) {
            return self.self_method_call(cx, recv, name, args, pos);
        }
        let recv_t = self.expr(cx, recv, None)?;
        match recv_t.ty.clone() {
            Type::Task(t) | Type::Channel(t) | Type::Shared(t) => {
                self.concurrency_method(cx, recv_t, &t, name, args, pos)
            }
            Type::Array(elem) if name == "par_map" => self.par_map(cx, recv_t, &elem, args, pos),
            Type::Result(t, e) => self.result_method(cx, recv_t, &t, &e, name, args, pos),
            Type::Struct(..) => self.struct_method_call(cx, recv, recv_t, name, args, pos),
            Type::Map(..) if name == "remove" => self.mutating_method(cx, recv, name, args, pos),
            Type::Array(_) if matches!(name, "sort" | "sort_by" | "push" | "pop") => {
                self.mutating_method(cx, recv, name, args, pos)
            }
            Type::Array(_) | Type::Str | Type::Map(..) => self.value_method(cx, recv_t, name, args, pos),
            Type::Generic(c, targs) => {
                let Some((owner, sig)) = self.m.find_method(&c, name) else {
                    return err(format!("`{}` has no method `{}`", recv_t.ty, name), pos);
                };
                self.check_method_access(cx, &owner, name, pos)?;
                let sig = subst_sig(&sig, &self.m.bindings(&c, &targs));
                let args = self.args(cx, &sig, args, pos, name)?;
                let callee = Callee::Method { recv: Box::new(recv_t), name: name.to_string(), dispatch: None };
                Ok(texpr(TExprKind::Call { callee, args }, sig.ret, pos))
            }
            Type::Class(c) => self.object_method(cx, recv_t, &c, name, args, pos),
            Type::Interface(i) => {
                let Some(sig) = self.m.interface_method(&i, name) else {
                    return err(format!("interface `{}` has no method `{}`", i, name), pos);
                };
                let args = self.args(cx, &sig, args, pos, name)?;
                let dispatch = self.m.declaring_interface(&i, name);
                let callee = Callee::Method { recv: Box::new(recv_t), name: name.to_string(), dispatch };
                Ok(texpr(TExprKind::Call { callee, args }, sig.ret, pos))
            }
            other => err(format!("`{}` has no method `{}`", other, name), pos),
        }
    }

    fn self_method_call(&self, cx: &mut Ctx, recv: &Expr, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        if let Some(st) = cx.strukt.clone() {
            if cx.is_static {
                return err("`self` is not available in a `static fn`", pos);
            }
            let self_t = texpr(TExprKind::StructSelf, self.struct_self_type(&st), recv.pos);
            return self.struct_method_call(cx, recv, self_t, name, args, pos);
        }
        let class = self.self_class(cx, recv.pos)?;
        let sig = match self.m.find_method(&class, name) {
            Some((owner, s)) => {
                self.check_method_access(cx, &owner, name, pos)?;
                s
            }
            None if self.m.find_static(&class, name).is_some() => {
                return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, class, name), pos);
            }
            None => {
                if let Some(Type::Func(ps, r)) = self.func_field(&class, name) {
                    let ft = self.field_type(cx, &class, name, pos)?;
                    let f = texpr(TExprKind::SelfField { field: name.to_string(), wrapped: false }, ft, pos);
                    return self.call_value(cx, f, &ps, &r, args, pos, name);
                }
                return err(format!("`{}` has no method `{}`", class, name), pos);
            }
        };
        let args = self.args(cx, &sig, args, pos, name)?;
        Ok(texpr(TExprKind::Call { callee: Callee::SelfMethod(name.to_string()), args }, sig.ret, pos))
    }

    fn object_method(
        &self,
        cx: &mut Ctx,
        recv_t: TExpr,
        c: &str,
        name: &str,
        args: &[Expr],
        pos: Span,
    ) -> Result<TExpr> {
        let sig = match self.m.find_method(c, name) {
            Some((owner, s)) => {
                self.check_method_access(cx, &owner, name, pos)?;
                s
            }
            None if self.m.find_static(c, name).is_some() => {
                return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, c, name), pos);
            }
            None => {
                if let Some(Type::Func(ps, r)) = self.func_field(c, name) {
                    let ft = self.field_type(cx, c, name, pos)?;
                    let f = texpr(TExprKind::Field { recv: Box::new(recv_t), field: name.to_string() }, ft, pos);
                    return self.call_value(cx, f, &ps, &r, args, pos, name);
                }
                return err(format!("`{}` has no method `{}`", c, name), pos);
            }
        };
        let args = self.args(cx, &sig, args, pos, name)?;
        let dispatch = self.m.has_subclasses(c).then(|| self.m.dispatch_trait(c, name));
        let callee = Callee::Method { recv: Box::new(recv_t), name: name.to_string(), dispatch };
        Ok(texpr(TExprKind::Call { callee, args }, sig.ret, pos))
    }

    /// Type of field `name` of a class, when it holds a function.
    fn func_field(&self, class: &str, name: &str) -> Option<Type> {
        self.m
            .all_fields(class)
            .into_iter()
            .find(|(n, t, _)| n == name && matches!(t, Type::Func(..)))
            .map(|(_, t, _)| t)
    }

    /// Calls `method` on a struct value: on `self`, on a place (for `mut fn`), or on any value.
    fn struct_method_call(
        &self,
        cx: &mut Ctx,
        recv: &Expr,
        recv_t: TExpr,
        name: &str,
        args: &[Expr],
        pos: Span,
    ) -> Result<TExpr> {
        let Type::Struct(st, targs) = &recv_t.ty else {
            return err(format!("`{}` has no method `{}`", recv_t.ty, name), pos);
        };
        let info = &self.m.structs[st];
        let mut m = match info.methods.get(name) {
            Some(m) if !m.is_static => m.clone(),
            Some(_) => return err(format!("`{}` is a `static fn`; call it as `{}.{}(...)`", name, st, name), pos),
            None => return err(format!("`{}` has no method `{}`", st, name), pos),
        };
        if m.is_private && cx.strukt.as_deref() != Some(st.as_str()) {
            return err(format!("method `{}` is private to `{}`", name, st), pos);
        }
        m.sig = subst_sig(&m.sig, &self.m.struct_bindings(st, targs));
        let targs = self.args(cx, &m.sig, args, pos, name)?;
        if matches!(recv.kind, ExprKind::SelfRef) {
            if m.is_mut && !cx.self_mut {
                return err(format!("`{}` changes `self`, so it can only be called from another `mut fn`", name), pos);
            }
            let callee = Callee::StructMethod { recv: Box::new(recv_t), name: name.to_string() };
            return Ok(texpr(TExprKind::Call { callee, args: targs }, m.sig.ret, pos));
        }
        if m.is_mut {
            let place = self.place(cx, recv).map_err(|mut e| {
                e.message = format!("{} (`{}` is a `mut fn`, so it needs a variable it can change)", e.message, name);
                e
            })?;
            let op = Mutation::StructMethod(name.to_string());
            return Ok(texpr(TExprKind::Mutate { place, op, args: targs }, m.sig.ret, pos));
        }
        let callee = Callee::StructMethod { recv: Box::new(recv_t), name: name.to_string() };
        Ok(texpr(TExprKind::Call { callee, args: targs }, m.sig.ret, pos))
    }

    fn concurrency_method(
        &self,
        cx: &mut Ctx,
        recv_t: TExpr,
        t: &Type,
        name: &str,
        args: &[Expr],
        pos: Span,
    ) -> Result<TExpr> {
        let kind = recv_t.ty.clone();
        let done =
            |op, recv_t, args, ty| Ok(texpr(TExprKind::BuiltinMethod { op, recv: Box::new(recv_t), args }, ty, pos));
        match (&kind, name, args.len()) {
            (Type::Task(_), "join", 0) => done(BuiltinMethod::Join, recv_t, vec![], t.clone()),
            (Type::Task(_), "is_done", 0) => done(BuiltinMethod::IsDone, recv_t, vec![], Type::Bool),
            (Type::Channel(_), "send", 1) => {
                let v = self.expr_to(cx, &args[0], t)?;
                done(BuiltinMethod::Send, recv_t, vec![v], Type::Void)
            }
            (Type::Channel(_), "receive", 0) => {
                done(BuiltinMethod::Receive, recv_t, vec![], Type::Optional(Box::new(t.clone())))
            }
            (Type::Channel(_), "try_receive", 0) => {
                done(BuiltinMethod::TryReceive, recv_t, vec![], Type::Optional(Box::new(t.clone())))
            }
            (Type::Channel(_), "close", 0) => done(BuiltinMethod::Close, recv_t, vec![], Type::Void),
            (Type::Shared(_), "get", 0) => done(BuiltinMethod::Get, recv_t, vec![], t.clone()),
            (Type::Shared(_), "set", 1) => {
                let v = self.expr_to(cx, &args[0], t)?;
                done(BuiltinMethod::Set, recv_t, vec![v], Type::Void)
            }
            (Type::Shared(_), "update", 1) => {
                let fty = Type::Func(vec![t.clone()], Box::new(t.clone()));
                let f = self.expr_to(cx, &args[0], &fty)?;
                done(BuiltinMethod::Update, recv_t, vec![f], Type::Void)
            }
            (Type::Task(_), _, _) => err(
                format!(
                    "`Task` has no method `{}` with {} argument(s) (use `join()` or `is_done()`)",
                    name,
                    args.len()
                ),
                pos,
            ),
            (Type::Channel(_), _, _) => err(
                format!(
                    "`Channel` has no method `{}` with {} argument(s) (use send, receive, try_receive or close)",
                    name,
                    args.len()
                ),
                pos,
            ),
            _ => err(
                format!("`Shared` has no method `{}` with {} argument(s) (use get, set or update)", name, args.len()),
                pos,
            ),
        }
    }

    fn par_map(&self, cx: &mut Ctx, recv_t: TExpr, elem: &Type, args: &[Expr], pos: Span) -> Result<TExpr> {
        if args.len() != 1 {
            return err("`par_map` takes one function: `xs.par_map(fn(x) => ...)`", pos);
        }
        if !self.m.is_sendable(elem) {
            return err(format!("`par_map` needs elements that can move between threads; `{}` cannot", elem), pos);
        }
        let f = self.thread_lambda(cx, &args[0], vec![elem.clone()], "par_map")?;
        let rt = match &f.ty {
            Type::Func(_, r) => (**r).clone(),
            other => other.clone(),
        };
        if rt == Type::Void {
            return err("the function given to `par_map` must return a value", args[0].pos);
        }
        let kind = TExprKind::BuiltinMethod { op: BuiltinMethod::ParMap, recv: Box::new(recv_t), args: vec![f] };
        Ok(texpr(kind, Type::Array(Box::new(rt)), pos))
    }

    #[allow(clippy::too_many_arguments)]
    fn result_method(
        &self,
        cx: &mut Ctx,
        recv_t: TExpr,
        t: &Type,
        e: &Type,
        name: &str,
        args: &[Expr],
        pos: Span,
    ) -> Result<TExpr> {
        let (op, args, ty) = match (name, args.len()) {
            ("is_ok", 0) => (BuiltinMethod::IsOk, vec![], Type::Bool),
            ("is_err", 0) => (BuiltinMethod::IsErr, vec![], Type::Bool),
            ("ok", 0) => (BuiltinMethod::OkValue, vec![], Type::Optional(Box::new(t.clone()))),
            ("err", 0) => (BuiltinMethod::ErrValue, vec![], Type::Optional(Box::new(e.clone()))),
            ("unwrap_or", 1) => (BuiltinMethod::UnwrapOr, vec![self.expr_to(cx, &args[0], t)?], t.clone()),
            ("unwrap", 0) => (BuiltinMethod::Unwrap, vec![], t.clone()),
            ("is_ok" | "is_err" | "ok" | "err" | "unwrap" | "unwrap_or", _) => {
                return err(format!("wrong number of arguments for `{}`", name), pos);
            }
            _ => {
                return err(
                    format!(
                        "`Result` has no method `{}` (use is_ok, is_err, ok, err, unwrap, unwrap_or or `match`)",
                        name
                    ),
                    pos,
                );
            }
        };
        Ok(texpr(TExprKind::BuiltinMethod { op, recv: Box::new(recv_t), args }, ty, pos))
    }

    /// `push`, `pop`, `sort`, `sort_by` on arrays and `remove` on maps change the receiver,
    /// so it must be a place that can be changed.
    fn mutating_method(&self, cx: &mut Ctx, recv: &Expr, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let place = self.place(cx, recv)?;
        let done = |place, op, args, ty| Ok(texpr(TExprKind::Mutate { place, op, args }, ty, pos));
        match (place.ty().clone(), name) {
            (Type::Map(k, v), "remove") => {
                if args.len() != 1 {
                    return err("`remove` takes exactly 1 argument (the key)", pos);
                }
                let key = self.expr_to(cx, &args[0], &k)?;
                done(place, Mutation::Remove, vec![key], Type::Optional(v))
            }
            (Type::Array(elem), "sort") => {
                if !args.is_empty() {
                    return err("`sort` takes no arguments (use `sort_by(fn(a, b) => ...)` for a custom order)", pos);
                }
                if !matches!(*elem, Type::Int | Type::Str | Type::Bool | Type::Float) {
                    return err(
                        format!("`sort` works on arrays of int, float, str or bool, not `{}`; use `sort_by`", elem),
                        pos,
                    );
                }
                done(place, Mutation::Sort, vec![], Type::Void)
            }
            (Type::Array(elem), "sort_by") => {
                if args.len() != 1 {
                    return err("`sort_by` takes one function: `fn(a, b) => <a comes before b>`", pos);
                }
                let fty = Type::Func(vec![(*elem).clone(), (*elem).clone()], Box::new(Type::Bool));
                let f = self.expr_to(cx, &args[0], &fty)?;
                done(place, Mutation::SortBy, vec![f], Type::Void)
            }
            (Type::Array(elem), "push") => {
                if args.len() != 1 {
                    return err("`push` takes exactly 1 argument", pos);
                }
                let v = self.expr_to(cx, &args[0], &elem)?;
                done(place, Mutation::Push, vec![v], Type::Void)
            }
            (Type::Array(elem), "pop") => {
                if !args.is_empty() {
                    return err("`pop` takes no arguments", pos);
                }
                done(place, Mutation::Pop, vec![], Type::Optional(elem))
            }
            (other, _) => err(format!("`{}` has no method `{}`", other, name), pos),
        }
    }

    /// Methods that read arrays, maps and strings.
    fn value_method(&self, cx: &mut Ctx, recv_t: TExpr, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        if recv_t.ty == Type::Str {
            return self.str_method(cx, recv_t, name, args, pos);
        }
        let arity = |n: usize| -> Result<()> {
            if args.len() != n { err(format!("`{}` takes {} argument(s)", name, n), pos) } else { Ok(()) }
        };
        let t = recv_t.ty.clone();
        let (op, args, ty) = match (&t, name) {
            (Type::Array(_), "len") => (arity(0).map(|_| BuiltinMethod::ArrayLen)?, vec![], Type::Int),
            (Type::Array(elem), "map") => {
                arity(1)?;
                let hint = Type::Func(vec![(**elem).clone()], Box::new(Type::Unknown));
                let f = self.expr(cx, &args[0], Some(&hint))?;
                let out = match &f.ty {
                    Type::Func(ps, r) if ps.len() == 1 && self.m.assignable(elem, &ps[0]) && **r != Type::Void => {
                        (**r).clone()
                    }
                    _ => {
                        return err(
                            format!("`map` needs a function `fn({}) -> T`, found `{}`", elem, f.ty),
                            args[0].pos,
                        );
                    }
                };
                (BuiltinMethod::ArrayMap, vec![f], Type::Array(Box::new(out)))
            }
            (Type::Array(elem), "filter" | "find") => {
                arity(1)?;
                let fty = Type::Func(vec![(**elem).clone()], Box::new(Type::Bool));
                let f = self.expr_to(cx, &args[0], &fty)?;
                if name == "filter" {
                    (BuiltinMethod::Filter, vec![f], t.clone())
                } else {
                    (BuiltinMethod::Find, vec![f], Type::Optional(elem.clone()))
                }
            }
            (Type::Array(elem), "contains") => {
                arity(1)?;
                if !self.m.is_plain(elem) {
                    return err("`contains` only works on arrays of simple values", pos);
                }
                (BuiltinMethod::Contains, vec![self.expr_to(cx, &args[0], elem)?], Type::Bool)
            }
            (Type::Map(..), "len") => (arity(0).map(|_| BuiltinMethod::MapLen)?, vec![], Type::Int),
            (Type::Map(k, v), "get") => {
                arity(1)?;
                (BuiltinMethod::MapGet, vec![self.expr_to(cx, &args[0], k)?], Type::Optional(v.clone()))
            }
            (Type::Map(k, _), "has") => {
                arity(1)?;
                (BuiltinMethod::MapHas, vec![self.expr_to(cx, &args[0], k)?], Type::Bool)
            }
            (Type::Map(k, _), "keys") => (arity(0).map(|_| BuiltinMethod::Keys)?, vec![], Type::Array(k.clone())),
            (Type::Map(_, v), "values") => (arity(0).map(|_| BuiltinMethod::Values)?, vec![], Type::Array(v.clone())),
            (Type::Array(elem), "join") => {
                arity(1)?;
                if **elem != Type::Str {
                    return err(format!("`join` works on arrays of `str`, not `{}`", t), pos);
                }
                (BuiltinMethod::ArrayJoin, vec![self.expr_to(cx, &args[0], &Type::Str)?], Type::Str)
            }
            _ => return err(format!("`{}` has no method `{}`", t, name), pos),
        };
        Ok(texpr(TExprKind::BuiltinMethod { op, recv: Box::new(recv_t), args }, ty, pos))
    }

    fn str_method(&self, cx: &mut Ctx, recv_t: TExpr, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let arity = |n: usize| -> Result<()> {
            if args.len() != n { err(format!("`{}` takes {} argument(s)", name, n), pos) } else { Ok(()) }
        };
        let (op, args, ty) = match name {
            "len" => (arity(0).map(|_| BuiltinMethod::StrLen)?, vec![], Type::Int),
            "upper" => (arity(0).map(|_| BuiltinMethod::Upper)?, vec![], Type::Str),
            "lower" => (arity(0).map(|_| BuiltinMethod::Lower)?, vec![], Type::Str),
            "trim" => (arity(0).map(|_| BuiltinMethod::Trim)?, vec![], Type::Str),
            "contains" => {
                arity(1)?;
                (BuiltinMethod::StrContains, vec![self.expr_to(cx, &args[0], &Type::Str)?], Type::Bool)
            }
            "split" => {
                arity(1)?;
                let sep = self.expr_to(cx, &args[0], &Type::Str)?;
                (BuiltinMethod::Split, vec![sep], Type::Array(Box::new(Type::Str)))
            }
            "replace" => {
                arity(2)?;
                let from = self.expr_to(cx, &args[0], &Type::Str)?;
                let to = self.expr_to(cx, &args[1], &Type::Str)?;
                (BuiltinMethod::Replace, vec![from, to], Type::Str)
            }
            "starts_with" | "ends_with" => {
                arity(1)?;
                let op = if name == "starts_with" { BuiltinMethod::StartsWith } else { BuiltinMethod::EndsWith };
                (op, vec![self.expr_to(cx, &args[0], &Type::Str)?], Type::Bool)
            }
            "find" => {
                arity(1)?;
                (
                    BuiltinMethod::StrFind,
                    vec![self.expr_to(cx, &args[0], &Type::Str)?],
                    Type::Optional(Box::new(Type::Int)),
                )
            }
            "code_at" => {
                arity(1)?;
                (BuiltinMethod::CodeAt, vec![self.expr_to(cx, &args[0], &Type::Int)?], Type::Int)
            }
            "chars" => (arity(0).map(|_| BuiltinMethod::Chars)?, vec![], Type::Array(Box::new(Type::Str))),
            "parse_int" => (arity(0).map(|_| BuiltinMethod::ParseInt)?, vec![], Type::Optional(Box::new(Type::Int))),
            "parse_float" => {
                (arity(0).map(|_| BuiltinMethod::ParseFloat)?, vec![], Type::Optional(Box::new(Type::Float)))
            }
            _ => return err(format!("`str` has no method `{}`", name), pos),
        };
        Ok(texpr(TExprKind::BuiltinMethod { op, recv: Box::new(recv_t), args }, ty, pos))
    }
}
