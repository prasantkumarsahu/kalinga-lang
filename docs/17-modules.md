# 17. Modules

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
    println(describe(Square(3.0)))
}
```

```bash
kalinga run main.kl
```

```text
a square with area 9.0
```

- `import "path"` goes at the top level, usually at the top of the file.
- A path ending in `.kl` is a file, relative to the file that contains the `import`.
  Folders work: `import "lib/geometry.kl"`.
- Everything declared at the top level of an imported file — functions, classes,
  interfaces, enums — can be used directly.
- Imported files can import other files.

## How imports are combined

- Each file is loaded **once**, however many files import it. Two files may even import
  each other.
- The files of one program share one set of names: two top-level declarations with the
  same name are an error. (Packages are different: each has its own names, see
  [18. Packages with utkal](18-packages.md).)
- `fn main()` must exist exactly once, normally in the file you run.
- `private` still works per class, not per file.

## Standard modules

Paths starting with `std/` are modules that come with Kalinga, built into the `kalinga`
executable:

| Import | Contents |
|---|---|
| `import "std/math2d"` | `Vec2` and `Rect` structs with operators and geometry helpers |
| `import "std/game"` | a window, drawing, input, images and sounds (includes `std/math2d`); see [22. Making a Game](22-making-a-game.md) |

```kalinga
import "std/math2d"

fn main() {
    let a = Vec2(3.0, 4.0)
    println(a.length(), a + Vec2(1.0, 1.0), Rect(0.0, 0.0, 10.0, 10.0).contains(a))
}
```

```text
5.0 Vec2(x: 4.0, y: 5.0) true
```

An import without `.kl` or `std/` names a package: `import "mathx"`. Packages are added to a
project with utkal (chapter 18).

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

Previous: [16. Error Handling](16-error-handling.md) · Next: [18. Packages with utkal](18-packages.md)
