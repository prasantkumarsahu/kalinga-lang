# 11. Tooling

## The kalinga command

| Command | What it does |
|---|---|
| `kalinga run <file.kl>` | Compile and run immediately. Exits with the program's exit code. |
| `kalinga build <file.kl>` | Compile to an executable next to the source (`file.exe` on Windows). |
| `kalinga build <file.kl> -o <name>` | Compile to the given output path. |
| `kalinga emit <file.kl>` | Print the generated Rust code. |
| `kalinga check <file.kl>` | Report errors only; does not invoke `rustc`. |
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

## VS Code extension

The extension lives in [`editors/vscode`](../editors/vscode/README.md).

### Features

- Syntax highlighting for keywords, types, classes, functions, strings, numbers and comments
- Errors underlined as you type (runs `kalinga check --stdin` about 0.4 s after you stop)
- ▶ **Run** button in the editor title bar, and `Ctrl+F5`
- Commands (Ctrl+Shift+P): **Kalinga: Run File**, **Kalinga: Build Executable**,
  **Kalinga: Show Generated Rust**
- Comment toggling (`Ctrl+/`), bracket matching, auto-closing pairs, auto-indent

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

### Installing

```bash
cargo install --path .
```

```bash
cd editors/vscode
npx @vscode/vsce package
code --install-extension kalinga-lang-0.1.0.vsix
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

This runs the example programs, checks the expected error messages, and compiles every
complete example in these docs.

---

Previous: [10. Safety and Errors](10-safety-and-errors.md) · Next: [12. How It Works](12-how-it-works.md)
