# Changelog

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
