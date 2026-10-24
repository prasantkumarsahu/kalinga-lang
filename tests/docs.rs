//! Keeps the documentation honest. In `docs/*.md` and the Kalinga skill for AI models
//! (`skills/kalinga/**/*.md`):
//! - every complete program (a ```` ```kalinga ```` block with `fn main()`, or with
//!   `test fn`s) must compile, and when a ```` ```text ```` block follows it, running it
//!   (with `kalinga test` for tests) must print exactly that;
//! - every ```` ```kalinga error ```` block must be rejected, with the first line of the
//!   following ```` ```text ```` block as the first line of the error;
//! - every other ```` ```kalinga ```` block is in the standard layout (`kalinga fmt`),
//!   except those showing `;`, which `fmt` would split.
//!
//! Programs that read input get `Asha` and `20` on stdin, and each runs in its own
//! folder. Blocks that need other files (`import` of a file or package, `use rust`,
//! `use crate`) are covered by the UI tests instead.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

struct Block {
    file: String,
    line: usize,
    expect_error: bool,
    code: String,
    /// The ```` ```text ```` block right after this one.
    output: Option<String>,
}

/// All fenced blocks of one Markdown text: (info string, first line, contents).
fn fenced(text: &str) -> Vec<(String, usize, String)> {
    let mut out = vec![];
    let mut current: Option<(String, usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        match current.take() {
            None => {
                if let Some(info) = trimmed.strip_prefix("```").filter(|i| !i.trim().is_empty()) {
                    current = Some((info.trim().to_string(), i + 1, String::new()));
                }
            }
            Some((info, start, mut body)) => {
                if trimmed == "```" {
                    out.push((info, start, body));
                } else {
                    body.push_str(line);
                    body.push('\n');
                    current = Some((info, start, body));
                }
            }
        }
    }
    out
}

/// The Markdown files in `dir`, sorted.
fn markdown_in(dir: &str) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {}", dir, e))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    paths.sort();
    paths
}

fn doc_blocks() -> Vec<Block> {
    let mut paths = markdown_in("docs");
    paths.extend(markdown_in("skills/kalinga"));
    paths.extend(markdown_in("skills/kalinga/references"));
    let mut blocks = vec![];
    for path in paths {
        let all = fenced(&std::fs::read_to_string(&path).unwrap());
        for (i, (info, line, code)) in all.iter().enumerate() {
            if info != "kalinga" && info != "kalinga error" {
                continue;
            }
            // The output is the next ```text block; ```bash blocks (the command that runs
            // the program) may come in between.
            let output = all[i + 1..]
                .iter()
                .find(|(next, _, _)| next != "bash")
                .filter(|(next, _, _)| next == "text")
                .map(|(_, _, t)| t.clone());
            blocks.push(Block {
                file: path.display().to_string(),
                line: *line,
                expect_error: info == "kalinga error",
                code: code.clone(),
                output,
            });
        }
    }
    blocks
}

/// Runs `kalinga` in `dir`.
fn kalinga(args: &[&str], stdin: &str, dir: &Path) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kalinga"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text.replace("\r\n", "\n"))
}

