//! Completion, on the programs the old JavaScript engine was tested with.

mod common;

use common::*;
use lsp_types::CompletionItemKind;

const PROGRAM: &str = r#"
interface Show {
    fn show() -> str
}

/// A living creature.
class Animal {
    name: str
    legs: int = 4
    tags: [str]

    init(name: str) {
        self.name = name
    }

    /// What the animal says.
    fn speak() -> str {
        return self.name
    }
}

class Dog : Animal, Show {
    owner: Person? = none

    init(name: str) {
        super(name)
    }

    override fn show() -> str {
        return "Dog"
    }

    fn fetch(item: str, times: int) -> bool {
        return true
    }
}

class Person {
    name: str
    pets: [Animal]

    init(name: str) {
        self.name = name
    }
}

fn helper(a: int, b: str) -> int {
    return a
}
"#;

/// Completes at `|` in `body`, placed inside `fn main()` after PROGRAM and `extra`.
fn at(body: &str, extra: &str) -> Vec<lsp_types::CompletionItem> {
    complete(&format!("{}{}\nfn main() {{\n{}\n}}\n", PROGRAM, extra, body))
}

fn has(items: &[lsp_types::CompletionItem], name: &str) -> bool {
    items.iter().any(|i| i.label == name)
}

#[test]
fn members_of_a_class_variable_include_inherited_fields_and_methods() {
    let items = at("    let d = Dog(\"Rex\")\n    d.|", "");
    for name in ["name", "legs", "tags", "owner", "speak", "show", "fetch"] {
        assert!(has(&items, name), "{} in {:?}", name, labels(&items));
    }
}

#[test]
fn method_items_carry_signature_and_doc_comment() {
    let items = at("    let a = Animal(\"x\")\n    a.|", "");
    let speak = find(&items, "speak");
    assert_eq!(speak.detail.as_deref(), Some("Animal.speak() -> str"));
    assert_eq!(docs(speak).as_deref(), Some("What the animal says."));
    let fetch = at("    let d = Dog(\"x\")\n    d.|", "");
    assert_eq!(find(&fetch, "fetch").insert_text.as_deref(), Some("fetch(${1:item}, ${2:times})"));
}

#[test]
fn annotated_variables_parameters_and_chains_resolve() {
    assert!(has(&at("    let p: Person = Person(\"a\")\n    p.pets[0].|", ""), "speak"));
    assert!(has(&at("    let p = Person(\"a\")\n    p.name.|", ""), "upper"));
    assert!(has(&at("    var xs = [1, 2]\n    xs.|", ""), "push"));
    assert!(has(&at("    let s: Show = Dog(\"x\")\n    s.|", ""), "show"));
}

#[test]
fn partial_member_name_still_completes() {
    assert!(has(&at("    let d = Dog(\"Rex\")\n    d.sp|", ""), "speak"));
}

#[test]
fn self_and_super_inside_methods() {
    let extra =
        "\nclass Puppy : Dog {\n    init() {\n        super(\"p\")\n    }\n    fn play() {\n        self.|\n    }\n}";
    let src = format!("{}{}\nfn main() {{}}\n", PROGRAM, extra);
    let items = complete(&src);
    assert!(has(&items, "fetch") && has(&items, "legs"), "{:?}", labels(&items));
    let sup = complete(&src.replace("self.|", "super.|"));
    assert!(has(&sup, "speak"), "{:?}", labels(&sup));
    assert!(!has(&sup, "legs"), "super. only offers methods");
}

#[test]
fn for_loop_and_if_let_variables_get_element_types() {
    assert!(has(&at("    let p = Person(\"a\")\n    for a in p.pets {\n        a.|\n    }", ""), "speak"));
    assert!(has(&at("    let d = Dog(\"a\")\n    if let o = d.owner {\n        o.|\n    }", ""), "pets"));
    assert!(has(&at("    for i in 0..3 {\n        let s = str(i)\n        s.|\n    }", ""), "trim"));
}

#[test]
fn general_completion_lists_locals_functions_classes_builtins() {
    let items = at("    let count = 3\n    |", "");
    for name in ["count", "helper", "Dog", "Person", "print", "len", "if", "while"] {
        assert!(has(&items, name), "{} in {:?}", name, labels(&items));
    }
    assert!(!has(&items, "main"));
    assert_eq!(find(&items, "Dog").insert_text.as_deref(), Some("Dog(${1:name})"));
}

