# Kalinga Documentation

Kalinga is a small, safe, object-oriented language that compiles to native code through
Rust. It aims to be to Rust what early C++ was to C: friendlier syntax and classes on top,
with the speed and memory safety of the language underneath.

```kalinga
class Greeter {
    name: str

    init(name: str) {
        self.name = name
    }

    fn greet() -> str {
        return "Hello, " + self.name + "!"
    }
}

fn main() {
    let g = Greeter("Kalinga")
    println(g.greet())
}
```

Output:

```text
Hello, Kalinga!
```

## Guide

Read these in order the first time. Each chapter builds on the previous one.

| # | Chapter | What you learn |
|---|---|---|
| 1 | [Getting Started](01-getting-started.md) | Install, write, run and build your first program |
| 2 | [Basics](02-basics.md) | Comments, statements, variables, types, operators |
| 3 | [Control Flow](03-control-flow.md) | `if`, `while`, `for`, `break`, `continue` |
| 4 | [Functions](04-functions.md) | Parameters, return values, recursion, built-in functions |
| 5 | [Arrays and Strings](05-arrays-and-strings.md) | Lists of values, text, string interpolation |
| 6 | [Maps](06-maps.md) | `{K: V}` dictionaries |
| 7 | [Optionals](07-optionals.md) | `T?`, `none` and `if let` — Kalinga's replacement for null |
| 8 | [Enums and match](08-enums-and-match.md) | Variants with data, exhaustive `match` |
| 9 | [Classes](09-classes.md) | Fields, `init`, methods, `self`, `private`, `static` |
| 10 | [Inheritance](10-inheritance.md) | `class B : A`, `override`, `super`, abstract classes, `is`/`as` |
| 11 | [Interfaces](11-interfaces.md) | Contracts that many classes implement; interface inheritance |
| 12 | [Lambdas](12-lambdas.md) | Function values, captures, `map`/`filter`/`sort_by` |
| 13 | [Generics](13-generics.md) | `fn first<T>`, `class Stack<T>` |
| 14 | [Structs and Operators](14-structs-and-operators.md) | Value types, `mut fn`, `op +` — fast game math |
| 15 | [Standard Library](15-standard-library.md) | `Math`, `Random`, `Time`, files with `std/fs` |
| 16 | [Error Handling](16-error-handling.md) | `Result<T, E>`, `Ok` / `Err`, `?` |
| 17 | [Modules](17-modules.md) | Splitting a program across files with `import` |
| 18 | [Packages with utkal](18-packages.md) | Projects, git and path packages, `import ... as`, `utkal.lock` |
| 19 | [Rust Interop](19-rust-interop.md) | `use rust`, `use crate`, `extern fn`, inline `rust { }` bodies |
| 20 | [Threads](20-threads.md) | `Thread.spawn`, `Channel`, `Shared`, `par_map` |
| 21 | [Async](21-async.md) | `async fn`, `await`, `Async.all`, timers |
| 22 | [Making a Game](22-making-a-game.md) | `std/game`: window, drawing, input, images, sounds |
| 23 | [Safety and Errors](23-safety-and-errors.md) | Compile errors, runtime errors and what Kalinga guarantees |

## Reference

| Document | Contents |
|---|---|
| [Tooling](24-tooling.md) | `kalinga` and `utkal` commands, the VS Code extension |
| [How It Works](25-how-it-works.md) | How Kalinga code becomes Rust, and why it is fast |
| [Language Reference](26-reference.md) | Keywords, types, operator precedence, built-ins, grammar |
| [Stability](27-stability.md) | What stays the same from 0.5 on, and how changes are handled |

## Conventions in these docs

- Code blocks marked `kalinga` that contain `fn main()` are complete programs. They are
  compiled by the test suite (`cargo test`), so they always match the current compiler.
- Blocks marked `kalinga error` must be rejected by the compiler; the test suite checks that too.
- Blocks marked `text` show program output or compiler messages.
- Examples that need other files or downloads (`import`, `use rust`, `use crate`, packages,
  `std/game`) are checked by the test suite's import, interop, package and game tests instead.
