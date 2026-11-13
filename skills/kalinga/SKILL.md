---
name: kalinga
description: Read, write, fix, explain and review code in Kalinga, the object-oriented language that compiles to Rust (`.kl` files, `kalinga` and `utkal` commands, `utkal.toml` projects). Use this skill whenever the user mentions Kalinga, a `.kl` file, utkal, or pastes code that uses `fn main()` with `println`, `init(...)`, `override fn`, `var` and `T?` optionals together — even if they only ask to "fix this error", "add a feature" or "port this to Kalinga". Kalinga looks like Swift, Kotlin and Rust but differs in many small ways, so do not write it from memory of those languages.
---

# Kalinga

Kalinga (version 0.6) is a small, statically typed, object-oriented language that compiles
to native code through Rust. Programs are `.kl` files with a `fn main()`. It has classes
with inheritance and interfaces, generics with bounds, enums with `match`, value-type
structs with operators, optionals instead of null, `Result` and `?` instead of exceptions,
lambdas, tuples, threads, async, and a standard library (`Math`, `Random`, `Time`,
`std/fs`, `std/reactive`, `std/math2d`, `std/game`). Memory is managed by reference
counting; there is no garbage collector.

It borrows syntax from Swift, Kotlin and Rust, but it is its own language. Most mistakes come
from writing what one of those would accept, so check unfamiliar constructs against this
file and `references/language-reference.md` rather than guessing.

## Work with the compiler

When the `kalinga` command is available, let it check your work. It is fast and its errors
name the fix (including "did you mean ...?" for misspelt names):

```bash
kalinga fmt main.kl
```

```bash
kalinga check main.kl
```

```bash
kalinga run main.kl
```

- `kalinga check` type-checks without building (and allows a file without `fn main()`);
  `kalinga run` builds and runs (it needs `rustc`); `kalinga test` runs `test fn`s.
- `kalinga fmt` sets the standard layout (4-space indents, one statement per line); run it
  on code you write.
- Projects have a `utkal.toml` and `src/main.kl` (or `src/lib.kl`): use `utkal check`,
  `utkal run`, `utkal test`, `utkal fmt`.
- Exit codes: 0 success, 1 compile error, 101 runtime error.
- If the compiler is not available, be extra careful with the rules below, and say the code
  was not compiled.

## A complete program

```kalinga
interface Shape {
    fn area() -> float
    fn name() -> str
}

class Circle : Shape {
    radius: float

    init(radius: float) {
        self.radius = radius
    }

    override fn area() -> float {
        return Math.PI * self.radius * self.radius
    }

    override fn name() -> str {
        return "circle"
    }
}

class Square : Shape {
    side: float = 1.0

    override fn area() -> float {
        return self.side * self.side
    }

    override fn name() -> str {
        return "square"
    }
}

fn largest(shapes: [Shape]) -> Shape? {
    var best: Shape? = none
    for s in shapes {
        if b = best {
            if s.area() > b.area() {
                best = s
            }
        } else {
            best = s
        }
    }
    return best
}

fn main() {
    shapes: [Shape] = [Circle(1.5), Square()]
    for s in shapes {
        println("{s.name()}: {Math.round(s.area() * 100.0) / 100.0}")
    }
    if big = largest(shapes) {
        println("largest is the", big.name())
    }
    counts = {"circle": 1, "square": 1}
    println(counts, len(shapes))
}
```

```text
circle: 7.07
square: 1.0
largest is the circle
{circle: 1, square: 1} 2
```

## Where Kalinga differs from what you may expect

These are the rules that trip up code written from memory of other languages. Each one is
a compile error (or a wrong result) if missed.

**Output and text**
- `println(a, b)` prints its values separated by spaces and ends the line; `print(...)`
  does not end the line. There is no `printf`, `console.log` or `puts`.
- Interpolation is `"{expr}"` (not `${}` or `\(...)`); write `{{` and `}}` for braces.
- `+` joins two `str`s only: `"n = " + str(n)`, or better `"n = {n}"`.
- Objects (class instances) print only when their class lists `Show`
  (`class Dog : Show`), which generates `as_str() -> str` (`Dog(name: Rex, age: 3)`);
  `override fn as_str() -> str` customises it. Otherwise print a field or a method's result.
  Structs and enums print when their contents do (`Point(x: 1, y: 2)`).
- `==` on objects needs `: Eq` (generated `equals(other: Dog) -> bool`), and `<` / `sort`
  need `: Ord` (generated `compare(other: Dog) -> int`); both can be overridden. Structs may
  list `Show`, `Eq`, `Ord` too (`struct Version : Ord`).

