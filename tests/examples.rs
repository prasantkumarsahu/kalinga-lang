use std::path::PathBuf;
use std::process::{Command, Output};

fn kalinga(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kalinga")).args(args).output().expect("failed to run kalinga")
}

fn run_file(path: &str) -> (i32, String, String) {
    let out = kalinga(&["run", path]);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Writes `source` to a temp file and runs it.
fn run_source(name: &str, source: &str) -> (i32, String, String) {
    let dir = std::env::temp_dir().join("kalinga-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(format!("{}.kl", name));
    std::fs::write(&path, source).unwrap();
    run_file(path.to_str().unwrap())
}

fn assert_compile_error(name: &str, source: &str, expected: &str) {
    let (code, _, stderr) = run_source(name, source);
    assert_eq!(code, 1, "expected a compile error for {}", name);
    assert!(stderr.contains(expected), "error for {} was:\n{}", name, stderr);
    assert!(!stderr.contains("internal compiler error"), "{}", stderr);
}

#[test]
fn hello_example() {
    let (code, out, err) = run_file("examples/hello.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "Hello, Kalinga!\nsum 1..10 = 55\nbig odd 5\nevens: [4] count: 5\nnothing yet\nnow got 42\narea: 12.56636 as int: 12\n"
    );
}

#[test]
fn oop_example() {
    let (code, out, err) = run_file("examples/oop.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "Rex says woof and has 4 legs\n\
         Bit says yip (tiny) and has 4 legs\n\
         Tweety says tweet and has 2 legs\n\
         Generic says ... and has 4 legs\n\
         Dog(Rex, tricks=[sit, roll])\n\
         Dog(Nib, tricks=[])\n\
         fewest legs: Tweety\n\
         counter: 3\n\
         Rexy\n"
    );
}

#[test]
fn fib_example() {
    let (code, out, err) = run_file("examples/fib.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "fib(35) = 9227465\n");
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
fn error_two_base_classes() {
    assert_compile_error("two_bases", "class A {}\nclass B {}\nclass C : A, B {}\nfn main() {}", "can extend only one class");
}

#[test]
fn error_missing_interface_method() {
    assert_compile_error("missing_iface", "interface S { fn show() -> str }\nclass A : S {}\nfn main() {}", "does not implement `S.show");
}

#[test]
fn error_missing_override() {
    assert_compile_error("no_override", "class A { fn f() {} }\nclass B : A { fn f() {} }\nfn main() {}", "mark it `override fn`");
}

#[test]
fn error_assign_to_let() {
    assert_compile_error("let_assign", "fn main() {\n    let x = 1\n    x = 2\n}", "declared with `let`");
}

#[test]
fn error_type_mismatch() {
    assert_compile_error("mismatch", "fn main() { let x: int = \"hi\" }", "expected `int`, found `str`");
}

#[test]
fn error_unknown_field() {
    assert_compile_error("unknown_field", "class A { a: int }\nfn main() {\n    let a = A()\n    print(a.b)\n}", "has no field `b`");
}

#[test]
fn error_missing_return() {
    assert_compile_error("no_return", "fn f(x: int) -> int { if x > 0 { return 1 } }\nfn main() {}", "must return");
}

#[test]
fn runtime_errors_are_safe() {
    let (code, _, err) = run_source("oob", "fn main() {\n    let xs = [1, 2]\n    print(xs[5])\n}");
    assert_eq!(code, 101);
    assert!(err.contains("runtime error: index out of bounds"), "{}", err);

    let (code, _, err) = run_source("overflow", "fn main() {\n    var x = 9223372036854775807\n    x += 1\n}");
    assert_eq!(code, 101);
    assert!(err.contains("overflow"), "{}", err);
}
