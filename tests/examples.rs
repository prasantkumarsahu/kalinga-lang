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
fn version_flags() {
    for flag in ["-v", "-version", "--version", "version"] {
        let out = kalinga(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{}", flag);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("kalinga {}", env!("CARGO_PKG_VERSION")), "{}", flag);
    }
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
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("buffer.kl:2:18: error: expected `int`, found `bool`"), "{}", err);

    assert_eq!(kalinga(&["check", "examples/oop.kl"]).status.code(), Some(0));

    // Files meant to be imported have no `main`; `check` accepts them, `run` does not.
    assert_eq!(kalinga(&["check", "examples/shapes/geometry.kl"]).status.code(), Some(0));
    let (code, _, err) = run_file("examples/shapes/geometry.kl");
    assert_eq!(code, 1);
    assert!(err.contains("program has no `fn main()`"), "{}", err);
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

// ---------------------------------------------------------------- v0.2 features

fn assert_runs(name: &str, source: &str, expected: &str) {
    let (code, out, err) = run_source(name, source);
    assert_eq!(code, 0, "{} failed:\n{}", name, err);
    assert_eq!(out, expected, "output of {}", name);
}

#[test]
fn string_interpolation() {
    assert_runs(
        "interp",
        "fn main() {\n    let name = \"Asha\"\n    let xs = [1, 2]\n    print(\"Hi {name}! {len(xs) * 10}% {xs} {{literal}}\")\n}\n",
        "Hi Asha! 20% [1, 2] {literal}\n",
    );
}

#[test]
fn maps() {
    assert_runs(
        "maps",
        "fn main() {\n    var m: {str: int} = {}\n    m[\"b\"] = 2\n    m[\"a\"] = 1\n    m[\"a\"] += 5\n    var lists = {1: [10]}\n    lists[1].push(20)\n    print(m, m[\"a\"], m.get(\"z\"), m.has(\"b\"), len(m))\n    print(m.keys(), m.values(), lists, m.remove(\"b\"), m)\n    for k in lists { print(k) }\n}\n",
        "{a: 6, b: 2} 6 none true 2\n[a, b] [6, 2] {1: [10, 20]} 2 {a: 6}\n1\n",
    );
    let (code, _, err) = run_source("map_missing", "fn main() {\n    let m = {\"a\": 1}\n    print(m[\"b\"])\n}\n");
    assert_eq!(code, 101);
    assert!(err.contains("key not found: b"), "{}", err);
}

#[test]
fn error_map_and_interp() {
    assert_compile_error("map_empty", "fn main() { let m = {} }", "cannot infer the type of `{}`");
    assert_compile_error("map_key", "fn main() { let m: {float: int} = {} }", "map keys must be `int`, `str` or `bool`");
    assert_compile_error("interp_obj", "class A {}\nfn main() {\n    let a = A()\n    print(\"{a}\")\n}", "cannot put a `A` value into a string");
    assert_compile_error("interp_brace", "fn main() { print(\"a } b\") }", "unmatched `}` in string");
}

#[test]
fn imports() {
    let dir = std::env::temp_dir().join("kalinga-tests").join("imports");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/geo.kl"), "import \"util.kl\"\n\nclass Square {\n    side: int\n    init(side: int) {\n        self.side = side\n    }\n    fn area() -> int {\n        return twice(self.side) / 2 * self.side\n    }\n}\n").unwrap();
    std::fs::write(dir.join("lib/util.kl"), "import \"geo.kl\"\n\nfn twice(n: int) -> int {\n    return n * 2\n}\n").unwrap();
    std::fs::write(dir.join("main.kl"), "import \"lib/geo.kl\"\n\nfn main() {\n    print(Square(3).area(), twice(5))\n}\n").unwrap();
    let (code, out, err) = run_file(dir.join("main.kl").to_str().unwrap());
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "9 10\n");

    std::fs::write(dir.join("lib/bad.kl"), "fn oops() {\n    let x: int = \"s\"\n}\n").unwrap();
    std::fs::write(dir.join("bad_main.kl"), "import \"lib/bad.kl\"\nfn main() {}\n").unwrap();
    let (code, _, err) = run_file(dir.join("bad_main.kl").to_str().unwrap());
    assert_eq!(code, 1);
    assert!(err.contains("bad.kl:2:18: error: expected `int`, found `str`"), "{}", err);

    std::fs::write(dir.join("missing.kl"), "import \"nope.kl\"\nfn main() {}\n").unwrap();
    let (_, _, err) = run_file(dir.join("missing.kl").to_str().unwrap());
    assert!(err.contains("cannot find `nope.kl`"), "{}", err);
}