**Variables and types**
- **Fixed by default.** `name = value` declares a fixed variable (no keyword); `var name =
  value` declares one that can change. There is **no `let` and no `const`** (both are errors).
  `name = value` declares only when no variable `name` is visible, otherwise it assigns (an
  error for a fixed one); `name: T = value` always declares. Tuples: `(a, b) = pair`.
- Types follow a colon: `var total: float = 0.0`.
- An unused variable is a warning; a misspelt assignment (`totl = total + 1`) shows as one.
- No implicit conversions: `float(n)`, `int(x)` (truncates), `str(v)`. A whole-number
  literal is accepted where a `float` is expected (`Vec2(600, 300)`), a variable is not.
- `int` and `float` are 64-bit; integer overflow and division by zero stop the program.
- Statements end at a newline; no semicolons are needed.
- Identifiers cannot start with `__`; `Box`, `Vec`, `Option`, `String`, `Rc`, `Self` and
  the built-in names cannot name a class.

**No null, no exceptions**
- A missing value is `T?`, either a `T` or `none`. Unwrap with
  `if x = maybe { ... } else { ... }` or `while x = maybe { ... }` (`=` in a condition
  always unwraps; compare with `==`). There is no `!` unwrap, no `??` and no `?.`. It also
  works as a value: `n = if x = maybe { x } else { 0 }`.
- Failure is `Result<T, E>`: `return Ok(v)` / `return Err(e)`; `value?` returns the error
  early (only inside a function that returns a `Result`); `match r { Ok(v) => ..., Err(e) => ... }`;
  `r.unwrap_or(d)`.
- `m[key]` on a map stops the program when the key is missing; `m.get(key)` gives `V?`.

**Functions and lambdas**
- `fn name(a: int, b: str) -> bool { ... }`; no return type means none. Every path of a
  function with a return type must `return`.
- Named arguments are allowed in parameter order: `connect(host: "x", port: 80)`.
- Lambdas: `fn(x) => x * 2` or `fn(a: int, b: int) -> int { return a + b }`. There is no
  `|x|`, `x => ...` or `{ x in ... }` syntax. A function type is `fn(int) -> int`.
- A lambda gets a copy of the outside variables it uses, except a `var` that some lambda
  changes, which is shared (`var count = 0; inc = fn() { count += 1 }` works).
- Inside a method, `self` cannot be used in a lambda; copy it first: `me = self`.

**Classes, structs and interfaces**
- Fields are **fixed** unless `var`: `name: Type` is set by its default or in its own
  class's `init` only (not in methods, subclasses or outside, and not with `push`/`+=`);
  `var name: Type` can change. Struct fields: fixed ones are set when the struct is built.
  `init(...)` is the constructor; objects are created with no `new`:
  `Circle(1.5)`. A class without `init` is created as `Square()` using field defaults, and
  a subclass without its own `init` is created with its base class's.
- Methods take no `self` parameter, but members are always reached through `self.`.
- Fields of type `int`, `float`, `bool`, `str`, array and map start at `0`, `0.0`,
  `false`, `""`, `[]`, `{}` unless given a default. Fields holding an object, a function,
  an enum or a struct need a default (`= Square()`, `= fn() {}`) or must be optional
  (`Square?`), even when `init` sets them.
- `class Dog : Animal, Named` extends at most one class and any number of interfaces. A
  method coming from a base class or an interface must be marked `override fn`. Interface
  methods may have a default body (`fn greet() -> str { return "hi {self.name()}" }`), which
  classes get unless they override it. A subclass
  `init` calls `super(args)` first when the base has an `init`.
- `private(set) var balance: int` is readable anywhere but changed only inside its class or
  struct.
- `#[deprecated("use g")]` on the line before a declaration makes each use a warning (the
  program still builds). It is the only attribute.
- `private` members, `static fn` (called `Type.f()`), `static NAME: T = v` (fixed) and
  `static var name: T = v` members (used as `Type.NAME`); top-level `NAME: T = v`. Their
  values are set at compile time: literals, operators, other static values, `none`, array /
  map / tuple literals, enum variants and struct values; **no calls or class objects** (for a
  shared object: `static var boss: Enemy? = none`, set at run time). No global variables.
- Classes are references (shared); `struct`s are values, copied on assignment, with
  `mut fn` for methods that change `self` and `op +(other: T) -> T` operators.
- Arrays and maps are **values** too: `var b = a` copies.
- While an object's method runs, the object is held: reaching the same object again (for
  example `a.transfer_to(a, 10)`, or a callback that calls back into it) stops the program
  with `RefCell already borrowed`. Restructure, or keep shared state in small structs.

**Collections**
- Arrays: `push`, `pop`, `len`, `contains`, `map`, `filter`, `find`, `sort`, `sort_by`,
  `join`, slices `xs[a..b]` (either end may be left out: `xs[1..]`). There is no
  `remove(index)`, `insert`, `+` concatenation,
  `length`, `size` or `count`: use `len(xs)` or `xs.len()`, `filter`, slices, or a loop.
