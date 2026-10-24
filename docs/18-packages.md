# 18. Packages with utkal

`utkal` is Kalinga's package manager. It creates projects, downloads the Kalinga packages a
project uses (from git repositories or folders), pins their versions, and builds, runs and
tests the project. It is installed next to `kalinga` (see [24. Tooling](24-tooling.md)).

## A new project

```bash
utkal new space-game
cd space-game
utkal run
```

```text
Hello from Kalinga!
```

`utkal new` creates:

```text
space-game/
├── utkal.toml       the project's manifest
├── .gitignore       ignores target/
└── src/
    └── main.kl      fn main() { ... }
```

`utkal new mylib --lib` creates a library instead: `src/lib.kl` (what other projects get when
they import it) and `tests/greet.kl`.

## utkal.toml

```toml
[package]
name = "space-game"
version = "0.1.0"
kalinga = "0.4"

[dependencies]
mathx = { git = "https://github.com/someone/mathx", version = "0.4" }
shared = { path = "../shared" }

[rust-dependencies]
fastrand = "2"
macroquad = { version = "0.4", features = ["audio"] }
```

| Section | Meaning |
|---|---|
| `[package]` | the project's name (letters, digits, `-`, `_`) and version |
| `[dependencies]` | Kalinga packages, from a `git` repository or a `path` on disk |
| `[rust-dependencies]` | crates.io crates for the project's Rust code; no `use crate` lines needed |

A git dependency can say which commit to use in one of four ways:

| Key | Picks |
|---|---|
| `version = "0.4"` | the newest tag matching the requirement: `v0.4.0`…`v0.4.x` (Cargo's rules: `"1.2"` = `^1.2`, `"~1.2.3"`, `"=1.0.0"`, `">=0.3"`, `"*"`) |
| `tag = "v0.4.2"` | exactly that tag |
| `branch = "main"` | the newest commit on the branch |
| `rev = "a1b2c3d"` | exactly that commit |
| (none) | the repository's default branch |

Tags look like `v1.2.3` or `1.2.3`.

## Adding and removing packages

`utkal add` edits `utkal.toml` and downloads the package right away (a mistake is reported
and the edit undone):

```bash
utkal add mathx --git https://github.com/someone/mathx --version 0.4
utkal add shared --path ../shared
utkal add fastrand --rust 2
utkal add macroquad --rust 0.4 --features audio
utkal remove shared
```

## Using a package

`import` with the package's name (no `.kl`) loads its `src/lib.kl`; `"name/file"` loads
`src/file.kl`:

```kalinga
import "mathx"
import "mathx/noise"

fn main() {
    let v = Vec2(3.0, 4.0)
    println(v.length(), perlin(0.5, 0.5))
}
```

Paths ending in `.kl` are still files next to the importing file, as in
[17. Modules](17-modules.md).

## Namespaces: `import ... as`

Each package has its own names. Two packages may both declare a `Vec2`, and so may your
project. To use a package's names behind a prefix, import it `as` a name:

```kalinga
import "mathx" as mx
import "physics" as phys

struct Vec2 {
    x: int
    y: int
}

fn main() {
    let a = mx.Vec2(1.0, 2.0)
    let b: mx.Vec2 = mx.Vec2(3.0, 4.0)
    let body = phys.Body(a)
    println(a.dot(b), mx.version(), Vec2(1, 2))
}
```

- `alias.Name` works for functions, classes, structs, enums and interfaces, in code and in
  types (`let b: mx.Vec2`).
- A plain `import "mathx"` brings the names in directly. Inside a file, a name is looked up
  in its own package (or project) first, then in the packages it imports.
- When two plainly imported packages both have the name you use, the compiler asks you to
  pick one:

```text
main.kl:5:13: error: `Vec2` is defined in both `mathx` and `physics`; use `import "..." as name` and write `name.Vec2`
```

- Messages name a package's types as `package.Name`:

```text
main.kl:4:20: error: expected `Point`, found `pixels.Point`
```

Files of your own project (imported with `.kl` paths) share one set of names, as before.

## Versions and utkal.lock

The first build picks a commit for every git package and writes it to `utkal.lock`:

```toml
[[package]]
name = "mathx"
source = "git https://github.com/someone/mathx version 0.4"
commit = "dedceb4f1e2302f9d6024003663629a0654a07c1"
```

Later builds use exactly those commits, so the project builds the same way on every
machine and next year. Commit `utkal.lock` together with `utkal.toml`.

```bash
utkal update      # pick the newest allowed versions and rewrite utkal.lock
```

Packages can depend on other packages. Each package is used in **one** version per
project; if two packages ask for incompatible versions, utkal names both:

```text
package `mathx` is required as `git https://github.com/someone/mathx version 0.2` by `app` but as `git https://github.com/someone/mathx tag v0.1.0` by `helper`; make them agree
```

Downloaded packages are kept in `~/.utkal/cache` (set `UTKAL_HOME` to move it), one copy per
commit, shared by all projects.

## Building, running and testing

| Command | Does |
|---|---|
| `utkal build` | downloads what is missing, compiles `src/main.kl` to `target/<name>` (`.exe` on Windows) |
| `utkal run [args...]` | builds, then runs with the arguments |
| `utkal check` | reports errors in `src/main.kl` / `src/lib.kl` |
| `utkal test` | runs the `test fn`s in `src/` and `tests/` (see [Testing](24-tooling.md#testing)), and every other `tests/*.kl` program, which passes when it exits with code 0 |
| `utkal update` | refreshes `utkal.lock` |

`utkal new --lib` starts `tests/greet.kl` with a test:

```kalinga
import "../src/lib.kl"

test fn greets_by_name() {
    assert_eq(greet("utkal"), "Hello, utkal!")
}
```

```text
tests/greet.kl
running 1 test
test greets_by_name ... ok

test result: ok. 1 passed; 0 failed

1 file(s) passed, 0 failed
```

`kalinga run`, `kalinga check` and the VS Code extension also find the nearest
`utkal.toml` above a file, so packages work there too once `utkal build` has downloaded
them.

## Rust dependencies

Crates in `[rust-dependencies]` are available to the project's Rust code (`use rust` files
and `= rust { ... }` bodies) without a `use crate` line:

```kalinga
extern fn roll() -> int = rust { fastrand::i64(1..=6) }

fn main() {
    println(roll())
}
```

The crates of the project and of all its packages, plus any `use crate` lines, are merged:
compatible versions (`"0.4"` and `"0.4.16"`) become the higher one and features are
combined; incompatible versions (`"0.3"` and `"0.4"`) are an error.

## Publishing a package

There is no central registry yet: a package is a git repository with a `utkal.toml` and a
`src/lib.kl`. Push it, tag releases as `v0.1.0`, `v0.2.0`, ..., and others add it with
`utkal add name --git <url> --version 0.1`.

---

Previous: [17. Modules](17-modules.md) · Next: [19. Rust Interop](19-rust-interop.md)