#[test]
fn enums_and_match() {
    assert_runs(
        "enums",
        "enum Shape {\n    Circle(r: float)\n    Rect(w: float, h: float)\n    Empty\n}\n\nfn area(s: Shape) -> float {\n    match s {\n        Circle(r) => return 3.0 * r * r\n        Rect(w, h) => {\n            return w * h\n        }\n        Empty => return 0.0\n    }\n}\n\nfn main() {\n    for s in [Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Empty] {\n        print(s, area(s))\n    }\n    print(Shape.Empty == Shape.Empty)\n    match \"b\" {\n        \"a\" => print(1)\n        _ => print(2)\n    }\n}\n",
        "Circle(1.0) 3.0\nRect(2.0, 3.0) 6.0\nEmpty 0.0\ntrue\n2\n",
    );
}

#[test]
fn error_match() {
    let e = "enum C { R, G, B }\n";
    assert_compile_error("match_missing", &format!("{}fn main() {{\n    match C.R {{\n        R => print(1)\n    }}\n}}", e), "missing: G, B");
    assert_compile_error("match_dup", &format!("{}fn main() {{\n    match C.R {{\n        R => print(1)\n        R => print(2)\n        _ => print(3)\n    }}\n}}", e), "already handled");
    assert_compile_error("match_int", "fn main() {\n    match 1 {\n        1 => print(1)\n    }\n}", "add a `_ =>` arm");
    assert_compile_error("enum_self", "enum T { Leaf, Node(l: T) }\nfn main() {}", "contains itself");
    assert_compile_error("enum_variant", &format!("{}fn main() {{ print(C.X) }}", e), "has no variant `X`");
}

