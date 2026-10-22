# Kalinga

A small "Rust++": simple syntax, classes with inheritance and interfaces, compiled to
native code through Rust. It relates to Rust the way early C++ related to C — the
Kalinga compiler translates `.kl` files to Rust source and hands them to `rustc -O`,
so programs get native speed and Rust's memory safety.

```kalinga
interface Show {
    fn show() -> str
}

abstract class Animal : Show {
    name: str
    init(name: str) { self.name = name }

    abstract fn sound() -> str
    fn show() -> str { return "{self.name} says {self.sound()}" }
}

class Dog : Animal {                 // `:` lists one base class + any interfaces
    override fn sound() -> str { return "woof" }
}

enum Pet {
    Real(a: Animal)
    Imaginary(name: str)
}

fn main() {
    let pets = [Pet.Real(Dog("Rex")), Pet.Imaginary("Nessie")]
    for p in pets {
        match p {
            Real(a) => print(a.show())
            Imaginary(n) => print("{n} is imaginary")
        }
    }
    let counts = {"dogs": 1, "cats": 0}
    print([3, 1, 2].map(fn(x) => x * 10), counts)
}
```

Output:

```text
Rex says woof
Nessie is imaginary
[30, 10, 20] {cats: 0, dogs: 1}
```

## Features

- **Simple syntax**: no semicolons, `let`/`var`, `"string {interpolation}"`, type inference.
- **Objects**: classes, single inheritance, interfaces (which can extend each other),
  `abstract`, `private`, `static`, downcasting with `is`/`as`.
- **Data**: arrays, sorted maps `{K: V}`, optionals `T?` instead of null, enums with data
  and exhaustive `match`.
- **Functions**: lambdas, function types, `map`/`filter`/`sort_by`, generics.
- **Value types**: `struct`s with operator overloading (`pos += vel * dt`), stored inline
  like Rust structs — built for game math and numeric code.
- **Standard library**: `Math`, `Random`, `Time`.
- **Modules**: `import "file.kl"`.
- **Rust interop**: call your own Rust code and any crates.io library with `use rust`,
  `use crate` and `extern fn` — the way to graphics, audio and ML libraries.
- **Fast and safe**: compiles to Rust and then native code; no null, no unchecked
  indexing, no silent overflow, no `unsafe` in generated code.

## Documentation

The full guide, with runnable examples for every feature, is in [docs/](docs/README.md):
getting started, basics, control flow, functions, arrays and strings, maps, optionals,
enums and match, classes, inheritance, interfaces, lambdas, generics, modules, safety,
tooling, internals and a language reference. See [CHANGELOG.md](CHANGELOG.md) for what is new.

## Usage

Requires a Rust toolchain (`rustc` on PATH).

```bash
cargo run -- run examples/oop.kl
```

```bash
cargo run -- build examples/fib.kl -o fib.exe
```

```bash
cargo run -- emit examples/oop.kl
```

`emit` prints the generated Rust so you can see exactly what your program becomes.
`check <file> [--stdin]` only reports errors, which editors use for live diagnostics.

## Editor support

A VS Code extension with syntax highlighting, snippets, live errors and a Run button is in
[editors/vscode](editors/vscode/README.md).

## Language

