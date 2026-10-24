# 24. Tooling

## The kalinga command

| Command | What it does |
|---|---|
| `kalinga run <file.kl>` | Compile and run immediately. Exits with the program's exit code. |
| `kalinga test <file.kl>` | Run the file's `test fn`s (see [Testing](#testing)). Exits with 1 when one fails. |
| `kalinga build <file.kl>` | Compile to an executable next to the source (`file.exe` on Windows). |
| `kalinga build <file.kl> -o <name>` | Compile to the given output path. |
| `kalinga emit <file.kl>` | Print the generated Rust code. |
| `kalinga check <file.kl>` | Report errors only; does not invoke `rustc`. `fn main()` is optional here, so files that are only imported can be checked too. |
| `kalinga -v` (also `-version`, `--version`) | Print the installed version, e.g. `kalinga 0.5.1`. |
| `kalinga check <name.kl> --stdin` | Check source read from stdin, reporting errors under `<name.kl>`. |
| `kalinga fix [--from <version>] <file.kl or folder>...` | Update code written for an earlier version of Kalinga, in place. The code's version comes from the nearest `utkal.toml` (`kalinga = "0.4"`) or `--from 0.4`; changes that only suit older code are skipped when it is unknown, with a note saying so. |
| `kalinga fmt [--check] <file.kl or folder>...` | Lay out code in the standard style, in place (see [Formatting](#formatting)). With `--check`, list the files that are not formatted and fail, changing nothing. |
| `kalinga lsp` | Run the language server on stdin / stdout; editors start it (see below). |

Exit codes:

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Compile error (or file not found) |
| `2` | Bad command-line usage |
| `101` | The program stopped with a runtime error |

### Examples

```bash
kalinga run examples/oop.kl
```

```bash
kalinga build examples/fib.kl -o fib.exe
```

```bash
kalinga emit examples/hello.kl > hello.rs
```

### Where files go

`run` and `build` write the generated Rust file and (for `run`) the executable to a
`kalinga-build` folder in your system's temporary directory. If the Rust step ever fails,
the error message includes the path of the generated `.rs` file so it can be inspected.

`rustc` must be on your PATH for `run` and `build`. `check` and `emit` work without it.

Programs with `use crate` (or `[rust-dependencies]`, or `import "std/game"`) are built with `cargo` in a project under
`kalinga-build/cargo/<name>`; compiled crates are kept in `kalinga-build/cargo-target`
and shared by all programs, so only the first build that uses a crate is slow.

Inside a utkal project, `kalinga` finds the nearest `utkal.toml` above the file and uses
its packages (as downloaded by `utkal build`), so `kalinga run src/main.kl` and editor
checks work there too.

## The utkal command

`utkal` manages projects and packages; see [18. Packages with utkal](18-packages.md).

| Command | What it does |
|---|---|
| `utkal new <name> [--lib]` | Create a project: an app (`src/main.kl`) or a library (`src/lib.kl`, `tests/`) |
| `utkal add <name> --git <url> [--tag t \| --branch b \| --rev c \| --version req]` | Add a git package |
| `utkal add <name> --path <folder>` | Add a package from a folder |
| `utkal add <crate> --rust <version> [--features a,b]` | Add a crates.io crate to `[rust-dependencies]` |
| `utkal remove <name>` | Remove a package or crate |
| `utkal build` | Download what is missing, compile `src/main.kl` to `target/<name>` |
| `utkal run [args...]` | Build and run, passing the arguments on |
| `utkal check` | Report errors in `src/main.kl` and `src/lib.kl` |
| `utkal fmt [--check]` | Format (or check) every `.kl` file in `src/` and `tests/` |
| `utkal fix` | Update `src/` and `tests/` for this Kalinga version |
| `utkal test` | Run the `test fn`s in `src/` and `tests/`, and every other `tests/*.kl` program (passes with exit code 0) |
| `utkal update` | Pick the newest allowed package versions and rewrite `utkal.lock` |
| `utkal -v` | Print the installed version |

Downloaded packages are cached in `~/.utkal/cache` (or `$UTKAL_HOME/cache`). `utkal` needs
`git` on PATH for git packages.

## Formatting

`kalinga fmt` lays out code in one standard style, so every Kalinga program reads the same
and nobody argues about spaces:

- 4 spaces of indentation per level; the lines inside brackets are indented one level,
  however many brackets a line opens; a line continuing an expression (after an
  operator, or starting with `.`) one more
- spaces around operators and after `,` and `:`; none inside `( )` and `[ ]`, before `(`
  in a call, around `..` or after a unary `-` / `!`: `f(a + b, -x)`, `xs[i]`, `0..n`,
  `Result<int, str>`, `{"key": 1}`; one-line blocks get spaces: `{ return 1 }`
- one statement per line (`a; b` becomes two lines)
- at most one blank line in a row, none just inside braces, and one after a top-level
  declaration that takes several lines
- trailing `//` comments on consecutive lines start in the same column

It changes only spaces and line breaks: comments, the blank lines you put between
groups, parentheses and how literals are written all stay, and it checks that the
program's tokens are unchanged. Formatting twice gives the same result. Before:

```text
fn area(w:float,h :float)->float{
return w*h   // square units
}
fn main(){ println( area(2,3) ) }
```

After `kalinga fmt`:

```kalinga
fn area(w: float, h: float) -> float {
    return w * h // square units
}

fn main() { println(area(2, 3)) }
```

In VS Code, **Format Document** (`Shift+Alt+F`) does the same. In CI, `kalinga fmt --check`
(or `utkal fmt --check`) fails when a file is not formatted.

## Testing

A `test fn` checks that code does what it should. It takes no parameters and returns
nothing; `assert(condition)` and `assert_eq(value, expected)` say what must hold:

```kalinga
fn clamp(value: int, low: int, high: int) -> int {
    if value < low {
        return low
    }
    return if value > high { high } else { value }
}

test fn keeps_values_in_range() {
    assert_eq(clamp(5, 0, 10), 5)
    assert_eq(clamp(-3, 0, 10), 0)
}

test fn the_upper_bound() {
    assert(clamp(99, 0, 10) == 10, "values above the range become the upper bound")
    assert_eq(clamp(11, 0, 10), 11)
}
```

`kalinga test clamp.kl` runs every `test fn` in the file and reports each one:

```text
running 2 tests
test keeps_values_in_range ... ok
test the_upper_bound ... FAILED

failures:
    the_upper_bound: assert_eq failed at main.kl:15: got 10, expected 11

test result: FAILED. 1 passed; 1 failed
```

- A test fails at the first `assert` or `assert_eq` that does not hold, or at a runtime
  error. `assert(condition, message)` adds `message`; `assert_eq` shows both values, which
  must be printable and comparable (not objects).
- Each test runs on its own thread, so one test's failure or its changes to `static var`s
  do not affect the others.
- `fn main()` is not run, and is not needed: a file can hold just code and its tests. The
  `test fn`s of imported files are run only by testing those files.
- The exit code is 0 when every test passed, 1 otherwise, so `kalinga test` works in CI.
- `utkal test` runs the tests of every file in `src/` and `tests/`; in VS Code,
  **Kalinga: Run Tests in File**.
- `assert` and `assert_eq` work in any program too: a failure stops it with a runtime error
  that says where.

## VS Code extension

The extension lives in [`editors/vscode`](../editors/vscode/README.md).

### Features

The extension starts the Kalinga language server (`kalinga lsp`). The server is the compiler
itself, so the editor sees exactly what the compiler sees, and any editor that speaks the
Language Server Protocol (Neovim, Helix, Zed, Emacs, ...) can use it by running
`kalinga lsp`.

- Errors as you type, all of them at once: every broken class rule and every type error,
  not only the first; errors inside imported files appear on the `import` line
- Completion: members after `.` (inherited ones too; `private` ones only inside their
  type), enum variants after `Enum.`, `static fn`s, constants and static variables after
  `Type.`, library functions after `Math.` and the others, variables in scope (including
  lambda parameters and `match` bindings), functions, constructors with their arguments,
  types after `:`, `->` and `<`, `match` arm patterns, `override fn` stubs in a class
  body, `alias.` after `import "pkg" as alias`, and standard modules and packages inside
  `import "`. `///` doc comments above a declaration appear as its documentation.
- Hover over a name to see its type or signature and its `///` documentation
- Parameter hints while typing the arguments of any call
- Go to definition, find references, and rename (renaming a method also renames the
  methods it overrides or implements, so the program still compiles)
- The outline of a file (document symbols)
- Quick fixes: mark a method `override fn`, remove a wrong `override`, add a method an
  interface or abstract base class requires
- **Format Document** with `kalinga fmt`
- Syntax highlighting; inline `= rust { ... }` bodies are highlighted as Rust
- ▶ **Run** button in the editor title bar, and `Ctrl+F5`
- Commands (Ctrl+Shift+P): **Kalinga: Run File**, **Kalinga: Run Tests in File**,
  **Kalinga: Build Executable**,
  **Kalinga: Show Generated Rust**, **Kalinga: Restart Language Server**
- Comment toggling (`Ctrl+/`), bracket matching, auto-closing pairs, auto-indent
- Hints for `utkal.toml` keys and values (with a TOML extension that reads JSON schemas,
  such as Even Better TOML)

### Snippets

Type the prefix and press Tab:

| Prefix | Inserts |
|---|---|
| `main` | `fn main() { }` |
| `fn` | function with a parameter and return type |
| `class` | class with a field and `init` |
| `classx` | subclass with `super(...)` and an override |
| `interface` | interface with one method |
| `init` | constructor |
| `meth` | method |
| `override` | overriding method |
| `if`, `ife`, `iflet` | `if`, `if/else`, `if let` |
| `while`, `for`, `forr` | loops (`forr` = range loop) |
| `pr` | `print(...)` |
| `enum`, `match` | enum declaration, match statement |
| `lam`, `lamb` | short lambda, block lambda |
| `aclass`, `gclass`, `sfn` | abstract class, generic class, static method |
| `ifas` | `if let d = x as Class { }` |
| `struct`, `mfn`, `op` | struct, `mut fn`, operator |
| `userust`, `usecrate`, `extern` | Rust interop declarations |
| `gameloop` | a frame loop with delta time |
| `import`, `importas` | `import "file.kl"`, `import "package" as name` |
| `resfn`, `matchr` | a function returning `Result`, a `match` on both cases |
| `externr` | an `extern fn` with an inline Rust body |
| `spawn`, `channel` | a thread and its `join()`; a channel fed by a thread |
| `asyncfn` | an `async fn` with an `await` |
| `gameclass` | a complete `std/game` program |

### Installing

```bash
cargo install --path .
```

```bash
cd editors/vscode
npm install
npm run package
code --install-extension kalinga-lang-0.6.0.vsix
```

### Settings

| Setting | Default | Meaning |
|---|---|---|
| `kalinga.compilerPath` | `kalinga` | Path to the compiler, if it is not on PATH; it also runs the language server |

### File icon

`.kl` files get a 24-spoke wheel (after the Ashoka Chakra) with file icon themes that use a
language's own icon, such as Seti, VS Code's default. Themes that draw their own icon for
every file, such as **Material Icon Theme**, need to be told about it:

1. Copy the extension's icon to a folder of your own under VS Code's extensions folder,
   for example (Windows) from
   `%USERPROFILE%\.vscode\extensions\kalinga.kalinga-lang-<version>\icons\kalinga-dark.svg`
   to `%USERPROFILE%\.vscode\extensions\icons\kalinga.svg` (`kalinga-light.svg` suits
   light themes).
2. Add to your settings (`settings.json`):

   ```json
   "material-icon-theme.files.associations": { "*.kl": "../../icons/kalinga" }
   ```

   The path is relative to the theme's own folder, without `.svg`.

## Running the compiler's tests

From the repository root:

```bash
cargo test
```

This runs:

- **snapshot tests**: every `tests/ui/<group>/<name>.kl` is run (or checked, when its first
  line is `// ui: check`, or tested with `kalinga test` for `// ui: test`), and its output
  must match `<name>.stdout`, `<name>.stderr` and
  `<name>.exit` next to it. After an intended change, `BLESS=1 cargo test --test ui`
  rewrites the snapshots; review them with `git diff` before committing;
- **documentation tests**: every complete example in these docs must compile and print
  exactly the output shown after it;
- **command-line and utkal tests**, including local packages and a local git repository;
- **language server tests**: completion, hover, signature help, navigation, rename and quick
  fixes on example programs (`cargo test -p kalinga-lsp`), and the protocol over stdio.

`cargo test -- --ignored` also runs the tests that download crates or open a window. The
same checks, plus `cargo fmt --check` and `cargo clippy -- -D warnings`, run on Windows,
Linux and macOS for every push (`.github/workflows/ci.yml`).

## Making a release

1. Set the version in `Cargo.toml` (`[workspace.package] version`) and write its section in
   `CHANGELOG.md` (`## X.Y.Z — name`).
2. After that change is merged into `main`, tag it and push the tag:

   ```bash
   git tag -a vX.Y.Z -m "Kalinga X.Y.Z"
   ```

   ```bash
   git push origin vX.Y.Z
   ```

3. `.github/workflows/release.yml` builds `kalinga` and `utkal` for Windows, Linux and macOS
   (Intel and Apple silicon) and the `.vsix`, and publishes them as a GitHub release with
   the CHANGELOG section as its notes and a `SHA256SUMS` file. It refuses a tag that does
   not match `Cargo.toml`. Running the workflow by hand (Actions → Release → Run workflow),
   or a pull request that changes it, builds everything without publishing.

---

Previous: [23. Safety and Errors](23-safety-and-errors.md) · Next: [25. How It Works](25-how-it-works.md)
