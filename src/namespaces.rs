//! Package namespaces.
//!
//! Every file belongs to a *unit*: the project itself (`""`), a utkal package (its name)
//! or a bundled `std/` module (`std_name`). All units are compiled into one program, so
//! when two units declare the same top-level name, all but one of the declarations are
//! renamed to `Name__unit`. The unit that keeps the plain name is the one Rust glue can see
//! (glue refers to Kalinga types as `crate::Name`), otherwise the project itself.
//!
//! References are then resolved per file: the file's own unit first, then the units it
//! imports (an import also brings in what that unit imports), and `alias.Name` for
//! `import "pkg" as alias`. Locals shadow top-level names and are never renamed.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{Pos, Result, err};

/// What the driver learned while loading files.
#[derive(Default)]
pub struct Units {
    /// Unit of each file (index = `Pos::file`).
    pub file_unit: Vec<String>,
    /// Per file: `import "pkg" as alias` → unit.
    pub file_aliases: Vec<HashMap<String, String>>,
    /// Per unit: units it imports without an alias.
    pub imports: HashMap<String, HashSet<String>>,
    /// Units containing Rust code (`use rust` or inline Rust bodies).
    pub rust_units: HashSet<String>,
}

/// Mangled name → how to show it in messages (`mathx.Vec2`).
pub type DisplayNames = Vec<(String, String)>;

pub fn apply(prog: &mut Program, units: &Units) -> Result<DisplayNames> {
    let unit_of = |pos: Pos| units.file_unit.get(pos.file as usize).cloned().unwrap_or_default();

    // 1. Which units declare which names.
    let mut declared: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |name: &str, pos: Pos| {
        let list = declared.entry(name.to_string()).or_default();
        let u = unit_of(pos);
        if !list.contains(&u) {
            list.push(u);
        }
    };
    for f in &prog.functions {
        add(&f.name, f.pos);
    }
    for c in &prog.classes {
        add(&c.name, c.pos);
    }
    for i in &prog.interfaces {
        add(&i.name, i.pos);
    }
    for e in &prog.enums {
        add(&e.name, e.pos);
    }
    for s in &prog.structs {
        add(&s.name, s.pos);
    }
    for x in &prog.extern_fns {
        add(&x.name, x.pos);
    }
    for (n, p) in &prog.extern_types {
        add(n, *p);
    }

    // 2. Units visible from each unit (its own, its imports, and their imports).
    let all_units: HashSet<String> = units.file_unit.iter().cloned().collect();
    let mut visible: HashMap<String, Vec<String>> = HashMap::new();
    for u in &all_units {
        let mut seen = vec![u.clone()];
        let mut i = 0;
        while i < seen.len() {
            if let Some(next) = units.imports.get(&seen[i]) {
                let mut next: Vec<&String> = next.iter().collect();
                next.sort();
                for n in next {
                    if !seen.contains(n) {
                        seen.push(n.clone());
                    }
                }
            }
            i += 1;
        }
        visible.insert(u.clone(), seen);
    }

    // Rust glue names Kalinga types as `crate::Name`, so the units glue can see keep their names.
    let glue_sees: HashSet<String> =
        units.rust_units.iter().flat_map(|u| visible.get(u).cloned().unwrap_or_default()).collect();

    // 3. Rename clashing declarations.
    let mut renamed: HashMap<(String, String), String> = HashMap::new();
    let mut display: DisplayNames = vec![];
    for (name, us) in &declared {
        if us.len() < 2 {
            continue;
        }
        let with_rust: Vec<&String> = us.iter().filter(|u| glue_sees.contains(*u)).collect();
        let keeper = if with_rust.len() == 1 {
            Some(with_rust[0].clone())
        } else if us.iter().any(|u| u.is_empty()) {
            Some(String::new())
        } else {
            None
        };
        for u in us {
            if Some(u) != keeper.as_ref() {
                let m = format!("{}__{}", name, u);
                let shown = if u.is_empty() { name.clone() } else { format!("{}.{}", u.replace("std_", "std/"), name) };
                display.push((m.clone(), shown));
                renamed.insert((u.clone(), name.clone()), m);
            }
        }
    }

    let r = Resolver { units, declared: &declared, renamed: &renamed, visible: &visible };
    r.rewrite(prog)?;
    Ok(display)
}