#[test]
fn out_of_scope_variables_are_not_offered() {
    assert!(!has(&at("    if true {\n        let inner = 1\n    }\n    |", ""), "inner"));
}

#[test]
fn type_positions_offer_types() {
    let items = at("    let x: |", "");
    for name in ["int", "str", "Animal", "Show"] {
        assert!(has(&items, name), "{}", name);
    }
    assert!(!has(&items, "print"));
}

#[test]
fn class_header_offers_base_classes_and_interfaces() {
    let items = complete(&format!("{}\nclass Cat : |", PROGRAM));
    assert!(has(&items, "Animal") && has(&items, "Show"), "{:?}", labels(&items));
    assert!(!has(&items, "int"));
}

#[test]
fn class_body_offers_overridable_methods() {
    let items = complete(&format!("{}\nclass Cat : Animal {{\n    |\n}}\n", PROGRAM));
    let ov = find(&items, "override fn speak");
    assert!(ov.insert_text.as_deref().is_some_and(|t| t.starts_with("override fn speak() -> str {")));
    assert!(has(&items, "init"));
}

#[test]
fn nothing_inside_strings_or_comments() {
    assert!(at("    print(\"a.|\")", "").is_empty());
    assert!(at("    // d.|", "").is_empty());
}

const V2: &str = r#"
enum Shape {
    Circle(r: float)
    Rect(w: float, h: float)
    Empty
}

interface Named {
    fn name() -> str
}
interface Pet : Named {
    fn sound() -> str
}

abstract class Animal : Pet {
    private secret: int = 1
    nick: str
    tags: {str: int}
    on_pet: fn(str) -> str = fn(who) => who

    override fn name() -> str { return self.nick }
    abstract fn sound() -> str
    private fn hidden() -> int { return self.secret }
    static fn create(nick: str) -> Animal { return Dog() }
}

class Dog : Animal {
    override fn sound() -> str { return "woof" }
}

class Stack<T> {
    items: [T]
    fn push(x: T) { self.items.push(x) }
    fn peek() -> T? { return none }
}

fn apply(f: fn(int) -> int, x: int) -> int { return f(x) }
fn first<T>(xs: [T]) -> T? { return none }
"#;

fn at2(body: &str) -> Vec<lsp_types::CompletionItem> {
    complete(&format!("{}\nfn main() {{\n{}\n}}\n", V2, body))
}

#[test]
fn enum_variants_after_enum_dot_and_static_methods_after_class_dot() {
    let v = at2("    let s = Shape.|");
    assert_eq!(labels(&v), ["Circle", "Empty", "Rect"]);
    assert_eq!(find(&v, "Rect").insert_text.as_deref(), Some("Rect(${1:w}, ${2:h})"));
    assert_eq!(labels(&at2("    let a = Animal.|")), ["create"]);
}

#[test]
fn private_members_are_hidden_outside_their_class() {
    let outside = at2("    let d = Dog()\n    d.|");
    assert!(!has(&outside, "secret") && !has(&outside, "hidden"));
    assert!(has(&outside, "nick") && has(&outside, "sound") && has(&outside, "on_pet"));
    let src =
        V2.replace("override fn name() -> str { return self.nick }", "override fn name() -> str { return self.| }");
    let inside = complete(&format!("{}\nfn main() {{}}\n", src));
    assert!(has(&inside, "secret") && has(&inside, "hidden"), "{:?}", labels(&inside));
}

#[test]
fn maps_generics_interface_inheritance_and_interpolation() {
    assert!(has(&at2("    let d = Dog()\n    d.tags.|"), "keys"));
    assert!(has(&at2("    let m = {\"a\": [1]}\n    m.|"), "values"));
    let st = at2("    let s: Stack<str> = Stack()\n    s.peek|");
    assert!(has(&st, "peek"));
    assert_eq!(find(&st, "push").detail.as_deref(), Some("Stack.push(x: str)"));
    assert!(has(&at2("    let p: Pet = Dog()\n    p.|"), "name"));
    assert!(has(&at2("    let n = \"Asha\"\n    print(\"hi {n.|}\")"), "upper"));
}

