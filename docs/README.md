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
    print(g.greet())
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
| 14 | [Modules](14-modules.md) | Splitting a program across files with `import` |
| 15 | [Safety and Errors](15-safety-and-errors.md) | Compile errors, runtime errors and what Kalinga guarantees |

## Reference

| Document | Contents |
|---|---|
| [Tooling](16-tooling.md) | `kalinga` commands and the VS Code extension |
| [How It Works](17-how-it-works.md) | How Kalinga code becomes Rust, and why it is fast |
| [Language Reference](18-reference.md) | Keywords, types, operator precedence, built-ins, grammar |

## Conventions in these docs

- Code blocks marked `kalinga` that contain `fn main()` are complete programs. They are
  compiled by the test suite (`cargo test`), so they always match the current compiler.
- Blocks marked `kalinga error` must be rejected by the compiler; the test suite checks that too.
- Blocks marked `text` show program output or compiler messages.
- Examples that `import` other files are checked by the test suite's import test instead.
