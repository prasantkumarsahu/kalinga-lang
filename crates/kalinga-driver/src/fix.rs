//! `kalinga fix`: rewrites code written for an earlier Kalinga version, keeping its
//! formatting and comments.
//!
//! Migrations, and the code they apply to:
//! - 0.5: `print(...)` no longer ends the line, so 0.4's `print(...)` becomes `println(...)`.
//!   Only for code written for 0.4: in 0.5 code, `print` staying on the line is meant.
//! - 0.7: fixed by default. For any code (the old forms no longer compile):
//!   - `let x = v` becomes `x = v`, and `if let` / `while let` become `if x = v` /
//!     `while x = v`; a `let` that hides a visible variable of the same name becomes `var`
//!     (without a keyword it would change that variable instead);
//!   - `const NAME: T = v` becomes `NAME: T = v` at the top level and `static NAME: T = v`
//!     in a class or struct;
//!   - `private(set) name: T` becomes `private(set) var name: T`;
//!   - fields that code changes become `var name: T` (found by the checker, `fix_fields`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use kalinga_syntax::error::Diagnostic;
use kalinga_syntax::lexer;
use kalinga_syntax::token::{Tok, Token};

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
    }
    edits.extend(fixed_by_default(&tokens));
    edits.sort_by_key(|(l, c, _, _)| (*l, *c));
    let mut text = source.to_string();
    let mut changed = 0;
    for (line, col, old, new) in edits.iter().rev() {
        if let Some(offset) = byte_offset(source, *line, *col)
            && source[offset..].starts_with(old)
        {
            text.replace_range(offset..offset + old.len(), new);
            changed += 1;
        }
    }
    Ok(Fixed { text, changed, held_back })
}

/// The 0.7 rewrites that need only the tokens (see the module docs). Local scopes are
/// followed from the braces, to find a `let` that hides a variable of the same name.
fn fixed_by_default(tokens: &[Token]) -> Vec<(usize, usize, &'static str, &'static str)> {
    let mut edits = vec![];
    // Names declared in each open `{`, and those the next `{` starts with (parameters, loop
    // variables, `if let` names).
    let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
    let mut pending: Vec<String> = vec![];
    let ident = |i: usize| match tokens.get(i).map(|t| &t.tok) {
        Some(Tok::Ident(n)) => Some(n.clone()),
        _ => None,
    };
    for (i, t) in tokens.iter().enumerate() {
        let at = (t.pos.line, t.pos.col);
        match &t.tok {
            Tok::LBrace => scopes.push(pending.drain(..).collect()),
            Tok::RBrace => {
                if scopes.len() > 1 {
                    scopes.pop();
                }
            }
            // Parameters of `fn name(...)`, `fn(...)` and `init(...)`: `name:` inside the
            // parentheses.
            Tok::Fn | Tok::Init => {
                let Some(open) = tokens[i..].iter().position(|t| t.tok == Tok::LParen).map(|p| i + p) else { continue };
                let mut depth = 0;
                for j in open..tokens.len() {
                    match tokens[j].tok {
                        Tok::LParen => depth += 1,
                        Tok::RParen => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Tok::Ident(ref n) if depth == 1 && tokens.get(j + 1).map(|t| &t.tok) == Some(&Tok::Colon) => {
                            pending.push(n.clone());
                        }
                        _ => {}
                    }
                }
            }
            // Loop variables, up to `in`.
            Tok::For => {
                for t in tokens[i + 1..].iter().take_while(|t| t.tok != Tok::In) {
                    if let Tok::Ident(n) = &t.tok {
                        pending.push(n.clone());
                    }
                }
            }
            Tok::Let => {
                let unwraps = i > 0 && matches!(tokens[i - 1].tok, Tok::If | Tok::While);
                if unwraps {
                    // `if let x = v` / `while let x = v`: `x` belongs to the block.
                    pending.extend(ident(i + 1));
                    edits.push((at.0, at.1, "let ", ""));
                    continue;
                }
                let names: Vec<String> = if tokens.get(i + 1).map(|t| &t.tok) == Some(&Tok::LParen) {
                    tokens[i + 2..]
                        .iter()
                        .take_while(|t| t.tok != Tok::RParen)
                        .filter_map(|t| match &t.tok {
                            Tok::Ident(n) => Some(n.clone()),
                            _ => None,
                        })
                        .collect()
                } else {
                    ident(i + 1).into_iter().collect()
                };
                let hides = names.iter().any(|n| scopes.iter().any(|s| s.contains(n)));
                edits.push(if hides { (at.0, at.1, "let", "var") } else { (at.0, at.1, "let ", "") });
                if let Some(s) = scopes.last_mut() {
                    s.extend(names);
                }
            }
            Tok::Var => {
                if let (Some(n), Some(s)) = (ident(i + 1), scopes.last_mut()) {
                    s.insert(n);
                }
            }
            Tok::Const => {
                let in_type = scopes.len() > 1;
                edits.push(if in_type { (at.0, at.1, "const", "static") } else { (at.0, at.1, "const ", "") });
            }
            // `private(set) name:` -> `private(set) var name:`
            Tok::Private
                if tokens.get(i + 1).map(|t| &t.tok) == Some(&Tok::LParen)
                    && ident(i + 2).as_deref() == Some("set")
                    && tokens.get(i + 3).map(|t| &t.tok) == Some(&Tok::RParen)
                    && ident(i + 4).is_some() =>
            {
                let name = &tokens[i + 4].pos;
                edits.push((name.line, name.col, "", "var "));
            }
            _ => {}
        }
    }
    edits
}