struct Resolver<'a> {
    units: &'a Units,
    declared: &'a HashMap<String, Vec<String>>,
    renamed: &'a HashMap<(String, String), String>,
    visible: &'a HashMap<String, Vec<String>>,
}

/// Local variable names in scope while walking a body.
struct Scope(Vec<HashSet<String>>);

impl Scope {
    fn has(&self, n: &str) -> bool {
        self.0.iter().any(|s| s.contains(n))
    }
    fn add(&mut self, n: &str) {
        self.0.last_mut().unwrap().insert(n.to_string());
    }
    fn push(&mut self) {
        self.0.push(HashSet::new());
    }
    fn pop(&mut self) {
        self.0.pop();
    }
}

impl<'a> Resolver<'a> {
    fn unit(&self, pos: Pos) -> String {
        self.units.file_unit.get(pos.file as usize).cloned().unwrap_or_default()
    }

    /// The name a declaration of `name` in `unit` ends up with.
    fn final_name(&self, unit: &str, name: &str) -> String {
        self.renamed.get(&(unit.to_string(), name.to_string())).cloned().unwrap_or_else(|| name.to_string())
    }

    /// Resolves a reference to top-level `name` from a file.
    fn resolve(&self, name: &str, pos: Pos) -> Result<String> {
        let Some(owners) = self.declared.get(name) else { return Ok(name.to_string()) };
        if owners.len() < 2 {
            return Ok(name.to_string());
        }
        let here = self.unit(pos);
        let vis = &self.visible[&here];
        if owners.contains(&here) {
            return Ok(self.final_name(&here, name));
        }
        let candidates: Vec<&String> = vis.iter().filter(|u| owners.contains(*u)).collect();
        match candidates.len() {
            1 => Ok(self.final_name(candidates[0], name)),
            0 => err(
                format!("`{}` is defined in several packages ({}); import the one you mean", name, show_units(owners)),
                pos,
            ),
            _ => err(
                format!(
                    "`{}` is defined in both {}; use `import \"...\" as name` and write `name.{}`",
                    name,
                    show_units(&candidates.into_iter().cloned().collect::<Vec<_>>()),
                    name
                ),
                pos,
            ),
        }
    }

    /// `alias.Name` for `import "pkg" as alias`.
    fn resolve_qualified(&self, alias: &str, name: &str, pos: Pos) -> Option<Result<String>> {
        let unit = self.units.file_aliases.get(pos.file as usize)?.get(alias)?;
        if !self.declared.get(name).is_some_and(|us| us.contains(unit)) {
            return Some(err(format!("package `{}` has no `{}`", alias, name), pos));
        }
        Some(Ok(self.final_name(unit, name)))
    }

    fn is_alias(&self, n: &str, pos: Pos, scope: &Scope) -> bool {
        !scope.has(n) && self.units.file_aliases.get(pos.file as usize).is_some_and(|a| a.contains_key(n))
    }

