//! Hover, on the programs the old JavaScript engine was tested with.

mod common;

use common::*;

const HOVER: &str = r#"
/// A point on the **map**.
struct Point {
    /// Distance from the left edge.
    x: int
    y: int

    /// Sum of both coordinates.
    fn sum() -> int { return self.x + self.y }
}

// Just a note for readers: not documentation.
fn helper() -> int { return 1 }

/// Doubles `n`.
///
///     double(2) == 4
fn double(n: int) -> int { return n * 2 }

interface Shape {
    /// Area in square units.
    fn area() -> float
}

enum Dir {
    Left
    Right(steps: int)
}

fn main() {
    let p = Point(1, 2)
    var total = double(p.x) + p.sum()
    println(Math.sqrt(2.0), len("a"), Dir.Right(1), helper())
}
"#;

fn text(word: &str, nth: usize) -> String {
    hover(HOVER, word, nth).map(|h| h.0).unwrap_or_default()
}

#[test]
fn functions_with_docs_as_markdown_and_notes_ignored() {
    let (h, len) = hover(HOVER, "double(p.x)", 0).expect("hover");
    assert!(h.contains("fn double(n: int) -> int"), "{}", h);
    assert!(h.contains("Doubles `n`.\n\n    double(2) == 4"), "{}", h);
    assert_eq!(len, "double".len() as u32);
    let note = text("helper()", 1);
    assert!(note.contains("fn helper() -> int"), "{}", note);
    assert!(!note.contains("Just a note"), "plain // is not documentation");
}

#[test]
fn structs_fields_methods_and_variables() {
    let point = text("Point(1, 2)", 0);
    assert!(point.contains("struct Point(x: int, y: int)") && point.contains("A point on the **map**."), "{}", point);
    assert!(text("x)", 0).contains("x: int"), "field after a dot");
    assert!(text("p.x", 0).contains("let p: Point"));
    let sum = text("sum()", 1);
    assert!(
        sum.contains("fn sum() -> int") && sum.contains("Sum of both coordinates.") && sum.contains("in `Point`"),
        "{}",
        sum
    );
    assert!(text("total", 0).contains("var total: int"));
    assert!(text("x: int", 0).contains("Distance from the left edge."), "field at its declaration");
    assert!(text("sum() -> int", 0).contains("Sum of both"), "method at its declaration");
}

#[test]
fn interfaces_enums_library_and_builtins() {
    assert!(text("Shape", 0).contains("fn area() -> float"));
    assert!(text("area", 0).contains("Area in square units."));
    assert!(text("Right(1)", 0).contains("Dir.Right(steps: int)"));
    assert!(text("sqrt", 0).contains("fn Math.sqrt(x: float) -> float"));
    assert!(text("Math", 0).contains("Built-in library"));
    assert!(text("len(", 0).contains("Number of elements"));
    assert_eq!(hover(HOVER, "Just a note", 0), None, "nothing inside comments");
    assert_eq!(hover(HOVER, "2.0", 0), None, "nothing on numbers");
}

#[test]
fn package_aliases_and_their_members() {
    let dir = temp_dir("hover-alias");
    std::fs::create_dir_all(dir.join("mathx/src")).expect("dir");
    std::fs::write(
        dir.join("utkal.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmathx = { path = \"mathx\" }\n",
    )
    .expect("write");
    std::fs::write(dir.join("mathx/utkal.toml"), "[package]\nname = \"mathx\"\nversion = \"0.1.0\"\n").expect("write");
    let lib = "/// A 2D vector.\nstruct Vec2 {\n    x: float\n    y: float\n}\n/// Dot product.\nfn dot(a: Vec2, b: Vec2) -> float { return a.x * b.x }\n";
    std::fs::write(dir.join("mathx/src/lib.kl"), lib).expect("write");
    let src = "import \"mathx\" as mx\nfn main() {\n    let v = mx.Vec2(1.0, 2.0)\n    println(mx.dot(v, v), v.x)\n}\n";
    let at = |word: &str| hover_in(&dir, src, word, 0).map(|h| h.0).unwrap_or_default();
    assert!(at("mx.Vec2").contains("import \"mathx\" as mx"), "{}", at("mx.Vec2"));
    assert!(at("dot").contains("Dot product."), "{}", at("dot"));
    assert!(at("Vec2(1.0").contains("A 2D vector."), "{}", at("Vec2(1.0"));
    assert!(at("v.x").contains("let v: mx.Vec2"), "{}", at("v.x"));
}

#[test]
fn bounds_are_shown() {
    let src = "interface Shape {\n    fn area() -> float\n}\nfn total<T: Shape + Show>(xs: [T]) -> int { return len(xs) }\nclass Crate<T: Ord> {}\nfn main() {\n    let n = 1\n}\n";
    let shown = |word: &str| hover(src, word, 0).map(|h| h.0).unwrap_or_default();
    assert!(shown("total").contains("fn total<T: Shape + Show>(xs: [T]) -> int"), "{}", shown("total"));
    assert!(shown("Crate").contains("class Crate<T: Ord>"), "{}", shown("Crate"));
}
