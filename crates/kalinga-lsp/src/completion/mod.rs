//! Completion: what can be written at the cursor.

mod context;
mod items;
pub(crate) mod probe;

use std::collections::HashSet;

use kalinga_sema::index::{PROBE, Probe};
use kalinga_syntax::lexer;
use kalinga_syntax::token::Tok;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, CompletionTextEdit, Range, TextEdit, Url,
};

use crate::text;
use crate::world::World;
use context::{Context, Opener, context};
use items::*;

pub fn complete(world: &mut World, p: CompletionParams) -> Option<CompletionResponse> {
    let uri = p.text_document_position.text_document.uri;
    let text = world.text(&uri)?.to_string();
    let offset = text::offset(&text, p.text_document_position.position);
    let word_start = text[..offset].trim_end_matches(|c: char| c.is_alphanumeric() || c == '_').len();
    let items = items_at(world, &uri, &text, word_start, offset);
    Some(CompletionResponse::Array(items))
}

/// The items for the cursor at `offset`, with the identifier being typed starting at
/// `word_start`.
pub fn items_at(world: &mut World, uri: &Url, text: &str, word_start: usize, offset: usize) -> Vec<CompletionItem> {
    let good = world.stable_snapshot(uri, text::line_index(text, offset));
    match context(text, offset, word_start) {
        Context::Nothing => vec![],
        Context::ImportPath => import_paths(uri, text, offset),
        Context::TopLevel => top_level(),
        Context::Type => good.map(|s| types(&s)).unwrap_or_default(),
        Context::ClassHeader => good.map(|s| bases(&s)).unwrap_or_default(),
        Context::Body(opener) => {
            let written = written_methods(text, offset);
            good.map(|s| body(&s, &opener, &written)).unwrap_or_default()
        }
        Context::MatchArm { scrutinee } => {
            let insert = format!("_ => {}.{}", scrutinee, PROBE);
            match probe::probe_with(world, uri, text, word_start, offset, &insert) {
                Some((snap, Probe::Members { ty, .. })) => patterns(&snap, &ty),
                _ => vec![],
            }
        }
        Context::Member { recv } => member(world, uri, text, word_start, offset, &recv),
        Context::Expression => expression(world, uri, text, word_start, offset),
    }
}

fn member(
    world: &mut World,
    uri: &Url,
    text: &str,
    word_start: usize,
    offset: usize,
    recv: &str,
) -> Vec<CompletionItem> {
    let good = world.stable_snapshot(uri, text::line_index(text, offset));
    if let Some(snap) = &good
        && snap.import_of_alias(recv).is_some()
    {
        return package(snap, recv);
    }
    if recv == "super" {
        return good.map(|s| super_methods(&s, text, offset)).unwrap_or_default();
    }
    match probe::probe(world, uri, text, word_start, offset) {
        Some((snap, Probe::Members { ty, on_self })) => members(&snap, &ty, on_self),
        Some((snap, Probe::TypeMembers(t))) => type_members(&snap, &t),
        Some((_, Probe::Library(l))) => library(&l),
        _ => vec![],
    }
}

fn expression(world: &mut World, uri: &Url, text: &str, word_start: usize, offset: usize) -> Vec<CompletionItem> {
    let probed = probe::probe(world, uri, text, word_start, offset);
    let mut out = vec![];
    if let Some((_, Probe::Scope { locals, owner, is_static, .. })) = &probed {
        for (name, ty) in locals.iter().filter(|(n, _)| n != PROBE) {
            out.push(item(name, CompletionItemKind::VARIABLE, Some(format!("{}: {}", name, ty)), None));
        }
        if owner.is_some() && !is_static {
            out.push(item("self", CompletionItemKind::KEYWORD, None, None));
        }
    }
    match &probed {
        Some((snap, _)) if snap.model().is_some() => out.extend(globals(snap, None)),
        _ => {
            if let Some(snap) = world.stable_snapshot(uri, text::line_index(text, offset)) {
                out.extend(globals(&snap, None));
            }
        }
    }
    if let Some(snap) = world.stable_snapshot(uri, text::line_index(text, offset)) {
        out.extend(aliases(&snap));
    }
    out.extend(language());
    out
}

