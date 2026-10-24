//! Statements, and the places assignments store into.

use std::fmt::Write as _;

use kalinga_sema::Type;
use kalinga_sema::tast::*;
use kalinga_syntax::ast::AssignOp;

use crate::{Gen, indent, rid};

impl Gen<'_> {
    /// Statements at `depth`, one per line.
    pub(crate) fn block_inner(&self, stmts: &[TStmt], depth: usize) -> String {
        let mut out = String::new();
        for s in stmts {
            out.push_str(&indent(depth));
            out.push_str(&self.stmt(s, depth));
            out.push('\n');
        }
        out
    }

    /// `{ ... }` around statements one level deeper than `depth`.
    fn block(&self, stmts: &[TStmt], depth: usize) -> String {
        format!("{{\n{}{}}}", self.block_inner(stmts, depth + 1), indent(depth))
    }

    /// `let (a, b) = value`; parts that a lambda changes then move into shared cells.
    fn let_tuple(&self, names: &[(String, bool)], mutable: bool, value: &TExpr) -> String {
        let kw = if mutable { "mut " } else { "" };
        let binds: Vec<String> = names.iter().map(|(n, _)| format!("{}{}", kw, rid(n))).collect();
        let mut out = format!("let ({}) = {};", binds.join(", "), self.expr(value));
        for (n, _) in names.iter().filter(|(_, shared)| *shared) {
            out += &format!(" let {} = Rc::new(RefCell::new({}));", rid(n), rid(n));
        }
        out
    }

    fn stmt(&self, s: &TStmt, depth: usize) -> String {
        match s {
            TStmt::LetTuple { names, mutable, value } => self.let_tuple(names, *mutable, value),
            // A `var` a lambda changes lives in a shared cell.
            TStmt::Let { name, shared: true, ty, value, .. } => {
                format!(
                    "let {}: Rc<RefCell<{}>> = Rc::new(RefCell::new({}));",
                    rid(name),
                    self.rty(ty),
                    self.expr(value)
                )
            }
            TStmt::Let { name, mutable, ty, value, .. } => {
                let kw = if *mutable { "let mut" } else { "let" };
                format!("{} {}: {} = {};", kw, rid(name), self.rty(ty), self.expr(value))
            }
            TStmt::Assign { place, op, value } => self.assign(place, *op, value),
            TStmt::SetField { recv, field, op, value } => {
                let new = match op {
                    AssignOp::Set => "__v".to_string(),
                    _ => compound(format!("__r.borrow().__get_{}()", field), *op, &value.ty),
                };
                format!(
                    "{{ let __v = {}; let __r = &{}; let __n = {}; __r.borrow_mut().__set_{}(__n); }}",
                    self.expr(value),
                    self.recv(recv),
                    new,
                    field
                )
            }
            TStmt::MapInsert { map, key, value } => {
                let (pre, place) = self.place(map, &mut 0);
                format!(
                    "{{ let __v = {}; let __k = {}; {}{}.insert(__k, __v); }}",
                    self.expr(value),
                    self.expr(key),
                    pre,
                    place
                )
            }
            TStmt::StructAssign { place, index, value } => {
                let (pre, p) = self.place(place, &mut 0);
                format!(
                    "{{ let __v = {}; {}let __n = {}.__op_{}(__v); {} = __n; }}",
                    self.expr(value),
                    pre,
                    p,
                    index,
                    p
                )
            }
            TStmt::If { cond, then, otherwise } => {
                let mut out = format!("if {} {}", self.expr(cond), self.block(then, depth));
                if let Some(o) = otherwise {
                    let _ = write!(out, " else {}", self.block(o, depth));
                }
                out
            }
            TStmt::IfLet { name, value, then, otherwise } => {
                let mut out = format!("if let Some({}) = {} {}", rid(name), self.expr(value), self.block(then, depth));
                if let Some(o) = otherwise {
                    let _ = write!(out, " else {}", self.block(o, depth));
                }
                out
            }
            TStmt::Match { value, arms } => self.match_stmt(value, arms, depth),
            TStmt::While { cond, body } => {
                if matches!(cond.kind, TExprKind::Bool(true)) {
                    format!("loop {}", self.block(body, depth))
                } else {
                    format!("while {} {}", self.expr(cond), self.block(body, depth))
                }
            }
            TStmt::WhileLet { name, value, body } => {
                format!("while let Some({}) = {} {}", rid(name), self.expr(value), self.block(body, depth))
            }
            TStmt::ForRange { var, from, to, inclusive, body } => {
                let dots = if *inclusive { "..=" } else { ".." };
                format!(
                    "for {} in ({}){}({}) {}",
                    rid(var),
                    self.expr(from),
                    dots,
                    self.expr(to),
                    self.block(body, depth)
                )
            }
            TStmt::ForEach { index, var, iter, body } => self.for_each(index.as_deref(), var, iter, body, depth),
            TStmt::Return(None) => "return;".into(),
            TStmt::Return(Some(v)) => format!("return {};", self.expr(v)),
            TStmt::Break => "break;".into(),
            TStmt::Continue => "continue;".into(),
            TStmt::Expr(e) => format!("{};", self.expr(e)),
            TStmt::SuperInit { owner, args } => {
                let (binds, names) = self.bind_args(args);
                format!("{{ {}self.__{}__init({}); }}", binds, owner, names)
            }
        }
    }

    fn assign(&self, place: &TPlace, op: AssignOp, value: &TExpr) -> String {
        let v = self.expr(value);
        let (pre, p) = self.place(place, &mut 0);
        let ty = place.ty();
        let wrapped =
            matches!(place, TPlace::SelfField { wrapped: true, .. } | TPlace::ObjectField { wrapped: true, .. });
        if wrapped {
            let new = match op {
                AssignOp::Set => "__v".to_string(),
                _ => compound(format!("__unset({}.clone(), \"field\")", p), op, ty),
            };
            return format!("{{ let __v = {}; {}let __n = {}; {} = Some(__n); }}", v, pre, new, p);
        }
        let stmt = match (op, ty) {
            (AssignOp::Set, _) => format!("{} = __v;", p),
            (AssignOp::Add, Type::Str) => format!("{}.push_str(&__v);", p),
            _ => format!("{} {}= __v;", p, op_sym(op)),
        };
        format!("{{ let __v = {}; {}{} }}", v, pre, stmt)
    }

    /// An assignable location: (setup statements, Rust place expression). `n` numbers the
    /// temporaries of nested indexes.
    pub(crate) fn place(&self, p: &TPlace, n: &mut usize) -> (String, String) {
        match p {
            TPlace::Local { name, .. } => (String::new(), rid(name)),
            TPlace::SharedLocal { name, .. } => (String::new(), format!("(*{}.borrow_mut())", rid(name))),
            TPlace::SelfField { field, .. } => (String::new(), format!("self.{}", rid(field))),
            TPlace::Static { owner, name, .. } => (String::new(), format!("(*{}::__s_{}().borrow_mut())", owner, name)),
            TPlace::StructField { base, field, .. } => {
                let (pre, b) = self.place(base, n);
                (pre, format!("{}.{}", b, rid(field)))
            }
            TPlace::ObjectField { recv, field, .. } => {
                (String::new(), format!("{}.borrow_mut().{}", self.recv(recv), rid(field)))
            }
            TPlace::Index { base, index, .. } => {
                let (pre, b) = self.place(base, n);
                let i = self.expr(index);
                let at = *n;
                *n += 1;
                if matches!(base.ty(), Type::Map(..)) {
                    (format!("{}let __k{} = {}; ", pre, at, i), format!("(*__map_slot(&mut {}, __k{}))", b, at))
                } else {
                    (format!("{}let __i{} = __idx({}); ", pre, at, i), format!("{}[__i{}]", b, at))
                }
            }
        }
    }

    fn for_each(&self, index: Option<&str>, var: &str, iter: &TExpr, body: &[TStmt], depth: usize) -> String {
        let code = self.expr(iter);
        let (body, var) = (self.block(body, depth), rid(var));
        let Some(index) = index.map(rid) else {
            return match &iter.ty {
                Type::Map(..) => format!("for {} in ({}).into_keys() {}", var, code, body),
                Type::Str => format!("for __c in ({}).chars() {{ let {} = __c.to_string(); {} }}", code, var, body),
                _ => format!("for {} in {} {}", var, code, body),
            };
        };
        match &iter.ty {
            Type::Map(..) => format!("for ({}, {}) in ({}).into_iter() {}", index, var, code, body),
            Type::Str => format!(
                "for (__i, __c) in ({}).chars().enumerate() {{ let {} = __i as i64; let {} = __c.to_string(); {} }}",
                code, index, var, body
            ),
            _ => format!(
                "for (__i, {}) in ({}).into_iter().enumerate() {{ let {} = __i as i64; {} }}",
                var, code, index, body
            ),
        }
    }

    fn match_stmt(&self, value: &TExpr, arms: &[TArm], depth: usize) -> String {
        let mut out = format!("match {} {{\n", self.scrutinee(value));
        for arm in arms {
            let (pattern, unbox) = self.pattern(&arm.pattern);
            let body = self.block(&arm.body, depth + 1);
            let body = if unbox.is_empty() { body } else { format!("{{ {}{} }}", unbox, body) };
            let _ = writeln!(out, "{}{} => {}", indent(depth + 1), pattern, body);
        }
        let _ = write!(out, "{}}}", indent(depth));
        out
    }
}

