//! utkal projects end to end: path and git packages, the lock file, namespaces.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh folder for one test, with its own utkal cache.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("kalinga-tests").join("utkal").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The test's own package cache: `<workspace>/.home`.
fn home(dir: &Path) -> PathBuf {
    let ws = dir.ancestors().find(|a| a.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "utkal")).unwrap();
    ws.join(".home")
}

fn utkal(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_utkal"))
        .args(args)
        .current_dir(dir)
        .env("UTKAL_HOME", home(dir))
        .output()
        .expect("failed to run utkal")
}

fn kalinga(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kalinga"))
        .args(args)
        .current_dir(dir)
        .env("UTKAL_HOME", home(dir))
        .output()
        .expect("failed to run kalinga")
}

fn ok(out: &Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "failed:\n{}\n{}", stdout, String::from_utf8_lossy(&out.stderr));
    stdout
}

fn fails(out: &Output) -> String {
    assert!(!out.status.success(), "should have failed:\n{}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
}

#[test]
fn new_build_run_and_test() {
    let ws = workspace("new");
    ok(&utkal(&ws, &["new", "hello"]));
    let app = ws.join("hello");
    assert!(app.join("utkal.toml").is_file() && app.join("src/main.kl").is_file());
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "Hello from Kalinga!");
    assert!(app.join("target").join(format!("hello{}", std::env::consts::EXE_SUFFIX)).is_file());

    ok(&utkal(&ws, &["new", "greeter", "--lib"]));
    let lib = ws.join("greeter");
    let out = ok(&utkal(&lib, &["test"]));
    assert!(out.contains("test greets_by_name ... ok"), "{}", out);
    // A `tests/*.kl` program without `test fn`s passes when it exits with 0.
    write(&lib.join("tests/broken.kl"), "fn main() -> Result<void, str> {\n    return Err(\"nope\")\n}\n");
    let out = utkal(&lib, &["test"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("test broken.kl ... FAILED"));
    assert!(!out.status.success());
}

#[test]
fn path_packages_and_namespaces() {
    let ws = workspace("paths");
    ok(&utkal(&ws, &["new", "geo", "--lib"]));
    write(
        &ws.join("geo/src/lib.kl"),
        "struct Point {\n    x: int\n    y: int\n}\n\nfn origin() -> Point {\n    return Point(0, 0)\n}\n\nfn describe(p: Point) -> str {\n    return \"geo point {p.x},{p.y}\"\n}\n",
    );
    write(&ws.join("geo/src/extra.kl"), "fn extra() -> str {\n    return \"extra\"\n}\n");
    ok(&utkal(&ws, &["new", "pixels", "--lib"]));
    write(
        &ws.join("pixels/src/lib.kl"),
        "struct Point {\n    px: float\n}\n\nfn describe(p: Point) -> str {\n    return \"pixel {p.px}\"\n}\n",
    );
    ok(&utkal(&ws, &["new", "app"]));
    let app = ws.join("app");
    ok(&utkal(&app, &["add", "geo", "--path", "../geo"]));
    ok(&utkal(&app, &["add", "pixels", "--path", "../pixels"]));
    let manifest = std::fs::read_to_string(app.join("utkal.toml")).unwrap();
    assert!(manifest.contains("geo = { path = \"../geo\" }"), "{}", manifest);
    assert!(!app.join("utkal.lock").exists(), "path packages need no lock file");

    // Three `Point`s and two `describe`s: the project's own, plus one per package via `as`.
    write(
        &app.join("src/main.kl"),
        r#"import "geo" as geo
import "geo/extra"
import "pixels" as px

struct Point {
    label: str
}

fn describe(p: Point) -> str {
    return "mine {p.label}"
}

fn main() {
    let a: geo.Point = geo.Point(1, 2)
    let b = px.Point(0.5)
    println(geo.describe(a), px.describe(b), describe(Point("x")), geo.origin().x, extra())
}
"#,
    );
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "geo point 1,2 pixel 0.5 mine x 0 extra");
    // The compiler finds the project on its own (this is what editors run).
    ok(&kalinga(&app, &["check", "src/main.kl"]));

    // A name two plain imports both define is an error that suggests `as`.
    write(
        &app.join("src/main.kl"),
        "import \"geo\"\nimport \"pixels\"\n\nfn main() {\n    println(describe(origin()))\n}\n",
    );
    let err = fails(&utkal(&app, &["build"]));
    assert!(err.contains("`describe` is defined in both `geo` and `pixels`"), "{}", err);
    assert!(err.contains("import \"...\" as name"), "{}", err);

    // Error messages name package types as `package.Name`.
    write(
        &app.join("src/main.kl"),
        "import \"geo\"\nimport \"pixels\" as px\n\nfn main() {\n    let p: Point = px.Point(1.0)\n}\n",
    );
    let err = fails(&utkal(&app, &["build"]));
    assert!(err.contains("pixels.Point"), "{}", err);

    write(&app.join("src/main.kl"), "import \"geo\" as g\n\nfn main() {\n    println(g.nothing())\n}\n");
    assert!(fails(&utkal(&app, &["build"])).contains("package `g` has no `nothing`"));
    write(&app.join("src/main.kl"), "import \"missing\"\n\nfn main() {\n}\n");
    assert!(fails(&utkal(&app, &["build"])).contains("unknown package `missing`"));

    ok(&utkal(&app, &["remove", "pixels"]));
    assert!(!std::fs::read_to_string(app.join("utkal.toml")).unwrap().contains("pixels"));
}

