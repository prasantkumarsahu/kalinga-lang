//! Warnings: problems that do not stop a program from building: uses of declarations
//! marked `#[deprecated]`, and variables that are never used (often a misspelt name: with no
//! keyword, `totl = 1` declares a new variable).

use std::collections::HashSet;

use kalinga_syntax::ast::{Program, Stmt, StmtKind, TypeExpr};
use kalinga_syntax::error::{Diagnostic, Span};
use kalinga_syntax::visit::{self, Visitor};

use crate::index::{Index, Symbol};
use crate::model::Model;

/// A warning for every use of something marked `#[deprecated]` (in code and in written
/// types), with the name used. Each points where the use starts; the driver moves it onto
/// the name.
pub fn warnings(prog: &Program, m: &Model, index: &Index) -> Vec<(Diagnostic, String)> {
    let mut out = deprecated(prog, m, index);
    out.extend(unused(prog, index));
    out.sort_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    out
}

/// Variables declared by a statement and never read. Names starting with `_` are left alone.
fn unused(prog: &Program, index: &Index) -> Vec<(Diagnostic, String)> {
    let mut lets = Lets(HashSet::new());
    visit::program(prog, &mut lets);
    let used: HashSet<Span> = index
        .refs
        .iter()
        .filter(|r| !r.is_decl)
        .filter_map(|r| match &r.symbol {
            Symbol::Local { decl, .. } => Some(*decl),
            _ => None,
        })
        .collect();
    let locals: Vec<(Span, &str)> = index
        .refs
        .iter()
        .filter(|r| r.is_decl)
        .filter_map(|r| match &r.symbol {
            Symbol::Local { name, decl, .. } => Some((*decl, name.as_str())),
            _ => None,
        })
        .collect();
    let mut seen = HashSet::new();
    let mut out = vec![];
    for &(decl, name) in &locals {
        if !lets.0.contains(&decl) || used.contains(&decl) || name.starts_with('_') || !seen.insert(decl) {
            continue;
        }
        let mut d = Diagnostic::warning(format!("`{}` is never used", name), decl);
        // A misspelling of a variable declared earlier in the same file?
        let earlier = locals
            .iter()
            .filter(|(s, n)| s.file == decl.file && (s.line, s.col) < (decl.line, decl.col) && *n != name)
            .map(|(_, n)| *n);
        if let Some(close) = crate::suggest::closest(name, earlier) {
            d = d.with_help(format!("did you mean `{}`? `{} = ...` declares a new variable", close, name));
        }
        out.push((d, name.to_string()));
    }
    out
}

/// Where statements declare variables (`x = ...`, `var x = ...`, `(a, b) = ...`).
struct Lets(HashSet<Span>);

impl Visitor for Lets {
    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { name, .. } if !name.starts_with("__") => {
                self.0.insert(s.pos);
            }
            StmtKind::LetTuple { names, .. } => self.0.extend(names.iter().map(|(_, p)| *p)),
            _ => {}
        }
    }
}

/// A warning for every use of something marked `#[deprecated]`.
fn deprecated(prog: &Program, m: &Model, index: &Index) -> Vec<(Diagnostic, String)> {
    if m.deprecated.is_empty() {
        return vec![];
    }
    let mut uses: Vec<(Span, String, Symbol)> =
        index.refs.iter().filter(|r| !r.is_decl).map(|r| (r.span, r.name.clone(), r.symbol.clone())).collect();
    let mut types = Types(vec![]);
    visit::program(prog, &mut types);
    uses.extend(types.0);
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (span, name, symbol) in uses {
        let Some((shown, note)) = deprecation(m, &symbol) else { continue };
        if !seen.insert((span, name.clone())) {
            continue;
        }
        let message = match note {
            Some(note) => format!("`{}` is deprecated: {}", shown, note),
            None => format!("`{}` is deprecated", shown),
        };
        out.push((Diagnostic::warning(message, span), name));
    }
    out.sort_by_key(|(d, _)| (d.span.file, d.span.line, d.span.col));
    out
}

/// The named types written in the program.
struct Types(Vec<(Span, String, Symbol)>);

impl Visitor for Types {
    fn ty(&mut self, t: &TypeExpr) {
        if let TypeExpr::Named(name, _, span) = t {
            self.0.push((*span, name.clone(), Symbol::Type(name.clone())));
        }
    }
}

/// How a deprecated symbol is named in the warning, and its message. A method or field is
/// also deprecated when the base class or interface that declares it marks it.
fn deprecation(m: &Model, symbol: &Symbol) -> Option<(String, Option<String>)> {
    let found = |s: &Symbol| m.deprecated.get(s).cloned();
    match symbol {
        Symbol::Function(n) | Symbol::Const(n) | Symbol::Type(n) => found(symbol).map(|note| (n.clone(), note)),
        Symbol::Member { owner, name } => {
            let mut owners = vec![owner.clone()];
            if m.classes.contains_key(owner) {
                owners = m.ancestors(owner);
                owners.extend(m.all_interfaces(owner));
            }
            owners.into_iter().find_map(|o| {
                let note = found(&Symbol::Member { owner: o.clone(), name: name.clone() })?;
                Some((format!("{}.{}", o, name), note))
            })
        }
        _ => None,
    }
}
