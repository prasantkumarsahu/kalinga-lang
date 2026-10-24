//! `if` and `match` used as values.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx, texpr};
use crate::tast::*;
use crate::types::Type;

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn if_expr(
        &self,
        cx: &mut Ctx,
        binding: Option<&str>,
        cond: &Expr,
        then: &[Stmt],
        otherwise: &[Stmt],
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let mut ty = expected.cloned();
        let (cond, then) = match binding {
            // `if let x = optional { ... }`: the `then` branch sees the value as `x`.
            Some(name) => {
                let (value, inner) = self.optional_value(cx, cond, "if let")?;
                let then = cx.scoped(|cx| {
                    self.declare(cx, name, inner, false, cond.pos)?;
                    self.value_block(cx, then, &mut ty)
                })?;
                (value, then)
            }
            None => {
                let cond = self.cond(cx, cond)?;
                (cond, self.value_block(cx, then, &mut ty)?)
            }
        };
        let otherwise = self.value_block(cx, otherwise, &mut ty)?;
        let mut branches = [then, otherwise];
        let ty = self.branch_type(&mut branches, ty, pos)?;
        let [then, otherwise] = branches;
        let binding = binding.map(str::to_string);
        Ok(texpr(TExprKind::If { binding, cond: Box::new(cond), then, otherwise }, ty, pos))
    }

    pub(super) fn match_expr(
        &self,
        cx: &mut Ctx,
        value: &Expr,
        arms: &[MatchArm],
        expected: Option<&Type>,
        pos: Span,
    ) -> Result<TExpr> {
        let mut ty = expected.cloned();
        let (value, arms) =
            self.match_arms(cx, value, arms, pos, |cx, arm| self.value_block(cx, &arm.body, &mut ty))?;
        let (patterns, mut blocks): (Vec<TPattern>, Vec<TValueBlock>) = arms.into_iter().unzip();
        let ty = self.branch_type(&mut blocks, ty, pos)?;
        let arms = patterns.into_iter().zip(blocks).collect();
        Ok(texpr(TExprKind::Match { value: Box::new(value), arms }, ty, pos))
    }

    /// A branch: its statements in their own scope, then its value. The first branch with a
    /// value other than `none` decides `ty` when nothing else did.
    fn value_block(&self, cx: &mut Ctx, stmts: &[Stmt], ty: &mut Option<Type>) -> Result<TValueBlock> {
        let Some((last, init)) = stmts.split_last() else {
            return err("a branch used as a value needs a value", Span::default());
        };
        let StmtKind::Expr(e) = &last.kind else {
            return err("this branch must end with its value", last.pos);
        };
        cx.scoped(|cx| {
            let stmts = self.stmts(cx, init)?;
            let value = self.expr(cx, e, ty.as_ref())?;
            if value.ty == Type::Void {
                return err("this branch gives no value", e.pos);
            }
            if ty.is_none() && value.ty != Type::NoneLit {
                *ty = Some(value.ty.clone());
            }
            Ok(TValueBlock { stmts, value: Box::new(value) })
        })
    }

    /// The type all branches give, converting each value to it; `none` in a branch makes
    /// it optional.
    fn branch_type(&self, branches: &mut [TValueBlock], ty: Option<Type>, pos: Span) -> Result<Type> {
        let Some(mut ty) = ty else {
            return err("cannot tell the type of this value: every branch is `none`; add a type", pos);
        };
        if branches.iter().any(|b| b.value.ty == Type::NoneLit) && !matches!(ty, Type::Optional(_)) {
            ty = Type::Optional(Box::new(ty));
        }
        for b in branches.iter_mut() {
            let value = std::mem::replace(&mut *b.value, texpr(TExprKind::Bool(false), Type::Bool, pos));
            *b.value = self.convert(value, &ty).map_err(|mut e| {
                e.message = format!("{} (every branch must give the same type)", e.message);
                e
            })?;
        }
        Ok(ty)
    }
}