/// Checks one block; `None` when it is as documented.
fn verify(b: &Block, id: usize) -> Option<String> {
    let at = format!("{}:{}", b.file, b.line);
    let (code, checked) = kalinga(&["check", "main.kl", "--stdin"], &b.code, Path::new("."));
    if b.expect_error {
        let expected = b.output.as_deref().and_then(|o| o.lines().next()).unwrap_or("");
        return match (code, checked.lines().next()) {
            (0, _) => Some(format!("{}: expected a compile error, but it compiled", at)),
            (_, Some(first)) if !expected.is_empty() && first != expected => {
                Some(format!("{}: error is\n  {}\nbut the docs say\n  {}", at, first, expected))
            }
            _ => None,
        };
    }
    if code != 0 {
        return Some(format!("{}: does not compile:\n{}", at, checked));
    }
    let expected = b.output.as_deref()?;
    // Each block runs in its own folder, so build files and files it writes never collide.
    let dir = std::env::temp_dir().join("kalinga-doc-tests").join(id.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.kl"), &b.code).unwrap();
    let command = if b.code.contains("test fn") { "test" } else { "run" };
    let (_, got) = kalinga(&[command, "main.kl"], "Asha\n20\n", &dir);
    (got.trim() != expected.trim()).then(|| format!("{}: output differs\n--- docs\n{}--- got\n{}", at, expected, got))
}

#[test]
fn doc_examples_match_their_output() {
    let blocks: Vec<Block> = doc_blocks()
        .into_iter()
        .filter(|b| b.expect_error || b.code.contains("fn main()") || b.code.contains("test fn"))
        .filter(|b| !["import \"", "use rust \"", "use crate \""].iter().any(|n| b.code.contains(n)))
        .collect();
    assert!(blocks.len() > 100, "only found {} doc examples", blocks.len());
    let queue = Mutex::new(blocks.iter().enumerate().collect::<Vec<_>>());
    let failures = Mutex::new(vec![]);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some((id, b)) = queue.lock().unwrap().pop() {
                    if let Some(f) = verify(b, id) {
                        failures.lock().unwrap().push(f);
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    assert!(failures.is_empty(), "{} doc example(s) are wrong:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn doc_examples_are_formatted() {
    let unformatted: Vec<String> = doc_blocks()
        .iter()
        .filter(|b| !b.expect_error && !b.code.contains(';'))
        .filter(|b| kalinga_fmt::format(&b.code).is_ok_and(|f| f != b.code))
        .map(|b| format!("{}:{}", b.file, b.line))
        .collect();
    assert!(unformatted.is_empty(), "examples not in the `kalinga fmt` layout: {}", unformatted.join(", "));
}

/// The `| ... |` rows of the table that follows `heading` in `text`.
fn table_rows<'a>(text: &'a str, heading: &str) -> Vec<Vec<&'a str>> {
    let after = &text[text.find(heading).unwrap_or_else(|| panic!("no `{}` section", heading))..];
    after
        .lines()
        .skip(1)
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .skip(2) // header and separator
        .map(|l| l.trim_matches('|').split(" | ").map(str::trim).collect())
        .collect()
}

/// Names called like `name(...)` in a table cell, e.g. `` `get(k)` / `has(k)` ``.
fn called_names(cell: &str) -> Vec<String> {
    let mut out = vec![];
    for part in cell.split('`').skip(1).step_by(2) {
        let name: String = part.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect();
        if part[name.len()..].starts_with('(') {
            out.push(name);
        }
    }
    out
}

/// The skill's language reference is the docs' Reference chapter, with its title and its
/// links adapted to where it lives.
#[test]
fn skill_reference_is_the_reference_chapter() {
    let chapter = std::fs::read_to_string("docs/26-reference.md").unwrap().replace(
        "
", "
",
    );
    let body = chapter
        .split(
            "
---
",
        )
        .next()
        .unwrap()
        .trim_end();
    let expected = body
        .replacen(
            "# 26. Language Reference

A compact summary of the whole language. For explanations and examples, see the guide
chapters linked from the [docs index](README.md).",
            "# Kalinga language reference

A compact summary of the whole language (a copy of `docs/26-reference.md` in the Kalinga
repository; chapter numbers below refer to the docs there).",
            1,
        )
        .replace("[Standard Library](15-standard-library.md)", "[standard-library.md](standard-library.md)");
    let copy = std::fs::read_to_string("skills/kalinga/references/language-reference.md").unwrap().replace(
        "
", "
",
    );
    assert!(
        copy.trim_end() == expected,
        "skills/kalinga/references/language-reference.md differs from docs/26-reference.md; copy the chapter again"
    );
}

#[test]
fn reference_lists_every_builtin_method() {
    use kalinga_sema::builtins::METHODS;
    let text = std::fs::read_to_string("docs/26-reference.md").unwrap();
    let mut documented: Vec<(String, String)> = vec![];
    for row in table_rows(&text, "## Built-in methods") {
        let receiver = row[0].trim_end_matches(" (var)").trim_matches('`').to_string();
        for name in called_names(row[1]) {
            documented.push((receiver.clone(), name));
        }
    }
    documented.sort();
    let mut registered: Vec<(String, String)> =
        METHODS.iter().map(|m| (m.receiver.to_string(), m.name.to_string())).collect();
    registered.sort();
    assert_eq!(documented, registered, "docs/26-reference.md's built-in methods differ from the registry");
}

#[test]
fn standard_library_chapter_lists_every_library_function() {
    use kalinga_sema::builtins::FUNCTIONS;
    let text = std::fs::read_to_string("docs/15-standard-library.md").unwrap();
    for lib in ["Math", "Random", "Time"] {
        let mut documented: Vec<String> = table_rows(&text, &format!("## {}", lib))
            .iter()
            .flat_map(|row| called_names(row[0]))
            .map(|n| format!("{}.{}", lib, n))
            .collect();
        documented.sort();
        let mut registered: Vec<String> =
            FUNCTIONS.iter().filter(|f| f.name.starts_with(&format!("{}.", lib))).map(|f| f.name.to_string()).collect();
        registered.sort();
        assert_eq!(documented, registered, "docs/15-standard-library.md's `{}` table differs from the registry", lib);
    }
}
