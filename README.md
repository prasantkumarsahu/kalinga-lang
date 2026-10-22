# Kalinga

A small "Rust++": simple syntax, classes with inheritance and interfaces, compiled to
native code through Rust. It relates to Rust the way early C++ related to C — the
Kalinga compiler translates `.kl` files to Rust source and hands them to `rustc -O`,
so programs get native speed and Rust's memory safety.

```kalinga
interface Show {
    fn show() -> str
}

class Animal {
    name: str
    sound: str = "..."

    init(name: str) { self.name = name }

    fn speak() -> str { return self.name + " says " + self.sound }
}

class Dog : Animal, Show {          // `:` lists one base class + any interfaces
    init(name: str) {
        super(name)
        self.sound = "woof"
    }
    override fn speak() -> str { return super.speak() + "!" }
    fn show() -> str { return "Dog(" + self.name + ")" }
}

fn main() {
    let pets: [Animal] = [Dog("Rex"), Animal("Cat")]
    for p in pets { print(p.speak()) }   // dynamic dispatch
}
```

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
| Types | `int` (64-bit), `float`, `bool`, `str`, `[T]` arrays, `T?` optionals, classes, interfaces |
| Functions | `fn add(a: int, b: int) -> int { return a + b }` |
| Control flow | `if / else if / else`, `while cond { }`, `for i in 0..10 { }`, `for x in array { }`, `for ch in "text" { }`, `break`, `continue` |
| Optionals | `var o: int? = none`, `if let v = o { } else { }`, `o == none` |
| Classes | fields with optional defaults, `init(...)`, methods with `fn`, `self.field`, `self.method()` |
| Inheritance | `class B : A` — single base class; overriding needs `override fn`; `super(args)` in `init`, `super.method()` |
| Interfaces | `interface I { fn m() -> str }`, `class C : Base, I, J`; interfaces work as types |
| Built-ins | `print(a, b, ...)`, `len(x)`, `str(x)`, `int(x)`, `float(x)`, `input(prompt)` |
| Array methods | `push(v)`, `pop()` → `T?`, `len()`, `contains(v)` |
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
| `examples/` | `hello.kl`, `oop.kl`, `fib.kl` |
| `tests/examples.rs` | end-to-end tests |
| `editors/vscode/` | VS Code extension |
