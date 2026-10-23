# 24. Tooling

## The kalinga command

| Command | What it does |
|---|---|
| `kalinga run <file.kl>` | Compile and run immediately. Exits with the program's exit code. |
| `kalinga build <file.kl>` | Compile to an executable next to the source (`file.exe` on Windows). |
| `kalinga build <file.kl> -o <name>` | Compile to the given output path. |
| `kalinga emit <file.kl>` | Print the generated Rust code. |
| `kalinga check <file.kl>` | Report errors only; does not invoke `rustc`. `fn main()` is optional here, so files that are only imported can be checked too. |
| `kalinga -v` (also `-version`, `--version`) | Print the installed version, e.g. `kalinga 0.4.0`. |
| `kalinga check <name.kl> --stdin` | Check source read from stdin, reporting errors under `<name.kl>`. Used by editors for unsaved files. |

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
| `utkal test` | Build and run every `tests/*.kl`; exit code 0 means passed |
| `utkal update` | Pick the newest allowed package versions and rewrite `utkal.lock` |
| `utkal -v` | Print the installed version |

Downloaded packages are cached in `~/.utkal/cache` (or `$UTKAL_HOME/cache`). `utkal` needs
`git` on PATH for git packages.

## VS Code extension

The extension lives in [`editors/vscode`](../editors/vscode/README.md).

### Features

- Syntax highlighting for keywords, types, classes, functions, strings, numbers and
  comments; inline `= rust { ... }` bodies are highlighted as Rust
- Code completion: members after `.` (inherited ones too; `private` ones only inside their
  class), enum variants after `Enum.`, static methods after `Class.`, map/array/string
  methods, variables (including lambda parameters and `match` bindings), types after `:`,
  `->` and `<`, `match` arm patterns, `override fn` stubs in a class body, and declarations
  from imported files, `std/` modules and utkal packages (`alias.` after
  `import "pkg" as alias` too). Built-ins: `Math`, `Random`, `Time`, `Thread`, `Async`, and
  the methods of `Result`, `Channel`, `Shared` and `Task`. `///` doc comments above a
  declaration appear as its documentation.
- Hover over a name to see its type or signature and its `///` documentation
  (variables, functions, classes, structs, fields, methods, enum variants, library
  functions and package names)
- Parameter hints while typing the arguments of a call (functions, generic functions,
  constructors, methods, static methods, enum variants and function-typed variables)
- Errors underlined as you type (runs `kalinga check --stdin` about 0.4 s after you stop);
  errors inside imported files appear on the `import` line
- ▶ **Run** button in the editor title bar, and `Ctrl+F5`
- Commands (Ctrl+Shift+P): **Kalinga: Run File**, **Kalinga: Build Executable**,
  **Kalinga: Show Generated Rust**
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
npx @vscode/vsce package
code --install-extension kalinga-lang-0.5.0.vsix
```

### Settings

| Setting | Default | Meaning |
|---|---|---|
| `kalinga.compilerPath` | `kalinga` | Path to the compiler, if it is not on PATH |
| `kalinga.checkOnType` | `true` | When `false`, check only on open and save |

## Running the compiler's tests

From the repository root:

```bash
cargo test
```

This runs the example programs, checks the expected error messages, compiles every
complete example in these docs, and exercises `utkal` with local packages and a local git
repository. `cargo test -- --ignored` also runs the tests that download crates or open a
window.

---

Previous: [23. Safety and Errors](23-safety-and-errors.md) · Next: [25. How It Works](25-how-it-works.md)
