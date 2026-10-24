//! The documents open in the editor and their analyses.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};

use crate::snapshot::Snapshot;
use crate::text;

/// An open document.
struct Doc {
    text: String,
    /// The analysis of the current text, made when first needed.
    current: Option<Rc<Snapshot>>,
}

#[derive(Default)]
pub struct World {
    docs: HashMap<Url, Doc>,
    /// The latest analysis of each document whose declarations were all valid, for
    /// features that need them while the current text has errors.
    last_good: HashMap<Url, Rc<Snapshot>>,
}

impl World {
    pub fn open(&mut self, uri: Url, text: String) {
        self.docs.insert(uri, Doc { text, current: None });
        // Files that import this one see the editor's text from now on.
        self.docs.values_mut().for_each(|d| d.current = None);
    }

    pub fn change(&mut self, uri: &Url, text: String) {
        if let Some(d) = self.docs.get_mut(uri) {
            d.text = text;
        }
        self.docs.values_mut().for_each(|d| d.current = None);
    }

    pub fn close(&mut self, uri: &Url) {
        self.docs.remove(uri);
        self.last_good.remove(uri);
        self.docs.values_mut().for_each(|d| d.current = None);
    }

    pub fn uris(&self) -> Vec<Url> {
        self.docs.keys().cloned().collect()
    }

    pub fn text(&self, uri: &Url) -> Option<&str> {
        self.docs.get(uri).map(|d| d.text.as_str())
    }

    /// The analysis of the document's current text.
    pub fn snapshot(&mut self, uri: &Url) -> Option<Rc<Snapshot>> {
        if let Some(s) = self.docs.get(uri)?.current.clone() {
            return Some(s);
        }
        let text = self.docs.get(uri)?.text.clone();
        let snap = Rc::new(self.analyze(uri, &text));
        if snap.analysis.model.is_some() {
            self.last_good.insert(uri.clone(), snap.clone());
        }
        if let Some(d) = self.docs.get_mut(uri) {
            d.current = Some(snap.clone());
        }
        Some(snap)
    }

    /// The analysis of the current text when its declarations are valid, else of the
    /// last text whose declarations were.
    pub fn good_snapshot(&mut self, uri: &Url) -> Option<Rc<Snapshot>> {
        let current = self.snapshot(uri)?;
        if current.analysis.model.is_some() {
            return Some(current);
        }
        self.last_good.get(uri).cloned().or(Some(current))
    }

    /// An analysis with valid declarations while line `line` (0-based) is being written:
    /// the current one, the last good one, or one of the text without that line.
    pub fn stable_snapshot(&mut self, uri: &Url, line: usize) -> Option<Rc<Snapshot>> {
        let current = self.good_snapshot(uri)?;
        if current.analysis.model.is_some() {
            return Some(current);
        }
        let text = self.text(uri)?;
        let without: String =
            text.split_inclusive('\n').enumerate().map(|(i, l)| if i == line { "\n" } else { l }).collect();
        let snap = Rc::new(self.analyze(uri, &without));
        if snap.analysis.model.is_some() {
            self.last_good.insert(uri.clone(), snap.clone());
            return Some(snap);
        }
        Some(current)
    }

    /// Analyses `text` as the document `uri` (which may be edited, for completion).
    pub fn analyze(&self, uri: &Url, text: &str) -> Snapshot {
        let open: HashMap<PathBuf, String> = self
            .docs
            .iter()
            .filter(|(u, _)| *u != uri)
            .filter_map(|(u, d)| Some((std::fs::canonicalize(u.to_file_path().ok()?).ok()?, d.text.clone())))
            .collect();
        Snapshot::new(kalinga_driver::analyze(&file_name(uri), text, &open))
    }

    /// The problems to show in the document. Problems in an imported file are shown on
    /// the line importing it (when that file is open, its own analysis shows them too).
    pub fn diagnostics(&mut self, uri: &Url) -> Vec<Diagnostic> {
        let Some(snap) = self.snapshot(uri) else { return vec![] };
        let a = &snap.analysis;
        let root = a.sources.file(0).1;
        a.diagnostics
            .iter()
            .map(|d| {
                let mut message = snap.show(&d.message);
                if let Some(help) = &d.help {
                    message = format!("{}\nhelp: {}", message, snap.show(help));
                }
                let range = if d.span.file == 0 && d.span.line > 0 {
                    text::range(root, d.span)
                } else {
                    let (name, _) = a.sources.file(d.span.file);
                    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
                    if d.span.line > 0 {
                        message = format!("in {}:{}:{}: {}", base, d.span.line, d.span.col, message);
                    }
                    import_line(root, base)
                };
                Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("kalinga".into()),
                    message,
                    ..Diagnostic::default()
                }
            })
            .collect()
    }
}

/// The `import` line that names the file `base` (or the first import, or the first line).
fn import_line(root: &str, base: &str) -> Range {
    let lines: Vec<&str> = root.lines().collect();
    let is_import = |l: &&str| l.trim_start().starts_with("import ");
    let stem = base.strip_suffix(".kl").unwrap_or(base);
    let i = lines
        .iter()
        .position(|l| is_import(l) && l.contains(stem))
        .or_else(|| lines.iter().position(is_import))
        .unwrap_or(0);
    let width = lines.get(i).map_or(0, |l| l.encode_utf16().count()) as u32;
    Range::new(Position::new(i as u32, 0), Position::new(i as u32, width))
}

/// The name the compiler gets for a document: its path, which also resolves its imports.
fn file_name(uri: &Url) -> String {
    match uri.to_file_path() {
        Ok(p) => p.display().to_string(),
        Err(()) => "untitled.kl".to_string(),
    }
}