    fn rewrite(&self, prog: &mut Program) -> Result<()> {
        let unit_of = |p: Pos| self.unit(p);
        for f in &mut prog.functions {
            f.name = self.final_name(&unit_of(f.pos), &f.name);
            self.func(f, &[])?;
        }
        for c in &mut prog.classes {
            c.name = self.final_name(&unit_of(c.pos), &c.name);
            for (b, p) in &mut c.bases {
                *b = self.resolve(b, *p)?;
            }
            for fd in &mut c.fields {
                self.ty(&mut fd.ty, &c.tparams)?;
                if let Some(d) = &mut fd.default {
                    self.expr(d, &mut Scope(vec![HashSet::new()]))?;
                }
            }
            if let Some(init) = &mut c.init {
                let mut sc = Scope(vec![HashSet::new()]);
                for p in &mut init.params {
                    self.ty(&mut p.ty, &c.tparams)?;
                    sc.add(&p.name);
                }
                self.block(&mut init.body, &mut sc)?;
            }
            for m in &mut c.methods {
                self.func(m, &c.tparams)?;
            }
        }
        for i in &mut prog.interfaces {
            i.name = self.final_name(&unit_of(i.pos), &i.name);
            for (b, p) in &mut i.bases {
                *b = self.resolve(b, *p)?;
            }
            for m in &mut i.methods {
                for p in &mut m.params {
                    self.ty(&mut p.ty, &[])?;
                }
                self.ty(&mut m.ret, &[])?;
            }
        }
        for e in &mut prog.enums {
            e.name = self.final_name(&unit_of(e.pos), &e.name);
            for v in &mut e.variants {
                for p in &mut v.fields {
                    self.ty(&mut p.ty, &[])?;
                }
            }
        }
        for s in &mut prog.structs {
            s.name = self.final_name(&unit_of(s.pos), &s.name);
            for fd in s.fields.iter_mut().chain(s.consts.iter_mut()) {
                self.ty(&mut fd.ty, &s.tparams)?;
                if let Some(d) = &mut fd.default {
                    self.expr(d, &mut Scope(vec![HashSet::new()]))?;
                }
            }
            for m in &mut s.methods {
                self.func(m, &s.tparams)?;
            }
            for o in &mut s.ops {
                let mut sc = Scope(vec![HashSet::new()]);
                for p in &mut o.params {
                    self.ty(&mut p.ty, &s.tparams)?;
                    sc.add(&p.name);
                }
                self.ty(&mut o.ret, &s.tparams)?;
                self.block(&mut o.body, &mut sc)?;
            }
        }
        for x in &mut prog.extern_fns {
            let new = self.final_name(&unit_of(x.pos), &x.name);
            if new != x.name && x.path.is_none() && x.body.is_none() {
                // The Rust function keeps its own name.
                x.path = Some(x.name.clone());
            }
            x.name = new;
            for p in &mut x.params {
                self.ty(&mut p.ty, &[])?;
            }
            self.ty(&mut x.ret, &[])?;
        }
        for (n, p) in &mut prog.extern_types {
            *n = self.final_name(&unit_of(*p), n);
        }
        Ok(())
    }

    fn func(&self, f: &mut FnDecl, outer_tparams: &[String]) -> Result<()> {
        let tps: Vec<String> = outer_tparams.iter().chain(f.tparams.iter()).cloned().collect();
        let mut sc = Scope(vec![HashSet::new()]);
        for p in &mut f.params {
            self.ty(&mut p.ty, &tps)?;
            sc.add(&p.name);
        }
        self.ty(&mut f.ret, &tps)?;
        self.block(&mut f.body, &mut sc)
    }

