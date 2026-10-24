# 27. Stability

Kalinga 0.5 is the first **stable** release. From 0.5 on:

- **Code that compiles keeps compiling** in every later 0.x release, and keeps doing the
  same thing.
- When a change cannot be avoided (a design mistake, a safety problem), it comes with a
  `kalinga fix` rule that updates your code, and the compiler's error message for the old
  form says to run it:

  ```bash
  kalinga fix src/
  ```

  `utkal fix` does the same for a whole project and records the new version in
  `utkal.toml`. 0.5 itself has two such rules, for code written for 0.4: `print(...)`
  becomes `println(...)` (`print` no longer ends the line), and member constants written
  `static NAME: Type = value` become `const NAME: Type = value`.

## What the promise covers

- The syntax and meaning of programs: everything in these docs and the
  [Language Reference](26-reference.md).
- The built-in functions and methods, and the `Math`, `Random`, `Time`, `Thread` and
  `Async` libraries, and the standard modules (`std/math2d`, `std/fs`, `std/game`): what is
  there stays, with the same signatures.
- The `kalinga` and `utkal` commands and their options, and the `utkal.toml` format.

## What may still change

- **Additions**: new syntax, built-ins, library functions, modules, commands and editor
  features. A new built-in name could clash with one of your own functions; the error then
  names the clash, and renaming your function fixes it.
- **Messages**: the wording of compiler errors, warnings and runtime errors.
- **The generated Rust**: `kalinga emit` shows how programs are compiled today, which can
  change in any release.
- **Formatting**: `kalinga fmt` may lay out code differently in a later release (it never
  changes what the code does).
- **Performance**: how fast programs run and how much memory they use can change; the aim
  is that it only improves.

## Not in 0.5

These are known gaps, so code does not depend on them working in a particular way yet:
tuples, `?.` on optionals, `async` instance methods, objects shared across threads (only
values cross threads), exceptions (use `Result`), bounds on type parameters, generic
interfaces and enums, and inheritance with generic classes.

## Versions

`kalinga -v` prints the compiler's version. A `utkal.toml` can say which language version
its code is written for (`kalinga = "0.5"`); `utkal new` and `utkal fix` fill it in.

---

Previous: [26. Language Reference](26-reference.md)