/// `import "...": the `.kl` files next to this one (and in folders below it), the
/// standard modules and the project's packages. Each replaces the whole path typed so far,
/// so the editor matches against all of it (`/` and `.` included).
fn import_paths(uri: &Url, text: &str, offset: usize) -> Vec<CompletionItem> {
    let this_file = uri.to_file_path().ok();
    let mut out: Vec<CompletionItem> = vec![];
    if let Some(dir) = this_file.as_ref().and_then(|f| f.parent()) {
        let mut files = vec![];
        kl_files_below(dir, dir, 0, &mut files);
        files.sort();
        for (path, rel) in files {
            if this_file.as_ref().is_some_and(|f| *f == path) {
                continue;
            }
            out.push(item(&rel, CompletionItemKind::FILE, Some("file".into()), None));
        }
    }
    out.extend(
        kalinga_driver::std_modules()
            .iter()
            .map(|m| item(&format!("std/{}", m), CompletionItemKind::MODULE, Some("standard module".into()), None)),
    );
    let project = this_file.as_deref().and_then(kalinga_project::find_project);
    if let Some(r) = project.and_then(|p| kalinga_project::resolve(&p, kalinga_project::Mode::Offline).ok()) {
        out.extend(r.packages.keys().map(|name| item(name, CompletionItemKind::MODULE, Some("package".into()), None)));
    }
    // From just after the opening `"` to the cursor.
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let quote = text[line_start..offset].find('"').map_or(offset, |i| line_start + i + 1);
    let (line, col) = text::line_col_of_offset(text, quote);
    let (end_line, end_col) = text::line_col_of_offset(text, offset);
    let range = Range::new(text::position(text, line, col), text::position(text, end_line, end_col));
    for (i, it) in out.iter_mut().enumerate() {
        it.text_edit = Some(CompletionTextEdit::Edit(TextEdit::new(range, it.label.clone())));
        it.filter_text = Some(it.label.clone());
        // Files first, then the standard modules and packages, each in order.
        it.sort_text = Some(format!("{:04}", i));
    }
    out
}

/// The `.kl` files in `dir` and its folders (a few levels down, skipping hidden folders and
/// build output), as (path, path relative to `base` written with `/`).
fn kl_files_below(
    base: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    out: &mut Vec<(std::path::PathBuf, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if depth < 3 && !name.starts_with('.') && !matches!(name.as_str(), "target" | "node_modules") {
                kl_files_below(base, &path, depth + 1, out);
            }
        } else if name.ends_with(".kl") {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let rel: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
            out.push((path.clone(), rel.join("/")));
        }
    }
}

/// The names given to packages with `import "..." as name`.
fn aliases(snap: &crate::snapshot::Snapshot) -> Vec<CompletionItem> {
    let Some(prog) = &snap.analysis.program else { return vec![] };
    prog.imports
        .iter()
        .filter(|i| i.pos.file == 0)
        .filter_map(|i| {
            let alias = i.alias.as_ref()?;
            Some(item(alias, CompletionItemKind::MODULE, Some(format!("import \"{}\" as {}", i.path, alias)), None))
        })
        .collect()
}

/// After `super.`: the methods of the base class of the class around the cursor.
fn super_methods(snap: &crate::snapshot::Snapshot, text: &str, offset: usize) -> Vec<CompletionItem> {
    let class = context::openers(&text[..offset]).into_iter().rev().find_map(|o| match o {
        Opener::Class { name, .. } => Some(name),
        _ => None,
    });
    let Some(parent) = class.and_then(|c| snap.model()?.classes.get(&c)?.parent.clone()) else { return vec![] };
    members(snap, &kalinga_sema::Type::Class(parent), false)
        .into_iter()
        .filter(|i| i.kind == Some(CompletionItemKind::METHOD))
        .collect()
}

/// The methods already written in the type body around the cursor (before or after it).
fn written_methods(text: &str, offset: usize) -> HashSet<String> {
    let tokens = lexer::lex(text, 0).or_else(|_| lexer::lex(&text[..offset], 0)).unwrap_or_default();
    let (line, col) = crate::text::line_col_of_offset(text, offset);
    // The innermost `{` open at the cursor, and the depth inside it.
    let mut stack: Vec<usize> = vec![];
    let mut body = None;
    for (i, t) in tokens.iter().enumerate() {
        if (t.pos.line, t.pos.col) >= (line, col) && body.is_none() {
            body = stack.last().copied();
        }
        match t.tok {
            Tok::LBrace => stack.push(i),
            Tok::RBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    let Some(start) = body.or(stack.last().copied()) else { return HashSet::new() };
    let mut depth = 0;
    let mut out = HashSet::new();
    for (i, t) in tokens.iter().enumerate().skip(start) {
        match &t.tok {
            Tok::LBrace => depth += 1,
            Tok::RBrace => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Tok::Fn if depth == 1 => {
                if let Some(Tok::Ident(n)) = tokens.get(i + 1).map(|t| &t.tok) {
                    out.insert(n.clone());
                }
            }
            _ => {}
        }
    }
    out
}
