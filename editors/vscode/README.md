# Kalinga for VS Code

Language support for Kalinga `.kl` files, a simple object-oriented language that compiles to Rust.

- Syntax highlighting: keywords, classes, interfaces, types, functions, strings and numbers
- Code completion:
  - members after `.`: fields and methods (including inherited ones and super-interfaces),
    string, array and map methods, generic classes with their type arguments filled in
  - `private` members only inside their own class
  - enum variants after `Enum.`, static methods after `Class.`
  - variables (including lambda parameters and `match` bindings), functions, classes with
    constructor arguments, enums and built-ins; inside `{...}` string interpolation too
  - types after `:`, `->` and `<`; bases after `class Name :` / `interface Name :`
  - in a `match`, the enum's variants as arm patterns
  - inside a class body, `override fn` stubs (abstract methods first), `private`, `static fn`
  - declarations from `import`ed files
  - structs (fields, methods, `static fn`), `extern fn` / `extern type` declarations, and the
    built-in `Math`, `Random` and `Time` libraries
  - documentation from `//` comments written above a declaration
- Signature help: parameter hints for functions, generic functions, constructors, methods,
  static methods, enum variants and function-typed variables
- Live error checking while you type, using the compiler's own checker (`kalinga check`);
  errors inside imported files are shown on the `import` line
- Snippets: `main`, `fn`, `class`, `classx` (subclass), `aclass` (abstract), `gclass` (generic),
  `interface`, `enum`, `match`, `init`, `meth`, `sfn` (static), `override`, `lam` / `lamb` (lambdas),
  `import`, `if`, `ife`, `iflet`, `ifas` (downcast), `while`, `for`, `forr`, `pr`,
  `struct`, `mfn` (`mut fn`), `op` (operator), `userust`, `usecrate`, `extern`, `gameloop`
- Commands:
  - **Kalinga: Run File**: the ▶ button in the editor title, or `Ctrl+F5`
  - **Kalinga: Build Executable**
  - **Kalinga: Show Generated Rust**
- Comment toggling, bracket matching and auto-indent

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
     npx @vscode/vsce package
     ```

     ```bash
     code --install-extension kalinga-lang-0.4.0.vsix
     ```

   - **Or try it without installing**: open `editors/vscode` in VS Code and press `F5`
     to launch an Extension Development Host with the extension loaded.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `kalinga.compilerPath` | `kalinga` | Compiler executable |
| `kalinga.checkOnType` | `true` | Check while typing; when off, files are checked on open and save |

Running a program needs `rustc` on PATH, because Kalinga compiles through Rust.

## Development

The completion engine is in `completion.js` and has no VS Code dependency. Run its tests
from this folder:

```bash
node --test
```
