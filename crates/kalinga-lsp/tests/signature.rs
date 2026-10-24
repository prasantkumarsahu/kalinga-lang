//! Signature help, on the programs the old JavaScript engine was tested with.

mod common;

use common::*;

const PROGRAM: &str = r#"
class Animal {
    name: str
    init(name: str) {
        self.name = name
    }
}

class Dog : Animal {
    init(name: str) {
        super(name)
    }
    fn fetch(item: str, times: int) -> bool {
        return true
    }
}

fn helper(a: int, b: str) -> int {
    return a
}
"#;

fn sig(body: &str) -> Option<(String, u32)> {
    signature(&format!("{}\nfn main() {{\n{}\n}}\n", PROGRAM, body))
}

#[test]
fn functions_constructors_methods_and_builtins() {
    assert_eq!(sig("    helper(1, |"), Some(("helper(a: int, b: str) -> int".into(), 1)));
    assert_eq!(sig("    let d = Dog(|").map(|s| s.0).as_deref(), Some("Dog(name: str)"));
    assert_eq!(
        sig("    let d = Dog(\"x\")\n    d.fetch(\"ball\", |"),
        Some(("fetch(item: str, times: int) -> bool".into(), 1))
    );
    assert_eq!(sig("    print(len(|").map(|s| s.0).as_deref(), Some("len(x: [T], {K: V} or str) -> int"));
    assert_eq!(sig("    let x = 1 + |"), None);
}

const V2: &str = r#"
enum Shape {
    Circle(r: float)
    Rect(w: float, h: float)
    Empty
}

abstract class Animal {
    nick: str
    abstract fn sound() -> str
    static fn create(nick: str) -> Animal { return Dog() }
}

class Dog : Animal {
    override fn sound() -> str { return "woof" }
}

fn apply(f: fn(int) -> int, x: int) -> int { return f(x) }
fn first<T>(xs: [T]) -> T? { return none }
"#;

fn sig2(body: &str) -> Option<(String, u32)> {
    signature(&format!("{}\nfn main() {{\n{}\n}}\n", V2, body))
}

#[test]
fn variants_statics_generics_and_function_values() {
    assert_eq!(sig2("    Shape.Rect(1.0, |").map(|s| s.0).as_deref(), Some("Shape.Rect(w: float, h: float)"));
    assert_eq!(sig2("    Animal.create(|").map(|s| s.0).as_deref(), Some("create(nick: str) -> Animal"));
    assert_eq!(sig2("    first<int>(|").map(|s| s.0).as_deref(), Some("first(xs: [T]) -> T?"));
    assert_eq!(sig2("    apply(fn(x) => x, |").map(|s| s.1), Some(1));
    assert_eq!(
        sig2("    let g = fn(a: int, b: str) => a\n    g(1, |").map(|s| s.0).as_deref(),
        Some("g(int, str) -> int")
    );
}

#[test]
fn library_functions() {
    assert_eq!(sig2("    Math.clamp(1, |").map(|s| s.0).as_deref(), Some("Math.clamp(x: T, lo: T, hi: T) -> T"));
}

#[test]
fn named_arguments_count_as_arguments() {
    assert_eq!(sig("    helper(a: 1, b: |"), Some(("helper(a: int, b: str) -> int".into(), 1)));
}
