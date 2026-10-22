# 16. Modules

A program can be split across several files with `import`.

## Importing a file

`geometry.kl`:

```kalinga
// Shapes and helpers used by main.kl.
class Square {
    side: float

    init(side: float) {
        self.side = side
    }

    fn area() -> float {
        return self.side * self.side
    }
}

fn describe(s: Square) -> str {
    return "a square with area {s.area()}"
}
```

`main.kl`:

```kalinga
import "geometry.kl"

fn main() {
    print(describe(Square(3.0)))
}
```

```bash
kalinga run main.kl
```

```text
a square with area 9.0
```

- `import "path"` goes at the top level, usually at the top of the file.
- The path is relative to the file that contains the `import`. Folders work:
  `import "lib/geometry.kl"`.
- Everything declared at the top level of an imported file — functions, classes,
  interfaces, enums — can be used directly.
- Imported files can import other files.

## How imports are combined

- Each file is loaded **once**, however many files import it. Two files may even import
  each other.
- All files share one set of names. Two top-level declarations with the same name are an
  error, wherever they are.
- `fn main()` must exist exactly once, normally in the file you run.
- `private` still works per class, not per file.

## Errors in imported files

Errors point into the file where they are:

```text
lib/geometry.kl:12:20: error: expected `float`, found `str`
12 |     return self.side * "2"
   |                    ^
```

A missing file is reported at the `import`:

```text
main.kl:1:1: error: cannot find `geometri.kl` (looked for geometri.kl)
```

In VS Code, an error inside an imported file is shown on the matching `import` line of the
file you are editing, and completion offers the declarations from imported files.

## Organizing a project

A simple layout that works well:

```text
my-app/
├── main.kl          imports "models/user.kl" and "util.kl"
├── util.kl
└── models/
    └── user.kl      can import "../util.kl"
```

Run or build the file with `fn main()`:

```bash
kalinga build main.kl -o my-app.exe
```

The result is still one self-contained executable.

---

Previous: [15. Standard Library](15-standard-library.md) · Next: [17. Rust Interop](17-rust-interop.md)
