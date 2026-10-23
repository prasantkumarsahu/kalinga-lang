//! One analysis of a document and the files it imports, with every name located exactly:
//! uses and declarations, each with the symbol it stands for.

use std::collections::HashSet;

use kalinga_driver::Analysis;
use kalinga_sema::index::Symbol;
use kalinga_sema::{Model, Type};
use kalinga_syntax::ast::*;
use kalinga_syntax::error::Span;
use kalinga_syntax::token::{Tok, Token};
use kalinga_syntax::{lexer, visit};

/// A name in the source and what it stands for.
#[derive(Debug, Clone)]
pub struct Name {
    /// Exactly the name's characters.
    pub span: Span,
    pub symbol: Symbol,
    /// The type of the expression, or of the declared variable.
    pub ty: Option<Type>,
    /// Where the symbol is declared, not a use of it.
    pub is_decl: bool,
}

/// What kind of declaration a symbol has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Function,
    Class,
    Struct,
    Interface,
    Enum,
    Variant,
    Field,
    Method,
    Const,
    StaticVar,
    ExternType,
}

/// A declaration of a top-level item or a member.
#[derive(Debug, Clone)]
pub struct Decl {
    pub symbol: Symbol,
    pub kind: DeclKind,
    /// The declared name, exactly.
    pub span: Span,
    /// Where the whole declaration starts (its doc comment is above this line).
    pub start: Span,
}

pub struct Snapshot {
    pub analysis: Analysis,
    pub names: Vec<Name>,
    pub decls: Vec<Decl>,
    /// The tokens of each file (by id); empty for a file that does not lex.
    tokens: Vec<Vec<Token>>,
    /// Declarations the analysed file reaches only as `alias.Name`: (internal name, alias).
    qualified: Vec<(String, String)>,
}

impl Snapshot {
    pub fn new(analysis: Analysis) -> Snapshot {
        let tokens: Vec<Vec<Token>> = (0..analysis.sources.len() as u32)
            .map(|id| lexer::lex(analysis.sources.file(id).1, id).unwrap_or_default())
            .collect();
        let finder = Finder { tokens: &tokens };
        let mut decls = vec![];
        if let Some(prog) = &analysis.program {
            declarations(prog, &finder, &mut decls);
        }
        let mut names: Vec<Name> =
            decls.iter().map(|d| Name { span: d.span, symbol: d.symbol.clone(), ty: None, is_decl: true }).collect();
        for r in &analysis.index.refs {
            if let Some(span) = finder.name(r.span, &r.name) {
                names.push(Name { span, symbol: r.symbol.clone(), ty: Some(r.ty.clone()), is_decl: r.is_decl });
            }
        }
        if let (Some(prog), Some(model)) = (&analysis.program, &analysis.model) {
            let mut types = TypeNames { finder: &finder, model, names: &mut names };
            visit::program(prog, &mut types);
            type_headers(prog, &mut types);
        }
        // One entry per place: a declaration wins over a use recorded at the same name.
        names.sort_by_key(|n| (n.span.file, n.span.line, n.span.col, !n.is_decl));
        names.dedup_by_key(|n| (n.span.file, n.span.line, n.span.col));
        let mut snap = Snapshot { analysis, names, decls, tokens, qualified: vec![] };
        snap.qualified = snap.qualified_names();
        snap
    }

    /// The units whose declarations the analysed file sees without a package prefix: its
    /// own and those it imports without `as`, with what they import.
    pub fn visible_units(&self) -> HashSet<String> {
        let units = &self.analysis.units;
        let mut out: Vec<String> = units.file_unit.first().cloned().into_iter().collect();
        let mut i = 0;
        while i < out.len() {
            for u in units.imports.get(&out[i]).into_iter().flatten() {
                if !out.contains(u) {
                    out.push(u.clone());
                }
            }
            i += 1;
        }
        out.into_iter().collect()
    }

    /// The unit of the file a declaration is in.
    pub fn unit_of(&self, d: &Decl) -> Option<&String> {
        self.analysis.units.file_unit.get(d.span.file as usize)
    }

