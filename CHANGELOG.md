# Changelog

## 0.3.0 — foundations for games and AI

### Language
- **Structs**: value types (`struct Vec2 { x: float, y: float }`) that are copied on
  assignment and stored inline in arrays, with methods, `mut fn` for methods that change
  `self`, `static fn` and `private`. A million struct values cost the same time and memory
  as in hand-written Rust.
- **Operator overloading** on structs: `op +(o: Vec2) -> Vec2`, several overloads per
  operator by right-hand type, unary `op -()`, and `+=` and friends.
- **Standard library**: `Math` (sqrt, pow, trig, floor/ceil/round, min/max/clamp, lerp,
  PI/TAU/E/INF), `Random` (int, float, chance, pick, shuffle, seed) and `Time` (now,
  millis, sleep).
- **Rust interop**: `use rust "file.rs"` compiles a Rust file into the program,
  `extern fn` / `extern type` declare Rust functions and opaque types, `= "rust::path"`
  binds a Rust function directly, and `use crate "name" = "version"` uses crates.io
  libraries (built with cargo; compiled crates are shared between programs).

### Performance
- Method calls and field reads borrow their receiver instead of copying a reference
  (object-heavy code: about 1.33x → 1.16x hand-written Rust).
- Objects store a self-pointer only when their class needs it (1M objects: 73 MB → 58 MB).

### Tooling
- `kalinga -v` / `--version`.
- `kalinga check` accepts files without `fn main()` (files that are only imported).
- VS Code extension 0.4.0: completion and signature help for structs, `Math`/`Random`/`Time`,
  `extern` declarations; new snippets (`struct`, `mfn`, `op`, `userust`, `usecrate`,
  `extern`, `gameloop`).

### Docs
- New chapters: Structs and Operators, Standard Library, Rust Interop (chapters now 1–21).
- New examples: `particles.kl` (a 10,000-particle simulation) and `interop/` (Rust stats
  and an opaque matrix type).

## 0.2.0

### Language
- **String interpolation**: `"Hi {name}, {len(xs)} items"`; `{{` and `}}` for literal braces.
  *Breaking:* a lone `{` or `}` inside a string is now an error.
- **Maps** `{K: V}` with `int`, `str` or `bool` keys, kept in sorted order: `m[k]`,
  `get`, `has`, `remove`, `keys`, `values`, `len`, `for k in m`.
- **Modules**: `import "file.kl"`, resolved relative to the importing file; errors point
  into the imported file.
- **Enums** with payloads (`Circle(r: float)`) and **`match`** on enums, `int`, `str` and
  `bool`, with exhaustiveness checking.
- **OOP**: `private` fields and methods, `static fn`, `abstract class` / `abstract fn`,
  interface inheritance (`interface Pet : Named`).
- **Downcasting**: `x as Dog` (gives `Dog?`) and `x is Dog`; `self` can be used as a value.
- **Lambdas and function types**: `fn(x: int) -> int { ... }`, `fn(x) => x * 2`,
  `fn(int) -> int`; named functions as values; array `map`, `filter`, `find`, `sort`,
  `sort_by`.
- **Generics**: `fn first<T>(xs: [T]) -> T?`, `class Stack<T>`, with type inference.
- A line starting with `.` continues the previous line (method chains).
- Newlines inside `{ }` are significant again even within `( )`, so block lambdas can be
  passed as arguments.

### Tooling
- VS Code extension 0.3.0: completion and signature help for all of the above, errors in
  imported files shown on the `import` line, new snippets and highlighting.

### Docs
- Five new chapters (maps, enums and match, lambdas, generics, modules); chapters
  renumbered 1–18. Every complete example is compiled by the test suite.

## 0.1.0

First version: classes, single inheritance, interfaces, optionals, arrays, compile to Rust,
`run`/`build`/`emit`/`check`, VS Code extension, documentation.
