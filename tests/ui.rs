//! UI snapshot tests.
//!
//! Every `tests/ui/<group>/<name>.kl` (one level deep; deeper folders hold helper files) is
//! a test. It is run with `kalinga run`, or `kalinga check` when its first line is
//! `// ui: check` (`kalinga test` for `// ui: test`), and its output is compared with the
//! files next to it:
//! `<name>.stdout`, `<name>.stderr` (both default to empty) and `<name>.exit` (defaults
//! to 0). Programs from `examples/` are snapshotted under `tests/ui/examples/`.
//!
//! `BLESS=1 cargo test --test ui` rewrites the snapshots from the current output. Tests
//! whose first line is `// ui: network` (they download crates) run only with `--ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Example programs and the snapshot name they are stored under in `tests/ui/examples`.
const EXAMPLES: &[(&str, &str)] = &[
    ("examples/hello.kl", "hello"),
    ("examples/oop.kl", "oop"),
    ("examples/fib.kl", "fib"),
    ("examples/inventory.kl", "inventory"),
    ("examples/pipeline.kl", "pipeline"),
    ("examples/particles.kl", "particles"),
    ("examples/shapes/main.kl", "shapes"),
    ("examples/interop/main.kl", "interop"),
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Run,
    Check,
    Test,
}

struct Case {
    /// The program, relative to the repository root.
    source: PathBuf,
    /// Snapshot path without extension.
    snapshot: PathBuf,
    mode: Mode,
    network: bool,
}

struct Outcome {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn cases() -> Vec<Case> {
    let mut out = vec![];
    let mut groups: Vec<PathBuf> =
        std::fs::read_dir("tests/ui").expect("tests/ui").map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    groups.sort();
    for group in groups {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&group)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "kl"))
            .collect();
        files.sort();
        for source in files {
            let first = std::fs::read_to_string(&source).unwrap().lines().next().unwrap_or("").trim().to_string();
            out.push(Case {
                snapshot: source.with_extension(""),
                mode: match first.as_str() {
                    "// ui: check" => Mode::Check,
                    "// ui: test" => Mode::Test,
                    _ => Mode::Run,
                },
                network: first == "// ui: network",
                source,
            });
        }
    }
    for (source, name) in EXAMPLES {
        out.push(Case {
            source: PathBuf::from(source),
            snapshot: Path::new("tests/ui/examples").join(name),
            mode: Mode::Run,
            network: false,
        });
    }
    out
}

/// Line endings and path separators differ between systems; snapshots use `\n` and `/`.
/// Build progress lines (printed only when crates are compiled for the first time) are dropped.
fn normalize(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n").replace('\\', "/");
    text.lines().filter(|l| !l.starts_with("building with cargo")).map(|l| format!("{}\n", l)).collect()
}

fn run(case: &Case) -> Outcome {
    let cmd = match case.mode {
        Mode::Run => "run",
        Mode::Check => "check",
        Mode::Test => "test",
    };
    let source = case.source.to_str().unwrap().replace('\\', "/");
    let out = Command::new(env!("CARGO_BIN_EXE_kalinga")).args([cmd, &source]).output().expect("failed to run kalinga");
    Outcome { stdout: normalize(&out.stdout), stderr: normalize(&out.stderr), exit: out.status.code().unwrap_or(-1) }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).map(|s| s.replace("\r\n", "\n")).unwrap_or_default()
}

/// Writes `text` to `path`, or removes the file when `text` is empty (the default).
fn write(path: &Path, text: &str) {
    if text.is_empty() {
        let _ = std::fs::remove_file(path);
    } else {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn compare(case: &Case, got: &Outcome) -> Option<String> {
    let snap = |ext: &str| case.snapshot.with_extension(ext);
    let expected_exit: i32 = read(&snap("exit")).trim().parse().unwrap_or(0);
    let mut problems = vec![];
    if got.exit != expected_exit {
        problems.push(format!("exit code: expected {}, got {}", expected_exit, got.exit));
    }
    for (ext, text) in [("stdout", &got.stdout), ("stderr", &got.stderr)] {
        let expected = read(&snap(ext));
        if &expected != text {
            problems.push(format!("{}:\n--- expected\n{}--- got\n{}", ext, expected, text));
        }
    }
    if problems.is_empty() { None } else { Some(format!("{}\n{}", case.source.display(), problems.join("\n"))) }
}

fn bless(case: &Case, got: &Outcome) {
    write(&case.snapshot.with_extension("stdout"), &got.stdout);
    write(&case.snapshot.with_extension("stderr"), &got.stderr);
    write(
        &case.snapshot.with_extension("exit"),
        &if got.exit == 0 { String::new() } else { format!("{}\n", got.exit) },
    );
}

fn run_all(network: bool) {
    let blessing = std::env::var_os("BLESS").is_some();
    let queue = Mutex::new(cases().into_iter().filter(|c| c.network == network).collect::<Vec<_>>());
    let failures = Mutex::new(vec![]);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(case) = queue.lock().unwrap().pop() {
                    let got = run(&case);
                    if blessing {
                        bless(&case, &got);
                    } else if let Some(f) = compare(&case, &got) {
                        failures.lock().unwrap().push(f);
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} UI test(s) failed (BLESS=1 to accept new output):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn ui() {
    run_all(false);
}

/// Downloads crates the first time: `cargo test --test ui -- --ignored`.
#[test]
#[ignore]
fn ui_network() {
    run_all(true);
}