    fn qualified_names(&self) -> Vec<(String, String)> {
        let visible = self.visible_units();
        let Some(aliases) = self.analysis.units.file_aliases.first() else { return vec![] };
        let mut out = vec![];
        for (alias, unit) in aliases.iter().filter(|(_, u)| !visible.contains(*u)) {
            for d in self.decls.iter().filter(|d| self.unit_of(d) == Some(unit)) {
                if let Symbol::Function(n) | Symbol::Type(n) | Symbol::Const(n) = &d.symbol {
                    out.push((n.clone(), alias.clone()));
                }
            }
        }
        out
    }

    /// The identifier token at a 1-based position of `file` (not inside a comment or string).
    pub fn ident_at(&self, file: u32, line: usize, col: usize) -> Option<(String, Span)> {
        self.tokens.get(file as usize)?.iter().find_map(|t| match &t.tok {
            Tok::Ident(n) if crate::text::contains(t.pos, line, col) => Some((n.clone(), t.pos)),
            _ => None,
        })
    }

    /// The path of `import "path" as alias` in the analysed file.
    pub fn import_of_alias(&self, alias: &str) -> Option<String> {
        let prog = self.analysis.program.as_ref()?;
        prog.imports.iter().find(|i| i.pos.file == 0 && i.alias.as_deref() == Some(alias)).map(|i| i.path.clone())
    }

    pub fn model(&self) -> Option<&Model> {
        self.analysis.model.as_ref()
    }

    pub fn source(&self, file: u32) -> &str {
        self.analysis.sources.file(file).1
    }

    /// `text` as the analysed file would write it: `alias.Vec2` for what it imports with
    /// `as`, and package names (`mathx.Vec2`) in place of other internal ones.
    pub fn show(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (name, alias) in &self.qualified {
            let written = name.split("__").next().unwrap_or(name);
            out = replace_word(&out, name, &format!("{}.{}", alias, written));
        }
        self.analysis.sources.show(&out)
    }

    /// The name at a 1-based position of `file`.
    pub fn name_at(&self, file: u32, line: usize, col: usize) -> Option<&Name> {
        self.names.iter().find(|n| n.span.file == file && crate::text::contains(n.span, line, col))
    }

    pub fn decl(&self, symbol: &Symbol) -> Option<&Decl> {
        self.decls.iter().find(|d| &d.symbol == symbol)
    }

    /// Every place `symbol` is named, its declaration included.
    pub fn occurrences<'a>(&'a self, symbol: &'a Symbol) -> impl Iterator<Item = &'a Name> {
        self.names.iter().filter(move |n| &n.symbol == symbol)
    }

    /// The `///` comment above the line `start` is on, without the slashes.
    pub fn docs(&self, start: Span) -> Option<String> {
        let source = self.source(start.file);
        let lines: Vec<&str> = source.lines().take(start.line.saturating_sub(1)).collect();
        let doc: Vec<&str> = lines
            .iter()
            .rev()
            .take_while(|l| l.trim_start().starts_with("///"))
            .map(|l| {
                let text = l.trim_start().trim_start_matches("///");
                text.strip_prefix(' ').unwrap_or(text)
            })
            .collect();
        if doc.is_empty() {
            return None;
        }
        Some(doc.into_iter().rev().collect::<Vec<_>>().join("\n"))
    }
}

/// `text` with the whole word `word` replaced by `with` (not inside longer names, nor
/// after a `.`, where it is already qualified).
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let is_name = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(word) {
        let before = rest[..i].chars().next_back();
        let after = rest[i + word.len()..].chars().next();
        out.push_str(&rest[..i]);
        if before.is_some_and(|c| is_name(c) || c == '.') || after.is_some_and(is_name) {
            out.push_str(word);
        } else {
            out.push_str(with);
        }
        rest = &rest[i + word.len()..];
    }
    out.push_str(rest);
    out
}

/// Finds a name's exact span from where its expression or declaration starts.
struct Finder<'a> {
    tokens: &'a [Vec<Token>],
}

impl Finder<'_> {
    /// The first identifier `name` at or after `from` (names renamed for packages,
    /// `Vec2__mathx`, are found as written: `Vec2`).
    fn name(&self, from: Span, name: &str) -> Option<Span> {
        let written = name.split("__").next().unwrap_or(name);
        let tokens = self.tokens.get(from.file as usize)?;
        tokens
            .iter()
            .filter(|t| (t.pos.line, t.pos.col) >= (from.line, from.col))
            .find(|t| matches!(&t.tok, Tok::Ident(n) if n == written))
            .map(|t| t.pos)
    }
}

