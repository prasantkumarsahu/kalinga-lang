//! Lambdas: parameter and result types (written, or taken from the expected type), the
//! outer variables they capture, and lambdas that run on other threads.

use std::cell::RefCell;
use std::collections::HashMap;

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, LambdaFrame, always_returns, texpr};
use crate::model::at_pos;
use crate::tast::*;
use crate::types::Type;

impl Checker<'_> {
    pub(super) fn lambda(
        &self,
        cx: &mut Ctx,
        params: &[(String, Option<TypeExpr>, Span)],
        ret: Option<&TypeExpr>,
        body: &LambdaBody,
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let l = self.lambda_parts(cx, params, ret, body, expected, pos)?;
        let ty = Type::Func(l.params.iter().map(|(_, t)| t.clone()).collect(), Box::new(l.ret.clone()));
        Ok(texpr(TExprKind::Lambda(Box::new(l)), ty, pos))
    }

    /// A lambda that may run on another thread: every captured variable, parameter and the
    /// result must be able to move between threads. `params` are the parameter types the
    /// caller provides.
    pub(super) fn thread_lambda(&self, cx: &mut Ctx, e: &Expr, params: Vec<Type>, what: &str) -> Result<TExpr> {
        let ExprKind::Lambda { params: ps, ret, body } = &e.kind else {
            return err(format!("`{}` needs a lambda written right here, e.g. `{}(fn() => ...)`", what, what), e.pos);
        };
        let hint = Type::Func(params, Box::new(Type::Unknown));
        let l = self.lambda(cx, ps, ret.as_ref(), body, Some(&hint), e.pos)?;
        let TExprKind::Lambda(parts) = &l.kind else { return Ok(l) };
        for (name, t) in &parts.captures {
            if cx.peek(name).is_some_and(|v| v.shared) {
                return err(
                    format!(
                        "`{}` cannot use `{}` here: it is changed by a lambda, so it cannot move to another thread (use a `Shared<{}>`)",
                        what, name, t
                    ),
                    e.pos,
                );
            }
            if !self.m.is_sendable(t) {
                return err(
                    format!(
                        "`{}` cannot use `{}` here: a `{}` cannot move to another thread (pass plain values, a `Channel` or a `Shared`)",
                        what, name, t
                    ),
                    e.pos,
                );
            }
        }
        for t in parts.params.iter().map(|(_, t)| t).chain(std::iter::once(&parts.ret)) {
            if !self.m.is_sendable(t) {
                return err(
                    format!("`{}` works with values that can move between threads; `{}` cannot", what, t),
                    e.pos,
                );
            }
        }
        Ok(l)
    }

    fn lambda_parts(
        &self,
        cx: &mut Ctx,
        params: &[(String, Option<TypeExpr>, Span)],
        ret: Option<&TypeExpr>,
        body: &LambdaBody,
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TLambda> {
        let hint = match expected {
            Some(Type::Func(ps, r)) if ps.len() == params.len() => Some((ps.clone(), (**r).clone())),
            _ => None,
        };
        let mut ptypes = Vec::new();
        for (i, (name, ty, ppos)) in params.iter().enumerate() {
            let t = match (ty, &hint) {
                (Some(te), _) => self.m.resolve(te, &cx.tparams).map_err(|e| at_pos(e, *ppos))?,
                (None, Some((ps, _))) if ps[i] != Type::Unknown => ps[i].clone(),
                _ => {
                    return err(
                        format!("cannot tell the type of `{}`; write it, e.g. `fn({}: int) => ...`", name, name),
                        *ppos,
                    );
                }
            };
            ptypes.push(t);
        }
        let declared_ret = match ret {
            Some(te) => Some(self.m.resolve(te, &cx.tparams).map_err(|e| at_pos(e, pos))?),
            None => hint.as_ref().map(|(_, r)| r.clone()).filter(|r| *r != Type::Unknown),
        };

        // The lambda body gets its own scope; outer variables become read-only captures.
        cx.lambdas.push(LambdaFrame { boundary: cx.scopes.len(), captures: RefCell::new(vec![]) });
        cx.scopes.push(HashMap::new());
        let saved = (cx.ret.clone(), cx.loops, cx.in_init);
        cx.loops = 0;
        cx.in_init = false;
        let result = self.lambda_body(cx, params, &ptypes, declared_ret, body, pos);
        (cx.ret, cx.loops, cx.in_init) = saved;
        cx.scopes.pop();
        let captures = cx.lambdas.pop().map(|f| f.captures.into_inner()).unwrap_or_default();
        let (body, ret) = result?;
        let params = params.iter().map(|(n, _, _)| n.clone()).zip(ptypes).collect();
        Ok(TLambda { params, captures, body, ret })
    }

    fn lambda_body(
        &self,
        cx: &mut Ctx,
        params: &[(String, Option<TypeExpr>, Span)],
        ptypes: &[Type],
        declared_ret: Option<Type>,
        body: &LambdaBody,
        pos: Span,
    ) -> Result<(TLambdaBody, Type)> {
        for ((name, _, ppos), t) in params.iter().zip(ptypes) {
            self.declare(cx, name, t.clone(), false, *ppos)?;
        }
        match body {
            LambdaBody::Expr(e) => {
                let v = match &declared_ret {
                    Some(r) => self.expr_to(cx, e, r)?,
                    None => self.expr(cx, e, None)?,
                };
                if v.ty == Type::NoneLit {
                    return err("cannot tell the result type; write it, e.g. `fn(x: int) -> int? => none`", e.pos);
                }
                let ty = v.ty.clone();
                Ok((TLambdaBody::Expr(Box::new(v)), ty))
            }
            LambdaBody::Block(stmts) => {
                let r = declared_ret.unwrap_or(Type::Void);
                cx.ret = r.clone();
                if r != Type::Void && !always_returns(stmts) {
                    return err(format!("this lambda must return a `{}` on every path", r), pos);
                }
                Ok((TLambdaBody::Block(self.stmts(cx, stmts)?), r))
            }
        }
    }
}