| Feature | Syntax |
|---|---|
| Variables | `let x = 1` (immutable), `var y = 2` (mutable), `let z: float = 1.0` |
| Types | `int` (64-bit), `float`, `bool`, `str`, `[T]` arrays, `{K: V}` maps, `T?` optionals, `fn(A) -> B`, classes, interfaces, enums |
| Functions | `fn add(a: int, b: int) -> int { return a + b }`, generic `fn first<T>(xs: [T]) -> T?` |
| Lambdas | `fn(x: int) -> int { return x * 2 }`, `fn(x) => x * 2`; named functions are values |
| Strings | `"Hi {name}"` interpolation, `{{` `}}` for literal braces |
| Control flow | `if / else if / else`, `while cond { }`, `for i in 0..10 { }`, `for x in array { }`, `for k in map { }`, `for ch in "text" { }`, `match`, `break`, `continue` |
| Enums | `enum Shape { Circle(r: float), Empty }`, `Shape.Circle(1.0)`, `match s { Circle(r) => ... }` |
| Optionals | `var o: int? = none`, `if let v = o { } else { }`, `o == none` |
| Classes | fields with optional defaults, `init(...)`, methods, `self`, `private`, `static fn`, generic `class Stack<T>` |
| Inheritance | `class B : A` — single base class; `override fn`; `super(...)`, `super.method()`; `abstract class` / `abstract fn`; `x is B`, `x as B` |
| Interfaces | `interface I : J { fn m() -> str }`, `class C : Base, I, J`; interfaces work as types |
| Modules | `import "lib/shapes.kl"` |
| Structs | `struct Vec2 { x: float, y: float }`, `mut fn`, `op +(o: Vec2) -> Vec2` |
| Library | `Math.sqrt(x)`, `Math.lerp(a, b, t)`, `Random.int(1, 7)`, `Time.now()` |
| Rust interop | `use rust "native/x.rs"`, `use crate "rand" = "0.8"`, `extern fn f(x: int) -> int`, `extern type T` |
| Built-ins | `print(a, b, ...)`, `len(x)`, `str(x)`, `int(x)`, `float(x)`, `input(prompt)` |
| Array methods | `push(v)`, `pop()` → `T?`, `len()`, `contains(v)`, `map(f)`, `filter(f)`, `find(f)`, `sort()`, `sort_by(f)` |
| Map methods | `m[k]`, `get(k)` → `V?`, `has(k)`, `remove(k)`, `keys()`, `values()`, `len()` |
| String methods | `len()`, `upper()`, `lower()`, `trim()`, `contains(s)` |

Statements end at a newline (`;` is optional). Comments use `//` and `/* */`.

## Safety

- No null: absence is an explicit `T?` that must be unwrapped with `if let`.
- Immutable by default; assigning to a `let` variable is a compile error.
- No implicit conversions: `"n=" + 5` is an error, write `"n=" + str(5)`.
- Array indexing is bounds-checked; integer overflow and division by zero stop the program with a `runtime error:` message instead of corrupting memory.
- Overrides and interface implementations are checked against the original signature.
- The generated Rust contains no `unsafe`.

## How it compiles

```
.kl → lexer → parser → checker (classes, inheritance, interfaces) → codegen (type check + Rust) → rustc -O → native exe
```

- Objects are `Rc<RefCell<Struct>>`: reference semantics with checked shared mutation.
- Each class becomes a Rust `struct` holding all of its fields, inherited ones included.
- Each class also gets a trait (`__I_Name`) whose supertraits are the base class trait and its interfaces. A class that has subclasses is used through `Rc<RefCell<dyn __I_Name>>` (dynamic dispatch); a class with no subclasses uses the concrete struct (static dispatch, no vtable).
- Arrays and strings are values (copied on assignment), like Rust's `Vec` and `String` after `.clone()`.

Limitation: calling a method on an object that is already running one of its own methods
through a different reference (for example `a.f(a)` where `f` modifies `a`) stops with a
`runtime error` instead of misbehaving silently.

## Layout

| Path | Purpose |
|---|---|
| `src/lexer.rs`, `src/token.rs` | tokens, newline handling |
| `src/parser.rs`, `src/ast.rs` | syntax tree |
| `src/checker.rs` | class/interface model, inheritance and override rules |
| `src/codegen.rs` | type checking of bodies, Rust generation, runtime prelude |
| `src/driver.rs` | invokes `rustc` and runs the program |
| `examples/` | `hello.kl`, `oop.kl`, `fib.kl`, `inventory.kl`, `pipeline.kl`, `particles.kl`, `shapes/` (modules), `interop/` (Rust) |
| `tests/examples.rs` | end-to-end tests |
| `editors/vscode/` | VS Code extension |
| `docs/` | language guide and reference |
| `tests/docs.rs` | compiles every example in `docs/` |
