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
  assert.strictEqual(s.label, "len(x: [T] or str) -> int");

  assert.strictEqual(sig("    let x = 1 + |"), null);
});
