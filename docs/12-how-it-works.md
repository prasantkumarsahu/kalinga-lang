# 12. How It Works

Kalinga is a **source-to-source compiler**: it translates Kalinga into Rust and lets
`rustc` produce optimized machine code. Early C++ compilers worked the same way, turning
C++ into C. This chapter explains the pipeline and how Kalinga's features map to Rust.
You do not need it to write Kalinga programs, but it explains the performance model.

## The pipeline

```text
 hello.kl
    │
    ▼
 Lexer        src/lexer.rs     characters → tokens; decides which newlines end statements
    │
    ▼
 Parser       src/parser.rs    tokens → syntax tree (src/ast.rs)
    │
    ▼
 Checker      src/checker.rs   classes, interfaces, inheritance, overrides, signatures
    │
    ▼
 Codegen      src/codegen.rs   type-checks every function body and writes Rust source
    │
    ▼
 rustc -O     src/driver.rs    Rust → native executable (with overflow checks on)
```

`kalinga check` stops after codegen without writing any file; `kalinga emit` prints the
Rust code; `kalinga build` and `run` go all the way.

Every Kalinga error is reported by the first three stages, in Kalinga terms. If `rustc`
ever rejects the generated code, that is a bug in the Kalinga compiler, and it is
reported as an *internal compiler error*.

## Mapping types

| Kalinga | Rust |
|---|---|
| `int` | `i64` |
| `float` | `f64` |
| `bool` | `bool` |
| `str` | `String` |
| `[T]` | `Vec<T>` |
| `T?` | `Option<T>` |
| class `C` with no subclasses | `Rc<RefCell<C>>` |
| class `C` with subclasses | `Rc<RefCell<dyn __I_C>>` |
| interface `I` | `Rc<RefCell<dyn I>>` |

- `Rc` is a reference-counted pointer: an object is freed automatically when the last
  reference to it goes away. No garbage collector, no manual `free`.
- `RefCell` enforces "one writer at a time" at runtime. This is what turns the re-entrancy
  mistake from [chapter 10](10-safety-and-errors.md#re-entrant-objects) into a clear error
  instead of undefined behaviour.

## Mapping classes

Take this program:

```kalinga
class Shape {
    area: float
    fn grow(k: float) {
        self.area *= k
    }
}

class Sq : Shape {
    init(s: float) {
        self.area = s * s
    }
}

fn main() {
    let s: Shape = Sq(3.0)
    print(s.area)
}
```

`kalinga emit` produces (shortened):

```rust
struct Sq {
    area: f64,                       // inherited fields are copied in: flat layout
}

trait __I_Shape {                    // one trait per class, for dynamic dispatch
    fn grow(&mut self, k: f64);
    fn __get_area(&self) -> f64;
    fn __set_area(&mut self, v: f64);
}
trait __I_Sq: __I_Shape {}           // subclass trait extends the base trait

impl Sq {
    fn __new(s: f64) -> Rc<RefCell<Sq>> {
        let mut __o = Sq { area: 0.0f64 };   // 1. field defaults
        __o.__Sq__init(s);                   // 2. init chain
        Rc::new(RefCell::new(__o))
    }
    fn __Sq__init(&mut self, s: f64) { self.area = s * s; }
    fn __Shape__grow(&mut self, k: f64) { self.area *= k; }   // inherited body
    fn grow(&mut self, k: f64) { self.__Shape__grow(k) }      // most-derived version
}

impl __I_Shape for Sq { /* forwards to the methods above */ }

fn kl_main() {
    let s: Rc<RefCell<dyn __I_Shape>> = Sq::__new(3.0) as Rc<RefCell<dyn __I_Shape>>;
    println!("{}", s.borrow().__get_area());
}
```

The rules behind it:

1. **Flat structs.** Each class becomes a `struct` containing all of its fields,
   including inherited ones. Field access is a direct memory offset — no lookup.
2. **Bodies are copied per subclass.** A method body written in `Shape` is emitted as
   `__Shape__grow` in `Shape` *and* in every subclass. `super.grow()` calls
   `__Shape__grow` directly.
3. **Virtual methods.** Each class gets a plain method `grow` that forwards to the
   most-derived body. Inside a method, `self.grow()` therefore always reaches the
   override, even when the call was written in the base class.
4. **One trait per class.** `__I_Shape` lists the methods `Shape` introduces plus getters
   and setters for its fields. Subclass traits extend their base trait, and interface
   traits are added as supertraits. Upcasting `Sq` → `Shape` → `Describable` is then a
   built-in Rust trait-object conversion.
5. **Static dispatch when possible.** A class with **no** subclasses is used through its
   concrete struct, so calls are direct (and can be inlined by the optimizer). Only
   classes that actually have subclasses pay for a virtual call.

## Mapping statements and expressions

| Kalinga | Rust |
|---|---|
| `let x = 1` / `var x = 1` | `let x: i64 = 1` / `let mut x: i64 = 1` |
| `while true { }` | `loop { }` |
| `for i in a..b` | `for i in a..b` |
| `for ch in s` | `for c in s.chars() { let ch = c.to_string(); ... }` |
| `"a" + b` | `format!("{}{}", "a", b)` |
| `xs[i]` | `xs[__idx(i)]` — rejects negative indexes, then Rust checks the length |
| `if let v = opt { }` | `if let Some(v) = opt { }` |
| `fn main()` | `fn kl_main()`, started by a generated `main` on a 256 MB stack thread |

Method-call arguments are evaluated into temporaries before the object is borrowed, so
`a.f(a.x)` is safe.

## Why Kalinga is fast

- The output is native code compiled by `rustc -O` (LLVM), the same optimizer that
  compiles Rust itself.
- `int` and `float` are machine numbers, not boxed objects.
- Fields are stored inline in structs, and leaf classes use static dispatch.
- There is no garbage collector and no interpreter loop.

For example, the naive recursive `fib(35)` from `examples/fib.kl` (about 30 million
calls) runs in roughly 0.05 seconds — in the range of C and Rust, and orders of magnitude
faster than a tree-walking interpreter.

The costs to be aware of:

- Arrays and strings are copied when assigned or passed (value semantics). Copying large
  arrays in a hot loop is expensive; build them in one place and pass them sparingly.
- Every object access goes through a reference count and a borrow check. These are cheap,
  but not free.
- Integer overflow checks stay on in release builds, for safety.

## Source layout

| File | Role |
|---|---|
| `src/main.rs` | Command-line interface |
| `src/token.rs`, `src/lexer.rs` | Tokens, newline handling |
| `src/ast.rs`, `src/parser.rs` | Syntax tree and recursive-descent parser |
| `src/checker.rs` | Type model, class hierarchy, override and interface rules |
| `src/codegen.rs` | Body type checking, Rust generation, runtime prelude |
| `src/driver.rs` | Writes the `.rs` file, runs `rustc` and the program |
| `src/error.rs` | Error type and the source-line-with-caret display |

---

Previous: [11. Tooling](11-tooling.md) · Next: [13. Language Reference](13-reference.md)
