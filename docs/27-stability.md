# 27. Stability

Kalinga is still at 0.x: a release may change the language when the design calls for
it. Every such change comes with a `kalinga fix` rule that updates your code, and the
compiler's error message for the old form says to run it:

```bash
kalinga fix src/
```

`kalinga fix` needs to know which version the code was written for: the nearest
`utkal.toml` says (`kalinga = "0.4"`), or you tell it with `--from 0.4`. A rule only
changes code older than the rule, so fixing code twice, or fixing code already written
for the new version, changes nothing. `utkal fix` does the same for a whole project and
records the new version in `utkal.toml`.

The rules so far:

- **0.5**, for code written for 0.4: `print(...)` becomes `println(...)` (`print` no
  longer ends the line).
- **0.7**, fixed by default, for any code: `let x = v` becomes `x = v` (or `var x = v`
  where it hid another `x`); `if let` / `while let` become `if x = v` / `while x = v`;
  `const NAME: T = v` becomes `NAME: T = v` at the top level and `static NAME: T = v` in a
  class or struct; fields that code changes become `var` fields, and `private(set)` fields
  become `private(set) var`.

## What changes come with a fix rule

- The syntax and meaning of programs: everything in these docs and the
  [Language Reference](26-reference.md).
- The built-in functions and methods, and the `Math`, `Random`, `Time`, `Thread` and
  `Async` libraries, and the standard modules (`std/math2d`, `std/fs`, `std/game`, and
  `std/reactive` from 0.6): what is there stays, with the same signatures.
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

## Not in 0.6

These are known gaps, so code does not depend on them working in a particular way yet:
`?.` on optionals, `async` instance methods, objects shared across threads (only values
cross threads), exceptions (use `Result`), inheritance with generic classes (and generic
classes implementing interfaces), type parameters on interface methods, and tuples in
`match` patterns.

## Versions

`kalinga -v` prints the compiler's version. A `utkal.toml` can say which language version
its code is written for (`kalinga = "0.5"`); `utkal new` and `utkal fix` fill it in.

---

Previous: [26. Language Reference](26-reference.md) · Next: [28. Reactive Values](28-reactive.md)
