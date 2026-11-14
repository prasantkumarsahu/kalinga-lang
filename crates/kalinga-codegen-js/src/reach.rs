//! The code a program can reach from `main` (or its tests), so a web build leaves out the
//! parts of imported modules it never uses. Types are kept whole: a class or struct that is
//! made or used brings all its methods.

use std::collections::HashSet;

use kalinga_sema::Model;
use kalinga_sema::tast::*;
use kalinga_sema::visit::{Visitor, walk_expr, walk_stmts};

/// What a program uses, by name.
#[derive(Default)]
pub(crate) struct Reach {
    pub functions: HashSet<String>,
    /// `extern fn`s with a body (by name).
    pub externs: HashSet<String>,
    /// Classes and structs.
    pub types: HashSet<String>,
    /// Top-level constants.
    pub consts: HashSet<String>,
    /// (owner, name) of `const` and `static var` members.
    pub statics: HashSet<(String, String)>,
}

enum Item {
    Function(String),
    Type(String),
    Const(String),
    Static(String, String),
}

/// Finds what `main` (or, with `tests`, the root file's tests) reaches.
pub(crate) fn reach(model: &Model, typed: &TypedProgram, tests: bool) -> Reach {
    let mut f = Finder { reach: Reach::default(), queue: vec![] };
    if tests {
        for t in typed.tests.iter().filter(|t| t.pos.file == 0) {
            walk_stmts(&mut f, &t.body.stmts);
        }
    } else {
        f.add(Item::Function("main".into()));
    }
    while let Some(item) = f.queue.pop() {
        match item {
            Item::Function(name) => {
                if let Some(b) = typed.functions.get(&name) {
                    walk_stmts(&mut f, &b.stmts);
                }
            }
            Item::Const(name) => {
                if let Some(v) = typed.static_values.get(&(String::new(), name)) {
                    walk_expr(&mut f, v);
                }
            }
            Item::Static(owner, name) => {
                if let Some(v) = typed.static_values.get(&(owner, name)) {
                    walk_expr(&mut f, v);
                }
            }
            Item::Type(name) => f.type_bodies(model, typed, &name),
        }
    }
    f.reach
}

struct Finder {
    reach: Reach,
    queue: Vec<Item>,
}

impl Finder {
    /// Queues `item` the first time it is seen.
    fn add(&mut self, item: Item) {
        let new = match &item {
            Item::Function(n) => self.reach.functions.insert(n.clone()),
            Item::Type(n) => self.reach.types.insert(n.clone()),
            Item::Const(n) => self.reach.consts.insert(n.clone()),
            Item::Static(o, n) => self.reach.statics.insert((o.clone(), n.clone())),
        };
        if new {
            self.queue.push(item);
        }
    }

    /// Everything a class or struct's code uses: methods, `init`, field defaults,
    /// operators, its base class and the interface methods it gets.
    fn type_bodies(&mut self, model: &Model, typed: &TypedProgram, name: &str) {
        let key = |m: &String| (name.to_string(), m.clone());
        if let Some(info) = model.classes.get(name) {
            if let Some(p) = &info.parent {
                self.add(Item::Type(p.clone()));
            }
            for m in &info.decl.methods {
                if let Some(Some(b)) = typed.methods.get(&key(&m.name)) {
                    walk_stmts(self, &b.stmts);
                }
                if let Some(b) = typed.statics.get(&key(&m.name)) {
                    walk_stmts(self, &b.stmts);
                }
            }
            if let Some(b) = typed.inits.get(name) {
                walk_stmts(self, &b.stmts);
            }
            for (f, _) in &info.fields {
                if let Some(e) = typed.field_defaults.get(&key(f)) {
                    walk_expr(self, e);
                }
            }
            for iface in model.all_interfaces(name) {
                for (m, _) in &model.interfaces[&iface].methods {
                    if let Some(b) = typed.iface_methods.get(&(iface.clone(), m.clone())) {
                        walk_stmts(self, &b.stmts);
                    }
                }
            }
        }
        if let Some(info) = model.structs.get(name) {
            for m in &info.decl.methods {
                if let Some(b) = typed.struct_methods.get(&key(&m.name)) {
                    walk_stmts(self, &b.stmts);
                }
            }
            for o in &info.ops {
                if let Some(b) = typed.struct_ops.get(&(name.to_string(), o.index)) {
                    walk_stmts(self, &b.stmts);
                }
            }
        }
    }
}

impl Visitor for Finder {
    fn expr(&mut self, e: &TExpr) {
        match &e.kind {
            TExprKind::FnRef(name) => self.add(Item::Function(name.clone())),
            TExprKind::Static { owner, name, .. } if owner.is_empty() => self.add(Item::Const(name.clone())),
            TExprKind::Static { owner, name, .. } => self.add(Item::Static(owner.clone(), name.clone())),
            TExprKind::Downcast { target, .. } => self.add(Item::Type(target.clone())),
            TExprKind::Call { callee, .. } => match callee {
                Callee::Function { name, .. } => self.add(Item::Function(name.clone())),
                Callee::Extern { path } => {
                    if let Some(name) = path.strip_prefix("__x_") {
                        self.reach.externs.insert(name.to_string());
                    }
                }
                Callee::New { class: t, .. }
                | Callee::StructNew { strukt: t, .. }
                | Callee::Static { owner: t, .. }
                | Callee::SuperMethod { owner: t, .. } => self.add(Item::Type(t.clone())),
                _ => {}
            },
            _ => {}
        }
    }

    fn place(&mut self, p: &TPlace) {
        if let TPlace::Static { owner, name, .. } = p {
            self.add(Item::Static(owner.clone(), name.clone()));
        }
    }
}