#[test]
fn match_arms_offer_variants_and_bind_payloads() {
    let arms = at2("    let s = Shape.Empty\n    match s {\n        |\n    }");
    assert_eq!(labels(&arms), ["Circle", "Empty", "Rect", "_"]);
    let bound =
        at2("    let s = Shape.Empty\n    match s {\n        Circle(r) => print(r.|)\n        _ => print(1)\n    }");
    assert!(bound.is_empty(), "float has no members: {:?}", labels(&bound));
    let scope = at2(
        "    let s = Shape.Empty\n    match s {\n        Rect(w, h) => {\n            |\n        }\n        _ => print(1)\n    }",
    );
    assert!(has(&scope, "w") && has(&scope, "h"), "{:?}", labels(&scope));
}

#[test]
fn lambda_parameters_get_element_types_from_map_and_filter() {
    assert!(has(&at2("    let xs = [\"a\"]\n    let ys = xs.map(fn(s) => s.|)"), "upper"));
    assert!(has(&at2("    let f = fn(n: int) => n\n    |"), "f"));
}

#[test]
fn class_body_offers_abstract_implementations_first_plus_modifiers() {
    let items = complete(&format!("{}\nclass Cat : Animal {{\n    |\n}}\n", V2));
    assert!(has(&items, "override fn sound") && has(&items, "private") && has(&items, "static fn"));
    assert!(!has(&items, "override fn hidden"), "private methods cannot be overridden");
    assert_eq!(find(&items, "override fn sound").sort_text.as_deref(), Some("0sound"));
}

#[test]
fn imported_declarations_are_offered() {
    let dir = temp_dir("imported");
    std::fs::write(dir.join("lib.kl"), "class Tool {\n    size: int\n}\nfn helper(n: int) -> int { return n }\n")
        .expect("write");
    let src = "import \"lib.kl\"\nfn main() {\n    let t = Tool()\n    t.|\n}\n";
    assert_eq!(labels(&complete_in(&dir, src)), ["size"]);
    let general = complete_in(&dir, &src.replace("t.|", "|"));
    assert!(has(&general, "helper"));
}

const V3: &str = r#"
use rust "native/stats.rs"

extern type Matrix
extern fn matrix_new(rows: int, cols: int) -> Matrix
extern fn mean(xs: [float]) -> float

struct Vec2 {
    x: float
    y: float
    private hidden: int = 0

    static fn zero() -> Vec2 { return Vec2(0.0, 0.0) }
    fn length() -> float { return Math.hypot(self.x, self.y) }
    mut fn scale(k: float) { self.x *= k }
    op +(o: Vec2) -> Vec2 { return Vec2(self.x + o.x, self.y + o.y) }
}
"#;

/// A folder with the Rust file V3 uses; each call gets its own, as tests run in parallel.
fn v3_dir() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = temp_dir(&format!("v3-{}", n));
    std::fs::create_dir_all(dir.join("native")).expect("dir");
    std::fs::write(dir.join("native/stats.rs"), "pub struct Matrix;\n").expect("write");
    dir
}

fn at3(body: &str) -> Vec<lsp_types::CompletionItem> {
    complete_in(&v3_dir(), &format!("{}\nfn main() {{\n{}\n}}\n", V3, body))
}

#[test]
fn struct_members_statics_and_constructor() {
    let members = at3("    let v = Vec2(1.0, 2.0)\n    v.|");
    assert!(has(&members, "x") && has(&members, "length") && has(&members, "scale"));
    assert!(!has(&members, "hidden"), "private field hidden");
    assert_eq!(labels(&at3("    let z = Vec2.|")), ["zero"]);
    let all = at3("    |");
    assert_eq!(find(&all, "Vec2").insert_text.as_deref(), Some("Vec2(${1:x}, ${2:y}, ${3:hidden})"));
}

#[test]
fn math_random_and_time() {
    let math = at3("    let r = Math.|");
    for n in ["PI", "sqrt", "lerp", "clamp", "atan2"] {
        assert!(has(&math, n), "{}", n);
    }
    assert!(has(&at3("    Random.|"), "shuffle"));
    assert!(has(&at3("    Time.|"), "now"));
}

#[test]
fn extern_functions_and_types() {
    let all = at3("    |");
    assert!(has(&all, "mean") && has(&all, "matrix_new"));
    assert!(has(&at3("    let m: |"), "Matrix"));
}

