# 16. Error Handling

Some operations can fail for reasons that are not bugs: a file is missing, a level number
is out of range, a network server does not answer. Kalinga handles these with a value of
type `Result<T, E>`: either `Ok(value)` with a `T`, or `Err(error)` with an `E`. There are
no exceptions, so a function's signature always says whether it can fail.

## Returning a Result

```kalinga
fn parse_level(text: str) -> Result<int, str> {
    if text == "" {
        return Err("empty")
    }
    let n = int(text)
    if n < 1 {
        return Err("level must be at least 1, got {n}")
    }
    return Ok(n)
}

fn main() {
    println(parse_level("3"))
    println(parse_level(""))
    println(parse_level("0"))
}
```

Output:

```text
Ok(3)
Err(empty)
Err(level must be at least 1, got 0)
```

- `Ok(...)` and `Err(...)` take their types from where they are used: here, from the
  function's return type.
- The error type `E` can be anything: a `str` message is the simplest; an enum lets the
  caller tell kinds of failure apart (see below).
- A function that can fail but has no value to return uses `Result<void, E>` and
  `return Ok()`.

## Handling both cases with match

`match` on a `Result` must handle both `Ok` and `Err`, and binds the value inside:

```kalinga
fn divide(a: int, b: int) -> Result<int, str> {
    if b == 0 {
        return Err("division by zero")
    }
    return Ok(a / b)
}

fn main() {
    for b in [2, 0] {
        match divide(10, b) {
            Ok(v) => println("10 /", b, "=", v)
            Err(e) => println("cannot divide:", e)
        }
    }
}
```

Output:

```text
10 / 2 = 5
cannot divide: division by zero
```

Forgetting a case is a compile error:

```kalinga error
fn f() -> Result<int, str> {
    return Ok(1)
}

fn main() {
    match f() {
        Ok(v) => println(v)
    }
}
```

```text
main.kl:6:5: error: `match` does not handle every case (missing: Err)
```

## Passing errors on with `?`

Often a function cannot do anything useful about an error except report it to its own
caller. Writing `?` after a `Result` does exactly that: it unwraps an `Ok`, or returns the
`Err` from the current function straight away.

```kalinga
enum LoadError {
    Missing(name: str)
    Bad(line: int)
}

fn load(name: str) -> Result<[int], LoadError> {
    if name == "missing" {
        return Err(LoadError.Missing(name))
    }
    return Ok([1, 2, 3])
}

fn total(name: str) -> Result<int, LoadError> {
    let xs = load(name)?
    var sum = 0
    for x in xs {
        sum += x
    }
    return Ok(sum)
}

fn main() {
    for name in ["game", "missing"] {
        match total(name) {
            Ok(t) => println("total", t)
            Err(e) => match e {
                Missing(n) => println("missing file:", n)
                Bad(l) => println("bad line", l)
            }
        }
    }
}
```

Output:

```text
total 6
missing file: missing
```

- `?` only works inside a function that returns a `Result`, and the error types must be
  the same:

```kalinga error
fn f() -> Result<int, str> {
    return Ok(1)
}

fn main() {
    let x = f()?
}
```

```text
main.kl:6:16: error: `?` can only be used in a function that returns a `Result`; this one returns `void`
```

- `?` also works on optionals, in a function that returns an optional: `none` is returned
  early.

```kalinga
fn first_even_times_ten(xs: [int]) -> int? {
    let f = xs.find(fn(x) => x % 2 == 0)?
    return f * 10
}

fn main() {
    println(first_even_times_ten([1, 3, 4]), first_even_times_ten([1, 3]))
}
```

Output:

```text
40 none
```

## Result methods

| Method | Result |
|---|---|
| `r.is_ok()` / `r.is_err()` | `bool` |
| `r.ok()` | the value as `T?` (`none` for an error) |
| `r.err()` | the error as `E?` (`none` for a value) |
| `r.unwrap_or(fallback)` | the value, or `fallback` for an error |
| `r.unwrap()` | the value; **stops the program** with the error for an `Err` |

```kalinga
fn parse_level(text: str) -> Result<int, str> {
    if text == "" {
        return Err("empty")
    }
    return Ok(int(text))
}

fn main() {
    let good = parse_level("7")
    let bad = parse_level("")
    println(good.is_ok(), bad.is_err())
    println(good.unwrap_or(1), bad.unwrap_or(1))
    println(good.ok(), bad.err())
}
```

Output:

```text
true true
7 1
7 empty
```

Use `unwrap()` only where an error really is a bug. When it fails, the program stops with a
runtime error (exit code 101), like an array index out of range:

```text
runtime error: called unwrap() on an error: disk full
```

## main can return a Result

`fn main() -> Result<void, E>` lets `?` be used all the way up. If `main` returns an
`Err`, the program prints `error: ` and the error, and exits with code 1:

```kalinga
fn load_config() -> Result<int, str> {
    return Err("config.txt not found")
}

fn main() -> Result<void, str> {
    let volume = load_config()?
    println("volume", volume)
    return Ok()
}
```

Output (on stderr, exit code 1):

```text
error: config.txt not found
```

## Giving Ok and Err a type

When nothing says which `Result` an `Ok` or `Err` belongs to, give the variable a type:

```kalinga error
fn main() {
    let r = Ok(5)
}
```

```text
main.kl:2:13: error: cannot tell which `Result` this `Ok` belongs to; give the variable a type, e.g. `let r: Result<int, str> = Ok(...)`
```

```kalinga
fn main() {
    let r: Result<int, str> = Ok(5)
    println(r)
}
```

Output:

```text
Ok(5)
```

## Errors from Rust code

An `extern fn` may return a `Result`; it maps to Rust's `Result<T, E>`. The game module uses
this for images and sounds that fail to load:

```kalinga
match Image.load("ship.png") {
    Ok(img) => self.ship = img
    Err(e) => println("no ship image:", e)
}
```

See [19. Rust Interop](19-rust-interop.md).

## Results or runtime errors?

| Situation | Use |
|---|---|
| Can fail in normal use (input, files, network, parsing) | return a `Result` |
| Should never happen if the code is right (index out of range, `unwrap` of an error) | a runtime error stops the program |

---

Previous: [15. Standard Library: Math, Random, Time](15-standard-library.md) · Next: [17. Modules](17-modules.md)