    fn ty(&self, t: &mut TypeExpr, tparams: &[String]) -> Result<()> {
        match t {
            TypeExpr::Named(n, args, pos) => {
                for a in args.iter_mut() {
                    self.ty(a, tparams)?;
                }
                if let Some((alias, name)) = n.split_once('.') {
                    *n = match self.resolve_qualified(alias, name, *pos) {
                        Some(r) => r?,
                        None => {
                            return err(
                                format!("`{}` is not an imported package (use `import \"...\" as {}`)", alias, alias),
                                *pos,
                            );
                        }
                    };
                } else if !tparams.contains(n) {
                    *n = self.resolve(n, *pos)?;
                }
            }
            TypeExpr::Array(i) | TypeExpr::Optional(i) => self.ty(i, tparams)?,
            TypeExpr::Map(k, v) => {
                self.ty(k, tparams)?;
                self.ty(v, tparams)?;
            }
            TypeExpr::Func(ps, r) => {
                for p in ps.iter_mut() {
                    self.ty(p, tparams)?;
                }
                self.ty(r, tparams)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn block(&self, stmts: &mut [Stmt], sc: &mut Scope) -> Result<()> {
        sc.push();
        let r = stmts.iter_mut().try_for_each(|s| self.stmt(s, sc));
        sc.pop();
        r
    }

    fn stmt(&self, s: &mut Stmt, sc: &mut Scope) -> Result<()> {
        match &mut s.kind {
            StmtKind::Let { name, ty, value, .. } => {
                if let Some(t) = ty {
                    self.ty(t, &[])?;
                }
                self.expr(value, sc)?;
                sc.add(name);
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, sc)?;
                self.expr(value, sc)?;
            }
            StmtKind::If { cond, then, otherwise } => {
                self.expr(cond, sc)?;
                self.block(then, sc)?;
                if let Some(o) = otherwise {
                    self.block(o, sc)?;
                }
            }
            StmtKind::IfLet { name, value, then, otherwise } => {
                self.expr(value, sc)?;
                sc.push();
                sc.add(name);
                let r = self.block(then, sc);
                sc.pop();
                r?;
                if let Some(o) = otherwise {
                    self.block(o, sc)?;
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, sc)?;
                self.block(body, sc)?;
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, sc)?;
                for arm in arms {
                    sc.push();
                    if let Pattern::Variant(_, binds) = &arm.pattern {
                        for b in binds {
                            sc.add(b);
                        }
                    }
                    let r = self.block(&mut arm.body, sc);
                    sc.pop();
                    r?;
                }
            }
            StmtKind::For { var, iter, body } => {
                match iter {
                    ForIter::Range(a, b) => {
                        self.expr(a, sc)?;
                        self.expr(b, sc)?;
                    }
                    ForIter::Each(e) => self.expr(e, sc)?,
                }
                sc.push();
                sc.add(var);
                let r = self.block(body, sc);
                sc.pop();
                r?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Expr(e) => self.expr(e, sc)?,
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
        Ok(())
    }

    fn expr(&self, e: &mut Expr, sc: &mut Scope) -> Result<()> {
        let pos = e.pos;
        // `alias.Name(...)` and `alias.Name` become plain references.
        match &mut e.kind {
            ExprKind::MethodCall(recv, name, args) => {
                if let ExprKind::Ident(a) = &recv.kind
                    && self.is_alias(a, recv.pos, sc)
                {
                    let resolved = self.resolve_qualified(a, name, pos).unwrap()?;
                    let mut args = std::mem::take(args);
                    for x in &mut args {
                        self.expr(x, sc)?;
                    }
                    e.kind = ExprKind::Call(resolved, vec![], args);
                    return Ok(());
                }
            }
            ExprKind::Field(recv, name) => {
                if let ExprKind::Ident(a) = &recv.kind
                    && self.is_alias(a, recv.pos, sc)
                {
                    let resolved = self.resolve_qualified(a, name, pos).unwrap()?;
                    e.kind = ExprKind::Ident(resolved);
                    return Ok(());
                }
            }
            _ => {}
        }
        match &mut e.kind {
            ExprKind::Ident(n) => {
                if !sc.has(n) {
                    *n = self.resolve(n, pos)?;
                }
            }
            ExprKind::Call(n, targs, args) => {
                if !sc.has(n) {
                    *n = self.resolve(n, pos)?;
                }
                for t in targs.iter_mut() {
                    self.ty(t, &[])?;
                }
                for a in args {
                    self.expr(a, sc)?;
                }
            }
            ExprKind::Downcast { value, class, .. } => {
                self.expr(value, sc)?;
                *class = self.resolve(class, pos)?;
            }
            ExprKind::Array(items) => {
                for i in items {
                    self.expr(i, sc)?;
                }
            }
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    self.expr(k, sc)?;
                    self.expr(v, sc)?;
                }
            }
            ExprKind::Interp(parts) => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.expr(x, sc)?;
                    }
                }
            }
            ExprKind::Binary(_, l, r) => {
                self.expr(l, sc)?;
                self.expr(r, sc)?;
            }
            ExprKind::Unary(_, x) | ExprKind::Await(x) | ExprKind::Try(x) => self.expr(x, sc)?,
            ExprKind::MethodCall(recv, _, args) => {
                self.expr(recv, sc)?;
                for a in args {
                    self.expr(a, sc)?;
                }
            }
            ExprKind::Field(recv, _) => self.expr(recv, sc)?,
            ExprKind::Index(b, i) => {
                self.expr(b, sc)?;
                self.expr(i, sc)?;
            }
            ExprKind::SuperInit(args) | ExprKind::SuperCall(_, args) => {
                for a in args {
                    self.expr(a, sc)?;
                }
            }
            ExprKind::Lambda { params, ret, body } => {
                sc.push();
                for (n, t, _) in params.iter_mut() {
                    if let Some(t) = t {
                        self.ty(t, &[])?;
                    }
                    sc.add(n);
                }
                if let Some(r) = ret {
                    self.ty(r, &[])?;
                }
                let r = match body {
                    LambdaBody::Expr(x) => self.expr(x, sc),
                    LambdaBody::Block(stmts) => self.block(stmts, sc),
                };
                sc.pop();
                r?;
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::SelfRef => {}
        }
        Ok(())
    }
}

fn show_units(units: &[String]) -> String {
    units
        .iter()
        .map(|u| if u.is_empty() { "this project".to_string() } else { format!("`{}`", u.replace("std_", "std/")) })
        .collect::<Vec<_>>()
        .join(" and ")
}
