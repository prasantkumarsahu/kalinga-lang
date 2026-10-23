//! Command-line behaviour that output snapshots (`tests/ui.rs`) cannot cover: flags,
//! `emit`, `check --stdin`, Rust build failures, executable contents and real windows.

use std::path::PathBuf;
use std::process::{Command, Output};

fn kalinga(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kalinga")).args(args).output().expect("failed to run kalinga")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A scratch file for one test.
fn scratch(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("kalinga-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

#[test]
fn version_flags() {
    for flag in ["-v", "-version", "--version", "version"] {
        let out = kalinga(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{}", flag);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("kalinga {}", env!("CARGO_PKG_VERSION")),
            "{}",
            flag
        );
    }
}

#[test]
fn emit_produces_rust() {
    let out = kalinga(&["emit", "examples/oop.kl"]);
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(rust.contains("struct Dog"));
    assert!(rust.contains("trait __I_Animal"));
    assert!(rust.contains("impl Show for Puppy"));
}

#[test]
fn check_reads_stdin() {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_kalinga"))
        .args(["check", "buffer.kl", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"fn main() {\n    let x: int = true\n}\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("buffer.kl:2:18: error: expected `int`, found `bool`"), "{}", stderr(&out));
}

#[test]
fn check_accepts_files_without_main() {
    // Files meant to be imported have no `main`; `check` accepts them, `run` does not.
    assert_eq!(kalinga(&["check", "examples/shapes/geometry.kl"]).status.code(), Some(0));
    let out = kalinga(&["run", "examples/shapes/geometry.kl"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("program has no `fn main()`"), "{}", stderr(&out));
}

#[test]
fn games_type_check() {
    // The games and std/game type-check without building macroquad.
    for f in ["examples/pong/main.kl", "examples/asteroids/main.kl"] {
        let out = kalinga(&["check", f]);
        assert!(out.status.success(), "{}: {}", f, stderr(&out));
    }
}

#[test]
fn rust_errors_in_inline_bodies_are_reported() {
    let src = scratch(
        "inline_bad.kl",
        "extern fn bad(x: int) -> int = rust { x + \"s\" }\nfn main() {\n    println(bad(1))\n}\n",
    );
    let out = kalinga(&["run", src.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("the Rust compiler rejected the program"), "{}", stderr(&out));
}

/// Built games must not need the Visual C++ runtime on Windows.
#[cfg(windows)]
#[test]
fn static_c_runtime() {
    let src = scratch("static_crt.kl", "fn main() {\n    println(\"hi\")\n}\n");
    let exe = src.with_extension("exe");
    let out = kalinga(&["build", src.to_str().unwrap(), "-o", exe.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let bytes = std::fs::read(&exe).unwrap().to_ascii_uppercase();
    assert!(!bytes.windows(12).any(|w| w == b"VCRUNTIME140"), "executable still depends on the VC++ runtime");
}

/// Opens a real window for a few frames and checks the screenshot. Needs a display and
/// the macroquad crate (internet the first time): `cargo test -- --ignored`.
#[test]
#[ignore]
fn game_window_smoke() {
    let dir = std::env::temp_dir().join("kalinga-tests").join("game");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("smoke.png");
    let _ = std::fs::remove_file(&shot);
    let src = dir.join("smoke.kl");
    std::fs::write(
        &src,
        format!(
            "import \"std/game\"\n\nclass Smoke : Game {{\n    frame: int\n\n    override fn update(input: Input, dt: float) {{\n        self.frame += 1\n        if self.frame >= 5 {{\n            input.quit()\n        }}\n    }}\n\n    override fn draw(screen: Screen) {{\n        screen.clear(Color.BLACK)\n        screen.rect(Rect(10.0, 10.0, 50.0, 50.0), Color.YELLOW)\n        if self.frame == 3 {{\n            screen.screenshot({:?})\n        }}\n    }}\n}}\n\nfn main() {{\n    let g = Smoke()\n    Window.run(\"smoke\", 200, 100, g)\n    println(\"frames\", g.frame)\n}}\n",
            shot.to_str().unwrap()
        ),
    )
    .unwrap();
    let out = kalinga(&["run", src.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"), "frames 5\n");
    let png = std::fs::read(&shot).expect("screenshot written");
    assert!(png.starts_with(b"\x89PNG"), "not a PNG");
}

#[test]
fn fmt_formats_files_and_check_reports_them() {
    let dir = std::env::temp_dir().join("kalinga-cli-fmt");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let file = dir.join("messy.kl");
    std::fs::write(&file, "fn main(){\nprintln( 1+2 )\n}\n").expect("write");
    let kalinga = env!("CARGO_BIN_EXE_kalinga");
    let check = std::process::Command::new(kalinga).args(["fmt", "--check"]).arg(&dir).output().expect("run");
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("not formatted:"));
    let fmt = std::process::Command::new(kalinga).arg("fmt").arg(&dir).output().expect("run");
    assert!(fmt.status.success());
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "fn main() {\n    println(1 + 2)\n}\n");
    let again = std::process::Command::new(kalinga).args(["fmt", "--check"]).arg(&dir).output().expect("run");
    assert!(again.status.success());
}
