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
        "fn main() {\n    x = - 1\n    y = x-1\n    println( f( x ) [0], !true )\n}\n",
        "fn main() {\n    x = -1\n    y = x - 1\n    println(f(x)[0], !true)\n}\n",
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
        "fn f(r: Result< int , str >) -> Stack<str> {\n    if a<b && c > d {\n        c = Channel <int>()\n    }\n    return first<int>(xs)\n}\n",
        "fn f(r: Result<int, str>) -> Stack<str> {\n    if a < b && c > d {\n        c = Channel<int>()\n    }\n    return first<int>(xs)\n}\n",
    );
}

#[test]
fn maps_blocks_ranges_and_lambdas() {
    check(
        "fn main() {\n    m: { str : int } = { \"a\" : 1 }\n    for i in 0 .. 3 { println(i) }\n    f = fn (x) => x\n    e = {}\n}\n",
        "fn main() {\n    m: {str: int} = {\"a\": 1}\n    for i in 0..3 { println(i) }\n    f = fn(x) => x\n    e = {}\n}\n",
    );
}

#[test]
fn comments_and_blank_lines_are_kept() {
    check(
        "// header\n\n\n/// Doc.\nfn main() {   // trailing\n\n    x = 1 /* inline */ + 2\n\n\n    // alone\n    println(x)\n\n}\nfn other() {}\n",
        "// header\n\n/// Doc.\nfn main() { // trailing\n    x = 1 /* inline */ + 2\n\n    // alone\n    println(x)\n}\n\nfn other() {}\n",
    );
}

#[test]
fn indentation_follows_brackets() {
    check(
        "class A {\nfn f() {\nxs = [1,\n2]\nys = xs.map(fn(x) {\nreturn x\n})\ntotal = 1 +\n2\nzs = xs\n.filter(fn(x) => x > 1)\n}\n}\n",
        "class A {\n    fn f() {\n        xs = [1,\n            2]\n        ys = xs.map(fn(x) {\n            return x\n        })\n        total = 1 +\n            2\n        zs = xs\n            .filter(fn(x) => x > 1)\n    }\n}\n",
    );
}

#[test]
fn one_statement_per_line() {
    check(
        "fn main() { a = 1; b = 2 }\nfn g() {\n    f(); g();\n}\n",
        "fn main() { a = 1; b = 2 }\nfn g() {\n    f()\n    g()\n}\n",
    );
}

#[test]
fn trailing_comments_line_up() {
    check(
        "fn main() {\n    a = 10   // int\n    bb: float = 2.5 // float\n    c = 1\n    f()     // call\n}\n",
        "fn main() {\n    a = 10          // int\n    bb: float = 2.5 // float\n    c = 1\n    f() // call\n}\n",
    );
}

#[test]
fn one_line_declarations_stay_together() {
    check(
        "extern fn a() -> int = rust { 1 }\nextern fn b() -> int = rust { 2 }\nX: int = 1\nfn f() {\n    a()\n}\nfn g() {}\n",
        "extern fn a() -> int = rust { 1 }\nextern fn b() -> int = rust { 2 }\nX: int = 1\nfn f() {\n    a()\n}\n\nfn g() {}\n",
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
        "fn main() {\na = 1\n    f(fn() {\n    println(\"Hello, {a} {g(a)}\")\n    })\n}\n",
        "fn main() {\n    a = 1\n    f(fn() {\n        println(\"Hello, {a} {g(a)}\")\n    })\n}\n",
    );
}

#[test]
fn struct_bases_and_private_set() {
    check(
        "struct Version: Ord,Show {\n    private (set) var major: int\n}\n",
        "struct Version : Ord, Show {\n    private(set) var major: int\n}\n",
    );
}

#[test]
fn attributes() {
    check(
        "#[ deprecated( \"use g\" ) ]\nfn f() {}\nclass A {\n#[deprecated]\nx: int = 1\n}\n",
        "#[deprecated(\"use g\")]\nfn f() {}\nclass A {\n    #[deprecated]\n    x: int = 1\n}\n",
    );
}
