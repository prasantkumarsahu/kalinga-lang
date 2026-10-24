//! Broken code must never crash the compiler, the formatter or the language server.
//!
//! Every `.kl` file of the examples, the standard library and the UI tests is broken in
//! many seeded, pseudo-random ways (a range deleted, the file cut short, a line repeated,
//! a token dropped in), and each version goes through everything an editor or a build
//! would do with it. `KALINGA_MUTANTS` sets how many versions of each file (default 12).

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kalinga_lsp::world::World;
use lsp_types::*;

/// Inserted at random places: the pieces of half-written code.
const PIECES: &[&str] = &[
    "{", "}", "(", ")", "[", "]", "\"", "'", "fn ", "class ", "let ", "if ", "match ", "..", "..=", "?", "//", "/*",
    ".", ",", ":", "->", "=>", "=", "<", ">", "\n", "    ", "self.", "0", "1.5", "none", "async ", "await ",
    "test fn ", "@", "\\", "é", "😀",
];

/// xorshift64*: the same broken files on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// A character boundary of `s` near a random byte.
fn boundary(s: &str, rng: &mut Rng) -> usize {
    let mut i = rng.below(s.len() + 1);
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn mutate(src: &str, rng: &mut Rng) -> String {
    let mut s = src.to_string();
    for _ in 0..1 + rng.below(3) {
        match rng.below(5) {
            0 => {
                let a = boundary(&s, rng);
                let mut b = (a + 1 + rng.below(40)).min(s.len());
                while !s.is_char_boundary(b) {
                    b += 1;
                }
                s.replace_range(a..b, "");
            }
            1 => {
                let a = boundary(&s, rng);
                s.truncate(a);
            }
            2 => {
                let lines: Vec<&str> = s.lines().collect();
                if !lines.is_empty() {
                    let i = rng.below(lines.len());
                    let mut out: Vec<&str> = lines.clone();
                    out.insert(rng.below(lines.len() + 1), lines[i]);
                    s = out.join("\n");
                }
            }
            _ => {
                let a = boundary(&s, rng);
                s.insert_str(a, PIECES[rng.below(PIECES.len())]);
            }
        }
    }
    s
}

/// The LSP position of byte `offset` in `text`.
fn position(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before[before.rfind('\n').map_or(0, |i| i + 1)..].encode_utf16().count() as u32;
    Position::new(line, col)
}

fn at(uri: &Url, pos: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri: uri.clone() }, position: pos }
}

/// Everything a build and an editor do with `text`, as the file at `path`.
fn exercise(path: &Path, text: &str, rng: &mut Rng) {
    let name = path.to_string_lossy();

    if let Ok(once) = kalinga_fmt::format(text) {
        let twice = kalinga_fmt::format(&once).expect("formatted code formats again");
        assert_eq!(once, twice, "formatting twice changes the code");
    }

    let analysis = kalinga_driver::analyze(&name, text, &HashMap::new());
    if analysis.diagnostics.is_empty() {
        let _ = kalinga_driver::to_rust(&name, text);
    }

    let uri = Url::from_file_path(path).expect("file url");
    let mut world = World::default();
    world.open(uri.clone(), text.to_string());
    let _ = world.diagnostics(&uri);
    let doc = TextDocumentIdentifier { uri: uri.clone() };
    let _ = kalinga_lsp::navigation::symbols(
        &mut world,
        DocumentSymbolParams {
            text_document: doc.clone(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    );
    let _ = kalinga_lsp::formatting::format(
        &mut world,
        DocumentFormattingParams {
            text_document: doc.clone(),
            options: FormattingOptions { tab_size: 4, insert_spaces: true, ..Default::default() },
            work_done_progress_params: Default::default(),
        },
    );
    for _ in 0..3 {
        let pos = position(text, boundary(text, rng));
        let _ = kalinga_lsp::hover::hover(
            &mut world,
            HoverParams { text_document_position_params: at(&uri, pos), work_done_progress_params: Default::default() },
        );
        let _ = kalinga_lsp::completion::complete(
            &mut world,
            CompletionParams {
                text_document_position: at(&uri, pos),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            },
        );
        let _ = kalinga_lsp::signature::help(
            &mut world,
            SignatureHelpParams {
                text_document_position_params: at(&uri, pos),
                work_done_progress_params: Default::default(),
                context: None,
            },
        );
        let _ = kalinga_lsp::navigation::definition(
            &mut world,
            GotoDefinitionParams {
                text_document_position_params: at(&uri, pos),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        );
        let _ = kalinga_lsp::navigation::prepare_rename(&mut world, at(&uri, pos));
        let _ = kalinga_lsp::actions::actions(
            &mut world,
            CodeActionParams {
                text_document: doc.clone(),
                range: Range::new(pos, pos),
                context: CodeActionContext::default(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        );
    }
}

fn kl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("read dir").flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            kl_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "kl") {
            out.push(p);
        }
    }
}

#[test]
fn broken_code_never_crashes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("workspace root");
    let mut files = vec![];
    for dir in ["examples", "crates/kalinga-driver/std", "tests/ui"] {
        kl_files(&root.join(dir), &mut files);
    }
    let per_file: usize = std::env::var("KALINGA_MUTANTS").ok().and_then(|n| n.parse().ok()).unwrap_or(12);

    // Instead of the default message, keep where the panic happened (the compiler's own
    // frames) for the report.
    static WHERE: Mutex<String> = Mutex::new(String::new());
    std::panic::set_hook(Box::new(|_| {
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let frames: Vec<&str> = trace
            .lines()
            .map(str::trim)
            .filter(|l| l.contains("kalinga") && !l.contains("robustness"))
            .take(6)
            .collect();
        *WHERE.lock().unwrap() = frames.join("\n    ");
    }));
    let mut crashes = vec![];
    for (n, file) in files.iter().enumerate() {
        let src = std::fs::read_to_string(file).expect("read");
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (n as u64 + 1));
        for i in 0..per_file {
            let text = mutate(&src, &mut rng);
            let result = catch_unwind(AssertUnwindSafe(|| exercise(file, &text, &mut rng)));
            if let Err(e) = result {
                let message = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                let place = std::mem::take(&mut *WHERE.lock().unwrap());
                crashes.push((file.clone(), i, format!("{}\n    {}", message, place), text));
            }
        }
    }
    let _ = std::panic::take_hook();

    if !crashes.is_empty() {
        let dir = std::env::temp_dir().join("kalinga-robustness");
        let _ = std::fs::create_dir_all(&dir);
        let mut report = format!("{} of {} broken files crashed:\n", crashes.len(), files.len() * per_file);
        for (k, (file, i, message, text)) in crashes.iter().enumerate() {
            let saved = dir.join(format!("crash{}.kl", k));
            let _ = std::fs::write(&saved, text);
            report += &format!("- {} #{}: {}\n  saved as {}\n", file.display(), i, message, saved.display());
        }
        panic!("{}", report);
    }
}
