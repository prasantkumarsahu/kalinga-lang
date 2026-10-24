//! The formatter on the repository's own code.

use std::path::{Path, PathBuf};

fn kl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "target") {
            kl_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "kl") {
            out.push(path);
        }
    }
}

fn files(dirs: &[&str]) -> Vec<PathBuf> {
    let mut out = vec![];
    for d in dirs {
        kl_files(Path::new(d), &mut out);
    }
    out
}

/// Examples and the standard modules are written in the standard layout
/// (`kalinga fmt examples crates/kalinga-driver/std` fixes them).
#[test]
fn examples_and_std_are_formatted() {
    for file in files(&["examples", "crates/kalinga-driver/std"]) {
        let source = std::fs::read_to_string(&file).expect("read");
        let formatted = kalinga_fmt::format(&source).expect("formats");
        assert!(formatted == source, "{} is not formatted; run `kalinga fmt` on it", file.display());
    }
}

/// On every program in the repository (UI tests included, whose layout is kept as
/// test data), formatting keeps the tokens (`format` checks that itself) and a second
/// pass changes nothing.
#[test]
fn formatting_is_stable_everywhere() {
    let mut checked = 0;
    for file in files(&["examples", "crates/kalinga-driver/std", "tests/ui"]) {
        let source = std::fs::read_to_string(&file).expect("read");
        // Some UI tests are about code that does not lex.
        let Ok(once) = kalinga_fmt::format(&source) else { continue };
        let twice = kalinga_fmt::format(&once).expect("formats again");
        assert!(once == twice, "formatting {} twice differs", file.display());
        checked += 1;
    }
    assert!(checked > 100, "only {} files checked", checked);
}
