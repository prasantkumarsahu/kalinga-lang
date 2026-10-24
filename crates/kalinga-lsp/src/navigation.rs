//! Go to definition, find references, rename and the document outline.

use std::collections::HashMap;

use kalinga_sema::Model;
use kalinga_sema::index::Symbol;
use kalinga_syntax::error::Span;
use lsp_types::*;

use crate::snapshot::{DeclKind, Name, Snapshot};
use crate::text;
use crate::world::World;

/// The name at a position, in the latest analysis with valid declarations.
fn name_at(world: &mut World, at: &TextDocumentPositionParams) -> Option<(std::rc::Rc<Snapshot>, Name)> {
    let snap = world.good_snapshot(&at.text_document.uri)?;
    let (line, col) = text::line_col(snap.source(0), at.position);
    let name = snap.name_at(0, line, col)?.clone();
    Some((snap, name))
}

pub fn definition(world: &mut World, p: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
    let uri = p.text_document_position_params.text_document.uri.clone();
    let (snap, name) = name_at(world, &p.text_document_position_params)?;
    let span = match &name.symbol {
        Symbol::Local { .. } => snap.occurrences(&name.symbol).find(|n| n.is_decl)?.span,
        other => snap.decl(other)?.span,
    };
    Some(GotoDefinitionResponse::Scalar(location(&snap, &uri, span)?))
}

pub fn references(world: &mut World, p: ReferenceParams) -> Option<Vec<Location>> {
    let uri = p.text_document_position.text_document.uri.clone();
    let (snap, name) = name_at(world, &p.text_document_position)?;
    let with_decl = p.context.include_declaration;
    let spans = related(&snap, &name.symbol).into_iter().filter(|n| with_decl || !n.is_decl).map(|n| n.span);
    Some(spans.filter_map(|s| location(&snap, &uri, s)).collect())
}

pub fn prepare_rename(world: &mut World, p: TextDocumentPositionParams) -> Option<PrepareRenameResponse> {
    let (snap, name) = name_at(world, &p)?;
    if !renamable(&snap, &name.symbol) {
        return None;
    }
    let range = text::range(snap.source(0), name.span);
    let placeholder = text::line(snap.source(0), range.start.line as usize)
        .chars()
        .skip(name.span.col - 1)
        .take(name.span.len)
        .collect();
    Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder })
}

pub fn rename(world: &mut World, p: RenameParams) -> Option<WorkspaceEdit> {
    let uri = p.text_document_position.text_document.uri.clone();
    let (snap, name) = name_at(world, &p.text_document_position)?;
    if !renamable(&snap, &name.symbol) || !is_identifier(&p.new_name) {
        return None;
    }
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for n in related(&snap, &name.symbol) {
        let Some(loc) = location(&snap, &uri, n.span) else { continue };
        changes.entry(loc.uri).or_default().push(TextEdit::new(loc.range, p.new_name.clone()));
    }
    Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() })
}

/// Only names declared in the user's own files (not built-ins, libraries or `std/`).
fn renamable(snap: &Snapshot, symbol: &Symbol) -> bool {
    let file = match symbol {
        Symbol::Builtin { .. } | Symbol::Library { .. } => return false,
        Symbol::Local { decl, .. } => decl.file,
        other => match snap.decl(other) {
            Some(d) => d.span.file,
            None => return false,
        },
    };
    !snap.analysis.units.file_unit.get(file as usize).is_some_and(|u| u.starts_with("std_"))
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    first_ok && chars.all(|c| c.is_alphanumeric() || c == '_') && !s.starts_with("__") && !is_keyword(s)
}

fn is_keyword(s: &str) -> bool {
    kalinga_syntax::lexer::lex(s, 0)
        .is_ok_and(|ts| !matches!(ts.first().map(|t| &t.tok), Some(kalinga_syntax::token::Tok::Ident(_))))
}

