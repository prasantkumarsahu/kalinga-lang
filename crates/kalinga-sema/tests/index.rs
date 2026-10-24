//! What the type checker records for editors: the symbol behind each name, and what is
//! in scope at the completion probe.

use kalinga_sema::index::{Index, PROBE, Probe, Symbol};
use kalinga_sema::{Model, Type, typeck};
use kalinga_syntax::{lexer, parser};

const PROGRAM: &str = r#"
class Animal {
    name: str
    init(name: str) {
        self.name = name
    }
    fn speak() -> str {
        return self.name
    }
}

class Dog : Animal {
    init(name: str) {
        super(name)
    }
    static fn make() -> Dog {
        return Dog("rex")
    }
}

enum Shape {
    Circle(r: float)
    Dot
}

struct P {
    x: int
    const ORIGIN: int = 0
}

fn twice(n: int) -> int {
    return n * 2
}

fn main() {
    let dog = Dog.make()
    var total = twice(P.ORIGIN)
    total += dog.speak().len()
    let s = Shape.Circle(1.0)
    match s {
        Circle(r) => println(r)
        Dot => println(dog.name)
    }
    println(Math.sqrt(2.0), total)
}
"#;

fn index(source: &str) -> Index {
    let tokens = lexer::lex(source, 0).expect("lexes");
    let prog = parser::parse(tokens).expect("parses");
    let model = Model::build(&prog, false).expect("builds");
    typeck::analyze(&prog, &model).1
}

/// The symbols recorded for `name`, in source order.
fn symbols(ix: &Index, name: &str) -> Vec<Symbol> {
    let mut refs: Vec<_> = ix.refs.iter().filter(|r| r.name == name).collect();
    refs.sort_by_key(|r| (r.span.line, r.span.col));
    refs.into_iter().map(|r| r.symbol.clone()).collect()
}

fn member(owner: &str, name: &str) -> Symbol {
    Symbol::Member { owner: owner.into(), name: name.into() }
}

#[test]
fn members_resolve_to_the_type_that_declares_them() {
    let ix = index(PROGRAM);
    assert_eq!(symbols(&ix, "speak"), vec![member("Animal", "speak")]);
    assert_eq!(symbols(&ix, "make"), vec![member("Dog", "make")]);
    assert_eq!(symbols(&ix, "ORIGIN"), vec![member("P", "ORIGIN")]);
    assert_eq!(symbols(&ix, "Circle"), vec![member("Shape", "Circle"), member("Shape", "Circle")]);
    // `self.name` in Animal, the assignment in `init`, and `dog.name` through a Dog.
    assert_eq!(symbols(&ix, "name").iter().filter(|s| **s == member("Animal", "name")).count(), 3);
}

#[test]
fn functions_types_libraries_and_builtins() {
    let ix = index(PROGRAM);
    assert_eq!(symbols(&ix, "twice"), vec![Symbol::Function("twice".into())]);
    assert!(symbols(&ix, "Dog").contains(&Symbol::Type("Dog".into())));
    assert_eq!(symbols(&ix, "sqrt"), vec![Symbol::Library { lib: "Math".into(), name: "sqrt".into() }]);
    assert_eq!(symbols(&ix, "len"), vec![Symbol::Builtin { receiver: Some("str"), name: "len".into() }]);
}

#[test]
fn locals_link_uses_to_their_declaration() {
    let ix = index(PROGRAM);
    let total: Vec<_> = ix.refs.iter().filter(|r| r.name == "total").collect();
    // `var total`, `total +=` and `println(..., total)`
    assert_eq!(total.len(), 3);
    assert_eq!(total.iter().filter(|r| r.is_decl).count(), 1);
    let decl = total.iter().find(|r| r.is_decl).map(|r| r.symbol.clone());
    assert!(total.iter().all(|r| Some(r.symbol.clone()) == decl));
    assert!(total.iter().all(|r| r.ty == Type::Int));
    // The match binding `r` is a local of type float.
    let r: Vec<_> = ix.refs.iter().filter(|x| x.name == "r").collect();
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| x.ty == Type::Float));
}

#[test]
fn probe_reports_the_scope() {
    let source = PROGRAM.replace("println(Math.sqrt(2.0), total)", &format!("println({})", PROBE));
    match index(&source).probe {
        Some(Probe::Scope { locals, owner, .. }) => {
            let names: Vec<&str> = locals.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, vec!["dog", "s", "total"]);
            assert_eq!(owner, None);
        }
        other => panic!("unexpected probe {:?}", other),
    }
}

#[test]
fn probe_reports_the_receiver() {
    let at = |code: &str| {
        let source = PROGRAM.replace("println(Math.sqrt(2.0), total)", code);
        index(&source).probe
    };
    let probe = at(&format!("dog.{}", PROBE));
    assert!(matches!(probe, Some(Probe::Members { ty: Type::Class(ref c), on_self: false }) if c == "Dog"));
    assert!(matches!(at(&format!("Shape.{}", PROBE)), Some(Probe::TypeMembers(ref t)) if t == "Shape"));
    assert!(matches!(at(&format!("Math.{}()", PROBE)), Some(Probe::Library(ref l)) if l == "Math"));
    let probe = at(&format!("dog.speak().{}()", PROBE));
    assert!(matches!(probe, Some(Probe::Members { ty: Type::Str, .. })));
}