- Maps: `{"a": 1}`, type `{str: int}`; keys are `int`, `str` or `bool`; `get`, `has`,
  `remove`, `keys`, `values`, `len`; iterate with `for k, v in m`.
- `for i in 0..n`, `0..=n`, `for x in xs`, `for i, x in xs`, `for (a, b) in pairs`.
- Strings: `len`, `upper`, `lower`, `trim`, `split`, `replace`, `contains`, `starts_with`,
  `ends_with`, `find`, `chars`, `parse_int`, `parse_float` (snake_case, not camelCase).

**Enums and match**
- `enum Shape { Circle(r: float), Empty }`; construct as `Shape.Circle(1.0)`, match as
  `Circle(r) => ...` (no enum prefix needed in patterns). `match` must cover every case or
  have `_ =>`; arms on one line are separated by `,`.
- `if` and `match` can be values (`x = if a > b { a } else { b }`); then `else` is
  required.

**Generics**
- `fn first<T>(xs: [T]) -> T?`, `class Stack<T>`, `struct Pair<A, B>`,
  `interface Store<T>`, `enum Tree<T>`. A plain `T` can only be stored and passed; to print,
  compare or call methods on it, add a bound: `<T: Show>`, `<T: Eq>`, `<T: Ord>`,
  `<T: Shape>`, `<T: Show + Eq>`.
- Generic classes cannot extend classes or implement interfaces (yet).

## Feature snippets

Tuples and optionals:

```kalinga
fn min_max(xs: [int]) -> (int, int) {
    var lo = xs[0]
    var hi = xs[0]
    for x in xs {
        if x < lo {
            lo = x
        }
        if x > hi {
            hi = x
        }
    }
    return (lo, hi)
}

fn main() {
    (lo, hi) = min_max([4, 9, 1])
    ages: {str: int} = {"Asha": 30}
    age = if a = ages.get("Ravi") { a } else { -1 }
    println(lo, hi, age)
}
```

```text
1 9 -1
```

Results and `?`:

```kalinga
fn parse_age(text: str) -> Result<int, str> {
    t = text.trim()
    if n = t.parse_int() {
        return if n >= 0 { Ok(n) } else { Err("negative: {n}") }
    }
    return Err("not a number: {text}")
}

fn total(a: str, b: str) -> Result<int, str> {
    return Ok(parse_age(a)? + parse_age(b)?)
}

fn main() {
    println(total("20", " 22 "))
    match total("20", "x") {
        Ok(n) => println("sum", n),
        Err(e) => println("error:", e),
    }
}
```

```text
Ok(42)
error: not a number: x
```

Enums, generics with a bound, and lambdas:

```kalinga
enum Tree<T> {
    Leaf
    Node(value: T, left: Tree<T>, right: Tree<T>)
}

fn insert<T: Ord>(t: Tree<T>, x: T) -> Tree<T> {
    return match t {
        Leaf => Tree.Node(x, Tree.Leaf, Tree.Leaf),
        Node(v, l, r) => if x < v { Tree.Node(v, insert(l, x), r) } else { Tree.Node(v, l, insert(r, x)) },
    }
}

fn to_list<T>(t: Tree<T>, out: [T]) -> [T] {
    match t {
        Leaf => {
            return out
        }
        Node(v, l, r) => {
            var acc = to_list(l, out)
            acc.push(v)
            return to_list(r, acc)
        }
    }
}

fn main() {
    var t: Tree<int> = Tree.Leaf
    for x in [5, 2, 8, 1] {
        t = insert(t, x)
    }
    sorted = to_list(t, [])
    println(sorted, sorted.map(fn(x) => x * 10).filter(fn(x) => x > 20))
    var calls = 0
    count = fn() {
        calls += 1
    }
    count()
    count()
    println(calls)
}
```

```text
[1, 2, 5, 8] [50, 80]
2
```

## Reference files

Read these when the task needs more than the rules above:

- `references/language-reference.md`: the whole language in one page — types,
  assignability, operator precedence, every statement and declaration form, all built-in
  functions and methods, how values print, the grammar, and what is not supported yet.
  Read it before using a construct you have not seen above, and when a compiler error
  surprises you.
- `references/standard-library.md`: `Math`, `Random`, `Time`, threads (`Thread`,
  `Channel`, `Shared`), async, files (`std/fs`), 2D math (`std/math2d`), reactive values
  (`std/reactive`), tests (`test fn`), modules and packages, and Rust interop.
- `references/examples.md`: complete idiomatic programs (inheritance, structs with
  operators, interfaces with generics, threads and channels, `std/reactive`) to copy
  patterns from.
