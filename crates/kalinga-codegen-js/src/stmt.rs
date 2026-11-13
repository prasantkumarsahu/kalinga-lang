//! Statements, and the places assignments store into.

use kalinga_sema::Type;
use kalinga_sema::tast::*;
use kalinga_syntax::ast::{AssignOp, BinOp};

use crate::expr::arith;
use crate::{Gen, indent, jid, js_str};

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

    /// `{ ... }` starting with `head` lines (already indented), then the statements.
    fn block_with(&self, head: &str, stmts: &[TStmt], depth: usize) -> String {
        format!("{{\n{}{}{}}}", head, self.block_inner(stmts, depth + 1), indent(depth))
    }

    fn stmt(&self, s: &TStmt, depth: usize) -> String {
        match s {
            TStmt::LetTuple { names, mutable, value } => {
                for (n, shared) in names {
                    if *shared {
                        self.shared.borrow_mut().insert(n.clone());
                    }
                }
                let kw = if *mutable || names.iter().any(|(_, s)| *s) { "let" } else { "const" };
                let binds: Vec<String> = names.iter().map(|(n, _)| jid(n)).collect();
                format!("{} [{}] = {};", kw, binds.join(", "), self.expr(value))
            }
            TStmt::Let { name, mutable, shared, value, .. } => {
                if *shared {
                    self.shared.borrow_mut().insert(name.clone());
                }
                let kw = if *mutable || *shared { "let" } else { "const" };
                format!("{} {} = {};", kw, jid(name), self.expr(value))
            }
            TStmt::Assign { place, op, value } => self.assign(place, *op, value),
            TStmt::SetField { recv, field, op, value } => {
                let new = match op {
                    AssignOp::Set => "$v".to_string(),
                    _ => compound(&format!("$r.{}", field), *op, &value.ty),
                };
                format!(
                    "{{ const $v = {}; const $r = {}; $r.{} = {}; }}",
                    self.expr(value),
                    self.recv(recv),
                    field,
                    new
                )
            }
            TStmt::MapInsert { map, key, value } => {
                format!(
                    "{{ const $v = {}; const $k = {}; {}.set($k, $v); }}",
                    self.expr(value),
                    self.expr(key),
                    self.read_place(map)
                )
            }
            TStmt::StructAssign { place, index, value } => {
                let (pre, target) = self.lvalue(place);
                format!("{{ const $v = {}; {}{} = ({}).op${}($v); }}", self.expr(value), pre, target, target, index)
            }
            TStmt::If { cond, then, otherwise } => {
                let mut out = format!("if ({}) {}", self.expr(cond), self.block(then, depth));
                if let Some(o) = otherwise {
                    out += &format!(" else {}", self.block(o, depth));
                }
                out
            }
            TStmt::IfLet { name, value, then, otherwise } => {
                let t = self.temp();
                let head = format!("{}const {} = {};\n", indent(depth + 1), jid(name), t);
                let mut out = format!(
                    "{{\n{in1}const {t} = {};\n{in1}if ({t} !== null) {}",
                    self.expr(value),
                    self.block_with(&format!("  {}", head), then, depth + 1),
                    in1 = indent(depth + 1),
                );
                if let Some(o) = otherwise {
                    out += &format!(" else {}", self.block(o, depth + 1));
                }
                out + &format!("\n{}}}", indent(depth))
            }
            TStmt::Match { value, arms } => self.match_stmt(value, arms, depth),
            TStmt::While { cond, body } => format!("while ({}) {}", self.expr(cond), self.block(body, depth)),
            TStmt::WhileLet { name, value, body } => {
                let t = self.temp();
                let head = format!(
                    "{in1}const {t} = {};\n{in1}if ({t} === null) break;\n{in1}const {} = {t};\n",
                    self.expr(value),
                    jid(name),
                    in1 = indent(depth + 1),
                );
                format!("while (true) {}", self.block_with(&head, body, depth))
            }
            TStmt::ForRange { var, from, to, inclusive, body } => {
                let end = self.temp();
                let v = jid(var);
                format!(
                    "for (let {v} = {}, {end} = {}; {v} {} {end}; {v}++) {}",
                    self.expr(from),
                    self.expr(to),
                    if *inclusive { "<=" } else { "<" },
                    self.block(body, depth)
                )
            }
            TStmt::ForEach { index, var, iter, body } => self.for_each(index.as_deref(), var, iter, body, depth),
            TStmt::Return(None) => "return;".into(),
            TStmt::Return(Some(v)) => format!("return {};", self.expr(v)),
            TStmt::Break => "break;".into(),
            TStmt::Continue => "continue;".into(),
            TStmt::Expr(e) => format!("{};", self.expr(e)),
            TStmt::SuperInit { owner, args } => format!("this.__init_{}({});", owner, self.exprs(args)),
        }
    }

    fn for_each(&self, index: Option<&str>, var: &str, iter: &TExpr, body: &[TStmt], depth: usize) -> String {
        let code = self.expr(iter);
        let (body, var) = (self.block(body, depth), jid(var));
        match (&iter.ty, index.map(jid)) {
            (Type::Map(..), None) => format!("for (const {} of ({}).keys()) {}", var, code, body),
            (Type::Map(..), Some(k)) => format!("for (const [{}, {}] of $kl.mentries({})) {}", k, var, code, body),
            (Type::Str, None) => format!("for (const {} of $kl.chars({})) {}", var, code, body),
            (Type::Str, Some(i)) => format!("for (const [{}, {}] of $kl.chars({}).entries()) {}", i, var, code, body),
            (_, None) => format!("for (const {} of {}) {}", var, code, body),
            (_, Some(i)) => format!("for (const [{}, {}] of ({}).entries()) {}", i, var, code, body),
        }
    }

    /// `match` as a chain of `if`s on a temporary (not `switch`, so `break` and `continue`
    /// inside an arm still reach the loop around it).
    fn match_stmt(&self, value: &TExpr, arms: &[TArm], depth: usize) -> String {
        let m = self.temp();
        let mut out = format!("{{\n{}const {} = {};\n{}", indent(depth + 1), m, self.expr(value), indent(depth + 1));
        let mut first = true;
        for arm in arms {
            let (test, binds) = self.pattern(&m, &arm.pattern);
            let binds = indent_lines(&binds, depth + 2);
            let block = self.block_with(&binds, &arm.body, depth + 1);
            let sep = if first { "" } else { " else " };
            first = false;
            match test {
                Some(t) => out += &format!("{}if ({}) {}", sep, t, block),
                None => {
                    out += &format!("{}{}", sep, block);
                    break;
                }
            }
        }
        out + &format!("\n{}}}", indent(depth))
    }

    /// `place = value` or `place op= value`.
    fn assign(&self, place: &TPlace, op: AssignOp, value: &TExpr) -> String {
        let v = self.expr(value);
        let ty = place.ty();
        // A map entry: read and written through the map, which checks the key exists.
        if let TPlace::Index { base, index, .. } = place
            && let Type::Map(k, _) = base.ty()
        {
            let show = self.shower(k);
            let new = match op {
                AssignOp::Set => "$v".to_string(),
                _ => compound(&format!("$kl.mref($m, $k, {})", show), op, ty),
            };
            return format!(
                "{{ const $v = {}; const $m = {}; const $k = {}; $kl.mput($m, $k, {}, {}); }}",
                v,
                self.read_place(base),
                self.expr(index),
                new,
                show
            );
        }
        let (pre, target) = self.lvalue(place);
        let wrapped =
            matches!(place, TPlace::SelfField { wrapped: true, .. } | TPlace::ObjectField { wrapped: true, .. });
        let new = match op {
            AssignOp::Set => "$v".to_string(),
            _ if wrapped => {
                let field = match place {
                    TPlace::SelfField { field, .. } | TPlace::ObjectField { field, .. } => field.as_str(),
                    _ => "field",
                };
                compound(&format!("$kl.unset({}, {})", target, js_str(field)), op, ty)
            }
            _ => compound(&target, op, ty),
        };
        format!("{{ const $v = {}; {}{} = {}; }}", v, pre, target, new)
    }

    /// Where an assignment stores: (setup statements, JavaScript place).
    fn lvalue(&self, p: &TPlace) -> (String, String) {
        match p {
            TPlace::Index { base, index, .. } => {
                // A struct field inside a map entry is changed in place, so maps only reach
                // here as the base of something else.
                let (a, i) = (self.temp(), self.temp());
                (
                    format!("const {a} = {}; const {i} = $kl.ix({a}, {}); ", self.read_place(base), self.expr(index)),
                    format!("{a}[{i}]"),
                )
            }
            _ => (String::new(), self.read_place(p)),
        }
    }

    /// The value stored in a place, read without copying it (for changing it in place).
    pub(crate) fn read_place(&self, p: &TPlace) -> String {
        match p {
            TPlace::Local { name, .. } | TPlace::SharedLocal { name, .. } => jid(name),
            TPlace::SelfField { field, .. } => format!("this.{}", field),
            TPlace::Static { owner, name, .. } => format!("s${}${}", owner, name),
            TPlace::StructField { base, field, .. } => format!("{}.{}", self.read_place(base), field),
            TPlace::ObjectField { recv, field, .. } => format!("{}.{}", self.recv(recv), field),
            TPlace::Index { base, index, .. } => match base.ty() {
                Type::Map(k, _) => {
                    format!("$kl.mref({}, {}, {})", self.read_place(base), self.expr(index), self.shower(k))
                }
                _ => format!("$kl.aref({}, {})", self.read_place(base), self.expr(index)),
            },
        }
    }
}

/// `current op $v`, for compound assignments.
fn compound(current: &str, op: AssignOp, ty: &Type) -> String {
    let bin = match op {
        AssignOp::Set => return "$v".into(),
        AssignOp::Add => BinOp::Add,
        AssignOp::Sub => BinOp::Sub,
        AssignOp::Mul => BinOp::Mul,
        AssignOp::Div => BinOp::Div,
        AssignOp::Rem => BinOp::Rem,
    };
    arith(bin, ty, current, "$v")
}

/// `lines` (each indented by 4) re-indented to `depth`.
fn indent_lines(lines: &str, depth: usize) -> String {
    lines.lines().map(|l| format!("{}{}\n", indent(depth), l.trim_start())).collect()
}
