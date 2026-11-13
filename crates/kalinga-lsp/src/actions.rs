//! Quick fixes for the compiler's messages about `override`, unwritten methods and
//! changing something fixed.

use std::collections::HashMap;

use lsp_types::*;

use crate::text;
use crate::world::World;

pub fn actions(world: &mut World, p: CodeActionParams) -> Option<CodeActionResponse> {
    let uri = p.text_document.uri;
    let source = world.text(&uri)?.to_string();
    let snap = world.snapshot(&uri);
    let mut out = vec![];
    for d in &p.context.diagnostics {
        let line_no = d.range.start.line as usize;
        let line = text::line(&source, line_no);
        let fix = if d.message.contains("; mark it `override fn`") {
            line.find("fn ").map(|at| ("Mark it `override fn`", insert(line_no, line, at, "override ")))
        } else if d.message.contains("is marked override but no base class") {
            line.find("override ").map(|at| ("Remove `override`", delete(line_no, line, at, "override ".len())))
        } else if let Some(stub) = missing_method(&d.message, snap.as_deref()) {
            class_end(&source, line_no).map(|end| ("Add the missing method", insert(end, "", 0, &stub)))
        } else {
            make_var(&d.message, d.range.start, snap.as_deref(), &source).map(|edit| ("Make it `var`", edit))
        };
        if let Some((title, edit)) = fix {
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![d.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
                    ..WorkspaceEdit::default()
                }),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        }
    }
    Some(out)
}

/// Inserts `new` at character `at` (a byte index into `line`) of line `line_no`.
fn insert(line_no: usize, line: &str, at: usize, new: &str) -> TextEdit {
    let col = line[..at].encode_utf16().count() as u32;
    let pos = Position::new(line_no as u32, col);
    TextEdit::new(Range::new(pos, pos), new.to_string())
}

fn delete(line_no: usize, line: &str, at: usize, len: usize) -> TextEdit {
    let start = line[..at].encode_utf16().count() as u32;
    let end = line[..at + len].encode_utf16().count() as u32;
    TextEdit::new(Range::new(Position::new(line_no as u32, start), Position::new(line_no as u32, end)), String::new())
}

/// For "cannot change `x`: it is fixed ..." (a variable) or "... it is a fixed field of `C`":
/// `var ` before the declaration, when it is in this file.
fn make_var(message: &str, at: Position, snap: Option<&crate::snapshot::Snapshot>, source: &str) -> Option<TextEdit> {
    use kalinga_sema::index::Symbol;
    let name = message.strip_prefix("cannot change `")?.split('`').next()?;
    let snap = snap?;
    let decl = if let Some(rest) = message.split_once("it is a fixed field of `") {
        let owner = rest.1.split('`').next()?;
        let symbol = Symbol::Member { owner: owner.to_string(), name: name.to_string() };
        snap.decls.iter().find(|d| d.kind == crate::snapshot::DeclKind::Field && d.symbol == symbol)?.span
    } else if message.contains(": it is fixed (declare it `var") {
        // The use at `at` when the index has it, else the nearest earlier declaration.
        let used = snap.names.iter().find_map(|n| match &n.symbol {
            Symbol::Local { decl, .. } if n.span.file == 0 && text::range(source, n.span).start == at => Some(*decl),
            _ => None,
        });
        let earlier = |n: &crate::snapshot::Name| {
            let p = text::range(source, n.span).start;
            n.is_decl
                && n.span.file == 0
                && matches!(&n.symbol, Symbol::Local { name: x, .. } if x == name)
                && (p.line, p.character) < (at.line, at.character)
        };
        used.or_else(|| snap.names.iter().rfind(|n| earlier(n)).map(|n| n.span))?
    } else {
        return None;
    };
    if decl.file != 0 {
        return None;
    }
    // Not inside `(a, b) = ...`, where `var` goes before the parenthesis.
    let line = text::line(source, decl.line.saturating_sub(1));
    let before = line.chars().take(decl.col.saturating_sub(1)).collect::<String>();
    if before.trim_end().ends_with(['(', ',']) {
        return None;
    }
    let pos = text::position(source, decl.line, decl.col);
    Some(TextEdit::new(Range::new(pos, pos), "var ".to_string()))
}

/// The stub to add for "`C` does not implement `I.m(a: int) -> str`" or "`C` must
/// implement abstract method `Base.m`".
fn missing_method(message: &str, snap: Option<&crate::snapshot::Snapshot>) -> Option<String> {
    let quoted: Vec<&str> = message.split('`').skip(1).step_by(2).collect();
    let header = if message.contains("does not implement") {
        let (_, method) = quoted.get(1)?.split_once('.')?;
        method.strip_suffix(" -> void").unwrap_or(method).to_string()
    } else if message.contains("must implement abstract method") {
        let (owner, name) = quoted.get(1)?.split_once('.')?;
        let m = snap?.model()?;
        let sig = m.classes.get(owner)?.methods.get(name)?;
        format!("{}{}", name, crate::describe::params_and_ret(sig))
    } else {
        return None;
    };
    Some(format!("\n    override fn {} {{\n    }}\n", header))
}

/// `line` without its comment and the text of its strings.
fn code_only(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (in_string, c) {
            (false, '/') if chars.peek() == Some(&'/') => break,
            (true, '\\') => {
                chars.next();
            }
            (_, '"') => in_string = !in_string,
            (false, c) => out.push(c),
            (true, _) => {}
        }
    }
    out
}

/// The line of the `}` closing the class declared on line `line_no`.
fn class_end(source: &str, line_no: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, line) in source.lines().enumerate().skip(line_no) {
        for c in code_only(line).chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}
