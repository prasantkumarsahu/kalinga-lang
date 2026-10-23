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
    override fn show() -> str { return "{self.name} says {self.sound()}" }
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
            Real(a) => println(a.show())
            Imaginary(n) => println("{n} is imaginary")
        }
    }
    let counts = {"dogs": 1, "cats": 0}
    println([3, 1, 2].map(fn(x) => x * 10), counts)
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
- **Errors as values**: `Result<T, E>`, `Ok` / `Err`, and `?` to pass errors on. No
  exceptions.
- **Threads**: `Thread.spawn`, channels, shared values and `par_map` over every core.
  Only values cross threads, so data races cannot happen and single-threaded code stays
  fast.
- **Async**: `async fn` / `await` with a built-in runtime: timers, background jobs,
  `Async.all`.
- **Games**: `import "std/game"` for a window, 2D drawing, input, images and sounds (on
  macroquad), with Pong and Asteroids as examples.
- **Standard library**: `Math`, `Random`, `Time`, `std/math2d` (`Vec2`, `Rect`), `std/fs`
  (files and folders).
- **Modules and packages**: `import "file.kl"`; the `utkal` package manager for git and
  path packages, with per-package namespaces (`import "mathx" as mx`) and a lock file.
- **Rust interop**: call your own Rust code and any crates.io library with `use rust`,
  `use crate`, `extern fn` and inline `= rust { ... }` bodies — the way to graphics, audio
  and ML libraries.
- **Fast and safe**: compiles to Rust and then native code; no null, no unchecked
  indexing, no silent overflow, no `unsafe` in generated code.
- **Tooling**: `test fn` with `assert` / `assert_eq` (`kalinga test`), a formatter
  (`kalinga fmt`), a language server (`kalinga lsp`), and `kalinga fix` to update code for
  a new version.

## Documentation

The full guide, with runnable examples for every feature, is in [docs/](docs/README.md):
getting started, basics, control flow, functions, arrays and strings, maps, optionals,
enums and match, classes, inheritance, interfaces, lambdas, generics, structs, error
handling, modules, packages, Rust interop, threads, async, making a game, safety, tooling,
internals and a language reference. See [CHANGELOG.md](CHANGELOG.md) for what is new.

## Usage

Requires a Rust toolchain (`rustc` on PATH; `cargo` for crates and games; `git` for git
packages). Install both commands, `kalinga` and `utkal`:

```bash
cargo install --path .
```

Or run straight from the repository:

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
`check <file>` only reports errors, `test <file>` runs the file's `test fn`s, and
`fmt <files or folders>` lays code out in the standard style.

```bash
cargo run -- run examples/pong/main.kl
```

Projects with packages use `utkal`:

```bash
utkal new my-game
cd my-game
utkal add mathx --git https://github.com/someone/mathx --version 0.4
utkal run
```

## Editor support

`kalinga lsp` is a language server built on the compiler: errors as you type, completion,
hover, signature help, go to definition, find references, rename, outline and quick fixes,
for any editor that speaks the Language Server Protocol. The VS Code extension in
[editors/vscode](editors/vscode/README.md) uses it, and adds syntax highlighting, snippets,
a Run button and `utkal.toml` hints.

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
| Modules | `import "lib/shapes.kl"`, `import "std/game"`, `import "mathx" as mx` (packages) |
| Errors | `fn f() -> Result<int, str>`, `return Err("...")`, `let v = f()?`, `match r { Ok(v) => ... Err(e) => ... }` |
| Threads | `Thread.spawn(fn() => work())` → `Task<T>`, `.join()`; `Channel<T>()`, `Shared(0)`, `xs.par_map(f)` |
| Async | `async fn load() -> int { await Time.wait(0.5) ... }`, `await job`, `Async.all([...])`, `Async.spawn(job)` |
| Structs | `struct Vec2 { x: float, y: float }`, `mut fn`, `op +(o: Vec2) -> Vec2`, `2.0 * v`, generic `struct Grid<T>`, `static ORIGIN: Vec2 = ...` |
| Library | `Math.sqrt(x)`, `Math.lerp(a, b, t)`, `Random.int(1, 7)`, `Time.now()` |
| Rust interop | `use rust "native/x.rs"`, `use crate "rand" = "0.8" features ["..."]`, `extern fn f(x: int) -> int`, `extern fn g(x: int) -> int = rust { x * 2 }`, `extern type T` |
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
- Failures that are part of normal use are `Result` values the compiler makes you handle.
- Threads share no objects, so there are no data races.
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
| `src/main.rs`, `src/bin/utkal.rs` | the `kalinga` and `utkal` commands |
| `crates/kalinga-syntax/` | tokens, lexer, syntax tree, parser, source positions and errors |
| `crates/kalinga-sema/` | package namespaces; the declaration model and its rules; type checking into a typed tree; the built-ins registry; the name index editors use |
| `crates/kalinga-codegen/` | Rust generation from the typed tree; the runtime (`runtime/*.rs`) |
| `crates/kalinga-lsp/` | the language server (`kalinga lsp`) |
| `crates/kalinga-project/` | `utkal.toml`, `utkal.lock`, fetching packages |
| `crates/kalinga-driver/` | loads files and packages, invokes `rustc` / `cargo` and runs the program |
| `crates/kalinga-driver/std/` | bundled modules: `math2d.kl`, `fs.kl`, `game.kl` and their Rust glue |
| `examples/` | `hello.kl`, `oop.kl`, `fib.kl`, `inventory.kl`, `pipeline.kl`, `particles.kl`, `shapes/` (modules), `interop/` (Rust), `pong/`, `asteroids/` |
| `tests/ui/`, `tests/ui.rs` | output snapshot tests: each `.kl` with its expected `.stdout` / `.stderr` / `.exit` (`BLESS=1` updates them) |
| `tests/cli.rs`, `tests/utkal.rs` | command-line behaviour of `kalinga` and `utkal` |
| `editors/vscode/` | VS Code extension (a client of `kalinga lsp`) |
| `docs/` | language guide and reference |
| `tests/docs.rs` | compiles every example in `docs/` and checks its documented output |
