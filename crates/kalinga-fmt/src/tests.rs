use super::format;

/// Formats `input`, checks the result is `expected`, and that formatting it again keeps it.
fn check(input: &str, expected: &str) {
    let out = format(input).expect("formats");
    assert_eq!(out, expected, "\n--- got\n{}--- expected\n{}", out, expected);
    assert_eq!(format(&out).expect("formats again"), out, "formatting is stable");
}

#[test]
fn spacing() {
    check(
        "fn add(a:int,b :int)->int{\nreturn a+b*2\n}\n",
        "fn add(a: int, b: int) -> int {\n    return a + b * 2\n}\n",
    );
    check(
        "fn main() {\n    let x = - 1\n    let y = x-1\n    println( f( x ) [0], !true )\n}\n",
        "fn main() {\n    let x = -1\n    let y = x - 1\n    println(f(x)[0], !true)\n}\n",
    );
}

#[test]
fn bounds() {
    check(
        "fn f<T:Show+Eq>(x: T) {}
class Box<T :Ord> : Named {}
struct P<A: Show, B> {}
fn g() {
    if a < b + c > d {}
}
",
        "fn f<T: Show + Eq>(x: T) {}
class Box<T: Ord> : Named {}
struct P<A: Show, B> {}
fn g() {
    if a < b + c > d {}
}
",
    );
}

#[test]
fn generics_and_comparisons() {
    check(
        "fn f(r: Result< int , str >) -> Stack<str> {\n    if a<b && c > d {\n        let c = Channel <int>()\n    }\n    return first<int>(xs)\n}\n",
        "fn f(r: Result<int, str>) -> Stack<str> {\n    if a < b && c > d {\n        let c = Channel<int>()\n    }\n    return first<int>(xs)\n}\n",
    );
}

#[test]
fn maps_blocks_ranges_and_lambdas() {
    check(
        "fn main() {\n    let m: { str : int } = { \"a\" : 1 }\n    for i in 0 .. 3 { println(i) }\n    let f = fn (x) => x\n    let e = {}\n}\n",
        "fn main() {\n    let m: {str: int} = {\"a\": 1}\n    for i in 0..3 { println(i) }\n    let f = fn(x) => x\n    let e = {}\n}\n",
    );
}

#[test]
fn comments_and_blank_lines_are_kept() {
    check(
        "// header\n\n\n/// Doc.\nfn main() {   // trailing\n\n    let x = 1 /* inline */ + 2\n\n\n    // alone\n    println(x)\n\n}\nfn other() {}\n",
        "// header\n\n/// Doc.\nfn main() { // trailing\n    let x = 1 /* inline */ + 2\n\n    // alone\n    println(x)\n}\n\nfn other() {}\n",
    );
}

#[test]
fn indentation_follows_brackets() {
    check(
        "class A {\nfn f() {\nlet xs = [1,\n2]\nlet ys = xs.map(fn(x) {\nreturn x\n})\nlet total = 1 +\n2\nlet zs = xs\n.filter(fn(x) => x > 1)\n}\n}\n",
        "class A {\n    fn f() {\n        let xs = [1,\n            2]\n        let ys = xs.map(fn(x) {\n            return x\n        })\n        let total = 1 +\n            2\n        let zs = xs\n            .filter(fn(x) => x > 1)\n    }\n}\n",
    );
}

#[test]
fn one_statement_per_line() {
    check(
        "fn main() { let a = 1; let b = 2 }\nfn g() {\n    f(); g();\n}\n",
        "fn main() { let a = 1; let b = 2 }\nfn g() {\n    f()\n    g()\n}\n",
    );
}

#[test]
fn trailing_comments_line_up() {
    check(
        "fn main() {\n    let a = 10   // int\n    let bb: float = 2.5 // float\n    let c = 1\n    f()     // call\n}\n",
        "fn main() {\n    let a = 10          // int\n    let bb: float = 2.5 // float\n    let c = 1\n    f() // call\n}\n",
    );
}

#[test]
fn one_line_declarations_stay_together() {
    check(
        "extern fn a() -> int = rust { 1 }\nextern fn b() -> int = rust { 2 }\nconst X: int = 1\nfn f() {\n    a()\n}\nfn g() {}\n",
        "extern fn a() -> int = rust { 1 }\nextern fn b() -> int = rust { 2 }\nconst X: int = 1\nfn f() {\n    a()\n}\n\nfn g() {}\n",
    );
}

#[test]
fn rust_bodies_and_operators_stay() {
    let input = "extern fn twice(x: int) -> int = rust {\n  x  *  2\n}\nstruct V {\n    x: float\n    op -() -> V { return V(-self.x) }\n    op +(o: V) -> V { return V(self.x + o.x) }\n}\n";
    let expected = "extern fn twice(x: int) -> int = rust {\n  x  *  2\n}\n\nstruct V {\n    x: float\n    op -() -> V { return V(-self.x) }\n    op +(o: V) -> V { return V(self.x + o.x) }\n}\n";
    check(input, expected);
}

#[test]
fn strings_keep_their_text() {
    check(
        "fn main() {\n    println(\"a  {x+1}  \\n\",   \"{{}}\")\n}\n",
        "fn main() {\n    println(\"a  {x+1}  \\n\", \"{{}}\")\n}\n",
    );
}

#[test]
fn interpolated_strings_on_reindented_lines() {
    // Moving a line moves the code inside `{...}`; that must not count as a change.
    check(
        "fn main() {\nlet a = 1\n    f(fn() {\n    println(\"Hello, {a} {g(a)}\")\n    })\n}\n",
        "fn main() {\n    let a = 1\n    f(fn() {\n        println(\"Hello, {a} {g(a)}\")\n    })\n}\n",
    );
}
