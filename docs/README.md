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
| 5 | [Arrays and Strings](05-arrays-and-strings.md) | Lists of values, text, their methods |
| 6 | [Optionals](06-optionals.md) | `T?`, `none` and `if let` — Kalinga's replacement for null |
| 7 | [Classes](07-classes.md) | Fields, `init`, methods, `self`, objects as references |
| 8 | [Inheritance](08-inheritance.md) | `class B : A`, `override`, `super`, polymorphism |
| 9 | [Interfaces](09-interfaces.md) | Contracts that many classes can implement |
| 10 | [Safety and Errors](10-safety-and-errors.md) | Compile errors, runtime errors and what Kalinga guarantees |

## Reference

| Document | Contents |
|---|---|
| [Tooling](11-tooling.md) | `kalinga` commands and the VS Code extension |
| [How It Works](12-how-it-works.md) | How Kalinga code becomes Rust, and why it is fast |
| [Language Reference](13-reference.md) | Keywords, types, operator precedence, built-ins, grammar |

## Conventions in these docs

- Code blocks marked `kalinga` that contain `fn main()` are complete programs. They are
  compiled by the test suite (`cargo test`), so they always match the current compiler.
- Blocks marked `text` show program output or compiler messages.
