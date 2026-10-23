# 25. How It Works

Kalinga is a **source-to-source compiler**: it translates Kalinga into Rust and lets
`rustc` produce optimized machine code. Early C++ compilers worked the same way, turning
C++ into C. This chapter explains the pipeline and how Kalinga's features map to Rust.
You do not need it to write Kalinga programs, but it explains the performance model.

## The pipeline

```text
 hello.kl
    │
    ▼
 Lexer        kalinga-syntax   characters → tokens; decides which newlines end statements
    │
    ▼
 Parser       kalinga-syntax   tokens → syntax tree; `import`ed files are loaded and
                               merged by kalinga-driver
    │
    ▼
 Checker      kalinga-sema     package names, classes, interfaces, inheritance, overrides
    │
    ▼
 Codegen      kalinga-codegen  type-checks every function body and writes Rust source
    │
    ▼
 rustc -O     kalinga-driver   Rust → native executable (with overflow checks on)
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
| `{K: V}` | `BTreeMap<K, V>` (sorted keys) |
| `enum E` | a Rust `enum E` (`#[derive(Clone)]`, plus `PartialEq` when printable) |
| `fn(A) -> B` | `Rc<dyn Fn(A) -> B>` |
| `T` (type parameter) | a Rust generic `T: Clone + 'static` |
| `Stack<int>` | `Rc<RefCell<Stack<i64>>>` |
| `struct Vec2` | a plain Rust `struct Vec2` (`#[derive(Clone)]`), stored inline |
| `extern type T` | the Rust type `T` from a `use rust` file |
| `Result<T, E>` | `Result<T, E>` |
| `Task<T>` | a handle to a `std::thread` plus its result |
| `Channel<T>` / `Shared<T>` | `Arc<Mutex<VecDeque<T>>>` + `Condvar` / `Arc<Mutex<T>>` |
| `Async<T>` | a boxed Rust `Future`, run by the built-in executor |

- `Rc` is a reference-counted pointer: an object is freed automatically when the last
  reference to it goes away. No garbage collector, no manual `free`.
- `RefCell` enforces "one writer at a time" at runtime. This is what turns the re-entrancy
  mistake from [chapter 23](23-safety-and-errors.md#re-entrant-objects) into a clear error
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
    println(s.area)
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
| `"a {x} b"` | `format!("a {} b", show(x))` |
| `match e { V(x) => ... }` | `match e { E::V(x) => ... }` |
| `fn(x) => x + k` | `{ let k = k.clone(); Rc::new(move \|x\| x + k) }` |
| `x as Dog` | a generated `__as_Dog()` hook on the object's trait |
| `a + b` (struct) | `a.__op_0(b)`, the struct's `op +` method |
| `use rust "f.rs"` | `#[path = "..."] mod __rs_f; use __rs_f::*;` |
| `Math.sqrt(x)` / `Random.int(a, b)` | `x.sqrt()` / a small xorshift generator in the runtime |
| `expr?` | `match expr { Ok(v) => v, Err(e) => return Err(e) }` (the same with `None` for optionals) |
| `Thread.spawn(fn() => ...)` | `std::thread::spawn(move \|\| ...)`; captures are checked to be values |
| `xs.par_map(f)` | chunks of `xs` mapped on `std::thread::scope` threads |
| `async fn f() -> T` / `await x` | a Rust `async` block returning `T` / `.await` |
| `extern fn f() = rust { ... }` | `fn __x_f() { ... }` |

Method-call arguments are evaluated into temporaries before the object is borrowed, so
`a.f(a.x)` is safe.

## Threads and async

Objects are `Rc<RefCell<...>>`, which Rust does not allow to cross threads. Kalinga keeps
them: only values (numbers, strings, arrays, structs, enums, channels, shared cells) may be
captured by a thread lambda or sent through a channel, and the compiler checks this before
Rust does. Single-threaded code therefore pays nothing for threads.

`async fn`s compile to Rust futures. The runtime includes a small single-threaded
executor (a ready queue, a timer list and wakers): `await` polls the future, `Time.wait`
registers a timer, and `await task` on a thread wakes the executor when the thread ends.
`async fn main()` runs on this executor.

## Packages and names

Every file belongs to a *unit*: the project, a utkal package or a `std/` module. All units
are compiled into one Rust program, so when two units declare the same top-level name,
the compiler renames all but one of them (`Vec2__mathx`) before type checking, and
resolves every use to the right one: a file's own unit first, then the units it imports;
`alias.Name` names one unit directly. Error messages turn the internal names back into
`mathx.Vec2`.

## Objects know themselves

Every object stores a weak pointer to its own `Rc` (created with `Rc::new_cyclic`). That
is what lets a method use `self` as a value, and what `as`/`is` use: each class hierarchy's
root trait (and each interface) gets one `__as_X()` method per class `X`, returning `None`
by default and `Some(...)` in `X` and its subclasses.

## Why Kalinga is fast

- The output is native code compiled by `rustc -O` (LLVM), the same optimizer that
  compiles Rust itself.
- Structs are plain Rust structs: an array of a million `Vec2`s is one block of memory,
  exactly as in Rust.
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

The compiler is a Cargo workspace. Each crate depends only on the ones above it:

| Crate / file | Role |
|---|---|
| `crates/kalinga-syntax` | `token.rs`, `lexer.rs` (tokens, newline handling), `ast.rs`, `parser.rs` (syntax tree, recursive-descent parser), `error.rs` (positions, the source-line-with-caret display) |
| `crates/kalinga-sema` | `namespaces.rs` (package namespaces: renaming clashing names, `import ... as`), `checker.rs` (type model, class hierarchy, override and interface rules) |
| `crates/kalinga-codegen` | Body type checking, Rust generation, runtime prelude |
| `crates/kalinga-project` | `utkal.toml`, `utkal.lock`, fetching git and path packages, versions |
| `crates/kalinga-driver` | Loads files and packages, writes the `.rs` file, runs `rustc` or `cargo` and the program; `stdlib.rs` and `std/` hold the bundled modules (`math2d`, `game` and its Rust glue) |
| `src/main.rs`, `src/bin/utkal.rs` | The `kalinga` and `utkal` command lines |

---

Previous: [24. Tooling](24-tooling.md) · Next: [26. Language Reference](26-reference.md)
