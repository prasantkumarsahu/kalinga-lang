# Changelog

## 0.5.0 — stable

From this release on, code that compiles keeps compiling in 0.x: any unavoidable change
comes with a `kalinga fix` rule (see [Stability](docs/27-stability.md)).

### Breaking changes
- **`print` no longer ends the line; `println` does.** `println()` prints an empty line, and
  `print` shows its text at once. `kalinga fix` (and `utkal fix`) rewrites 0.4's `print(...)`
  to `println(...)`.
- **Member constants are `const NAME: Type = value`**, no longer `static NAME: ...`; the
  parser says so and `kalinga fix` rewrites them.
- **`override fn` for interface methods too.** A method that comes from anything after `:`
  in `class C : Base, Iface`, a base class or an interface, must be marked `override fn`;
  the compiler says what to add, and the editor offers a quick fix.
- The VS Code setting `kalinga.checkOnType` is gone: the language server checks every edit.

### Language
- **Strings**: `s[i]` (one character), slices `s[a..b]` / `xs[a..b]` (either end optional),
  and `split`, `replace`, `starts_with`, `ends_with`, `find`, `code_at`, `chars`,
  `parse_int`, `parse_float`; `[str].join(sep)`; built-ins `chr(code)` and `args()`.
- **Recursive enums**: `Add(left: Expr, right: Expr)`, also through structs and optionals.
- **Constants and static variables**: top-level `const NAME: T = value`, and `const` /
  `static var` members of classes and structs, used as `Type.NAME`. Values that depend on
  themselves are rejected.
- **`if` and `match` as values**: `let x = if a > b { a } else { b }`,
  `let n = match d { Up => 1, _ => 0 }`; one-line match arms take commas.
- **Loops**: `while let x = optional { }`, `for i, x in xs`, `for k, v in m`, and inclusive
  ranges `a..=b` (in slices too).
- **Named arguments**: `clamp(15, low: 0, high: 10)`, `Rect(w: 10, h: 5)`, checked against
  the parameters; struct fields with defaults may be skipped.
- **Whole-number literals where a float is expected**: `Vec2(600, 300)`; variables still
  need `float(x)`.
- **`std/fs`**: `File`, `Dir` and `Path` for reading, writing and listing, with `Result`
  errors worded the same on every system.
- **`test fn`** with `assert(condition[, message])` and `assert_eq(value, expected)`.
- **Doc comments**: `///` above a declaration is its documentation (Markdown); plain `//` is
  not.

### Tooling
- **`kalinga lsp`**, a language server built on the compiler: errors as you type (all of
  them at once), completion, hover with docs, signature help, go to definition, find
  references, rename (methods together with what they override or implement), outline,
  quick fixes and formatting.
- **`kalinga fmt`** / **`utkal fmt`** lay code out in one standard style (`--check` for CI).
  They change only whitespace and line breaks and check that the tokens are unchanged.
- **`kalinga test`** / **`utkal test`** run `test fn`s, each on its own thread, and report
  failures with the Kalinga file and line.
- **`kalinga fix`** / **`utkal fix`** update code for a new version.
- The compiler reports up to 20 errors at once instead of the first, and the parser keeps
  going after a mistake in one declaration.

### Editor
- VS Code extension 0.6.0: a thin client of `kalinga lsp` (the old JavaScript completion
  engine is gone), with a Run Tests in File command, Format Document, a `test` snippet, and a
  Kalinga icon for `.kl` files: a 24-spoke wheel after the Ashoka Chakra.

### Internals
- The compiler is a Cargo workspace: `kalinga-syntax`, `kalinga-sema`, `kalinga-codegen`,
  `kalinga-driver`, `kalinga-project`, `kalinga-fmt` and `kalinga-lsp`.
- Type checking produces a typed tree that code generation prints; the runtime is real Rust
  files (`crates/kalinga-codegen/runtime/`), checked by rustfmt and the compiler.