#[test]
fn struct_body_and_top_level_keywords() {
    let body = V3.replace("    static fn zero()", "    |\n    static fn zero()");
    let items = complete_in(&v3_dir(), &body);
    assert!(has(&items, "mut fn") && has(&items, "op +"), "{:?}", labels(&items));
    let top = complete_in(&v3_dir(), &format!("{}\n|", V3));
    for k in ["struct", "use rust", "use crate", "extern fn", "extern type", "const"] {
        assert!(has(&top, k), "{}", k);
    }
}

#[test]
fn threads_channels_shared_values_and_results() {
    assert_eq!(labels(&at3("    Thread.|")), ["cores", "spawn"]);
    assert!(has(&at3("    let t = Thread.spawn(fn() => 1)\n    t.|"), "join"));
    assert_eq!(labels(&at3("    let c = Channel<int>()\n    c.|")), ["close", "receive", "send", "try_receive"]);
    assert!(has(&at3("    let s = Shared<int>(0)\n    s.|"), "update"));
    assert!(has(&at3("    let xs = [1, 2]\n    xs.|"), "par_map"));
    let r = complete(
        "fn parse(s: str) -> Result<int, str> {\n    return Ok(1)\n}\nfn main() {\n    let r = parse(\"1\")\n    r.|\n}\n",
    );
    assert_eq!(labels(&r), ["err", "is_err", "is_ok", "ok", "unwrap", "unwrap_or"]);
    let general = at3("    |");
    for k in ["Ok", "Err", "Channel", "Shared", "Thread", "Async", "await"] {
        assert!(has(&general, k), "{}", k);
    }
    assert!(has(&at3("    let x: |"), "Result"));
}

#[test]
fn async_functions() {
    let items =
        complete("async fn load(n: int) -> int {\n    return n\n}\nfn main() {\n    let job = load(1)\n    job|\n}\n");
    assert_eq!(find(&items, "job").detail.as_deref(), Some("job: Async<int>"));
    assert!(has(&at3("    Async.|"), "all"));
    assert!(has(&at3("    Time.|"), "wait"));
    let top = complete("\n|");
    assert!(has(&top, "async fn") && has(&top, "import as"));
}

#[test]
fn import_as_namespaces() {
    let dir = temp_dir("alias");
    std::fs::create_dir_all(dir.join("geo/src")).expect("dir");
    std::fs::write(
        dir.join("utkal.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ngeo = { path = \"geo\" }\n",
    )
    .expect("write");
    std::fs::write(dir.join("geo/utkal.toml"), "[package]\nname = \"geo\"\nversion = \"0.1.0\"\n").expect("write");
    let lib = "struct Point {\n    x: int\n    y: int\n    fn sum() -> int { return self.x + self.y }\n    static fn origin() -> Point { return Point(0, 0) }\n}\nfn describe(p: Point) -> str { return \"p\" }\n";
    std::fs::write(dir.join("geo/src/lib.kl"), lib).expect("write");
    let run = |body: &str| complete_in(&dir, &format!("import \"geo\" as g\nfn main() {{\n{}\n}}\n", body));
    assert_eq!(labels(&run("    g.|")), ["Point", "describe"]);
    assert_eq!(labels(&run("    let p = g.Point(1, 2)\n    p.|")), ["sum", "x", "y"]);
    assert_eq!(labels(&run("    g.Point.|")), ["origin"]);
    let general = run("    |");
    assert!(has(&general, "g"));
    assert!(!has(&general, "describe"), "aliased names stay behind the alias");
    let sig = signature_in(&dir, "import \"geo\" as g\nfn main() {\n    g.describe(|\n}\n");
    // Types are shown as this file writes them.
    assert_eq!(sig.map(|s| s.0).as_deref(), Some("g.describe(p: g.Point) -> str"));
}

#[test]
fn package_and_std_import_paths() {
    let items = complete("import \"|");
    for m in ["std/game", "std/math2d", "std/fs"] {
        assert!(has(&items, m), "{}", m);
    }
}

#[test]
fn class_body_offers_override_stubs_for_interface_methods() {
    let items = complete(
        "interface Game {\n    fn update(dt: float)\n    fn draw()\n}\nclass G : Game {\n    override fn draw() {}\n    |\n}\n",
    );
    let stub = find(&items, "override fn update");
    assert_eq!(stub.insert_text.as_deref(), Some("override fn update(dt: float) {\n\t$0\n}"));
    assert!(!has(&items, "override fn draw"), "already written");
    assert_eq!(stub.kind, Some(CompletionItemKind::METHOD));
}