#[test]
fn oop_extras() {
    let (code, out, err) = run_file("tests/oop_extras.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "Rex says woof (42)\nmeow Tom hi Tom\nhi A\nhi B\n9 10\n");
}

#[test]
fn error_oop_extras() {
    let a = "abstract class A {\n    private x: int = 1\n    abstract fn f() -> int\n    private fn g() {}\n    static fn make() -> int {\n        return 1\n    }\n}\n";
    assert_compile_error("abstract_missing", &format!("{}class B : A {{}}\nfn main() {{}}", a), "must implement abstract method `A.f`");
    assert_compile_error("abstract_new", &format!("{}fn main() {{ let a = A() }}", a), "`A` is abstract and cannot be created");
    assert_compile_error(
        "private_field",
        &format!("{}class B : A {{\n    override fn f() -> int {{\n        return self.x\n    }}\n}}\nfn main() {{}}", a),
        "field `x` is private to `A`",
    );
    assert_compile_error("static_on_obj", "class C {\n    fn f() {}\n}\nfn main() { C.f() }", "is not a `static fn`");
    assert_compile_error("iface_cycle", "interface I : J {}\ninterface J : I {}\nfn main() {}", "extends itself");
}

#[test]
fn downcast_and_self_value() {
    let (code, out, err) = run_file("tests/downcast.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "Rex fetches\nfalse true\nTom is not a dog\nfalse true\nBit fetches\ntrue true\npuppy Nib\ntrue\nR true\nparent of 2 is 1\n1\n"
    );
    assert_compile_error("downcast_never", "class A {}\nclass C {}\nfn main() {\n    print(A() as C)\n}", "can never be a `C`");
    assert_compile_error("self_in_init", "class D {\n    init() {\n        let s = self\n    }\n}\nfn main() {}", "inside `init`");
}

#[test]
fn lambdas() {
    let (code, out, err) = run_file("tests/lambdas.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "[30, 10, 40, 10, 50, 90, 20, 60]\n[4, 2, 6] 5 none\n[6, 2, 8, 2, 10, 18, 4, 12] [n3, n1, n4, n1, n5, n9, n2, n6]\n\
         [1, 1, 2, 3, 4, 5, 6, 9]\n[fig, pear, kiwi, banana]\n[-1.0, 2.5, 3.25]\n6 15 42 101\nitem-1\nnobody handles Asha\n\
         Ravi clicked OK\n14\n12\n-7\nside effect\n"
    );
    assert_compile_error("lambda_infer", "fn main() { let f = fn(x) => x }", "cannot tell the type of `x`");
    assert_compile_error("lambda_capture", "fn main() {\n    var n = 0\n    let f = fn() {\n        n += 1\n    }\n}", "cannot change `n` inside a lambda");
    assert_compile_error("lambda_self", "class A {\n    x: int\n    fn m() {\n        let f = fn() => self.x\n    }\n}\nfn main() {}", "`self` cannot be used inside a lambda");
}

#[test]
fn generics() {
    let (code, out, err) = run_file("tests/generics.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "2 2 2 1 none\na\nx 5 5 x\n15\n3 z 3\n[1, 2] [ab, ab, ab]\nRex\nnone\n");
    let h = "class Holder<T> {\n    value: T\n    fn get() -> T {\n        return self.value\n    }\n}\n";
    assert_compile_error("generic_infer", &format!("{}fn main() {{ let b = Holder() }}", h), "cannot tell what `T` is");
    assert_compile_error("generic_arity", &format!("{}fn main() {{ let b: Holder<int, str> = Holder() }}", h), "needs 1 type argument");
    assert_compile_error("generic_ops", "fn add<T>(a: T, b: T) -> T {\n    return a + b\n}\nfn main() {}", "cannot apply `+` to `T` and `T`");
    let (code, _, err) = run_source("generic_unset", &format!("{}fn main() {{\n    print(Holder<int>().get())\n}}\n", h));
    assert_eq!(code, 101);
    assert!(err.contains("field `value` was read before it was set"), "{}", err);
}

#[test]
fn v02_examples() {
    let cases = [
        (
            "examples/inventory.kl",
            "sold 12 apples: true\nsold 9 pears: false\nsold 1 kiwi: false\n  apple: 3 at 0.5  (low!)\n  pear: 3 at 0.75  (low!)\nstock value: 3.75\n",
        ),
        (
            "examples/shapes/main.kl",
            "circle: area 3.14159, round\nrect: area 6.0, 4 corners\nrect: area 16.0, 4 corners\nlargest is the rect\na rect of 2.0 x 3.0\na rect of 4.0 x 4.0\n",
        ),
        (
            "examples/pipeline.kl",
            "[Paris (2161k), Bhubaneswar (837k), Cuttack (606k)]\nFR: [Lyon, Paris]\nIN: [Bhubaneswar, Cuttack, Puri]\n[apple, banana]\n",
        ),
    ];
    for (file, expected) in cases {
        let (code, out, err) = run_file(file);
        assert_eq!(code, 0, "{}: {}", file, err);
        assert_eq!(out, expected, "{}", file);
    }
}

// ---------------------------------------------------------------- v0.3 features

#[test]
fn structs_and_operators() {
    let (code, out, err) = run_file("tests/structs.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "Vec2(x: 1.0, y: 2.0) Vec2(x: 10.0, y: 2.0) true\n\
         Vec2(x: 11.0, y: 4.0) Vec2(x: -9.0, y: 0.0) Vec2(x: 2.0, y: 4.0) Vec2(x: -1.0, y: -2.0) Vec2(x: 0.0, y: 0.0)\n\
         Vec2(x: 7.0, y: 9.0) 7.0 Vec2(x: 7.0, y: 9.0)\nfalse true\ntrue false\nVec2(x: 5.0, y: 0.0) Vec2(x: 3.0, y: 3.0) 1\n"
    );
    let v = "struct V {\n    x: float\n    mut fn grow() {\n        self.x += 1.0\n    }\n    op +(o: V) -> V {\n        return V(self.x + o.x)\n    }\n}\n";
    assert_compile_error("struct_mut", "struct W {\n    x: int\n    fn bad() {\n        self.x = 1\n    }\n}\nfn main() {}", "declare it `mut fn`");
    assert_compile_error("struct_let", &format!("{}fn main() {{\n    let v = V(1.0)\n    v.grow()\n}}", v), "`grow` is a `mut fn`");
    assert_compile_error("struct_op", &format!("{}fn main() {{\n    print(V(1.0) * 2.0)\n}}", v), "has no `op *` taking a `float`");
    assert_compile_error("struct_args", &format!("{}fn main() {{ print(V()) }}", v), "takes 1 argument(s)");
    assert_compile_error("struct_loop", "struct L { next: L? }\nfn main() {}", "contains itself");
}

#[test]
fn math_random_time() {
    let (code, out, err) = run_file("tests/stdlib.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "4.0 1024.0 3 2.5 3 1.5\n2.0 3.0 3.0 10 0.0\n2.5 -1 100 5.0 true\ntrue\nranges ok: true\n[1, 2, 3, 4, 5, 6] true none\ntrue true\n"
    );
    assert_compile_error("math_mixed", "fn main() { print(Math.min(1, 2.0)) }", "all `int` or all `float`");
    let (code, _, err) = run_source("rand_bounds", "fn main() {\n    print(Random.int(5, 5))\n}\n");
    assert_eq!(code, 101);
    assert!(err.contains("the second number must be greater"), "{}", err);
}

#[test]
fn rust_interop() {
    let (code, out, err) = run_file("tests/interop/main.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "  0.0: #\n  2.0: ##\n  4.0: ###\n  8.0: #\n14.0 hello none\n5.0 3.0\n");
    assert_compile_error("extern_dup", "extern fn print(x: int)\nfn main() {}", "already a built-in function");
    assert_compile_error("rust_missing", "use rust \"nope.rs\"\nfn main() {}", "cannot find Rust file `nope.rs`");
}

/// Needs internet the first time (downloads the `fastrand` crate): `cargo test -- --ignored`.
#[test]
#[ignore]
fn rust_crates() {
    let (code, out, err) = run_file("tests/crates/main.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "fastrand works: true true\n");
}

#[test]
fn v03_examples() {
    let (code, out, err) = run_file("examples/particles.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "frame 0: 10000 particles, highest at 8 cm\nframe 30: 10000 particles, highest at 122 cm\nframe 60: 9841 particles, highest at -8 cm\n\
         frame 90: 4776 particles, highest at -384 cm\n120 frames of 10000 particles took under a second: true\n"
    );
    let (code, out, err) = run_file("examples/interop/main.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "mean 5.0, std dev 2.0\n[[7.0, 10.0], [15.0, 22.0]]\n4.0\n");
}

// ---------------------------------------------------------------- v0.4 features

#[test]
fn inline_rust_bodies() {
    let (code, out, err) = run_file("tests/inline_rust.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "HI! 3 14.0\n");
    let (code, _, err) = run_source("inline_bad", "extern fn bad(x: int) -> int = rust { x + \"s\" }\nfn main() {\n    print(bad(1))\n}\n");
    assert_eq!(code, 1);
    assert!(err.contains("the Rust compiler rejected the program"), "{}", err);
}

#[test]
fn std_modules() {
    let (code, out, err) = run_file("tests/std_math2d.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "5.0 Vec2(x: 0.6, y: 0.8) Vec2(x: -3.0, y: -4.0) Vec2(x: 1.5, y: 2.0) Vec2(x: 1.5, y: 2.0)\ntrue true Vec2(x: 8.0, y: 9.0)\n"
    );
    assert_compile_error("std_missing", "import \"std/nope\"\nfn main() {}", "there is no standard module `std/nope`");
}

/// Built games must not need the Visual C++ runtime on Windows.
#[cfg(windows)]
#[test]
fn static_c_runtime() {
    let dir = std::env::temp_dir().join("kalinga-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("static_crt.kl");
    std::fs::write(&src, "fn main() {\n    print(\"hi\")\n}\n").unwrap();
    let exe = dir.join("static_crt.exe");
    let out = kalinga(&["build", src.to_str().unwrap(), "-o", exe.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&exe).unwrap().to_ascii_uppercase();
    assert!(!bytes.windows(12).any(|w| w == b"VCRUNTIME140"), "executable still depends on the VC++ runtime");
}

/// Needs internet the first time: `cargo test -- --ignored`.
#[test]
#[ignore]
fn crate_features() {
    let (code, out, err) = run_file("tests/crates/features.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "roll ok: true\n");
}

#[test]
fn error_handling() {
    let (code, out, err) = run_file("tests/results.kl");
    assert_eq!(code, 1, "main returns Err, so the exit code is 1");
    assert_eq!(
        out,
        "Ok(3) Err(empty) Err(level must be at least 1, got 0)\ntotal 6\nmissing file: missing\ntrue 7 1 7 empty 7\n40 none\nsetup ok\n"
    );
    assert!(err.contains("error: setup failed"), "{}", err);
    let f = "fn f() -> Result<int, str> {\n    return Ok(1)\n}\n";
    assert_compile_error("try_void", &format!("{}fn main() {{\n    let x = f()?\n}}", f), "can only be used in a function that returns a `Result`");
    assert_compile_error("try_types", &format!("{}fn g() -> Result<int, int> {{\n    let x = f()?\n    return Ok(x)\n}}\nfn main() {{}}", f), "passes on a `str` error");
    assert_compile_error("ok_untyped", "fn main() { let r = Ok(5) }", "cannot tell which `Result`");
    assert_compile_error("result_match", &format!("{}fn main() {{\n    match f() {{\n        Ok(v) => print(v)\n    }}\n}}", f), "missing: Err");
    let (code, _, err) = run_source("unwrap_err", "fn main() {\n    let r: Result<int, str> = Err(\"disk full\")\n    print(r.unwrap())\n}\n");
    assert_eq!(code, 101);
    assert!(err.contains("called unwrap() on an error: disk full"), "{}", err);
}

#[test]
fn generic_structs_and_left_operators() {
    let (code, out, err) = run_file("tests/generic_structs.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(
        out,
        "# . 6 Grid(w: 3, h: 2, cells: [., ., ., ., #, .])\n[5, 0, 0, 0] true\nPair(first: 3, second: x) 4\n\
         Vec2(x: 2.0, y: 4.0) Vec2(x: 3.0, y: 6.0) Vec2(x: 1.0, y: 2.0)\ne\n"
    );
    assert_compile_error("gstruct_arity", "struct B<T> { v: T }\nfn main() { let b: B = B(1) }", "needs 1 type argument");
    assert_compile_error("gstruct_print", "struct B<T> { v: T }\nclass C {}\nfn main() { print(B(C())) }", "cannot print");
}

#[test]
fn threads() {
    let (code, out, err) = run_file("tests/threads.kl");
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "joined: 5050 true\nworker handled 5 jobs\ntotal 150\ncounter 8000\npar_map equal: true cores: true\n");
    let p = "class Player {\n    hp: int\n}\n";
    assert_compile_error("spawn_object", &format!("{}fn main() {{\n    let p = Player()\n    let t = Thread.spawn(fn() => p.hp)\n}}", p), "cannot use `p` here");
    assert_compile_error("channel_object", &format!("{}fn main() {{ let c = Channel<Player>() }}", p), "a channel can only carry values");
    assert_compile_error("spawn_fn_name", "fn work() -> int {\n    return 1\n}\nfn main() { let t = Thread.spawn(work) }", "needs a lambda written right here");
    let (code, out, err) = run_source(
        "thread_fail",
        "fn main() {\n    let t = Thread.spawn(fn() -> int {\n        let xs = [1]\n        return xs[5]\n    })\n    print(\"before join\")\n    print(t.join())\n}\n",
    );
    assert_eq!(code, 101);
    assert_eq!(out, "before join\n");
    assert!(err.contains("a thread stopped with an error"), "{}", err);
}
