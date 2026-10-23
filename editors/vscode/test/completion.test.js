// Run with: node --test   (from editors/vscode)
"use strict";
const test = require("node:test");
const assert = require("node:assert");
const { complete, signatureHelp } = require("../completion");

const PROGRAM = `
interface Show {
    fn show() -> str
}

// A living creature.
class Animal {
    name: str
    legs: int = 4
    tags: [str]

    init(name: str) {
        self.name = name
    }

    // What the animal says.
    fn speak() -> str {
        return self.name
    }
}

class Dog : Animal, Show {
    owner: Person? = none

    init(name: str) {
        super(name)
    }

    fn show() -> str {
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
`;

/** Completes at the `|` marker in `body`, which is placed inside `fn main()`. */
function at(body, extra = "") {
  const src = PROGRAM + extra + "\nfn main() {\n" + body + "\n}\n";
  const offset = src.indexOf("|");
  return complete(src.replace("|", ""), offset);
}

function atRaw(src) {
  const offset = src.indexOf("|");
  return complete(src.replace("|", ""), offset);
}

const labels = (items) => items.map((i) => i.label).sort();

test("members of a class variable include inherited fields and methods", () => {
  const items = at('    let d = Dog("Rex")\n    d.|');
  const l = labels(items);
  for (const name of ["name", "legs", "tags", "owner", "speak", "show", "fetch"]) assert.ok(l.includes(name), name);
});

test("method items carry signature and doc comment", () => {
  const items = at('    let a = Animal("x")\n    a.|');
  const speak = items.find((i) => i.label === "speak");
  assert.strictEqual(speak.detail, "Animal.speak() -> str");
  assert.strictEqual(speak.documentation, "What the animal says.");
  const fetch = at('    let d = Dog("x")\n    d.|').find((i) => i.label === "fetch");
  assert.strictEqual(fetch.insertText, "fetch(${1:item}, ${2:times})");
});

test("annotated variables, parameters and chains resolve", () => {
  assert.ok(labels(at("    let p: Person = Person(\"a\")\n    p.pets[0].|")).includes("speak"));
  assert.ok(labels(at('    let p = Person("a")\n    p.name.|')).includes("upper"));
  assert.ok(labels(at("    let xs = [1, 2]\n    xs.|")).includes("push"));
  assert.ok(labels(at("    let s: Show = Dog(\"x\")\n    s.|")).includes("show"));
});

test("partial member name still completes", () => {
  assert.ok(labels(at('    let d = Dog("Rex")\n    d.sp|')).includes("speak"));
});

test("self and super inside methods", () => {
  const extra = `
class Puppy : Dog {
    fn play() {
        self.|
    }
}`;
  const src = PROGRAM + extra + "\nfn main() {}\n";
  const items = atRaw(src);
  assert.ok(labels(items).includes("fetch"));
  assert.ok(labels(items).includes("legs"));

  const sup = atRaw(src.replace("self.|", "super.|"));
  assert.ok(labels(sup).includes("speak"));
  assert.ok(!labels(sup).includes("legs"), "super. only offers methods");
});

test("for-loop and if-let variables get element types", () => {
  assert.ok(labels(at('    let p = Person("a")\n    for a in p.pets {\n        a.|\n    }')).includes("speak"));
  assert.ok(labels(at('    let d = Dog("a")\n    if let o = d.owner {\n        o.|\n    }')).includes("pets"));
  assert.ok(labels(at("    for i in 0..3 {\n        let s = str(i)\n        s.|\n    }")).includes("trim"));
});

test("general completion lists locals, functions, classes, builtins", () => {
  const items = at("    let count = 3\n    |");
  const l = labels(items);
  for (const name of ["count", "helper", "Dog", "Person", "print", "len", "if", "while"]) assert.ok(l.includes(name), name);
  assert.ok(!l.includes("main"));
  const dog = items.find((i) => i.label === "Dog");
  assert.strictEqual(dog.insertText, "Dog(${1:name})");
});

