//! `kalinga fix`: rewrites code written for an earlier Kalinga version, keeping its
//! formatting and comments.
//!
//! Migrations, and the code they apply to:
//! - 0.5: `print(...)` no longer ends the line, so 0.4's `print(...)` becomes `println(...)`.
//!   Only for code written for 0.4: in 0.5 code, `print` staying on the line is meant.
//! - 0.5: member constants are `const NAME: Type = value`, no longer `static NAME: ...`.
//!   For any code: 0.5 rejects the old form, so rewriting it cannot change a program.

use std::path::{Path, PathBuf};

use kalinga_syntax::error::Diagnostic;
use kalinga_syntax::lexer;
use kalinga_syntax::token::Tok;

use crate::error::{DriverError, Result, SourceMap};

/// A language version, `(major, minor)`: `0.4` is `(0, 4)`.
pub type LangVersion = (u32, u32);

/// `"0.4"` or `"0.4.2"` as a language version.
pub fn parse_version(text: &str) -> Option<LangVersion> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// The language version of the project `path` is in: its `utkal.toml`'s `kalinga = "..."`,
/// or 0.4 when the field is missing (it was added in 0.4). `None` outside a project.
pub fn project_version(path: &Path) -> Option<LangVersion> {
    let dir = kalinga_project::find_project(path)?;
    match kalinga_project::read_manifest(&dir).ok()?.kalinga {
        Some(v) => parse_version(&v),
        None => Some((0, 4)),
    }
}

/// What fixing one file did.
#[derive(Debug, PartialEq)]
pub struct Fixed {
    pub text: String,
    /// How many places changed.
    pub changed: usize,
    /// `print(` calls left alone because the code's version is unknown.
    pub held_back: usize,
}

/// `source`, written for version `from` (unknown when `None`), with the migrations for it
/// applied.
pub fn fix_source(source: &str, from: Option<LangVersion>) -> std::result::Result<Fixed, Diagnostic> {
    let tokens = lexer::lex(source, 0)?;
    let before_0_5 = from.is_some_and(|v| v < (0, 5));
    // (line, column, old word, new word), in source order.
    let mut edits: Vec<(usize, usize, &str, &str)> = vec![];
    let mut held_back = 0;
    for (i, t) in tokens.iter().enumerate() {
        let next = |k: usize| tokens.get(i + k).map(|n| &n.tok);
        let after = |tok: Tok| i > 0 && tokens[i - 1].tok == tok;
        // Calls of the built-in `print`: the name, then `(`; not a method (`x.print(`) and
        // not a declaration (`fn print(`).
        if t.tok == Tok::Ident("print".into()) && next(1) == Some(&Tok::LParen) && !after(Tok::Dot) && !after(Tok::Fn) {
            if before_0_5 {
                edits.push((t.pos.line, t.pos.col, "print", "println"));
            } else if from.is_none() {
                held_back += 1;
            }
        }
        // `static NAME: Type = value`
        if t.tok == Tok::Static && matches!(next(1), Some(Tok::Ident(_))) && next(2) == Some(&Tok::Colon) {
            edits.push((t.pos.line, t.pos.col, "static", "const"));
        }
    }
    let mut text = source.to_string();
    for (line, col, old, new) in edits.iter().rev() {
        if let Some(offset) = byte_offset(source, *line, *col) {
            text.replace_range(offset..offset + old.len(), new);
        }
    }
    Ok(Fixed { text, changed: edits.len(), held_back })
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
    /// Files with `print(` calls left alone because their version is unknown, and how many.
    pub held_back: Vec<(PathBuf, usize)>,
    /// Files left alone because they could not be read as Kalinga (the errors say why).
    pub skipped: Vec<DriverError>,
}

/// Fixes `path` (a `.kl` file, or every `.kl` file under a folder) in place, as code
/// written for `from` (unknown when `None`).
pub fn fix_path(path: &Path, from: Option<LangVersion>) -> Result<FixReport> {
    let files = kl_files(path)?;
    let mut report = FixReport::default();
    for file in files {
        let source = std::fs::read_to_string(&file).map_err(DriverError::io("read", &file))?;
        match fix_source(&source, from) {
            Ok(fixed) => {
                if fixed.changed > 0 {
                    std::fs::write(&file, &fixed.text).map_err(DriverError::io("write", &file))?;
                    report.changed.push((file.clone(), fixed.changed));
                }
                if fixed.held_back > 0 {
                    report.held_back.push((file, fixed.held_back));
                }
            }
            Err(d) => {
                let sources = SourceMap { files: vec![(file.display().to_string(), source)], display: vec![] };
                report.skipped.push(DriverError::compile(sources, d));
            }
        }
    }
    Ok(report)
}

/// `path` if it is a file, else every `.kl` file under the folder (skipping `target`), sorted.
pub fn kl_files(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vec![];
    collect_kl_files(path, &mut files)?;
    Ok(files)
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
    use super::{Fixed, fix_source, parse_version};

    const V0_4: Option<(u32, u32)> = Some((0, 4));

    fn fixed(source: &str, from: Option<(u32, u32)>) -> Fixed {
        fix_source(source, from).unwrap()
    }

    #[test]
    fn print_calls_in_0_4_code_become_println() {
        let out = fixed("fn main() {\n    print(\"a\", 1) // print(\n    obj.print(2)\n    print()\n}\n", V0_4);
        assert_eq!(out.text, "fn main() {\n    println(\"a\", 1) // print(\n    obj.print(2)\n    println()\n}\n");
        assert_eq!((out.changed, out.held_back), (2, 0));
    }

    #[test]
    fn print_calls_in_0_5_code_stay() {
        let source = "fn main() {\n    print(\"no newline\")\n}\n";
        assert_eq!(fixed(source, Some((0, 5))), Fixed { text: source.into(), changed: 0, held_back: 0 });
        // Of unknown version: left alone, and counted.
        assert_eq!(fixed(source, None), Fixed { text: source.into(), changed: 0, held_back: 1 });
    }

    #[test]
    fn fixing_twice_changes_nothing_more() {
        let once = fixed("fn main() {\n    print(1)\n}\n", V0_4).text;
        // After `utkal fix`, the project says 0.5.
        assert_eq!(fixed(&once, Some((0, 5))).changed, 0);
    }

    #[test]
    fn declarations_are_left_alone() {
        let source = "extern fn print(x: int)\nclass P {\n    fn print() {}\n}\n";
        let out = fixed(source, V0_4);
        assert_eq!((out.text.as_str(), out.changed), (source, 0));
    }

    #[test]
    fn static_constants_become_const_in_any_code() {
        let source = "struct C {\n    static RED: int = 1\n    static var n: int = 0\n    static fn f() {}\n}\n";
        let expected = "struct C {\n    const RED: int = 1\n    static var n: int = 0\n    static fn f() {}\n}\n";
        for from in [V0_4, Some((0, 5)), None] {
            let out = fixed(source, from);
            assert_eq!((out.text.as_str(), out.changed), (expected, 1));
        }
    }

    #[test]
    fn non_ascii_text_before_a_call() {
        let out = fixed("fn main() { let s = \"é→\"; print(s) }\n", V0_4);
        assert_eq!(out.text, "fn main() { let s = \"é→\"; println(s) }\n");
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("0.4"), Some((0, 4)));
        assert_eq!(parse_version(" 0.10.2 "), Some((0, 10)));
        assert_eq!(parse_version("x"), None);
    }
}
