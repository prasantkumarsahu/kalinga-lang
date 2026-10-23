//! Opening a document and asking the server about the position marked `|`.

#![allow(dead_code)]

use std::path::PathBuf;

use kalinga_lsp::world::World;
use lsp_types::*;

/// A folder of its own for a test's files.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("kalinga-lsp-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// The text without the `|` marker, and the marker's position.
pub fn marked(src: &str) -> (String, Position) {
    let offset = src.find('|').expect("a | marker");
    let before = &src[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before[before.rfind('\n').map_or(0, |i| i + 1)..].encode_utf16().count() as u32;
    (src.replacen('|', "", 1), Position::new(line, col))
}

/// A world with `text` open as `main.kl` in `dir` (a file that need not exist).
pub fn open(dir: &std::path::Path, text: &str) -> (World, Url) {
    let uri = Url::from_file_path(dir.join("main.kl")).expect("file url");
    let mut world = World::default();
    world.open(uri.clone(), text.to_string());
    (world, uri)
}

pub fn at(uri: &Url, pos: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri: uri.clone() }, position: pos }
}

/// Completion items at `|` in `src`.
pub fn complete_in(dir: &std::path::Path, src: &str) -> Vec<CompletionItem> {
    let (text, pos) = marked(src);
    let (mut world, uri) = open(dir, &text);
    let params = CompletionParams {
        text_document_position: at(&uri, pos),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    };
    match kalinga_lsp::completion::complete(&mut world, params) {
        Some(CompletionResponse::Array(items)) => items,
        _ => vec![],
    }
}

pub fn complete(src: &str) -> Vec<CompletionItem> {
    complete_in(&std::env::temp_dir(), src)
}

/// The labels, sorted.
pub fn labels(items: &[CompletionItem]) -> Vec<String> {
    let mut l: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    l.sort();
    l
}

pub fn find<'a>(items: &'a [CompletionItem], label: &str) -> &'a CompletionItem {
    items.iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no item `{}` in {:?}", label, labels(items)))
}

pub fn docs(item: &CompletionItem) -> Option<String> {
    match &item.documentation {
        Some(Documentation::MarkupContent(m)) => Some(m.value.clone()),
        Some(Documentation::String(s)) => Some(s.clone()),
        None => None,
    }
}

/// Signature help at `|` in `src`: (label, active parameter).
pub fn signature_in(dir: &std::path::Path, src: &str) -> Option<(String, u32)> {
    let (text, pos) = marked(src);
    let (mut world, uri) = open(dir, &text);
    let params = SignatureHelpParams {
        context: None,
        text_document_position_params: at(&uri, pos),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let help = kalinga_lsp::signature::help(&mut world, params)?;
    let sig = help.signatures.into_iter().next()?;
    Some((sig.label, help.active_parameter.unwrap_or(0)))
}

pub fn signature(src: &str) -> Option<(String, u32)> {
    signature_in(&std::env::temp_dir(), src)
}

/// Hover text at the `nth` occurrence of `word` (on its second character), and the
/// length of the range it covers.
pub fn hover_in(dir: &std::path::Path, src: &str, word: &str, nth: usize) -> Option<(String, u32)> {
    let start = src.match_indices(word).nth(nth).map(|(i, _)| i).unwrap_or_else(|| panic!("`{}` not found", word));
    let at_byte = start + 1;
    let marked_src = format!("{}|{}", &src[..at_byte], &src[at_byte..]);
    let (text, pos) = marked(&marked_src);
    let (mut world, uri) = open(dir, &text);
    let h = kalinga_lsp::hover::hover(
        &mut world,
        HoverParams {
            text_document_position_params: at(&uri, pos),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )?;
    let value = match h.contents {
        HoverContents::Markup(m) => m.value,
        _ => String::new(),
    };
    let len = h.range.map_or(0, |r| r.end.character - r.start.character);
    Some((value, len))
}

pub fn hover(src: &str, word: &str, nth: usize) -> Option<(String, u32)> {
    hover_in(&std::env::temp_dir(), src, word, nth)
}
