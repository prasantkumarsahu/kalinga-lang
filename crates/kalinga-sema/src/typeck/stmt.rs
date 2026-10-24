//! Statements: declarations, assignments, control flow and `match`.

use kalinga_syntax::ast::*;
use kalinga_syntax::error::{Result, Span, err};

use super::{Checker, Ctx};
use crate::model::EnumInfo;
use crate::suggest::unknown;
use crate::tast::*;
use crate::types::Type;

impl Checker<'_> {
    pub(super) fn stmts(&self, cx: &mut Ctx, stmts: &[Stmt]) -> Result<Vec<TStmt>> {
        stmts.iter().map(|s| self.stmt(cx, s)).collect()
    }

    /// A `{ ... }` block with its own scope.
    pub(super) fn block(&self, cx: &mut Ctx, stmts: &[Stmt]) -> Result<Vec<TStmt>> {
        cx.scoped(|cx| self.stmts(cx, stmts))
    }

    pub(super) fn cond(&self, cx: &mut Ctx, e: &Expr) -> Result<TExpr> {
        let c = self.expr(cx, e, None)?;
        if c.ty != Type::Bool {
            return err(format!("condition must be `bool`, found `{}`", c.ty), e.pos);
        }
        Ok(c)
    }

    fn stmt(&self, cx: &mut Ctx, s: &Stmt) -> Result<TStmt> {
        match &s.kind {
            StmtKind::Let { name, mutable, ty, value } => self.let_stmt(cx, name, *mutable, ty.as_ref(), value, s.pos),
            StmtKind::Assign { target, op, value } => self.assign(cx, target, *op, value),
            StmtKind::If { cond, then, otherwise } => {
                let cond = self.cond(cx, cond)?;
                let then = self.block(cx, then)?;
                let otherwise = otherwise.as_ref().map(|o| self.block(cx, o)).transpose()?;
                Ok(TStmt::If { cond, then, otherwise })
            }
            StmtKind::IfLet { name, value, then, otherwise } => {
                let (value_t, inner) = self.optional_value(cx, value, "if let")?;
                let then = cx.scoped(|cx| {
                    self.declare(cx, name, inner, false, s.pos)?;
                    self.block(cx, then)
                })?;
                let otherwise = otherwise.as_ref().map(|o| self.block(cx, o)).transpose()?;
                Ok(TStmt::IfLet { name: name.clone(), value: value_t, then, otherwise })
            }
            StmtKind::Match { value, arms } => self.match_stmt(cx, value, arms, s.pos),
            StmtKind::While { cond, body } => {
                let cond = self.cond(cx, cond)?;
                let body = self.in_loop(cx, |cx| self.block(cx, body))?;
                Ok(TStmt::While { cond, body })
            }
            StmtKind::WhileLet { name, value, body } => {
                let (value, inner) = self.optional_value(cx, value, "while let")?;
                let body = self.loop_body(cx, &[(name, inner)], body, s.pos)?;
                Ok(TStmt::WhileLet { name: name.clone(), value, body })
            }
            StmtKind::For { index, var, iter, body } => self.for_stmt(cx, index.as_deref(), var, iter, body, s.pos),
            StmtKind::Return(value) => match (value, cx.ret.clone()) {
                (None, Type::Void) => Ok(TStmt::Return(None)),
                (None, t) => err(format!("this function must return a `{}`", t), s.pos),
                (Some(v), Type::Void) => err("this function does not return a value", v.pos),
                (Some(v), t) => Ok(TStmt::Return(Some(self.expr_to(cx, v, &t)?))),
            },
            StmtKind::Break | StmtKind::Continue => {
                if cx.loops == 0 {
                    return err("`break`/`continue` outside of a loop", s.pos);
                }
                Ok(if matches!(s.kind, StmtKind::Break) { TStmt::Break } else { TStmt::Continue })
            }
            StmtKind::Expr(e) => {
                if let ExprKind::SuperInit(args) = &e.kind {
                    return self.super_init(cx, args, e.pos);
                }
                let t = self.expr(cx, e, None)?;
                if matches!(t.ty, Type::Async(_)) {
                    return err(
                        "calling an `async fn` only prepares it: put `await` in front, or start it with `Async.spawn(...)`",
                        e.pos,
                    );
                }
                Ok(TStmt::Expr(t))
            }
        }
    }

    fn let_stmt(
        &self,
        cx: &mut Ctx,
        name: &str,
        mutable: bool,
        ty: Option<&TypeExpr>,
        value: &Expr,
        pos: Span,
    ) -> Result<TStmt> {
        let value_t = match ty {
            Some(te) => {
                let t = self.m.resolve(te, &cx.tparams).map_err(|e| crate::model::at_pos(e, pos))?;
                self.expr_to(cx, value, &t)?
            }
            None => self.expr(cx, value, None)?,
        };
        match value_t.ty {
            Type::Void => return err("cannot store the result of a function that returns nothing", value.pos),
            Type::NoneLit => {
                return err("cannot infer the type of `none`; add a type, e.g. `let x: int? = none`", value.pos);
            }
            _ => {}
        }
        let shared = mutable && self.shared.borrow().contains(&pos);
        self.declare(cx, name, value_t.ty.clone(), mutable, pos)?;
        Ok(TStmt::Let { name: name.to_string(), mutable, shared, ty: value_t.ty.clone(), value: value_t })
    }

    fn in_loop<T>(&self, cx: &mut Ctx, f: impl FnOnce(&mut Ctx) -> Result<T>) -> Result<T> {
        cx.loops += 1;
        let r = f(cx);
        cx.loops -= 1;
        r
    }

    /// The value of `if let` / `while let`, which must be optional, and the type inside.
    fn optional_value(&self, cx: &mut Ctx, value: &Expr, what: &str) -> Result<(TExpr, Type)> {
        let value_t = self.expr(cx, value, None)?;
        match &value_t.ty {
            Type::Optional(i) => {
                let inner = (**i).clone();
                Ok((value_t, inner))
            }
            other => err(format!("`{}` needs an optional value (`T?`), found `{}`", what, other), value.pos),
        }
    }

    fn for_stmt(
        &self,
        cx: &mut Ctx,
        index: Option<&str>,
        var: &str,
        iter: &ForIter,
        body: &[Stmt],
        pos: Span,
    ) -> Result<TStmt> {
        match iter {
            ForIter::Range(a, b, inclusive) => {
                if index.is_some() {
                    return err("a range gives one number at a time: write `for i in a..b`", pos);
                }
                let from = self.expr_to(cx, a, &Type::Int)?;
                let to = self.expr_to(cx, b, &Type::Int)?;
                let body = self.loop_body(cx, &[(var, Type::Int)], body, pos)?;
                Ok(TStmt::ForRange { var: var.to_string(), from, to, inclusive: *inclusive, body })
            }
            ForIter::Each(e) => {
                let iter = self.expr(cx, e, None)?;
                // (type of the first name when there are two, type of `var`)
                let (first, elem) = match &iter.ty {
                    Type::Array(inner) => (Type::Int, (**inner).clone()),
                    Type::Map(k, v) if index.is_some() => ((**k).clone(), (**v).clone()),
                    Type::Map(k, _) => (Type::Int, (**k).clone()),
                    Type::Str => (Type::Int, Type::Str),
                    other => {
                        return err(
                            format!("cannot loop over `{}` (expected an array, map, string or `a..b` range)", other),
                            e.pos,
                        );
                    }
                };
                let names: Vec<(&str, Type)> = index.map(|i| (i, first)).into_iter().chain([(var, elem)]).collect();
                let body = self.loop_body(cx, &names, body, pos)?;
                Ok(TStmt::ForEach { index: index.map(str::to_string), var: var.to_string(), iter, body })
            }
        }
    }

    /// A loop's body, with the loop variables declared.
    fn loop_body(&self, cx: &mut Ctx, vars: &[(&str, Type)], body: &[Stmt], pos: Span) -> Result<Vec<TStmt>> {
        cx.scoped(|cx| {
            for (name, ty) in vars {
                self.declare(cx, name, ty.clone(), false, pos)?;
            }
            self.in_loop(cx, |cx| self.block(cx, body))
        })
    }

    fn super_init(&self, cx: &mut Ctx, args: &[Expr], pos: Span) -> Result<TStmt> {
        if !cx.in_init {
            return err("`super(...)` can only be called inside `init`", pos);
        }
        let class = cx.class.clone().unwrap_or_default();
        let parent = match &self.m.classes[&class].parent {
            Some(p) => p.clone(),
            None => return err(format!("`{}` has no base class", class), pos),
        };
        let (owner, sig) = match self.m.find_init(&parent) {
            Some(x) => x,
            None => return err(format!("base class `{}` has no `init`", parent), pos),
        };
        let args = self.args(cx, &sig, args, pos, "super")?;
        Ok(TStmt::SuperInit { owner, args })
    }

    // ------------------------------------------------------------ match

    fn match_stmt(&self, cx: &mut Ctx, value: &Expr, arms: &[MatchArm], pos: Span) -> Result<TStmt> {
        let (value, arms) = self.match_arms(cx, value, arms, pos, |cx, arm| self.block(cx, &arm.body))?;
        Ok(TStmt::Match { value, arms: arms.into_iter().map(|(pattern, body)| TArm { pattern, body }).collect() })
    }

    /// The value and patterns of a `match`, which must handle every case; `body` checks
    /// each arm's body with the pattern's bindings in scope.
    pub(super) fn match_arms<T>(
        &self,
        cx: &mut Ctx,
        value: &Expr,
        arms: &[MatchArm],
        pos: Span,
        mut body: impl FnMut(&mut Ctx, &MatchArm) -> Result<T>,
    ) -> Result<(TExpr, Vec<(TPattern, T)>)> {
        let value_t = self.expr(cx, value, None)?;
        let ty = value_t.ty.clone();
        let enum_info = match &ty {
            Type::Enum(e, args) => Some((e.clone(), self.m.enum_variants(e, args))),
            Type::Result(t, e) => Some(("Result".to_string(), result_variants(t, e))),
            Type::Int | Type::Str | Type::Bool => None,
            other => {
                return err(
                    format!("cannot `match` on `{}` (use an enum, `Result`, int, str or bool)", other),
                    value.pos,
                );
            }
        };
        let mut cover = Coverage::default();
        let mut tarms = vec![];
        for arm in arms {
            if cover.wildcard {
                return err("this arm can never run: an earlier `_` arm already matches everything", arm.pos);
            }
            let tarm = cx.scoped(|cx| {
                let pattern = self.pattern(cx, arm, &ty, enum_info.as_ref(), &mut cover)?;
                Ok((pattern, body(cx, arm)?))
            })?;
            tarms.push(tarm);
        }
        let all_variants =
            enum_info.as_ref().is_some_and(|(_, info)| info.variants.iter().all(|(v, _)| cover.variants.contains(v)));
        if !(cover.wildcard || (cover.saw_true && cover.saw_false) || all_variants) {
            let missing = match &enum_info {
                Some((_, info)) => {
                    let m: Vec<&str> = info
                        .variants
                        .iter()
                        .filter(|(v, _)| !cover.variants.contains(v))
                        .map(|(v, _)| v.as_str())
                        .collect();
                    format!("missing: {}", m.join(", "))
                }
                None => "add a `_ =>` arm".to_string(),
            };
            return err(format!("`match` does not handle every case ({})", missing), pos);
        }
        Ok((value_t, tarms))
    }

    /// The pattern of one arm; declares its bindings in the current scope.
    fn pattern(
        &self,
        cx: &mut Ctx,
        arm: &MatchArm,
        ty: &Type,
        enum_info: Option<&(String, EnumInfo)>,
        cover: &mut Coverage,
    ) -> Result<TPattern> {
        Ok(match (&arm.pattern, enum_info) {
            (Pattern::Wildcard, _) => {
                cover.wildcard = true;
                TPattern::Wildcard
            }
            (Pattern::Variant(v, binds), Some((ename, info))) => {
                let Some(fields) = info.variant(v).cloned() else {
                    let variants = info.variants.iter().map(|(n, _)| n.as_str());
                    return unknown(format!("enum `{}` has no variant `{}`", ename, v), arm.pos, v, variants);
                };
                if cover.variants.contains(v) {
                    return err(format!("variant `{}` is already handled by an earlier arm", v), arm.pos);
                }
                cover.variants.push(v.clone());
                self.note_variant(arm.pos, ename, v);
                if binds.len() != fields.len() {
                    return err(
                        format!("`{}` has {} value(s), but the pattern binds {}", v, fields.len(), binds.len()),
                        arm.pos,
                    );
                }
                let mut names = vec![];
                for (b, (_, t)) in binds.iter().zip(&fields) {
                    if b == "_" {
                        names.push(None);
                    } else {
                        self.declare(cx, b, t.clone(), false, arm.pos)?;
                        names.push(Some(b.clone()));
                    }
                }
                TPattern::Variant { enum_name: ename.clone(), variant: v.clone(), binds: names }
            }
            (Pattern::Int(n), None) if *ty == Type::Int => TPattern::Int(*n),
            (Pattern::Str(st), None) if *ty == Type::Str => TPattern::Str(st.clone()),
            (Pattern::Bool(b), None) if *ty == Type::Bool => {
                if *b {
                    cover.saw_true = true;
                } else {
                    cover.saw_false = true;
                }
                TPattern::Bool(*b)
            }
            (Pattern::Variant(v, _), None) => return err(format!("`{}` is not a variant of `{}`", v, ty), arm.pos),
            _ => return err(format!("this pattern does not fit a `{}` value", ty), arm.pos),
        })
    }
}

/// Which cases of a `match` the arms so far handle.
#[derive(Default)]
struct Coverage {
    variants: Vec<String>,
    wildcard: bool,
    saw_true: bool,
    saw_false: bool,
}

/// `Ok` and `Err` as enum variants, for matching on a `Result<t, e>`.
fn result_variants(t: &Type, e: &Type) -> EnumInfo {
    let ok = if *t == Type::Void { vec![] } else { vec![("value".to_string(), t.clone())] };
    let variants = vec![("Ok".to_string(), ok), ("Err".to_string(), vec![("error".to_string(), e.clone())])];
    EnumInfo { variants, ..EnumInfo::default() }
}
