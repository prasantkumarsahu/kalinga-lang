//! The built-in libraries: `Math`, `Random`, `Time`, `Thread` and `Async`.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, texpr};
use crate::tast::*;
use crate::types::Type;

/// The library names, which work as receivers unless a variable shadows them.
const LIBRARIES: &[&str] = &["Math", "Random", "Time", "Thread", "Async"];

impl Checker<'_> {
    /// `Math`, `Random`, ... used as a receiver.
    pub(super) fn library_receiver(&self, cx: &Ctx, recv: &Expr) -> Option<String> {
        match &recv.kind {
            ExprKind::Ident(n) if LIBRARIES.contains(&n.as_str()) && cx.lookup(n).is_none() => Some(n.clone()),
            _ => None,
        }
    }

    /// `Math.PI` and the other library constants.
    pub(super) fn library_value(&self, ns: &str, name: &str, pos: Span) -> Result<TExpr> {
        match (ns, name) {
            ("Math", "PI" | "TAU" | "E" | "INF") => Ok(texpr(TExprKind::MathConst(name.to_string()), Type::Float, pos)),
            _ => err(format!("`{}.{}` is not a value; library functions are called with `()`", ns, name), pos),
        }
    }

    /// A numeric argument as a `float` (ints are converted).
    fn float_arg(&self, cx: &mut Ctx, e: &Expr, what: &str) -> Result<TExpr> {
        let v = self.expr(cx, e, None)?;
        match v.ty {
            Type::Float => Ok(v),
            Type::Int => Ok(texpr(TExprKind::Builtin { op: Builtin::ToFloat, args: vec![v] }, Type::Float, e.pos)),
            t => err(format!("`{}` needs a number, found `{}`", what, t), e.pos),
        }
    }

    /// Arguments of the same numeric type (`int` or `float`) for min / max / clamp / abs / sign.
    fn same_number_args(&self, cx: &mut Ctx, args: &[Expr], what: &str) -> Result<(Vec<TExpr>, Type)> {
        let mut out = vec![];
        let mut ty: Option<Type> = None;
        for a in args {
            let v = self.expr(cx, a, None)?;
            if !matches!(v.ty, Type::Int | Type::Float) {
                return err(format!("`{}` needs numbers, found `{}`", what, v.ty), a.pos);
            }
            if let Some(prev) = &ty
                && *prev != v.ty
            {
                return err(
                    format!("`{}` needs all `int` or all `float` arguments; convert with `float(x)`", what),
                    a.pos,
                );
            }
            ty = Some(v.ty.clone());
            out.push(v);
        }
        Ok((out, ty.unwrap_or(Type::Float)))
    }

    pub(super) fn library_call(&self, cx: &mut Ctx, ns: &str, name: &str, args: &[Expr], pos: Span) -> Result<TExpr> {
        let what = format!("{}.{}", ns, name);
        let arity = |n: usize| -> Result<()> {
            if args.len() == n {
                Ok(())
            } else {
                err(format!("`{}` takes {} argument(s), but {} were given", what, n, args.len()), pos)
            }
        };
        let done = |op, args, ty| Ok(texpr(TExprKind::Builtin { op, args }, ty, pos));
        match (ns, name) {
            (
                "Math",
                "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "ln" | "log10" | "floor" | "ceil"
                | "round",
            ) => {
                arity(1)?;
                let x = self.float_arg(cx, &args[0], &what)?;
                done(Builtin::MathUnary(name.to_string()), vec![x], Type::Float)
            }
            ("Math", "pow" | "atan2" | "hypot") => {
                arity(2)?;
                let a = self.float_arg(cx, &args[0], &what)?;
                let b = self.float_arg(cx, &args[1], &what)?;
                let f = if name == "pow" { "powf" } else { name };
                done(Builtin::MathBinary(f.to_string()), vec![a, b], Type::Float)
            }
            ("Math", "lerp") => {
                arity(3)?;
                let a = self.float_arg(cx, &args[0], &what)?;
                let b = self.float_arg(cx, &args[1], &what)?;
                let t = self.float_arg(cx, &args[2], &what)?;
                done(Builtin::Lerp, vec![a, b, t], Type::Float)
            }
            ("Math", "abs" | "sign" | "min" | "max" | "clamp") => {
                let (n, op) = match name {
                    "abs" => (1, Builtin::Abs),
                    "sign" => (1, Builtin::Sign),
                    "min" => (2, Builtin::Min),
                    "max" => (2, Builtin::Max),
                    _ => (3, Builtin::Clamp),
                };
                arity(n)?;
                let (vs, t) = self.same_number_args(cx, args, &what)?;
                done(op, vs, t)
            }
            ("Random", _) => self.random_call(cx, name, args, pos, &what),
            ("Thread", "spawn") => {
                arity(1)?;
                let f = self.thread_lambda(cx, &args[0], vec![], "Thread.spawn")?;
                let ret = match &f.ty {
                    Type::Func(_, r) => (**r).clone(),
                    other => other.clone(),
                };
                done(Builtin::Spawn, vec![f], Type::Task(Box::new(ret)))
            }
            ("Thread", "cores") => {
                arity(0)?;
                done(Builtin::Cores, vec![], Type::Int)
            }
            ("Time", "wait") => {
                arity(1)?;
                let secs = self.float_arg(cx, &args[0], &what)?;
                done(Builtin::Wait, vec![secs], Type::Async(Box::new(Type::Void)))
            }
            ("Async", "spawn" | "run") => {
                arity(1)?;
                let job = self.expr(cx, &args[0], None)?;
                let Type::Async(inner) = job.ty.clone() else {
                    return err(
                        format!("`{}` needs the result of calling an `async fn`, not `{}`", what, job.ty),
                        args[0].pos,
                    );
                };
                if name == "spawn" {
                    done(Builtin::AsyncSpawn, vec![job], Type::Async(inner))
                } else {
                    done(Builtin::AsyncRun, vec![job], *inner)
                }
            }
            ("Async", "all") => {
                arity(1)?;
                let jobs = self.expr(cx, &args[0], None)?;
                let inner = match &jobs.ty {
                    Type::Array(elem) => match &**elem {
                        Type::Async(inner) => Some((**inner).clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(inner) = inner else {
                    return err(
                        format!("`Async.all` needs an array of `Async<T>` values, not `{}`", jobs.ty),
                        args[0].pos,
                    );
                };
                done(Builtin::AsyncAll, vec![jobs], Type::Async(Box::new(Type::Array(Box::new(inner)))))
            }
            ("Time", "now") => {
                arity(0)?;
                done(Builtin::Now, vec![], Type::Float)
            }
            ("Time", "millis") => {
                arity(0)?;
                done(Builtin::Millis, vec![], Type::Int)
            }
            ("Time", "sleep") => {
                arity(1)?;
                let secs = self.float_arg(cx, &args[0], &what)?;
                done(Builtin::Sleep, vec![secs], Type::Void)
            }
            _ => err(format!("`{}` has no function `{}`", ns, name), pos),
        }
    }

    fn random_call(&self, cx: &mut Ctx, name: &str, args: &[Expr], pos: Span, what: &str) -> Result<TExpr> {
        let arity = |n: usize| -> Result<()> {
            if args.len() == n {
                Ok(())
            } else {
                err(format!("`{}` takes {} argument(s), but {} were given", what, n, args.len()), pos)
            }
        };
        let done = |op, args, ty| Ok(texpr(TExprKind::Builtin { op, args }, ty, pos));
        match name {
            "int" => {
                arity(2)?;
                let lo = self.expr_to(cx, &args[0], &Type::Int)?;
                let hi = self.expr_to(cx, &args[1], &Type::Int)?;
                done(Builtin::RandInt, vec![lo, hi], Type::Int)
            }
            "float" => {
                arity(0)?;
                done(Builtin::RandFloat, vec![], Type::Float)
            }
            "chance" => {
                arity(1)?;
                let p = self.float_arg(cx, &args[0], what)?;
                done(Builtin::Chance, vec![p], Type::Bool)
            }
            "seed" => {
                arity(1)?;
                let n = self.expr_to(cx, &args[0], &Type::Int)?;
                done(Builtin::Seed, vec![n], Type::Void)
            }
            "pick" => {
                arity(1)?;
                let xs = self.expr(cx, &args[0], None)?;
                let Type::Array(elem) = xs.ty.clone() else {
                    return err(format!("`Random.pick` needs an array, found `{}`", xs.ty), args[0].pos);
                };
                done(Builtin::Pick, vec![xs], Type::Optional(elem))
            }
            "shuffle" => {
                arity(1)?;
                let place = self.place(cx, &args[0])?;
                if !matches!(place.ty(), Type::Array(_)) {
                    return err(format!("`Random.shuffle` needs an array, found `{}`", place.ty()), args[0].pos);
                }
                Ok(texpr(TExprKind::Mutate { place, op: Mutation::Shuffle, args: vec![] }, Type::Void, pos))
            }
            _ => err(format!("`Random` has no function `{}`", name), pos),
        }
    }
}