test("out-of-scope variables are not offered", () => {
  const l = labels(at("    if true {\n        let inner = 1\n    }\n    |"));
  assert.ok(!l.includes("inner"));
});

test("type positions offer types", () => {
  const l = labels(at("    let x: |"));
  for (const name of ["int", "str", "Animal", "Show"]) assert.ok(l.includes(name), name);
  assert.ok(!l.includes("print"));
});

test("class header offers base classes and interfaces", () => {
  const l = labels(atRaw(PROGRAM + "\nclass Cat : |"));
  assert.ok(l.includes("Animal") && l.includes("Show"));
  assert.ok(!l.includes("int"));
});

test("class body offers overridable methods", () => {
  const items = atRaw(PROGRAM + "\nclass Cat : Animal {\n    |\n}\n");
  const ov = items.find((i) => i.label === "override fn speak");
  assert.ok(ov, labels(items).join(","));
  assert.ok(ov.insertText.startsWith("override fn speak() -> str {"));
  assert.ok(items.find((i) => i.label === "init"));
});

test("nothing inside strings or comments", () => {
  assert.deepStrictEqual(at('    print("a.|")'), []);
  assert.deepStrictEqual(at("    // d.|"), []);
});

test("signature help for functions, constructors, methods and builtins", () => {
  const sig = (body) => {
    const src = PROGRAM + "\nfn main() {\n" + body + "\n}\n";
    const offset = src.indexOf("|");
    return signatureHelp(src.replace("|", ""), offset);
  };
  let s = sig("    helper(1, |");
  assert.strictEqual(s.label, "helper(a: int, b: str) -> int");
  assert.strictEqual(s.activeParameter, 1);

  s = sig("    let d = Dog(|");
  assert.strictEqual(s.label, "Dog(name: str)");

  s = sig('    let d = Dog("x")\n    d.fetch("ball", |');
  assert.strictEqual(s.label, "fetch(item: str, times: int) -> bool");
  assert.strictEqual(s.activeParameter, 1);

  s = sig("    print(len(|");
  assert.strictEqual(s.label, "len(x: [T], {K: V} or str) -> int");

  assert.strictEqual(sig("    let x = 1 + |"), null);
});

// ---------------------------------------------------------------- v0.2 syntax

const V2 = `
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

    fn name() -> str { return self.nick }
    abstract fn sound() -> str
    private fn hidden() -> int { return self.secret }
    static fn create(nick: str) -> Animal { return Dog(nick) }
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
`;

function at2(body, extra = "") {
  const src = V2 + extra + "\nfn main() {\n" + body + "\n}\n";
  const offset = src.indexOf("|");
  return complete(src.replace("|", ""), offset);
}

test("enum variants after `Enum.` and static methods after `Class.`", () => {
  const v = at2("    let s = Shape.|");
  assert.deepStrictEqual(labels(v), ["Circle", "Empty", "Rect"]);
  assert.strictEqual(v.find((i) => i.label === "Rect").insertText, "Rect(${1:w}, ${2:h})");
  assert.deepStrictEqual(labels(at2("    let a = Animal.|")), ["create"]);
});

test("private members are hidden outside their class", () => {
  const outside = labels(at2('    let d = Dog("x")\n    d.|'));
  assert.ok(!outside.includes("secret") && !outside.includes("hidden"));
  assert.ok(outside.includes("nick") && outside.includes("sound") && outside.includes("on_pet"));
  const src = V2.replace("fn name() -> str { return self.nick }", "fn name() -> str { return self.| }") + "\nfn main() {}\n";
  const inside = labels(complete(src.replace("|", ""), src.indexOf("|")));
  assert.ok(inside.includes("secret") && inside.includes("hidden"));
});