#[test]
fn git_packages_versions_and_lock() {
    let ws = workspace("git");
    // A library with two tagged releases.
    ok(&utkal(&ws, &["new", "mathx", "--lib"]));
    let lib = ws.join("mathx");
    write(&lib.join("src/lib.kl"), "fn mathx_version() -> str {\n    return \"0.1.0\"\n}\n");
    git(&lib, &["init", "-q"]);
    git(&lib, &["add", "-A"]);
    git(&lib, &["commit", "-qm", "first"]);
    git(&lib, &["tag", "v0.1.0"]);
    write(&lib.join("src/lib.kl"), "fn mathx_version() -> str {\n    return \"0.1.5\"\n}\n");
    git(&lib, &["commit", "-qam", "fix"]);
    git(&lib, &["tag", "v0.1.5"]);
    write(&lib.join("src/lib.kl"), "fn mathx_version() -> str {\n    return \"0.2.0\"\n}\n");
    git(&lib, &["commit", "-qam", "breaking"]);
    git(&lib, &["tag", "v0.2.0"]);
    let url = lib.display().to_string();

    ok(&utkal(&ws, &["new", "app"]));
    let app = ws.join("app");
    write(&app.join("src/main.kl"), "import \"mathx\"\n\nfn main() {\n    println(mathx_version())\n}\n");
    ok(&utkal(&app, &["add", "mathx", "--git", &url, "--version", "0.1"]));
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "0.1.5", "newest 0.1.x");
    let lock = std::fs::read_to_string(app.join("utkal.lock")).unwrap();
    assert!(lock.contains("name = \"mathx\"") && lock.contains("commit = "), "{}", lock);

    // A new matching release does not change the build until `utkal update`.
    write(&lib.join("src/lib.kl"), "fn mathx_version() -> str {\n    return \"0.1.9\"\n}\n");
    git(&lib, &["commit", "-qam", "another fix"]);
    git(&lib, &["tag", "v0.1.9"]);
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "0.1.5", "the lock file pins the commit");
    ok(&utkal(&app, &["update"]));
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "0.1.9");

    // Exact tags.
    ok(&utkal(&app, &["add", "mathx", "--git", &url, "--tag", "v0.2.0"]));
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "0.2.0");

    // Two packages asking for incompatible versions.
    ok(&utkal(&ws, &["new", "helper", "--lib"]));
    let helper = ws.join("helper");
    ok(&utkal(&helper, &["add", "mathx", "--git", &url, "--version", "0.1"]));
    let err = fails(&utkal(&app, &["add", "helper", "--path", "../helper"]));
    assert!(
        err.contains("package `mathx` is required as") && err.contains("by `app`") && err.contains("by `helper`"),
        "{}",
        err
    );
    assert!(!std::fs::read_to_string(app.join("utkal.toml")).unwrap().contains("helper"), "a failed add is undone");
    // ...and compatible ones: tag v0.1.5 satisfies helper's "0.1".
    ok(&utkal(&app, &["add", "mathx", "--git", &url, "--tag", "v0.1.5"]));
    ok(&utkal(&app, &["add", "helper", "--path", "../helper"]));
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "0.1.5");

    // No release matches.
    let err = fails(&utkal(&app, &["add", "mathx", "--git", &url, "--version", "3"]));
    assert!(err.contains("no tag matches version `3`"), "{}", err);
    assert!(std::fs::read_to_string(app.join("utkal.toml")).unwrap().contains("tag = \"v0.1.5\""));
}

#[test]
fn imports_outside_a_project() {
    let ws = workspace("outside");
    write(&ws.join("main.kl"), "import \"mathx\"\n\nfn main() {\n}\n");
    let err = fails(&kalinga(&ws, &["check", "main.kl"]));
    assert!(err.contains("unknown package `mathx`") && err.contains("utkal new"), "{}", err);
}

/// Needs crates.io access: `[rust-dependencies]` are usable without `use crate`.
#[test]
#[ignore]
fn rust_dependencies_from_the_manifest() {
    let ws = workspace("rustdeps");
    ok(&utkal(&ws, &["new", "dice"]));
    let app = ws.join("dice");
    ok(&utkal(&app, &["add", "fastrand", "--rust", "2"]));
    write(
        &app.join("src/main.kl"),
        "extern fn roll() -> int = rust { fastrand::i64(1..=6) }\n\nfn main() {\n    let r = roll()\n    println(r >= 1 && r <= 6)\n}\n",
    );
    assert_eq!(ok(&utkal(&app, &["run"])).trim(), "true");
}

#[test]
fn fix_updates_0_4_code_once() {
    let ws = workspace("fix");
    ok(&utkal(&ws, &["new", "old"]));
    let app = ws.join("old");
    let manifest = std::fs::read_to_string(app.join("utkal.toml")).unwrap();
    write(&app.join("utkal.toml"), &manifest.replace(&format!("kalinga = \"{}\"", "0.5"), "kalinga = \"0.4\""));
    write(&app.join("src/main.kl"), "fn main() {\n    print(\"a\")\n}\n");
    ok(&utkal(&app, &["fix"]));
    assert_eq!(std::fs::read_to_string(app.join("src/main.kl")).unwrap(), "fn main() {\n    println(\"a\")\n}\n");
    assert!(std::fs::read_to_string(app.join("utkal.toml")).unwrap().contains("kalinga = \"0.5\""));
    // Now 0.5 code: a `print` added afterwards is meant, and a second fix leaves it.
    write(&app.join("src/main.kl"), "fn main() {\n    print(\"a\")\n}\n");
    ok(&utkal(&app, &["fix"]));
    assert_eq!(std::fs::read_to_string(app.join("src/main.kl")).unwrap(), "fn main() {\n    print(\"a\")\n}\n");
}
