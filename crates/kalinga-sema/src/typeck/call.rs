//! Calls by name: functions, constructors, `Ok` / `Err`, `Channel` / `Shared`, built-in
//! functions, static methods and enum variants, with type-argument inference.

use std::collections::HashMap;

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, texpr};
use crate::model::{BUILTINS, at_pos};
use crate::tast::*;
use crate::types::{Signature, Type, has_params, subst, unify};

/// A resolved generic call: the type arguments learned and the converted arguments.
struct Inferred {
    targs: Vec<Type>,
    args: Vec<TExpr>,
    ret: Type,
}

impl Checker<'_> {
    pub(super) fn call(
        &self,
        cx: &mut Ctx,
        name: &str,
        targs: &[TypeExpr],
        args: &[Expr],
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let shadowed = cx.lookup(name).is_some();
        if (name == "Channel" || name == "Shared") && !shadowed {
            return self.channel_or_shared(cx, name, targs, args, expected, pos);
        }
        if (name == "Ok" || name == "Err") && !shadowed {
            return self.ok_or_err(cx, name, args, expected, pos);
        }
        let generic_class = self.m.classes.get(name).is_some_and(|c| !c.tparams.is_empty());
        let generic_fn = self.m.fn_tparams.get(name).is_some_and(|t| !t.is_empty());
        let generic_struct = self.m.structs.get(name).is_some_and(|s| !s.tparams.is_empty());
        if !targs.is_empty() && !generic_class && !generic_fn && !generic_struct {
            return err(format!("`{}` is not generic and takes no type arguments", name), pos);
        }
        if generic_class && !shadowed {
            let tparams = self.m.classes[name].tparams.clone();
            let init = self.m.find_init(name).map(|(_, s)| s.params).unwrap_or_default();
            let sig = Signature { params: init, ret: self.class_type(name) };
            let inf = self.infer_call(cx, &tparams, targs, &sig, args, expected, pos, name)?;
            let callee = Callee::New { class: name.to_string(), targs: inf.targs };
            return Ok(texpr(TExprKind::Call { callee, args: inf.args }, inf.ret, pos));
        }
        if generic_fn && !shadowed {
            let tparams = self.m.fn_tparams[name].clone();
            let sig = self.m.functions[name].clone();
            let inf = self.infer_call(cx, &tparams, targs, &sig, args, expected, pos, name)?;
            let ret = if self.m.async_fns.contains(name) { Type::Async(Box::new(inf.ret)) } else { inf.ret };
            let callee = Callee::Function { name: name.to_string(), targs: inf.targs };
            return Ok(texpr(TExprKind::Call { callee, args: inf.args }, ret, pos));
        }
        if let Some(v) = cx.lookup(name) {
            return match v.ty.clone() {
                Type::Func(ps, r) => {
                    let f = texpr(TExprKind::Local(name.to_string()), v.ty, pos);
                    self.call_value(cx, f, &ps, &r, args, pos, name)
                }
                other => err(format!("`{}` is a `{}`, not a function", name, other), pos),
            };
        }
        if BUILTINS.contains(&name) {
            return self.builtin(cx, name, args, pos);
        }
        if self.m.structs.contains_key(name) {
            return self.struct_new(cx, name, targs, args, expected, pos);
        }
        if self.m.classes.contains_key(name) {
            return self.class_new(cx, name, args, pos);
        }
        if self.m.interfaces.contains_key(name) {
            return err(format!("`{}` is an interface and cannot be created directly", name), pos);
        }
        if let Some((sig, path)) = self.m.extern_fns.get(name) {
            let args = self.args(cx, sig, args, pos, name)?;
            let callee = Callee::Extern { path: path.clone() };
            return Ok(texpr(TExprKind::Call { callee, args }, sig.ret.clone(), pos));
        }
        let Some(sig) = self.m.functions.get(name).cloned() else {
            return err(format!("unknown function `{}`", name), pos);
        };
        let args = self.args(cx, &sig, args, pos, name)?;
        let ret = if self.m.async_fns.contains(name) { Type::Async(Box::new(sig.ret)) } else { sig.ret };
        let callee = Callee::Function { name: name.to_string(), targs: vec![] };
        Ok(texpr(TExprKind::Call { callee, args }, ret, pos))
    }

    fn channel_or_shared(
        &self,
        cx: &mut Ctx,
        name: &str,
        targs: &[TypeExpr],
        args: &[Expr],
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let explicit = match targs {
            [] => None,
            [t] => Some(self.m.resolve(t, &cx.tparams).map_err(|e| at_pos(e, pos))?),
            _ => return err(format!("`{}` takes one type argument", name), pos),
        };
        let intrinsic = |op, args, ty| Ok(texpr(TExprKind::Builtin { op, args }, ty, pos));
        if name == "Channel" {
            if !args.is_empty() {
                return err("`Channel<T>()` takes no arguments", pos);
            }
            let elem = match (explicit, expected) {
                (Some(t), _) => t,
                (None, Some(Type::Channel(t))) => (**t).clone(),
                _ => return err("cannot tell what the channel carries; write `Channel<int>()`", pos),
            };
            if !self.m.is_sendable(&elem) {
                return err(
                    format!("a channel can only carry values that can move between threads, not `{}`", elem),
                    pos,
                );
            }
            return intrinsic(Builtin::NewChannel, vec![], Type::Channel(Box::new(elem)));
        }
        if args.len() != 1 {
            return err("`Shared(value)` takes the starting value", pos);
        }
        let hint = explicit.or_else(|| match expected {
            Some(Type::Shared(t)) => Some((**t).clone()),
            _ => None,
        });
        let value = match hint {
            Some(t) => self.expr_to(cx, &args[0], &t)?,
            None => self.expr(cx, &args[0], None)?,
        };
        if !self.m.is_sendable(&value.ty) {
            return err(
                format!("`Shared` can only hold values that can move between threads, not `{}`", value.ty),
                pos,
            );
        }
        let ty = Type::Shared(Box::new(value.ty.clone()));
        intrinsic(Builtin::NewShared, vec![value], ty)
    }

    fn ok_or_err(&self, cx: &mut Ctx, name: &str, args: &[Expr], expected: Option<&Type>, pos: Span) -> Result<TExpr> {
        let rt = match (expected, &cx.ret) {
            (Some(t @ Type::Result(..)), _) => t.clone(),
            (None, t @ Type::Result(..)) => t.clone(),
            _ => {
                return err(
                    format!(
                        "cannot tell which `Result` this `{}` belongs to; give the variable a type, e.g. `let r: Result<int, str> = {}(...)`",
                        name, name
                    ),
                    pos,
                );
            }
        };
        let Type::Result(t, e) = &rt else { return err("expected a `Result`", pos) };
        let payload = if name == "Ok" { (**t).clone() } else { (**e).clone() };
        let callee = if name == "Ok" { Callee::Ok } else { Callee::Err };
        if payload == Type::Void {
            if !args.is_empty() {
                return err("`Ok()` of a `Result<void, E>` takes no value", pos);
            }
            return Ok(texpr(TExprKind::Call { callee, args: vec![] }, rt, pos));
        }
        if args.len() != 1 {
            return err(format!("`{}` takes exactly one value", name), pos);
        }
        let v = self.expr_to(cx, &args[0], &payload)?;
        Ok(texpr(TExprKind::Call { callee, args: vec![v] }, rt, pos))
    }

    /// `Point(1, 2)`, or `Grid<int>(...)` for a generic struct: every field gets a value, in
    /// order; trailing fields with defaults may be left out.
    fn struct_new(
        &self,
        cx: &mut Ctx,
        name: &str,
        targs: &[TypeExpr],
        args: &[Expr],
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let info = self.m.structs[name].clone();
        let (min, max) = (info.required_args(), info.fields.len());
        if args.len() < min || args.len() > max {
            let generic = !info.tparams.is_empty();
            let range = if min == max && !generic { format!("{}", max) } else { format!("{} to {}", min, max) };
            return err(
                format!(
                    "`{}(...)` takes {} argument(s) (its fields in order) but {} were given",
                    name,
                    range,
                    args.len()
                ),
                pos,
            );
        }
        let (bound, mut given, ty) = if info.tparams.is_empty() {
            let mut given = vec![];
            for (a, (_, t)) in args.iter().zip(&info.fields) {
                given.push(self.expr_to(cx, a, t)?);
            }
            (vec![], given, Type::Struct(name.to_string(), vec![]))
        } else {
            let sig = Signature { params: info.fields[..args.len()].to_vec(), ret: self.struct_self_type(name) };
            let inf = self.infer_call(cx, &info.tparams, targs, &sig, args, expected, pos, name)?;
            (inf.targs, inf.args, inf.ret)
        };
        // Fields left out take their declared defaults.
        let map = self.m.struct_bindings(name, &bound);
        for i in given.len()..info.fields.len() {
            let (_, t) = &info.fields[i];
            let Some(default) = &info.decl.fields[i].default else { break };
            let mut dcx = Ctx::new(None, Type::Void, false);
            given.push(self.expr_to(&mut dcx, default, &subst(t, &map))?);
        }
        let callee = Callee::StructNew { strukt: name.to_string(), targs: bound };
        Ok(texpr(TExprKind::Call { callee, args: given }, ty, pos))
    }

    fn class_new(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        if self.m.classes[name].decl.is_abstract {
            return err(format!("`{}` is abstract and cannot be created; create one of its subclasses", name), pos);
        }
        let params = self.m.find_init(name).map(|(_, s)| s.params).unwrap_or_default();
        if args.len() != params.len() {
            return err(
                format!("`{}(...)` takes {} argument(s) but {} were given", name, params.len(), args.len()),
                pos,
            );
        }
        let mut targs = vec![];
        for (a, (_, t)) in args.iter().zip(&params) {
            targs.push(self.expr_to(cx, a, t)?);
        }
        let callee = Callee::New { class: name.to_string(), targs: vec![] };
        Ok(texpr(TExprKind::Call { callee, args: targs }, Type::Class(name.to_string()), pos))
    }

    /// Calls a function value.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_value(
        &self,
        cx: &mut Ctx,
        f: TExpr,
        ps: &[Type],
        ret: &Type,
        args: &[Expr],
        pos: Span,
        what: &str,
    ) -> Result<TExpr> {
        let sig = Signature {
            params: ps.iter().enumerate().map(|(i, t)| (format!("p{}", i), t.clone())).collect(),
            ret: ret.clone(),
        };
        let args = self.args(cx, &sig, args, pos, what)?;
        Ok(texpr(TExprKind::Call { callee: Callee::Value(Box::new(f)), args }, ret.clone(), pos))
    }

    /// The arguments of a call, each converted to its parameter's type.
    pub(super) fn args(
        &self,
        cx: &mut Ctx,
        sig: &Signature,
        args: &[Expr],
        pos: Span,
        what: &str,
    ) -> Result<Vec<TExpr>> {
        if args.len() != sig.params.len() {
            return err(
                format!("`{}` takes {} argument(s) but {} were given", what, sig.params.len(), args.len()),
                pos,
            );
        }
        args.iter().zip(&sig.params).map(|(a, (_, t))| self.expr_to(cx, a, t)).collect()
    }

    /// Checks the arguments of a call to something generic, learning its type parameters
    /// from explicit type arguments, the expected type and the arguments.
    #[allow(clippy::too_many_arguments)]
    fn infer_call(
        &self,
        cx: &mut Ctx,
        tparams: &[String],
        targs: &[TypeExpr],
        sig: &Signature,
        args: &[Expr],
        expected: Option<&Type>,
        pos: Span,
        what: &str,
    ) -> Result<Inferred> {
        if args.len() != sig.params.len() {
            return err(
                format!("`{}` takes {} argument(s) but {} were given", what, sig.params.len(), args.len()),
                pos,
            );
        }
        let mut map: HashMap<String, Type> = HashMap::new();
        if !targs.is_empty() {
            if targs.len() != tparams.len() {
                return err(format!("`{}` takes {} type argument(s), not {}", what, tparams.len(), targs.len()), pos);
            }
            for (tp, te) in tparams.iter().zip(targs) {
                map.insert(tp.clone(), self.m.resolve(te, &cx.tparams).map_err(|e| at_pos(e, pos))?);
            }
        } else if let Some(exp) = expected {
            unify(&sig.ret, exp, &mut map);
        }
        let mut checked: Vec<Option<TExpr>> = vec![None; args.len()];
        // Plain arguments first, so lambdas can then get full parameter types.
        for pass in 0..2 {
            for (i, a) in args.iter().enumerate() {
                let is_lambda = matches!(a.kind, ExprKind::Lambda { .. });
                if (pass == 0) == is_lambda {
                    continue;
                }
                let pt = subst(&sig.params[i].1, &map);
                let t = if !has_params(&pt) {
                    self.expr_to(cx, a, &pt)?
                } else if is_lambda {
                    self.expr(cx, a, Some(&unknown_params(&pt)))?
                } else {
                    self.expr(cx, a, None)?
                };
                unify(&sig.params[i].1, &t.ty, &mut map);
                checked[i] = Some(t);
            }
        }
        for tp in tparams {
            if !map.contains_key(tp) {
                return err(
                    format!("cannot tell what `{}` is for `{}`; write it, e.g. `{}<int>(...)`", tp, what, what),
                    pos,
                );
            }
        }
        let mut out = vec![];
        for (i, t) in checked.into_iter().enumerate() {
            let Some(t) = t else { continue };
            out.push(self.convert(t, &subst(&sig.params[i].1, &map))?);
        }
        let targs = tparams.iter().map(|t| map[t].clone()).collect();
        Ok(Inferred { targs, args: out, ret: subst(&sig.ret, &map) })
    }

    fn builtin(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let intrinsic = |op, args: Vec<TExpr>, ty| Ok(texpr(TExprKind::Builtin { op, args }, ty, pos));
        let one = |cx: &mut Ctx| -> Result<TExpr> {
            if args.len() != 1 {
                return err(format!("`{}` takes exactly 1 argument", name), pos);
            }
            self.expr(cx, &args[0], None)
        };
        match name {
            "print" => {
                let mut parts = vec![];
                for a in args {
                    let t = self.expr(cx, a, None)?;
                    if !self.m.is_plain(&t.ty) {
                        return err(
                            format!("cannot print a `{}` value (print its fields or a method result instead)", t.ty),
                            a.pos,
                        );
                    }
                    parts.push(t);
                }
                intrinsic(Builtin::Print, parts, Type::Void)
            }
            "len" => {
                if args.len() != 1 {
                    return err("`len` takes exactly 1 argument", pos);
                }
                let v = self.expr(cx, &args[0], None)?;
                match &v.ty {
                    Type::Array(_) | Type::Map(..) | Type::Str => intrinsic(Builtin::Len, vec![v], Type::Int),
                    other => err(format!("`len` needs an array, map or string, found `{}`", other), pos),
                }
            }
            "str" => {
                let v = one(cx)?;
                if !self.m.is_plain(&v.ty) {
                    return err(format!("cannot convert `{}` to str", v.ty), pos);
                }
                intrinsic(Builtin::ToStr, vec![v], Type::Str)
            }
            "int" => {
                let v = one(cx)?;
                if !matches!(v.ty, Type::Int | Type::Float | Type::Bool | Type::Str) {
                    return err(format!("cannot convert `{}` to int", v.ty), pos);
                }
                intrinsic(Builtin::ToInt, vec![v], Type::Int)
            }
            "float" => {
                let v = one(cx)?;
                if !matches!(v.ty, Type::Int | Type::Float | Type::Str) {
                    return err(format!("cannot convert `{}` to float", v.ty), pos);
                }
                intrinsic(Builtin::ToFloat, vec![v], Type::Float)
            }
            "input" => {
                let prompt = match args.len() {
                    0 => vec![],
                    1 => vec![self.expr_to(cx, &args[0], &Type::Str)?],
                    _ => return err("`input` takes an optional prompt string", pos),
                };
                intrinsic(Builtin::Input, prompt, Type::Str)
            }
            _ => err(format!("unknown function `{}`", name), pos),
        }
    }

    /// `Class.method(args)` for a `static fn`.
    pub(super) fn static_call(&self, cx: &mut Ctx, class: &str, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let (owner, sig) = match self.m.find_static(class, name) {
            Some(x) => x,
            None if self.m.find_method(class, name).is_some() => {
                return err(format!("`{}` is not a `static fn`; call it on a `{}` object", name, class), pos);
            }
            None => return err(format!("`{}` has no static method `{}`", class, name), pos),
        };
        self.check_method_access(cx, &owner, name, pos)?;
        let args = self.args(cx, &sig, args, pos, name)?;
        let ret =
            if self.m.classes[&owner].async_statics.contains(name) { Type::Async(Box::new(sig.ret)) } else { sig.ret };
        let callee = Callee::Static { owner, name: name.to_string(), targs: vec![] };
        Ok(texpr(TExprKind::Call { callee, args }, ret, pos))
    }

    /// `Struct.method(args)` for a struct's `static fn`.
    pub(super) fn struct_static_call(
        &self,
        cx: &mut Ctx,
        strukt: &str,
        name: &str,
        args: &[Expr],
        pos: Span,
    ) -> Result<TExpr> {
        let m = match self.m.structs[strukt].methods.get(name) {
            Some(m) if m.is_static => m.clone(),
            Some(_) => return err(format!("`{}` is not a `static fn`; call it on a `{}` value", name, strukt), pos),
            None => return err(format!("`{}` has no static method `{}`", strukt, name), pos),
        };
        if m.is_private && cx.strukt.as_deref() != Some(strukt) {
            return err(format!("method `{}` is private to `{}`", name, strukt), pos);
        }
        let tparams = self.m.structs[strukt].tparams.clone();
        let (targs, args, ret) = if tparams.is_empty() {
            (vec![], self.args(cx, &m.sig, args, pos, name)?, m.sig.ret)
        } else {
            let inf = self.infer_call(cx, &tparams, &[], &m.sig, args, None, pos, name)?;
            (inf.targs, inf.args, inf.ret)
        };
        let ret = if m.is_async { Type::Async(Box::new(ret)) } else { ret };
        let callee = Callee::Static { owner: strukt.to_string(), name: name.to_string(), targs };
        Ok(texpr(TExprKind::Call { callee, args }, ret, pos))
    }

    /// `Shape.Circle(args)` or `Shape.Empty`, when `type_name` is an enum.
    pub(super) fn variant_value(
        &self,
        cx: &mut Ctx,
        type_name: &str,
        variant: &str,
        args: Option<&[Expr]>,
        pos: Span,
    ) -> Result<TExpr> {
        let Some(fields) = self.m.enums[type_name].variant(variant).cloned() else {
            return err(format!("enum `{}` has no variant `{}`", type_name, variant), pos);
        };
        let ty = Type::Enum(type_name.to_string());
        let args = match args {
            None if fields.is_empty() => vec![],
            None => {
                return err(
                    format!(
                        "`{}.{}` needs {} value(s): `{}.{}(...)`",
                        type_name,
                        variant,
                        fields.len(),
                        type_name,
                        variant
                    ),
                    pos,
                );
            }
            Some(_) if fields.is_empty() => {
                return err(format!("`{}.{}` takes no values; write it without `()`", type_name, variant), pos);
            }
            Some(args) => self.args(cx, &Signature { params: fields, ret: Type::Void }, args, pos, variant)?,
        };
        let kind = TExprKind::Variant { enum_name: type_name.to_string(), variant: variant.to_string(), args };
        Ok(texpr(kind, ty, pos))
    }

    pub(super) fn check_method_access(&self, cx: &Ctx, owner: &str, method: &str, pos: Span) -> Result<()> {
        if self.m.classes[owner].private_methods.contains(method) && cx.class.as_deref() != Some(owner) {
            return err(format!("method `{}` is private to `{}`", method, owner), pos);
        }
        Ok(())
    }
}

/// Replaces type parameters with `Unknown`, for use as an expected-type hint.
fn unknown_params(t: &Type) -> Type {
    match t {
        Type::Param(_) => Type::Unknown,
        Type::Array(i) => Type::Array(Box::new(unknown_params(i))),
        Type::Optional(i) => Type::Optional(Box::new(unknown_params(i))),
        Type::Map(k, v) => Type::Map(Box::new(unknown_params(k)), Box::new(unknown_params(v))),
        Type::Func(ps, r) => Type::Func(ps.iter().map(unknown_params).collect(), Box::new(unknown_params(r))),
        Type::Generic(n, args) => Type::Generic(n.clone(), args.iter().map(unknown_params).collect()),
        Type::Struct(n, args) => Type::Struct(n.clone(), args.iter().map(unknown_params).collect()),
        Type::Result(t, e) => Type::Result(Box::new(unknown_params(t)), Box::new(unknown_params(e))),
        other => other.clone(),
    }
}