- One registry of built-in functions and methods feeds the type checker, the editor and the
  docs, and tests keep them in step.
- Errors are typed values in every library crate; spans cover whole names.
- Tests: output snapshots for every UI case, every doc example run and its output checked,
  the language server's features, the formatter on all repository code. CI runs on
  Windows, Linux and macOS, with clippy, rustfmt, ESLint and Prettier.

### Fixes
- Async timers that are due at the same time now finish in time order, also on a busy
  machine.

## 0.4.0 — first playable

### Games
- **`std/game`**: a window, 2D drawing (rectangles, circles, lines, triangles, text,
  images), keyboard and mouse input, sounds and screenshots, built on macroquad. A game is
  a class implementing `Game { update(input, dt); draw(screen) }`, started with
  `Window.run(...)`. Drawing 10,000 sprites takes 0.73 ms per frame, against 0.63 ms for the
  same program in plain Rust.
- **`std/math2d`**: `Vec2` and `Rect` structs with operators and geometry helpers.
- Examples: `examples/pong/` and `examples/asteroids/`, with Pong's rules unit-tested.

### Language
- **Error handling**: `Result<T, E>` with `Ok` / `Err`, exhaustive `match`, postfix `?`
  (also on optionals), `is_ok`, `is_err`, `ok`, `err`, `unwrap_or`, `unwrap`, and
  `fn main() -> Result<void, E>`.
- **Threads** (share-nothing): `Thread.spawn(fn() => ...)` → `Task<T>` with `join()` and
  `is_done()`, `Channel<T>`, `Shared<T>`, `xs.par_map(f)` (5.4x faster on 12 cores) and
  `Thread.cores()`. Only values cross threads; using an object in a thread is a compile
  error, so there are no data races and single-threaded code is unchanged.
- **Async**: `async fn` and `static async fn`, `await`, `async fn main()`, and a built-in
  executor with `Time.wait`, `Async.spawn`, `Async.all`, `Async.run`, and `await` on thread
  tasks.
- **Generic structs** (`struct Grid<T>`), struct constants (`static ORIGIN: Vec2 = ...`) and
  numbers on the left of struct operators (`2.0 * v`).
- **Package namespaces**: `import "pkg" as alias` with `alias.Name` in code and types;
  packages may reuse names, and error messages show `pkg.Name`.

### Rust interop
- Inline Rust bodies: `extern fn roll(n: int) -> int = rust { fastrand::i64(1..=n) }`.
- `extern fn` can return `Result`.
- Crate features: `use crate "macroquad" = "0.4" features ["audio"]`; compatible versions
  of the same crate are merged.
- Windows builds link the C runtime statically, so executables run without the Visual C++
  runtime installed.

### utkal, the package manager
- New `utkal` command: `new [--lib]`, `add`, `remove`, `build`, `run`, `check`, `test`,
  `update`.
- `utkal.toml` with `[dependencies]` from git (`tag`, `branch`, `rev` or a semver
  `version` matched against tags) or a `path`, and `[rust-dependencies]` that need no
  `use crate` lines.
- `utkal.lock` pins every git package to a commit; packages are cached in `~/.utkal`.
- `kalinga` and the editor find the nearest `utkal.toml`, so packages work everywhere.

### Tooling
- VS Code extension 0.5.0: highlighting for `async`/`await`, `Ok`/`Err` and inline Rust
  bodies (and a fix for patterns that never matched, such as `fn` names); completion and
  signature help for `Thread`, `Async`, `Result`, `Channel`, `Shared`, `Task`, `std/`
  modules, packages and `alias.` names; new snippets (`resfn`, `matchr`, `externr`, `spawn`,
  `channel`, `asyncfn`, `importas`, `gameclass`); `utkal.toml` hints.
- Docs: new chapters Error Handling, Packages with utkal, Threads, Async and Making a Game;
  the rest renumbered.

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
