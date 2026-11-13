//! Warnings: problems that do not stop a program from building. So far, uses of
//! declarations marked `#[deprecated]`.

use std::collections::HashSet;

use kalinga_syntax::ast::{Program, TypeExpr};
use kalinga_syntax::error::{Diagnostic, Span};
use kalinga_syntax::visit::{self, Visitor};

use crate::index::{Index, Symbol};
use crate::model::Model;

/// A warning for every use of something marked `#[deprecated]` (in code and in written
/// types), with the name used. Each points where the use starts; the driver moves it onto
/// the name.
pub fn warnings(prog: &Program, m: &Model, index: &Index) -> Vec<(Diagnostic, String)> {
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
