# Kalinga for VS Code

Language support for Kalinga `.kl` files, a simple object-oriented language that compiles to Rust.

The language features come from the Kalinga language server (`kalinga lsp`), which is the
compiler itself, so what the editor shows is exactly what the compiler sees:

- Errors as you type, in every file at once (all broken class rules and type errors, not only the
  first); errors inside imported files are shown on the `import` line
- Completion:
  - members after `.`: fields and methods (including inherited ones and super-interfaces),
    string, array, map, `Result`, `Channel`, `Shared` and `Task` methods, generic classes
    with their type arguments filled in; `private` members only inside their own type
  - enum variants after `Enum.`; `static fn`s, `const`s and `static var`s after `Type.`;
    the functions of `Math`, `Random`, `Time`, `Thread` and `Async`
  - variables in scope (including lambda parameters and `match` bindings), functions,
    classes and structs with constructor arguments, constants and built-ins; inside `{...}`
    string interpolation too
  - types after `:`, `->` and `<`; bases after `class Name :`
  - in a `match`, the patterns of the matched value
  - inside a class body, `override fn` stubs (required ones first) and member keywords
  - `import "pkg" as name`: the package's declarations after `name.`; `std/...` modules and
    the project's packages inside `import "`
- Hover: the type of a variable, or the signature of a function, method, class, struct,
  field, enum variant, constant or library function, with its `///` documentation (Markdown)
- Signature help for every kind of call, with the current argument highlighted
- Go to definition, find references and rename (renaming a method also renames the methods
  it overrides or implements, so the program keeps compiling)
- Outline (document symbols)
- Quick fixes: mark a method `override fn`, remove a wrong `override`, add a missing
  interface or abstract method
- Syntax highlighting, snippets (`main`, `fn`, `class`, `struct`, `match`, `gameclass`, ...),
  comment toggling, bracket matching and auto-indent
- `utkal.toml`: key and value hints, with an extension that reads JSON schemas for TOML
  (such as Even Better TOML)
- Commands:
  - **Kalinga: Run File**: the ▶ button in the editor title, or `Ctrl+F5`
  - **Kalinga: Build Executable**
  - **Kalinga: Show Generated Rust**
  - **Kalinga: Restart Language Server**

## Setup

1. Install the compiler (from the repository root):

   ```bash
   cargo install --path .
   ```

   This puts `kalinga` on your PATH. To skip that, set `kalinga.compilerPath` to the full
   path of `target/release/kalinga.exe` (or `target/debug/kalinga.exe`) instead.

2. Install the extension. Either:
   - **Package it** (needs Node.js), from `editors/vscode`:

     ```bash
     npm install
     ```

     ```bash
     npm run package
     ```

     ```bash
     code --install-extension kalinga-lang-0.6.0.vsix
     ```

   - **Or try it without installing**: open `editors/vscode` in VS Code and press `F5`
     to launch an Extension Development Host with the extension loaded.

## Settings

| Setting                | Default   | Meaning                                           |
| ---------------------- | --------- | ------------------------------------------------- |
| `kalinga.compilerPath` | `kalinga` | The compiler, which also runs the language server |

Running a program needs `rustc` on PATH, because Kalinga compiles through Rust.

## Development

The extension only starts the language server and adds commands; the language features are
in the `kalinga-lsp` crate and tested there (`cargo test -p kalinga-lsp`). Here:

```bash
npm run lint
```

```bash
npm run format:check
```
