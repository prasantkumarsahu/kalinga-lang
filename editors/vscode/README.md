# Kalinga for VS Code

Language support for Kalinga `.kl` files, a simple object-oriented language that compiles to Rust.

- Syntax highlighting: keywords, classes, interfaces, types, functions, strings and numbers
- Live error checking while you type, using the compiler's own checker (`kalinga check`)
- Snippets: `main`, `fn`, `class`, `classx` (subclass), `interface`, `init`, `meth`, `override`, `if`, `ife`, `iflet`, `while`, `for`, `forr`, `pr`
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
     code --install-extension kalinga-lang-0.1.0.vsix
     ```

   - **Or try it without installing**: open `editors/vscode` in VS Code and press `F5`
     to launch an Extension Development Host with the extension loaded.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `kalinga.compilerPath` | `kalinga` | Compiler executable |
| `kalinga.checkOnType` | `true` | Check while typing; when off, files are checked on open and save |

Running a program needs `rustc` on PATH, because Kalinga compiles through Rust.
