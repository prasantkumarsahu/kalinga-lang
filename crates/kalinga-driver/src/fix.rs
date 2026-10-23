//! `kalinga fix`: rewrites code written for an earlier Kalinga version, keeping its
//! formatting and comments.
//!
//! Migrations:
//! - 0.5: `print(...)` no longer ends the line, so 0.4's `print(...)` becomes `println(...)`.

use std::path::{Path, PathBuf};

use kalinga_syntax::error::Diagnostic;
use kalinga_syntax::lexer;
use kalinga_syntax::token::Tok;

use crate::error::{DriverError, Result, SourceMap};

/// `source` with every migration applied, and how many places changed.
pub fn fix_source(source: &str) -> std::result::Result<(String, usize), Diagnostic> {
    let tokens = lexer::lex(source, 0)?;
    // Calls of the built-in `print`: the name, then `(`; not a method (`x.print(`) and not a
    // declaration (`fn print(`).
    let mut at = vec![];
    for (i, t) in tokens.iter().enumerate() {
        let is_print = t.tok == Tok::Ident("print".into());
        let called = tokens.get(i + 1).is_some_and(|n| n.tok == Tok::LParen);
        let after = |tok: Tok| i > 0 && tokens[i - 1].tok == tok;
        if is_print && called && !after(Tok::Dot) && !after(Tok::Fn) {
            at.push((t.pos.line, t.pos.col));
        }
    }
    let mut out = source.to_string();
    for (line, col) in at.iter().rev() {
        if let Some(offset) = byte_offset(source, *line, *col) {
            out.insert_str(offset + "print".len(), "ln");
        }
    }
    Ok((out, at.len()))
}

/// Byte offset of a 1-based line and character column.
fn byte_offset(source: &str, line: usize, col: usize) -> Option<usize> {
    let start: usize = source.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum();
    let text = source[start..].split('\n').next()?;
    let within = text.char_indices().nth(col.saturating_sub(1)).map_or(text.len(), |(i, _)| i);
    Some(start + within)
}

/// What `fix_path` did.
#[derive(Debug, Default)]
pub struct FixReport {
    /// Files that changed, and how many places in each.
    pub changed: Vec<(PathBuf, usize)>,
    /// Files left alone because they could not be read as Kalinga (the errors say why).
    pub skipped: Vec<DriverError>,
}

/// Fixes `path` (a `.kl` file, or every `.kl` file under a folder) in place.
pub fn fix_path(path: &Path) -> Result<FixReport> {
    let mut files = vec![];
    collect_kl_files(path, &mut files)?;
    let mut report = FixReport::default();
    for file in files {
        let source = std::fs::read_to_string(&file).map_err(DriverError::io("read", &file))?;
        match fix_source(&source) {
            Ok((fixed, n)) if n > 0 => {
                std::fs::write(&file, fixed).map_err(DriverError::io("write", &file))?;
                report.changed.push((file, n));
            }
            Ok(_) => {}
            Err(d) => {
                let sources = SourceMap { files: vec![(file.display().to_string(), source)], display: vec![] };
                report.skipped.push(DriverError::compile(sources, d));
            }
        }
    }
    Ok(report)
}

fn collect_kl_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let entries = std::fs::read_dir(path).map_err(DriverError::io("read", path))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() && p.file_name().is_some_and(|n| n != "target") {
            collect_kl_files(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "kl") {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::fix_source;

    #[test]
    fn print_calls_become_println() {
        let (out, n) =
            fix_source("fn main() {\n    print(\"a\", 1) // print(\n    obj.print(2)\n    print()\n}\n").unwrap();
        assert_eq!(out, "fn main() {\n    println(\"a\", 1) // print(\n    obj.print(2)\n    println()\n}\n");
        assert_eq!(n, 2);
    }

    #[test]
    fn declarations_are_left_alone() {
        let source = "extern fn print(x: int)\nclass P {\n    fn print() {}\n}\n";
        let (out, n) = fix_source(source).unwrap();
        assert_eq!(n, 0);
        assert_eq!(out, source);
    }

    #[test]
    fn non_ascii_text_before_a_call() {
        let (out, _) = fix_source("fn main() { let s = \"é→\"; print(s) }\n").unwrap();
        assert_eq!(out, "fn main() { let s = \"é→\"; println(s) }\n");
    }
}
