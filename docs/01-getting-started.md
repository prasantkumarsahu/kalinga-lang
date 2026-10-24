# 1. Getting Started

## What you need

Kalinga compiles your program to Rust and then uses the Rust compiler to make a native
executable, so you need a Rust toolchain:

1. Install Rust from <https://rustup.rs>. This gives you `cargo` and `rustc`.
2. Install the Kalinga compiler and the `utkal` package manager. Either:

   - **Download a release**: from the repository's
     [Releases](https://github.com/prasantkumarsahu/kalinga-lang/releases) page, take the
     archive for your system (`kalinga-0.5.1-x86_64-pc-windows-msvc.zip`,
     `…-x86_64-unknown-linux-gnu.tar.gz`, `…-aarch64-apple-darwin.tar.gz` or
     `…-x86_64-apple-darwin.tar.gz`), unpack it and put the folder with `kalinga` and `utkal`
     on your PATH.
   - **Or build them** from the root of the `kalinga-lang` repository:

     ```bash
     cargo install --path .
     ```

3. Check that it works:

   ```bash
   kalinga -v
   ```

   It prints the installed version, for example `kalinga 0.5.1`. Run `kalinga` on its own
   to see the list of commands.

## Your first program

Create a file called `hello.kl`:

```kalinga
fn main() {
    println("Hello, world!")
}
```

Run it:

```bash
kalinga run hello.kl
```

```text
Hello, world!
```

Every program starts at `fn main()`. It takes no parameters and returns nothing.

## A slightly bigger program

```kalinga
fn square(n: int) -> int {
    return n * n
}

fn main() {
    let name = "Kalinga"
    var total = 0

    for i in 1..4 {
        total += square(i)
    }

    println("Hi from", name)
    println("1² + 2² + 3² =", total)
}
```

```text
Hi from Kalinga
1² + 2² + 3² = 14
```

What is happening here:

- `fn square(n: int) -> int` declares a function that takes an `int` and returns an `int`.
- `let` makes a variable that cannot change, `var` makes one that can.
- `for i in 1..4` counts `1, 2, 3` (the end is not included).
- `print` accepts any number of values and puts a space between them.

## Building an executable

`kalinga run` compiles into a temporary folder and runs the result. To keep the
executable, use `build`:

```bash
kalinga build hello.kl
```

This writes `hello.exe` on Windows (`hello` on Linux and macOS) next to the source file.
Choose another name with `-o`:

```bash
kalinga build hello.kl -o greet.exe
```

The executable has no dependency on Kalinga or Rust — copy it anywhere and run it.

## Checking without running

```bash
kalinga check hello.kl
```

Reports errors only. It is fast because it skips the Rust step, which makes it useful in
editors.

## Projects

For a program with several files, packages or Rust crates, let `utkal` set up a project:

```bash
utkal new my-app
cd my-app
utkal run
```

See [18. Packages with utkal](18-packages.md).

## Editor support

Install the VS Code extension for highlighting, snippets, live errors and a ▶ Run button.
See [Tooling](24-tooling.md).

---

Next: [2. Basics](02-basics.md)
