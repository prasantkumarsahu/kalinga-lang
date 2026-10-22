//! Keeps the documentation honest: every complete program in `docs/*.md` must compile,
//! and every block marked ```` ```kalinga error ```` must be rejected.

use std::io::Write;
use std::process::{Command, Stdio};

struct Block {
    file: String,
    line: usize,
    expect_error: bool,
    code: String,
}

fn doc_blocks() -> Vec<Block> {
    let mut blocks = vec![];
    let mut paths: Vec<_> = std::fs::read_dir("docs")
        .expect("docs folder")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut current: Option<Block> = None;
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            match current.take() {
                None => {
                    if let Some(info) = trimmed.strip_prefix("```") {
                        let info = info.trim();
                        if info == "kalinga" || info == "kalinga error" {
                            current = Some(Block {
                                file: path.display().to_string(),
                                line: i + 1,
                                expect_error: info == "kalinga error",
                                code: String::new(),
                            });
                        }
                    }
                }
                Some(mut b) => {
                    if trimmed == "```" {
                        blocks.push(b);
                    } else {
                        b.code.push_str(line);
                        b.code.push('\n');
                        current = Some(b);
                    }
                }
            }
        }
    }
    blocks
}

fn check(code: &str) -> (bool, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kalinga"))
        .args(["check", "main.kl", "--stdin"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(code.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn doc_examples_compile() {
    let blocks = doc_blocks();
    let mut checked = 0;
    let mut failures = vec![];
    for b in &blocks {
        if !b.expect_error && !b.code.contains("fn main()") {
            continue; // a fragment, not a complete program
        }
        if b.code.contains("import \"") {
            continue; // needs other files; covered by the `imports` test
        }
        checked += 1;
        let (ok, stderr) = check(&b.code);
        if b.expect_error && ok {
            failures.push(format!("{}:{}: expected a compile error, but it compiled", b.file, b.line));
        } else if !b.expect_error && !ok {
            failures.push(format!("{}:{}: does not compile:\n{}", b.file, b.line, stderr));
        }
    }
    assert!(checked > 40, "only found {} doc examples", checked);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