impl Gen<'_> {
    /// The value a `match` looks at; strings are matched as `&str`.
    pub(crate) fn scrutinee(&self, value: &TExpr) -> String {
        let code = self.expr(value);
        if value.ty == Type::Str { format!("({}).as_str()", code) } else { code }
    }

    /// A branch of an `if` or `match` used as a value, as a Rust block.
    pub(crate) fn value_block(&self, b: &TValueBlock, unbox: &str) -> String {
        format!("{{ {}{}{} }}", unbox, self.block_inner(&b.stmts, 1), self.expr(&b.value))
    }

    /// `match` used as a value.
    pub(crate) fn match_expr(&self, value: &TExpr, arms: &[(TPattern, TValueBlock)]) -> String {
        let arms: Vec<String> = arms
            .iter()
            .map(|(p, b)| {
                let (pattern, unbox) = self.pattern(p);
                format!("{} => {}", pattern, self.value_block(b, &unbox))
            })
            .collect();
        format!("(match {} {{ {} }})", self.scrutinee(value), arms.join(", "))
    }

    /// A match pattern, and the `let x = *x;` lines that take boxed fields out of their box.
    fn pattern(&self, pattern: &TPattern) -> (String, String) {
        let TPattern::Variant { enum_name, variant, binds } = pattern else {
            let text = match pattern {
                TPattern::Int(n) => format!("{}i64", n),
                TPattern::Str(s) => format!("{:?}", s),
                TPattern::Bool(b) => b.to_string(),
                TPattern::Wildcard | TPattern::Variant { .. } => "_".to_string(),
            };
            return (text, String::new());
        };
        let names: Vec<String> = binds.iter().map(|b| b.as_deref().map_or("_".to_string(), rid)).collect();
        let unbox: String = match self.m.enums.get(enum_name) {
            Some(info) => binds
                .iter()
                .enumerate()
                .filter_map(|(i, b)| b.as_deref().filter(|_| info.is_boxed(variant, i)))
                .map(|b| format!("let {0} = *{0}; ", rid(b)))
                .collect(),
            None => String::new(),
        };
        let text = match (names.is_empty(), enum_name.as_str()) {
            (true, "Result") => format!("Result::{}(_)", variant),
            (true, _) => format!("{}::{}", enum_name, variant),
            _ => format!("{}::{}({})", enum_name, variant, names.join(", ")),
        };
        (text, unbox)
    }
}

/// `current op __v`, for compound assignments that rebuild the value.
fn compound(current: String, op: AssignOp, ty: &Type) -> String {
    if op == AssignOp::Add && *ty == Type::Str {
        format!("format!(\"{{}}{{}}\", {}, __v)", current)
    } else {
        format!("({} {} __v)", current, op_sym(op))
    }
}

fn op_sym(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "",
        AssignOp::Add => "+",
        AssignOp::Sub => "-",
        AssignOp::Mul => "*",
        AssignOp::Div => "/",
        AssignOp::Rem => "%",
    }
}