/// Adds `var` to every field that the code changes, using the checker: each "it is a fixed
/// field" error names a field, whose declaration gets `var`. `texts` holds the files being
/// fixed (by path) and is updated; files they import are read as usual. Returns how many
/// fields changed in each file.
pub fn fix_fields(texts: &mut BTreeMap<PathBuf, String>) -> HashMap<PathBuf, usize> {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let keys: HashMap<PathBuf, PathBuf> = texts.keys().map(|k| (canon(k), k.clone())).collect();
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    // Each round can only see the first error of some bodies: repeat until none are left.
    for _ in 0..100 {
        let open: HashMap<PathBuf, String> = texts.iter().map(|(k, v)| (canon(k), v.clone())).collect();
        let mut inserts: HashSet<(PathBuf, usize, usize)> = HashSet::new();
        for (path, text) in texts.iter() {
            let a = crate::analyze(&path.display().to_string(), text, &open);
            let Some(prog) = &a.program else { continue };
            for d in &a.diagnostics {
                let Some((field, owner)) = fixed_field_error(&d.message) else { continue };
                let fields = prog
                    .classes
                    .iter()
                    .filter(|c| c.name == owner)
                    .flat_map(|c| &c.fields)
                    .chain(prog.structs.iter().filter(|s| s.name == owner).flat_map(|s| &s.fields));
                for f in fields.filter(|f| f.name == field) {
                    let file = a.paths.get(f.pos.file as usize).cloned().flatten();
                    if let Some(key) = file.and_then(|p| keys.get(&canon(&p)).cloned()) {
                        inserts.insert((key, f.pos.line, f.pos.col));
                    }
                }
            }
        }
        if inserts.is_empty() {
            break;
        }
        let mut inserts: Vec<_> = inserts.into_iter().collect();
        inserts.sort();
        for (key, line, col) in inserts.iter().rev() {
            let text = texts.get_mut(key).expect("a file being fixed");
            if let Some(offset) = byte_offset(text, *line, *col) {
                text.insert_str(offset, "var ");
                *counts.entry(key.clone()).or_default() += 1;
            }
        }
    }
    counts
}