test("maps, generics, interface inheritance and interpolation", () => {
  assert.ok(labels(at2('    let d = Dog("x")\n    d.tags.|')).includes("keys"));
  assert.ok(labels(at2('    let m = {"a": [1]}\n    m.|')).includes("values"));
  const st = at2("    let s: Stack<str> = Stack()\n    s.peek|");
  assert.ok(labels(st).includes("peek"));
  assert.strictEqual(st.find((i) => i.label === "push").detail, "Stack.push(x: str)");
  assert.ok(labels(at2('    let p: Pet = Dog("x")\n    p.|')).includes("name"));
  assert.ok(labels(at2('    let n = "Asha"\n    print("hi {n.|}")')).includes("upper"));
});

test("match arms offer variants and bind payloads", () => {
  const arms = at2("    let s = Shape.Empty\n    match s {\n        |\n    }");
  assert.deepStrictEqual(labels(arms), ["Circle", "Empty", "Rect", "_"]);
  const bound = labels(at2("    let s = Shape.Empty\n    match s {\n        Circle(r) => print(r.|)\n    }"));
  assert.deepStrictEqual(bound, []); // float has no members, but no crash
  const scope = labels(at2("    let s = Shape.Empty\n    match s {\n        Rect(w, h) => {\n            |\n        }\n    }"));
  assert.ok(scope.includes("w") && scope.includes("h"));
});

test("lambda parameters get element types from map/filter", () => {
  assert.ok(labels(at2('    let xs = ["a"]\n    let ys = xs.map(fn(s) => s.|)')).includes("upper"));
  assert.ok(labels(at2("    let f = fn(n: int) => n\n    |")).includes("f"));
});

test("class body offers abstract implementations first, plus modifiers", () => {
  const items = complete(V2 + "\nclass Cat : Animal {\n    |\n}\n".replace("|", ""), (V2 + "\nclass Cat : Animal {\n    ").length);
  const l = labels(items);
  assert.ok(l.includes("override fn sound") && l.includes("private") && l.includes("static fn"));
  assert.ok(!l.includes("override fn hidden"), "private methods cannot be overridden");
  assert.strictEqual(items.find((i) => i.label === "override fn sound").sortText, "0sound");
});

test("imported declarations are offered", () => {
  const lib = "class Tool {\n    size: int\n}\nfn helper(n: int) -> int { return n }\n";
  const src = 'import "lib.kl"\nfn main() {\n    let t = Tool()\n    t.|\n}\n';
  const load = (p) => (p === "lib.kl" ? { text: lib, dir: "", key: "lib" } : null);
  const items = complete(src.replace("|", ""), src.indexOf("|"), { load });
  assert.deepStrictEqual(labels(items), ["size"]);
  const general = complete(src.replace("t.|", "|"), src.indexOf("t.|"), { load });
  assert.ok(labels(general).includes("helper"));
});

test("signature help for v0.2 calls", () => {
  const sig = (body) => {
    const src = V2 + "\nfn main() {\n" + body + "\n}\n";
    return signatureHelp(src.replace("|", ""), src.indexOf("|"));
  };
  assert.strictEqual(sig("    Shape.Rect(1.0, |").label, "Shape.Rect(w: float, h: float)");
  assert.strictEqual(sig("    Animal.create(|").label, "create(nick: str) -> Animal");
  assert.strictEqual(sig("    first<int>(|").label, "first(xs: [T]) -> T?");
  assert.strictEqual(sig("    apply(fn(x) => x, |").activeParameter, 1);
  assert.strictEqual(sig("    let g = fn(a: int, b: str) => a\n    g(1, |").label, "g(int, str) -> int");
});

// ---------------------------------------------------------------- v0.3 syntax

const V3 = `
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
`;

function at3(body) {
  const src = V3 + "\nfn main() {\n" + body + "\n}\n";
  return complete(src.replace("|", ""), src.indexOf("|"));
}

