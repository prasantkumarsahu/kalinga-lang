//! Quick fixes for the compiler's messages about `override` and unwritten methods.

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
            None
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