/// (field, owner) from "cannot change `f`: it is a fixed field of `C`...".
fn fixed_field_error(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("cannot change `")?;
    let (field, rest) = rest.split_once('`')?;
    let owner = rest.split_once("it is a fixed field of `")?.1.split_once('`')?.0;
    Some((field.to_string(), owner.to_string()))
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
    let mut texts: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut changed: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for file in files {
        let source = std::fs::read_to_string(&file).map_err(DriverError::io("read", &file))?;
        match fix_source(&source, from) {
            Ok(fixed) => {
                if fixed.changed > 0 {
                    changed.insert(file.clone(), fixed.changed);
                }
                if fixed.held_back > 0 {
                    report.held_back.push((file.clone(), fixed.held_back));
                }
                texts.insert(file, fixed.text);
            }
            Err(d) => {
                let sources = SourceMap { files: vec![(file.display().to_string(), source)], display: vec![] };
                report.skipped.push(DriverError::compile(sources, d));
            }
        }
    }
    for (file, n) in fix_fields(&mut texts) {
        *changed.entry(file).or_default() += n;
    }
    for (file, n) in changed {
        std::fs::write(&file, &texts[&file]).map_err(DriverError::io("write", &file))?;
        report.changed.push((file, n));
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
    fn let_and_const_go() {
        let source = "const MAX: int = 3\n\nstruct C {\n    const RED: int = 1\n    static var n: int = 0\n    private(set) hp: int = 0\n}\n\nfn main() {\n    let a = 1\n    let (b, c) = (2, 3)\n    if let d = f() {\n        let a = d\n    }\n    while let e = g() {}\n}\n";
        let expected = "MAX: int = 3\n\nstruct C {\n    static RED: int = 1\n    static var n: int = 0\n    private(set) var hp: int = 0\n}\n\nfn main() {\n    a = 1\n    (b, c) = (2, 3)\n    if d = f() {\n        var a = d\n    }\n    while e = g() {}\n}\n";
        for from in [Some((0, 6)), None] {
            let out = fixed(source, from);
            assert_eq!(out.text, expected);
            assert_eq!(out.changed, 8);
            // Fixing again changes nothing.
            assert_eq!(fixed(&out.text, from).changed, 0);
        }
    }

    #[test]
    fn a_let_hiding_a_parameter_or_loop_variable_becomes_var() {
        let source =
            "fn f(x: int) {\n    for i in 0..3 {\n        let i = i * 2\n    }\n    let x = x + 1\n    let y = x\n}\n";
        let expected =
            "fn f(x: int) {\n    for i in 0..3 {\n        var i = i * 2\n    }\n    var x = x + 1\n    y = x\n}\n";
        assert_eq!(fixed(source, None).text, expected);
    }

    #[test]
    fn changed_fields_become_var() {
        let dir = std::env::temp_dir().join("kalinga-fix-fields");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("main.kl");
        let source = "class A {\n    name: str\n    hp: int = 3\n    init(name: str) {\n        self.name = name\n    }\n    fn hit() {\n        self.hp -= 1\n    }\n}\n\nstruct P {\n    x: int\n    y: int\n}\n\nfn main() {\n    var p = P(1, 2)\n    p.y = 3\n    A(\"a\").hit()\n}\n";
        std::fs::write(&path, source).unwrap();
        let mut texts = std::collections::BTreeMap::from([(path.clone(), source.to_string())]);
        let counts = super::fix_fields(&mut texts);
        assert_eq!(counts.get(&path), Some(&2));
        let text = &texts[&path];
        assert!(text.contains("    name: str\n    var hp: int = 3\n"), "{text}");
        assert!(text.contains("    x: int\n    var y: int\n"), "{text}");
    }

    #[test]
    fn non_ascii_text_before_a_call() {
        let out = fixed("fn main() { let s = \"é→\"; print(s) }\n", V0_4);
        assert_eq!(out.text, "fn main() { s = \"é→\"; println(s) }\n");
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("0.4"), Some((0, 4)));
        assert_eq!(parse_version(" 0.10.2 "), Some((0, 10)));
        assert_eq!(parse_version("x"), None);
    }
}