/// Every place naming `symbol`; for a method, also the methods it overrides or implements
/// and those overriding or implementing it, which must keep the same name.
fn related<'a>(snap: &'a Snapshot, symbol: &Symbol) -> Vec<&'a Name> {
    let family: Vec<Symbol> = match (symbol, snap.model()) {
        (Symbol::Member { owner, name }, Some(m)) if is_method(m, owner, name) => family(m, owner)
            .into_iter()
            .filter(|o| is_method(m, o, name))
            .map(|o| Symbol::Member { owner: o, name: name.clone() })
            .collect(),
        _ => vec![symbol.clone()],
    };
    snap.names.iter().filter(|n| family.contains(&n.symbol)).collect()
}

fn is_method(m: &Model, owner: &str, name: &str) -> bool {
    m.classes.get(owner).is_some_and(|c| c.methods.contains_key(name))
        || m.interfaces.get(owner).is_some_and(|i| i.methods.iter().any(|(n, _)| n == name))
}

/// The classes and interfaces connected to `owner` by `:` (in either direction).
fn family(m: &Model, owner: &str) -> Vec<String> {
    let mut edges: Vec<(String, String)> = vec![];
    for (c, info) in &m.classes {
        edges.extend(info.parent.iter().map(|p| (c.clone(), p.clone())));
        edges.extend(info.interfaces.iter().map(|i| (c.clone(), i.clone())));
    }
    for (i, info) in &m.interfaces {
        edges.extend(info.supers.iter().map(|s| (i.clone(), s.clone())));
    }
    let mut out = vec![owner.to_string()];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        for (a, b) in &edges {
            for (from, to) in [(a, b), (b, a)] {
                if *from == cur && !out.contains(to) {
                    out.push(to.clone());
                }
            }
        }
        i += 1;
    }
    out
}

/// Where a span is, as a location in its file.
fn location(snap: &Snapshot, root: &Url, span: Span) -> Option<Location> {
    let uri = if span.file == 0 {
        root.clone()
    } else {
        Url::from_file_path(snap.analysis.paths.get(span.file as usize)?.as_ref()?).ok()?
    };
    Some(Location::new(uri, text::range(snap.source(span.file), span)))
}

/// The outline of the document: its declarations, with members inside their types.
pub fn symbols(world: &mut World, p: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
    let snap = world.good_snapshot(&p.text_document.uri)?;
    let source = snap.source(0);
    // (the type a top-level entry declares, the entry)
    let mut top: Vec<(Option<String>, DocumentSymbol)> = vec![];
    for d in snap.decls.iter().filter(|d| d.span.file == 0) {
        let (name, owner) = match &d.symbol {
            Symbol::Member { owner, name } => (name, Some(owner)),
            Symbol::Function(n) | Symbol::Type(n) | Symbol::Const(n) => (n, None),
            _ => continue,
        };
        let range = Range::new(text::position(source, d.start.line, d.start.col), text::range(source, d.span).end);
        #[allow(deprecated)]
        let sym = DocumentSymbol {
            name: snap.show(name),
            detail: None,
            kind: symbol_kind(d.kind),
            tags: None,
            deprecated: None,
            range,
            selection_range: text::range(source, d.span),
            children: None,
        };
        match owner.and_then(|o| top.iter_mut().find(|(t, _)| t.as_ref() == Some(o))) {
            Some((_, parent)) => parent.children.get_or_insert_with(Vec::new).push(sym),
            None => {
                let declares = matches!(d.symbol, Symbol::Type(_)).then(|| name.clone());
                top.push((declares, sym));
            }
        }
    }
    Some(DocumentSymbolResponse::Nested(top.into_iter().map(|(_, s)| s).collect()))
}

fn symbol_kind(k: DeclKind) -> SymbolKind {
    match k {
        DeclKind::Function => SymbolKind::FUNCTION,
        DeclKind::Class => SymbolKind::CLASS,
        DeclKind::Struct => SymbolKind::STRUCT,
        DeclKind::Interface => SymbolKind::INTERFACE,
        DeclKind::Enum => SymbolKind::ENUM,
        DeclKind::Variant => SymbolKind::ENUM_MEMBER,
        DeclKind::Field => SymbolKind::FIELD,
        DeclKind::Method => SymbolKind::METHOD,
        DeclKind::Const => SymbolKind::CONSTANT,
        DeclKind::StaticVar => SymbolKind::VARIABLE,
        DeclKind::ExternType => SymbolKind::TYPE_PARAMETER,
    }
}