test("struct members, statics and constructor", () => {
  const members = labels(at3("    let v = Vec2(1.0, 2.0)\n    v.|"));
  assert.ok(members.includes("x") && members.includes("length") && members.includes("scale"));
  assert.ok(!members.includes("hidden"), "private field hidden");
  assert.deepStrictEqual(labels(at3("    let z = Vec2.|")), ["zero"]);
  const ctor = at3("    |").find((i) => i.label === "Vec2");
  assert.strictEqual(ctor.insertText, "Vec2(${1:x}, ${2:y}, ${3:hidden})");
});

test("Math, Random and Time", () => {
  const math = labels(at3("    let r = Math.|"));
  for (const n of ["PI", "sqrt", "lerp", "clamp", "atan2"]) assert.ok(math.includes(n), n);
  assert.ok(labels(at3("    Random.|")).includes("shuffle"));
  assert.ok(labels(at3("    Time.|")).includes("now"));
  const src = V3 + "\nfn main() {\n    Math.clamp(1, |\n}\n";
  assert.strictEqual(signatureHelp(src.replace("|", ""), src.indexOf("|")).label, "Math.clamp(x: T, lo: T, hi: T) -> T");
});

test("extern functions and types", () => {
  const all = labels(at3("    |"));
  assert.ok(all.includes("mean") && all.includes("matrix_new"));
  assert.ok(labels(at3("    let m: |")).includes("Matrix"));
});

test("struct body and top-level keywords", () => {
  const body = V3.replace("    static fn zero()", "    |\n    static fn zero()");
  const items = labels(complete(body.replace("|", ""), body.indexOf("|")));
  assert.ok(items.includes("mut fn") && items.includes("op +"));
  const top = labels(complete(V3 + "\n", V3.length + 1));
  for (const k of ["struct", "use rust", "use crate", "extern fn", "extern type"]) assert.ok(top.includes(k), k);
});

// ---------------------------------------------------------------- v0.4

test("threads, channels, shared values and results", () => {
  assert.deepStrictEqual(labels(at3("    Thread.|")), ["cores", "spawn"]);
  assert.ok(labels(at3("    let t = Thread.spawn(fn() => 1)\n    t.|")).includes("join"));
  assert.deepStrictEqual(labels(at3("    let c = Channel<int>()\n    c.|")), ["close", "receive", "send", "try_receive"]);
  assert.ok(labels(at3("    let s = Shared<int>(0)\n    s.|")).includes("update"));
  assert.ok(labels(at3("    let xs = [1, 2]\n    xs.|")).includes("par_map"));
  const rsrc = "fn parse(s: str) -> Result<int, str> {\n    return Ok(1)\n}\nfn main() {\n    let r = parse(\"1\")\n    r.|\n}\n";
  const r = labels(complete(rsrc.replace("|", ""), rsrc.indexOf("|")));
  assert.deepStrictEqual(r, ["err", "is_err", "is_ok", "ok", "unwrap", "unwrap_or"]);
  assert.ok(labels(at3("    let n = Shared(0)\n    n.|")).includes("get"));
  const general = labels(at3("    |"));
  for (const k of ["Ok", "Err", "Channel", "Shared", "Thread", "Async", "await"]) assert.ok(general.includes(k), k);
  assert.ok(labels(at3("    let x: |")).includes("Result"));
});

test("async functions", () => {
  const src = "async fn load(n: int) -> int {\n    return n\n}\nfn main() {\n    let job = load(1)\n    job|\n}\n";
  const items = complete(src.replace("|", ""), src.indexOf("|"));
  assert.strictEqual(items.find((i) => i.label === "job").detail, "job: Async<int>");
  assert.ok(labels(at3("    Async.|")).includes("all"));
  assert.ok(labels(at3("    Time.|")).includes("wait"));
  const top = labels(complete("\n", 1));
  assert.ok(top.includes("async fn") && top.includes("import as"));
});