fn declarations(prog: &Program, f: &Finder, out: &mut Vec<Decl>) {
    let mut add = |symbol: Symbol, kind: DeclKind, start: Span, name: &str| {
        if let Some(span) = f.name(start, name) {
            out.push(Decl { symbol, kind, span, start });
        }
    };
    let member = |owner: &str, name: &str| Symbol::Member { owner: owner.to_string(), name: name.to_string() };
    for x in &prog.functions {
        add(Symbol::Function(x.name.clone()), DeclKind::Function, x.pos, &x.name);
    }
    for x in &prog.extern_fns {
        add(Symbol::Function(x.name.clone()), DeclKind::Function, x.pos, &x.name);
    }
    for (n, pos) in &prog.extern_types {
        add(Symbol::Type(n.clone()), DeclKind::ExternType, *pos, n);
    }
    for c in &prog.consts {
        add(Symbol::Const(c.name.clone()), DeclKind::Const, c.pos, &c.name);
    }
    for c in &prog.classes {
        add(Symbol::Type(c.name.clone()), DeclKind::Class, c.pos, &c.name);
        for x in &c.fields {
            add(member(&c.name, &x.name), DeclKind::Field, x.pos, &x.name);
        }
        for m in &c.methods {
            add(member(&c.name, &m.name), DeclKind::Method, m.pos, &m.name);
        }
        for s in &c.statics {
            add(member(&c.name, &s.name), static_kind(s), s.pos, &s.name);
        }
    }
    for s in &prog.structs {
        add(Symbol::Type(s.name.clone()), DeclKind::Struct, s.pos, &s.name);
        for x in &s.fields {
            add(member(&s.name, &x.name), DeclKind::Field, x.pos, &x.name);
        }
        for m in &s.methods {
            add(member(&s.name, &m.name), DeclKind::Method, m.pos, &m.name);
        }
        for st in &s.statics {
            add(member(&s.name, &st.name), static_kind(st), st.pos, &st.name);
        }
    }
    for i in &prog.interfaces {
        add(Symbol::Type(i.name.clone()), DeclKind::Interface, i.pos, &i.name);
        for m in &i.methods {
            add(member(&i.name, &m.name), DeclKind::Method, m.pos, &m.name);
        }
    }
    for e in &prog.enums {
        add(Symbol::Type(e.name.clone()), DeclKind::Enum, e.pos, &e.name);
        for v in &e.variants {
            add(member(&e.name, &v.name), DeclKind::Variant, v.pos, &v.name);
        }
    }
}

fn static_kind(s: &StaticDecl) -> DeclKind {
    if s.mutable { DeclKind::StaticVar } else { DeclKind::Const }
}

/// Records the types written in annotations, `as` / `is`, and after `:` in headers.
struct TypeNames<'a, 'b> {
    finder: &'a Finder<'a>,
    model: &'a Model,
    names: &'b mut Vec<Name>,
}

impl TypeNames<'_, '_> {
    fn add(&mut self, name: &str, from: Span) {
        if self.model.names.contains(name)
            && let Some(span) = self.finder.name(from, name)
        {
            self.names.push(Name { span, symbol: Symbol::Type(name.to_string()), ty: None, is_decl: false });
        }
    }
}

impl visit::Visitor for TypeNames<'_, '_> {
    fn ty(&mut self, t: &TypeExpr) {
        if let TypeExpr::Named(n, _, pos) = t {
            self.add(n, *pos);
        }
    }

    fn expr(&mut self, e: &Expr) {
        if let ExprKind::Downcast { class, .. } = &e.kind {
            self.add(class, e.pos);
        }
    }
}

/// The names after `:` in class and interface headers.
fn type_headers(prog: &Program, types: &mut TypeNames) {
    let bases = prog.classes.iter().flat_map(|c| &c.bases).chain(prog.interfaces.iter().flat_map(|i| &i.bases));
    for (name, pos) in bases {
        types.add(name, *pos);
    }
}