test("`import ... as` namespaces", () => {
  const lib = "struct Point {\n    x: int\n    y: int\n    fn sum() -> int { return self.x + self.y }\n    static fn origin() -> Point { return Point(0, 0) }\n}\nfn describe(p: Point) -> str { return \"p\" }\n";
  const load = (p) => (p === "geo" ? { text: lib, dir: "", key: "geo" } : null);
  const run = (body) => {
    const src = 'import "geo" as g\nfn main() {\n' + body + "\n}\n";
    return complete(src.replace("|", ""), src.indexOf("|"), { load });
  };
  assert.deepStrictEqual(labels(run("    g.|")), ["Point", "describe"]);
  assert.deepStrictEqual(labels(run("    let p = g.Point(1, 2)\n    p.|")), ["sum", "x", "y"]);
  assert.deepStrictEqual(labels(run("    g.Point.|")), ["origin"]);
  assert.ok(labels(run("    |")).includes("g"));
  assert.ok(!labels(run("    |")).includes("describe"), "aliased names stay behind the alias");
  const src = 'import "geo" as g\nfn main() {\n    g.describe(|\n}\n';
  assert.strictEqual(signatureHelp(src.replace("|", ""), src.indexOf("|"), { load }).label, "g.describe(p: Point) -> str");
});

test("package and std import paths", () => {
  const path = require("path");
  const pk = require("../packages");
  const root = path.resolve("/proj");
  const files = {
    [path.join(root, "utkal.toml")]: '[package]\nname = "app"\n\n[dependencies]\ngeo = { path = "../geo" }\nmathx = { git = "https://x/mathx", tag = "v1" }\n',
    [path.join(root, "utkal.lock")]: '[[package]]\nname = "mathx"\nsource = "git https://x/mathx tag v1"\ncommit = "0123456789abcdef"\n',
    [path.resolve("/geo/utkal.toml")]: '[package]\nname = "geo"\n[dependencies]\nunits = { path = "units" }\n',
  };
  const dirs = pk.packageDirs(root, (p) => files[p] || null, path.resolve("/home/.utkal"));
  assert.strictEqual(dirs.get("geo"), path.resolve("/geo"));
  assert.strictEqual(dirs.get("units"), path.resolve("/geo/units"));
  assert.strictEqual(dirs.get("mathx"), path.join(path.resolve("/home/.utkal"), "cache", "src", "mathx-0123456789ab"));
  const ctx = { packages: dirs, std: path.resolve("/tmp/std-0.4.0") };
  assert.strictEqual(pk.resolveImport("geo", root, ctx), path.join(path.resolve("/geo"), "src", "lib.kl"));
  assert.strictEqual(pk.resolveImport("geo/shapes", root, ctx), path.join(path.resolve("/geo"), "src", "shapes.kl"));
  assert.strictEqual(pk.resolveImport("std/game", root, ctx), path.join(ctx.std, "game.kl"));
  assert.strictEqual(pk.resolveImport("util.kl", root, ctx), path.join(root, "util.kl"));
  assert.strictEqual(pk.resolveImport("nope", root, ctx), null);
  assert.strictEqual(pk.stdDir("/t", () => ["std-0.3.0", "std-0.10.1", "std-0.4.0", "cargo"]), path.join("/t", "kalinga-build", "std-0.10.1"));
});

test("class body offers override stubs for interface methods", () => {
  const src = "interface Game {\n    fn update(dt: float)\n    fn draw()\n}\nclass G : Game {\n    override fn draw() {}\n    |\n}\n";
  const items = complete(src.replace("|", ""), src.indexOf("|"));
  const stub = items.find((i) => i.label === "override fn update");
  assert.ok(stub, labels(items).join(", "));
  assert.strictEqual(stub.insertText, "override fn update(dt: float) {\n\t$0\n}");
  assert.ok(!items.some((i) => i.label === "override fn draw"), "already written");
});
